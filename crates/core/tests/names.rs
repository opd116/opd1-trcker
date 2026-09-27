//! Tests for `opd1_core::names::display_name` — CLAUDE.md's "Hidden names
//! (incognito)" precedence: visible -> remembered -> agent -> hidden.

use opd1_core::model::DisplayName;
use opd1_core::names::display_name;

fn pair(name: &str, tag: &str) -> Option<(String, String)> {
    Some((name.to_string(), tag.to_string()))
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
        display_name(false, pair("Shanky", "OPD1"), None, pair("Old", "TAG")),
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
            pair("Rival", "ENEMY")
        ),
        DisplayName::Remembered {
            name: "Rival".into(),
            tag: "ENEMY".into()
        }
    );
    // …and before agent select too.
    assert_eq!(
        display_name(true, None, None, pair("Rival", "ENEMY")),
        DisplayName::Remembered {
            name: "Rival".into(),
            tag: "ENEMY".into()
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
        display_name(true, None, Some("agent-jett"), pair("", "TAG")),
        DisplayName::HiddenAgent {
            agent_id: "agent-jett".into()
        }
    );
}
