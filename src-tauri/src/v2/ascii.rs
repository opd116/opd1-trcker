//! The ASCII studio's saved gallery (R189).

use opd1_core::wire::AsciiArt;

use super::AppStateRef;

/// Newest first.
#[tauri::command]
pub async fn ascii_list(_state: AppStateRef<'_>) -> Result<Vec<AsciiArt>, String> {
    // TODO(backend-store)
    Ok(Vec::new())
}

#[tauri::command]
pub async fn ascii_save(
    title: String,
    raw: String,
    _state: AppStateRef<'_>,
) -> Result<AsciiArt, String> {
    // TODO(backend-store)
    Ok(AsciiArt {
        id: 0,
        title,
        raw,
        created_ms: 0,
    })
}

#[tauri::command]
pub async fn ascii_delete(id: i64, _state: AppStateRef<'_>) -> Result<(), String> {
    // TODO(backend-store)
    let _ = id;
    Ok(())
}
