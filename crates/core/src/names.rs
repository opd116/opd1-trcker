//! CLAUDE.md "Hidden names (incognito)": which name a card shows.
//!
//! A hidden player hides **only** the Riot ID + Tag — every other card
//! field stays visible. The precedence, highest first:
//!
//! 1. not incognito -> `Visible` (their current Riot ID);
//! 2. incognito, but the user already learned the name from a finished
//!    match *the user personally played in* -> `Remembered` (shown in a
//!    distinct style);
//! 3. incognito with an agent selected -> `HiddenAgent` (the agent's
//!    name, no tag);
//! 4. otherwise -> `HiddenPlayer`.
//!
//! This function only decides; it never fetches. The caller must supply
//! `remembered` from `Repo::known_name` (shared matches only) and must
//! never ask name-service for an incognito player (ruling R16) — an
//! incognito player's `visible` name is ignored here even if one is
//! passed, so a stale value can't leak.

use crate::model::DisplayName;

/// `(name, tag)` only when both halves are non-empty.
fn full((name, tag): (String, String)) -> Option<(String, String)> {
    (!name.is_empty() && !tag.is_empty()).then_some((name, tag))
}

/// The name to show for one player (see the module docs for the order).
///
/// `visible` is the player's current Riot ID when known. A visible
/// (non-incognito) player without a full Riot ID has nothing to show and
/// gets `HiddenPlayer` — a caller with a "still loading" state (the card's
/// tri-state `name`, ruling R34) handles that case before calling.
/// `agent_selected` is the agent shown on the card (only after it's
/// selected — CLAUDE.md: never a placeholder agent); `""` means none.
pub fn display_name(
    incognito: bool,
    visible: Option<(String, String)>,
    agent_selected: Option<&str>,
    remembered: Option<(String, String)>,
) -> DisplayName {
    if !incognito {
        return match visible.and_then(full) {
            Some((name, tag)) => DisplayName::Visible { name, tag },
            None => DisplayName::HiddenPlayer,
        };
    }
    if let Some((name, tag)) = remembered.and_then(full) {
        return DisplayName::Remembered { name, tag };
    }
    match agent_selected.filter(|a| !a.is_empty()) {
        Some(agent) => DisplayName::HiddenAgent {
            agent_id: agent.to_string(),
        },
        None => DisplayName::HiddenPlayer,
    }
}
