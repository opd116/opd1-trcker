//! The ASCII studio's saved gallery (R189): migration 4's `ascii_art`.
//! A title is trimmed and cut to 40 characters; art that is blank or over
//! 8 KB is refused with an error rather than cut (half a piece is broken
//! art) — `opd1_core::db::marks::check_ascii`.

use opd1_core::db::marks::check_ascii;
use opd1_core::wire::AsciiArt;

use super::AppStateRef;
use crate::state::now_ms;

/// Newest first.
#[tauri::command]
pub async fn ascii_list(state: AppStateRef<'_>) -> Result<Vec<AsciiArt>, String> {
    state
        .db(|repo| repo.ascii_list())
        .await
        .ok_or_else(|| "gallery unavailable".to_string())
}

/// Stores a piece and returns it with its id.
#[tauri::command]
pub async fn ascii_save(
    title: String,
    raw: String,
    state: AppStateRef<'_>,
) -> Result<AsciiArt, String> {
    let (title, raw) = check_ascii(&title, &raw).map_err(|e| e.to_string())?;
    let now = now_ms();
    state
        .db(move |repo| repo.ascii_save(&title, &raw, now))
        .await
        .ok_or_else(|| "art not stored".to_string())
}

/// Deletes a piece; an id that is already gone is not an error.
#[tauri::command]
pub async fn ascii_delete(id: i64, state: AppStateRef<'_>) -> Result<(), String> {
    state
        .db(move |repo| repo.ascii_delete(id))
        .await
        .map(|_| ())
        .ok_or_else(|| "art not deleted".to_string())
}
