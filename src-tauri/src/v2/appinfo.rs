//! `app_info` (R189): the About screen.

use opd1_core::wire::AppInfo;

use super::AppStateRef;

#[tauri::command]
pub async fn app_info(_state: AppStateRef<'_>) -> Result<AppInfo, String> {
    // TODO(backend-store): commit/build date, client versions, DB counts.
    Ok(AppInfo {
        version: env!("CARGO_PKG_VERSION").into(),
        commit: None,
        build_ms: None,
        riot_client_version: None,
        valorant_version: None,
        db_matches: 0,
        db_bytes: 0,
    })
}
