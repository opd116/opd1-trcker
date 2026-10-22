//! `collection` (R189): the user's owned weapon skins.

use opd1_core::wire::Collection;

use super::AppStateRef;

/// `None` while the owned list was never fetched (the game has not run).
#[tauri::command]
pub async fn collection(_state: AppStateRef<'_>) -> Result<Option<Collection>, String> {
    // TODO(backend-system)
    Ok(None)
}
