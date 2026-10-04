//! Tauri shell for OPD1 TRCKER: setup and wiring only.
//!
//! - `logbuf` — the 500-line in-memory log (`copy_log`; the UI's own lines
//!   arrive through `ui_log`)
//! - `state` — shared state, the live model, `"live"` emission
//! - `poll` — the live poll loop (stage machine)
//! - `sync` — per-player cached-first sync + the roster worker
//! - `roster` — pure roster/stage decisions (unit-tested)
//! - `store` — DB helpers over `opd1_core::db::repo::Repo`
//! - `details` — Player Details assembly
//! - `match_details` — Match Details scoreboard (DB read + fetch-once)
//! - `my_stats` — My Stats' DB-only computation and its `"mystats"` push
//! - `imgproto` — the `img` URI scheme
//! - `commands` — IPC commands

mod commands;
mod details;
mod imgproto;
mod logbuf;
mod match_details;
mod my_stats;
#[cfg(test)]
mod pipeline_tests;
mod poll;
mod rank_retry;
mod roster;
mod state;
mod store;
mod sync;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tauri::Manager;

use opd1_core::db::open_db;
use opd1_core::db::repo::Repo;
use opd1_riot::assets::AssetCache;

use crate::state::{lock, AppState, EventSink};
use crate::store::{load_player_db, SELF_PUUID_KEY};

/// Opens the DB at `app_data_dir/data/opd1.sqlite` (the installer keeps
/// `data\`), the asset cache at `app_cache_dir/assets/` (the installer
/// wipes it), and builds the shared state.
fn init_state(app: &tauri::App) -> Result<Arc<AppState>, Box<dyn std::error::Error>> {
    let data_dir = app.path().app_data_dir()?;
    let cache_dir = app.path().app_cache_dir()?;
    build_state(Box::new(app.handle().clone()), &data_dir, &cache_dir)
}

/// [`init_state`] without the Tauri app: where the events go and the two
/// directories are arguments (the startup tests build a state from a DB left
/// by an earlier run). The user's puuid is read back from the DB here, so a
/// start with VALORANT closed already knows whose card and stats to show.
fn build_state(
    sink: Box<dyn EventSink>,
    data_dir: &Path,
    cache_dir: &Path,
) -> Result<Arc<AppState>, Box<dyn std::error::Error>> {
    let db_dir: PathBuf = data_dir.join("data");
    std::fs::create_dir_all(&db_dir)?;
    let conn = open_db(&db_dir.join("opd1.sqlite"))?;
    let repo = Repo::new(conn);
    let self_puuid = repo.get_state(SELF_PUUID_KEY)?;

    let assets = AssetCache::new(cache_dir.join("assets"));
    let st = Arc::new(AppState::new(
        sink,
        repo,
        assets,
        data_dir.join("debug"),
        self_puuid.clone(),
    ));

    // The Closed-stage snapshot (the user's own card from the DB) is ready
    // before the UI can ask `live_state()`.
    if let Some(me) = self_puuid {
        let db = st.db_blocking(|repo| load_player_db(repo, Some(&me), &me));
        if let Some(slot) = db.and_then(|db| state::closed_self_slot(&me, db)) {
            lock(&st.live).slots = vec![slot];
        }
    }
    st.publish();
    Ok(st)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    logbuf::init();
    log::info!("OPD1 TRCKER {} starting", env!("CARGO_PKG_VERSION"));

    tauri::Builder::default()
        .register_asynchronous_uri_scheme_protocol("img", |ctx, request, responder| {
            let Some(st) = ctx.app_handle().try_state::<Arc<AppState>>() else {
                responder.respond(
                    tauri::http::Response::builder()
                        .status(tauri::http::StatusCode::SERVICE_UNAVAILABLE)
                        .body(Vec::new())
                        .unwrap_or_default(),
                );
                return;
            };
            let st = st.inner().clone();
            let method = request.method().as_str().to_string();
            let path = imgproto::request_path(request.uri());
            tauri::async_runtime::spawn(async move {
                responder.respond(imgproto::serve(st, method, path).await);
            });
        })
        .invoke_handler(tauri::generate_handler![
            commands::meta,
            commands::live_state,
            commands::player_details,
            commands::close_details,
            commands::match_details,
            commands::close_match,
            commands::my_stats,
            commands::copy_log,
            commands::ui_log,
            commands::dump_raw,
        ])
        .setup(|app| {
            let started = std::time::Instant::now();
            let st = init_state(app).inspect_err(|e| log::error!("setup failed: {e}"))?;
            log::info!("setup: state ready in {} ms", started.elapsed().as_millis());
            app.manage(st.clone());

            // Asset tables (agent/map/tier names, acts) once at start —
            // from disk when valorant-api's version is unchanged.
            let tables_st = st.clone();
            tauri::async_runtime::spawn(async move {
                if tables_st.ensure_tables(false).await.is_ok() {
                    tables_st.publish();
                    // B2: the acts are known now: My Stats, if the DB has the
                    // user's puuid, becomes computable (once, at startup).
                    my_stats::notify(&tables_st);
                }
            });
            // B2: the slow tick (every 15 minutes) that keeps what depends on
            // the clock alone current; the push's byte dedup makes it free.
            my_stats::start_periodic(&st);
            poll::start(st);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod startup_tests {
    use super::*;
    use crate::pipeline_tests::{ACT, ME};
    use crate::state::{now_ms, EventSink};
    use crate::store::{save_player, PlayerUpdate};
    use opd1_core::model::{LiveState, MatchDetailsView, PlayerDetails};
    use opd1_core::stats::Act;

    struct Quiet;

    impl EventSink for Quiet {
        fn live(&self, _: &LiveState) {}
        fn details(&self, _: &PlayerDetails) {}
        fn match_view(&self, _: &MatchDetailsView) {}
    }

    /// A data dir as an earlier run left it: the DB with the user's puuid and
    /// their `players` row.
    fn earlier_run(dir: &Path, puuid: Option<&str>) {
        let db_dir = dir.join("data");
        std::fs::create_dir_all(&db_dir).unwrap();
        let repo = Repo::new(open_db(&db_dir.join("opd1.sqlite")).unwrap());
        if let Some(puuid) = puuid {
            repo.set_state(SELF_PUUID_KEY, puuid).unwrap();
            save_player(
                repo.conn(),
                &PlayerUpdate {
                    puuid: puuid.into(),
                    name: Some(("Me".into(), "OPD1".into())),
                    account_level: 87,
                    card_id: "card".into(),
                },
                now_ms(),
            )
            .unwrap();
        }
    }

    fn the_act() -> Vec<Act> {
        vec![Act {
            id: ACT.into(),
            name: "ACT V".into(),
            episode_name: "V26".into(),
            start_ms: now_ms() - 30 * 86_400_000,
            end_ms: now_ms() + 30 * 86_400_000,
        }]
    }

    #[tokio::test]
    async fn a_start_with_valorant_closed_knows_the_user_from_the_db() {
        // The Closed stage shows the user's own card and My Stats is
        // computable, with no Riot client around: the puuid comes back from
        // the DB at startup.
        let dir = tempfile::tempdir().unwrap();
        earlier_run(dir.path(), Some(ME));

        let st = build_state(Box::new(Quiet), dir.path(), &dir.path().join("cache")).unwrap();

        assert_eq!(lock(&st.live).self_puuid.as_deref(), Some(ME));
        let live = st.live_state();
        assert_eq!(live.players.len(), 1, "the user's own card, from the DB");
        assert_eq!(live.players[0].puuid, ME);
        // The act comes from the asset tables (loaded after startup).
        assert!(
            my_stats::compute(&st).await.unwrap().is_none(),
            "no act yet"
        );
        st.set_acts_for_test(the_act());
        assert!(my_stats::compute(&st).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn a_first_start_has_no_user_and_no_my_stats() {
        let dir = tempfile::tempdir().unwrap();
        earlier_run(dir.path(), None);

        let st = build_state(Box::new(Quiet), dir.path(), &dir.path().join("cache")).unwrap();
        st.set_acts_for_test(the_act());

        assert_eq!(lock(&st.live).self_puuid, None);
        assert!(st.live_state().players.is_empty());
        assert!(my_stats::compute(&st).await.unwrap().is_none());
    }
}
