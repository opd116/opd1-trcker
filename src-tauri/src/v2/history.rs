//! `match_history` (R189): the user's own stored matches, DB only.

use opd1_core::wire::{HistoryPage, HistoryQueue};

use super::AppStateRef;

/// One page, newest first.
#[tauri::command]
pub async fn match_history(
    queue: HistoryQueue,
    offset: u32,
    limit: u32,
    _state: AppStateRef<'_>,
) -> Result<HistoryPage, String> {
    // TODO(backend-store)
    let _ = (queue, offset, limit);
    Ok(HistoryPage {
        rows: Vec::new(),
        total: 0,
        competitive_total: 0,
        all_total: 0,
    })
}
