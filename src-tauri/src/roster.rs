//! Pure decisions for the live loop — everything here is plain data in,
//! plain data out (no Tauri, no DB, no network), so it's unit-tested
//! directly; the loop in `poll.rs` only wires these together.
//!
//! - which roster fetch a stage transition needs ([`plan_transition`])
//! - Riot roster responses -> [`RosterPlayer`]s, with the ally/enemy split
//!   by `TeamID` ([`from_party`], [`from_pregame`], [`from_coregame`])
//! - card/sync order: self -> own party -> other allies -> enemies
//!   ([`order_players`])
//! - the presence "party key" that triggers a lobby party re-fetch
//!   ([`party_key`])
//! - equipped Vandal/Phantom/Operator/Sheriff chromas from core-game
//!   loadouts ([`extract_loadouts`])
//! - Riot map path -> valorant-api map uuid ([`map_uuid`])
//! - the current act ([`current_act`])

use std::collections::{HashMap, HashSet};

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use serde_json::Value;

use opd1_core::model::{Loadout, Team};
use opd1_core::presence::Stage;
use opd1_core::stats::Act;
use opd1_riot::endpoints::{
    CoreGameLoadoutsResponse, CoreGameMatchResponse, PartyResponse, PlayerIdentity,
    PregameMatchResponse,
};

/// One player in the current live roster: everything that arrives with the
/// roster fetch itself (no per-player call needed).
#[derive(Debug, Clone, PartialEq)]
pub struct RosterPlayer {
    pub puuid: String,
    /// Riot's `TeamID` (`"Blue"`/`"Red"`, or the player's own id in
    /// Deathmatch); empty in the lobby.
    pub team_id: String,
    pub team: Team,
    pub is_self_party: bool,
    pub incognito: bool,
    pub hide_account_level: bool,
    pub account_level: u32,
    pub card_id: String,
    pub character_id: String,
    /// `""` | `"selected"` | `"locked"` — see `opd1_core::model::Identity`.
    pub selection_state: String,
}

impl RosterPlayer {
    fn from_identity(
        puuid: &str,
        identity: &PlayerIdentity,
        team_id: &str,
        team: Team,
        is_self_party: bool,
        character_id: &str,
        selection_state: &str,
    ) -> Self {
        Self {
            puuid: puuid.to_string(),
            team_id: team_id.to_string(),
            team,
            is_self_party,
            incognito: identity.incognito,
            hide_account_level: identity.hide_account_level,
            account_level: identity.account_level,
            card_id: identity.player_card_id.clone(),
            character_id: character_id.to_string(),
            selection_state: selection_state.to_string(),
        }
    }

    /// Hidden-name rule as it applies to *this app's user*: the user's own
    /// card always shows their own Riot ID, even with Riot's streamer-mode
    /// "hide my name" on (that setting hides the user from *others*; it
    /// never hides the user from themselves). Everyone else follows their
    /// `Incognito` flag.
    pub fn effectively_incognito(&self, self_puuid: &str) -> bool {
        self.incognito && self.puuid != self_puuid
    }
}

// ---------------------------------------------------------------------
// stage transitions
// ---------------------------------------------------------------------

/// Which roster a stage needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RosterFetch {
    /// Closed: no roster — only the user's own card from the DB.
    None,
    /// Menus (Lobby): the user's party, 1-5 players.
    Party,
    /// Pregame (Agent Select): the ally team only.
    Pregame,
    /// Ingame: both teams + loadouts.
    Ingame,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Transition {
    pub fetch: RosterFetch,
    /// Forget which players were already synced this cycle — their data
    /// may have changed (a match just ended, or the client went away).
    pub reset_synced: bool,
    /// Ingame -> Menus: a match just ended; schedule the post-match
    /// follow-up re-sync of the user's party.
    pub match_ended: bool,
}

/// What a presence stage change requires. `prev == next` is not a
/// transition (returns `None`): the caller then only handles in-stage
/// events (pregame agent re-poll, party changes).
pub fn plan_transition(prev: Stage, next: Stage) -> Option<Transition> {
    if prev == next {
        return None;
    }
    let fetch = match next {
        Stage::Closed => RosterFetch::None,
        Stage::Menus => RosterFetch::Party,
        Stage::Pregame => RosterFetch::Pregame,
        Stage::Ingame => RosterFetch::Ingame,
    };
    Some(Transition {
        fetch,
        // Back in the lobby (or closed), everyone's data may be stale: a
        // match was played or the app lost the client. Pregame -> Ingame
        // keeps the allies synced during Agent Select.
        reset_synced: matches!(next, Stage::Menus | Stage::Closed),
        match_ended: prev == Stage::Ingame && next == Stage::Menus,
    })
}

// ---------------------------------------------------------------------
// roster conversions
// ---------------------------------------------------------------------

/// Lobby: every party member is an ally and in the user's own party.
pub fn from_party(resp: &PartyResponse) -> Vec<RosterPlayer> {
    resp.members
        .iter()
        .map(|m| {
            RosterPlayer::from_identity(
                &m.subject,
                &m.player_identity,
                "",
                Team::Ally,
                true,
                "",
                "",
            )
        })
        .collect()
}

/// Agent Select: the user's own team only (CLAUDE.md: "the user's 5
/// teammates only"). `AllyTeam` when Riot sends it, else whichever team
/// contains the user.
pub fn from_pregame(
    resp: &PregameMatchResponse,
    self_puuid: &str,
    self_party: &HashSet<String>,
) -> Vec<RosterPlayer> {
    let team = resp.ally_team.as_ref().or_else(|| {
        resp.teams
            .iter()
            .find(|t| t.players.iter().any(|p| p.subject == self_puuid))
    });
    let Some(team) = team else {
        return Vec::new();
    };
    team.players
        .iter()
        .map(|p| {
            RosterPlayer::from_identity(
                &p.subject,
                &p.player_identity,
                &team.team_id,
                Team::Ally,
                p.subject == self_puuid || self_party.contains(&p.subject),
                &p.character_id,
                &p.character_selection_state,
            )
        })
        .collect()
}

/// In Game: both teams, ally = the user's own `TeamID`. Core-game players
/// have no `CharacterSelectionState` (every agent is locked by now), so
/// `"locked"` is passed — see `opd1_core::model::Identity`'s doc.
pub fn from_coregame(
    resp: &CoreGameMatchResponse,
    self_puuid: &str,
    self_party: &HashSet<String>,
) -> Vec<RosterPlayer> {
    let self_team = resp
        .players
        .iter()
        .find(|p| p.subject == self_puuid)
        .map(|p| p.team_id.as_str());
    resp.players
        .iter()
        .map(|p| {
            let team = if Some(p.team_id.as_str()) == self_team {
                Team::Ally
            } else {
                Team::Enemy
            };
            RosterPlayer::from_identity(
                &p.subject,
                &p.player_identity,
                &p.team_id,
                team,
                p.subject == self_puuid || self_party.contains(&p.subject),
                &p.character_id,
                "locked",
            )
        })
        .collect()
}

/// Display and sync order: the user first, then the user's party, then
/// the other allies, then enemies — stable (Riot's order) within each
/// group. The sync loop walks this same order ("self -> allies ->
/// enemies"), so the user's own card is always the first to fill in.
pub fn order_players(mut players: Vec<RosterPlayer>, self_puuid: &str) -> Vec<RosterPlayer> {
    players.sort_by_key(|p| {
        if p.puuid == self_puuid {
            0
        } else if p.team == Team::Ally && p.is_self_party {
            1
        } else if p.team == Team::Ally {
            2
        } else {
            3
        }
    });
    players
}

// ---------------------------------------------------------------------
// presence party key
// ---------------------------------------------------------------------

/// `(partyId, partySize)` from the user's own presence `private` blob —
/// when this changes in the lobby, the party is re-fetched (R39). Reads
/// the flat shape first and falls back to `partyPresenceData`, the same
/// two shapes `opd1_core::presence::decode_presence` handles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartyKey {
    pub party_id: Option<String>,
    pub size: Option<u64>,
}

pub fn party_key(private_b64: &str) -> Option<PartyKey> {
    let bytes = BASE64.decode(private_b64).ok()?;
    let value: Value = serde_json::from_slice(&bytes).ok()?;
    let nested = value.get("partyPresenceData");
    let field = |key: &str| -> Option<&Value> {
        value.get(key).or_else(|| nested.and_then(|n| n.get(key)))
    };
    let party_id = field("partyId")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let size = field("partySize").and_then(Value::as_u64);
    Some(PartyKey { party_id, size })
}

// ---------------------------------------------------------------------
// loadouts
// ---------------------------------------------------------------------

/// valorant-api `/v1/weapons` uuids, verified against the live API
/// (2026-09-27): `GET /v1/weapons/{uuid}` -> Vandal / Phantom / Operator /
/// Sheriff.
pub const VANDAL: &str = "9c82e19d-4575-0200-1a81-3eacf00cf872";
pub const PHANTOM: &str = "ee8e8d15-496b-07ac-e5f6-8fae5d4c7b1a";
pub const OPERATOR: &str = "a03b24d3-4319-996d-0f8c-94bbfba1dfc7";
pub const SHERIFF: &str = "e336c6b8-418d-9340-d77f-7a9e4cfe0702";

/// The loadout socket that holds a weapon's equipped skin *chroma*
/// ("Skin Variants" in techchrism's item-type table; the skin itself is
/// socket `bcef87d6-…`, the skin level `e7c63390-…`).
pub const CHROMA_SOCKET: &str = "3ad1b2b2-acdb-4524-852f-954a76ddae0a";

fn chroma_of(items: &Value, weapon: &str) -> Option<String> {
    let id = items
        .get(weapon)?
        .get("Sockets")?
        .get(CHROMA_SOCKET)?
        .get("Item")?
        .get("ID")?
        .as_str()?;
    (!id.is_empty()).then(|| id.to_ascii_lowercase())
}

/// puuid -> the four equipped chromas, from `core-game/.../loadouts`.
/// The subject lives inside `Loadout.Subject` (techchrism's current
/// `loadoutsSchema`); an entry without a subject is skipped rather than
/// guessed by position.
pub fn extract_loadouts(resp: &CoreGameLoadoutsResponse) -> HashMap<String, Loadout> {
    let mut out = HashMap::new();
    for entry in &resp.loadouts {
        let loadout = &entry.loadout;
        let Some(subject) = loadout.get("Subject").and_then(Value::as_str) else {
            continue;
        };
        let items = loadout.get("Items").unwrap_or(&Value::Null);
        out.insert(
            subject.to_string(),
            Loadout {
                vandal: chroma_of(items, VANDAL),
                phantom: chroma_of(items, PHANTOM),
                operator: chroma_of(items, OPERATOR),
                sheriff: chroma_of(items, SHERIFF),
            },
        );
    }
    out
}

// ---------------------------------------------------------------------
// maps / acts
// ---------------------------------------------------------------------

/// Riot `MapID` path (`/Game/Maps/Ascent/Ascent`) -> valorant-api map
/// uuid via `lookup` (`AssetCache::map_uuid_for_path` in the app). Never
/// returns the Riot path itself: an unknown/empty path is `None`, so the
/// UI can never be handed a `/Game/...` string as a map id.
pub fn map_uuid(path: &str, lookup: impl Fn(&str) -> Option<String>) -> Option<String> {
    if path.is_empty() {
        return None;
    }
    lookup(path)
}

/// The act running at `now_ms` — the last act that has started, if `now`
/// is still inside it. `acts` sorted ascending (`to_core::acts_from_seasons`).
pub fn current_act(acts: &[Act], now_ms: i64) -> Option<&Act> {
    acts.iter()
        .rev()
        .find(|a| a.start_ms <= now_ms)
        .filter(|a| now_ms < a.end_ms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn identity(incognito: bool) -> Value {
        json!({
            "Incognito": incognito, "HideAccountLevel": false,
            "AccountLevel": 42, "PlayerCardID": "card-1"
        })
    }

    fn coregame() -> CoreGameMatchResponse {
        serde_json::from_value(json!({
            "MapID": "/Game/Maps/Ascent/Ascent",
            "Players": [
                {"Subject": "e1", "TeamID": "Red", "CharacterID": "a1", "PlayerIdentity": identity(false)},
                {"Subject": "a2", "TeamID": "Blue", "CharacterID": "a2", "PlayerIdentity": identity(true)},
                {"Subject": "me", "TeamID": "Blue", "CharacterID": "a3", "PlayerIdentity": identity(false)},
                {"Subject": "e2", "TeamID": "Red", "CharacterID": "a4", "PlayerIdentity": identity(false)},
                {"Subject": "friend", "TeamID": "Blue", "CharacterID": "a5", "PlayerIdentity": identity(false)}
            ]
        }))
        .unwrap()
    }

    #[test]
    fn transition_to_each_stage_picks_its_roster() {
        let t = plan_transition(Stage::Closed, Stage::Menus).unwrap();
        assert_eq!(t.fetch, RosterFetch::Party);
        assert_eq!(
            plan_transition(Stage::Menus, Stage::Pregame).unwrap().fetch,
            RosterFetch::Pregame
        );
        assert_eq!(
            plan_transition(Stage::Pregame, Stage::Ingame)
                .unwrap()
                .fetch,
            RosterFetch::Ingame
        );
        assert_eq!(
            plan_transition(Stage::Ingame, Stage::Closed).unwrap().fetch,
            RosterFetch::None
        );
    }

    #[test]
    fn same_stage_is_not_a_transition() {
        assert_eq!(plan_transition(Stage::Ingame, Stage::Ingame), None);
        assert_eq!(plan_transition(Stage::Menus, Stage::Menus), None);
    }

    #[test]
    fn match_end_returns_to_lobby_resets_and_flags_post_match() {
        let t = plan_transition(Stage::Ingame, Stage::Menus).unwrap();
        assert_eq!(t.fetch, RosterFetch::Party);
        assert!(t.reset_synced);
        assert!(t.match_ended);
    }

    #[test]
    fn pregame_to_ingame_keeps_synced_allies() {
        let t = plan_transition(Stage::Pregame, Stage::Ingame).unwrap();
        assert!(!t.reset_synced);
        assert!(!t.match_ended);
        // A dodge back to the lobby is not a finished match.
        assert!(
            !plan_transition(Stage::Pregame, Stage::Menus)
                .unwrap()
                .match_ended
        );
    }

    #[test]
    fn coregame_splits_ally_enemy_by_self_team_id() {
        let party: HashSet<String> = ["friend".to_string()].into();
        let players = from_coregame(&coregame(), "me", &party);
        let team_of = |id: &str| players.iter().find(|p| p.puuid == id).unwrap().team;
        assert_eq!(team_of("me"), Team::Ally);
        assert_eq!(team_of("a2"), Team::Ally);
        assert_eq!(team_of("friend"), Team::Ally);
        assert_eq!(team_of("e1"), Team::Enemy);
        assert_eq!(team_of("e2"), Team::Enemy);
        // Core-game has no selection state: every agent counts as locked.
        assert!(players.iter().all(|p| p.selection_state == "locked"));
        let party_flag = |id: &str| {
            players
                .iter()
                .find(|p| p.puuid == id)
                .unwrap()
                .is_self_party
        };
        assert!(party_flag("me") && party_flag("friend"));
        assert!(!party_flag("a2") && !party_flag("e1"));
    }

    #[test]
    fn coregame_without_self_makes_everyone_enemy_not_ally() {
        let players = from_coregame(&coregame(), "not-in-match", &HashSet::new());
        assert!(players.iter().all(|p| p.team == Team::Enemy));
    }

    #[test]
    fn order_is_self_then_party_then_allies_then_enemies() {
        let party: HashSet<String> = ["friend".to_string()].into();
        let ordered = order_players(from_coregame(&coregame(), "me", &party), "me");
        let ids: Vec<&str> = ordered.iter().map(|p| p.puuid.as_str()).collect();
        assert_eq!(ids, ["me", "friend", "a2", "e1", "e2"]);
    }

    #[test]
    fn pregame_uses_ally_team_and_keeps_selection_state() {
        let resp: PregameMatchResponse = serde_json::from_value(json!({
            "MapID": "/Game/Maps/Ascent/Ascent",
            "Teams": [],
            "AllyTeam": {"TeamID": "Blue", "Players": [
                {"Subject": "me", "CharacterID": "", "CharacterSelectionState": "", "PlayerIdentity": identity(false)},
                {"Subject": "x", "CharacterID": "agent", "CharacterSelectionState": "locked", "PlayerIdentity": identity(true)}
            ]},
            "EnemyTeam": null
        }))
        .unwrap();
        let players = from_pregame(&resp, "me", &HashSet::new());
        assert_eq!(players.len(), 2);
        assert!(players.iter().all(|p| p.team == Team::Ally));
        assert_eq!(players[1].selection_state, "locked");
        assert!(players[1].incognito);
        assert!(players[0].is_self_party);
        assert!(!players[1].is_self_party);
    }

    #[test]
    fn pregame_falls_back_to_the_team_containing_self() {
        let resp: PregameMatchResponse = serde_json::from_value(json!({
            "MapID": "", "AllyTeam": null, "EnemyTeam": null,
            "Teams": [
                {"TeamID": "Red", "Players": [
                    {"Subject": "other", "CharacterID": "", "CharacterSelectionState": "", "PlayerIdentity": identity(false)}]},
                {"TeamID": "Blue", "Players": [
                    {"Subject": "me", "CharacterID": "", "CharacterSelectionState": "", "PlayerIdentity": identity(false)}]}
            ]
        }))
        .unwrap();
        let players = from_pregame(&resp, "me", &HashSet::new());
        assert_eq!(players.len(), 1);
        assert_eq!(players[0].team_id, "Blue");
    }

    #[test]
    fn self_is_never_treated_as_incognito() {
        let players = from_coregame(&coregame(), "a2", &HashSet::new());
        let a2 = players.iter().find(|p| p.puuid == "a2").unwrap();
        assert!(a2.incognito);
        assert!(!a2.effectively_incognito("a2"));
        assert!(a2.effectively_incognito("me"));
    }

    #[test]
    fn party_key_reads_flat_and_nested_shapes() {
        let flat = BASE64.encode(r#"{"sessionLoopState":"MENUS","partyId":"p1","partySize":2}"#);
        assert_eq!(
            party_key(&flat),
            Some(PartyKey {
                party_id: Some("p1".into()),
                size: Some(2)
            })
        );
        let nested = BASE64.encode(
            r#"{"matchPresenceData":{"sessionLoopState":"MENUS"},"partyPresenceData":{"partyId":"p2","partySize":3}}"#,
        );
        assert_eq!(
            party_key(&nested),
            Some(PartyKey {
                party_id: Some("p2".into()),
                size: Some(3)
            })
        );
        assert_eq!(party_key("%%%not base64"), None);
    }

    #[test]
    fn loadouts_pick_the_chroma_socket_of_the_four_weapons() {
        let item = |chroma: &str| {
            json!({"ID": "w", "TypeID": "t", "Sockets": {
                "bcef87d6-209b-46c6-8b19-fbe40bd95abc": {"ID": "s", "Item": {"ID": "skin", "TypeID": "t"}},
                CHROMA_SOCKET: {"ID": CHROMA_SOCKET, "Item": {"ID": chroma, "TypeID": "t"}}
            }})
        };
        let resp: CoreGameLoadoutsResponse = serde_json::from_value(json!({
            "Loadouts": [
                {"CharacterID": "c", "Loadout": {"Subject": "me", "Items": {
                    VANDAL: item("19629AE1-4996-AE98-7742-24A240D41F99"),
                    SHERIFF: item("0d3c5ca0-49a2-eef8-d01a-2e9dd8d5d574")
                }}},
                {"CharacterID": "c", "Loadout": {"Items": {}}}
            ]
        }))
        .unwrap();
        let loadouts = extract_loadouts(&resp);
        assert_eq!(loadouts.len(), 1, "an entry without a subject is skipped");
        let me = &loadouts["me"];
        assert_eq!(
            me.vandal.as_deref(),
            Some("19629ae1-4996-ae98-7742-24a240d41f99")
        );
        assert_eq!(
            me.sheriff.as_deref(),
            Some("0d3c5ca0-49a2-eef8-d01a-2e9dd8d5d574")
        );
        assert_eq!(me.phantom, None);
        assert_eq!(me.operator, None);
    }

    #[test]
    fn map_path_converts_to_uuid_and_never_leaks_the_path() {
        let lookup = |p: &str| (p == "/Game/Maps/Ascent/Ascent").then(|| "7eaecc1b".to_string());
        assert_eq!(
            map_uuid("/Game/Maps/Ascent/Ascent", lookup).as_deref(),
            Some("7eaecc1b")
        );
        assert_eq!(map_uuid("/Game/Maps/Unknown/Unknown", lookup), None);
        assert_eq!(map_uuid("", |_| Some("x".into())), None);
    }

    #[test]
    fn current_act_is_the_started_act_containing_now() {
        let act = |id: &str, s: i64, e: i64| Act {
            id: id.into(),
            name: "ACT I".into(),
            episode_name: "V26".into(),
            start_ms: s,
            end_ms: e,
        };
        let acts = vec![
            act("a", 0, 100),
            act("b", 100, 200),
            act("future", 200, 300),
        ];
        assert_eq!(current_act(&acts, 150).unwrap().id, "b");
        assert_eq!(current_act(&acts, 100).unwrap().id, "b");
        assert_eq!(current_act(&acts, 250).unwrap().id, "future");
        assert!(current_act(&acts, 400).is_none());
        assert!(current_act(&[], 1).is_none());
    }
}
