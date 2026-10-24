//! R189 (user 2026-10-22): the Match History page — the user's own stored
//! matches, any queue, from the DB only (`match_history` in
//! `src-tauri/src/v2/history.rs` is a thin wrapper around
//! [`Repo::match_history`]).
//!
//! A match counts when the user's history list names it (`history_index`)
//! AND its details are stored (`matches` + the user's `match_players` row).
//! A match only in `history_index` (its match-details not fetched yet, or a
//! 404) is skipped from the rows and from the totals alike: it has no map,
//! result or stats to show, and the backfill (R101) fills it later. So the
//! tab counts always equal what paging can reach.

use rusqlite::{params, Result};

use super::repo::Repo;
use crate::model::{display_tier, match_result};
use crate::stats;
use crate::wire::{HistoryPage, HistoryQueue, HistoryRow};

/// A page never holds more rows than this, whatever the caller asks.
pub const HISTORY_MAX_LIMIT: u32 = 200;

/// One page of the user's (`?1`) matches, newest first; `?2` is 1 for the
/// competitive tab (`queue_id = 'competitive'` only), 0 for all queues; `?3`
/// the limit, `?4` the offset.
///
/// As `COMP_MATCHES_SQL` (R88): the page is cut first (`mine`, walked in
/// order on `idx_history_index_puuid_start`, each row joined to its match and
/// the user's row by primary key), and only those rows are joined to
/// `match_teams` (the user's team's rounds, and the other team's when there
/// is exactly one) and to the user's RR row (by primary key). The cost grows
/// with offset + limit, not with the stored history. Public so a test can
/// `EXPLAIN QUERY PLAN` exactly this text.
pub const HISTORY_PAGE_SQL: &str = "\
WITH mine AS (
    SELECT hi.start_ms AS sort_ms, m.match_id, m.map_id, m.queue_id, m.start_ms,
           m.game_length_ms, m.winning_team,
           mp.puuid, mp.team_id, mp.agent_id, mp.kills, mp.deaths, mp.assists,
           mp.score, mp.rounds, mp.hs, mp.bs, mp.ls
    FROM history_index hi
    JOIN matches m ON m.match_id = hi.match_id
    JOIN match_players mp ON mp.match_id = hi.match_id AND mp.puuid = hi.puuid
    WHERE hi.puuid = ?1 AND (?2 = 0 OR m.queue_id = 'competitive')
    ORDER BY hi.start_ms DESC, hi.match_id DESC
    LIMIT ?3 OFFSET ?4
)
SELECT mine.match_id, mine.map_id, mine.queue_id, mine.start_ms, mine.game_length_ms,
       mine.winning_team, mine.team_id, mine.agent_id,
       mine.kills, mine.deaths, mine.assists, mine.score, mine.rounds,
       mine.hs, mine.bs, mine.ls,
       MAX(CASE WHEN t.team_id = mine.team_id THEN t.rounds_won END),
       CASE WHEN COUNT(CASE WHEN t.team_id != mine.team_id THEN 1 END) = 1
            THEN MAX(CASE WHEN t.team_id != mine.team_id THEN t.rounds_won END) END,
       cu.rr_earned, cu.tier_after
FROM mine
LEFT JOIN match_teams t ON t.match_id = mine.match_id
LEFT JOIN comp_updates cu ON cu.puuid = mine.puuid AND cu.match_id = mine.match_id
GROUP BY mine.match_id
ORDER BY mine.sort_ms DESC, mine.match_id DESC";

/// The two tab counts for the user (`?1`): every stored match of theirs, and
/// the competitive ones — the same rows [`HISTORY_PAGE_SQL`] pages through.
pub const HISTORY_TOTALS_SQL: &str = "\
SELECT COUNT(*), COALESCE(SUM(m.queue_id = 'competitive'), 0)
FROM history_index hi
JOIN matches m ON m.match_id = hi.match_id
JOIN match_players mp ON mp.match_id = hi.match_id AND mp.puuid = hi.puuid
WHERE hi.puuid = ?1";

/// One row of [`HISTORY_PAGE_SQL`] on the wire. The stat line follows
/// `model::match_summary` exactly: no round count stored means the player had
/// no stats row, and then every stat is `None` (R102); ACS is score over
/// rounds, K/D is `stats::kd`, HS% is `stats::hs_pct` (headshots over all
/// shots landed).
fn history_row(r: &rusqlite::Row, map_uuid: &dyn Fn(&str) -> Option<String>) -> Result<HistoryRow> {
    let map_path: String = r.get(1)?;
    let team_id: String = r.get(6)?;
    let winning_team: Option<String> = r.get(5)?;
    let (kills, deaths, assists, score): (i64, i64, i64, i64) =
        (r.get(8)?, r.get(9)?, r.get(10)?, r.get(11)?);
    let rounds = r.get::<_, Option<i64>>(12)?.filter(|&n| n > 0);
    let (hs, bs, ls): (i64, i64, i64) = (r.get(13)?, r.get(14)?, r.get(15)?);
    let has_stats = rounds.is_some();
    let count = |v: i64| has_stats.then(|| u32::try_from(v).unwrap_or(0));
    let rounds_of = |v: Option<i64>| v.and_then(|n| u32::try_from(n).ok());
    Ok(HistoryRow {
        match_id: r.get(0)?,
        // R84: never the Riot path on the wire.
        map_id: map_uuid(&map_path).filter(|m| !m.is_empty() && !m.starts_with('/')),
        queue_id: r.get(2)?,
        start_ms: r.get(3)?,
        duration_ms: r.get(4)?,
        result: match_result(&team_id, winning_team.as_deref()),
        rounds_won: rounds_of(r.get(16)?),
        rounds_lost: rounds_of(r.get(17)?),
        agent_id: r.get::<_, Option<String>>(7)?.filter(|a| !a.is_empty()),
        kills: count(kills),
        deaths: count(deaths),
        assists: count(assists),
        kd: has_stats.then(|| stats::kd(kills, deaths)),
        acs: rounds.map(|n| score as f32 / n as f32),
        hs_pct: stats::hs_pct(hs, bs, ls).filter(|_| has_stats),
        rr_earned: r
            .get::<_, Option<i64>>(18)?
            .and_then(|v| i32::try_from(v).ok()),
        tier_after: display_tier(r.get(19)?),
    })
}

impl Repo {
    /// One page of the user's (`me`) stored matches, newest first, plus the
    /// counts of both tabs (see the module docs for which matches count).
    /// `limit` is capped at [`HISTORY_MAX_LIMIT`]; `total` is the asked
    /// queue's count. `map_uuid` turns the stored Riot map path into a
    /// valorant-api uuid (`None` when unknown).
    pub fn match_history(
        &self,
        me: &str,
        queue: HistoryQueue,
        offset: u32,
        limit: u32,
        map_uuid: &dyn Fn(&str) -> Option<String>,
    ) -> Result<HistoryPage> {
        let competitive_only = matches!(queue, HistoryQueue::Competitive);
        let limit = limit.min(HISTORY_MAX_LIMIT);
        let (all_total, competitive_total): (u32, u32) =
            self.conn()
                .query_row(HISTORY_TOTALS_SQL, params![me], |r| {
                    Ok((r.get(0)?, r.get(1)?))
                })?;
        let rows = if limit == 0 {
            Vec::new()
        } else {
            self.conn()
                .prepare(HISTORY_PAGE_SQL)?
                .query_map(params![me, competitive_only, limit, offset], |r| {
                    history_row(r, map_uuid)
                })?
                .collect::<Result<Vec<_>>>()?
        };
        Ok(HistoryPage {
            rows,
            total: if competitive_only {
                competitive_total
            } else {
                all_total
            },
            competitive_total,
            all_total,
        })
    }

    /// How many matches the DB stores (any player's, any queue): the About
    /// screen's count.
    pub fn stored_match_count(&self) -> Result<u32> {
        self.conn()
            .query_row("SELECT COUNT(*) FROM matches", [], |r| r.get(0))
    }
}
