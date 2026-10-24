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

use crate::model::RememberedSource;
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
    /// from name-service for an incognito player. Feeds [`Repo::known_name`]
    /// (R145: any stored row with a full name is a known name).
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

/// [`Repo::last_n_comp`]'s query (`?1` the player, `?2` the limit): the
/// player's rows come from `idx_match_players_puuid`, each joins its match by
/// primary key, and the newest are picked by a sort, so the cost follows the
/// player's own stored matches (a few ms at several thousand, never the whole
/// table). Public so a test can `EXPLAIN QUERY PLAN` exactly this text.
pub const LAST_N_COMP_SQL: &str = "\
SELECT m.match_id, m.map_id, m.start_ms, m.winning_team,
       mp.team_id, mp.party_id, mp.agent_id, mp.kills, mp.deaths, mp.assists,
       mp.score, mp.rounds, mp.hs, mp.bs, mp.ls, mp.game_name, mp.tag_line
FROM match_players mp
JOIN matches m ON m.match_id = mp.match_id
WHERE mp.puuid = ?1 AND m.queue_id = 'competitive'
ORDER BY m.start_ms DESC
LIMIT ?2";

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
    /// newest first. See [`LAST_N_COMP_SQL`].
    pub fn last_n_comp(&self, puuid: &str, n: u32) -> Result<Vec<MatchPlayerRow>> {
        let mut stmt = self.conn.prepare(LAST_N_COMP_SQL)?;

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

/// R154: what a stored competitive match says about a player's rank when
/// MMR has no answer — see [`Repo::newest_ranked_match_tier`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoredTier {
    /// When the match the tier was read from started (epoch ms).
    pub start_ms: i64,
    /// The player's rank in that match (Riot's `competitiveTier`, `3..=255`).
    pub tier: u8,
}

/// [`Repo::newest_ranked_match_tier`]'s query (`?1` the player): the newest
/// competitive match in which the player's stored `competitive_tier` (the
/// tier Match Details shows) is a ranked one, i.e. at least 3 (0 is unranked,
/// 1 and 2 are unused / placements, `NULL` is a match stored before
/// migration 2). One index seek on `match_players(puuid)` and one primary-key
/// read of `matches` per stored match of the player — no scan. Public so a
/// test can `EXPLAIN QUERY PLAN` exactly this text.
pub const NEWEST_RANKED_MATCH_TIER_SQL: &str = "\
SELECT m.start_ms, mp.competitive_tier
FROM match_players mp
JOIN matches m ON m.match_id = mp.match_id
WHERE mp.puuid = ?1 AND m.queue_id = 'competitive' AND mp.competitive_tier >= 3
ORDER BY m.start_ms DESC
LIMIT 1";

impl Repo {
    /// R154, path (b): the newest competitive match of `puuid` that carries a
    /// ranked tier for them (see [`NEWEST_RANKED_MATCH_TIER_SQL`]), whatever
    /// act it was played in — the caller decides whether it is in the current
    /// act, so nothing here depends on the act tables. `None` when there is no
    /// such match (or its tier is not a tier at all: above 255).
    pub fn newest_ranked_match_tier(&self, puuid: &str) -> Result<Option<StoredTier>> {
        let row: Option<(i64, i64)> = self
            .conn
            .query_row(NEWEST_RANKED_MATCH_TIER_SQL, params![puuid], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .optional()?;
        Ok(row.and_then(|(start_ms, tier)| {
            Some(StoredTier {
                start_ms,
                tier: u8::try_from(tier).ok()?,
            })
        }))
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

/// [`Repo::comp_updates_for_season`]'s query (`?1` the player, `?2` the
/// season): `idx_comp_updates_puuid_start (puuid, start_ms)` hands the player's
/// rows over newest first (read backwards): no sort. `season_id` is filtered on
/// those rows, not seeked, so only the player's own rows are read. Public so a
/// test can `EXPLAIN QUERY PLAN` exactly this text.
pub const COMP_UPDATES_FOR_SEASON_SQL: &str = "\
SELECT match_id, season_id, tier_after, tier_before, rr_after, rr_earned, map_id, start_ms
FROM comp_updates
WHERE puuid = ?1 AND season_id = ?2
ORDER BY start_ms DESC";

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

    /// A player's competitive-update rows for one season, newest first. See
    /// [`COMP_UPDATES_FOR_SEASON_SQL`].
    pub fn comp_updates_for_season(
        &self,
        puuid: &str,
        season_id: &str,
    ) -> Result<Vec<CompUpdateRow>> {
        let mut stmt = self.conn.prepare(COMP_UPDATES_FOR_SEASON_SQL)?;
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

/// [`Repo::encounters`]' query (`?1` the user, `?2` the other player): the
/// competitive matches both played (R156, user 2026-09-30: Deathmatch and
/// every other queue are no encounter), newest first. The other player's
/// rows drive the query (`idx_match_players_puuid`), because the user is in
/// every match they played (thousands) and the other player in few: every
/// join after that is a key seek, the user's row, the match and each side's RR
/// row by primary key (B7: driven from the user's rows instead, a stranger
/// with a dozen matches cost a walk over all of the user's thousands). Public
/// so a test can `EXPLAIN QUERY PLAN` exactly this text.
//
// `CROSS JOIN` pins that order: SQLite never swaps the two tables around it,
// whatever the statistics (a plain `JOIN` is free to start from either).
pub const ENCOUNTERS_SQL: &str =
    "SELECT m.match_id, m.map_id, m.start_ms, mp_self.team_id, mp_other.team_id, m.winning_team,
            MAX(CASE WHEN t.team_id = mp_self.team_id THEN t.rounds_won END),
            CASE WHEN COUNT(CASE WHEN t.team_id != mp_self.team_id THEN 1 END) = 1
                 THEN MAX(CASE WHEN t.team_id != mp_self.team_id THEN t.rounds_won END) END,
            m.game_length_ms, mp_self.agent_id, mp_other.agent_id,
            COALESCE(mp_self.competitive_tier, cu_self.tier_after),
            COALESCE(mp_other.competitive_tier, cu_other.tier_after)
     FROM match_players mp_other
     CROSS JOIN match_players mp_self
       ON mp_self.match_id = mp_other.match_id AND mp_self.puuid = ?1
     JOIN matches m ON m.match_id = mp_other.match_id AND m.queue_id = 'competitive'
     LEFT JOIN match_teams t ON t.match_id = m.match_id
     LEFT JOIN comp_updates cu_self
       ON cu_self.puuid = mp_self.puuid AND cu_self.match_id = m.match_id
     LEFT JOIN comp_updates cu_other
       ON cu_other.puuid = mp_other.puuid AND cu_other.match_id = m.match_id
     WHERE mp_other.puuid = ?2
     GROUP BY m.match_id
     ORDER BY m.start_ms DESC";

/// [`Repo::encounter_counts`]' query (`?1` the user, `?2` the other player):
/// the same competitive-only rule as [`ENCOUNTERS_SQL`], driven the same way
/// (the other player's rows first), without reading anything else. Public for
/// the plan test.
//
// `CROSS JOIN` pins that order, as in [`ENCOUNTERS_SQL`].
pub const ENCOUNTER_COUNTS_SQL: &str =
    "SELECT COALESCE(SUM(mp_self.team_id = mp_other.team_id), 0),
            COALESCE(SUM(mp_self.team_id != mp_other.team_id), 0)
     FROM match_players mp_other
     CROSS JOIN match_players mp_self
       ON mp_self.match_id = mp_other.match_id AND mp_self.puuid = ?1
     JOIN matches m ON m.match_id = mp_other.match_id AND m.queue_id = 'competitive'
     WHERE mp_other.puuid = ?2";

impl Repo {
    /// Every competitive match where both `self_puuid` and `other` have a
    /// `match_players` row (R156: Deathmatch and the other queues are not
    /// encounters; party detection and remembered names still read them).
    ///
    /// The two teams of each match are aggregated with a join (the user's
    /// team's rounds, and the other team's when there is exactly one), not
    /// a per-row subquery; each side's RR row is looked up by its primary
    /// key. [`Repo::encounter_counts`] answers "how many" without reading
    /// any of this.
    pub fn encounters(&self, self_puuid: &str, other: &str) -> Result<Encounters> {
        let mut stmt = self.conn.prepare(ENCOUNTERS_SQL)?;

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

    /// `(with, against)`: how many competitive matches `self_puuid` shared
    /// with `other` (R156), on the same team / not — one `COUNT` query over
    /// the two players' rows, for the ⟲ badge on every card.
    pub fn encounter_counts(&self, self_puuid: &str, other: &str) -> Result<(u32, u32)> {
        self.conn
            .query_row(ENCOUNTER_COUNTS_SQL, params![self_puuid, other], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
    }

    /// The best name the DB knows for `other`, from any source, most recent
    /// first (R145): a stored `match_players` row with a full name (marked
    /// [`RememberedSource::OwnMatch`] when `self_puuid` also has a row in
    /// that match, else [`RememberedSource::OtherMatch`]; its map is the
    /// stored Riot path — the shell converts it when it builds a
    /// `DisplayName` — and its start is the match's), or the `players`
    /// row a name-service answer for a visible player stored
    /// ([`RememberedSource::Seen`]: no map, `start_ms` is `updated_ms`).
    /// On equal times own match beats seen beats other match.
    ///
    /// # Privacy boundary
    ///
    /// R145 (user decision, 2026-09-30, superseding R143's own-matches-only
    /// rule): a hidden player's name may be *shown* whenever the DB knows it
    /// from any of these sources — on the live screen in the remembered
    /// style, off it as a plain name. What never changes is what is
    /// *requested* during a match: no endpoint is ever asked for a hidden
    /// player's name in a live stage — during a match name-service is only
    /// called with a `VisiblePuuid` for the visible players of the roster.
    /// R153 (user, 2026-09-30): *between* matches the names fill (R150) does
    /// ask about players who were hidden in the match that just ended, because
    /// the game's own Match History shows their real names after it; Riot
    /// answers blank for anyone still hiding, so the `players` table only
    /// holds names Riot gave after the match. This function reads; it fetches
    /// nothing.
    ///
    /// One indexed query: seeks on `idx_match_players_puuid`, the primary
    /// keys of `matches` and `match_players`, and `players` (see
    /// [`KNOWN_NAME_SQL`]).
    pub fn known_name(&self, self_puuid: &str, other: &str) -> Result<Option<KnownName>> {
        self.conn
            .query_row(KNOWN_NAME_SQL, params![self_puuid, other], |row| {
                let map_id: Option<String> = row.get(2)?;
                let source = match row.get::<_, i64>(4)? {
                    0 => RememberedSource::OwnMatch,
                    1 => RememberedSource::Seen,
                    _ => RememberedSource::OtherMatch,
                };
                Ok(KnownName {
                    name: row.get(0)?,
                    tag: row.get(1)?,
                    map_id: map_id.filter(|m| !m.is_empty()),
                    start_ms: Some(row.get(3)?),
                    source,
                })
            })
            .optional()
    }

    /// `other`'s name in the `players` table (what a name-service answer for
    /// a visible player stored), when it is a full name: one primary-key
    /// lookup. Match Details shows it as a plain name for a row whose stored
    /// match name is empty and who is not hidden on the live screen (R145).
    pub fn seen_name(&self, other: &str) -> Result<Option<(String, String)>> {
        self.conn
            .query_row(
                "SELECT game_name, tag_line FROM players
                 WHERE puuid = ?1 AND game_name != '' AND tag_line != ''",
                params![other],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
    }
}

/// [`Repo::known_name`]'s query (`?1` the user, `?2` the other player): the
/// named `match_players` rows of `?2` (rank 0 when `?1` has a row in the
/// same match, else 2) and their `players` row (rank 1), newest first, ties
/// by rank. Public so a test can `EXPLAIN QUERY PLAN` exactly this text.
pub const KNOWN_NAME_SQL: &str = "SELECT name, tag, map_id, ts, rank FROM (
       SELECT mp.game_name AS name, mp.tag_line AS tag, m.map_id AS map_id,
              m.start_ms AS ts,
              CASE WHEN EXISTS (SELECT 1 FROM match_players mine
                                WHERE mine.match_id = mp.match_id AND mine.puuid = ?1)
                   THEN 0 ELSE 2 END AS rank
       FROM match_players mp
       JOIN matches m ON m.match_id = mp.match_id
       WHERE mp.puuid = ?2
         AND mp.game_name IS NOT NULL AND mp.game_name != ''
         AND mp.tag_line IS NOT NULL AND mp.tag_line != ''
       UNION ALL
       SELECT p.game_name, p.tag_line, NULL, p.updated_ms, 1
       FROM players p
       WHERE p.puuid = ?2 AND p.game_name != '' AND p.tag_line != ''
     )
     ORDER BY ts DESC, rank ASC
     LIMIT 1";

// ---------------------------------------------------------------------
// names fill (R150)
// ---------------------------------------------------------------------

/// One name-service question (R150): a puuid, and the start (epoch ms) of
/// the finished match that raised it — `None` for an on-demand question
/// (Player Details, Match Details, Top 3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameAsk {
    pub puuid: String,
    pub match_start_ms: Option<i64>,
}

/// [`Repo::names_to_ask`]'s per-puuid check (`?1` the puuid): whether the
/// `players` row has a full name, whether any stored `match_players` row has
/// one, and the puuid's marker (`players.name_asked_ms`). Three key seeks:
/// `players`' primary key twice and `idx_match_players_puuid`. Public so a
/// test can `EXPLAIN QUERY PLAN` exactly this text.
pub const NAMES_TO_ASK_SQL: &str = "SELECT
       EXISTS (SELECT 1 FROM players
               WHERE puuid = ?1 AND game_name != '' AND tag_line != ''),
       EXISTS (SELECT 1 FROM match_players
               WHERE puuid = ?1
                 AND game_name IS NOT NULL AND game_name != ''
                 AND tag_line IS NOT NULL AND tag_line != ''),
       (SELECT name_asked_ms FROM players WHERE puuid = ?1)";

impl Repo {
    /// The asks name-service should be sent (R150), in input order: the
    /// puuids with no known name — no full `players` name and no full
    /// `match_players` name — that a previous blank answer doesn't rule out.
    ///
    /// A blank answer is remembered as a marker, `players.name_asked_ms`: the
    /// start of the newest finished match whose question ended blank (0 for
    /// an on-demand one). Name-service answers with the player's status now,
    /// so an ask raised by a finished match (`match_start_ms: Some`) is
    /// dropped when the marker is at least that match's start — the same
    /// match again, or an older one: nothing changed since — and sent when
    /// the match is newer: the next finished match the puuid is in asks
    /// again, and there is no timer. An on-demand ask (`None`) is dropped by
    /// any marker: it only covers puuids never asked before.
    ///
    /// This decides only what is *worth* asking. Who may be asked at all (not
    /// the user; nothing during a match, R153) is the caller's rule.
    pub fn names_to_ask(&self, asks: &[NameAsk]) -> Result<Vec<NameAsk>> {
        let mut stmt = self.conn.prepare(NAMES_TO_ASK_SQL)?;
        let mut out = Vec::new();
        for a in asks {
            let (in_players, in_matches, marker): (bool, bool, Option<i64>) = stmt
                .query_row(params![a.puuid], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                })?;
            let covered = in_players
                || in_matches
                || match (a.match_start_ms, marker) {
                    (_, None) => false,
                    (None, Some(_)) => true,
                    (Some(start), Some(marker)) => marker >= start,
                };
            if !covered {
                out.push(a.clone());
            }
        }
        Ok(out)
    }

    /// Stores what name-service answered (R150), in one transaction.
    /// `learned` is `(puuid, name, tag)` — a full name, kept in `players`
    /// with `updated_ms = now_ms` (its `Seen` kind in [`Self::known_name`]);
    /// the row's level and card are left alone. `blank` are the asks that got
    /// no name: each raises the marker (`name_asked_ms`) to its match's start
    /// (0 for an on-demand ask; the marker never goes down), and nothing else
    /// on the row moves — a blank answer never erases a name.
    pub fn record_name_answers(
        &self,
        learned: &[(String, String, String)],
        blank: &[NameAsk],
        now_ms: i64,
    ) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        for (puuid, name, tag) in learned {
            tx.execute(
                "INSERT INTO players (puuid, game_name, tag_line, account_level, card_id, updated_ms)
                 VALUES (?1, ?2, ?3, 0, '', ?4)
                 ON CONFLICT(puuid) DO UPDATE SET
                    game_name = excluded.game_name,
                    tag_line = excluded.tag_line,
                    updated_ms = excluded.updated_ms",
                params![puuid, name, tag, now_ms],
            )?;
        }
        for a in blank {
            tx.execute(
                "INSERT INTO players (puuid, game_name, tag_line, account_level, card_id, updated_ms, name_asked_ms)
                 VALUES (?1, '', '', 0, '', ?2, ?3)
                 ON CONFLICT(puuid) DO UPDATE SET
                    name_asked_ms = MAX(COALESCE(players.name_asked_ms, 0), excluded.name_asked_ms)",
                params![a.puuid, now_ms, a.match_start_ms.unwrap_or(0)],
            )?;
        }
        tx.commit()
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
    /// Of the `count` shared matches, the player's (`?1`'s) wins and losses
    /// (`matches.winning_team`; a draw — no winner — is neither).
    pub wins: u32,
    pub losses: u32,
}

/// [`Repo::top_teammates`]' query (`?1` the player, `?2` the earliest start):
/// the player's rows by `idx_match_players_puuid`, then every other table by
/// key (the teammates of one match are the `match_id` prefix of the primary
/// key); the newest-first order is a sort. Public so a test can `EXPLAIN QUERY
/// PLAN` exactly this text. Column 6 is the player's result in that match
/// (1 won, 0 lost, NULL a draw), read off the `matches` row already joined.
pub const TOP_TEAMMATES_SQL: &str = "\
SELECT mp2.puuid, mp2.game_name, mp2.tag_line, m.start_ms,
       pl.game_name, pl.tag_line,
       CASE WHEN m.winning_team IS NULL THEN NULL
            WHEN m.winning_team = mp1.team_id THEN 1 ELSE 0 END
FROM match_players mp1
JOIN match_players mp2
  ON mp2.match_id = mp1.match_id
 AND mp2.team_id = mp1.team_id
 AND mp2.puuid != mp1.puuid
JOIN matches m ON m.match_id = mp1.match_id
LEFT JOIN players pl ON pl.puuid = mp2.puuid
WHERE mp1.puuid = ?1
  AND m.queue_id = 'competitive'
  AND m.start_ms >= ?2
ORDER BY m.start_ms DESC";

impl Repo {
    /// The current act's most frequent teammates: competitive matches
    /// with `start_ms >= season_start_ms`, other players on the same
    /// team, ordered by shared-match count desc (tie -> most recent
    /// shared match). The name is the most recent full name among their
    /// shared rows (Riot's match-details can return them empty), else the
    /// `players` row a visible sighting stored (R145); `None` (both
    /// halves) when neither has a full name.
    pub fn top_teammates(
        &self,
        puuid: &str,
        season_start_ms: i64,
        limit: usize,
    ) -> Result<Vec<TeammateRow>> {
        let mut stmt = self.conn.prepare(TOP_TEAMMATES_SQL)?;

        /// `(name, tag)` when both halves are non-empty.
        fn full(name: Option<String>, tag: Option<String>) -> Option<(String, String)> {
            name.zip(tag)
                .filter(|(n, t)| !n.is_empty() && !t.is_empty())
        }

        struct Agg {
            count: u32,
            wins: u32,
            losses: u32,
            most_recent_ms: i64,
            /// The most recent shared row with a full name.
            match_name: Option<(String, String)>,
            /// The `players` row's full name (the same for every row).
            seen_name: Option<(String, String)>,
        }

        let mut agg: HashMap<String, Agg> = HashMap::new();
        let rows = stmt.query_map(params![puuid, season_start_ms], |row| {
            Ok((
                row.get::<_, String>(0)?,
                full(row.get(1)?, row.get(2)?),
                row.get::<_, i64>(3)?,
                full(row.get(4)?, row.get(5)?),
                row.get::<_, Option<bool>>(6)?,
            ))
        })?;

        // Rows arrive newest-first, so the first time a teammate is seen
        // is their most recent shared match — later (older) occurrences
        // only bump the count, and fill the name when every newer row had
        // none.
        for row in rows {
            let (teammate_puuid, match_name, start_ms, seen_name, won) = row?;
            let a = agg.entry(teammate_puuid).or_insert(Agg {
                count: 0,
                wins: 0,
                losses: 0,
                most_recent_ms: start_ms,
                match_name: None,
                seen_name,
            });
            a.count += 1;
            match won {
                Some(true) => a.wins += 1,
                Some(false) => a.losses += 1,
                None => {}
            }
            if a.match_name.is_none() {
                a.match_name = match_name;
            }
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
            .map(|(puuid, a)| {
                let (game_name, tag_line) = match a.match_name.or(a.seen_name) {
                    Some((name, tag)) => (Some(name), Some(tag)),
                    None => (None, None),
                };
                TeammateRow {
                    puuid,
                    game_name,
                    tag_line,
                    count: a.count,
                    wins: a.wins,
                    losses: a.losses,
                }
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

    /// Removes one `app_state` key (absence is "never set"; a key that
    /// isn't there is not an error).
    pub fn delete_state(&self, key: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM app_state WHERE key = ?1", params![key])?;
        Ok(())
    }
}
