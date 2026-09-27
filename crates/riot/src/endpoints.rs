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

use serde::Deserialize;
use serde_json::Value;

use crate::session::{RiotError, RiotSession};

// ---------------------------------------------------------------------
// VisiblePuuid — ruling R16
// ---------------------------------------------------------------------

/// A PUUID known, at construction time, not to belong to an incognito
/// player. [`RiotSession::names`] only accepts these, which makes CLAUDE.md's
/// "never request a hidden player's name from any endpoint" a type-level
/// guarantee instead of a convention a caller could forget to follow.
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
/// until characters are locked, and no field here was called out as
/// needed yet (loadout display is a later task), so we keep it as opaque
/// JSON rather than guessing at the shape.
#[derive(Debug, Clone, Deserialize)]
pub struct PregameLoadoutsResponse {
    #[serde(rename = "Loadouts")]
    pub loadouts: Vec<Value>,
    #[serde(rename = "LoadoutsValid")]
    pub loadouts_valid: bool,
}

// ---------------------------------------------------------------------
// Core game (In Game)
// ---------------------------------------------------------------------

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
    #[serde(rename = "NumberOfGames")]
    pub number_of_games: i64,
    #[serde(rename = "WinsByTier")]
    pub wins_by_tier: Option<HashMap<String, i64>>,
    #[serde(rename = "LeaderboardRank")]
    pub leaderboard_rank: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QueueSkill {
    #[serde(rename = "SeasonalInfoBySeasonID")]
    pub seasonal_info_by_season_id: HashMap<String, SeasonalInfo>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlayerMmrResponse {
    #[serde(rename = "Subject")]
    pub subject: String,
    #[serde(rename = "QueueSkills")]
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
    pub player_stats: Vec<PlayerRoundStats>,
}

#[derive(Debug, Clone, Deserialize)]
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
    pub stats: MatchPlayerStats,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchTeam {
    pub team_id: String,
    pub won: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchDetailsResponse {
    pub players: Vec<MatchPlayer>,
    pub teams: Vec<MatchTeam>,
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
    #[serde(rename = "IsReady")]
    pub is_ready: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PartyResponse {
    #[serde(rename = "ID")]
    pub id: String,
    #[serde(rename = "Members")]
    pub members: Vec<PartyMember>,
    #[serde(rename = "State")]
    pub state: String,
    #[serde(rename = "EligibleQueues")]
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
    #[serde(rename = "DisplayName")]
    pub display_name: String,
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

    pub async fn comp_updates(
        &self,
        puuid: &str,
        start: Option<u32>,
        end: Option<u32>,
    ) -> Result<CompetitiveUpdatesResponse, RiotError> {
        let query = query_string(&[
            ("startIndex", start.map(|v| v.to_string())),
            ("endIndex", end.map(|v| v.to_string())),
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
