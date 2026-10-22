//! `app_status`, `retry_now`, `launch_valorant` (R189).

use opd1_core::wire::AppStatus;

use super::AppStateRef;

/// The current connection status; later changes arrive as `"status"`.
#[tauri::command]
pub fn app_status(state: AppStateRef<'_>) -> AppStatus {
    state.status()
}

/// "Retry now": the poll loop tries the Riot Client again at once.
#[tauri::command]
pub fn retry_now(_state: AppStateRef<'_>) {
    // TODO(backend-live): wake the poll loop.
}

/// "Launch VALORANT": starts the game through the Riot Client.
#[tauri::command]
pub fn launch_valorant() -> Result<(), String> {
    // TODO(backend-live): RiotClientServices.exe --launch-product=valorant --launch-patchline=live
    Err("not implemented".into())
}
