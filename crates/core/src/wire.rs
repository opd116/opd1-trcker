//! The v2 redesign's additions to the UI contract (R189, user 2026-10-22).
//!
//! Each type is new: nothing in `model.rs` changes shape, so the existing
//! goldens stay valid. Same conventions as `model.rs`: camelCase on the
//! wire (R10), map/agent ids are valorant-api uuids, rates are 0..1
//! fractions (R5), `None` is a known-missing value shown as `—`.
//!
//! - [`AppStatus`] — the `"status"` event / `app_status` command: the
//!   connection pill, the lost/error states, the Agent Select timer, the
//!   queue timer and the post-match recap pointer.
//! - [`PlayerMarks`] — the `"marks"` event / `player_marks` command: saved
//!   players, private notes and dismissed smurf flags.
//! - [`SavedPlayer`] — one row of the Saved screen (`saved_players`).
//! - [`HistoryPage`] / [`HistoryRow`] — the Match History list
//!   (`match_history`).
//! - [`Settings`] — the Settings screen (`get_settings` / `set_settings`,
//!   the `"settings"` event).
//! - [`AsciiArt`] — one saved ASCII gallery item.
//! - [`AppInfo`] — the About screen (`app_info`).
//! - [`Collection`] / [`OwnedSkin`] — the Skins screen (`collection`).
//! - [`ProfileExtras`] — the profile drawer's per-map record and the
//!   "in your match" chips of its recent rows (`profile_extras`).

use serde::{Deserialize, Serialize};

use crate::model::{DisplayName, MatchResultKind, StatRow};

// ---------------------------------------------------------------------
// AppStatus
// ---------------------------------------------------------------------

/// The title-bar pill. `Waiting`: the Riot Client answers but VALORANT is
/// not running. `Ok`: the game is running and the last poll succeeded.
/// `Lost`: the game was running and polls are failing (retrying). `Error`:
/// no lockfile / the Riot Client does not answer at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConnState {
    Waiting,
    Ok,
    Lost,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PregameInfo {
    /// When the agent-select phase ends (epoch ms), from pregame's
    /// `PhaseTimeRemainingNS`; `None` when Riot gave none.
    pub phase_ends_ms: Option<i64>,
    /// Ally players who locked an agent.
    pub locked: u8,
    /// Ally players in the roster.
    pub total: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueInfo {
    /// Riot's queue id, e.g. `competitive`, `unrated`, `swiftplay`.
    pub queue_id: String,
    /// When matchmaking started (epoch ms).
    pub searching_since_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppStatus {
    pub conn: ConnState,
    pub riot_client_version: Option<String>,
    /// The last successful local-API answer (epoch ms).
    pub last_data_ms: Option<i64>,
    /// While `Lost`: the current reconnect attempt (1-based) of `max_attempts`.
    pub attempt: u32,
    pub max_attempts: u32,
    /// While `Lost` / `Error`: when the next automatic retry runs, ms from now.
    pub retry_in_ms: Option<i64>,
    /// While `Error`: `LOCKFILE_NOT_FOUND` or `CLIENT_UNREACHABLE`.
    pub error_code: Option<String>,
    pub error_at_ms: Option<i64>,
    /// The lockfile path the app looks at (shown on the error screen).
    pub lockfile_path: String,
    /// Agent Select only.
    pub pregame: Option<PregameInfo>,
    /// In the menus while matchmaking only.
    pub queue: Option<QueueInfo>,
    /// The user's own match that just ended: set when a match ends, cleared
    /// when the user queues again (or enters Agent Select). The UI shows the
    /// recap from `match_details(recapMatchId, self)` + My Stats.
    pub recap_match_id: Option<String>,
    /// The VALORANT build, e.g. `release-11.08`.
    pub game_version: Option<String>,
}

impl AppStatus {
    /// The starting value, before the first poll: the Riot Client has not
    /// been looked at yet, so the pill waits.
    pub fn initial(lockfile_path: String) -> Self {
        AppStatus {
            conn: ConnState::Waiting,
            riot_client_version: None,
            last_data_ms: None,
            attempt: 0,
            max_attempts: 5,
            retry_in_ms: None,
            error_code: None,
            error_at_ms: None,
            lockfile_path,
            pregame: None,
            queue: None,
            recap_match_id: None,
            game_version: None,
        }
    }
}

// ---------------------------------------------------------------------
// Saved players, notes, smurf flags
// ---------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerMarks {
    /// Saved puuids, newest saved first.
    pub saved: Vec<String>,
    /// Private notes by puuid (non-empty only). The user's own puuid holds
    /// their personal notes.
    pub notes: std::collections::BTreeMap<String, String>,
    /// Puuids whose smurf flag the user dismissed ("Not a smurf").
    pub not_smurf: Vec<String>,
}

/// Stored competitive matches shared with the user on one side.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedRecord {
    pub games: u32,
    /// From the user's side.
    pub wins: u32,
    pub losses: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedPlayer {
    pub puuid: String,
    /// Hidden-name rules apply (R18/R145).
    pub name: DisplayName,
    /// The agent of their newest stored match.
    pub agent_id: Option<String>,
    /// The newest known competitive tier.
    pub tier: Option<u8>,
    /// Same team as the user.
    pub with: SharedRecord,
    /// The other team.
    pub against: SharedRecord,
    /// `""` when there is none.
    pub note: String,
    pub saved_ms: i64,
    /// Start of the newest stored match with the user; `None` when none.
    pub last_seen_ms: Option<i64>,
    pub last_seen_map_id: Option<String>,
}

// ---------------------------------------------------------------------
// Match history
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HistoryQueue {
    Competitive,
    All,
}

/// One of the user's own stored matches, any queue.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryRow {
    pub match_id: String,
    pub map_id: Option<String>,
    pub queue_id: String,
    pub start_ms: i64,
    pub duration_ms: Option<i64>,
    pub result: MatchResultKind,
    pub rounds_won: Option<u32>,
    pub rounds_lost: Option<u32>,
    pub agent_id: Option<String>,
    /// R102: the stats are all `None` when the match has no stats row.
    pub kills: Option<u32>,
    pub deaths: Option<u32>,
    pub assists: Option<u32>,
    pub kd: Option<f32>,
    pub acs: Option<f32>,
    /// Fraction 0..1; `None` also when no shot landed.
    pub hs_pct: Option<f32>,
    /// Competitive only; `None` when Riot recorded no change.
    pub rr_earned: Option<i32>,
    /// The tier after this match (competitive only).
    pub tier_after: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryPage {
    /// Newest first.
    pub rows: Vec<HistoryRow>,
    /// Stored matches in the asked queue.
    pub total: u32,
    /// Counts for the two queue tabs.
    pub competitive_total: u32,
    pub all_total: u32,
}

// ---------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SmurfLevel {
    Off,
    Cautious,
    Eager,
}

/// Stored as JSON in `app_state` under `ui_settings`; survives updates.
/// Unknown or missing keys fall back to the defaults (the design's).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub highlight: bool,
    pub enemy_first: bool,
    pub smurf: SmurfLevel,
    pub saved_alert: bool,
    pub sound: bool,
    pub on_top: bool,
    pub boot: bool,
    pub tray: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            highlight: true,
            enemy_first: false,
            smurf: SmurfLevel::Cautious,
            saved_alert: true,
            sound: false,
            on_top: true,
            boot: true,
            tray: true,
        }
    }
}

// ---------------------------------------------------------------------
// ASCII gallery, About
// ---------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AsciiArt {
    pub id: i64,
    pub title: String,
    /// The art itself, rows joined by `\n`.
    pub raw: String,
    pub created_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    /// Short git hash of the build, when known.
    pub commit: Option<String>,
    /// Build date (epoch ms), when known.
    pub build_ms: Option<i64>,
    pub riot_client_version: Option<String>,
    pub valorant_version: Option<String>,
    /// Stored matches in the local DB.
    pub db_matches: u32,
    /// Size of the DB file.
    pub db_bytes: u64,
}

// ---------------------------------------------------------------------
// Skins collection
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Edition {
    Select,
    Deluxe,
    Premium,
    Exclusive,
    Ultra,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnedSkin {
    /// valorant-api skin uuid.
    pub skin_id: String,
    /// The skin's first level uuid (its `skinLevel` image).
    pub level_id: String,
    pub name: String,
    /// valorant-api weapon uuid.
    pub weapon_id: String,
    /// `None` for skins without a content tier (battle pass, free).
    pub edition: Option<Edition>,
    /// The standard store price for its edition; `None` when not sold for VP.
    pub vp: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeaponInfo {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Collection {
    /// Owned skins, standard (default) skins left out.
    pub items: Vec<OwnedSkin>,
    /// Every weapon, in valorant-api's order.
    pub weapons: Vec<WeaponInfo>,
    /// When the owned list was fetched from Riot (epoch ms).
    pub fetched_ms: i64,
}

// ---------------------------------------------------------------------
// Profile extras
// ---------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveMates {
    pub match_id: String,
    /// Players of the current live match who were in that stored match.
    pub puuids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileExtras {
    pub puuid: String,
    /// Current-act competitive record per map, most played first.
    pub maps: Vec<StatRow>,
    /// For the player's recent matches (`PlayerDetails.recent`).
    pub live_mates: Vec<LiveMates>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_fill_missing_keys_with_defaults() {
        let s: Settings = serde_json::from_str(r#"{"highlight":false}"#).unwrap();
        assert!(!s.highlight);
        assert_eq!(s.smurf, SmurfLevel::Cautious);
        assert!(s.tray);
    }

    #[test]
    fn settings_round_trip_in_camel_case() {
        let json = serde_json::to_string(&Settings::default()).unwrap();
        assert!(json.contains("\"enemyFirst\":false"));
        assert!(json.contains("\"smurf\":\"cautious\""));
    }
}
