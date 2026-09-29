//! Tests for `opd1_core::names::display_name` — CLAUDE.md's "Hidden names
//! (incognito)" precedence: visible -> remembered -> agent -> hidden.

use opd1_core::model::DisplayName;
use opd1_core::names::{display_name, KnownName};

fn pair(name: &str, tag: &str) -> Option<(String, String)> {
    Some((name.to_string(), tag.to_string()))
}

fn known(name: &str, tag: &str) -> Option<KnownName> {
    Some(KnownName {
        name: name.to_string(),
        tag: tag.to_string(),
        map_id: None,
        start_ms: None,
    })
}

#[test]
fn display_name_visible() {
    assert_eq!(
        display_name(false, pair("Shanky", "OPD1"), Some("agent-1"), None),
        DisplayName::Visible {
            name: "Shanky".into(),
            tag: "OPD1".into()
        }
    );
    // A visible player is never shown in the "remembered" style.
    assert_eq!(
        display_name(false, pair("Shanky", "OPD1"), None, known("Old", "TAG")),
        DisplayName::Visible {
            name: "Shanky".into(),
            tag: "OPD1".into()
        }
    );
}

#[test]
fn display_name_hidden_before_pick() {
    assert_eq!(
        display_name(true, None, None, None),
        DisplayName::HiddenPlayer
    );
    // Even if a caller hands over a real name, an incognito player's
    // visible name is never used.
    assert_eq!(
        display_name(true, pair("Real", "NAME"), None, None),
        DisplayName::HiddenPlayer
    );
    // An empty agent id is "no agent yet".
    assert_eq!(
        display_name(true, None, Some(""), None),
        DisplayName::HiddenPlayer
    );
}

#[test]
fn display_name_hidden_after_pick() {
    assert_eq!(
        display_name(true, pair("Real", "NAME"), Some("agent-jett"), None),
        DisplayName::HiddenAgent {
            agent_id: "agent-jett".into()
        }
    );
}

#[test]
fn display_name_remembered_beats_agent() {
    assert_eq!(
        display_name(
            true,
            pair("Real", "NAME"),
            Some("agent-jett"),
            known("Rival", "ENEMY")
        ),
        DisplayName::Remembered {
            name: "Rival".into(),
            tag: "ENEMY".into(),
            from_map_id: None,
            from_start_ms: None,
        }
    );
    // …and before agent select too.
    assert_eq!(
        display_name(true, None, None, known("Rival", "ENEMY")),
        DisplayName::Remembered {
            name: "Rival".into(),
            tag: "ENEMY".into(),
            from_map_id: None,
            from_start_ms: None,
        }
    );
}

#[test]
fn display_name_ignores_half_known_names() {
    // A visible player without a full Riot ID has nothing to show: the
    // tri-state caller handles "still loading" before asking.
    assert_eq!(
        display_name(false, pair("Shanky", ""), None, None),
        DisplayName::HiddenPlayer
    );
    // A remembered name missing its tag isn't a remembered name.
    assert_eq!(
        display_name(true, None, Some("agent-jett"), known("", "TAG")),
        DisplayName::HiddenAgent {
            agent_id: "agent-jett".into()
        }
    );
}

#[test]
fn display_name_remembered_carries_source() {
    let source = Some(KnownName {
        name: "Rival".into(),
        tag: "ENEMY".into(),
        map_id: Some("uuid-split".into()),
        start_ms: Some(1_700_000_000_000),
    });
    assert_eq!(
        display_name(true, None, Some("agent-jett"), source),
        DisplayName::Remembered {
            name: "Rival".into(),
            tag: "ENEMY".into(),
            from_map_id: Some("uuid-split".into()),
            from_start_ms: Some(1_700_000_000_000),
        }
    );
}

#[test]
fn known_name_with_map_uuid_converts_path() {
    let lookup =
        |path: &str| (path == "/Game/Maps/Bonsai/Bonsai").then(|| "uuid-split".to_string());
    let from = |map_id: Option<&str>| KnownName {
        name: "Rival".into(),
        tag: "ENEMY".into(),
        map_id: map_id.map(str::to_string),
        start_ms: Some(42),
    };

    let converted = from(Some("/Game/Maps/Bonsai/Bonsai")).with_map_uuid(&lookup);
    assert_eq!(converted.map_id.as_deref(), Some("uuid-split"));
    // Everything but the map is untouched.
    assert_eq!(
        (
            converted.name.as_str(),
            converted.tag.as_str(),
            converted.start_ms
        ),
        ("Rival", "ENEMY", Some(42))
    );

    // A path the tables don't know never survives as a path.
    assert_eq!(
        from(Some("/Game/Maps/Unknown/Unknown"))
            .with_map_uuid(&lookup)
            .map_id,
        None
    );
    // No map stays no map.
    assert_eq!(from(None).with_map_uuid(&lookup).map_id, None);
}
