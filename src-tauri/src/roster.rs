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
//! - equipped Vandal/Phantom/Operator/Sheriff skins from core-game
//!   loadouts ([`extract_loadouts`]), pre-game loadouts
//!   ([`extract_pregame_loadouts`], gated by [`want_pregame_loadouts`]) and
//!   the user's own ([`extract_player_loadout`])
//! - Riot map path -> valorant-api map uuid ([`map_uuid`])
//! - the current act ([`current_act`])
//! - free-for-all queues keep the Lobby view (R41, [`view_stage`],
//!   [`is_ffa_roster`])
//! - VALORANT (re)launch under a live Riot Client session ([`GameWatch`], R46)
//! - connect-failure backoff ([`connect_retry_delay`], R47)

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use opd1_core::model::{display_tier, Team};
use opd1_core::presence::Stage;
use opd1_core::stats::Act;
use opd1_riot::endpoints::{
    ContentAct, ContentActs, CoreGameLoadoutsResponse, CoreGameMatchResponse, PartyResponse,
    PlayerIdentity, PlayerLoadoutResponse, PregameLoadoutsResponse, PregameMatchResponse,
    PresencesResponse,
};

use crate::store::PlayerUpdate;

/// The `private` blob (base64) of the user's own VALORANT presence in a
/// `/chat/v4/presences` answer: what the poll loop reads the stage from, and
/// what the names fill re-reads right before a request goes out. `None` while
/// VALORANT shows no presence (or one without a blob).
pub fn own_valorant_private(presences: &PresencesResponse, me: &str) -> Option<String> {
    presences
        .presences
        .iter()
        .find(|p| p.puuid == me && p.product == "valorant")
        .and_then(|p| p.private.clone())
}

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
    /// R161: the player's current ranked tier as the roster answer carried
    /// it (pregame / party `CompetitiveTier`), or one remembered from the
    /// Agent Select of the same match ([`fill_tiers`]). `None` for Unranked
    /// (tier 0, or any value below the first ranked tier) and when Riot sent
    /// no tier. Costs no request; the card shows it only while nothing better
    /// is known (`opd1_core::model::build_card`).
    pub live_tier: Option<u8>,
}

/// R161: Riot's `CompetitiveTier` as a ranked tier, or `None` for Unranked
/// (0), the unused 1 and 2, a missing field or a value no tier table has.
fn ranked_tier(tier: Option<i64>) -> Option<u8> {
    display_tier(tier)
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
            live_tier: None,
        }
    }

    /// R161: the roster answer's `CompetitiveTier` for this player.
    fn with_tier(mut self, competitive_tier: Option<i64>) -> Self {
        self.live_tier = ranked_tier(competitive_tier);
        self
    }

    /// R100: this is the user's own entry, their level is known, and it
    /// differs from the stored row's level (`None` = no row). Such an entry
    /// is saved even though its slot is already on screen (the Closed-stage
    /// slot from startup, or an earlier stage), so the stored level, which
    /// the Closed card and the user's own Player Details read, can't stay
    /// `0` or stale.
    pub fn own_level_stale(&self, me: &str, stored_level: Option<i64>) -> bool {
        self.puuid == me
            && self.account_level > 0
            && stored_level != Some(i64::from(self.account_level))
    }

    /// The `players` row upsert for this roster player; `me` is the user's
    /// PUUID. A level another player hides (`HideAccountLevel`) is never
    /// stored (`0` = unknown; the upsert keeps what the row has). The user's
    /// own level is always stored (R100): the Closed-stage card and the
    /// user's own Player Details read it back from this row.
    pub fn player_update(&self, me: &str) -> PlayerUpdate {
        PlayerUpdate {
            puuid: self.puuid.clone(),
            name: None,
            account_level: if self.hide_account_level && self.puuid != me {
                0
            } else {
                self.account_level
            },
            card_id: self.card_id.clone(),
        }
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
// free-for-all (R41)
// ---------------------------------------------------------------------

/// Presence `queueId`s of free-for-all modes. Checked against every mode
/// Valorant-Scout knows (`vconstants.py` GAMEMODES: competitive, unrated,
/// swiftplay, spikerush, deathmatch, ggteam, hurm, fortcollins,
/// skirmish2v2, custom): Deathmatch is the only one without teams — Team
/// Deathmatch (`hurm`), Escalation (`ggteam`), Retake and Skirmish 2v2 are
/// all team modes and keep the normal In Game view.
pub const FFA_QUEUES: &[&str] = &["deathmatch"];

pub fn is_ffa_queue(queue_id: Option<&str>) -> bool {
    queue_id.is_some_and(|q| FFA_QUEUES.contains(&q))
}

/// The stage the app *shows* for a presence stage (R41): a free-for-all
/// match has no teammates and a dozen strangers, so Agent Select / In Game
/// in an FFA queue keep the Lobby (party) view — `Menus` — and nobody but
/// the party is fetched or synced.
pub fn view_stage(stage: Stage, queue_id: Option<&str>) -> Stage {
    match stage {
        Stage::Pregame | Stage::Ingame if is_ffa_queue(queue_id) => Stage::Menus,
        other => other,
    }
}

/// Fallback when presence carries no usable `queueId`: a core-game roster
/// where no two players share a `TeamID` (Deathmatch gives every player a
/// team of their own) is free-for-all. Two players is still a duel-shaped
/// team game (e.g. a 1v1 custom), not FFA.
pub fn is_ffa_roster(resp: &CoreGameMatchResponse) -> bool {
    if resp.players.len() < 3 {
        return false;
    }
    let teams: HashSet<&str> = resp.players.iter().map(|p| p.team_id.as_str()).collect();
    teams.len() == resp.players.len()
}

// ---------------------------------------------------------------------
// game presence watch (R46) / connect backoff (R47)
// ---------------------------------------------------------------------

/// What one presence tick means for the session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameVerdict {
    /// VALORANT is running and the session is fine.
    Present,
    /// VALORANT is running, but this session was built while it wasn't
    /// (stale `ShooterGame.log` shards, fallback client version):
    /// rebuild the session.
    Reconnect,
    /// No VALORANT presence this tick (maybe transient).
    Absent,
    /// Absent long enough: VALORANT is closed.
    Closed,
}

/// Tracks VALORANT's own presence under one Riot Client session. A session
/// is "stale" when it was created, or has lived, while the game wasn't
/// running — both a first launch and a relaunch (e.g. after a patch) under
/// a Riot Client that stayed open must rebuild it (R46).
#[derive(Debug, Clone, Default)]
pub struct GameWatch {
    absent_ticks: u32,
    /// `None` until the first tick of this session.
    stale: Option<bool>,
}

impl GameWatch {
    /// Consecutive absent presences before VALORANT counts as closed (one
    /// odd payload mustn't flash the Closed stage).
    pub const ABSENT_TICKS_TO_CLOSE: u32 = 2;

    pub fn on_presence(&mut self, game_present: bool) -> GameVerdict {
        if game_present {
            self.absent_ticks = 0;
            if self.stale == Some(true) {
                return GameVerdict::Reconnect;
            }
            self.stale = Some(false);
            return GameVerdict::Present;
        }
        self.absent_ticks += 1;
        if self.stale.is_none() {
            // Connected while the game wasn't up.
            self.stale = Some(true);
        }
        if self.absent_ticks >= Self::ABSENT_TICKS_TO_CLOSE {
            // The game is gone: whenever it comes back, rebuild.
            self.stale = Some(true);
            GameVerdict::Closed
        } else {
            GameVerdict::Absent
        }
    }
}

/// Wait before the next `RiotSession::connect` after `failures`
/// consecutive failures: 5s, 10s, 20s, 40s, then 60s. A stale lockfile or
/// a logged-out Riot Client then costs one attempt a minute, not one every
/// 5s (each attempt re-reads `ShooterGame.log` and calls the local API).
pub fn connect_retry_delay(failures: u32) -> Duration {
    const BASE: u64 = 5;
    const CAP: u64 = 60;
    let exp = failures.saturating_sub(1).min(8);
    Duration::from_secs((BASE << exp).min(CAP))
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
            .with_tier(m.competitive_tier)
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
            .with_tier(p.competitive_tier)
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
            // A missing `TeamID` decodes as "" (R166): that is no team, so
            // only the user themselves is placed by it.
            let same_team = p.subject == self_puuid
                || (!p.team_id.is_empty() && Some(p.team_id.as_str()) == self_team);
            let team = if same_team { Team::Ally } else { Team::Enemy };
            // Core-game answers carry no current tier (R161): `live_tier`
            // stays `None` until `fill_tiers`.
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

/// R161: the current ranked tier of every player a pregame answer lists,
/// both teams: Riot sends `CompetitiveTier` for the enemy team too, which
/// Agent Select does not show but In Game will (core-game answers carry no
/// tier). Unranked players are left out.
pub fn pregame_tiers(resp: &PregameMatchResponse) -> HashMap<String, u8> {
    resp.teams
        .iter()
        .chain(resp.ally_team.iter())
        .chain(resp.enemy_team.iter())
        .flat_map(|t| t.players.iter())
        .filter_map(|p| Some((p.subject.clone(), ranked_tier(p.competitive_tier)?)))
        .collect()
}

/// R161: gives every player of `players` with no tier of their own the one
/// in `known`, if any (the tiers an earlier roster of the same match
/// carried). A tier the roster answer itself carried is never replaced.
pub fn fill_tiers(players: &mut [RosterPlayer], known: &HashMap<String, u8>) {
    for p in players {
        if p.live_tier.is_none() {
            p.live_tier = known.get(&p.puuid).copied();
        }
    }
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
/// [`SKIN_SOCKET`], the skin level `e7c63390-…`).
pub const CHROMA_SOCKET: &str = "3ad1b2b2-acdb-4524-852f-954a76ddae0a";

/// The loadout socket that holds a weapon's equipped *skin* (Scout
/// `backend/valapi.py`): its item id is valorant-api's skin uuid.
pub const SKIN_SOCKET: &str = "bcef87d6-209b-46c6-8b19-fbe40bd95abc";

/// One equipped weapon skin as the loadout endpoints name it: the skin's
/// uuid (`/v1/weapons/skins/{skin_id}`) and the equipped chroma's uuid.
/// Both are stored lower-case.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EquippedSkin {
    pub skin_id: String,
    pub chroma_id: String,
}

/// A player's four equipped skins, before any name is known. The names
/// resolve from valorant-api on demand (`details::resolve_skins`, R83).
/// `None` = not equipped or not readable.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawLoadout {
    pub vandal: Option<EquippedSkin>,
    pub phantom: Option<EquippedSkin>,
    pub operator: Option<EquippedSkin>,
    pub sheriff: Option<EquippedSkin>,
}

impl RawLoadout {
    /// The four slots in the Details order: Vandal, Phantom, Operator,
    /// Sheriff.
    pub fn slots(&self) -> [&Option<EquippedSkin>; 4] {
        [&self.vandal, &self.phantom, &self.operator, &self.sheriff]
    }
}

/// `Sockets[socket].Item.ID` of a core-game / pre-game loadout item,
/// lower-cased; `None` when absent or empty.
fn socket_item(items: &Value, weapon: &str, socket: &str) -> Option<String> {
    let id = items
        .get(weapon)?
        .get("Sockets")?
        .get(socket)?
        .get("Item")?
        .get("ID")?
        .as_str()?;
    (!id.is_empty()).then(|| id.to_ascii_lowercase())
}

/// The equipped skin of `weapon`: its skin and chroma ids (a weapon
/// without both is unknown, not half a skin).
fn equipped(items: &Value, weapon: &str) -> Option<EquippedSkin> {
    Some(EquippedSkin {
        skin_id: socket_item(items, weapon, SKIN_SOCKET)?,
        chroma_id: socket_item(items, weapon, CHROMA_SOCKET)?,
    })
}

fn raw_loadout_of(items: &Value) -> RawLoadout {
    RawLoadout {
        vandal: equipped(items, VANDAL),
        phantom: equipped(items, PHANTOM),
        operator: equipped(items, OPERATOR),
        sheriff: equipped(items, SHERIFF),
    }
}

/// puuid -> the four equipped skins, from `core-game/.../loadouts`.
/// The subject lives inside `Loadout.Subject` (techchrism's current
/// `loadoutsSchema`); an entry without a subject is skipped rather than
/// guessed by position.
pub fn extract_loadouts(resp: &CoreGameLoadoutsResponse) -> HashMap<String, RawLoadout> {
    let mut out = HashMap::new();
    for entry in &resp.loadouts {
        let loadout = &entry.loadout;
        let Some(subject) = loadout.get("Subject").and_then(Value::as_str) else {
            continue;
        };
        let items = loadout.get("Items").unwrap_or(&Value::Null);
        out.insert(subject.to_string(), raw_loadout_of(items));
    }
    out
}

/// puuid -> the four equipped skins, from `pregame/.../loadouts` (Agent
/// Select, allies only). Unlike core-game, each `Loadouts[]` entry carries
/// `Subject` and `Items` at the top level. An entry without a `Subject` is
/// skipped, never matched by position (as [`extract_loadouts`]); and the
/// map is empty unless `LoadoutsValid` — Riot nulls most of the payload
/// until the picks are in.
pub fn extract_pregame_loadouts(resp: &PregameLoadoutsResponse) -> HashMap<String, RawLoadout> {
    let mut out = HashMap::new();
    if !resp.loadouts_valid {
        return out;
    }
    for entry in &resp.loadouts {
        let Some(subject) = entry.get("Subject").and_then(Value::as_str) else {
            continue;
        };
        let items = entry.get("Items").unwrap_or(&Value::Null);
        out.insert(subject.to_string(), raw_loadout_of(items));
    }
    out
}

/// R90: the user's own four skins from the personalization endpoint: the
/// `Guns[]` entry whose `ID` is the weapon (matched ignoring case), with
/// its `SkinID` and `ChromaID` stored lower-case. A weapon that is missing
/// from the response, or has no skin or chroma id, is `None`.
pub fn extract_player_loadout(resp: &PlayerLoadoutResponse) -> RawLoadout {
    let gun = |weapon: &str| {
        let gun = resp
            .guns
            .iter()
            .find(|g| g.id.eq_ignore_ascii_case(weapon))?;
        (!gun.skin_id.is_empty() && !gun.chroma_id.is_empty()).then(|| EquippedSkin {
            skin_id: gun.skin_id.to_ascii_lowercase(),
            chroma_id: gun.chroma_id.to_ascii_lowercase(),
        })
    };
    RawLoadout {
        vandal: gun(VANDAL),
        phantom: gun(PHANTOM),
        operator: gun(OPERATOR),
        sheriff: gun(SHERIFF),
    }
}

/// R83: whether the Agent Select re-poll should ask for the pre-game
/// loadouts: only until Riot answered `LoadoutsValid` once (`seen_valid`),
/// and only while some ally has no [`RawLoadout`] yet. So at most one call
/// per 3s re-poll, none after a valid answer, and none at all when every
/// ally's loadout is already known.
pub fn want_pregame_loadouts(
    allies: &[String],
    have: &HashMap<String, RawLoadout>,
    seen_valid: bool,
) -> bool {
    !seen_valid && allies.iter().any(|puuid| !have.contains_key(puuid))
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

/// How long an act Riot still calls active past its own end time is kept
/// before Riot is asked again (Riot late at a rollover).
const RIOT_LATE_RECHECK_MS: i64 = 10 * 60_000;

/// The act list with Riot's own answer folded in (R162), and when it stops
/// being right.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedActs {
    /// Ascending; the last act is the one Riot calls current, the one before
    /// it the one that ended when it began. Acts valorant-api dates after
    /// the current one are gone, so no caller can take them for current.
    pub acts: Vec<Act>,
    /// Rebuild from this wall-clock time on (the current act's end, or the
    /// next hour while the card clock still reads before its start).
    pub valid_until_ms: i64,
    /// The current act's true start (the list's own entry may carry the
    /// hour-floored card clock for the first hour): what the backfill bound,
    /// My Stats and the hero stats read.
    pub current_start_ms: i64,
}

/// Folds Riot's current/previous act into valorant-api's act table. Riot's
/// id, start and end win; valorant-api only supplies display names (Riot's
/// own `Name` fields stand in for an id it does not know yet). `None` when
/// the current act has no start or end anywhere, so nothing can be said
/// about it: the caller keeps valorant-api's date rule.
pub fn resolve_acts(api: &[Act], riot: &ContentActs, now_ms: i64) -> Option<ResolvedActs> {
    let known = |id: &str| api.iter().find(|a| a.id == id);
    let build = |c: &ContentAct, start: Option<i64>, end: Option<i64>| -> Option<Act> {
        let base = known(&c.id);
        Some(Act {
            id: c.id.clone(),
            name: base.map_or_else(|| c.name.clone(), |a| a.name.clone()),
            episode_name: base.map_or_else(|| c.episode_name.clone(), |a| a.episode_name.clone()),
            start_ms: start.or(base.map(|a| a.start_ms))?,
            end_ms: end.or(base.map(|a| a.end_ms))?,
        })
    };

    let mut current = build(&riot.current, riot.current.start_ms, riot.current.end_ms)?;
    let mut valid_until = current.end_ms;
    if current.end_ms <= now_ms {
        // Riot still says it is active: believe Riot, ask again soon.
        current.end_ms = now_ms + RIOT_LATE_RECHECK_MS;
        valid_until = current.end_ms;
    }
    // Cards are built with the clock floored to the hour: until the act's
    // start is behind that clock, say it began there, and rebuild at the
    // next hour with the true start.
    let card_now = crate::state::card_clock(now_ms);
    let true_start = current.start_ms;
    if true_start > card_now {
        current.start_ms = card_now;
        valid_until = valid_until.min(card_now + 3_600_000);
    }

    let previous = riot.previous.as_ref().and_then(|p| {
        // It ended when the current act began (Riot's own rule).
        build(p, p.start_ms, Some(current.start_ms))
    });
    let before = previous.as_ref().map_or(true_start, |p| p.start_ms);
    let mut acts: Vec<Act> = api
        .iter()
        .filter(|a| a.start_ms < before && a.id != current.id)
        .filter(|a| previous.as_ref().is_none_or(|p| p.id != a.id))
        .cloned()
        .collect();
    acts.extend(previous);
    acts.push(current);
    Some(ResolvedActs {
        acts,
        valid_until_ms: valid_until,
        current_start_ms: true_start,
    })
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
    fn core_game_players_without_a_team_id_are_never_allies_of_each_other() {
        // R166: a missing `TeamID` decodes as "". Two players with no team id
        // are not teammates, and the user is always on their own side.
        let resp: CoreGameMatchResponse = serde_json::from_value(json!({
            "MapID": "/Game/Maps/Ascent/Ascent",
            "Players": [
                {"Subject": "me", "PlayerIdentity": identity(false)},
                {"Subject": "x", "TeamID": null, "PlayerIdentity": identity(false)},
                {"Subject": "y", "TeamID": "Red", "PlayerIdentity": identity(false)}
            ]
        }))
        .unwrap();
        let teams: Vec<(String, Team)> = from_coregame(&resp, "me", &HashSet::new())
            .into_iter()
            .map(|p| (p.puuid, p.team))
            .collect();
        assert_eq!(
            teams,
            [
                ("me".to_string(), Team::Ally),
                ("x".to_string(), Team::Enemy),
                ("y".to_string(), Team::Enemy)
            ]
        );
    }

    fn roster_player(puuid: &str, hide_account_level: bool, account_level: u32) -> RosterPlayer {
        RosterPlayer {
            puuid: puuid.into(),
            team_id: "Blue".into(),
            team: Team::Ally,
            is_self_party: puuid == "me",
            incognito: false,
            hide_account_level,
            account_level,
            card_id: "card-1".into(),
            character_id: String::new(),
            selection_state: String::new(),
            live_tier: None,
        }
    }

    #[test]
    fn player_update_stores_the_users_own_level_even_when_hidden() {
        // R100: the user's own level is always theirs to see, so the roster
        // save keeps it (the Closed stage and the user's own Player Details
        // read it back from the DB).
        let u = roster_player("me", true, 87).player_update("me");
        assert_eq!(u.puuid, "me");
        assert_eq!(u.account_level, 87);
        assert_eq!(u.card_id, "card-1");
        assert_eq!(u.name, None);
        assert_eq!(
            roster_player("me", false, 87)
                .player_update("me")
                .account_level,
            87
        );
    }

    #[test]
    fn own_level_stale_only_for_the_user_with_a_known_different_level() {
        let me = roster_player("me", true, 87);
        assert!(me.own_level_stale("me", None), "no row yet");
        assert!(me.own_level_stale("me", Some(0)), "stored 0 is unknown");
        assert!(me.own_level_stale("me", Some(80)), "levelled up since");
        assert!(!me.own_level_stale("me", Some(87)), "already stored");
        assert!(
            !roster_player("me", true, 0).own_level_stale("me", Some(0)),
            "an unknown level is never saved"
        );
        assert!(
            !roster_player("x", false, 120).own_level_stale("me", None),
            "only the user's own entry is refreshed this way"
        );
    }

    #[test]
    fn player_update_never_stores_another_players_hidden_level() {
        assert_eq!(
            roster_player("x", true, 120)
                .player_update("me")
                .account_level,
            0
        );
        assert_eq!(
            roster_player("x", false, 120)
                .player_update("me")
                .account_level,
            120
        );
        // An unknown level (0) stays 0 for the user too: the DB then keeps
        // whatever it already had.
        assert_eq!(
            roster_player("me", true, 0)
                .player_update("me")
                .account_level,
            0
        );
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

    // -- R161: the live tier --------------------------------------------

    fn tiered(puuid: &str, tier: Value, incognito: bool) -> Value {
        json!({"Subject": puuid, "CharacterID": "", "CharacterSelectionState": "",
               "CompetitiveTier": tier, "PlayerIdentity": identity(incognito)})
    }

    fn pregame_with_tiers() -> PregameMatchResponse {
        serde_json::from_value(json!({
            "MapID": "/Game/Maps/Ascent/Ascent",
            "Teams": [],
            "AllyTeam": {"TeamID": "Blue", "Players": [
                tiered("me", json!(22), false),
                tiered("hid", json!(19), true),
                tiered("new", json!(0), false),
                {"Subject": "gone", "CharacterID": "", "CharacterSelectionState": "",
                 "PlayerIdentity": identity(false)}
            ]},
            "EnemyTeam": {"TeamID": "Red", "Players": [tiered("e1", json!(24), true)]}
        }))
        .unwrap()
    }

    #[test]
    fn pregame_roster_carries_each_allys_tier_and_treats_unranked_as_none() {
        let players = from_pregame(&pregame_with_tiers(), "me", &HashSet::new());
        let tier = |id: &str| players.iter().find(|p| p.puuid == id).unwrap().live_tier;
        assert_eq!(tier("me"), Some(22));
        assert_eq!(tier("hid"), Some(19), "rank is not part of the hidden name");
        assert_eq!(tier("new"), None, "tier 0 is Unranked");
        assert_eq!(tier("gone"), None, "no field, no tier");
    }

    #[test]
    fn pregame_tiers_cover_both_teams_without_the_unranked() {
        let tiers = pregame_tiers(&pregame_with_tiers());
        let mut got: Vec<(&str, u8)> = tiers.iter().map(|(k, v)| (k.as_str(), *v)).collect();
        got.sort_unstable();
        assert_eq!(got, [("e1", 24), ("hid", 19), ("me", 22)]);
        // The `Teams` list alone (no AllyTeam/EnemyTeam) is read too, and
        // a player listed in both is counted once.
        let resp: PregameMatchResponse = serde_json::from_value(json!({
            "MapID": "", "AllyTeam": null, "EnemyTeam": null,
            "Teams": [
                {"TeamID": "Blue", "Players": [tiered("a", json!(15), false)]},
                {"TeamID": "Red", "Players": [tiered("b", json!(17), false)]}
            ]
        }))
        .unwrap();
        assert_eq!(pregame_tiers(&resp).len(), 2);
        assert_eq!(pregame_tiers(&resp)["b"], 17);
    }

    #[test]
    fn party_roster_carries_each_members_tier() {
        let resp: PartyResponse = serde_json::from_value(json!({
            "ID": "p1",
            "Members": [
                {"Subject": "me", "CompetitiveTier": 22, "PlayerIdentity": identity(false)},
                {"Subject": "pal", "CompetitiveTier": 0, "PlayerIdentity": identity(true)},
                {"Subject": "nofield", "PlayerIdentity": identity(false)}
            ]
        }))
        .unwrap();
        let players = from_party(&resp);
        assert_eq!(players[0].live_tier, Some(22));
        assert_eq!(players[1].live_tier, None);
        assert_eq!(players[2].live_tier, None);
    }

    #[test]
    fn coregame_roster_has_no_tier_of_its_own_until_filled_from_the_known_ones() {
        let mut players = from_coregame(&coregame(), "me", &HashSet::new());
        assert!(
            players.iter().all(|p| p.live_tier.is_none()),
            "core-game answers carry no current tier"
        );
        let known: HashMap<String, u8> = [("me".to_string(), 22), ("e1".to_string(), 24)].into();
        fill_tiers(&mut players, &known);
        let tier = |id: &str| players.iter().find(|p| p.puuid == id).unwrap().live_tier;
        assert_eq!(tier("me"), Some(22));
        assert_eq!(tier("e1"), Some(24));
        assert_eq!(tier("a2"), None);

        // A tier the roster answer itself carried is never overwritten.
        let mut own = roster_player("x", false, 1);
        own.live_tier = Some(30);
        let mut one = vec![own];
        fill_tiers(&mut one, &[("x".to_string(), 5)].into());
        assert_eq!(one[0].live_tier, Some(30));
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
    fn incognito_flag_is_carried_verbatim_even_for_self() {
        let players = from_coregame(&coregame(), "a2", &HashSet::new());
        let a2 = players.iter().find(|p| p.puuid == "a2").unwrap();
        assert!(a2.incognito, "no self-exemption at the roster level (R43)");
    }

    #[test]
    fn ffa_queue_keeps_the_lobby_view_in_agent_select_and_in_game() {
        assert_eq!(view_stage(Stage::Ingame, Some("deathmatch")), Stage::Menus);
        assert_eq!(view_stage(Stage::Pregame, Some("deathmatch")), Stage::Menus);
        // Team modes keep their real stage.
        for q in [
            "competitive",
            "unrated",
            "swiftplay",
            "spikerush",
            "hurm",
            "ggteam",
            "skirmish2v2",
            "",
        ] {
            assert_eq!(view_stage(Stage::Ingame, Some(q)), Stage::Ingame, "{q}");
        }
        assert_eq!(view_stage(Stage::Ingame, None), Stage::Ingame);
        assert_eq!(view_stage(Stage::Menus, Some("deathmatch")), Stage::Menus);
        assert_eq!(view_stage(Stage::Closed, Some("deathmatch")), Stage::Closed);
    }

    #[test]
    fn ffa_match_is_not_a_transition_from_the_lobby() {
        // Lobby -> Deathmatch -> Lobby: the view never changes, so no roster
        // fetch, no opponent sync, no post-match follow-up.
        let into = plan_transition(Stage::Menus, view_stage(Stage::Ingame, Some("deathmatch")));
        assert_eq!(into, None);
        // App started mid-Deathmatch: the transition fetches the party.
        let t =
            plan_transition(Stage::Closed, view_stage(Stage::Ingame, Some("deathmatch"))).unwrap();
        assert_eq!(t.fetch, RosterFetch::Party);
        assert!(!t.match_ended);
    }

    #[test]
    fn ffa_roster_is_every_player_on_their_own_team() {
        let mut dm = coregame();
        for p in dm.players.iter_mut() {
            p.team_id = p.subject.clone();
        }
        assert!(is_ffa_roster(&dm));
        assert!(
            !is_ffa_roster(&coregame()),
            "5 players on two teams is a team game"
        );
        dm.players.truncate(2);
        assert!(!is_ffa_roster(&dm), "two players is a duel, not FFA");
    }

    #[test]
    fn game_watch_reconnects_on_first_launch_and_on_relaunch() {
        // Connected while VALORANT was running: fine.
        let mut w = GameWatch::default();
        assert_eq!(w.on_presence(true), GameVerdict::Present);
        // One odd tick doesn't close anything, nor mark the session stale.
        assert_eq!(w.on_presence(false), GameVerdict::Absent);
        assert_eq!(w.on_presence(true), GameVerdict::Present);
        // The game really closes (e.g. to patch) under the same Riot Client…
        assert_eq!(w.on_presence(false), GameVerdict::Absent);
        assert_eq!(w.on_presence(false), GameVerdict::Closed);
        // …and relaunches: the session must be rebuilt (R46).
        assert_eq!(w.on_presence(true), GameVerdict::Reconnect);

        // Connected before the game was up at all.
        let mut w = GameWatch::default();
        assert_eq!(w.on_presence(false), GameVerdict::Absent);
        assert_eq!(w.on_presence(true), GameVerdict::Reconnect);
    }

    #[test]
    fn connect_backoff_doubles_to_a_minute() {
        let secs: Vec<u64> = (1..=7).map(|f| connect_retry_delay(f).as_secs()).collect();
        assert_eq!(secs, [5, 10, 20, 40, 60, 60, 60]);
        assert_eq!(connect_retry_delay(0).as_secs(), 5);
        assert_eq!(connect_retry_delay(u32::MAX).as_secs(), 60);
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

    /// A loadout item as core-game / pre-game send it: the skin in its
    /// socket and the chroma in the chroma socket.
    fn item(skin: &str, chroma: &str) -> Value {
        json!({"ID": "w", "TypeID": "t", "Sockets": {
            SKIN_SOCKET: {"ID": SKIN_SOCKET, "Item": {"ID": skin, "TypeID": "t"}},
            CHROMA_SOCKET: {"ID": CHROMA_SOCKET, "Item": {"ID": chroma, "TypeID": "t"}}
        }})
    }

    fn skin(skin_id: &str, chroma_id: &str) -> Option<EquippedSkin> {
        Some(EquippedSkin {
            skin_id: skin_id.into(),
            chroma_id: chroma_id.into(),
        })
    }

    #[test]
    fn extract_loadouts_reads_skin_and_chroma() {
        let resp: CoreGameLoadoutsResponse = serde_json::from_value(json!({
            "Loadouts": [
                {"CharacterID": "c", "Loadout": {"Subject": "me", "Items": {
                    VANDAL: item("30388628-42F0-606C-82C0-73AD43DE997F", "B2A065C0-4632-CCAA-F496-7681DC2A6185"),
                    SHERIFF: item("skin-sheriff", "chroma-sheriff"),
                    // A weapon with a skin but no chroma socket is unknown.
                    PHANTOM: {"ID": "w", "Sockets": {SKIN_SOCKET: {"Item": {"ID": "skin-phantom"}}}}
                }}},
                {"CharacterID": "c", "Loadout": {"Items": {}}}
            ]
        }))
        .unwrap();
        let loadouts = extract_loadouts(&resp);
        assert_eq!(loadouts.len(), 1, "an entry without a subject is skipped");
        let me = &loadouts["me"];
        assert_eq!(
            me.vandal,
            skin(
                "30388628-42f0-606c-82c0-73ad43de997f",
                "b2a065c0-4632-ccaa-f496-7681dc2a6185"
            ),
            "ids are stored lower-case"
        );
        assert_eq!(me.sheriff, skin("skin-sheriff", "chroma-sheriff"));
        assert_eq!(me.phantom, None, "skin without chroma: not a known skin");
        assert_eq!(me.operator, None);
    }

    fn fixture(name: &str) -> String {
        let path = format!(
            "{}/../crates/riot/tests/fixtures/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("missing fixture {name}: {e}"))
    }

    #[test]
    fn pregame_loadouts_by_subject() {
        // Built from techchrism's real pre-game capture: 3 entries, the
        // last without a `Subject`.
        let resp: PregameLoadoutsResponse =
            serde_json::from_str(&fixture("pregame_loadouts.json")).unwrap();
        assert_eq!(resp.loadouts.len(), 3);
        let loadouts = extract_pregame_loadouts(&resp);
        assert_eq!(
            loadouts.len(),
            2,
            "the entry without a Subject is skipped, not matched by position"
        );
        let first = &loadouts["a6bbe1f1-a9be-457b-9b58-35db705f7499"];
        assert_eq!(
            first.vandal,
            skin(
                "30388628-42f0-606c-82c0-73ad43de997f",
                "b2a065c0-4632-ccaa-f496-7681dc2a6185"
            )
        );
        assert_eq!(
            first.phantom,
            skin(
                "11111111-1111-4111-8111-111111111111",
                "11111111-1111-4111-8111-111111111112"
            )
        );
        assert_eq!(
            first.sheriff,
            skin(
                "33333333-3333-4333-8333-333333333331",
                "33333333-3333-4333-8333-333333333332"
            )
        );
        assert_eq!(first.operator, None, "not in this entry");
        // The second subject has other skins on other weapons.
        let second = &loadouts["b7c1a2d3-4e5f-4a6b-8c7d-9e0f1a2b3c4d"];
        assert_eq!(
            second.vandal,
            skin(
                "44444444-4444-4444-8444-444444444441",
                "44444444-4444-4444-8444-444444444442"
            )
        );
        assert_eq!(
            second.operator,
            skin(
                "55555555-5555-4555-8555-555555555551",
                "55555555-5555-4555-8555-555555555552"
            )
        );
        assert_eq!((&second.phantom, &second.sheriff), (&None, &None));
    }

    #[test]
    fn pregame_loadouts_invalid_is_empty() {
        let mut value: Value = serde_json::from_str(&fixture("pregame_loadouts.json")).unwrap();
        value["LoadoutsValid"] = json!(false);
        let resp: PregameLoadoutsResponse = serde_json::from_value(value).unwrap();
        assert!(
            extract_pregame_loadouts(&resp).is_empty(),
            "Riot nulls the payload until it is valid: nothing is read"
        );
        let resp: PregameLoadoutsResponse =
            serde_json::from_value(json!({"Loadouts": [], "LoadoutsValid": true})).unwrap();
        assert!(extract_pregame_loadouts(&resp).is_empty());
    }

    #[test]
    fn pregame_loadouts_gate() {
        let allies: Vec<String> = ["me", "a", "b"].map(String::from).into();
        let have = |who: &[&str]| -> HashMap<String, RawLoadout> {
            who.iter()
                .map(|p| (p.to_string(), RawLoadout::default()))
                .collect()
        };
        // Nothing known, no valid answer yet: ask.
        assert!(want_pregame_loadouts(&allies, &have(&[]), false));
        // Some ally still missing: ask.
        assert!(want_pregame_loadouts(&allies, &have(&["me", "a"]), false));
        // A valid answer was seen: never again, even with allies missing.
        assert!(!want_pregame_loadouts(&allies, &have(&["me"]), true));
        assert!(!want_pregame_loadouts(&allies, &have(&[]), true));
        // Everyone already has a loadout: nothing to ask for.
        assert!(!want_pregame_loadouts(
            &allies,
            &have(&["me", "a", "b"]),
            false
        ));
        // No allies (an empty roster) has nothing to ask about.
        assert!(!want_pregame_loadouts(&[], &have(&[]), false));
    }

    #[test]
    fn extract_player_loadout_reads_four_guns() {
        // The real capture (techchrism `player-loadout/response.json`) with
        // the first four `ID`s set to the four weapons — the Sheriff's in
        // upper case, to prove matching ignores case.
        let resp: PlayerLoadoutResponse =
            serde_json::from_str(&fixture("player_loadout.json")).unwrap();
        let loadout = extract_player_loadout(&resp);
        let gun = |i: usize| {
            skin(
                &resp.guns[i].skin_id.to_ascii_lowercase(),
                &resp.guns[i].chroma_id.to_ascii_lowercase(),
            )
        };
        assert_eq!(loadout.vandal, gun(0));
        assert_eq!(loadout.phantom, gun(1));
        assert_eq!(loadout.operator, gun(2));
        assert_eq!(loadout.sheriff, gun(3), "the upper-case Sheriff id matches");
        assert_eq!(
            loadout.vandal,
            skin(
                "19af9724-ad73-4d6b-b103-c150ce5f03da",
                "f831fb66-b68e-4dab-acd1-d48a29bf00f4"
            )
        );

        // Ids are stored lower-case even when Riot sends them upper-case,
        // and a weapon that isn't in the response is `None`.
        let resp: PlayerLoadoutResponse = serde_json::from_value(json!({
            "Subject": "me",
            "Guns": [
                {"ID": VANDAL.to_uppercase(), "SkinID": "AAAAAAAA-0000-0000-0000-000000000001",
                 "ChromaID": "BBBBBBBB-0000-0000-0000-000000000001"},
                // A gun without a skin or chroma id is unknown too.
                {"ID": PHANTOM, "SkinID": "", "ChromaID": "cccccccc-0000-0000-0000-000000000001"},
                {"ID": "2f59173c-4bed-b6c3-2191-dea9b58be9c7", "SkinID": "s", "ChromaID": "c"}
            ]
        }))
        .unwrap();
        let loadout = extract_player_loadout(&resp);
        assert_eq!(
            loadout.vandal,
            skin(
                "aaaaaaaa-0000-0000-0000-000000000001",
                "bbbbbbbb-0000-0000-0000-000000000001"
            )
        );
        assert_eq!(
            (&loadout.phantom, &loadout.operator, &loadout.sheriff),
            (&None, &None, &None)
        );
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
    // -- R162: the act Riot says is current ------------------------------

    const H: i64 = 3_600_000;
    const D: i64 = 24 * H;

    fn api_act(id: &str, name: &str, s: i64, e: i64) -> Act {
        Act {
            id: id.into(),
            name: name.into(),
            episode_name: "V26".into(),
            start_ms: s,
            end_ms: e,
        }
    }

    fn riot_act(id: &str, s: Option<i64>, e: Option<i64>) -> ContentAct {
        ContentAct {
            id: id.into(),
            name: format!("riot-{id}"),
            episode_name: "riot-ep".into(),
            start_ms: s,
            end_ms: e,
        }
    }

    /// valorant-api: a(0..100D) b(100D..200D) c(200D..300D).
    fn api() -> Vec<Act> {
        vec![
            api_act("a", "ACT III", 0, 100 * D),
            api_act("b", "ACT IV", 100 * D, 200 * D),
            api_act("c", "ACT V", 200 * D, 300 * D),
        ]
    }

    fn ids(acts: &[Act]) -> Vec<&str> {
        acts.iter().map(|a| a.id.as_str()).collect()
    }

    #[test]
    fn riot_ahead_of_valorant_api_makes_riots_act_current() {
        // Riot rolled to c two hours ago; valorant-api's dates still say b.
        let now = 200 * D + 3 * H;
        let riot_start = now - 3 * H;
        let riot = ContentActs {
            current: riot_act("c", Some(riot_start), Some(300 * D)),
            previous: Some(riot_act("b", Some(100 * D), Some(riot_start))),
        };
        // valorant-api thinks c starts a day later than Riot did.
        let mut a = api();
        a[2].start_ms = now + D;
        let r = resolve_acts(&a, &riot, now).unwrap();
        assert_eq!(ids(&r.acts), ["a", "b", "c"]);
        assert_eq!(current_act(&r.acts, now).unwrap().id, "c");
        assert_eq!(
            r.acts[2].start_ms, riot_start,
            "Riot's start, not the api's"
        );
        assert_eq!(
            r.acts[1].end_ms, riot_start,
            "previous ends when current begins"
        );
        // Names stay valorant-api's.
        assert_eq!(r.acts[2].name, "ACT V");
        assert_eq!(r.acts[2].episode_name, "V26");
    }

    #[test]
    fn valorant_api_ahead_of_riot_does_not_win() {
        // valorant-api already says c (start 200D) but Riot still has b.
        let now = 200 * D + H;
        let riot = ContentActs {
            current: riot_act("b", Some(100 * D), Some(200 * D + 3 * H)),
            previous: Some(riot_act("a", Some(0), Some(100 * D))),
        };
        let r = resolve_acts(&api(), &riot, now).unwrap();
        assert_eq!(ids(&r.acts), ["a", "b"], "the api's later act is dropped");
        assert_eq!(current_act(&r.acts, now).unwrap().id, "b");
        assert_eq!(r.acts[1].end_ms, 200 * D + 3 * H);
        assert_eq!(r.valid_until_ms, 200 * D + 3 * H, "expires with Riot's end");
    }

    #[test]
    fn an_act_valorant_api_does_not_know_uses_riots_names() {
        let now = 250 * D;
        let riot = ContentActs {
            current: ContentAct {
                name: "ACT VI".into(),
                episode_name: "V27".into(),
                ..riot_act("new", Some(240 * D), Some(300 * D))
            },
            previous: Some(riot_act("c", Some(200 * D), Some(240 * D))),
        };
        let r = resolve_acts(&api(), &riot, now).unwrap();
        assert_eq!(ids(&r.acts), ["a", "b", "c", "new"]);
        let new = r.acts.last().unwrap();
        assert_eq!(
            (new.name.as_str(), new.episode_name.as_str()),
            ("ACT VI", "V27")
        );
        // The previous act is the api's own c: its names kept, Riot's end.
        assert_eq!(r.acts[2].name, "ACT V");
        assert_eq!(r.acts[2].end_ms, 240 * D);
        assert_eq!(current_act(&r.acts, now).unwrap().id, "new");
    }

    #[test]
    fn a_previous_act_valorant_api_does_not_know_is_still_second_to_last() {
        let now = 250 * D;
        let riot = ContentActs {
            current: riot_act("new", Some(240 * D), Some(300 * D)),
            previous: Some(riot_act("mystery", Some(220 * D), Some(240 * D))),
        };
        let r = resolve_acts(&api(), &riot, now).unwrap();
        // c (api) starts at 200D, before mystery's 220D, so it stays ahead;
        // the previous act is the one Riot named.
        assert_eq!(ids(&r.acts), ["a", "b", "c", "mystery", "new"]);
        assert_eq!(r.acts[3].name, "riot-mystery");
    }

    #[test]
    fn without_a_riot_previous_the_api_acts_before_it_lead_up() {
        let now = 250 * D;
        let riot = ContentActs {
            current: riot_act("c", Some(200 * D), Some(300 * D)),
            previous: None,
        };
        let r = resolve_acts(&api(), &riot, now).unwrap();
        assert_eq!(ids(&r.acts), ["a", "b", "c"]);
    }

    #[test]
    fn missing_riot_times_fall_back_to_the_apis_for_a_known_id() {
        let now = 250 * D;
        let riot = ContentActs {
            current: riot_act("c", None, None),
            previous: None,
        };
        let r = resolve_acts(&api(), &riot, now).unwrap();
        assert_eq!(r.acts.last().unwrap().start_ms, 200 * D);
        assert_eq!(r.acts.last().unwrap().end_ms, 300 * D);
    }

    #[test]
    fn an_act_with_no_times_anywhere_cannot_be_resolved() {
        let riot = ContentActs {
            current: riot_act("nobody", None, None),
            previous: None,
        };
        assert!(resolve_acts(&api(), &riot, 250 * D).is_none());
        // Nor can one with a start but no end and no api entry.
        let riot = ContentActs {
            current: riot_act("nobody", Some(1), None),
            previous: None,
        };
        assert!(resolve_acts(&api(), &riot, 250 * D).is_none());
    }

    #[test]
    fn riots_act_is_current_for_the_hour_floored_card_clock_too() {
        // Cards are built with the clock floored to the hour: an act that
        // began 10 minutes ago must already be the current one for them.
        let now = 250 * D + 5 * H + 30 * 60_000;
        let start = now - 10 * 60_000;
        let riot = ContentActs {
            current: riot_act("c", Some(start), Some(300 * D)),
            previous: Some(riot_act("b", Some(100 * D), Some(start))),
        };
        let r = resolve_acts(&api(), &riot, now).unwrap();
        let card_now = crate::state::card_clock(now);
        assert_eq!(current_act(&r.acts, card_now).unwrap().id, "c");
        assert_eq!(
            r.valid_until_ms,
            card_now + H,
            "rebuilt at the next hour with the true start"
        );
        assert_eq!(r.current_start_ms, start, "the true start is kept apart");
        let later = resolve_acts(&api(), &riot, card_now + H).unwrap();
        assert_eq!(later.acts.last().unwrap().start_ms, start);
        assert_eq!(later.valid_until_ms, 300 * D);
    }

    #[test]
    fn an_act_riot_still_calls_active_past_its_end_is_extended_briefly() {
        // Riot late: its act's end passed, but it still says IsActive.
        let now = 300 * D + H;
        let riot = ContentActs {
            current: riot_act("c", Some(200 * D), Some(300 * D)),
            previous: Some(riot_act("b", Some(100 * D), Some(200 * D))),
        };
        let r = resolve_acts(&api(), &riot, now).unwrap();
        assert_eq!(current_act(&r.acts, now).unwrap().id, "c");
        assert!(r.valid_until_ms > now && r.valid_until_ms <= now + H);
    }
}
