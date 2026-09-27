//! Thin Tauri shell for OPD1 TRCKER.
//!
//! No commands or plugins are registered yet — this task only proves the
//! window boots with the right size/behavior. Later tasks wire the poll
//! loop, Tauri events, and IPC commands backed by `opd1-core` / `opd1-riot`.

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
