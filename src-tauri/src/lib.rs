//! Tauri shell for OPD1 TRCKER: setup and wiring only.
//!
//! - `logbuf` — the 500-line in-memory log (`copy_log`)
//! - `state` — shared state, the live model, `"live"` emission
//! - `poll` — the live poll loop (stage machine)
//! - `sync` — per-player cached-first sync + the roster worker
//! - `roster` — pure roster/stage decisions (unit-tested)
//! - `store` — DB helpers over `opd1_core::db::repo::Repo`
//! - `details` — Player Details assembly
//! - `match_details` — Match Details scoreboard (DB read + fetch-once)
//! - `imgproto` — the `img` URI scheme
//! - `commands` — IPC commands

mod commands;
mod details;
mod imgproto;
mod logbuf;
mod match_details;
#[cfg(test)]
mod pipeline_tests;
mod poll;
mod rank_retry;
mod roster;
mod state;
mod store;
mod sync;

use std::path::PathBuf;
use std::sync::Arc;

use tauri::Manager;

use opd1_core::db::open_db;
use opd1_core::db::repo::Repo;
use opd1_riot::assets::AssetCache;

use crate::state::{lock, AppState};
use crate::store::{load_player_db, SELF_PUUID_KEY};

/// Opens the DB at `app_data_dir/data/opd1.sqlite` (the installer keeps
/// `data\`), the asset cache at `app_cache_dir/assets/` (the installer
/// wipes it), and builds the shared state.
fn init_state(app: &tauri::App) -> Result<Arc<AppState>, Box<dyn std::error::Error>> {
    let data_dir = app.path().app_data_dir()?;
    let cache_dir = app.path().app_cache_dir()?;
    let db_dir: PathBuf = data_dir.join("data");
    std::fs::create_dir_all(&db_dir)?;
    let conn = open_db(&db_dir.join("opd1.sqlite"))?;
    let repo = Repo::new(conn);
    let self_puuid = repo.get_state(SELF_PUUID_KEY)?;

    let assets = AssetCache::new(cache_dir.join("assets"));
    let st = Arc::new(AppState::new(
        Box::new(app.handle().clone()),
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
            let path = request.uri().path().to_string();
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
                }
            });
            poll::start(st);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
