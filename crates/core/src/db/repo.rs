//! The typed sync repository: every read/write `opd1-core` needs against
//! the schema in `schema.sql`, wrapped around a [`Connection`] so callers
//! never hand-write SQL.
//!
//! "Sync, don't refetch" (CLAUDE.md): [`Repo::new_match_ids`] diffs a
//! fetched history list against what's already stored so the caller only
//! fetches genuinely new match-details; [`Repo::needs_mmr_refresh`] does
//! the same for the (rate-limited) MMR call.

use std::collections::HashMap;
use std::ops::Range;

use rusqlite::{params, Connection, OptionalExtension, Result};

use crate::names::KnownName;

/// Wraps a [`Connection`] (from [`super::open_db`]) with the app's typed
/// queries. Methods take `&self` — SQLite happily serves reads and writes
/// through a shared connection, and `rusqlite::Connection`'s own methods
/// are all `&self` too.
pub struct Repo {
    conn: Connection,
}

impl Repo {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    /// Escape hatch for callers (and tests) that need a query this repo
    /// doesn't expose a typed method for.
    pub fn conn(&self) -> &Connection {
        &self.conn
    }
}

// ---------------------------------------------------------------------
// players
// ---------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Player {
    pub puuid: String,
    pub game_name: String,
    pub tag_line: String,
    pub account_level: i64,
    pub card_id: String,
    pub updated_ms: i64,
}

impl Repo {
    pub fn upsert_player(&self, player: &Player) -> Result<()> {
        self.conn.execute(
            "INSERT INTO players (puuid, game_name, tag_line, account_level, card_id, updated_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(puuid) DO UPDATE SET
                game_name = excluded.game_name,
                tag_line = excluded.tag_line,
                account_level = excluded.account_level,
                card_id = excluded.card_id,
                updated_ms = excluded.updated_ms",
            params![
                player.puuid,
                player.game_name,
                player.tag_line,
                player.account_level,
                player.card_id,
                player.updated_ms
            ],
        )?;
        Ok(())
    }
}

// ---------------------------------------------------------------------
// matches / match_teams / match_players
// ---------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    pub match_id: String,
    pub map_id: String,
    pub queue_id: String,
    pub start_ms: i64,
    /// `None` for a draw / no team winner (ruling R29).
    pub winning_team: Option<String>,
    pub is_ranked: bool,
    pub rounds_played: Option<i64>,
    /// `matchInfo.gameLengthMillis` (migration 2); `None` when the payload
    /// lacks it or the row was stored before Phase 2.
    pub game_length_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchTeam {
    pub team_id: String,
    pub won: bool,
    pub rounds_won: Option<i64>,
    /// Rounds won in rounds `roundNum` 0..=11, the first half (migration
    /// 2). Filled only for competitive, unrated and premier, whose halves
    /// are 12 rounds; `None` for any other queue or without round results.
    pub rounds_won_h1: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchPlayer {
    pub puuid: String,
    pub team_id: String,
    pub party_id: Option<String>,
    pub agent_id: Option<String>,
    pub kills: i64,
    pub deaths: i64,
    pub assists: i64,
    pub score: i64,
    pub rounds: Option<i64>,
    pub hs: i64,
    pub bs: i64,
    pub ls: i64,
    /// The name as shown *in this match's own details* — never fetched
    /// from name-service for an incognito player. Feeds [`Repo::known_name`].
    pub game_name: Option<String>,
    pub tag_line: Option<String>,
    /// Riot's rank for the player DURING the match, as stored (migration 2;
    /// `players[].competitiveTier`, `0` when unranked). [`Repo::match_rows`]
    /// returns it with the R81 fallback already applied.
    pub competitive_tier: Option<i64>,
    /// Σ `roundResults[].playerStats[subject].damage[].damage` (migration
    /// 2); `None` without round results. ADR is this over `rounds`.
    pub damage: Option<i64>,
}

/// A [`MatchPlayer`] joined back with its match's basics — what
/// [`Repo::last_n_comp`] returns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchPlayerRow {
    pub match_id: String,
    pub map_id: String,
    pub start_ms: i64,
    pub winning_team: Option<String>,
    pub team_id: String,
    pub party_id: Option<String>,
    pub agent_id: Option<String>,
    pub kills: i64,
    pub deaths: i64,
    pub assists: i64,
    pub score: i64,
    pub rounds: Option<i64>,
    pub hs: i64,
    pub bs: i64,
    pub ls: i64,
    pub game_name: Option<String>,
    pub tag_line: Option<String>,
}

/// One of a player's competitive matches with everything a match row in
/// the UI shows: the stat line, the round score and the RR change — what
/// [`Repo::comp_matches`] returns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompMatchRow {
    pub match_id: String,
    /// As stored (a Riot map path); callers convert it to a valorant-api
    /// uuid before it reaches the UI.
    pub map_id: String,
    pub start_ms: i64,
    /// `None` for a draw / no team winner (ruling R29).
    pub winning_team: Option<String>,
    pub team_id: String,
    pub agent_id: Option<String>,
    pub kills: i64,
    pub deaths: i64,
    pub assists: i64,
    pub score: i64,
    pub rounds: Option<i64>,
    pub hs: i64,
    pub bs: i64,
    pub ls: i64,
    /// The player's team's rounds won.
    pub rounds_won: Option<i64>,
    /// The other team's rounds won, when there are exactly two teams.
    pub rounds_lost: Option<i64>,
    /// The player's own `comp_updates` row for the match, if stored.
    pub rr_earned: Option<i64>,
    /// That same row's Riot `SeasonID` — which act the match counted for
    /// (`None` without a row).
    pub season_id: Option<String>,
    /// That same row's tiers; below 3 is unranked (placements).
    pub tier_before: Option<i64>,
    /// The `comp_updates` row's rank AFTER the match (My Stats' RR math).
    /// It differs from [`CompMatchRow::tier`] on a promotion or demotion
    /// game; never use one for the other.
    pub tier_after: Option<i64>,
    /// The player's rank DURING the match, for display (recent rows,
    /// encounters, scoreboard): `COALESCE(mp.competitive_tier,
    /// cu.tier_after)`, Riot's at-match value first (R81).
    pub tier: Option<i64>,
}

impl Repo {
    /// Inserts or overwrites a match and all of its teams/players in one
    /// transaction. Match details are immutable in practice (CLAUDE.md),
    /// but the call itself is a genuine upsert so replaying it is always
    /// safe.
    pub fn upsert_match(
        &self,
        m: &Match,
        teams: &[MatchTeam],
        players: &[MatchPlayer],
    ) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;

        tx.execute(
            "INSERT INTO matches (match_id, map_id, queue_id, start_ms, winning_team, is_ranked, rounds_played, game_length_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(match_id) DO UPDATE SET
                map_id = excluded.map_id,
                queue_id = excluded.queue_id,
                start_ms = excluded.start_ms,
                winning_team = excluded.winning_team,
                is_ranked = excluded.is_ranked,
                rounds_played = excluded.rounds_played,
                game_length_ms = excluded.game_length_ms",
            params![
                m.match_id,
                m.map_id,
                m.queue_id,
                m.start_ms,
                m.winning_team,
                m.is_ranked,
                m.rounds_played,
                m.game_length_ms
            ],
        )?;

        for t in teams {
            tx.execute(
                "INSERT INTO match_teams (match_id, team_id, won, rounds_won, rounds_won_h1)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(match_id, team_id) DO UPDATE SET
                    won = excluded.won,
                    rounds_won = excluded.rounds_won,
                    rounds_won_h1 = excluded.rounds_won_h1",
                params![m.match_id, t.team_id, t.won, t.rounds_won, t.rounds_won_h1],
            )?;
        }

        for p in players {
            tx.execute(
                "INSERT INTO match_players
                    (match_id, puuid, team_id, party_id, agent_id, kills, deaths, assists, score, rounds, hs, bs, ls, game_name, tag_line, competitive_tier, damage)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
                 ON CONFLICT(match_id, puuid) DO UPDATE SET
                    team_id = excluded.team_id,
                    party_id = excluded.party_id,
                    agent_id = excluded.agent_id,
                    kills = excluded.kills,
                    deaths = excluded.deaths,
                    assists = excluded.assists,
                    score = excluded.score,
                    rounds = excluded.rounds,
                    hs = excluded.hs,
                    bs = excluded.bs,
                    ls = excluded.ls,
                    game_name = excluded.game_name,
                    tag_line = excluded.tag_line,
                    competitive_tier = excluded.competitive_tier,
                    damage = excluded.damage",
                params![
                    m.match_id,
                    p.puuid,
                    p.team_id,
                    p.party_id,
                    p.agent_id,
                    p.kills,
                    p.deaths,
                    p.assists,
                    p.score,
                    p.rounds,
                    p.hs,
                    p.bs,
                    p.ls,
                    p.game_name,
                    p.tag_line,
                    p.competitive_tier,
                    p.damage
                ],
            )?;
        }

        tx.commit()
    }

    /// Ruling R19: drops the `puuid` param from the brief's signature —
    /// match details are global per match, not per player — and returns
    /// the ids from `ids` absent from `matches`, preserving input order.
    pub fn new_match_ids(&self, ids: &[String]) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT 1 FROM matches WHERE match_id = ?1")?;
        let mut out = Vec::new();
        for id in ids {
            if !stmt.exists(params![id])? {
                out.push(id.clone());
            }
        }
        Ok(out)
    }

    /// The player's rows in matches with `queue_id = 'competitive'`,
    /// newest first.
    pub fn last_n_comp(&self, puuid: &str, n: u32) -> Result<Vec<MatchPlayerRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT m.match_id, m.map_id, m.start_ms, m.winning_team,
                    mp.team_id, mp.party_id, mp.agent_id, mp.kills, mp.deaths, mp.assists,
                    mp.score, mp.rounds, mp.hs, mp.bs, mp.ls, mp.game_name, mp.tag_line
             FROM match_players mp
             JOIN matches m ON m.match_id = mp.match_id
             WHERE mp.puuid = ?1 AND m.queue_id = 'competitive'
             ORDER BY m.start_ms DESC
             LIMIT ?2",
        )?;

        let rows = stmt.query_map(params![puuid, n], |row| {
            Ok(MatchPlayerRow {
                match_id: row.get(0)?,
                map_id: row.get(1)?,
                start_ms: row.get(2)?,
                winning_team: row.get(3)?,
                team_id: row.get(4)?,
                party_id: row.get(5)?,
                agent_id: row.get(6)?,
                kills: row.get(7)?,
                deaths: row.get(8)?,
                assists: row.get(9)?,
                score: row.get(10)?,
                rounds: row.get(11)?,
                hs: row.get(12)?,
                bs: row.get(13)?,
                ls: row.get(14)?,
                game_name: row.get(15)?,
                tag_line: row.get(16)?,
            })
        })?;

        rows.collect()
    }

    /// The player's competitive matches with `start_ms` in `range`,
    /// newest first, at most `limit`. See [`COMP_MATCHES_SQL`] for how the
    /// rows are read (R88).
    pub fn comp_matches(
        &self,
        puuid: &str,
        range: Range<i64>,
        limit: u32,
    ) -> Result<Vec<CompMatchRow>> {
        let mut stmt = self.conn.prepare_cached(COMP_MATCHES_SQL)?;
        let rows = stmt.query_map(params![puuid, range.start, range.end, limit], |row| {
            Ok(CompMatchRow {
                match_id: row.get(0)?,
                map_id: row.get(1)?,
                start_ms: row.get(2)?,
                winning_team: row.get(3)?,
                team_id: row.get(4)?,
                agent_id: row.get(5)?,
                kills: row.get(6)?,
                deaths: row.get(7)?,
                assists: row.get(8)?,
                score: row.get(9)?,
                rounds: row.get(10)?,
                hs: row.get(11)?,
                bs: row.get(12)?,
                ls: row.get(13)?,
                rounds_won: row.get(14)?,
                rounds_lost: row.get(15)?,
                rr_earned: row.get(16)?,
                season_id: row.get(17)?,
                tier_before: row.get(18)?,
                tier_after: row.get(19)?,
                tier: row.get(20)?,
            })
        })?;
        rows.collect()
    }
}

/// The query behind [`Repo::comp_matches`]; parameters `?1` puuid, `?2`
/// range start, `?3` range end (exclusive), `?4` limit.
///
/// R88: the player's competitive rows in the range are limited first
/// (`mine`: newest first, `LIMIT ?4`), and only those few rows are joined
/// to `match_teams` (both teams, aggregated per match: the player's team's
/// rounds won, and the other team's when there is exactly one) and to the
/// player's `comp_updates` row (by its primary key). No correlated
/// subquery, so the read cost grows with `limit`, not with the stored
/// history. Exposed for `EXPLAIN QUERY PLAN` in `tests/db.rs`.
#[doc(hidden)]
pub const COMP_MATCHES_SQL: &str = "\
WITH mine AS (
    SELECT m.match_id, m.map_id, m.start_ms, m.winning_team,
           mp.puuid, mp.team_id, mp.agent_id, mp.kills, mp.deaths, mp.assists,
           mp.score, mp.rounds, mp.hs, mp.bs, mp.ls, mp.competitive_tier
    FROM match_players mp
    JOIN matches m ON m.match_id = mp.match_id
    WHERE mp.puuid = ?1 AND m.queue_id = 'competitive'
      AND m.start_ms >= ?2 AND m.start_ms < ?3
    ORDER BY m.start_ms DESC
    LIMIT ?4
)
SELECT mine.match_id, mine.map_id, mine.start_ms, mine.winning_team,
       mine.team_id, mine.agent_id, mine.kills, mine.deaths, mine.assists,
       mine.score, mine.rounds, mine.hs, mine.bs, mine.ls,
       MAX(CASE WHEN t.team_id = mine.team_id THEN t.rounds_won END),
       CASE WHEN COUNT(CASE WHEN t.team_id != mine.team_id THEN 1 END) = 1
            THEN MAX(CASE WHEN t.team_id != mine.team_id THEN t.rounds_won END) END,
       cu.rr_earned, cu.season_id, cu.tier_before, cu.tier_after,
       COALESCE(mine.competitive_tier, cu.tier_after)
FROM mine
LEFT JOIN match_teams t ON t.match_id = mine.match_id
LEFT JOIN comp_updates cu ON cu.puuid = mine.puuid AND cu.match_id = mine.match_id
GROUP BY mine.match_id
ORDER BY mine.start_ms DESC";

/// One stored match with all of its teams and players — what
/// [`Repo::match_rows`] returns for Match Details.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredMatch {
    pub info: Match,
    pub teams: Vec<MatchTeam>,
    /// Ordered by puuid. Each player's `competitive_tier` is
    /// `COALESCE(mp.competitive_tier, cu.tier_after)` (R81): Riot's
    /// at-match rank, else that player's own RR row's tier.
    pub players: Vec<MatchPlayer>,
}

impl Repo {
    /// A stored match with its teams and players, or `None` when the match
    /// isn't in the DB. Three plain key lookups, no aggregation.
    pub fn match_rows(&self, match_id: &str) -> Result<Option<StoredMatch>> {
        let info = self
            .conn
            .query_row(
                "SELECT match_id, map_id, queue_id, start_ms, winning_team, is_ranked,
                        rounds_played, game_length_ms
                 FROM matches WHERE match_id = ?1",
                params![match_id],
                |row| {
                    Ok(Match {
                        match_id: row.get(0)?,
                        map_id: row.get(1)?,
                        queue_id: row.get(2)?,
                        start_ms: row.get(3)?,
                        winning_team: row.get(4)?,
                        is_ranked: row.get(5)?,
                        rounds_played: row.get(6)?,
                        game_length_ms: row.get(7)?,
                    })
                },
            )
            .optional()?;
        let Some(info) = info else {
            return Ok(None);
        };

        let mut stmt = self.conn.prepare(
            "SELECT team_id, won, rounds_won, rounds_won_h1
             FROM match_teams WHERE match_id = ?1 ORDER BY team_id",
        )?;
        let teams = stmt
            .query_map(params![match_id], |row| {
                Ok(MatchTeam {
                    team_id: row.get(0)?,
                    won: row.get(1)?,
                    rounds_won: row.get(2)?,
                    rounds_won_h1: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>>>()?;

        let mut stmt = self.conn.prepare(
            "SELECT mp.puuid, mp.team_id, mp.party_id, mp.agent_id, mp.kills, mp.deaths,
                    mp.assists, mp.score, mp.rounds, mp.hs, mp.bs, mp.ls, mp.game_name,
                    mp.tag_line, COALESCE(mp.competitive_tier, cu.tier_after), mp.damage
             FROM match_players mp
             LEFT JOIN comp_updates cu ON cu.puuid = mp.puuid AND cu.match_id = mp.match_id
             WHERE mp.match_id = ?1
             ORDER BY mp.puuid",
        )?;
        let players = stmt
            .query_map(params![match_id], |row| {
                Ok(MatchPlayer {
                    puuid: row.get(0)?,
                    team_id: row.get(1)?,
                    party_id: row.get(2)?,
                    agent_id: row.get(3)?,
                    kills: row.get(4)?,
                    deaths: row.get(5)?,
                    assists: row.get(6)?,
                    score: row.get(7)?,
                    rounds: row.get(8)?,
                    hs: row.get(9)?,
                    bs: row.get(10)?,
                    ls: row.get(11)?,
                    game_name: row.get(12)?,
                    tag_line: row.get(13)?,
                    competitive_tier: row.get(14)?,
                    damage: row.get(15)?,
                })
            })?
            .collect::<Result<Vec<_>>>()?;

        Ok(Some(StoredMatch {
            info,
            teams,
            players,
        }))
    }
}

// ---------------------------------------------------------------------
// history_index
// ---------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryEntry {
    pub match_id: String,
    pub queue_id: String,
    pub start_ms: i64,
}

impl Repo {
    pub fn index_history(&self, puuid: &str, entries: &[HistoryEntry]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        for e in entries {
            tx.execute(
                "INSERT INTO history_index (puuid, match_id, queue_id, start_ms)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(puuid, match_id) DO UPDATE SET
                    queue_id = excluded.queue_id,
                    start_ms = excluded.start_ms",
                params![puuid, e.match_id, e.queue_id, e.start_ms],
            )?;
        }
        tx.commit()
    }
}

// ---------------------------------------------------------------------
// comp_updates
// ---------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompUpdateRow {
    pub match_id: String,
    pub season_id: String,
    pub tier_after: i64,
    /// Ruling R29 addition — lets a caller show a per-match tier delta
    /// without re-deriving it from consecutive rows.
    pub tier_before: Option<i64>,
    pub rr_after: i64,
    pub rr_earned: i64,
    pub map_id: Option<String>,
    pub start_ms: i64,
}

impl Repo {
    pub fn upsert_comp_updates(&self, puuid: &str, rows: &[CompUpdateRow]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        for r in rows {
            tx.execute(
                "INSERT INTO comp_updates
                    (puuid, match_id, season_id, tier_after, tier_before, rr_after, rr_earned, map_id, start_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT(puuid, match_id) DO UPDATE SET
                    season_id = excluded.season_id,
                    tier_after = excluded.tier_after,
                    tier_before = excluded.tier_before,
                    rr_after = excluded.rr_after,
                    rr_earned = excluded.rr_earned,
                    map_id = excluded.map_id,
                    start_ms = excluded.start_ms",
                params![
                    puuid,
                    r.match_id,
                    r.season_id,
                    r.tier_after,
                    r.tier_before,
                    r.rr_after,
                    r.rr_earned,
                    r.map_id,
                    r.start_ms
                ],
            )?;
        }
        tx.commit()
    }

    /// A player's competitive-update rows for one season, newest first.
    pub fn comp_updates_for_season(
        &self,
        puuid: &str,
        season_id: &str,
    ) -> Result<Vec<CompUpdateRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT match_id, season_id, tier_after, tier_before, rr_after, rr_earned, map_id, start_ms
             FROM comp_updates
             WHERE puuid = ?1 AND season_id = ?2
             ORDER BY start_ms DESC",
        )?;
        let rows = stmt.query_map(params![puuid, season_id], |row| {
            Ok(CompUpdateRow {
                match_id: row.get(0)?,
                season_id: row.get(1)?,
                tier_after: row.get(2)?,
                tier_before: row.get(3)?,
                rr_after: row.get(4)?,
                rr_earned: row.get(5)?,
                map_id: row.get(6)?,
                start_ms: row.get(7)?,
            })
        })?;
        rows.collect()
    }
}

// ---------------------------------------------------------------------
// mmr_snapshots
// ---------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MmrSnapshot {
    pub puuid: String,
    pub json: String,
    pub last_comp_match_id: Option<String>,
    pub fetched_ms: i64,
}

impl Repo {
    pub fn save_mmr_snapshot(&self, snap: &MmrSnapshot) -> Result<()> {
        self.conn.execute(
            "INSERT INTO mmr_snapshots (puuid, json, last_comp_match_id, fetched_ms)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(puuid) DO UPDATE SET
                json = excluded.json,
                last_comp_match_id = excluded.last_comp_match_id,
                fetched_ms = excluded.fetched_ms",
            params![
                snap.puuid,
                snap.json,
                snap.last_comp_match_id,
                snap.fetched_ms
            ],
        )?;
        Ok(())
    }

    pub fn load_mmr_snapshot(&self, puuid: &str) -> Result<Option<MmrSnapshot>> {
        self.conn
            .query_row(
                "SELECT puuid, json, last_comp_match_id, fetched_ms FROM mmr_snapshots WHERE puuid = ?1",
                params![puuid],
                |row| {
                    Ok(MmrSnapshot {
                        puuid: row.get(0)?,
                        json: row.get(1)?,
                        last_comp_match_id: row.get(2)?,
                        fetched_ms: row.get(3)?,
                    })
                },
            )
            .optional()
    }

    /// True when there's no snapshot yet, or its `last_comp_match_id`
    /// differs from `latest_comp_match_id` — i.e. a new competitive match
    /// has landed since the last MMR fetch, so rank may have changed.
    pub fn needs_mmr_refresh(&self, puuid: &str, latest_comp_match_id: &str) -> Result<bool> {
        let stored = self.load_mmr_snapshot(puuid)?;
        Ok(match stored {
            None => true,
            Some(snap) => snap.last_comp_match_id.as_deref() != Some(latest_comp_match_id),
        })
    }
}

// ---------------------------------------------------------------------
// encounters
// ---------------------------------------------------------------------

/// One past match shared with `other` (with or against), newest first.
/// Scores, agents and tiers are from the user's (`self`'s) side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncounterRow {
    pub match_id: String,
    pub map_id: String,
    pub start_ms: i64,
    /// `true` when `self` and `other` were on the same team.
    pub with: bool,
    /// `None` for a draw / no team winner.
    pub self_won: Option<bool>,
    /// The user's team's rounds won.
    pub rounds_won: Option<i64>,
    /// The other team's rounds won, when there is exactly one other team
    /// (`None` in modes with more or no team rows).
    pub rounds_lost: Option<i64>,
    pub game_length_ms: Option<i64>,
    pub self_agent_id: Option<String>,
    pub other_agent_id: Option<String>,
    /// Each side's rank during the match, by the one R81 rule:
    /// `COALESCE(mp.competitive_tier, cu.tier_after)`.
    pub self_tier: Option<i64>,
    pub other_tier: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Encounters {
    pub with: u32,
    pub against: u32,
    pub matches: Vec<EncounterRow>,
}

impl Repo {
    /// Every match where both `self_puuid` and `other` have a
    /// `match_players` row, any mode. Deathmatch players are always on
    /// different team ids, so they naturally count as "against".
    ///
    /// The two teams of each match are aggregated with a join (the user's
    /// team's rounds, and the other team's when there is exactly one), not
    /// a per-row subquery; each side's RR row is looked up by its primary
    /// key. [`Repo::encounter_counts`] answers "how many" without reading
    /// any of this.
    pub fn encounters(&self, self_puuid: &str, other: &str) -> Result<Encounters> {
        let mut stmt = self.conn.prepare(
            "SELECT m.match_id, m.map_id, m.start_ms, mp_self.team_id, mp_other.team_id, m.winning_team,
                    MAX(CASE WHEN t.team_id = mp_self.team_id THEN t.rounds_won END),
                    CASE WHEN COUNT(CASE WHEN t.team_id != mp_self.team_id THEN 1 END) = 1
                         THEN MAX(CASE WHEN t.team_id != mp_self.team_id THEN t.rounds_won END) END,
                    m.game_length_ms, mp_self.agent_id, mp_other.agent_id,
                    COALESCE(mp_self.competitive_tier, cu_self.tier_after),
                    COALESCE(mp_other.competitive_tier, cu_other.tier_after)
             FROM match_players mp_self
             JOIN match_players mp_other
               ON mp_other.match_id = mp_self.match_id AND mp_other.puuid = ?2
             JOIN matches m ON m.match_id = mp_self.match_id
             LEFT JOIN match_teams t ON t.match_id = m.match_id
             LEFT JOIN comp_updates cu_self
               ON cu_self.puuid = mp_self.puuid AND cu_self.match_id = m.match_id
             LEFT JOIN comp_updates cu_other
               ON cu_other.puuid = mp_other.puuid AND cu_other.match_id = m.match_id
             WHERE mp_self.puuid = ?1
             GROUP BY m.match_id
             ORDER BY m.start_ms DESC",
        )?;

        let rows = stmt.query_map(params![self_puuid, other], |row| {
            let self_team: String = row.get(3)?;
            let other_team: String = row.get(4)?;
            let winning_team: Option<String> = row.get(5)?;
            let with = self_team == other_team;
            let self_won = winning_team.map(|wt| wt == self_team);
            Ok(EncounterRow {
                match_id: row.get(0)?,
                map_id: row.get(1)?,
                start_ms: row.get(2)?,
                with,
                self_won,
                rounds_won: row.get(6)?,
                rounds_lost: row.get(7)?,
                game_length_ms: row.get(8)?,
                self_agent_id: row.get(9)?,
                other_agent_id: row.get(10)?,
                self_tier: row.get(11)?,
                other_tier: row.get(12)?,
            })
        })?;

        let matches: Vec<EncounterRow> = rows.collect::<Result<_>>()?;
        let with = matches.iter().filter(|r| r.with).count() as u32;
        let against = matches.len() as u32 - with;

        Ok(Encounters {
            with,
            against,
            matches,
        })
    }

    /// `(with, against)`: how many matches `self_puuid` shared with
    /// `other`, on the same team / not — one `COUNT` query over the two
    /// players' rows, for the ⟲ badge on every card.
    pub fn encounter_counts(&self, self_puuid: &str, other: &str) -> Result<(u32, u32)> {
        self.conn.query_row(
            "SELECT COALESCE(SUM(mp_self.team_id = mp_other.team_id), 0),
                    COALESCE(SUM(mp_self.team_id != mp_other.team_id), 0)
             FROM match_players mp_self
             JOIN match_players mp_other
               ON mp_other.match_id = mp_self.match_id AND mp_other.puuid = ?2
             WHERE mp_self.puuid = ?1",
            params![self_puuid, other],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
    }

    /// The most recent non-empty name `other` used in a match `self_puuid`
    /// also has a row in, with where and when that match was (the stored
    /// Riot map path — the shell converts it when it builds a
    /// `DisplayName` — and its start). Never widen this to a match
    /// `self_puuid` didn't play — this is the privacy boundary that lets
    /// a hidden player's name show once the user has legitimately learned
    /// it (CLAUDE.md's "Hidden names" exception), and it must never leak
    /// a name seen only in someone else's match history.
    pub fn known_name(&self, self_puuid: &str, other: &str) -> Result<Option<KnownName>> {
        self.conn
            .query_row(
                "SELECT mp_other.game_name, mp_other.tag_line, m.map_id, m.start_ms
                 FROM match_players mp_other
                 JOIN match_players mp_self
                   ON mp_self.match_id = mp_other.match_id AND mp_self.puuid = ?1
                 JOIN matches m ON m.match_id = mp_other.match_id
                 WHERE mp_other.puuid = ?2
                   AND mp_other.game_name IS NOT NULL AND mp_other.game_name != ''
                   AND mp_other.tag_line IS NOT NULL AND mp_other.tag_line != ''
                 ORDER BY m.start_ms DESC
                 LIMIT 1",
                params![self_puuid, other],
                |row| {
                    let map_id: String = row.get(2)?;
                    Ok(KnownName {
                        name: row.get(0)?,
                        tag: row.get(1)?,
                        map_id: (!map_id.is_empty()).then_some(map_id),
                        start_ms: Some(row.get(3)?),
                    })
                },
            )
            .optional()
    }
}

// ---------------------------------------------------------------------
// top_teammates
// ---------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeammateRow {
    pub puuid: String,
    pub game_name: Option<String>,
    pub tag_line: Option<String>,
    pub count: u32,
}

impl Repo {
    /// The current act's most frequent teammates: competitive matches
    /// with `start_ms >= season_start_ms`, other players on the same
    /// team, ordered by shared-match count desc (tie -> most recent
    /// shared match). Name comes from their most recent shared row and
    /// may be `None` (e.g. an incognito player never resolved).
    pub fn top_teammates(
        &self,
        puuid: &str,
        season_start_ms: i64,
        limit: usize,
    ) -> Result<Vec<TeammateRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT mp2.puuid, mp2.game_name, mp2.tag_line, m.start_ms
             FROM match_players mp1
             JOIN match_players mp2
               ON mp2.match_id = mp1.match_id
              AND mp2.team_id = mp1.team_id
              AND mp2.puuid != mp1.puuid
             JOIN matches m ON m.match_id = mp1.match_id
             WHERE mp1.puuid = ?1
               AND m.queue_id = 'competitive'
               AND m.start_ms >= ?2
             ORDER BY m.start_ms DESC",
        )?;

        struct Agg {
            count: u32,
            most_recent_ms: i64,
            game_name: Option<String>,
            tag_line: Option<String>,
        }

        let mut agg: HashMap<String, Agg> = HashMap::new();
        let rows = stmt.query_map(params![puuid, season_start_ms], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })?;

        // Rows arrive newest-first, so the first time a teammate is seen
        // is their most recent shared match — later (older) occurrences
        // must only bump the count, never overwrite the name/recency.
        for row in rows {
            let (teammate_puuid, game_name, tag_line, start_ms) = row?;
            agg.entry(teammate_puuid)
                .and_modify(|a| a.count += 1)
                .or_insert(Agg {
                    count: 1,
                    most_recent_ms: start_ms,
                    game_name,
                    tag_line,
                });
        }

        let mut rows: Vec<(String, Agg)> = agg.into_iter().collect();
        rows.sort_by(|a, b| {
            b.1.count
                .cmp(&a.1.count)
                .then(b.1.most_recent_ms.cmp(&a.1.most_recent_ms))
        });
        rows.truncate(limit);

        Ok(rows
            .into_iter()
            .map(|(puuid, a)| TeammateRow {
                puuid,
                game_name: a.game_name,
                tag_line: a.tag_line,
                count: a.count,
            })
            .collect())
    }
}

// ---------------------------------------------------------------------
// app_state (ruling R31)
// ---------------------------------------------------------------------

impl Repo {
    /// A value from the small `app_state` key/value table, or `None` when
    /// the key was never set (or was set to SQL `NULL`).
    pub fn get_state(&self, key: &str) -> Result<Option<String>> {
        self.conn
            .query_row(
                "SELECT value FROM app_state WHERE key = ?1",
                params![key],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()
            .map(Option::flatten)
    }

    /// Inserts or overwrites one `app_state` value.
    pub fn set_state(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO app_state (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }
}
