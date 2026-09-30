//! Raw-response capture: the last response body seen at each of
//! `session.rs`'s two HTTP choke points (`call_local`/`call_remote`), kept
//! around so Task 5b's `dump_raw()` command can write them to disk for
//! Task 11's fixtures. Bounded (one body per endpoint, each capped at
//! [`MAX_CAPTURE_BYTES`]) and always redacted before anything reads it
//! back out — see [`redact_bytes`].
//!
//! Not captured: the `/entitlements/v1/token` call. It's made directly by
//! `fetch_entitlements`/`refresh_entitlements`, never through
//! `call_local`/`call_remote` (it runs before a `RiotSession` even exists,
//! at `connect()` time, and again bypassing them on re-auth) — see
//! task-5a-report.md's concerns for what Task 5b should know about this.
//!
//! Also not captured: a non-2xx response (fix-round item 9 / R36) — only
//! `session.rs`'s choke points decide this (they see the status code;
//! [`RawCaptures::record`] itself has no opinion), by simply not calling
//! [`RawCaptures::record`] for one. A 500/404/etc body is rarely what
//! Task 11 wants as a fixture, and skipping it means a real error
//! response can never overwrite a good, previously-captured success body
//! for the same endpoint.

use std::collections::HashMap;
use std::sync::Mutex;

use serde_json::Value;

/// A captured body larger than this is stored as a small truncation
/// marker instead (see [`RawCaptures::record`]) rather than the body
/// itself. Real match-details bodies can run several hundred KB to
/// low-single-digit MB for a long match, so this is sized well above the
/// ~512KB this module originally shipped with (fix-round item 9 / R36) —
/// still one bounded body per endpoint, just a more generous bound.
pub const MAX_CAPTURE_BYTES: usize = 2 * 1024 * 1024;

/// Object keys (matched case-insensitively, by substring) whose value is
/// always replaced regardless of type or shape.
const REDACT_KEY_SUBSTRINGS: &[&str] = &[
    "accesstoken",
    "token",
    "entitlements",
    "password",
    "authorization",
];

const REDACTED: &str = "REDACTED";

/// Turns a request path into a stable, human-readable name — the
/// filename Task 5b's `dump_raw()` will write to
/// (`<endpoint>.json`) — by recognizing this app's known endpoints and
/// falling back to a generic normalizer (dynamic-id segments replaced)
/// for anything else.
pub(crate) fn endpoint_key(path: &str) -> String {
    let path_only = path.split('?').next().unwrap_or(path);

    let prefix_matches: &[(&str, &str)] = &[
        ("/pregame/v1/players/", "pregame_player"),
        ("/core-game/v1/players/", "coregame_player"),
        ("/match-history/v1/history/", "match_history"),
        ("/match-details/v1/matches/", "match_details"),
        ("/parties/v1/players/", "party_of"),
        ("/parties/v1/parties/", "party"),
    ];
    for (prefix, name) in prefix_matches {
        if path_only.starts_with(prefix) {
            return name.to_string();
        }
    }

    if let Some(rest) = path_only.strip_prefix("/pregame/v1/matches/") {
        return if rest.ends_with("/loadouts") {
            "pregame_loadouts".to_string()
        } else {
            "pregame_match".to_string()
        };
    }
    if let Some(rest) = path_only.strip_prefix("/core-game/v1/matches/") {
        return if rest.ends_with("/loadouts") {
            "coregame_loadouts".to_string()
        } else {
            "coregame_match".to_string()
        };
    }
    if let Some(rest) = path_only.strip_prefix("/mmr/v1/players/") {
        return if rest.ends_with("/competitiveupdates") {
            "comp_updates".to_string()
        } else {
            "mmr".to_string()
        };
    }

    // R90: `/personalization/v2/players/{puuid}/playerloadout`.
    if path_only.starts_with("/personalization/v2/players/")
        && path_only.ends_with("/playerloadout")
    {
        return "player_loadout".to_string();
    }

    match path_only {
        "/chat/v4/presences" => return "presences".to_string(),
        "/name-service/v2/players" => return "names".to_string(),
        _ => {}
    }

    normalize_generic(path_only)
}

fn normalize_generic(path: &str) -> String {
    let trimmed = path.trim_matches('/');
    if trimmed.is_empty() {
        return "root".to_string();
    }
    trimmed
        .split('/')
        .map(|seg| if looks_like_id(seg) { "id" } else { seg })
        .collect::<Vec<_>>()
        .join("_")
}

fn looks_like_id(seg: &str) -> bool {
    let is_uuid = seg.len() == 36
        && seg.char_indices().all(|(i, c)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                c == '-'
            } else {
                c.is_ascii_hexdigit()
            }
        });
    let is_numeric = !seg.is_empty() && seg.chars().all(|c| c.is_ascii_digit());
    is_uuid || is_numeric
}

// ---------------------------------------------------------------------
// redaction
// ---------------------------------------------------------------------

/// A JWT is three `.`-separated base64url segments; real ones are long
/// (Riot's entitlements JWT header segment alone is well over this), so a
/// short first segment is treated as "not actually a JWT" to avoid
/// false-positiving on an unrelated dotted string.
fn looks_like_jwt(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    parts.len() == 3
        && parts[0].len() >= 10
        && parts.iter().all(|p| {
            !p.is_empty()
                && p.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        })
}

fn redact_value(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, v) in map.iter_mut() {
                let key_lower = key.to_ascii_lowercase();
                if REDACT_KEY_SUBSTRINGS
                    .iter()
                    .any(|needle| key_lower.contains(needle))
                {
                    *v = Value::String(REDACTED.to_string());
                } else {
                    redact_value(v);
                }
            }
        }
        Value::Array(items) => {
            for v in items.iter_mut() {
                redact_value(v);
            }
        }
        Value::String(s) if looks_like_jwt(s) => {
            *value = Value::String(REDACTED.to_string());
        }
        _ => {}
    }
}

/// Scans plain (non-JSON) text for JWT-shaped substrings and redacts them
/// — the fallback path for a body that isn't valid JSON (an HTML error
/// page, an empty body, etc.). Manual scan rather than a `regex`
/// dependency: the shape is simple (`eyJ` then two more base64url runs
/// separated by single dots) and this path is a defensive fallback, not
/// the common case (every endpoint this app calls returns JSON).
fn redact_jwts_in_plain_text(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        if s[i..].starts_with("eyJ") {
            let mut j = i;
            let is_token_char =
                |c: u8| c.is_ascii_alphanumeric() || c == b'-' || c == b'_' || c == b'.';
            while j < bytes.len() && is_token_char(bytes[j]) {
                j += 1;
            }
            let candidate = &s[i..j];
            if looks_like_jwt(candidate) {
                out.push_str(REDACTED);
                i = j;
                continue;
            }
        }
        // Advance by one UTF-8 scalar, not one byte, so multi-byte
        // characters aren't split.
        let ch = s[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// Redacts a captured response body: parsed-and-walked as JSON when
/// possible (the expected case — every endpoint here returns JSON),
/// falling back to a plain-text JWT scan otherwise. Always valid UTF-8 out
/// (non-UTF8 bytes in a non-JSON body are lossily replaced) since this is
/// only ever used to produce a human-readable debug file.
pub(crate) fn redact_bytes(body: &[u8]) -> String {
    match serde_json::from_slice::<Value>(body) {
        Ok(mut value) => {
            redact_value(&mut value);
            serde_json::to_string_pretty(&value).unwrap_or_else(|_| REDACTED.to_string())
        }
        Err(_) => redact_jwts_in_plain_text(&String::from_utf8_lossy(body)),
    }
}

// ---------------------------------------------------------------------
// RawCaptures — the bounded per-endpoint store
// ---------------------------------------------------------------------

#[derive(Default)]
pub(crate) struct RawCaptures {
    inner: Mutex<HashMap<String, Vec<u8>>>,
}

impl RawCaptures {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Records `body` as the latest response for the endpoint `path`
    /// resolves to, overwriting whatever was captured for that endpoint
    /// before. Callers (`session.rs`'s choke points) are expected to only
    /// call this for a 2xx response — see the module docs.
    ///
    /// Fix-round item 9 / R36: a body over [`MAX_CAPTURE_BYTES`] is
    /// **never** byte-truncated into storage — truncating valid JSON mid
    /// -stream would leave invalid JSON in the debug dump, defeating the
    /// point of Task 11 reading it back as a fixture. Instead, a small,
    /// always-valid JSON marker (`{"truncated":true,"bytes":N}`) is stored
    /// in its place, overwriting whatever was there — an oversized
    /// response is itself the interesting fact to record, and keeping a
    /// stale unrelated capture around under this endpoint's name would be
    /// more misleading than useful.
    pub(crate) fn record(&self, path: &str, body: &[u8]) {
        let key = endpoint_key(path);
        let stored: Vec<u8> = if body.len() > MAX_CAPTURE_BYTES {
            format!(r#"{{"truncated":true,"bytes":{}}}"#, body.len()).into_bytes()
        } else {
            body.to_vec()
        };
        self.inner.lock().unwrap().insert(key, stored);
    }

    /// Every captured endpoint's last body, redacted, sorted by endpoint
    /// name for deterministic output.
    pub(crate) fn snapshot(&self) -> Vec<(String, String)> {
        let guard = self.inner.lock().unwrap();
        let mut out: Vec<(String, String)> = guard
            .iter()
            .map(|(k, v)| (k.clone(), redact_bytes(v)))
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn endpoint_key_recognizes_known_paths() {
        assert_eq!(endpoint_key("/pregame/v1/players/abc"), "pregame_player");
        assert_eq!(endpoint_key("/pregame/v1/matches/abc"), "pregame_match");
        assert_eq!(
            endpoint_key("/pregame/v1/matches/abc/loadouts"),
            "pregame_loadouts"
        );
        assert_eq!(endpoint_key("/core-game/v1/players/abc"), "coregame_player");
        assert_eq!(endpoint_key("/core-game/v1/matches/abc"), "coregame_match");
        assert_eq!(
            endpoint_key("/core-game/v1/matches/abc/loadouts"),
            "coregame_loadouts"
        );
        assert_eq!(endpoint_key("/mmr/v1/players/abc"), "mmr");
        assert_eq!(
            endpoint_key("/mmr/v1/players/abc/competitiveupdates?queue=competitive"),
            "comp_updates"
        );
        assert_eq!(
            endpoint_key("/match-history/v1/history/abc?startIndex=0"),
            "match_history"
        );
        assert_eq!(
            endpoint_key("/match-details/v1/matches/abc"),
            "match_details"
        );
        assert_eq!(endpoint_key("/parties/v1/players/abc"), "party_of");
        assert_eq!(endpoint_key("/parties/v1/parties/abc"), "party");
        assert_eq!(endpoint_key("/chat/v4/presences"), "presences");
        assert_eq!(endpoint_key("/name-service/v2/players"), "names");
    }

    #[test]
    fn endpoint_key_names_the_player_loadout() {
        // R90: `dump_raw` writes it as `player_loadout.json`.
        assert_eq!(
            endpoint_key("/personalization/v2/players/abc/playerloadout"),
            "player_loadout"
        );
        assert_eq!(
            endpoint_key(
                "/personalization/v2/players/123e4567-e89b-12d3-a456-426614174000/playerloadout"
            ),
            "player_loadout"
        );
    }

    #[test]
    fn endpoint_key_falls_back_to_generic_normalization_for_unknown_paths() {
        assert_eq!(
            endpoint_key("/some-new/v1/players/123e4567-e89b-12d3-a456-426614174000"),
            "some-new_v1_players_id"
        );
        assert_eq!(
            endpoint_key("/some-new/v1/things/42"),
            "some-new_v1_things_id"
        );
    }

    #[test]
    fn redact_bytes_redacts_access_token_and_entitlements_jwt_fields() {
        let body = json!({
            "accessToken": "super-secret-access-token-value",
            "subject": "aaaaaaaa-0000-0000-0000-000000000001",
            "token": "super-secret-entitlements-jwt-value",
        });
        let redacted = redact_bytes(body.to_string().as_bytes());
        let parsed: Value = serde_json::from_str(&redacted).unwrap();
        assert_eq!(parsed["accessToken"], REDACTED);
        assert_eq!(parsed["token"], REDACTED);
        assert_eq!(parsed["subject"], "aaaaaaaa-0000-0000-0000-000000000001");
    }

    #[test]
    fn redact_bytes_redacts_password_field_case_insensitively() {
        let body = json!({ "Password": "hunter2", "ok": true });
        let redacted = redact_bytes(body.to_string().as_bytes());
        let parsed: Value = serde_json::from_str(&redacted).unwrap();
        assert_eq!(parsed["Password"], REDACTED);
        assert_eq!(parsed["ok"], true);
    }

    #[test]
    fn redact_bytes_redacts_jwt_looking_strings_under_any_key() {
        // A JWT under an unanticipated key name is still caught, since
        // detection also runs on string *values*, not just key names.
        let jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dQw4w9WgXcQ";
        let body = json!({ "someWeirdField": jwt });
        let redacted = redact_bytes(body.to_string().as_bytes());
        let parsed: Value = serde_json::from_str(&redacted).unwrap();
        assert_eq!(parsed["someWeirdField"], REDACTED);
    }

    #[test]
    fn redact_bytes_redacts_nested_objects_and_arrays() {
        let body = json!({
            "outer": {
                "inner": [
                    { "accessToken": "leaked-1" },
                    { "accessToken": "leaked-2" }
                ]
            }
        });
        let redacted = redact_bytes(body.to_string().as_bytes());
        let parsed: Value = serde_json::from_str(&redacted).unwrap();
        assert_eq!(parsed["outer"]["inner"][0]["accessToken"], REDACTED);
        assert_eq!(parsed["outer"]["inner"][1]["accessToken"], REDACTED);
    }

    #[test]
    fn redact_bytes_leaves_ordinary_fields_untouched() {
        let body = json!({ "gameName": "Shanky", "tagLine": "OPD1", "kills": 22 });
        let redacted = redact_bytes(body.to_string().as_bytes());
        let parsed: Value = serde_json::from_str(&redacted).unwrap();
        assert_eq!(parsed["gameName"], "Shanky");
        assert_eq!(parsed["tagLine"], "OPD1");
        assert_eq!(parsed["kills"], 22);
    }

    #[test]
    fn redact_bytes_falls_back_to_plain_text_scan_for_non_json_bodies() {
        let jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dQw4w9WgXcQ";
        let body = format!("<html>error, token was {jwt}</html>");
        let redacted = redact_bytes(body.as_bytes());
        assert!(!redacted.contains(jwt));
        assert!(redacted.contains(REDACTED));
        assert!(redacted.contains("<html>error, token was"));
    }

    #[test]
    fn redact_bytes_handles_empty_and_invalid_utf8_bodies_without_panicking() {
        assert_eq!(redact_bytes(&[]), "");
        let invalid_utf8 = [0xFF, 0xFE, 0xFD];
        // Must not panic; exact content isn't asserted (lossy replacement).
        let _ = redact_bytes(&invalid_utf8);
    }

    #[test]
    fn raw_captures_keeps_only_the_last_body_per_endpoint() {
        let captures = RawCaptures::new();
        captures.record("/mmr/v1/players/abc", br#"{"v":1}"#);
        captures.record("/mmr/v1/players/abc", br#"{"v":2}"#);

        let snapshot = captures.snapshot();
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].0, "mmr");
        assert!(snapshot[0].1.contains("\"v\": 2"));
    }

    #[test]
    fn raw_captures_stores_a_truncation_marker_instead_of_invalid_json_for_oversized_bodies() {
        let captures = RawCaptures::new();
        let huge_len = MAX_CAPTURE_BYTES + 10;
        let huge = vec![b'a'; huge_len];
        captures.record("/name-service/v2/players", &huge);

        // The stored bytes are themselves valid, parseable JSON — never a
        // byte-truncated (and therefore corrupt) slice of the real body.
        let snapshot = captures.snapshot();
        assert_eq!(snapshot.len(), 1);
        let (key, body) = &snapshot[0];
        assert_eq!(key, "names");
        let parsed: Value = serde_json::from_str(body).expect("marker must be valid JSON");
        assert_eq!(parsed["truncated"], true);
        assert_eq!(parsed["bytes"], huge_len);
    }

    #[test]
    fn raw_captures_records_a_body_under_the_cap_unchanged() {
        let captures = RawCaptures::new();
        captures.record("/mmr/v1/players/abc", br#"{"ok":true}"#);
        let snapshot = captures.snapshot();
        assert_eq!(snapshot[0].0, "mmr");
        let parsed: Value = serde_json::from_str(&snapshot[0].1).unwrap();
        assert_eq!(parsed["ok"], true);
    }
}
