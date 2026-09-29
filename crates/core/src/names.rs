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

/// A name the user learned in a finished match they played in
/// (`Repo::known_name`), with where and when it was seen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownName {
    pub name: String,
    pub tag: String,
    /// The map of the match that showed the name. It is the stored Riot
    /// path (`/Game/Maps/...`) until [`KnownName::with_map_uuid`] runs, and
    /// a valorant-api uuid (or `None`) after it.
    pub map_id: Option<String>,
    /// When that match started (epoch ms).
    pub start_ms: Option<i64>,
}

impl KnownName {
    /// R84/R95: the one Riot-path -> valorant-api-uuid conversion of a
    /// remembered source. `map_id` holds the stored Riot path
    /// (`/Game/Maps/…`) until this runs, and a uuid or `None` after it; it
    /// never holds a path once converted. Called exactly once, where the
    /// [`DisplayName`] is built: `card_for`, `teammate_name` and
    /// `build_match_view`. No other code converts it.
    pub fn with_map_uuid(self, map_uuid: &dyn Fn(&str) -> Option<String>) -> KnownName {
        KnownName {
            map_id: self.map_id.as_deref().and_then(map_uuid),
            ..self
        }
    }
}

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
/// locked (R99) — CLAUDE.md: never a placeholder agent); `""` means none.
/// `remembered` arrives already converted ([`KnownName::with_map_uuid`]).
pub fn display_name(
    incognito: bool,
    visible: Option<(String, String)>,
    agent_selected: Option<&str>,
    remembered: Option<KnownName>,
) -> DisplayName {
    if !incognito {
        return match visible.and_then(full) {
            Some((name, tag)) => DisplayName::Visible { name, tag },
            None => DisplayName::HiddenPlayer,
        };
    }
    if let Some(known) = remembered.filter(|k| !k.name.is_empty() && !k.tag.is_empty()) {
        return DisplayName::Remembered {
            name: known.name,
            tag: known.tag,
            from_map_id: known.map_id,
            from_start_ms: known.start_ms,
        };
    }
    match agent_selected.filter(|a| !a.is_empty()) {
        Some(agent) => DisplayName::HiddenAgent {
            agent_id: agent.to_string(),
        },
        None => DisplayName::HiddenPlayer,
    }
}
