//! Typed wrappers, one per Riot endpoint this app uses, as methods on
//! [`RiotSession`]. Response structs cover only the fields the app needs
//! (see the field list in `.superpowers/sdd/plan/task-3-brief.md` and the
//! endpoint URL table in task-3-report.md); everything else in Riot's
//! payloads is silently ignored by `serde`.
//!
//! Local/pd endpoints are PascalCase (`Subject`, `MatchID`, ...); the
//! `match-details` endpoint is the one exception, using camelCase
//! (`subject`, `matchId`, ...) — each struct below is annotated to match
//! its own endpoint's real casing.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Deserializer};
use serde_json::Value;

use crate::session::{RiotError, RiotSession};

/// Deserializes a field that Riot sometimes sends as an explicit JSON
/// `null` rather than omitting it (per techchrism's `valorant-api-types`
/// zod schemas — e.g. match-details' `teams`/`roundResults`, both
/// `.nullable()` — and Scout's own `(r.get("QueueSkills") or {})`-style
/// guards) into that type's `Default` instead of failing the whole
/// response. `#[serde(default)]` alone only covers a field that's
/// *missing*; this also covers one that's *present but null*.
fn null_to_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

/// R161: a tier Riot's roster answers carry (`CompetitiveTier`) — a number,
/// or nothing. An absent key, a `null` or any other kind of value is "no
/// tier" instead of a decode error, so a surprise in this one field can never
/// blank the whole roster. A value that is present but neither `null` nor an
/// integer is logged once per run (debug), so ranks that turn late again
/// because Riot changed the type leave a trace.
fn lenient_tier<'de, D>(deserializer: D) -> Result<Option<i64>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    let tier = value.as_i64();
    if tier.is_none() && !value.is_null() && !TIER_TYPE_LOGGED.swap(true, Ordering::Relaxed) {
        log::debug!("CompetitiveTier is not an integer ({value}); read as no tier");
    }
    Ok(tier)
}

/// Whether the odd-`CompetitiveTier` line was logged this run.
static TIER_TYPE_LOGGED: AtomicBool = AtomicBool::new(false);

// ---------------------------------------------------------------------
// VisiblePuuid — ruling R16
// ---------------------------------------------------------------------

/// A PUUID known, at construction time, not to belong to an incognito
/// player. [`RiotSession::names`] only accepts these, which makes CLAUDE.md's
/// "never request a hidden player's name from any endpoint" a type-level
/// guarantee instead of a convention a caller could forget to follow.
///
/// The live-stage lookup (Agent Select, In Game) builds one with
/// [`VisiblePuuid::new`] from the roster's real `Incognito` flag, so a hidden
/// player is never asked about during a match. The between-matches names fill
/// (R150, R153) uses [`VisiblePuuid::between_matches`] for players who hid
/// their name *during* a finished match — the game's own Match History shows
/// those names once it is over — and is itself barred from running while a
/// match is on (`LiveModel::fill_visible`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisiblePuuid(String);

impl VisiblePuuid {
    /// Returns `None` when `incognito` is `true` — there is no way to
    /// construct a `VisiblePuuid` for a hidden player.
    pub fn new(puuid: String, incognito: bool) -> Option<Self> {
        debug_assert!(!puuid.is_empty(), "puuid should not be empty");
        if incognito {
            None
        } else {
            Some(Self(puuid))
        }
    }

    /// R167: a PUUID the caller may ask about because **no match is running**
    /// — the names fill's door (R150, R153). It takes no incognito flag: a
    /// player who hid their name during a match that is over is asked on
    /// purpose, and that claim holds only between matches. The type cannot
    /// check the stage, so its one caller (`LiveModel::fill_visible`, in the
    /// shell) refuses to build one while Agent Select, In Game or a
    /// free-for-all match is on; a live-stage lookup must use
    /// [`VisiblePuuid::new`]. `None` for an empty id.
    pub fn between_matches(puuid: String) -> Option<Self> {
        (!puuid.is_empty()).then_some(Self(puuid))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

// ---------------------------------------------------------------------
// Shared PascalCase shapes (pregame / core-game / party)
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct PlayerIdentity {
    #[serde(rename = "Incognito")]
    pub incognito: bool,
    #[serde(rename = "HideAccountLevel")]
    pub hide_account_level: bool,
    #[serde(rename = "AccountLevel")]
    pub account_level: u32,
    #[serde(rename = "PlayerCardID")]
    pub player_card_id: String,
}

/// The shape shared by `Pregame_GetPlayer` and `CoreGame_FetchPlayer`:
/// both just resolve a PUUID to "the match they're currently in".
#[derive(Debug, Clone, Deserialize)]
pub struct MatchRef {
    #[serde(rename = "Subject")]
    pub subject: String,
    #[serde(rename = "MatchID")]
    pub match_id: String,
}

// ---------------------------------------------------------------------
// Pregame (Agent Select)
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct PregamePlayer {
    #[serde(rename = "Subject")]
    pub subject: String,
    #[serde(rename = "CharacterID")]
    pub character_id: String,
    #[serde(rename = "CharacterSelectionState")]
    pub character_selection_state: String,
    #[serde(rename = "PlayerIdentity")]
    pub player_identity: PlayerIdentity,
    /// R161: the player's current competitive tier (`0` = unranked) — real
    /// pregame answers carry it for every player, enemies included. `None`
    /// when absent. The neighbouring `SeasonalBadgeInfo.Rank` is not the
    /// current tier (it is the badge the player picked) and is not read.
    #[serde(rename = "CompetitiveTier", default, deserialize_with = "lenient_tier")]
    pub competitive_tier: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PregameTeam {
    #[serde(rename = "TeamID")]
    pub team_id: String,
    #[serde(rename = "Players")]
    pub players: Vec<PregamePlayer>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PregameMatchResponse {
    #[serde(rename = "MapID")]
    pub map_id: String,
    #[serde(rename = "Teams")]
    pub teams: Vec<PregameTeam>,
    /// The user's own team (the "5 teammates only" the Agent Select
    /// screen shows) — `null` in some payload snapshots.
    #[serde(rename = "AllyTeam")]
    pub ally_team: Option<PregameTeam>,
    #[serde(rename = "EnemyTeam")]
    pub enemy_team: Option<PregameTeam>,
}

/// Skins/sprays for a pregame match. Riot's payload nulls out most of this
/// until characters are locked. Each `Loadouts[]` entry carries `Subject`
/// and `Items` at the top level (no `Loadout` wrapper, unlike core-game);
/// they stay opaque JSON here and `roster::extract_pregame_loadouts` reads
/// the four weapons out.
#[derive(Debug, Clone, Deserialize)]
pub struct PregameLoadoutsResponse {
    #[serde(rename = "Loadouts")]
    pub loadouts: Vec<Value>,
    #[serde(rename = "LoadoutsValid")]
    pub loadouts_valid: bool,
}

// ---------------------------------------------------------------------
// The user's own loadout (personalization)
// ---------------------------------------------------------------------

/// One gun of `personalization/v3/players/{puuid}/playerloadout` (`Guns[]` is
/// the same in v2 and v3; techchrism `PlayerLoadout.ts`): the weapon uuid and
/// what is equipped on it. Charm fields, `SkinLevelID` and `Attachments` are
/// ignored.
#[derive(Debug, Clone, Deserialize)]
pub struct LoadoutGun {
    /// The weapon's uuid (`/v1/weapons/{uuid}`); the knife is in the list too.
    #[serde(rename = "ID")]
    pub id: String,
    /// The equipped skin's uuid (`/v1/weapons/skins/{uuid}`).
    #[serde(rename = "SkinID", default)]
    pub skin_id: String,
    /// The equipped chroma's uuid.
    #[serde(rename = "ChromaID", default)]
    pub chroma_id: String,
}

/// `personalization/v3/players/{puuid}/playerloadout` — "only works for
/// your own PUUID" (techchrism), so the app calls it for the user only
/// (R90). Riot moved it from v2 (which now answers 404 `RESOURCE_NOT_FOUND`
/// on every shard) to v3, where `Sprays[]` became
/// `ActiveExpressions[{TypeID, AssetID}]` and `DynamicOptions` was added.
/// Only `Subject` and `Guns[]` are read; every other key (v2's `Sprays`,
/// v3's `ActiveExpressions` / `DynamicOptions`, `Identity`, `Incognito`) is
/// ignored, so a v2-shaped body still decodes.
#[derive(Debug, Clone, Deserialize)]
pub struct PlayerLoadoutResponse {
    #[serde(rename = "Subject")]
    pub subject: String,
    #[serde(rename = "Guns", default, deserialize_with = "null_to_default")]
    pub guns: Vec<LoadoutGun>,
}

// ---------------------------------------------------------------------
// Core game (In Game)
// ---------------------------------------------------------------------

/// R161: core-game players carry no current tier. Their `SeasonalBadgeInfo.Rank`
/// is the badge the player picked (an earlier act's: 21 where pregame and
/// party said 22 for the same player), so it is not read.
#[derive(Debug, Clone, Deserialize)]
pub struct CoreGamePlayer {
    #[serde(rename = "Subject")]
    pub subject: String,
    #[serde(rename = "TeamID")]
    pub team_id: String,
    #[serde(rename = "CharacterID")]
    pub character_id: String,
    #[serde(rename = "PlayerIdentity")]
    pub player_identity: PlayerIdentity,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CoreGameMatchResponse {
    #[serde(rename = "MapID")]
    pub map_id: String,
    #[serde(rename = "Players")]
    pub players: Vec<CoreGamePlayer>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CoreGameLoadoutEntry {
    #[serde(rename = "CharacterID")]
    pub character_id: String,
    /// Sprays/skins keyed by dynamic UUIDs — kept opaque for the same
    /// reason as [`PregameLoadoutsResponse`].
    #[serde(rename = "Loadout")]
    pub loadout: Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CoreGameLoadoutsResponse {
    #[serde(rename = "Loadouts")]
    pub loadouts: Vec<CoreGameLoadoutEntry>,
}

// ---------------------------------------------------------------------
// MMR / competitive updates
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct SeasonalInfo {
    #[serde(rename = "SeasonID")]
    pub season_id: String,
    #[serde(rename = "CompetitiveTier")]
    pub competitive_tier: i64,
    #[serde(rename = "RankedRating")]
    pub ranked_rating: i64,
    #[serde(rename = "NumberOfWins")]
    pub number_of_wins: i64,
    /// Wins including the placement games, which `NumberOfWins` leaves out
    /// (the act win rate counts them). Absent on older responses and old
    /// stored snapshots, so it is optional: `NumberOfWins` stands in.
    #[serde(rename = "NumberOfWinsWithPlacements", default)]
    pub number_of_wins_with_placements: Option<i64>,
    #[serde(rename = "NumberOfGames")]
    pub number_of_games: i64,
    #[serde(rename = "WinsByTier")]
    pub wins_by_tier: Option<HashMap<String, i64>>,
    /// Not in the brief's field list and not consumed by this crate yet;
    /// kept `Option` (rather than a plain `i64`) so an absent value can't
    /// fail the whole `PlayerMmrResponse` decode.
    #[serde(rename = "LeaderboardRank", default)]
    pub leaderboard_rank: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QueueSkill {
    /// Scout null-guards this exact field (`(... or {}).get("SeasonalInfoBySeasonID")) or {}`
    /// in `live_match.py`) — defaults to empty on a missing *or* explicit
    /// `null` value rather than failing the decode.
    #[serde(
        rename = "SeasonalInfoBySeasonID",
        default,
        deserialize_with = "null_to_default"
    )]
    pub seasonal_info_by_season_id: HashMap<String, SeasonalInfo>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlayerMmrResponse {
    #[serde(rename = "Subject")]
    pub subject: String,
    /// Scout null-guards this exact field (`r.get("QueueSkills") or {}` in
    /// `live_match.py`) — same treatment as
    /// [`QueueSkill::seasonal_info_by_season_id`].
    #[serde(rename = "QueueSkills", default, deserialize_with = "null_to_default")]
    pub queue_skills: HashMap<String, QueueSkill>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CompetitiveUpdateMatch {
    #[serde(rename = "MatchID")]
    pub match_id: String,
    #[serde(rename = "MapID")]
    pub map_id: String,
    #[serde(rename = "SeasonID")]
    pub season_id: String,
    #[serde(rename = "MatchStartTime")]
    pub match_start_time: i64,
    #[serde(rename = "TierAfterUpdate")]
    pub tier_after_update: i64,
    #[serde(rename = "TierBeforeUpdate")]
    pub tier_before_update: i64,
    #[serde(rename = "RankedRatingAfterUpdate")]
    pub ranked_rating_after_update: i64,
    #[serde(rename = "RankedRatingBeforeUpdate")]
    pub ranked_rating_before_update: i64,
    #[serde(rename = "RankedRatingEarned")]
    pub ranked_rating_earned: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CompetitiveUpdatesResponse {
    #[serde(rename = "Subject")]
    pub subject: String,
    #[serde(rename = "Matches")]
    pub matches: Vec<CompetitiveUpdateMatch>,
}

// ---------------------------------------------------------------------
// Match history / match details
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct MatchHistoryEntry {
    #[serde(rename = "MatchID")]
    pub match_id: String,
    #[serde(rename = "GameStartTime")]
    pub game_start_time: i64,
    #[serde(rename = "QueueID")]
    pub queue_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MatchHistoryResponse {
    #[serde(rename = "Subject")]
    pub subject: String,
    #[serde(rename = "History")]
    pub history: Vec<MatchHistoryEntry>,
    /// How many matches the history holds in all (for the requested queue
    /// filter), not just on this page: the fixture shows `"EndIndex": 2,
    /// "Total": 214`. It is how a pager tells the last page from a short one
    /// (clamping or a server-side page cap). Optional so a payload without
    /// it can't fail the whole decode.
    #[serde(rename = "Total", default)]
    pub total: Option<u32>,
}

// match-details is camelCase, unlike every other pd/glz endpoint.

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DamageEntry {
    pub receiver: String,
    pub damage: i64,
    pub legshots: i64,
    pub bodyshots: i64,
    pub headshots: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerRoundStats {
    pub subject: String,
    pub damage: Vec<DamageEntry>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoundResult {
    /// Zero-based round number (`MatchDetails.ts`); rounds 0..=11 are the
    /// first half of a 12-round-half queue. Optional so a payload variant
    /// without it can't fail the whole decode.
    #[serde(default)]
    pub round_num: Option<i64>,
    /// The team id that won the round (`""` on a round nobody won).
    #[serde(default)]
    pub winning_team: Option<String>,
    pub player_stats: Vec<PlayerRoundStats>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchPlayerStats {
    pub score: i64,
    pub rounds_played: i64,
    pub kills: i64,
    pub deaths: i64,
    pub assists: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchPlayer {
    pub subject: String,
    pub game_name: String,
    pub tag_line: String,
    pub team_id: String,
    pub party_id: String,
    pub character_id: String,
    /// `.nullable()` in techchrism's `valorant-api-types` (`MatchDetails.ts`
    /// — a player can apparently have no `stats` at all, e.g. an observer
    /// or a very early surrender), so this must not fail the whole
    /// `MatchDetailsResponse` decode when it's `null`.
    #[serde(default)]
    pub stats: Option<MatchPlayerStats>,
    /// Riot's rank for the player during this match (`MatchDetails.ts`;
    /// `0` when unranked). Optional: absent in older captures.
    #[serde(default)]
    pub competitive_tier: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchTeam {
    pub team_id: String,
    pub won: bool,
    /// Not in Task 3's original field list (which covered only
    /// `teamId`/`won`) — Task 5's `to_core::match_from_details` needs it to
    /// fill `opd1_core::db::repo::MatchTeam.rounds_won`. `#[serde(default)]`
    /// guards against a payload variant that omits it.
    #[serde(default)]
    pub rounds_won: Option<i64>,
}

/// `matchInfo` — fix-round item 3: Task 3 originally left this out
/// ("map/time/queue come from the already-fetched, cheaper
/// `match_history`"), but the controller reversed that: match-details is
/// meant to be self-sufficient, and relying on a caller to thread the
/// right `match_id`/`map_id`/`queue_id`/`start_ms` through from a
/// different call is fragile. `to_core::match_from_details` now builds
/// `Match` from this struct instead of taking those as parameters.
///
/// **Field-name casing is genuinely unsettled between sources**:
/// techchrism's `valorant-api-types` (`MatchDetails.ts`) documents
/// `matchId`/`queueID`/`seasonId`, but the Task 3 fixture
/// (`tests/fixtures/match_details.json`, written against
/// `docs/PVP Endpoints/GET MatchDetails_FetchMatchDetails.md`) uses
/// `matchID`/`queueId`. Since neither source is a live-captured sample,
/// `#[serde(alias = ...)]` accepts both spellings for the two fields that
/// disagree, so this doesn't silently break against a real payload
/// however Riot actually cases it.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchInfo {
    #[serde(alias = "matchID")]
    pub match_id: String,
    pub map_id: String,
    pub game_start_millis: i64,
    #[serde(alias = "queueID")]
    pub queue_id: String,
    pub is_ranked: bool,
    /// `z.enum(['Surrendered', 'Completed', 'VoteDraw', ''])` per
    /// techchrism's types — not in the Task 3 fixture at all, so this
    /// defaults to `""` (itself one of the documented values) rather than
    /// failing the decode.
    #[serde(default)]
    pub completion_state: String,
    /// How long the game ran (`MatchDetails.ts`). Optional: absent in older
    /// captures.
    #[serde(default)]
    pub game_length_millis: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchDetailsResponse {
    // `rename_all = "camelCase"` on this container already turns
    // `match_info` into `matchInfo`.
    pub match_info: MatchInfo,
    pub players: Vec<MatchPlayer>,
    /// `.nullable()` in `MatchDetails.ts` — defaults to empty rather than
    /// failing the decode when Riot sends `null`.
    #[serde(default, deserialize_with = "null_to_default")]
    pub teams: Vec<MatchTeam>,
    /// `.nullable()` in `MatchDetails.ts` — same treatment as `teams`.
    #[serde(default, deserialize_with = "null_to_default")]
    pub round_results: Vec<RoundResult>,
}

// ---------------------------------------------------------------------
// Parties / presences / name-service
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct PartyOfResponse {
    #[serde(rename = "Subject")]
    pub subject: String,
    #[serde(rename = "CurrentPartyID")]
    pub current_party_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PartyMember {
    #[serde(rename = "Subject")]
    pub subject: String,
    #[serde(rename = "PlayerIdentity")]
    pub player_identity: PlayerIdentity,
    #[serde(rename = "IsOwner")]
    pub is_owner: Option<bool>,
    /// Not in the brief's field list and not consumed by this crate yet;
    /// kept `Option` so an absent value can't fail the whole
    /// `PartyResponse` decode.
    #[serde(rename = "IsReady", default)]
    pub is_ready: Option<bool>,
    /// R161: the member's current competitive tier (`0` = unranked); `None`
    /// when absent.
    #[serde(rename = "CompetitiveTier", default, deserialize_with = "lenient_tier")]
    pub competitive_tier: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PartyResponse {
    #[serde(rename = "ID")]
    pub id: String,
    #[serde(rename = "Members")]
    pub members: Vec<PartyMember>,
    /// Not in the brief's field list and not consumed by this crate yet;
    /// kept `Option` so an absent value can't fail the whole
    /// `PartyResponse` decode.
    #[serde(rename = "State", default)]
    pub state: Option<String>,
    /// Not in the brief's field list and not consumed by this crate yet;
    /// `#[serde(default)]` (an empty `Vec`, not `Option`) is the
    /// equivalent "an absent field can't blank the whole response"
    /// guard for a collection.
    #[serde(rename = "EligibleQueues", default)]
    pub eligible_queues: Vec<String>,
}

/// `/chat/v4/presences` fields are already snake_case-ish
/// (`game_name`/`game_tag`/`puuid`/`product`/`private`) in the real API,
/// so no `rename`s are needed for the ones we use.
#[derive(Debug, Clone, Deserialize)]
pub struct Presence {
    pub puuid: String,
    pub product: String,
    pub private: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PresencesResponse {
    pub presences: Vec<Presence>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NameServiceEntry {
    #[serde(rename = "Subject")]
    pub subject: String,
    #[serde(rename = "GameName")]
    pub game_name: String,
    #[serde(rename = "TagLine")]
    pub tag_line: String,
    /// Not in the brief's field list (`game_name`/`tag_line` already give
    /// the Riot ID display) and not consumed by this crate yet; kept
    /// `Option` so an absent value can't fail the whole `names()` decode.
    #[serde(rename = "DisplayName", default)]
    pub display_name: Option<String>,
}

// ---------------------------------------------------------------------
// Query-string helper (comp_updates / match_history)
// ---------------------------------------------------------------------

fn query_string(params: &[(&str, Option<String>)]) -> String {
    let parts: Vec<String> = params
        .iter()
        .filter_map(|(key, value)| value.as_ref().map(|v| format!("{key}={v}")))
        .collect();
    if parts.is_empty() {
        String::new()
    } else {
        format!("?{}", parts.join("&"))
    }
}

// ---------------------------------------------------------------------
// Endpoint wrappers
// ---------------------------------------------------------------------

impl RiotSession {
    pub async fn pregame_player(&self, puuid: &str) -> Result<MatchRef, RiotError> {
        self.glz(&format!("/pregame/v1/players/{puuid}")).await
    }

    pub async fn pregame_match(&self, match_id: &str) -> Result<PregameMatchResponse, RiotError> {
        self.glz(&format!("/pregame/v1/matches/{match_id}")).await
    }

    pub async fn pregame_loadouts(
        &self,
        match_id: &str,
    ) -> Result<PregameLoadoutsResponse, RiotError> {
        self.glz(&format!("/pregame/v1/matches/{match_id}/loadouts"))
            .await
    }

    /// The user's own equipped skins (R90): a read-only pd `GET`, made once
    /// per connect. It takes no puuid: it always asks for the session's own
    /// ([`Self::self_puuid`]), so a caller cannot ask for anyone else's.
    pub async fn player_loadout(&self) -> Result<PlayerLoadoutResponse, RiotError> {
        let puuid = self.self_puuid();
        self.pd(&format!(
            "/personalization/v3/players/{puuid}/playerloadout"
        ))
        .await
    }

    pub async fn coregame_player(&self, puuid: &str) -> Result<MatchRef, RiotError> {
        self.glz(&format!("/core-game/v1/players/{puuid}")).await
    }

    pub async fn coregame_match(&self, match_id: &str) -> Result<CoreGameMatchResponse, RiotError> {
        self.glz(&format!("/core-game/v1/matches/{match_id}")).await
    }

    pub async fn coregame_loadouts(
        &self,
        match_id: &str,
    ) -> Result<CoreGameLoadoutsResponse, RiotError> {
        self.glz(&format!("/core-game/v1/matches/{match_id}/loadouts"))
            .await
    }

    pub async fn mmr(&self, puuid: &str) -> Result<PlayerMmrResponse, RiotError> {
        self.pd(&format!("/mmr/v1/players/{puuid}")).await
    }

    /// Ruling R25: always filters to the `competitive` queue. Without
    /// `queue=competitive`, Riot returns *every* queue's matches mixed
    /// together, and there is no reliable way to filter them back apart
    /// after the fact (queue isn't part of this endpoint's per-match
    /// fields — see [`CompetitiveUpdateMatch`]).
    pub async fn comp_updates(
        &self,
        puuid: &str,
        start: Option<u32>,
        end: Option<u32>,
    ) -> Result<CompetitiveUpdatesResponse, RiotError> {
        let query = query_string(&[
            ("startIndex", start.map(|v| v.to_string())),
            ("endIndex", end.map(|v| v.to_string())),
            ("queue", Some("competitive".to_string())),
        ]);
        self.pd(&format!(
            "/mmr/v1/players/{puuid}/competitiveupdates{query}"
        ))
        .await
    }

    pub async fn match_history(
        &self,
        puuid: &str,
        start: Option<u32>,
        end: Option<u32>,
        queue: Option<&str>,
    ) -> Result<MatchHistoryResponse, RiotError> {
        let query = query_string(&[
            ("startIndex", start.map(|v| v.to_string())),
            ("endIndex", end.map(|v| v.to_string())),
            ("queue", queue.map(str::to_string)),
        ]);
        self.pd(&format!("/match-history/v1/history/{puuid}{query}"))
            .await
    }

    pub async fn match_details(&self, match_id: &str) -> Result<MatchDetailsResponse, RiotError> {
        self.pd(&format!("/match-details/v1/matches/{match_id}"))
            .await
    }

    pub async fn party_of(&self, puuid: &str) -> Result<PartyOfResponse, RiotError> {
        self.glz(&format!("/parties/v1/players/{puuid}")).await
    }

    pub async fn party(&self, party_id: &str) -> Result<PartyResponse, RiotError> {
        self.glz(&format!("/parties/v1/parties/{party_id}")).await
    }

    pub async fn presences(&self) -> Result<PresencesResponse, RiotError> {
        self.local("/chat/v4/presences").await
    }

    /// Looks up display names for a batch of PUUIDs. Every element must be
    /// a [`VisiblePuuid`], so there is no way to pass an incognito
    /// player's PUUID here — see [`VisiblePuuid`] and CLAUDE.md's "never
    /// request a hidden player's name from any endpoint".
    pub async fn names(&self, puuids: &[VisiblePuuid]) -> Result<Vec<NameServiceEntry>, RiotError> {
        let body: Vec<&str> = puuids.iter().map(VisiblePuuid::as_str).collect();
        let body = serde_json::to_value(&body).map_err(|e| RiotError::Decode(e.to_string()))?;
        self.pd_put("/name-service/v2/players", &body).await
    }
}

// ---------------------------------------------------------------------
// content-service (R162)
// ---------------------------------------------------------------------

/// One act as Riot's content-service names it. Times are `None` when Riot's
/// answer lacks them or they do not parse: the id alone still identifies the
/// act (valorant-api, which shares the id space, may know its dates).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentAct {
    pub id: String,
    /// Riot's own `Name`, normalized to valorant-api's style (`ACT 3` ->
    /// `ACT III`). Empty when Riot gave none.
    pub name: String,
    /// The parent episode's `Name` (`EPISODE 9`, `V26`), empty when unknown.
    pub episode_name: String,
    pub start_ms: Option<i64>,
    pub end_ms: Option<i64>,
}

/// The act Riot says is running and the one before it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentActs {
    pub current: ContentAct,
    pub previous: Option<ContentAct>,
}

#[derive(Debug, Clone)]
struct ContentSeason {
    id: String,
    kind: String,
    name: String,
    start_ms: Option<i64>,
    end_ms: Option<i64>,
    is_active: bool,
}

/// Riot's `/content-service/v3/content` `Seasons` list, decoded tolerantly:
/// whatever the shape, building one never fails. An entry that is not an
/// object, or has no id, is dropped; a field of the wrong type is unknown.
/// Only `ID`/`Type`/`Name`/`StartTime`/`EndTime`/`IsActive` are read: the
/// real answer's entries have exactly those six fields (no parent id).
#[derive(Debug, Clone, Default)]
pub struct ContentSeasons {
    seasons: Vec<ContentSeason>,
}

impl ContentSeasons {
    pub fn from_value(value: &Value) -> Self {
        let text = |o: &serde_json::Map<String, Value>, k: &str| {
            o.get(k).and_then(Value::as_str).unwrap_or("").to_string()
        };
        let time = |o: &serde_json::Map<String, Value>, k: &str| {
            o.get(k)
                .and_then(Value::as_str)
                .and_then(crate::to_core::parse_iso8601_utc_ms)
        };
        let seasons = value
            .get("Seasons")
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .filter_map(Value::as_object)
                    .map(|o| ContentSeason {
                        id: text(o, "ID"),
                        kind: text(o, "Type").to_ascii_lowercase(),
                        name: text(o, "Name"),
                        start_ms: time(o, "StartTime"),
                        end_ms: time(o, "EndTime"),
                        is_active: o.get("IsActive").and_then(Value::as_bool) == Some(true),
                    })
                    .filter(|s| !s.id.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        Self { seasons }
    }

    /// The active act (`Type == "act"`, case-insensitive, `IsActive`) and
    /// the act whose `EndTime` equals its `StartTime` (Scout's rule). With
    /// several active acts the latest-starting one wins. `None` when no act
    /// is active.
    pub fn active_acts(&self) -> Option<ContentActs> {
        let acts = || self.seasons.iter().filter(|s| s.kind == "act");
        let current = acts()
            .filter(|s| s.is_active)
            .max_by_key(|s| s.start_ms.unwrap_or(i64::MIN))?;
        let previous = current
            .start_ms
            .and_then(|start| acts().find(|s| s.id != current.id && s.end_ms == Some(start)));
        Some(ContentActs {
            current: self.to_act(current),
            previous: previous.map(|s| self.to_act(s)),
        })
    }

    /// The act's episode is the episode entry that contains its start
    /// (`episode.start <= act.start < episode.end`; the shortest if several),
    /// since the real entries carry no parent id.
    fn to_act(&self, s: &ContentSeason) -> ContentAct {
        let episode_name = s
            .start_ms
            .and_then(|start| {
                self.seasons
                    .iter()
                    .filter(|e| e.kind == "episode")
                    .filter_map(|e| Some((e, e.start_ms?, e.end_ms?)))
                    .filter(|&(_, from, to)| from <= start && start < to)
                    .min_by_key(|&(_, from, to)| to - from)
                    .map(|(e, _, _)| normalize_episode_name(&e.name))
            })
            .unwrap_or_default();
        ContentAct {
            id: s.id.clone(),
            name: normalize_act_name(&s.name),
            episode_name,
            start_ms: s.start_ms,
            end_ms: s.end_ms,
        }
    }
}

/// Riot's `EPISODE 6` (any case) is kept as `EPISODE 6`, the form
/// `opd1_core::stats::act_label` shortens to `E6`; any other episode name
/// (`V26`) is used as given, trimmed.
fn normalize_episode_name(raw: &str) -> String {
    let name = raw.trim();
    match name.get(..8) {
        Some(p) if p.eq_ignore_ascii_case("episode ") => {
            format!("EPISODE {}", name[8..].trim())
        }
        _ => name.to_string(),
    }
}

/// `ACT 3` / `act 3` -> `ACT III` (valorant-api's naming, which
/// `opd1_core::stats::act_label` understands); anything else is trimmed and
/// kept as Riot wrote it.
fn normalize_act_name(raw: &str) -> String {
    let name = raw.trim();
    if let Some(rest) = name
        .get(..3)
        .filter(|p| p.eq_ignore_ascii_case("act"))
        .map(|_| name[3..].trim())
    {
        let roman = match rest {
            "1" => Some("I"),
            "2" => Some("II"),
            "3" => Some("III"),
            "4" => Some("IV"),
            "5" => Some("V"),
            "6" => Some("VI"),
            _ => None,
        };
        if let Some(r) = roman {
            return format!("ACT {r}");
        }
    }
    name.to_string()
}

impl RiotSession {
    /// `GET shared.{shard}.a.pvp.net/content-service/v3/content`: the
    /// season list, from which Riot's own active act is read (R162). Any
    /// 2xx body decodes; only a transport or status failure is an error.
    pub async fn content(&self) -> Result<ContentSeasons, RiotError> {
        let body: Value = self.shared("/content-service/v3/content").await?;
        Ok(ContentSeasons::from_value(&body))
    }
}
