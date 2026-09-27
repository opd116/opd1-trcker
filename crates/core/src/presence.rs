//! Decodes the base64-encoded `private` blob of the local client's own
//! `/chat/v4/presences` entry into a [`SelfPresence`].
//!
//! Two on-the-wire shapes exist for that blob:
//! - the flat shape documented by techchrism's valorant-api-docs
//!   (`valorant-api-types/src/endpoints/local/Presence.ts`,
//!   `valorantPresenceSchema`): `sessionLoopState`, `partyId`, `queueId`
//!   live at the top level.
//! - a newer nested shape observed by Valorant-Scout
//!   (`backend/live_match.py::game_state`/`party_map`/`match_score`):
//!   the same fields live under `matchPresenceData` / `partyPresenceData`.
//!
//! We read the top-level field first and fall back to the nested one, as
//! Scout's `game_state`/`party_snapshot` helpers do.

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use serde::Serialize;
use serde_json::{Map, Value};

/// Where the local player currently is in the client.
///
/// `Closed` is never produced by [`decode_presence`] — it means "no
/// lockfile / Riot client not running", which only the caller (the Riot
/// session, not this parser) can know.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Stage {
    Closed,
    Menus,
    Pregame,
    Ingame,
}

/// The pieces of the local player's own presence this app needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelfPresence {
    pub stage: Stage,
    pub party_id: Option<String>,
    pub queue_id: Option<String>,
}

fn str_field<'a>(obj: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    obj.get(key).and_then(Value::as_str)
}

fn nested_str_field<'a>(obj: &'a Map<String, Value>, outer: &str, inner: &str) -> Option<&'a str> {
    obj.get(outer)
        .and_then(Value::as_object)
        .and_then(|nested| nested.get(inner))
        .and_then(Value::as_str)
}

/// Turns an empty string into `None`, per the Riot API's convention of
/// using `""` for "not applicable" (no party, no queue).
fn non_empty(s: Option<&str>) -> Option<String> {
    s.filter(|s| !s.is_empty()).map(str::to_string)
}

/// Decodes the local player's own presence `private` blob.
///
/// Returns `None` only when `private_b64` is not valid base64, or the
/// decoded bytes are not a JSON object. A missing or unrecognized
/// `sessionLoopState` maps to [`Stage::Menus`] (matching Scout's default),
/// not `None`.
pub fn decode_presence(private_b64: &str) -> Option<SelfPresence> {
    let bytes = BASE64.decode(private_b64).ok()?;
    let value: Value = serde_json::from_slice(&bytes).ok()?;
    let obj = value.as_object()?;

    let session_loop_state = str_field(obj, "sessionLoopState")
        .or_else(|| nested_str_field(obj, "matchPresenceData", "sessionLoopState"));

    let stage = match session_loop_state {
        Some("MENUS") => Stage::Menus,
        Some("PREGAME") => Stage::Pregame,
        Some("INGAME") => Stage::Ingame,
        _ => Stage::Menus,
    };

    let party_id = non_empty(
        str_field(obj, "partyId").or_else(|| nested_str_field(obj, "partyPresenceData", "partyId")),
    );

    let queue_id = non_empty(
        str_field(obj, "queueId").or_else(|| nested_str_field(obj, "matchPresenceData", "queueId")),
    );

    Some(SelfPresence {
        stage,
        party_id,
        queue_id,
    })
}
