//! Integration tests for the pure-Rust parsers: lockfile, shard/region log,
//! and self-presence blob. See `.superpowers/sdd/plan/task-2-brief.md`.

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;

use opd1_core::lockfile::parse_lockfile;
use opd1_core::presence::{decode_presence, Stage};
use opd1_core::shards::parse_shards;

// ---------------------------------------------------------------------
// lockfile
// ---------------------------------------------------------------------

#[test]
fn lockfile_parses_standard_format() {
    let lf = parse_lockfile("Riot Client:1234:54321:pw:https").expect("should parse");
    assert_eq!(lf.port, 54321);
    assert_eq!(lf.password, "pw");
    assert_eq!(lf.protocol, "https");
}

#[test]
fn lockfile_rejects_malformed_input() {
    assert!(parse_lockfile("not-a-lockfile").is_err());
}

#[test]
fn lockfile_rejects_too_few_fields() {
    // Only 4 fields: name:pid:port:password (no protocol).
    assert!(parse_lockfile("Riot Client:1234:54321:pw").is_err());
}

#[test]
fn lockfile_rejects_non_numeric_port() {
    assert!(parse_lockfile("Riot Client:1234:not-a-port:pw:https").is_err());
}

#[test]
fn lockfile_handles_password_containing_colons() {
    // The password itself can contain ':' — name/pid/port come from the
    // front, protocol from the back, and everything in between is the
    // password.
    let lf = parse_lockfile("Riot Client:1234:54321:pw:with:colons:https").expect("should parse");
    assert_eq!(lf.port, 54321);
    assert_eq!(lf.password, "pw:with:colons");
    assert_eq!(lf.protocol, "https");
}

#[test]
fn lockfile_trims_trailing_whitespace_and_newline() {
    let lf = parse_lockfile("Riot Client:1234:54321:pw:https\n").expect("should parse");
    assert_eq!(lf.port, 54321);
    assert_eq!(lf.password, "pw");
    assert_eq!(lf.protocol, "https");
}

// ---------------------------------------------------------------------
// shards
// ---------------------------------------------------------------------

#[test]
fn shards_uses_last_occurrence_when_log_has_eu_then_na() {
    let log = "\
[0001] LogPlatform: Info: https://pd.eu.a.pvp.net/account-xp/v1/players/abc
[0002] LogPlatform: Info: https://glz-eu-1.eu.a.pvp.net/pregame/v1/players/abc
[0100] LogPlatform: Info: https://pd.na.a.pvp.net/account-xp/v1/players/abc
[0101] LogPlatform: Info: https://glz-na-1.na.a.pvp.net/pregame/v1/players/abc
";
    let shards = parse_shards(log).expect("should find shards");
    assert_eq!(shards.pd, "na");
    assert_eq!(shards.glz_region, "na-1");
    assert_eq!(shards.glz_shard, "na");
}

#[test]
fn shards_returns_none_when_pd_url_missing() {
    let log = "[0002] LogPlatform: Info: https://glz-eu-1.eu.a.pvp.net/pregame/v1/players/abc\n";
    assert!(parse_shards(log).is_none());
}

#[test]
fn shards_returns_none_when_glz_url_missing() {
    let log = "[0001] LogPlatform: Info: https://pd.eu.a.pvp.net/account-xp/v1/players/abc\n";
    assert!(parse_shards(log).is_none());
}

#[test]
fn shards_returns_none_for_empty_log() {
    assert!(parse_shards("").is_none());
}

// ---------------------------------------------------------------------
// presence
// ---------------------------------------------------------------------

fn fixture_b64(name: &str) -> String {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    let raw = std::fs::read_to_string(path).expect("fixture should exist");
    BASE64.encode(raw.as_bytes())
}

#[test]
fn presence_decodes_old_flat_shape_to_pregame_with_party_id() {
    let encoded = fixture_b64("presence_old_flat.json");
    let presence = decode_presence(&encoded).expect("should decode");
    assert_eq!(presence.stage, Stage::Pregame);
    assert_eq!(
        presence.party_id.as_deref(),
        Some("d5aa4b34-19b2-4c34-9e2a-000000000001")
    );
    assert_eq!(presence.queue_id.as_deref(), Some("competitive"));
}

#[test]
fn presence_decodes_new_nested_shape_to_pregame_with_party_id() {
    let encoded = fixture_b64("presence_new_nested.json");
    let presence = decode_presence(&encoded).expect("should decode");
    assert_eq!(presence.stage, Stage::Pregame);
    assert_eq!(
        presence.party_id.as_deref(),
        Some("7c11e6a2-88f1-4a77-9e2a-000000000002")
    );
    assert_eq!(presence.queue_id.as_deref(), Some("competitive"));
}

#[test]
fn presence_returns_none_for_invalid_base64() {
    assert!(decode_presence("not-valid-base64!!!").is_none());
}

#[test]
fn presence_returns_none_for_non_object_json() {
    let encoded = BASE64.encode(b"[1, 2, 3]");
    assert!(decode_presence(&encoded).is_none());
}

#[test]
fn presence_maps_missing_session_state_to_menus() {
    let encoded = BASE64.encode(b"{}");
    let presence = decode_presence(&encoded).expect("should decode");
    assert_eq!(presence.stage, Stage::Menus);
    assert_eq!(presence.party_id, None);
    assert_eq!(presence.queue_id, None);
}

#[test]
fn presence_maps_unrecognized_session_state_to_menus() {
    let encoded = BASE64.encode(br#"{"sessionLoopState":"SOMETHING_NEW"}"#);
    let presence = decode_presence(&encoded).expect("should decode");
    assert_eq!(presence.stage, Stage::Menus);
}

#[test]
fn presence_maps_ingame_state() {
    let encoded = BASE64.encode(br#"{"sessionLoopState":"INGAME"}"#);
    let presence = decode_presence(&encoded).expect("should decode");
    assert_eq!(presence.stage, Stage::Ingame);
}

#[test]
fn presence_empty_party_id_and_queue_id_become_none() {
    let encoded = BASE64.encode(br#"{"sessionLoopState":"MENUS","partyId":"","queueId":""}"#);
    let presence = decode_presence(&encoded).expect("should decode");
    assert_eq!(presence.party_id, None);
    assert_eq!(presence.queue_id, None);
}

#[test]
fn presence_falls_back_to_matchpresencedata_queue_id() {
    let encoded = BASE64
        .encode(br#"{"sessionLoopState":"MENUS","matchPresenceData":{"queueId":"deathmatch"}}"#);
    let presence = decode_presence(&encoded).expect("should decode");
    assert_eq!(presence.queue_id.as_deref(), Some("deathmatch"));
}

#[test]
fn stage_serializes_as_lower_camel_case_for_the_ui() {
    assert_eq!(serde_json::to_string(&Stage::Closed).unwrap(), "\"closed\"");
    assert_eq!(serde_json::to_string(&Stage::Menus).unwrap(), "\"menus\"");
    assert_eq!(
        serde_json::to_string(&Stage::Pregame).unwrap(),
        "\"pregame\""
    );
    assert_eq!(serde_json::to_string(&Stage::Ingame).unwrap(), "\"ingame\"");
}
