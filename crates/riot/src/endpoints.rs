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

use serde::de::DeserializeOwned;
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

/// R166: a list whose elements are decoded one by one; an element that does not
/// decode (no id, a wrong type) is dropped instead of failing the answer, and a
/// missing, `null` or non-array list is empty. Identity fields (a subject, a
/// match id) stay required on the element type, so "no id" means "no element",
/// never an invented one. Costs one `Value` pass, so it is used for the short
/// lists (rosters, histories, comp updates), not for `roundResults`.
fn lenient_list<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    Ok(list_from_value(Value::deserialize(deserializer)?))
}

/// The elements of a JSON array that decode as `T`; anything else is empty.
pub(crate) fn list_from_value<T: DeserializeOwned>(value: Value) -> Vec<T> {
    let Value::Array(items) = value else {
        return Vec::new();
    };
    let total = items.len();
    let kept: Vec<T> = items
        .into_iter()
        .filter_map(|item| serde_json::from_value(item).ok())
        .collect();
    if kept.len() < total && !ELEMENT_DROP_LOGGED.swap(true, Ordering::Relaxed) {
        log::debug!(
            "dropped {} of {total} list elements that did not decode",
            total - kept.len()
        );
    }
    kept
}

/// Whether the dropped-element line was logged this run.
static ELEMENT_DROP_LOGGED: AtomicBool = AtomicBool::new(false);

/// A list that is empty when missing or `null`, but whose elements must all
/// decode: one that does not fails the whole answer (R166, fix round 1). Used
/// where dropping an element would shift what the rest means: the newest
/// history entry decides party detection, the newest competitive-update row
/// decides the stored rank and whether MMR is asked again, so a row Riot sent
/// half-empty is a failed answer (the existing retry paths ask again), never a
/// shorter list.
fn whole_list<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    Ok(Option::<Vec<T>>::deserialize(deserializer)?.unwrap_or_default())
}

/// R166: an optional object that decodes as `T` or is `None` — a missing,
/// `null` or malformed one is "not there", never a failed answer.
fn lenient_opt<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    Ok(serde_json::from_value(Value::deserialize(deserializer)?).ok())
}

/// R166: a map whose values are decoded one by one; an entry whose value is
/// `null` or malformed is dropped, a missing or `null` map is empty.
fn lenient_map<'de, D, T>(deserializer: D) -> Result<HashMap<String, T>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    let Value::Object(entries) = Value::deserialize(deserializer)? else {
        return Ok(HashMap::new());
    };
    Ok(entries
        .into_iter()
        .filter_map(|(k, v)| Some((k, serde_json::from_value(v).ok()?)))
        .collect())
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

/// A roster entry's `PlayerIdentity`. Decoding never fails and **fails
/// closed** (R166): a flag that did not arrive is the safe reading, so a player
/// whose `Incognito` is missing, `null` or not a bool is hidden (no name is
/// ever asked for them), and a level that did not arrive is not shown
/// (`hide_account_level`, level 0) rather than shown as 0. A missing
/// `HideAccountLevel` next to a level that did arrive is read as "not hidden"
/// (a deliberate choice: the level is shown only when Riot withholds it).
#[derive(Debug, Clone, Deserialize)]
#[serde(from = "RawPlayerIdentity")]
pub struct PlayerIdentity {
    pub incognito: bool,
    pub hide_account_level: bool,
    pub account_level: u32,
    pub player_card_id: String,
}

#[derive(Default, Deserialize)]
struct RawPlayerIdentity {
    #[serde(rename = "Incognito", default)]
    incognito: Value,
    #[serde(rename = "HideAccountLevel", default)]
    hide_account_level: Value,
    #[serde(rename = "AccountLevel", default)]
    account_level: Value,
    #[serde(rename = "PlayerCardID", default)]
    player_card_id: Value,
}

impl From<RawPlayerIdentity> for PlayerIdentity {
    fn from(raw: RawPlayerIdentity) -> Self {
        let level = raw.account_level.as_u64();
        Self {
            incognito: raw.incognito.as_bool().unwrap_or(true),
            // No level that arrived, none shown, whatever the flag says.
            hide_account_level: level.is_none()
                || raw.hide_account_level.as_bool().unwrap_or(false),
            account_level: level.map_or(0, |n| n.min(u64::from(u32::MAX)) as u32),
            player_card_id: raw.player_card_id.as_str().unwrap_or("").to_string(),
        }
    }
}

impl Default for PlayerIdentity {
    /// An identity that did not arrive at all: hidden, no level, no card.
    fn default() -> Self {
        RawPlayerIdentity::default().into()
    }
}

/// The shape shared by `Pregame_GetPlayer` and `CoreGame_FetchPlayer`:
/// both just resolve a PUUID to "the match they're currently in".
#[derive(Debug, Clone, Deserialize)]
pub struct MatchRef {
    #[serde(rename = "Subject", default, deserialize_with = "null_to_default")]
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
    #[serde(rename = "CharacterID", default, deserialize_with = "null_to_default")]
    pub character_id: String,
    #[serde(
        rename = "CharacterSelectionState",
        default,
        deserialize_with = "null_to_default"
    )]
    pub character_selection_state: String,
    #[serde(
        rename = "PlayerIdentity",
        default,
        deserialize_with = "null_to_default"
    )]
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
    #[serde(rename = "TeamID", default, deserialize_with = "null_to_default")]
    pub team_id: String,
    /// A player without a `Subject` is dropped (R166).
    #[serde(rename = "Players", default, deserialize_with = "lenient_list")]
    pub players: Vec<PregamePlayer>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PregameMatchResponse {
    #[serde(rename = "MapID", default, deserialize_with = "null_to_default")]
    pub map_id: String,
    #[serde(rename = "Teams", default, deserialize_with = "lenient_list")]
    pub teams: Vec<PregameTeam>,
    /// The user's own team (the "5 teammates only" the Agent Select
    /// screen shows) — `null` in some payload snapshots; a malformed one
    /// is `None` too (R166).
    #[serde(rename = "AllyTeam", default, deserialize_with = "lenient_opt")]
    pub ally_team: Option<PregameTeam>,
    #[serde(rename = "EnemyTeam", default, deserialize_with = "lenient_opt")]
    pub enemy_team: Option<PregameTeam>,
}

/// Skins/sprays for a pregame match. Riot's payload nulls out most of this
/// until characters are locked. Each `Loadouts[]` entry carries `Subject`
/// and `Items` at the top level (no `Loadout` wrapper, unlike core-game);
/// they stay opaque JSON here and `roster::extract_pregame_loadouts` reads
/// the four weapons out.
#[derive(Debug, Clone, Deserialize)]
pub struct PregameLoadoutsResponse {
    #[serde(rename = "Loadouts", default, deserialize_with = "null_to_default")]
    pub loadouts: Vec<Value>,
    #[serde(
        rename = "LoadoutsValid",
        default,
        deserialize_with = "null_to_default"
    )]
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
    #[serde(rename = "SkinID", default, deserialize_with = "null_to_default")]
    pub skin_id: String,
    /// The equipped chroma's uuid.
    #[serde(rename = "ChromaID", default, deserialize_with = "null_to_default")]
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
    #[serde(rename = "Subject", default, deserialize_with = "null_to_default")]
    pub subject: String,
    /// A gun without an `ID` is dropped (R166).
    #[serde(rename = "Guns", default, deserialize_with = "lenient_list")]
    pub guns: Vec<LoadoutGun>,
}

// ---------------------------------------------------------------------
// Owned items (store entitlements, R189)
// ---------------------------------------------------------------------

/// The store's item type id for weapon skin *levels* (techchrism's
/// valapidocs, "Owned Items": `e7c63390-…` = Skins). Owning a skin means
/// owning its first level; upgrades add the later levels.
pub const SKIN_LEVEL_ITEM_TYPE: &str = "e7c63390-eda7-46e0-bb7a-a6abdacd2433";

/// The most item ids [`OwnedItemsResponse::item_ids`] keeps: far above any
/// real account (there are ~1400 skins in the game), so a runaway answer
/// cannot grow the stored copy without bound.
const MAX_OWNED_ITEMS: usize = 10_000;

/// One owned item.
#[derive(Debug, Clone, Deserialize)]
pub struct Entitlement {
    /// The item's type id (the type asked for); `""` when absent.
    #[serde(rename = "TypeID", default, deserialize_with = "null_to_default")]
    pub type_id: String,
    /// The item itself (for skins: the skin level uuid). Required: an
    /// element without one is dropped (R166).
    #[serde(rename = "ItemID")]
    pub item_id: String,
}

/// One type's group in the all-types answer.
#[derive(Debug, Clone, Deserialize)]
pub struct EntitlementGroup {
    #[serde(rename = "ItemTypeID", default, deserialize_with = "null_to_default")]
    pub item_type_id: String,
    #[serde(rename = "Entitlements", default, deserialize_with = "lenient_list")]
    pub entitlements: Vec<Entitlement>,
}

/// `pd …/store/v1/entitlements/{puuid}/{ItemTypeID}`. The community docs
/// show two shapes: the per-type answer `{ItemTypeID, Entitlements: […]}`
/// (HeyM1ke's ValorantClientAPI capture) and the all-types one
/// `{EntitlementsByTypes: [{ItemTypeID, Entitlements}]}` (techchrism's
/// zod schema). Both decode; [`Self::item_ids`] reads either (R166:
/// lenient lists, `null` is empty).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct OwnedItemsResponse {
    #[serde(rename = "ItemTypeID", default, deserialize_with = "null_to_default")]
    pub item_type_id: String,
    #[serde(rename = "Entitlements", default, deserialize_with = "lenient_list")]
    pub entitlements: Vec<Entitlement>,
    #[serde(
        rename = "EntitlementsByTypes",
        default,
        deserialize_with = "lenient_list"
    )]
    pub by_types: Vec<EntitlementGroup>,
}

impl OwnedItemsResponse {
    /// The owned item ids of `item_type`, from either shape: lower-case,
    /// uuid-shaped only, each once, in answer order, at most
    /// [`MAX_OWNED_ITEMS`]. A group or element that names another type is
    /// skipped; one that names none is taken as the asked type (the per-type
    /// answer omits it).
    pub fn item_ids(&self, item_type: &str) -> Vec<String> {
        let same = |t: &str| t.is_empty() || t.eq_ignore_ascii_case(item_type);
        let top = same(&self.item_type_id).then_some(&self.entitlements);
        let groups = self
            .by_types
            .iter()
            .filter(|g| same(&g.item_type_id))
            .map(|g| &g.entitlements);
        let mut seen = std::collections::HashSet::new();
        top.into_iter()
            .chain(groups)
            .flatten()
            .filter(|e| same(&e.type_id))
            .map(|e| e.item_id.trim().to_ascii_lowercase())
            .filter(|id| is_uuid(id) && seen.insert(id.clone()))
            .take(MAX_OWNED_ITEMS)
            .collect()
    }
}

/// `8-4-4-4-12` hex digits.
fn is_uuid(s: &str) -> bool {
    s.len() == 36
        && s.char_indices().all(|(i, c)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                c == '-'
            } else {
                c.is_ascii_hexdigit()
            }
        })
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
    /// `""` when Riot sent none: `roster::from_coregame` never lets that
    /// pass for a team (R166).
    #[serde(rename = "TeamID", default, deserialize_with = "null_to_default")]
    pub team_id: String,
    #[serde(rename = "CharacterID", default, deserialize_with = "null_to_default")]
    pub character_id: String,
    #[serde(
        rename = "PlayerIdentity",
        default,
        deserialize_with = "null_to_default"
    )]
    pub player_identity: PlayerIdentity,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CoreGameMatchResponse {
    #[serde(rename = "MapID", default, deserialize_with = "null_to_default")]
    pub map_id: String,
    /// A player without a `Subject` is dropped (R166).
    #[serde(rename = "Players", default, deserialize_with = "lenient_list")]
    pub players: Vec<CoreGamePlayer>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CoreGameLoadoutEntry {
    #[serde(rename = "CharacterID", default, deserialize_with = "null_to_default")]
    pub character_id: String,
    /// Sprays/skins keyed by dynamic UUIDs — kept opaque for the same
    /// reason as [`PregameLoadoutsResponse`].
    #[serde(rename = "Loadout", default)]
    pub loadout: Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CoreGameLoadoutsResponse {
    #[serde(rename = "Loadouts", default, deserialize_with = "lenient_list")]
    pub loadouts: Vec<CoreGameLoadoutEntry>,
}

// ---------------------------------------------------------------------
// MMR / competitive updates
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct SeasonalInfo {
    /// Not read: the map key of `SeasonalInfoBySeasonID` is the act id
    /// ([`crate::to_core::seasonal_map_from_mmr`]), so an entry whose own
    /// `SeasonID` is missing is still filed under the right act (R166).
    #[serde(rename = "SeasonID", default, deserialize_with = "null_to_default")]
    pub season_id: String,
    /// `0` (unranked) when absent: no rank is shown from such an entry.
    #[serde(
        rename = "CompetitiveTier",
        default,
        deserialize_with = "null_to_default"
    )]
    pub competitive_tier: i64,
    /// `None` when absent: a rank with an invented "0 RR" is wrong data, so
    /// [`crate::to_core::current_rank_from_mmr`] gives no rank from this entry
    /// and the stored competitive updates answer instead (R154, R166).
    #[serde(rename = "RankedRating", default)]
    pub ranked_rating: Option<i64>,
    /// `None` when absent. A record with an unknown wins or games count is
    /// unknown as a whole ([`crate::to_core::seasonal_map_from_mmr`] gives it
    /// 0 games, which the card shows as `—`), never "0 wins of N games" (R166,
    /// fix round 1).
    #[serde(rename = "NumberOfWins", default)]
    pub number_of_wins: Option<i64>,
    /// Wins including the placement games, which `NumberOfWins` leaves out
    /// (the act win rate counts them). Absent on older responses and old
    /// stored snapshots, so it is optional: `NumberOfWins` stands in.
    #[serde(rename = "NumberOfWinsWithPlacements", default)]
    pub number_of_wins_with_placements: Option<i64>,
    #[serde(rename = "NumberOfGames", default)]
    pub number_of_games: Option<i64>,
    #[serde(rename = "WinsByTier", default)]
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
        deserialize_with = "lenient_map"
    )]
    pub seasonal_info_by_season_id: HashMap<String, SeasonalInfo>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlayerMmrResponse {
    #[serde(rename = "Subject", default, deserialize_with = "null_to_default")]
    pub subject: String,
    /// Scout null-guards this exact field (`r.get("QueueSkills") or {}` in
    /// `live_match.py`) — same treatment as
    /// [`QueueSkill::seasonal_info_by_season_id`].
    #[serde(rename = "QueueSkills", default, deserialize_with = "lenient_map")]
    pub queue_skills: HashMap<String, QueueSkill>,
}

/// One row of `competitiveupdates`. What makes it a row (`MatchID`,
/// `MatchStartTime`, the tier and RR after, the RR earned) is required: a row
/// lacking one fails the whole answer ([`whole_list`]), because a default 0
/// would be a 1970 match, an unranked tier or "+0 RR" in the charts, and a
/// dropped row would shift "newest". The before-values, map and season are
/// optional.
#[derive(Debug, Clone, Deserialize)]
pub struct CompetitiveUpdateMatch {
    #[serde(rename = "MatchID")]
    pub match_id: String,
    #[serde(rename = "MapID", default, deserialize_with = "null_to_default")]
    pub map_id: String,
    /// `""` when absent: never equal to an act id, so such a row counts for no
    /// act (as the real `SeasonID: ""` rows already do).
    #[serde(rename = "SeasonID", default, deserialize_with = "null_to_default")]
    pub season_id: String,
    #[serde(rename = "MatchStartTime")]
    pub match_start_time: i64,
    #[serde(rename = "TierAfterUpdate")]
    pub tier_after_update: i64,
    #[serde(rename = "TierBeforeUpdate", default)]
    pub tier_before_update: Option<i64>,
    #[serde(rename = "RankedRatingAfterUpdate")]
    pub ranked_rating_after_update: i64,
    #[serde(rename = "RankedRatingBeforeUpdate", default)]
    pub ranked_rating_before_update: Option<i64>,
    #[serde(rename = "RankedRatingEarned")]
    pub ranked_rating_earned: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CompetitiveUpdatesResponse {
    #[serde(rename = "Subject", default, deserialize_with = "null_to_default")]
    pub subject: String,
    #[serde(rename = "Matches", default, deserialize_with = "whole_list")]
    pub matches: Vec<CompetitiveUpdateMatch>,
}

// ---------------------------------------------------------------------
// Match history / match details
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct MatchHistoryEntry {
    #[serde(rename = "MatchID")]
    pub match_id: String,
    /// Required, like the id: an entry without a start time would be dated
    /// 1970 and fall outside every act, and a dropped entry would shift "most
    /// recent", so the whole answer fails ([`whole_list`], R166).
    #[serde(rename = "GameStartTime")]
    pub game_start_time: i64,
    /// `""` when absent: not a competitive match and not Deathmatch, so it
    /// counts for neither.
    #[serde(rename = "QueueID", default, deserialize_with = "null_to_default")]
    pub queue_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MatchHistoryResponse {
    #[serde(rename = "Subject", default, deserialize_with = "null_to_default")]
    pub subject: String,
    #[serde(rename = "History", default, deserialize_with = "whole_list")]
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
    #[serde(default, deserialize_with = "null_to_default")]
    pub receiver: String,
    #[serde(default, deserialize_with = "null_to_default")]
    pub damage: i64,
    #[serde(default, deserialize_with = "null_to_default")]
    pub legshots: i64,
    #[serde(default, deserialize_with = "null_to_default")]
    pub bodyshots: i64,
    #[serde(default, deserialize_with = "null_to_default")]
    pub headshots: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerRoundStats {
    /// `""` when absent: matches no player, so the entry counts for nobody
    /// (R166). Not a [`lenient_list`] element on purpose: `roundResults` is the
    /// heavy part of a match-details answer and is decoded straight.
    #[serde(default, deserialize_with = "null_to_default")]
    pub subject: String,
    #[serde(default, deserialize_with = "null_to_default")]
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
    #[serde(default, deserialize_with = "null_to_default")]
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
    /// Required (R166, fix round 1: a player without one fails the answer).
    pub subject: String,
    #[serde(default, deserialize_with = "null_to_default")]
    pub game_name: String,
    #[serde(default, deserialize_with = "null_to_default")]
    pub tag_line: String,
    /// Required: it is stored, and party detection, the encounter counts and the
    /// match result all compare it, so a `""` would make strangers teammates. A
    /// player without one fails the answer and the match is fetched again later
    /// (R166, fix round 1).
    pub team_id: String,
    #[serde(default, deserialize_with = "null_to_default")]
    pub party_id: String,
    #[serde(default, deserialize_with = "null_to_default")]
    pub character_id: String,
    /// `.nullable()` in techchrism's `valorant-api-types` (`MatchDetails.ts`
    /// — a player can apparently have no `stats` at all, e.g. an observer
    /// or a very early surrender), so this must not fail the whole
    /// `MatchDetailsResponse` decode when it's `null`. A block with a field
    /// missing is `None` too (R166): a stored "0 kills" would stay for good.
    #[serde(default, deserialize_with = "lenient_opt")]
    pub stats: Option<MatchPlayerStats>,
    /// Riot's rank for the player during this match (`MatchDetails.ts`;
    /// `0` when unranked). Optional: absent in older captures.
    #[serde(default)]
    pub competitive_tier: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchTeam {
    /// Required, like `won`: a team lacking either fails the answer, so no
    /// winner is lost or invented (R166, fix round 1).
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
    #[serde(default, deserialize_with = "null_to_default")]
    pub map_id: String,
    /// Required (with the match id and the queue): a match is stored for good,
    /// so one without a start or a queue is not stored at all (R166).
    pub game_start_millis: i64,
    #[serde(alias = "queueID")]
    pub queue_id: String,
    pub is_ranked: bool,
    /// `z.enum(['Surrendered', 'Completed', 'VoteDraw', ''])` per
    /// techchrism's types — not in the Task 3 fixture at all, so this
    /// defaults to `""` (itself one of the documented values) rather than
    /// failing the decode.
    #[serde(default, deserialize_with = "null_to_default")]
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
    /// Required, and every player must decode (`subject`, `teamId`): a match
    /// is stored for good, so a missing list or a half-empty player fails the
    /// answer and the fetch is retried later (R166, fix round 1).
    pub players: Vec<MatchPlayer>,
    /// `.nullable()` in `MatchDetails.ts` — defaults to empty rather than
    /// failing the decode when Riot sends `null`.
    #[serde(default, deserialize_with = "whole_list")]
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
    #[serde(rename = "Subject", default, deserialize_with = "null_to_default")]
    pub subject: String,
    #[serde(rename = "CurrentPartyID")]
    pub current_party_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PartyMember {
    #[serde(rename = "Subject")]
    pub subject: String,
    #[serde(
        rename = "PlayerIdentity",
        default,
        deserialize_with = "null_to_default"
    )]
    pub player_identity: PlayerIdentity,
    #[serde(rename = "IsOwner", default)]
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
    #[serde(rename = "ID", default, deserialize_with = "null_to_default")]
    pub id: String,
    /// A member without a `Subject` is dropped (R166).
    #[serde(rename = "Members", default, deserialize_with = "lenient_list")]
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
    #[serde(
        rename = "EligibleQueues",
        default,
        deserialize_with = "null_to_default"
    )]
    pub eligible_queues: Vec<String>,
}

/// `/chat/v4/presences` fields are already snake_case-ish
/// (`game_name`/`game_tag`/`puuid`/`product`/`private`) in the real API,
/// so no `rename`s are needed for the ones we use.
#[derive(Debug, Clone, Deserialize)]
pub struct Presence {
    /// Required: a presence without one is dropped by the list (R166).
    pub puuid: String,
    #[serde(default, deserialize_with = "null_to_default")]
    pub product: String,
    #[serde(default)]
    pub private: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PresencesResponse {
    #[serde(default, deserialize_with = "lenient_list")]
    pub presences: Vec<Presence>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NameServiceEntry {
    #[serde(rename = "Subject")]
    pub subject: String,
    /// `""` when absent: Riot already answers `""` for a player who hides
    /// their name, and every consumer treats it as "no name" (R166).
    #[serde(rename = "GameName", default, deserialize_with = "null_to_default")]
    pub game_name: String,
    #[serde(rename = "TagLine", default, deserialize_with = "null_to_default")]
    pub tag_line: String,
    /// Not in the brief's field list (`game_name`/`tag_line` already give
    /// the Riot ID display) and not consumed by this crate yet; kept
    /// `Option` so an absent value can't fail the whole `names()` decode.
    #[serde(rename = "DisplayName", default)]
    pub display_name: Option<String>,
}

/// Queues with 12-round halves, whose answers always list their teams.
const WINNER_QUEUES: [&str; 3] = ["competitive", "unrated", "premier"];

/// A competitive, unrated or premier answer whose `teams` is missing or empty,
/// or names more than one winner, is not trustworthy and is not stored. Both
/// teams `won: false` is a draw (R29) and is accepted, and so is a `VoteDraw`
/// whatever its teams say. Other queues (Deathmatch has no teams) are not
/// checked.
fn check_winner(details: &MatchDetailsResponse) -> Result<(), RiotError> {
    let info = &details.match_info;
    if WINNER_QUEUES.contains(&info.queue_id.as_str())
        && info.completion_state != "VoteDraw"
        && (details.teams.is_empty() || details.teams.iter().filter(|t| t.won).count() > 1)
    {
        return Err(RiotError::Decode(
            "the match lists no teams or more than one winning team".into(),
        ));
    }
    Ok(())
}

/// How long [`RiotSession::names`] waits before its one retry (R166).
const NAMES_RETRY_PAUSE: std::time::Duration = std::time::Duration::from_millis(250);

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

    /// R189: the user's owned weapon skin levels — a read-only pd `GET`
    /// (`other` throttle family), always for the session's own puuid (the
    /// method takes none, like [`Self::player_loadout`]).
    pub async fn owned_skin_levels(&self) -> Result<OwnedItemsResponse, RiotError> {
        let puuid = self.self_puuid();
        self.pd(&format!(
            "/store/v1/entitlements/{puuid}/{SKIN_LEVEL_ITEM_TYPE}"
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

    /// A match is stored for good, so an answer with no teams or two winners
    /// is a failed fetch (retried later like any failure): see
    /// [`check_winner`] (R166).
    pub async fn match_details(&self, match_id: &str) -> Result<MatchDetailsResponse, RiotError> {
        let details: MatchDetailsResponse = self
            .pd(&format!("/match-details/v1/matches/{match_id}"))
            .await?;
        check_winner(&details)?;
        Ok(details)
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
        // R166 (Scout: retries across builds): one more try, after a short
        // pause, for a 5xx or a transport failure. Same body, same
        // `VisiblePuuid`s: the privacy gate is the argument type, not this loop.
        let mut retried = false;
        let raw: Value = loop {
            match self.pd_put("/name-service/v2/players", &body).await {
                Err(RiotError::Status(code, _)) if code >= 500 && !retried => {}
                Err(RiotError::Http(_)) if !retried => {}
                other => break other?,
            }
            retried = true;
            log::debug!("name-service answered badly; asking once more");
            tokio::time::sleep(NAMES_RETRY_PAUSE).await;
        };
        // Tolerant like every list (R166): a non-array answer is no names, an
        // entry without a `Subject` is skipped.
        Ok(list_from_value(raw))
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
