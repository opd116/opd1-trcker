//! IPC commands (all registered in `lib.rs`). The UI never polls Riot:
//! live data arrives through the `"live"` event; these are the pull-side
//! entry points.
//!
//! - `meta()` — agent/map/tier names + tier colors (R21).
//! - `live_state()` — the last emitted `LiveState` (the UI calls it once on
//!   start so it never misses the first snapshot).
//! - `player_details(puuid)` — cached details now; a background sync then
//!   emits `"details"` if it found more.
//! - `copy_log()` — the last 500 log lines (R30; the UI copies them).
//! - `dump_raw()` — redacted raw responses to `app_data/debug/*.json`.

use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use opd1_core::model::{LiveState, PlayerDetails};
use opd1_riot::assets::Meta;

use crate::logbuf::short;
use crate::state::AppState;

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

    #[test]
    fn uuid_check() {
        assert!(is_uuid("aaaaaaaa-0000-0000-0000-000000000001"));
        assert!(!is_uuid("../../etc"));
        assert!(!is_uuid(""));
    }

    #[test]
    fn file_stem_is_filesystem_safe() {
        assert_eq!(file_stem("comp_updates"), "comp_updates");
        assert_eq!(file_stem("a/../b"), "a____b");
        assert_eq!(file_stem(""), "unnamed");
    }
}
