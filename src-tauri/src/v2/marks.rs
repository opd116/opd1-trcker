//! Saved players, private notes and dismissed smurf flags (R189).

use opd1_core::wire::{PlayerMarks, SavedPlayer};

use super::AppStateRef;

/// Every mark; later changes arrive as `"marks"`.
#[tauri::command]
pub async fn player_marks(_state: AppStateRef<'_>) -> Result<PlayerMarks, String> {
    // TODO(backend-store)
    Ok(PlayerMarks::default())
}

/// The Saved screen's rows, newest saved first.
#[tauri::command]
pub async fn saved_players(_state: AppStateRef<'_>) -> Result<Vec<SavedPlayer>, String> {
    // TODO(backend-store)
    Ok(Vec::new())
}

/// Saves or unsaves a player; emits `"marks"`.
#[tauri::command]
pub async fn set_saved(puuid: String, saved: bool, _state: AppStateRef<'_>) -> Result<(), String> {
    // TODO(backend-store)
    let _ = (puuid, saved);
    Ok(())
}

/// Sets (or, with `""`, clears) the private note on a player; emits `"marks"`.
#[tauri::command]
pub async fn set_note(puuid: String, note: String, _state: AppStateRef<'_>) -> Result<(), String> {
    // TODO(backend-store)
    let _ = (puuid, note);
    Ok(())
}

/// "Not a smurf, hide flag"; emits `"marks"`.
#[tauri::command]
pub async fn dismiss_smurf(puuid: String, _state: AppStateRef<'_>) -> Result<(), String> {
    // TODO(backend-store)
    let _ = puuid;
    Ok(())
}
