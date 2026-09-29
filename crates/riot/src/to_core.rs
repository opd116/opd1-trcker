//! Riot / valorant-api -> `opd1_core` conversions. `opd1-riot` depends on
//! `opd1-core` (not the other way around), so every "turn Riot's/
//! valorant-api's wire shape into our plain data" function lives here
//! rather than in `opd1_core::stats`/`opd1_core::db::repo`, which stay
//! Riot-shape-agnostic (`opd1_core::stats`'s own doc comment: "Task 5
//! converts Riot's raw responses into the plain input types here").
//!
//! Scope (Task 5a): MMR -> [`SeasonalMap`] + current rank, competitive
//! updates -> [`CompUpdateRow`]s, match-details -> [`Match`] +
//! [`MatchTeam`] + [`MatchPlayer`] rows, and valorant-api's `/v1/seasons`
//! -> ordered [`Act`]s.

use std::collections::HashMap;

use serde::Deserialize;

use opd1_core::db::repo::{Match, MatchPlayer, MatchTeam};
use opd1_core::model::CurrentRank;
use opd1_core::stats::{Act, SeasonInfo, SeasonalMap};

use crate::endpoints::{
    CompetitiveUpdatesResponse, MatchDetailsResponse, PlayerMmrResponse, RoundResult,
};
use opd1_core::db::repo::CompUpdateRow;

// ---------------------------------------------------------------------
// small shared helpers
// ---------------------------------------------------------------------

/// Turns Riot's "`""` means not applicable" convention into `None` — same
/// convention `opd1_core::presence` already applies to `partyId`/`queueId`.
fn non_empty(s: &str) -> Option<String> {
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

fn clamp_u8(v: i64) -> u8 {
    v.clamp(0, u8::MAX as i64) as u8
}

fn clamp_u16(v: i64) -> u16 {
    v.clamp(0, u16::MAX as i64) as u16
}

fn clamp_u32(v: i64) -> u32 {
    v.clamp(0, u32::MAX as i64) as u32
}

// ---------------------------------------------------------------------
// MMR -> SeasonalMap + current rank
// ---------------------------------------------------------------------

const COMPETITIVE_QUEUE: &str = "competitive";

/// The "unranked" sentinel — tiers below this (0-2) mean no rank yet
/// (still in placements, or unrated-only this act). Mirrors
/// `opd1_core::stats::peak`/`prev_act_tier`'s own `tier < 3` guard.
const UNRANKED_BELOW: u8 = 3;

/// `mmr/v1/players/{puuid}`'s competitive `SeasonalInfoBySeasonID` ->
/// `opd1_core::stats::SeasonalMap`. Every season present in the response
/// is kept (needed across all acts for `stats::peak`); RR isn't part of
/// this map (see [`current_rank_from_mmr`]) since `SeasonInfo` has no use
/// for it beyond the *current* act.
pub fn seasonal_map_from_mmr(resp: &PlayerMmrResponse) -> SeasonalMap {
    let Some(competitive) = resp.queue_skills.get(COMPETITIVE_QUEUE) else {
        return SeasonalMap::new();
    };

    competitive
        .seasonal_info_by_season_id
        .iter()
        .map(|(season_id, info)| {
            let wins_by_tier: HashMap<u8, u32> = info
                .wins_by_tier
                .clone()
                .unwrap_or_default()
                .into_iter()
                .filter_map(|(tier_str, wins)| {
                    tier_str
                        .parse::<u8>()
                        .ok()
                        .map(|tier| (tier, clamp_u32(wins)))
                })
                .collect();

            (
                season_id.clone(),
                SeasonInfo {
                    competitive_tier: clamp_u8(info.competitive_tier),
                    wins_by_tier,
                    // Placement wins count toward the act win rate; fall
                    // back to `NumberOfWins` when Riot doesn't send them.
                    number_of_wins: clamp_u32(
                        info.number_of_wins_with_placements
                            .unwrap_or(info.number_of_wins),
                    ),
                    number_of_games: clamp_u32(info.number_of_games),
                },
            )
        })
        .collect()
}

/// The current act's resolved `{tier, rr}` from the same MMR response,
/// separately from [`seasonal_map_from_mmr`] since `RankedRating` isn't
/// part of `SeasonInfo`. `None` when the current act has no
/// `SeasonalInfoBySeasonID` entry at all, or its tier is unranked
/// (`< 3` — still in placements / unrated-only this act).
pub fn current_rank_from_mmr(resp: &PlayerMmrResponse, season_id: &str) -> Option<CurrentRank> {
    let competitive = resp.queue_skills.get(COMPETITIVE_QUEUE)?;
    let info = competitive.seasonal_info_by_season_id.get(season_id)?;
    let tier = clamp_u8(info.competitive_tier);
    if tier < UNRANKED_BELOW {
        return None;
    }
    Some(CurrentRank {
        tier,
        rr: clamp_u16(info.ranked_rating),
    })
}

// ---------------------------------------------------------------------
// competitive updates -> CompUpdateRow
// ---------------------------------------------------------------------

/// `mmr/v1/players/{puuid}/competitiveupdates` -> one
/// `opd1_core::db::repo::CompUpdateRow` per match, same order as Riot
/// returned them. `tier_before` is always `Some` here (ruling R29) — Riot
/// always sends `TierBeforeUpdate` on this endpoint.
pub fn comp_update_rows_from_response(resp: &CompetitiveUpdatesResponse) -> Vec<CompUpdateRow> {
    resp.matches
        .iter()
        .map(|m| CompUpdateRow {
            match_id: m.match_id.clone(),
            season_id: m.season_id.clone(),
            tier_after: m.tier_after_update,
            tier_before: Some(m.tier_before_update),
            rr_after: m.ranked_rating_after_update,
            rr_earned: m.ranked_rating_earned,
            map_id: non_empty(&m.map_id),
            start_ms: m.match_start_time,
        })
        .collect()
}

// ---------------------------------------------------------------------
// match-details -> Match + MatchTeam + MatchPlayer
// ---------------------------------------------------------------------

/// Sums `headshots`/`bodyshots`/`legshots` landed *by* `subject`, across
/// every round's `playerStats[].damage[]` (CLAUDE.md's per-player hs/bs/ls
/// aggregation rule). A player who never appears in `round_results` (e.g.
/// the null-fields fixture, where it's empty) gets all zeros.
fn aggregate_shots(subject: &str, round_results: &[RoundResult]) -> (i64, i64, i64) {
    let mut hs = 0i64;
    let mut bs = 0i64;
    let mut ls = 0i64;
    for round in round_results {
        for player_stats in &round.player_stats {
            if player_stats.subject != subject {
                continue;
            }
            for damage in &player_stats.damage {
                hs += damage.headshots;
                bs += damage.bodyshots;
                ls += damage.legshots;
            }
        }
    }
    (hs, bs, ls)
}

/// Σ `damage` dealt by `subject` over every round's `playerStats`, or
/// `None` when the player appears in no round (no round results, or a
/// player without stats): unknown damage is not "0".
fn total_damage(subject: &str, round_results: &[RoundResult]) -> Option<i64> {
    let mut total = None;
    for round in round_results {
        for player_stats in round.player_stats.iter().filter(|p| p.subject == subject) {
            let dealt: i64 = player_stats.damage.iter().map(|d| d.damage).sum();
            *total.get_or_insert(0) += dealt;
        }
    }
    total
}

/// Queues whose halves are the first 12 rounds (competitive, unrated and
/// premier). Swiftplay and Spike Rush swap sides after 3-4 rounds and
/// Deathmatch has no sides, so "rounds 0..=11" would be meaningless there.
const TWELVE_ROUND_HALF_QUEUES: [&str; 3] = ["competitive", "unrated", "premier"];

/// A team's wins among rounds `roundNum` 0..=11, or `None` when the queue
/// has no such halves or the payload carries no round results.
fn first_half_wins(queue_id: &str, team_id: &str, round_results: &[RoundResult]) -> Option<i64> {
    if !TWELVE_ROUND_HALF_QUEUES.contains(&queue_id) || round_results.is_empty() {
        return None;
    }
    let wins = round_results
        .iter()
        .filter(|r| r.round_num.is_some_and(|n| n < 12))
        .filter(|r| r.winning_team.as_deref() == Some(team_id))
        .count();
    Some(wins as i64)
}

/// The single winning team id, or `None` for a draw/no-winner (ruling
/// R29) — including the degenerate "more than one team marked `won`"
/// case, which shouldn't happen in real data but shouldn't be reported as
/// a winner either.
fn winning_team_id(teams: &[crate::endpoints::MatchTeam]) -> Option<String> {
    let mut winners = teams.iter().filter(|t| t.won).map(|t| t.team_id.as_str());
    let first = winners.next()?;
    match winners.next() {
        None => Some(first.to_string()),
        Some(_) => None,
    }
}

/// `match-details/v1/matches/{id}` -> a `Match` row plus its teams/players,
/// ready for `Repo::upsert_match`.
///
/// Fix-round item 3 (reverses the original Task 5a design): `Match`'s
/// `match_id`/`map_id`/`queue_id`/`start_ms`/`is_ranked` all come straight
/// from `details.match_info` now, not from caller-supplied parameters —
/// match-details is self-sufficient, and threading those four fields
/// through from a separate, earlier call (the original design's rationale)
/// is exactly the kind of cross-call coupling that's easy to get wrong.
/// Phase 2 adds the game length, each player's rank during the match and
/// damage dealt, and each team's first-half rounds (see the helpers above).
/// `winning_team` cross-checks `matchInfo.completionState`: a
/// `"VoteDraw"` completion always means no winner, even if `teams`
/// somehow disagreed (e.g. reported a spurious single `won: true`).
pub fn match_from_details(
    details: &MatchDetailsResponse,
) -> (Match, Vec<MatchTeam>, Vec<MatchPlayer>) {
    let info = &details.match_info;

    let rounds_played = details
        .players
        .iter()
        .filter_map(|p| p.stats.as_ref().map(|s| s.rounds_played))
        .max();

    let winning_team = if info.completion_state == "VoteDraw" {
        None
    } else {
        winning_team_id(&details.teams)
    };

    let m = Match {
        match_id: info.match_id.clone(),
        map_id: info.map_id.clone(),
        queue_id: info.queue_id.clone(),
        start_ms: info.game_start_millis,
        winning_team,
        is_ranked: info.is_ranked,
        rounds_played,
        game_length_ms: info.game_length_millis,
    };

    let teams: Vec<MatchTeam> = details
        .teams
        .iter()
        .map(|t| MatchTeam {
            team_id: t.team_id.clone(),
            won: t.won,
            rounds_won: t.rounds_won,
            rounds_won_h1: first_half_wins(&info.queue_id, &t.team_id, &details.round_results),
        })
        .collect();

    let players: Vec<MatchPlayer> = details
        .players
        .iter()
        .map(|p| {
            let (hs, bs, ls) = aggregate_shots(&p.subject, &details.round_results);
            let (kills, deaths, assists, score, rounds) = match &p.stats {
                Some(s) => (s.kills, s.deaths, s.assists, s.score, Some(s.rounds_played)),
                None => (0, 0, 0, 0, None),
            };
            MatchPlayer {
                puuid: p.subject.clone(),
                team_id: p.team_id.clone(),
                party_id: non_empty(&p.party_id),
                agent_id: non_empty(&p.character_id),
                kills,
                deaths,
                assists,
                score,
                rounds,
                hs,
                bs,
                ls,
                game_name: non_empty(&p.game_name),
                tag_line: non_empty(&p.tag_line),
                competitive_tier: p.competitive_tier,
                damage: total_damage(&p.subject, &details.round_results),
            }
        })
        .collect();

    (m, teams, players)
}

// ---------------------------------------------------------------------
// valorant-api /v1/seasons -> ordered Vec<Act>
// ---------------------------------------------------------------------

/// One entry of valorant-api.com's `/v1/seasons` response — episodes and
/// acts share this shape; an episode has `parentUuid: null` and
/// `type: null`, an act has both set (`type` is always the literal string
/// `"EAresSeasonType::Act"` for an act, confirmed against a live
/// `valorant-api.com/v1/seasons` response and `crates/core/tests/fixtures/
/// seasons.json`).
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct SeasonEntry {
    pub uuid: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
    #[serde(rename = "type")]
    pub season_type: Option<String>,
    #[serde(rename = "startTime")]
    pub start_time: String,
    #[serde(rename = "endTime")]
    pub end_time: String,
    #[serde(rename = "parentUuid")]
    pub parent_uuid: Option<String>,
}

const ACT_TYPE: &str = "EAresSeasonType::Act";

/// Parses a `YYYY-MM-DDTHH:MM:SSZ` timestamp (the only shape
/// valorant-api.com's `/v1/seasons` has ever used) into epoch
/// milliseconds. `None` on anything that doesn't parse as RFC3339 — a
/// caller should skip that entry rather than fail the whole act list over
/// one bad timestamp.
fn parse_iso8601_utc_ms(s: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.timestamp_millis())
}

/// Filters `seasons` down to acts (`type == "EAresSeasonType::Act"`),
/// resolves each one's parent episode name via `parentUuid`, and returns
/// them sorted ascending by `start_ms` — the precondition
/// `opd1_core::stats::peak`/`prev_act_tier` both document. An act whose
/// parent isn't found in `seasons`, or whose `startTime`/`endTime` doesn't
/// parse, is skipped rather than failing the whole conversion.
pub fn acts_from_seasons(seasons: &[SeasonEntry]) -> Vec<Act> {
    let mut acts: Vec<Act> = seasons
        .iter()
        .filter(|s| s.season_type.as_deref() == Some(ACT_TYPE))
        .filter_map(|s| {
            let parent_uuid = s.parent_uuid.as_ref()?;
            let parent = seasons.iter().find(|e| &e.uuid == parent_uuid)?;
            let start_ms = parse_iso8601_utc_ms(&s.start_time)?;
            let end_ms = parse_iso8601_utc_ms(&s.end_time)?;
            Some(Act {
                id: s.uuid.clone(),
                name: s.display_name.clone(),
                episode_name: parent.display_name.clone(),
                start_ms,
                end_ms,
            })
        })
        .collect();

    acts.sort_by_key(|a| a.start_ms);
    acts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_iso8601_utc_ms_parses_seasons_json_shape() {
        assert_eq!(
            parse_iso8601_utc_ms("2022-06-21T00:00:00Z"),
            Some(1_655_769_600_000)
        );
    }

    #[test]
    fn parse_iso8601_utc_ms_none_for_garbage() {
        assert_eq!(parse_iso8601_utc_ms("not a date"), None);
    }

    #[test]
    fn clamp_helpers_saturate_instead_of_panicking() {
        assert_eq!(clamp_u8(-1), 0);
        assert_eq!(clamp_u8(1000), u8::MAX);
        assert_eq!(clamp_u16(-1), 0);
        assert_eq!(clamp_u32(-1), 0);
    }

    #[test]
    fn winning_team_id_none_when_no_team_won() {
        assert_eq!(winning_team_id(&[]), None);
    }

    #[test]
    fn winning_team_id_none_when_more_than_one_team_won() {
        let teams = vec![
            crate::endpoints::MatchTeam {
                team_id: "Blue".into(),
                won: true,
                rounds_won: Some(13),
            },
            crate::endpoints::MatchTeam {
                team_id: "Red".into(),
                won: true,
                rounds_won: Some(13),
            },
        ];
        assert_eq!(winning_team_id(&teams), None);
    }
}
