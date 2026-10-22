//! R189 (user 2026-10-22): the v2 redesign's commands. One file per area,
//! each with its own owner; every command is registered in `lib.rs`.
//!
//! - `status` — `app_status`, `retry_now`, `launch_valorant`
//! - `marks` — `player_marks`, `saved_players`, `set_saved`, `set_note`,
//!   `dismiss_smurf`
//! - `history` — `match_history`
//! - `settings` — `get_settings`, `set_settings`
//! - `ascii` — `ascii_list`, `ascii_save`, `ascii_delete`
//! - `appinfo` — `app_info`
//! - `collection` — `collection`
//! - `profile` — `profile_extras`
//! - `window` — `set_glance`, plus the settings' window side effects

pub mod appinfo;
pub mod ascii;
pub mod collection;
pub mod history;
pub mod marks;
pub mod profile;
pub mod settings;
pub mod status;
pub mod window;

use std::sync::Arc;

use tauri::State;

use crate::state::AppState;

pub type AppStateRef<'a> = State<'a, Arc<AppState>>;
