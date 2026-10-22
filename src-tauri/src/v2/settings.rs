//! `get_settings` / `set_settings` (R189): stored as JSON in `app_state`
//! under `ui_settings`, so they survive updates. A stored copy that does not
//! parse falls back to the defaults (the design's).

use opd1_core::db::repo::Repo;
use opd1_core::wire::Settings;

use super::AppStateRef;

/// The key in `app_state`.
pub const SETTINGS_KEY: &str = "ui_settings";

/// The stored settings, or the defaults.
pub fn load(repo: &Repo) -> rusqlite::Result<Settings> {
    Ok(repo
        .get_state(SETTINGS_KEY)?
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default())
}

#[tauri::command]
pub async fn get_settings(state: AppStateRef<'_>) -> Result<Settings, String> {
    state
        .db(load)
        .await
        .ok_or_else(|| "settings unavailable".to_string())
}

/// Stores the settings, applies their window side effects
/// (`window::apply_settings`) and emits `"settings"`. Returns what was stored.
#[tauri::command]
pub async fn set_settings(
    settings: Settings,
    app: tauri::AppHandle,
    state: AppStateRef<'_>,
) -> Result<Settings, String> {
    let json = serde_json::to_string(&settings).map_err(|e| e.to_string())?;
    state
        .db(move |repo| repo.set_state(SETTINGS_KEY, &json))
        .await
        .ok_or_else(|| "settings not stored".to_string())?;
    super::window::apply_settings(&app, &settings);
    state.emit_settings(&settings);
    Ok(settings)
}
