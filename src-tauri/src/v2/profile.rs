//! `profile_extras` (R189): the profile drawer's maps and "in your match"
//! chips, DB only.

use opd1_core::wire::ProfileExtras;

use super::AppStateRef;

#[tauri::command]
pub async fn profile_extras(
    puuid: String,
    _state: AppStateRef<'_>,
) -> Result<ProfileExtras, String> {
    // TODO(backend-live)
    Ok(ProfileExtras {
        puuid,
        maps: Vec::new(),
        live_mates: Vec::new(),
    })
}
