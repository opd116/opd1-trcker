//! IPC commands (all registered in `lib.rs`). The UI never polls Riot:
//! live data arrives through the `"live"` event; these are the pull-side
//! entry points.
//!
//! - `meta()` — agent/map/tier names + tier colors (R21).
//! - `live_state()` — the last emitted `LiveState` (the UI calls it once on
//!   start so it never misses the first snapshot).
//! - `player_details(puuid)` — cached details now; a background sync then
//!   emits `"details"` if it found more.
//! - `match_details(matchId, focusPuuid)` — one match as the Match Details
//!   scoreboard, from the DB (fetched once when it isn't stored yet).
//! - `my_stats()` — the user's My Stats for the current act, from the DB
//!   only (no Riot call); `null` while the user or the act is unknown.
//! - `copy_log()` — the last 500 log lines (R30; the UI copies them).
//! - `dump_raw()` — redacted raw responses to `app_data/debug/*.json`.

use std::collections::HashMap;
use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use opd1_core::model::{LiveState, MatchDetailsView, MyStats, PlayerDetails};
use opd1_riot::assets::Meta;

use crate::logbuf::short;
use crate::state::{lock, now_ms, AppState};

type AppStateRef<'a> = State<'a, Arc<AppState>>;

fn is_uuid(s: &str) -> bool {
    s.len() == 36
        && s.char_indices().all(|(i, c)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                c == '-'
            } else {
                c.is_ascii_hexdigit()
            }
        })
}

/// Both ids of a `match_details` call must be uuids — they end up in a DB
/// key and a Riot URL path, the same check `player_details` applies to its
/// puuid. The match id is checked first.
fn check_match_args(match_id: &str, focus_puuid: &str) -> Result<(), String> {
    if !is_uuid(match_id) {
        return Err("invalid match id".into());
    }
    if !is_uuid(focus_puuid) {
        return Err("invalid player id".into());
    }
    Ok(())
}

#[tauri::command]
pub async fn meta(state: AppStateRef<'_>) -> Result<Meta, String> {
    state.ensure_tables(false).await?;
    state.assets.meta().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn live_state(state: AppStateRef<'_>) -> Result<LiveState, String> {
    Ok(state.live_state())
}

#[tauri::command]
pub async fn player_details(
    puuid: String,
    state: AppStateRef<'_>,
) -> Result<PlayerDetails, String> {
    if !is_uuid(&puuid) {
        return Err("invalid player id".into());
    }
    let st = state.inner().clone();
    let details = crate::details::build(&st, &puuid).await?;

    if let Some(session) = st.session() {
        if st.claim_details_sync(&puuid) {
            let before = details.clone();
            tauri::async_runtime::spawn(async move {
                if crate::sync::sync_details(&st, &session, &puuid)
                    .await
                    .is_err()
                {
                    return;
                }
                // The card on screen may have gained matches too.
                st.reload_slot(&puuid).await;
                match crate::details::build(&st, &puuid).await {
                    Ok(fresh) if fresh != before => {
                        log::debug!("details for {} updated", short(&puuid));
                        st.emit_details(&fresh);
                    }
                    Ok(_) => {}
                    Err(e) => log::warn!("details rebuild failed: {e}"),
                }
            });
        }
    }
    Ok(details)
}

#[tauri::command]
pub async fn match_details(
    match_id: String,
    focus_puuid: String,
    state: AppStateRef<'_>,
) -> Result<MatchDetailsView, String> {
    check_match_args(&match_id, &focus_puuid)?;
    crate::match_details::load(state.inner(), &match_id, &focus_puuid).await
}

#[tauri::command]
pub async fn my_stats(state: AppStateRef<'_>) -> Result<Option<MyStats>, String> {
    build_my_stats(state.inner()).await
}

/// `my_stats` without the Tauri wrapper: computed on demand from the DB,
/// never a Riot call. It first awaits `ensure_tables`, the gate `meta()`
/// uses too — a no-op once the asset tables are loaded; at startup it
/// waits for the first load instead of racing it (the acts and map names
/// come from those tables). `None` until the user's puuid is known (first
/// run) or while no act is current.
pub async fn build_my_stats(st: &Arc<AppState>) -> Result<Option<MyStats>, String> {
    // Tables unavailable just means no act yet (-> `None` below).
    let _ = st.ensure_tables(false).await;
    let Some(me) = lock(&st.live).self_puuid.clone() else {
        return Ok(None);
    };
    let Some(act) = st.current_act() else {
        return Ok(None);
    };
    let map_names: HashMap<String, String> = st
        .assets
        .meta()
        .map(|meta| meta.maps.into_iter().map(|(id, m)| (id, m.name)).collect())
        .unwrap_or_default();
    let assets = st.assets.clone();
    let now = now_ms();
    st.db(move |repo| {
        let lookup = |path: &str| {
            crate::roster::map_uuid(path, |p| assets.map_uuid_for_path(p).ok().flatten())
        };
        crate::store::load_my_stats(repo, &me, &act, now, &lookup, &map_names, &local_offset_ms)
    })
    .await
    .map(Some)
    .ok_or_else(|| "database unavailable".to_string())
}

/// The machine's UTC offset (ms) at `utc_ms` — per instant, so a match
/// on the other side of a DST change lands in the right hour.
fn local_offset_ms(utc_ms: i64) -> i64 {
    use chrono::{Local, Offset, TimeZone};
    Local
        .timestamp_millis_opt(utc_ms)
        .single()
        .map_or(0, |t| i64::from(t.offset().fix().local_minus_utc()) * 1000)
}

#[tauri::command]
pub fn copy_log() -> String {
    crate::logbuf::snapshot()
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DumpResult {
    pub dir: String,
    pub files: u32,
}

/// Keeps a capture key safe as a file name (keys are already
/// `[a-z_]` today; this guards the generic-normalizer fallback).
fn file_stem(key: &str) -> String {
    let s: String = key
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if s.is_empty() {
        "unnamed".into()
    } else {
        s
    }
}

#[tauri::command]
pub async fn dump_raw(state: AppStateRef<'_>) -> Result<DumpResult, String> {
    let Some(session) = state.session() else {
        return Err("VALORANT is not connected".into());
    };
    // Already redacted by opd1-riot (tokens/JWTs/passwords).
    let captures = session.raw_captures();
    let dir = state.debug_dir.clone();
    let shown = dir.display().to_string();
    let files = tauri::async_runtime::spawn_blocking(move || -> std::io::Result<u32> {
        std::fs::create_dir_all(&dir)?;
        let mut n = 0;
        for (key, body) in captures {
            std::fs::write(dir.join(format!("{}.json", file_stem(&key))), body)?;
            n += 1;
        }
        Ok(n)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;
    log::info!("dump_raw: wrote {files} files");
    Ok(DumpResult { dir: shown, files })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    use opd1_core::db::open_db;
    use opd1_core::db::repo::{CompUpdateRow, Match, MatchPlayer, Repo};
    use opd1_core::stats::Act;
    use opd1_riot::assets::AssetCache;
    use serde_json::Value;

    use crate::state::EventSink;

    const ME: &str = "aaaaaaaa-0000-0000-0000-000000000001";
    const DAY: i64 = 86_400_000;

    struct NoEvents;

    impl EventSink for NoEvents {
        fn live(&self, _: &LiveState) {}
        fn details(&self, _: &PlayerDetails) {}
    }

    /// An app with an empty in-memory DB and asset tables that can't load
    /// (nothing listens on the base URL).
    fn app(self_puuid: Option<&str>, dir: &Path) -> Arc<AppState> {
        let repo = Repo::new(open_db(Path::new(":memory:")).unwrap());
        let assets = AssetCache::with_base(dir.join("assets"), "http://127.0.0.1:1".into());
        Arc::new(AppState::new(
            Box::new(NoEvents),
            repo,
            assets,
            dir.join("debug"),
            self_puuid.map(str::to_string),
        ))
    }

    fn act(id: &str, start_ms: i64, end_ms: i64) -> Act {
        Act {
            id: id.into(),
            name: "ACT".into(),
            episode_name: "V26".into(),
            start_ms,
            end_ms,
        }
    }

    fn comp_match(repo: &Repo, id: &str, season: &str, start_ms: i64) -> rusqlite::Result<()> {
        let m = Match {
            match_id: id.into(),
            map_id: "/Game/Maps/Ascent/Ascent".into(),
            queue_id: "competitive".into(),
            start_ms,
            winning_team: Some("Blue".into()),
            is_ranked: true,
            rounds_played: Some(20),
            game_length_ms: None,
        };
        let me = MatchPlayer {
            puuid: ME.into(),
            team_id: "Blue".into(),
            party_id: None,
            agent_id: Some("agent".into()),
            kills: 10,
            deaths: 5,
            assists: 2,
            score: 3000,
            rounds: Some(20),
            hs: 1,
            bs: 3,
            ls: 0,
            game_name: None,
            tag_line: None,
            competitive_tier: None,
            damage: None,
        };
        repo.upsert_match(&m, &[], &[me])?;
        repo.upsert_comp_updates(
            ME,
            &[CompUpdateRow {
                match_id: id.into(),
                season_id: season.into(),
                tier_after: 12,
                tier_before: Some(12),
                rr_after: 50,
                rr_earned: 18,
                map_id: None,
                start_ms,
            }],
        )
    }

    #[tokio::test]
    async fn my_stats_is_null_until_the_user_and_the_act_are_known() {
        let dir = tempfile::tempdir().unwrap();
        let now = now_ms();
        let unknown_user = app(None, dir.path());
        unknown_user.set_acts_for_test(vec![act("cur", now - DAY, now + DAY)]);
        assert_eq!(build_my_stats(&unknown_user).await, Ok(None));
        // Asset tables never loaded (offline first run): no current act.
        let no_act = app(Some(ME), dir.path());
        assert_eq!(build_my_stats(&no_act).await, Ok(None));
    }

    #[tokio::test]
    async fn my_stats_reads_the_current_act_from_the_db() {
        let dir = tempfile::tempdir().unwrap();
        let now = now_ms();
        let st = app(Some(ME), dir.path());
        st.set_acts_for_test(vec![
            act("prev", now - 90 * DAY, now - 30 * DAY),
            act("cur", now - 30 * DAY, now + 30 * DAY),
        ]);
        st.db_blocking(|repo| {
            comp_match(repo, "old", "prev", now - 40 * DAY)?;
            comp_match(repo, "new", "cur", now - DAY)
        })
        .unwrap();

        let stats = build_my_stats(&st).await.unwrap().expect("stats");
        let v: Value = serde_json::from_str(&serde_json::to_string(&stats).unwrap()).unwrap();
        let ids: Vec<&str> = v["last5"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["matchId"].as_str().unwrap())
            .collect();
        assert_eq!(ids, ["new"]);
        assert_eq!(v["rank"]["netRr"], serde_json::json!(18));
        assert_eq!(v["rrSeries"].as_array().unwrap().len(), 1);
        // Map tables not loaded: an unknown map is "", never the Riot path.
        assert_eq!(v["last5"][0]["mapId"], serde_json::json!(""));
        assert_eq!(v["maps"], serde_json::json!([]));
        assert!(v.get("byWeekday").is_some() && v.get("byDaypart").is_some());
    }

    #[test]
    fn local_offset_is_whole_minutes_within_a_day() {
        let offset = local_offset_ms(now_ms());
        assert_eq!(offset % 60_000, 0);
        assert!(offset.abs() < DAY);
    }

    #[test]
    fn uuid_check() {
        assert!(is_uuid("aaaaaaaa-0000-0000-0000-000000000001"));
        assert!(!is_uuid("../../etc"));
        assert!(!is_uuid(""));
    }

    #[test]
    fn match_details_args_must_be_uuids() {
        // Both ids reach a DB key and a URL path: the same shape check
        // `player_details` applies to its puuid.
        let id = "1a2b3c4d-0000-0000-0000-000000000010";
        assert_eq!(
            check_match_args("not-a-match", ME),
            Err("invalid match id".to_string())
        );
        assert_eq!(
            check_match_args("../../etc/passwd", ME),
            Err("invalid match id".to_string())
        );
        assert_eq!(
            check_match_args(id, "../x"),
            Err("invalid player id".to_string())
        );
        assert_eq!(
            check_match_args(id, ""),
            Err("invalid player id".to_string())
        );
        assert_eq!(check_match_args(id, ME), Ok(()));
        // The match id is checked first.
        assert_eq!(
            check_match_args("", "../x"),
            Err("invalid match id".to_string())
        );
    }

    #[test]
    fn file_stem_is_filesystem_safe() {
        assert_eq!(file_stem("comp_updates"), "comp_updates");
        assert_eq!(file_stem("a/../b"), "a____b");
        assert_eq!(file_stem(""), "unnamed");
    }
}
