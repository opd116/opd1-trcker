//! The typed sync repository: every read/write `opd1-core` needs against
//! the schema in `schema.sql`, wrapped around a [`Connection`] so callers
//! never hand-write SQL.
//!
//! "Sync, don't refetch" (CLAUDE.md): [`Repo::new_match_ids`] diffs a
//! fetched history list against what's already stored so the caller only
//! fetches genuinely new match-details; [`Repo::needs_mmr_refresh`] does
//! the same for the (rate-limited) MMR call.

use std::collections::HashMap;

use rusqlite::{params, Connection, OptionalExtension, Result};

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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchTeam {
    pub team_id: String,
    pub won: bool,
    pub rounds_won: Option<i64>,
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
            "INSERT INTO matches (match_id, map_id, queue_id, start_ms, winning_team, is_ranked, rounds_played)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(match_id) DO UPDATE SET
                map_id = excluded.map_id,
                queue_id = excluded.queue_id,
                start_ms = excluded.start_ms,
                winning_team = excluded.winning_team,
                is_ranked = excluded.is_ranked,
                rounds_played = excluded.rounds_played",
            params![
                m.match_id,
                m.map_id,
                m.queue_id,
                m.start_ms,
                m.winning_team,
                m.is_ranked,
                m.rounds_played
            ],
        )?;

        for t in teams {
            tx.execute(
                "INSERT INTO match_teams (match_id, team_id, won, rounds_won)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(match_id, team_id) DO UPDATE SET
                    won = excluded.won,
                    rounds_won = excluded.rounds_won",
                params![m.match_id, t.team_id, t.won, t.rounds_won],
            )?;
        }

        for p in players {
            tx.execute(
                "INSERT INTO match_players
                    (match_id, puuid, team_id, party_id, agent_id, kills, deaths, assists, score, rounds, hs, bs, ls, game_name, tag_line)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
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
                    tag_line = excluded.tag_line",
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
                    p.tag_line
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncounterRow {
    pub match_id: String,
    pub map_id: String,
    pub start_ms: i64,
    /// `true` when `self` and `other` were on the same team.
    pub with: bool,
    /// `None` for a draw / no team winner.
    pub self_won: Option<bool>,
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
    pub fn encounters(&self, self_puuid: &str, other: &str) -> Result<Encounters> {
        let mut stmt = self.conn.prepare(
            "SELECT m.match_id, m.map_id, m.start_ms, mp_self.team_id, mp_other.team_id, m.winning_team
             FROM match_players mp_self
             JOIN match_players mp_other
               ON mp_other.match_id = mp_self.match_id AND mp_other.puuid = ?2
             JOIN matches m ON m.match_id = mp_self.match_id
             WHERE mp_self.puuid = ?1
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

    /// The most recent non-empty `(game_name, tag_line)` `other` used in a
    /// match `self_puuid` also has a row in. Never widen this to a match
    /// `self_puuid` didn't play — this is the privacy boundary that lets
    /// a hidden player's name show once the user has legitimately learned
    /// it (CLAUDE.md's "Hidden names" exception), and it must never leak
    /// a name seen only in someone else's match history.
    pub fn known_name(&self, self_puuid: &str, other: &str) -> Result<Option<(String, String)>> {
        self.conn
            .query_row(
                "SELECT mp_other.game_name, mp_other.tag_line
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
                |row| Ok((row.get(0)?, row.get(1)?)),
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
