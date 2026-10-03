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
//! Also not captured *as a success*: a non-2xx response (fix-round item 9 /
//! R36) — only `session.rs`'s choke points decide this (they see the status
//! code; [`RawCaptures::record`] itself has no opinion), by simply not
//! calling [`RawCaptures::record`] for one. A 500/404/etc body is rarely what
//! Task 11 wants as a fixture, and skipping it means a real error
//! response can never overwrite a good, previously-captured success body
//! for the same endpoint.
//!
//! What Riot said when it refused (its `errorCode`) is the other half of a
//! field report (R158), so a non-2xx body goes to a *separate* store —
//! [`RawCaptures::record_error`]: one per endpoint key, redacted like the
//! rest and cut to [`MAX_ERROR_CAPTURE_BYTES`], overwritten only by the next
//! error of that endpoint (a later 2xx neither deletes nor replaces it).

use std::collections::HashMap;
use std::sync::Mutex;

use serde_json::Value;

use crate::locks::locked;

/// A captured body larger than this is stored as a small truncation
/// marker instead (see [`RawCaptures::record`]) rather than the body
/// itself. Real match-details bodies can run several hundred KB to
/// low-single-digit MB for a long match, so this is sized well above the
/// ~512KB this module originally shipped with (fix-round item 9 / R36) —
/// still one bounded body per endpoint, just a more generous bound.
pub const MAX_CAPTURE_BYTES: usize = 2 * 1024 * 1024;

/// A non-2xx body is kept redacted and cut to this many bytes, one per
/// endpoint key (R158): a few dozen KB at the very most, whatever Riot sends.
pub const MAX_ERROR_CAPTURE_BYTES: usize = 4 * 1024;

/// How many characters of an error body a log line carries (R158).
pub const LOG_EXCERPT_CHARS: usize = 200;

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

    // R90: `/personalization/v3/players/{puuid}/playerloadout` (v2 before
    // Riot moved it; both name the same capture file).
    if (path_only.starts_with("/personalization/v3/players/")
        || path_only.starts_with("/personalization/v2/players/"))
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
        // A token quoted inside a longer string ("invalid bearer eyJ... for
        // player"): the same scan the plain-text path runs.
        Value::String(s) if s.contains("eyJ") => {
            *s = redact_jwts_in_plain_text(s);
        }
        _ => {}
    }
}

/// Scans plain text for JWT-shaped substrings and redacts them — the path
/// for a body that isn't valid JSON (an HTML error page, an empty body,
/// etc.), and for a string value inside a JSON body that quotes a token
/// within longer text. Manual scan rather than a `regex`
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
            // A token ending a sentence ("... was eyJ....abc.") is still one:
            // trailing dots belong to the text around it, not to the token.
            while j > i && bytes[j - 1] == b'.' {
                j -= 1;
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

/// Whether `b` (exactly 36 bytes) is a canonical hyphenated uuid.
fn is_uuid_bytes(b: &[u8]) -> bool {
    b.len() == 36
        && b.iter().enumerate().all(|(i, c)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                *c == b'-'
            } else {
                c.is_ascii_hexdigit()
            }
        })
}

/// Every full uuid in `s` (a puuid, a match id) shortened to its first 8
/// characters — the way the log writes ids (`logbuf::short`). A uuid glued to
/// more hex digits is left alone: it is not one.
pub(crate) fn shorten_uuids(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < b.len() {
        let is_id = i + 36 <= b.len()
            && is_uuid_bytes(&b[i..i + 36])
            && !b.get(i + 36).is_some_and(u8::is_ascii_hexdigit)
            && !(i > 0 && b[i - 1].is_ascii_hexdigit());
        if is_id {
            out.push_str(&s[i..i + 8]);
            i += 36;
            continue;
        }
        // `i` is always on a char boundary: a uuid run is pure ASCII.
        let ch = s[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// `s` cut to at most `max_chars` characters, with an ellipsis when it was
/// cut.
fn cut_chars(s: String, max_chars: usize) -> String {
    match s.char_indices().nth(max_chars) {
        Some((end, _)) => format!("{}…", &s[..end]),
        None => s,
    }
}

/// A short, log-safe rendition of a response body or an error message (R158):
/// redacted like a capture (secret-named keys and JWT-shaped strings are gone),
/// JSON on one line, every full uuid shortened to 8 characters, whitespace
/// runs collapsed, and at most `max_chars` characters (an ellipsis marks a cut).
/// Never carries a token or a full puuid; carries no URL unless the body itself
/// quoted one.
pub(crate) fn excerpt(body: &[u8], max_chars: usize) -> String {
    let redacted = match serde_json::from_slice::<Value>(body) {
        Ok(mut value) => {
            redact_value(&mut value);
            serde_json::to_string(&value).unwrap_or_else(|_| REDACTED.to_string())
        }
        Err(_) => redact_jwts_in_plain_text(&String::from_utf8_lossy(body)),
    };
    let shortened = shorten_uuids(&redacted);
    let one_line = shortened.split_whitespace().collect::<Vec<_>>().join(" ");
    cut_chars(one_line, max_chars)
}

// ---------------------------------------------------------------------
// RawCaptures — the bounded per-endpoint store
// ---------------------------------------------------------------------

/// One endpoint's last non-2xx answer, as [`RawCaptures::record_error`] keeps
/// it: the status, and the body redacted and cut to
/// [`MAX_ERROR_CAPTURE_BYTES`].
struct ErrorCapture {
    status: u16,
    text: String,
    cut: bool,
}

/// One error capture ready to be written next to the 2xx captures
/// (`<key>.error.<ext>`): see [`RawCaptures::error_snapshot`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorDump {
    /// The endpoint key (`mmr`, `comp_updates`, ...), like a 2xx capture's.
    pub key: String,
    /// `"json"` when the body was JSON (the file is `{"status": N, "body": ...}`),
    /// else `"txt"` (the file's first line is `HTTP N`).
    pub ext: &'static str,
    pub contents: String,
}

/// `s` cut to at most `max` bytes on a char boundary; whether it was cut.
fn cut_bytes(s: String, max: usize) -> (String, bool) {
    if s.len() <= max {
        return (s, false);
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    (s[..end].to_string(), true)
}

#[derive(Default)]
pub(crate) struct RawCaptures {
    inner: Mutex<HashMap<String, Vec<u8>>>,
    /// R158: the last non-2xx answer per endpoint key, apart from `inner` so a
    /// later 2xx cannot delete it and an error can never replace a 2xx capture.
    errors: Mutex<HashMap<String, ErrorCapture>>,
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
        locked(&self.inner).insert(key, stored);
    }

    /// R158: records a non-2xx answer (`status`, `body`) as the latest error
    /// of the endpoint `path` resolves to, overwriting that endpoint's earlier
    /// error and nothing else. The body is redacted the moment it is stored and
    /// cut to [`MAX_ERROR_CAPTURE_BYTES`], so one key never holds more than
    /// that; a body over [`MAX_CAPTURE_BYTES`] is not even scanned (a marker
    /// stands in, as for a 2xx capture). Full uuids are shortened to their
    /// first 8 characters, as in a log line.
    pub(crate) fn record_error(&self, path: &str, status: u16, body: &[u8]) {
        let key = endpoint_key(path);
        let redacted = if body.len() > MAX_CAPTURE_BYTES {
            format!(r#"{{"truncated":true,"bytes":{}}}"#, body.len())
        } else {
            // Unlike a 2xx capture (a fixture, full ids and all), an error
            // file is a field report: no full puuid or match id in it either.
            shorten_uuids(&redact_bytes(body))
        };
        let (text, cut) = cut_bytes(redacted, MAX_ERROR_CAPTURE_BYTES);
        locked(&self.errors).insert(key, ErrorCapture { status, text, cut });
    }

    /// Every endpoint's last error as a file: JSON bodies as
    /// `{"status": N, "body": {...}}`, anything else (an HTML page, an empty
    /// body, a body cut mid-JSON) as text whose first line is `HTTP N`. Sorted
    /// by endpoint key.
    pub(crate) fn error_snapshot(&self) -> Vec<ErrorDump> {
        let guard = locked(&self.errors);
        let mut out: Vec<ErrorDump> = guard
            .iter()
            .map(|(key, e)| {
                let json = (!e.cut)
                    .then(|| serde_json::from_str::<Value>(&e.text).ok())
                    .flatten();
                match json {
                    Some(body) => ErrorDump {
                        key: key.clone(),
                        ext: "json",
                        contents: serde_json::to_string_pretty(
                            &serde_json::json!({ "status": e.status, "body": body }),
                        )
                        .unwrap_or_default(),
                    },
                    None => ErrorDump {
                        key: key.clone(),
                        ext: "txt",
                        contents: format!(
                            "HTTP {}\n{}{}",
                            e.status,
                            e.text,
                            if e.cut { "\n[cut]" } else { "" }
                        ),
                    },
                }
            })
            .collect();
        out.sort_by(|a, b| a.key.cmp(&b.key));
        out
    }

    /// Every captured endpoint's last body, redacted, sorted by endpoint
    /// name for deterministic output.
    pub(crate) fn snapshot(&self) -> Vec<(String, String)> {
        let guard = locked(&self.inner);
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
    fn locks_poisoned_by_a_panicking_caller_do_not_stop_later_captures() {
        let captures = RawCaptures::new();
        std::thread::scope(|s| {
            let holders = [
                s.spawn(|| {
                    let _held = captures.inner.lock().unwrap();
                    panic!("a caller panicked while it held the captures lock");
                }),
                s.spawn(|| {
                    let _held = captures.errors.lock().unwrap();
                    panic!("a caller panicked while it held the errors lock");
                }),
            ];
            for holder in holders {
                let _ = holder.join();
            }
        });
        assert!(captures.inner.is_poisoned() && captures.errors.is_poisoned());

        captures.record("/mmr/v1/players/abc", br#"{"ok":true}"#);
        captures.record_error(
            "/match-history/v1/history/abc",
            500,
            br#"{"errorCode":"X"}"#,
        );
        let keys: Vec<String> = captures.snapshot().into_iter().map(|(k, _)| k).collect();
        assert_eq!(keys, ["mmr"]);
        let errors: Vec<String> = captures
            .error_snapshot()
            .into_iter()
            .map(|e| e.key)
            .collect();
        assert_eq!(errors, ["match_history"]);
    }

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
        // R90: `dump_raw` writes it as `player_loadout.json`. Riot moved the
        // endpoint from v2 to v3; both versions get the same file name.
        for version in ["v2", "v3"] {
            assert_eq!(
                endpoint_key(&format!(
                    "/personalization/{version}/players/abc/playerloadout"
                )),
                "player_loadout"
            );
            assert_eq!(
                endpoint_key(&format!(
                    "/personalization/{version}/players/123e4567-e89b-12d3-a456-426614174000/playerloadout"
                )),
                "player_loadout"
            );
        }
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

    // -- R158: what a log line may carry of an error body ------------------

    const PUUID: &str = "aaaaaaaa-0000-0000-0000-000000000001";
    const JWT: &str = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dQw4w9WgXcQ";

    #[test]
    fn excerpt_carries_riots_error_code_on_one_line() {
        let body = br#"{
            "httpStatus": 400,
            "errorCode": "BAD_CLAIMS",
            "message": "Invalid claims"
        }"#;
        let e = excerpt(body, LOG_EXCERPT_CHARS);
        assert!(e.contains(r#""errorCode":"BAD_CLAIMS""#), "{e}");
        assert!(e.contains(r#""httpStatus":400"#), "{e}");
        assert!(
            !e.contains('\n') && !e.contains("  "),
            "one line, collapsed: {e}"
        );
    }

    #[test]
    fn excerpt_never_carries_a_token() {
        let body = json!({
            "errorCode": "X",
            "accessToken": "opaque-secret-value",
            "detail": JWT,
            "nested": {"Authorization": "Bearer opaque-2"},
        });
        let e = excerpt(body.to_string().as_bytes(), LOG_EXCERPT_CHARS);
        for leaked in ["opaque-secret-value", JWT, "opaque-2"] {
            assert!(!e.contains(leaked), "{leaked} leaked: {e}");
        }
        assert!(e.contains("X"), "the errorCode stays: {e}");
        // A body that is not JSON: JWT-shaped strings still go.
        let e = excerpt(format!("<html> gateway said {JWT} </html>").as_bytes(), 200);
        assert!(!e.contains(JWT), "{e}");
        assert!(e.contains("<html> gateway said"), "{e}");
    }

    #[test]
    fn excerpt_shortens_a_full_puuid_to_its_first_8_characters() {
        let body = json!({"errorCode": "NOT_FOUND", "message": format!("no player {PUUID} here")});
        let e = excerpt(body.to_string().as_bytes(), LOG_EXCERPT_CHARS);
        assert!(!e.contains(PUUID), "{e}");
        assert!(e.contains("no player aaaaaaaa here"), "{e}");
        // Plain text too, and a uuid glued to more hex is not one.
        let e = excerpt(format!("{PUUID}/{PUUID}").as_bytes(), 200);
        assert_eq!(e, "aaaaaaaa/aaaaaaaa");
        let glued = format!("{PUUID}ff");
        assert_eq!(excerpt(glued.as_bytes(), 200), glued);
    }

    #[test]
    fn excerpt_is_cut_to_the_first_200_characters() {
        let long = "é".repeat(500);
        let e = excerpt(long.as_bytes(), LOG_EXCERPT_CHARS);
        assert_eq!(
            e.chars().count(),
            LOG_EXCERPT_CHARS + 1,
            "200 + the ellipsis"
        );
        assert!(e.starts_with(&"é".repeat(200)) && e.ends_with('…'));
        // Exactly 200 is not cut.
        let exact = "a".repeat(200);
        assert_eq!(excerpt(exact.as_bytes(), LOG_EXCERPT_CHARS), exact);
        assert_eq!(excerpt(b"", LOG_EXCERPT_CHARS), "");
    }

    #[test]
    fn a_json_error_body_is_dumped_with_its_status() {
        let captures = RawCaptures::new();
        captures.record_error(
            "/mmr/v1/players/abc",
            400,
            br#"{"httpStatus":400,"errorCode":"BAD_CLAIMS","message":"x"}"#,
        );
        let dumps = captures.error_snapshot();
        assert_eq!(dumps.len(), 1);
        assert_eq!((dumps[0].key.as_str(), dumps[0].ext), ("mmr", "json"));
        let parsed: Value = serde_json::from_str(&dumps[0].contents).unwrap();
        assert_eq!(parsed["status"], 400);
        assert_eq!(parsed["body"]["errorCode"], "BAD_CLAIMS");
    }

    #[test]
    fn a_text_error_body_is_dumped_as_text_with_the_status_on_the_first_line() {
        let captures = RawCaptures::new();
        captures.record_error(
            "/match-details/v1/matches/abc",
            502,
            b"<html>Bad Gateway</html>",
        );
        captures.record_error("/mmr/v1/players/abc", 500, b"");
        let dumps = captures.error_snapshot();
        assert_eq!(dumps.len(), 2);
        assert_eq!(
            (dumps[0].key.as_str(), dumps[0].ext),
            ("match_details", "txt")
        );
        assert_eq!(dumps[0].contents, "HTTP 502\n<html>Bad Gateway</html>");
        assert_eq!((dumps[1].key.as_str(), dumps[1].ext), ("mmr", "txt"));
        assert_eq!(dumps[1].contents, "HTTP 500\n");
    }

    #[test]
    fn an_error_body_is_redacted_before_it_is_kept() {
        let captures = RawCaptures::new();
        let body = json!({"errorCode": "E", "token": "opaque-secret", "note": JWT});
        captures.record_error("/mmr/v1/players/abc", 403, body.to_string().as_bytes());
        captures.record_error(
            "/pregame/v1/players/abc",
            500,
            format!("oops {JWT}").as_bytes(),
        );
        for d in captures.error_snapshot() {
            assert!(
                !d.contents.contains("opaque-secret") && !d.contents.contains(JWT),
                "{}: {}",
                d.key,
                d.contents
            );
        }
    }

    #[test]
    fn one_error_per_endpoint_is_kept_and_it_is_bounded_to_4_kb() {
        let captures = RawCaptures::new();
        captures.record_error("/mmr/v1/players/one", 500, br#"{"errorCode":"FIRST"}"#);
        captures.record_error("/mmr/v1/players/two", 429, br#"{"errorCode":"SECOND"}"#);
        let dumps = captures.error_snapshot();
        assert_eq!(dumps.len(), 1, "one per endpoint key");
        assert!(dumps[0].contents.contains("SECOND") && dumps[0].contents.contains("429"));

        // A huge body: the stored text is cut; the file says so.
        captures.record_error("/mmr/v1/players/one", 500, &vec![b'x'; 1_000_000]);
        let dumps = captures.error_snapshot();
        assert!(
            dumps[0].contents.len() < MAX_ERROR_CAPTURE_BYTES + 64,
            "{}",
            dumps[0].contents.len()
        );
        assert_eq!(dumps[0].ext, "txt");
        assert!(dumps[0].contents.ends_with("[cut]"));
        // Oversized beyond the capture cap: the marker stands in.
        captures.record_error(
            "/mmr/v1/players/one",
            500,
            &vec![b'x'; MAX_CAPTURE_BYTES + 1],
        );
        assert!(captures.error_snapshot()[0].contents.contains("truncated"));
        // A JSON body just over 4 KB is cut mid-JSON: text, not broken JSON.
        let big = json!({"errorCode": "E", "pad": "y".repeat(6_000)});
        captures.record_error("/mmr/v1/players/one", 500, big.to_string().as_bytes());
        let d = &captures.error_snapshot()[0];
        assert_eq!(d.ext, "txt");
        assert!(d.contents.starts_with("HTTP 500\n"));
    }

    #[test]
    fn errors_and_successes_are_kept_apart() {
        let captures = RawCaptures::new();
        captures.record_error("/mmr/v1/players/abc", 500, br#"{"errorCode":"E"}"#);
        // A later 2xx neither deletes the error nor is affected by it.
        captures.record("/mmr/v1/players/abc", br#"{"ok":true}"#);
        assert_eq!(captures.error_snapshot().len(), 1);
        assert!(captures.snapshot()[0].1.contains("\"ok\": true"));
        // And an error never replaces the 2xx capture.
        captures.record_error("/mmr/v1/players/abc", 404, b"");
        assert!(captures.snapshot()[0].1.contains("\"ok\": true"));
        assert_eq!(captures.snapshot().len(), 1);
    }

    // -- review fix round 1: tokens quoted inside longer strings ------------

    #[test]
    fn a_jwt_ending_a_sentence_is_redacted_and_the_dot_stays() {
        let e = excerpt(format!("token was {JWT}. Retry later.").as_bytes(), 200);
        assert_eq!(e, format!("token was {REDACTED}. Retry later."));
        // Dotted text that is not a token is left alone.
        assert_eq!(
            excerpt(b"see eyJ.a.b for more", 200),
            "see eyJ.a.b for more"
        );
    }

    #[test]
    fn a_jwt_quoted_inside_a_longer_json_string_is_redacted_in_the_excerpt() {
        let body = json!({
            "errorCode": "BAD_CLAIMS",
            "message": format!("invalid bearer {JWT} for player"),
            "list": [format!("a {JWT} b")],
            "nested": {"detail": format!("x{JWT}")},
        });
        let e = excerpt(body.to_string().as_bytes(), 400);
        assert!(!e.contains(JWT), "{e}");
        assert!(!e.contains("eyJ"), "{e}");
        assert!(e.contains("BAD_CLAIMS"), "{e}");
        assert!(
            e.contains("invalid bearer") && e.contains("for player"),
            "{e}"
        );
    }

    #[test]
    fn a_jwt_quoted_inside_a_longer_json_string_is_redacted_in_the_error_dump() {
        let captures = RawCaptures::new();
        let body = json!({
            "errorCode": "BAD_CLAIMS",
            "message": format!("invalid bearer {JWT} for player"),
        });
        captures.record_error("/mmr/v1/players/abc", 400, body.to_string().as_bytes());
        let dump = &captures.error_snapshot()[0];
        assert!(
            !dump.contents.contains(JWT) && !dump.contents.contains("eyJ"),
            "{}",
            dump.contents
        );
        assert!(
            dump.contents.contains("invalid bearer"),
            "{}",
            dump.contents
        );
        assert_eq!(dump.ext, "json", "still JSON");
    }

    #[test]
    fn a_jwt_quoted_inside_a_longer_json_string_is_redacted_in_a_2xx_capture() {
        let captures = RawCaptures::new();
        let body = json!({"Subject": "x", "note": format!("token was {JWT}.")});
        captures.record("/mmr/v1/players/abc", body.to_string().as_bytes());
        let stored = &captures.snapshot()[0].1;
        assert!(!stored.contains(JWT) && !stored.contains("eyJ"), "{stored}");
        assert!(stored.contains("token was"), "{stored}");
    }

    #[test]
    fn an_error_dump_shortens_full_puuids_but_a_2xx_capture_keeps_them() {
        let captures = RawCaptures::new();
        let body = json!({"errorCode": "NOT_FOUND", "message": format!("no player {PUUID}")});
        captures.record_error("/mmr/v1/players/abc", 404, body.to_string().as_bytes());
        captures.record("/mmr/v1/players/abc", body.to_string().as_bytes());
        let dump = &captures.error_snapshot()[0];
        assert!(!dump.contents.contains(PUUID), "{}", dump.contents);
        assert!(
            dump.contents.contains("no player aaaaaaaa"),
            "{}",
            dump.contents
        );
        assert_eq!(dump.ext, "json");
        // A 2xx capture is a fixture: untouched.
        assert!(captures.snapshot()[0].1.contains(PUUID));
    }
}
