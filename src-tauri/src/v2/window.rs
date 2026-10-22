//! Window behaviour (R189): Glance mode and the settings' window side
//! effects (keep on top, close to tray, launch with Windows).

use opd1_core::wire::Settings;

/// Glance mode: a 420px-wide column (on) or the normal window (off).
#[tauri::command]
pub fn set_glance(on: bool, app: tauri::AppHandle) -> Result<(), String> {
    // TODO(backend-system)
    let _ = (on, app);
    Ok(())
}

/// Applies `onTop`, `boot` (autostart) and `tray` (close to tray).
pub fn apply_settings(app: &tauri::AppHandle, settings: &Settings) {
    // TODO(backend-system)
    let _ = (app, settings);
}
