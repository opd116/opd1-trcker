//! The JSON contract to the UI (Task 5's `model.rs`) and the pure
//! [`build_card`] that fills it. `#[serde(rename_all = "camelCase")]`
//! everywhere (ruling R10) — the UI is a Svelte/TS app that expects
//! camelCase field names.
//!
//! # Tri-state fields (ruling R23)
//!
//! Every field that's fetched asynchronously (name, level, rank,
//! prevActTier, peak, kd, hsPct, actWinrate, last5) needs three distinct
//! states, not two:
//! - **still loading** — the poll loop hasn't attempted this data source
//!   yet for this player this session. Renders as an absent JSON key, so
//!   the UI's skeleton loader keeps showing (CLAUDE.md: "Loading uses
//!   neutral skeleton shapes with no text").
//! - **known missing** — attempted, and there's genuinely nothing there
//!   (e.g. a player with zero competitive matches ever). Renders as JSON
//!   `null`, so the UI shows `—` (CLAUDE.md: "Missing values show —").
//! - **a real value**.
//!
//! [`Field<T>`] carries exactly these three states end to end: it's both
//! the wire type (via its hand-written [`Serialize`] impl plus
//! `#[serde(skip_serializing_if = "Field::is_loading")]` on every tri-state
//! field) and, reused, the shape a caller hands `build_card` for any input
//! that's itself still-loading/known-missing/available (`mmr`, `last5`
//! below) — one small type instead of two parallel ones.

use std::collections::HashMap;

use serde::{Serialize, Serializer};

use crate::db::repo::{CompMatchRow, CompUpdateRow, MatchPlayerRow};
use crate::names::{display_name, KnownName};
use crate::presence::Stage;
use crate::stats::{self, Act, SeasonalMap};

/// CLAUDE.md: "Last 5 Competitive results" — the cap `build_card` enforces
/// on its `last5` output regardless of how many rows the caller passes in
/// (ruling R35).
const LAST5_COUNT: usize = 5;

// ---------------------------------------------------------------------
// Field<T> — the R23 tri-state carrier
// ---------------------------------------------------------------------

/// A value that is either still being fetched, fetched-and-absent, or
/// present. See the module docs for the on-the-wire meaning of each state.
#[derive(Debug, Clone, PartialEq)]
pub enum Field<T> {
    Loading,
    Missing,
    Value(T),
}

impl<T> Field<T> {
    /// For `#[serde(skip_serializing_if = "Field::is_loading")]` — the
    /// mechanism that turns [`Field::Loading`] into an absent key rather
    /// than a serialized value.
    pub fn is_loading(&self) -> bool {
        matches!(self, Field::Loading)
    }
}

impl<T> From<Option<T>> for Field<T> {
    /// `None` -> [`Field::Missing`], `Some(v)` -> [`Field::Value`]. There is
    /// deliberately no way to reach [`Field::Loading`] through this
    /// conversion — a caller that knows it has (or doesn't have) a value
    /// can never mean "still loading".
    fn from(opt: Option<T>) -> Self {
        match opt {
            Some(v) => Field::Value(v),
            None => Field::Missing,
        }
    }
}

impl<T: Serialize> Serialize for Field<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            // Never actually reached when `skip_serializing_if` is wired
            // up correctly (that's what keeps the key absent instead), but
            // degrades to `null` rather than panicking if a future field
            // forgets the attribute.
            Field::Loading => serializer.serialize_none(),
            Field::Missing => serializer.serialize_none(),
            Field::Value(v) => v.serialize(serializer),
        }
    }
}

// ---------------------------------------------------------------------
// DisplayName (R9/R10) / PartyMark / EncounterCount / Team
// ---------------------------------------------------------------------

/// CLAUDE.md's "Hidden names" contract, as a wire type. Which variant a
/// player gets is decided by [`crate::names::display_name`].
///
/// Fix-round item 1: `rename_all = "camelCase"` alone only renames
/// *variants* (`HiddenAgent` -> `"hiddenAgent"`); it does nothing to each
/// variant's *fields*, so `HiddenAgent { agent_id }` was serializing as
/// `{"kind":"hiddenAgent","agent_id":...}` — the brief's contract is
/// `agentId`. `rename_all_fields` (serde >= 1.0.183; this workspace is on
/// 1.0.229) is the separate attribute that renames field names across
/// every variant.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum DisplayName {
    Visible {
        name: String,
        tag: String,
    },
    /// A hidden player's name the DB knows from any source (R145; the wire
    /// says which in `source`). `from_map_id` is a valorant-api uuid, never
    /// a Riot `/Game/...` path (`KnownName::with_map_uuid`), and `null` for
    /// [`RememberedSource::Seen`] (a name-service answer has no map);
    /// `from_start_ms` is when the source match started (or, for `Seen`,
    /// when the name was last confirmed). Both are `null` on the wire when
    /// unknown.
    Remembered {
        name: String,
        tag: String,
        source: RememberedSource,
        from_map_id: Option<String>,
        from_start_ms: Option<i64>,
    },
    HiddenPlayer,
    HiddenAgent {
        agent_id: String,
    },
}

/// R145: where a remembered name came from, on the wire as
/// `"ownMatch" | "otherMatch" | "seen"`. Not a privacy tier: every kind is
/// shown in the remembered style; the UI words the tooltip by it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RememberedSource {
    /// A finished match the user played in showed the name.
    OwnMatch,
    /// A stored match the user did not play in carried the name (someone
    /// else's match history).
    OtherMatch,
    /// A name-service answer from a time the player was visible (the
    /// `players` table).
    Seen,
}

/// A per-group party stripe (CLAUDE.md's "Party detection"), from
/// [`crate::party::detect_parties`]. `group` is the stripe color index
/// (`0..n`); `confirmed` = solid, otherwise dashed.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PartyMark {
    pub group: u8,
    pub confirmed: bool,
}

/// The ⟲ badge's count (CLAUDE.md's "Encounters"): past matches shared
/// with the user, with and against.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EncounterCount {
    pub with: u32,
    pub against: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Team {
    Ally,
    Enemy,
}

/// R32: `"win" | "loss" | "draw"`, not a `bool` — a match can end with no
/// winning team (ruling R29's draw/no-winner case).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MatchResultKind {
    Win,
    Loss,
    Draw,
}

/// Derives [`MatchResultKind`] from a match-players-row shape: `team_id`
/// is this player's team, `winning_team` is the match's winner (`None` for
/// a draw / no winner, ruling R29). Shared by [`build_card`]'s `last5` and
/// by `MatchSummary`-building callers (Task 9).
pub fn match_result(team_id: &str, winning_team: Option<&str>) -> MatchResultKind {
    match winning_team {
        None => MatchResultKind::Draw,
        Some(wt) if wt == team_id => MatchResultKind::Win,
        Some(_) => MatchResultKind::Loss,
    }
}

// ---------------------------------------------------------------------
// PlayerCard
// ---------------------------------------------------------------------

/// R28: `tier`, `actLabel` (short, e.g. `"E8 A3"`), `actName` (full, e.g.
/// `"V26 ACT V"` — `stats::act_full_name`), `sinceActEndMs` (`None` for
/// the current act — the UI shows "This Act"), plus `actStartMs`/
/// `actEndMs` for the hover tooltip's exact dates.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeakInfo {
    pub tier: u8,
    pub act_label: String,
    pub act_name: String,
    pub since_act_end_ms: Option<i64>,
    pub act_start_ms: i64,
    pub act_end_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RankInfo {
    pub tier: u8,
    pub rr: u16,
}

/// R5: `rrEarned` is a signed delta.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LastFiveEntry {
    pub result: MatchResultKind,
    pub rr_earned: Option<i32>,
    pub start_ms: i64,
}

/// The per-poll, per-player card shown in `Live`. See the module docs for
/// which fields are tri-state ([`Field<T>`]) vs plain `Option`/values.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerCard {
    pub puuid: String,
    /// R34: tri-state like every other async field, not a plain
    /// `DisplayName`. A hidden player's `HiddenPlayer`/`HiddenAgent` is
    /// known instantly from identity + selection state (no separate fetch,
    /// so it's `Field::Value` immediately), but a *visible* player's real
    /// Riot ID comes from a separate name-service call the roster fetch
    /// itself doesn't include — until that resolves, this is
    /// `Field::Loading` (absent key), never `Value(Visible { name: "", tag: "" })`.
    #[serde(skip_serializing_if = "Field::is_loading")]
    pub name: Field<DisplayName>,
    #[serde(skip_serializing_if = "Field::is_loading")]
    pub level: Field<u32>,
    pub card_id: String,
    /// Not tri-state (ruling: unlike level/rank/etc., the agent selection
    /// state arrives atomically with the roster fetch itself — there is no
    /// separate async round trip that could still be "loading" for it).
    pub agent_id: Option<String>,
    #[serde(skip_serializing_if = "Field::is_loading")]
    pub rank: Field<RankInfo>,
    #[serde(skip_serializing_if = "Field::is_loading")]
    pub prev_act_tier: Field<u8>,
    #[serde(skip_serializing_if = "Field::is_loading")]
    pub peak: Field<PeakInfo>,
    #[serde(skip_serializing_if = "Field::is_loading")]
    pub kd: Field<f32>,
    #[serde(skip_serializing_if = "Field::is_loading")]
    pub hs_pct: Field<f32>,
    #[serde(skip_serializing_if = "Field::is_loading")]
    pub act_winrate: Field<f32>,
    /// The current act's wins (placement games included, as Riot's
    /// `NumberOfWinsWithPlacements`; `NumberOfWins` when that's absent).
    /// `actWinrate` is `act_wins / act_games`. `null` when MMR is missing,
    /// the current act has no entry, or it has no games.
    #[serde(skip_serializing_if = "Field::is_loading")]
    pub act_wins: Field<u32>,
    #[serde(skip_serializing_if = "Field::is_loading")]
    pub act_games: Field<u32>,
    #[serde(skip_serializing_if = "Field::is_loading")]
    pub last5: Field<Vec<LastFiveEntry>>,
    /// `None` when not in a detected party — and always for the user and
    /// their own party.
    pub party: Option<PartyMark>,
    /// `None` for strangers (nothing shared) — and always for the user and
    /// their own party.
    pub encounters: Option<EncounterCount>,
    pub team: Team,
    /// This card is the user's own.
    pub is_self: bool,
    pub is_self_party: bool,
}

// ---------------------------------------------------------------------
// LiveState
// ---------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveState {
    pub stage: Stage,
    /// A valorant-api map uuid, never Riot's `/Game/Maps/...` path — the
    /// UI looks assets up by uuid.
    pub map_id: Option<String>,
    pub players: Vec<PlayerCard>,
}

// ---------------------------------------------------------------------
// PlayerDetails / MatchSummary (Task 9 reuses MatchSummary)
// ---------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchSummary {
    pub match_id: String,
    /// A valorant-api map uuid, like every other `map_id` sent to the UI
    /// in this module — never Riot's `/Game/Maps/...` path. 5b converts
    /// via `opd1_riot::assets::AssetCache::map_uuid_for_path` before
    /// constructing one of these.
    pub map_id: String,
    pub start_ms: i64,
    pub result: MatchResultKind,
    pub rounds_won: Option<i64>,
    pub rounds_lost: Option<i64>,
    pub agent_id: Option<String>,
    /// The stat line is `None` (`null`, shown as `—`) when the match's
    /// details had no stats row for the player (stored 0/0/0 with no
    /// rounds): an unknown value is never a counted zero (R102). The same
    /// goes for `kd`, `acs` and `hs_pct` below.
    pub kills: Option<i64>,
    pub deaths: Option<i64>,
    pub assists: Option<i64>,
    /// `stats::kd` (zero deaths -> the kill count, ruling R5). `None`
    /// exactly when the stat line is.
    pub kd: Option<f32>,
    /// Average combat score: score / rounds played. `None` without stats.
    pub acs: Option<f32>,
    /// Headshot fraction 0..1 (R5); `None` when no shot landed or the
    /// player has no stats row.
    pub hs_pct: Option<f32>,
    pub rr_earned: Option<i32>,
    /// The player's rank during the match (R81), `None` when unknown or
    /// below 3 (unranked); see [`display_tier`].
    pub tier: Option<u8>,
}

/// A stored tier as the UI shows it: `None` below 3 (unranked, placements,
/// unrated queues) and for a value no tier table has, so the UI draws no
/// rank icon rather than a wrong one.
pub fn display_tier(tier: Option<i64>) -> Option<u8> {
    tier.and_then(|t| u8::try_from(t).ok()).filter(|&t| t >= 3)
}

/// One stored competitive match as a [`MatchSummary`] — shared by Player
/// Details' "Recent" and My Stats' "Last 5". `row.map_id` must already be
/// a valorant-api uuid (`""` when unknown).
pub fn match_summary(row: &CompMatchRow) -> MatchSummary {
    // `match_from_details` stores no round count when the player's stats
    // were missing: then the whole stat line is unknown (R102).
    let rounds = row.rounds.filter(|&r| r > 0);
    let has_stats = rounds.is_some();
    MatchSummary {
        match_id: row.match_id.clone(),
        map_id: row.map_id.clone(),
        start_ms: row.start_ms,
        result: match_result(&row.team_id, row.winning_team.as_deref()),
        rounds_won: row.rounds_won,
        rounds_lost: row.rounds_lost,
        agent_id: row.agent_id.clone(),
        kills: has_stats.then_some(row.kills),
        deaths: has_stats.then_some(row.deaths),
        assists: has_stats.then_some(row.assists),
        kd: has_stats.then(|| stats::kd(row.kills, row.deaths)),
        acs: rounds.map(|r| row.score as f32 / r as f32),
        hs_pct: stats::hs_pct(row.hs, row.bs, row.ls).filter(|_| has_stats),
        rr_earned: row.rr_earned.map(|v| v as i32),
        tier: display_tier(row.tier),
    }
}

/// Player Details' hero K/D and HS% (R116): pooled over `rows`, the 20
/// newest current-act competitive matches, the window the Details sync
/// fetches for Top 3. Only rows with a stats row count
/// (R102): a match without one adds no kills, deaths or shots and is not
/// in `matches`. No such row: everything is `None` / 0.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct HeroStats {
    pub kd: Option<f32>,
    /// Headshots / shots landed, 0..1.
    pub hs_pct: Option<f32>,
    /// How many matches `kd` and `hs_pct` are computed over.
    pub matches: u32,
}

pub fn hero_stats(rows: &[CompMatchRow]) -> HeroStats {
    let counted: Vec<&CompMatchRow> = rows
        .iter()
        .filter(|r| r.rounds.is_some_and(|n| n > 0))
        .collect();
    if counted.is_empty() {
        return HeroStats::default();
    }
    let sum = |f: fn(&CompMatchRow) -> i64| counted.iter().map(|r| f(r)).sum::<i64>();
    HeroStats {
        kd: Some(stats::kd(sum(|r| r.kills), sum(|r| r.deaths))),
        hs_pct: stats::hs_pct(sum(|r| r.hs), sum(|r| r.bs), sum(|r| r.ls)),
        matches: counted.len() as u32,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeammateSummary {
    pub puuid: String,
    pub name: DisplayName,
    pub count: u32,
}

/// One equipped weapon skin, named (R97/R83): `skin_name` is the skin's
/// display name minus the trailing weapon word (`"Reaver Vandal"` ->
/// `"Reaver"`), `chroma_name` the `X` of the chroma's `(Variant N X)`
/// (`None` for the base chroma), `tier_id` the skin's content-tier uuid
/// (`None` for default and battle-pass skins).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeaponSkin {
    pub chroma_id: String,
    pub skin_name: String,
    pub chroma_name: Option<String>,
    pub tier_id: Option<String>,
}

/// Vandal/Phantom/Operator/Sheriff, each with the R23 tri-state (R97):
/// the key is absent while the skin's name is still resolving
/// ([`Field::Loading`]), `null` when the player has no such skin or it
/// can't be known ([`Field::Missing`]), else the [`WeaponSkin`].
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Loadout {
    #[serde(skip_serializing_if = "Field::is_loading")]
    pub vandal: Field<WeaponSkin>,
    #[serde(skip_serializing_if = "Field::is_loading")]
    pub phantom: Field<WeaponSkin>,
    #[serde(skip_serializing_if = "Field::is_loading")]
    pub operator: Field<WeaponSkin>,
    #[serde(skip_serializing_if = "Field::is_loading")]
    pub sheriff: Field<WeaponSkin>,
}

impl Default for Loadout {
    /// Nothing known: all four weapons [`Field::Missing`] (`null`).
    fn default() -> Self {
        Self {
            vandal: Field::Missing,
            phantom: Field::Missing,
            operator: Field::Missing,
            sheriff: Field::Missing,
        }
    }
}

/// One shared match in Player Details' Encounters, from the user's side
/// (`result`, `rounds_*`, `self_*`) with the other player as `them_*`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EncounterDetailRow {
    pub match_id: String,
    /// A valorant-api map uuid — see [`MatchSummary::map_id`]'s doc.
    pub map_id: String,
    pub start_ms: i64,
    pub with: bool,
    /// The user's result (`Draw` for no team winner, ruling R29).
    pub result: MatchResultKind,
    /// The user's team's rounds won / the opponent's (`None` when the
    /// match has no exactly-two-team score).
    pub rounds_won: Option<i64>,
    pub rounds_lost: Option<i64>,
    pub duration_ms: Option<i64>,
    pub them_agent_id: Option<String>,
    /// Ranks during that match (R81), `None` below 3; see [`display_tier`].
    pub them_tier: Option<u8>,
    pub self_agent_id: Option<String>,
    pub self_tier: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerDetails {
    pub card: PlayerCard,
    /// Up to 8, newest first (CLAUDE.md: "3 shown ... Show More reveals
    /// more, up to 8 total").
    pub recent: Vec<MatchSummary>,
    pub top_teammates: Vec<TeammateSummary>,
    pub loadout: Loadout,
    pub encounters: Vec<EncounterDetailRow>,
    /// R116: the hero's K/D over the player's most recent (up to 20)
    /// competitive matches of the current act; `null` when there is none.
    pub hero_kd: Option<f32>,
    /// R116: HS% (0..1) over the same set.
    pub hero_hs: Option<f32>,
    /// R116: how many matches `hero_kd` / `hero_hs` use (0..=20).
    pub hero_matches: u32,
}

// ---------------------------------------------------------------------
// MatchDetailsView (Task 15): the Match Details scoreboard
// ---------------------------------------------------------------------

/// Whose side the scoreboard is drawn from: the user's when the user
/// played the match (`self`), else the focus player's (`focus`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Perspective {
    #[serde(rename = "self")]
    SelfView,
    Focus,
}

/// `match_details(matchId, focusPuuid)`: built by
/// [`crate::matchview::build_match_view`].
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchDetailsView {
    pub match_id: String,
    /// A valorant-api map uuid (`""` when unknown), never a Riot path.
    pub map_id: String,
    pub queue_id: String,
    pub start_ms: i64,
    pub duration_ms: Option<i64>,
    pub perspective: Perspective,
    /// `[0]` is the perspective team. A mode without two teams (Deathmatch)
    /// is one block with an empty `team_id` and no result or score.
    pub teams: Vec<MatchTeamView>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchTeamView {
    pub team_id: String,
    pub result: Option<MatchResultKind>,
    pub rounds_won: Option<i64>,
    /// Rounds won in the first half (rounds 1–12), when known.
    pub first_half: Option<i64>,
    /// Sorted by ACS descending, rows without ACS last.
    pub players: Vec<ScoreRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoreRow {
    pub puuid: String,
    pub name: DisplayName,
    pub agent_id: Option<String>,
    /// The player's rank during the match (R81); `None` below 3.
    pub tier: Option<u8>,
    /// `None` (JSON `null`, the UI shows `—`) when the player has no stats
    /// row in the match (R124): unknown is never a counted 0. Same rule as
    /// `acs`, `adr` and `hs_pct`, and as `MatchSummary` (R102).
    pub kills: Option<i64>,
    pub deaths: Option<i64>,
    pub assists: Option<i64>,
    pub acs: Option<f32>,
    pub adr: Option<f32>,
    pub hs_pct: Option<f32>,
    pub is_self: bool,
    pub is_focus: bool,
}

// ---------------------------------------------------------------------
// MyStats (Task 9)
// ---------------------------------------------------------------------

/// The My Stats page (CLAUDE.md "My Stats"): the user's own account, the
/// current act only, built by [`crate::mystats::build`] from what the DB
/// has — never from a live Riot call. `null` always means "the DB doesn't
/// know" (the UI shows `—`); a `0` is only ever a counted zero.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MyStats {
    pub rank: RankSummary,
    /// The RR chart, oldest first (one point per ranked competitive
    /// update).
    pub rr_series: Vec<RrPoint>,
    /// The current session (a 6h gap with no competitive match starts a
    /// new one); `None` when the act's newest stored match started more
    /// than 6h ago (that session is over) or there is none.
    pub session: Option<SessionSummary>,
    /// The act's newest 5 competitive matches, newest first.
    pub last5: Vec<MatchSummary>,
    /// Most played first.
    pub maps: Vec<StatRow>,
    /// Most played first.
    pub agents: Vec<StatRow>,
    /// Local time, Monday first: index 0 = Monday … 6 = Sunday.
    pub by_weekday: [WinCell; 7],
    /// Local time: Morning 05–12, Afternoon 12–17, Evening 17–22,
    /// Night 22–05.
    pub by_daypart: [WinCell; 4],
    pub insights: Vec<Insight>,
}

/// Section 1, the rank card.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RankSummary {
    /// From the stored MMR snapshot, like the live card's rank.
    pub tier: Option<u8>,
    pub rr: Option<u16>,
    /// Counted from the act's stored matches; draws are neither.
    pub wins: Option<u32>,
    pub losses: Option<u32>,
    /// `None` from Immortal up (RR accumulates, no fixed next step) and
    /// when unranked.
    pub rr_to_next: Option<u16>,
    /// Sum of the act's stored RR changes.
    pub net_rr: Option<i32>,
    /// Mean RR change over ranked wins / losses with a known RR change —
    /// placement games (0 RR) and draws left out (`avgLoss` is negative).
    pub avg_win: Option<f32>,
    pub avg_loss: Option<f32>,
    /// The latest tier changes, newest first (at most 5).
    pub changes: Vec<RankChange>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RankChange {
    /// The tier reached.
    pub tier: u8,
    pub up: bool,
    pub start_ms: i64,
}

/// `absoluteRr` puts every rank on one line: `tier * 100 + rr` below
/// Immortal, `2400 + rr` from Immortal 1 up (Immortal/Radiant RR is
/// cumulative from Immortal 1's floor).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RrPoint {
    pub start_ms: i64,
    pub absolute_rr: i32,
}

/// Section 3.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    /// `None` when no match in the session has a stored RR change.
    pub net_rr: Option<i32>,
    pub wins: u32,
    pub losses: u32,
    pub started_ms: i64,
    /// Every result of the session, oldest first.
    pub streak: Vec<StreakMark>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum StreakMark {
    #[serde(rename = "W")]
    Win,
    #[serde(rename = "L")]
    Loss,
    #[serde(rename = "D")]
    Draw,
}

/// One row of the Maps or Agents table (sections 5–6). `id` is a
/// valorant-api map or agent uuid.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatRow {
    pub id: String,
    pub games: u32,
    pub wins: u32,
    pub losses: u32,
    /// Wins / games (draws count as games), a fraction (R5).
    pub winrate: f32,
    /// `None` when none of the row's matches has a stored RR change.
    pub net_rr: Option<i32>,
    pub kd: f32,
    /// `None` when no match of the row has a known round count.
    pub acs: Option<f32>,
}

/// One weekday or time-of-day bucket (section 7).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct WinCell {
    pub games: u32,
    pub wins: u32,
    /// `None` when the bucket has no games.
    pub winrate: Option<f32>,
}

/// Section 8. `text` is finished English UI copy; `matches` is the sample
/// it's based on. An insight without enough data is left out entirely.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Insight {
    pub kind: InsightKind,
    pub text: String,
    pub matches: u32,
    /// Good news (a strength, RR gained) vs a weak spot.
    pub positive: bool,
}

/// Stable per-insight key, e.g. for picking an icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum InsightKind {
    BestMap,
    WorstMap,
    TimeOfDay,
    WeekdayWeekend,
    RrEconomy,
    Last7Days,
}

// ---------------------------------------------------------------------
// build_card
// ---------------------------------------------------------------------

/// Identity fields that arrive atomically with the roster fetch itself —
/// never independently "loading" (see [`PlayerCard::agent_id`]'s doc).
#[derive(Debug, Clone, PartialEq)]
pub struct Identity {
    pub puuid: String,
    pub incognito: bool,
    pub hide_account_level: bool,
    pub account_level: u32,
    pub card_id: String,
    /// Riot's `CharacterID` — `""` before a character is chosen.
    pub character_id: String,
    /// Riot's `CharacterSelectionState` — `""` | `"selected"` | `"locked"`.
    /// **Only `PregamePlayer` (Agent Select) actually has this field.**
    /// `CoreGamePlayer` (In Game) has no `CharacterSelectionState` at all —
    /// every player in a started match already has an agent locked, so a
    /// 5b caller building an `Identity` from a core-game roster must pass
    /// `"locked"` here itself. The value must be `"locked"` (R99: only a
    /// locked pick shows an agent; `"selected"`, `""` or anything else
    /// hides it). Passing `""` for a core-game player would incorrectly
    /// hide their already-known agent (see [`build_card`]'s agent-select
    /// handling).
    pub character_selection_state: String,
    /// `""` when not yet known (e.g. before name-service resolves) —
    /// [`build_card`] treats an empty `game_name`/`tag_line` pair as
    /// "loading", not "this player has an empty name" (ruling R34).
    pub game_name: String,
    pub tag_line: String,
}

/// The current act's resolved rank — `RankedRating` isn't part of
/// [`stats::SeasonInfo`] (it isn't needed for the peak/prevActTier/winrate
/// math that struct exists for), so it travels alongside the seasonal map
/// as its own small value instead.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CurrentRank {
    pub tier: u8,
    pub rr: u16,
}

/// The whole MMR-derived bundle for a player — one Riot call
/// (`mmr/v1/players/{puuid}`), so it's fetched (and can be "loading") as
/// one unit rather than as separate fields.
#[derive(Debug, Clone, PartialEq)]
pub struct MmrInput {
    pub seasons: SeasonalMap,
    /// `None` when the current act's `SeasonalInfoBySeasonID` entry is
    /// absent or unranked (`CompetitiveTier < 3`) — e.g. still in
    /// placements, or unrated-only this act.
    pub current_rank: Option<CurrentRank>,
}

/// Everything [`build_card`] needs. Grouped into one struct per the
/// brief's "your choice of exact parameter struct" — every field maps
/// straight onto an existing `opd1_core` type (no DB/network needed to
/// construct one, so this stays trivially unit-testable).
pub struct BuildCardInput<'a> {
    pub identity: Identity,
    /// Ruling (R33, see task-5a-report.md): `rank`, `prevActTier`, `peak`
    /// and `actWinrate` are *all* MMR-derived and go [`Field::Missing`]
    /// together when this is [`Field::Missing`] — they're one Riot call.
    /// `kd`/`hsPct`/`last5` come from `last5` below instead, and stay
    /// populated even without MMR (CLAUDE.md/brief: "no MMR → rank None,
    /// other fields present").
    pub mmr: Field<MmrInput>,
    /// This player's `comp_updates` rows (any seasons) — used only to look
    /// up each `last5` entry's `rrEarned` by `match_id`. Missing/Loading
    /// simply leaves every entry's `rrEarned` as `None`; it does not blank
    /// `last5` itself (that's independently driven by `last5` below).
    pub comp_updates: Field<Vec<CompUpdateRow>>,
    /// This player's last-N (typically 5) `competitive` `match_players`
    /// rows (`Repo::last_n_comp`), in any order — `build_card` sorts
    /// newest first itself.
    pub last5: Field<Vec<MatchPlayerRow>>,
    /// Sorted ascending by `start_ms` (same precondition as
    /// `stats::peak`/`stats::prev_act_tier`).
    pub ordered_acts: &'a [Act],
    pub current_act_id: &'a str,
    pub now_ms: i64,
    pub team: Team,
    /// The card is the user's own (`PlayerCard::is_self`).
    pub is_self: bool,
    /// The user or a member of the user's own party (CLAUDE.md: they get
    /// no party stripe and no encounter badge — [`build_card`] drops
    /// `party`/`encounters` for them).
    pub is_self_party: bool,
    /// The best name the DB knows for the player from any source
    /// (`Repo::known_name`, R145) — shown only when the player is incognito
    /// now (CLAUDE.md "Hidden names" exception).
    pub remembered: Option<KnownName>,
    pub party: Option<PartyMark>,
    pub encounters: Option<EncounterCount>,
}

/// The agent a card shows: `character_id`, but only once it's locked (R99).
/// `CharacterSelectionState` is `""` before a pick, `"selected"` while the
/// player only hovers an agent, and `"locked"` once it's confirmed; a
/// hover changes nothing (CLAUDE.md: never a placeholder agent). Core-game
/// rows pass `"locked"` themselves (see [`Identity::character_selection_state`]).
pub fn shown_agent<'a>(character_id: &'a str, selection_state: &str) -> Option<&'a str> {
    if selection_state.eq_ignore_ascii_case("locked") && !character_id.is_empty() {
        Some(character_id)
    } else {
        None
    }
}

fn non_empty(s: &str) -> Option<String> {
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

/// Builds one [`PlayerCard`]. Pure — no DB, no I/O, no Riot/valorant-api
/// types (those conversions live in `opd1-riot`'s `to_core.rs`); every
/// input is a plain `opd1_core` type, so this is fully exercised by
/// `crates/core/tests/card.rs` without a database or network.
pub fn build_card(input: BuildCardInput) -> PlayerCard {
    let BuildCardInput {
        identity,
        mmr,
        comp_updates,
        last5,
        ordered_acts,
        current_act_id,
        now_ms,
        team,
        is_self,
        is_self_party,
        remembered,
        party,
        encounters,
    } = input;

    // CLAUDE.md: agent head icon "only after the agent is selected" — R99:
    // locked, not merely hovered.
    let agent_id = shown_agent(&identity.character_id, &identity.character_selection_state)
        .map(str::to_string);

    // CLAUDE.md "Hidden names", decided by `names::display_name`. An
    // incognito player's name never comes from `identity` (not even
    // passed in). Ruling R34: the hidden states are known instantly (no
    // separate fetch — everything they need is already here), so they're
    // `Field::Value` immediately; a *visible* player's name comes from a
    // separate name-service call the roster fetch doesn't include, so it's
    // `Field::Loading` until `identity.game_name`/`tag_line` are both
    // non-empty — never `Value(Visible { name: "", tag: "" })`.
    let name = if identity.incognito {
        Field::Value(display_name(true, None, agent_id.as_deref(), remembered))
    } else {
        match (
            non_empty(&identity.game_name),
            non_empty(&identity.tag_line),
        ) {
            (Some(name), Some(tag)) => {
                Field::Value(display_name(false, Some((name, tag)), None, None))
            }
            _ => Field::Loading,
        }
    };

    // CLAUDE.md: "The user and their own party members get no party
    // stripe and no encounter badge."
    let (party, encounters) = if is_self_party {
        (None, None)
    } else {
        (party, encounters)
    };

    let level = if identity.hide_account_level {
        Field::Missing
    } else {
        Field::Value(identity.account_level)
    };

    // Ruling R35: valorant-api's `/v1/seasons` lists future acts (observed
    // live — e.g. the act after the current one is already published,
    // months ahead of its own start date). `stats::peak`/`prev_act_tier`
    // both assume `ordered_acts`'s *last* element is the current act; an
    // unstarted future act at the end would break that assumption (the
    // "previous act" would resolve to the actual current act instead).
    // Clip to acts that have actually started before doing anything
    // act-position-sensitive with them.
    let started_acts: Vec<Act> = ordered_acts
        .iter()
        .filter(|a| a.start_ms <= now_ms)
        .cloned()
        .collect();

    let MmrFields {
        rank,
        prev_act_tier,
        peak,
        act_winrate,
        act_wins,
        act_games,
    } = build_mmr_fields(&mmr, &started_acts, current_act_id, now_ms);
    let (kd, hs_pct, last5_out) = build_last5_fields(&last5, &comp_updates);

    PlayerCard {
        puuid: identity.puuid,
        name,
        level,
        card_id: identity.card_id,
        agent_id,
        rank,
        prev_act_tier,
        peak,
        kd,
        hs_pct,
        act_winrate,
        act_wins,
        act_games,
        last5: last5_out,
        party,
        encounters,
        team,
        is_self,
        is_self_party,
    }
}

/// The MMR-derived card fields, resolved together (one Riot call).
struct MmrFields {
    rank: Field<RankInfo>,
    prev_act_tier: Field<u8>,
    peak: Field<PeakInfo>,
    act_winrate: Field<f32>,
    act_wins: Field<u32>,
    act_games: Field<u32>,
}

fn build_mmr_fields(
    mmr: &Field<MmrInput>,
    ordered_acts: &[Act],
    current_act_id: &str,
    now_ms: i64,
) -> MmrFields {
    match mmr {
        Field::Loading => MmrFields {
            rank: Field::Loading,
            prev_act_tier: Field::Loading,
            peak: Field::Loading,
            act_winrate: Field::Loading,
            act_wins: Field::Loading,
            act_games: Field::Loading,
        },
        Field::Missing => MmrFields {
            rank: Field::Missing,
            prev_act_tier: Field::Missing,
            peak: Field::Missing,
            act_winrate: Field::Missing,
            act_wins: Field::Missing,
            act_games: Field::Missing,
        },
        Field::Value(mmr_input) => {
            let rank = match mmr_input.current_rank {
                Some(cr) => Field::Value(RankInfo {
                    tier: cr.tier,
                    rr: cr.rr,
                }),
                None => Field::Missing,
            };

            let prev_act_tier = stats::prev_act_tier(&mmr_input.seasons, ordered_acts).into();

            let peak = match stats::peak(&mmr_input.seasons, ordered_acts) {
                Some(p) => match ordered_acts.iter().find(|a| a.id == p.season_id) {
                    Some(act) => Field::Value(PeakInfo {
                        tier: p.tier,
                        act_label: stats::act_label(act),
                        act_name: stats::act_full_name(act),
                        since_act_end_ms: stats::since_act_end(act, now_ms),
                        act_start_ms: act.start_ms,
                        act_end_ms: act.end_ms,
                    }),
                    // The peak's season isn't in `ordered_acts` — can't
                    // label it, so degrade to Missing rather than panic.
                    None => Field::Missing,
                },
                None => Field::Missing,
            };

            // The current act's record; no entry, or no games, leaves the
            // win rate and both counts unknown.
            let act = mmr_input
                .seasons
                .get(current_act_id)
                .filter(|info| info.number_of_games > 0);
            let act_winrate = act
                .and_then(|info| {
                    stats::winrate(info.number_of_wins as i64, info.number_of_games as i64)
                })
                .into();
            let act_wins = act.map(|info| info.number_of_wins).into();
            let act_games = act.map(|info| info.number_of_games).into();

            MmrFields {
                rank,
                prev_act_tier,
                peak,
                act_winrate,
                act_wins,
                act_games,
            }
        }
    }
}

type Last5Fields = (Field<f32>, Field<f32>, Field<Vec<LastFiveEntry>>);

fn build_last5_fields(
    last5: &Field<Vec<MatchPlayerRow>>,
    comp_updates: &Field<Vec<CompUpdateRow>>,
) -> Last5Fields {
    let rows = match last5 {
        Field::Loading => return (Field::Loading, Field::Loading, Field::Loading),
        Field::Missing => return (Field::Missing, Field::Missing, Field::Missing),
        Field::Value(rows) => rows,
    };

    if rows.is_empty() {
        return (Field::Missing, Field::Missing, Field::Value(Vec::new()));
    }

    let mut sorted: Vec<&MatchPlayerRow> = rows.iter().collect();
    sorted.sort_by_key(|r| std::cmp::Reverse(r.start_ms));
    // Ruling R35: defensively cap at 5 *after* sorting newest-first, even
    // though every real caller is expected to already pass exactly
    // `Repo::last_n_comp(puuid, 5)` — kd/hsPct must only ever aggregate
    // the last 5, not however many rows happened to be passed in.
    sorted.truncate(LAST5_COUNT);

    let total_kills: i64 = sorted.iter().map(|r| r.kills).sum();
    let total_deaths: i64 = sorted.iter().map(|r| r.deaths).sum();
    let total_hs: i64 = sorted.iter().map(|r| r.hs).sum();
    let total_bs: i64 = sorted.iter().map(|r| r.bs).sum();
    let total_ls: i64 = sorted.iter().map(|r| r.ls).sum();

    let kd = Field::Value(stats::kd(total_kills, total_deaths));
    let hs_pct = stats::hs_pct(total_hs, total_bs, total_ls).into();

    let rr_by_match: HashMap<&str, i32> = match comp_updates {
        Field::Value(updates) => updates
            .iter()
            .map(|u| (u.match_id.as_str(), u.rr_earned as i32))
            .collect(),
        Field::Loading | Field::Missing => HashMap::new(),
    };

    let entries: Vec<LastFiveEntry> = sorted
        .iter()
        .map(|r| LastFiveEntry {
            result: match_result(&r.team_id, r.winning_team.as_deref()),
            rr_earned: rr_by_match.get(r.match_id.as_str()).copied(),
            start_ms: r.start_ms,
        })
        .collect();

    (kd, hs_pct, Field::Value(entries))
}
