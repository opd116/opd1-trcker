//! `match_history` (R189): the user's own stored matches, DB only.
//!
//! The query and the row rules are `opd1_core::db::history`
//! (`Repo::match_history`): newest first, `competitive` or every queue, at
//! most 200 rows a page; a match whose details are not stored yet (only in
//! `history_index`) is in neither the rows nor the totals. Map ids are
//! converted from the stored Riot path with the same lookup Player Details
//! uses (`AppState::map_uuid`).

use opd1_core::wire::{HistoryPage, HistoryQueue};

use super::AppStateRef;
use crate::state::{lock, AppState};
use crate::store::SELF_PUUID_KEY;

/// One page for the user; an empty page (all counts 0) while the app has
/// never known whose history to show.
pub async fn match_history_for(
    st: &AppState,
    queue: HistoryQueue,
    offset: u32,
    limit: u32,
) -> Result<HistoryPage, String> {
    let live_self = lock(&st.live).self_puuid.clone();
    let assets = st.assets.clone();
    st.db(move |repo| {
        let me = match live_self {
            Some(me) => Some(me),
            None => repo.get_state(SELF_PUUID_KEY)?,
        };
        let Some(me) = me else {
            return Ok(HistoryPage {
                rows: Vec::new(),
                total: 0,
                competitive_total: 0,
                all_total: 0,
            });
        };
        let lookup = |path: &str| {
            crate::roster::map_uuid(path, |x| assets.map_uuid_for_path(x).ok().flatten())
        };
        repo.match_history(&me, queue, offset, limit, &lookup)
    })
    .await
    .ok_or_else(|| "match history unavailable".to_string())
}

/// One page, newest first.
#[tauri::command]
pub async fn match_history(
    queue: HistoryQueue,
    offset: u32,
    limit: u32,
    state: AppStateRef<'_>,
) -> Result<HistoryPage, String> {
    match_history_for(&state, queue, offset, limit).await
}
