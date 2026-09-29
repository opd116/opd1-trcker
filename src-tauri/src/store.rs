//! DB helpers the Tauri shell needs on top of `opd1_core::db::repo::Repo`:
//! the per-player bundle a card is built from ([`load_player_db`], incl.
//! the remembered name, encounter count and the last two matches party
//! detection reads), a name/level-preserving `players` upsert
//! ([`save_player`]), the trimmed MMR snapshot format
//! ([`trim_mmr`]/[`parse_mmr_snapshot`]), the Player Details reads
//! ([`recent_matches`]) and My Stats ([`load_my_stats`]). Everything here
//! is plain synchronous rusqlite — callers run it through `AppState::db`
//! (`spawn_blocking`), never on an async task directly.
//!
//! Extra SQL goes through `Repo::conn()` (the repo's documented escape
//! hatch) rather than new `Repo` methods, keeping `opd1-core`'s repo API
//! as Task 4 shipped it (+ `app_state`, + `comp_matches`, which My Stats'
//! core tests exercise directly).

use std::collections::HashMap;

use rusqlite::{params, Connection, OptionalExtension, Result};
use serde_json::{json, Map, Value};

use opd1_core::db::repo::{CompMatchRow, CompUpdateRow, MatchPlayerRow, Repo};
use opd1_core::model::{match_summary, EncounterCount, MatchSummary, MyStats};
use opd1_core::mystats::{self, MyStatsInput};
use opd1_core::party::SharedRef;
use opd1_core::stats::Act;
use opd1_riot::endpoints::PlayerMmrResponse;
use opd1_riot::to_core::current_rank_from_mmr;

use crate::roster::is_ffa_queue;

/// `app_state` key holding the user's own puuid (R31) — lets the Closed
/// stage show the user's card before the Riot client is ever reached.
pub const SELF_PUUID_KEY: &str = "self_puuid";

/// The queue key Riot uses for ranked in `QueueSkills`.
const COMPETITIVE: &str = "competitive";

// ---------------------------------------------------------------------
// players
// ---------------------------------------------------------------------

/// A `players` row as the shell reads it back.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredPlayer {
    pub game_name: String,
    pub tag_line: String,
    pub account_level: i64,
    pub card_id: String,
}

impl StoredPlayer {
    /// `(name, tag)` only when both are known.
    pub fn name(&self) -> Option<(String, String)> {
        (!self.game_name.is_empty() && !self.tag_line.is_empty())
            .then(|| (self.game_name.clone(), self.tag_line.clone()))
    }
}

/// What the shell learned about a player this time. `None`/`0`/`""` mean
/// "not learned now" and never overwrite what the row already has — so a
/// player seen with a hidden name or level keeps the values stored when
/// they were visible (they're only ever *displayed* per the live
/// hidden-name rules, never because they're in this table).
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerUpdate {
    pub puuid: String,
    pub name: Option<(String, String)>,
    /// `0` = unknown / hidden (`HideAccountLevel`).
    pub account_level: u32,
    pub card_id: String,
}

pub fn save_player(conn: &Connection, upd: &PlayerUpdate, now_ms: i64) -> Result<()> {
    let (name, tag) = upd
        .name
        .as_ref()
        .map(|(n, t)| (n.as_str(), t.as_str()))
        .unwrap_or(("", ""));
    conn.execute(
        "INSERT INTO players (puuid, game_name, tag_line, account_level, card_id, updated_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(puuid) DO UPDATE SET
            game_name = CASE WHEN excluded.game_name != '' AND excluded.tag_line != ''
                             THEN excluded.game_name ELSE players.game_name END,
            tag_line = CASE WHEN excluded.game_name != '' AND excluded.tag_line != ''
                            THEN excluded.tag_line ELSE players.tag_line END,
            account_level = CASE WHEN excluded.account_level > 0
                                 THEN excluded.account_level ELSE players.account_level END,
            card_id = CASE WHEN excluded.card_id != '' THEN excluded.card_id ELSE players.card_id END,
            updated_ms = excluded.updated_ms",
        params![
            upd.puuid,
            name,
            tag,
            upd.account_level as i64,
            upd.card_id,
            now_ms
        ],
    )?;
    Ok(())
}

pub fn load_player(conn: &Connection, puuid: &str) -> Result<Option<StoredPlayer>> {
    conn.query_row(
        "SELECT game_name, tag_line, account_level, card_id FROM players WHERE puuid = ?1",
        params![puuid],
        |row| {
            Ok(StoredPlayer {
                game_name: row.get(0)?,
                tag_line: row.get(1)?,
                account_level: row.get(2)?,
                card_id: row.get(3)?,
            })
        },
    )
    .optional()
}

// ---------------------------------------------------------------------
// MMR snapshot (trimmed)
// ---------------------------------------------------------------------

/// The MMR snapshot stored in `mmr_snapshots.json`: Riot's own response
/// shape (so it parses straight back as a `PlayerMmrResponse` and goes
/// through the same `to_core` conversions as a live response), trimmed to
/// the competitive queue and the six fields the card math uses. A full
/// MMR response carries every queue and every field (tens of KB per
/// player); every player ever seen gets a snapshot, so lightness wins.
pub fn trim_mmr(resp: &PlayerMmrResponse) -> Value {
    let mut seasons = Map::new();
    if let Some(comp) = resp.queue_skills.get(COMPETITIVE) {
        for (id, s) in &comp.seasonal_info_by_season_id {
            seasons.insert(
                id.clone(),
                json!({
                    "SeasonID": s.season_id,
                    "CompetitiveTier": s.competitive_tier,
                    "RankedRating": s.ranked_rating,
                    "NumberOfWins": s.number_of_wins,
                    "NumberOfGames": s.number_of_games,
                    "WinsByTier": s.wins_by_tier,
                }),
            );
        }
    }
    json!({
        "Subject": resp.subject,
        "QueueSkills": { COMPETITIVE: { "SeasonalInfoBySeasonID": seasons } }
    })
}

/// A stored snapshot back into Riot's type; `None` if it doesn't parse
/// (then the card simply treats MMR as not-yet-known).
pub fn parse_mmr_snapshot(json: &str) -> Option<PlayerMmrResponse> {
    serde_json::from_str(json).ok()
}

// ---------------------------------------------------------------------
// the per-player card bundle
// ---------------------------------------------------------------------

/// Everything the DB knows that a card is built from.
#[derive(Debug, Clone, Default)]
pub struct PlayerDb {
    pub mmr: Option<PlayerMmrResponse>,
    /// Last 5 competitive rows, newest first.
    pub last5: Vec<MatchPlayerRow>,
    /// Recent competitive-update rows (the `rrEarned` join for `last5`).
    pub comp: Vec<CompUpdateRow>,
    pub row: Option<StoredPlayer>,
    /// Their name as shown in a finished match the user played in
    /// (`Repo::known_name`) — displayed only while they're incognito.
    pub remembered: Option<(String, String)>,
    /// Matches shared with the user; `None` when there are none (no ⟲
    /// badge for strangers, R52) and always for the user themself.
    pub encounters: Option<EncounterCount>,
    /// Their last two matches (any mode), newest first — party detection.
    pub recent: [Option<SharedRef>; 2],
}

/// `self_puuid` is the user (`None` before the app ever learned it): the
/// remembered name and encounter count are relative to the user, so the
/// user's own bundle (and any bundle while the user is unknown) has
/// neither.
pub fn load_player_db(repo: &Repo, self_puuid: Option<&str>, puuid: &str) -> Result<PlayerDb> {
    let mmr = repo
        .load_mmr_snapshot(puuid)?
        .and_then(|snap| parse_mmr_snapshot(&snap.json));
    let (remembered, encounters) = match self_puuid {
        Some(me) if me != puuid => {
            let e = repo.encounters(me, puuid)?;
            let count = (e.with + e.against > 0).then_some(EncounterCount {
                with: e.with,
                against: e.against,
            });
            (repo.known_name(me, puuid)?, count)
        }
        _ => (None, None),
    };
    Ok(PlayerDb {
        mmr,
        last5: repo.last_n_comp(puuid, 5)?,
        comp: recent_comp_updates(repo.conn(), puuid, 10)?,
        row: load_player(repo.conn(), puuid)?,
        remembered,
        encounters,
        recent: recent_refs(repo.conn(), puuid)?,
    })
}

/// A player's last two matches from their own history listing
/// (`history_index`: the sync indexes their last 2 matches in any queue
/// plus their last 5 competitive ones), newest first. A match whose
/// details aren't stored is `None` (unknown team/party — links nothing).
/// Free-for-all matches have no team (`team_id: None`).
pub fn recent_refs(conn: &Connection, puuid: &str) -> Result<[Option<SharedRef>; 2]> {
    let mut stmt = conn.prepare(
        "SELECT h.match_id, h.queue_id, mp.team_id, mp.party_id
         FROM history_index h
         LEFT JOIN match_players mp ON mp.match_id = h.match_id AND mp.puuid = h.puuid
         WHERE h.puuid = ?1
         ORDER BY h.start_ms DESC
         LIMIT 2",
    )?;
    let rows = stmt.query_map(params![puuid], |row| {
        let match_id: String = row.get(0)?;
        let queue_id: String = row.get(1)?;
        let team_id: Option<String> = row.get(2)?;
        let party_id: Option<String> = row.get(3)?;
        Ok(team_id.map(|team| SharedRef {
            match_id,
            team_id: (!is_ffa_queue(Some(&queue_id))).then_some(team),
            party_id,
        }))
    })?;
    let mut out = [None, None];
    for (slot, r) in out.iter_mut().zip(rows) {
        *slot = r?;
    }
    Ok(out)
}

/// The `comp_updates` columns [`comp_update_row`] reads, in its order.
const COMP_UPDATE_COLUMNS: &str =
    "match_id, season_id, tier_after, tier_before, rr_after, rr_earned, map_id, start_ms";

fn comp_update_row(row: &rusqlite::Row) -> Result<CompUpdateRow> {
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
}

/// A player's newest `limit` `comp_updates` rows, any season.
pub fn recent_comp_updates(
    conn: &Connection,
    puuid: &str,
    limit: u32,
) -> Result<Vec<CompUpdateRow>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COMP_UPDATE_COLUMNS} FROM comp_updates
         WHERE puuid = ?1 ORDER BY start_ms DESC LIMIT ?2"
    ))?;
    let rows = stmt.query_map(params![puuid, limit], comp_update_row)?;
    rows.collect()
}

/// A player's `comp_updates` rows of `season_id` plus those of any season
/// from `since_ms` on, newest first.
fn season_or_recent_comp_updates(
    conn: &Connection,
    puuid: &str,
    season_id: &str,
    since_ms: i64,
) -> Result<Vec<CompUpdateRow>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COMP_UPDATE_COLUMNS} FROM comp_updates
         WHERE puuid = ?1 AND (season_id = ?2 OR start_ms >= ?3)
         ORDER BY start_ms DESC"
    ))?;
    let rows = stmt.query_map(params![puuid, season_id, since_ms], comp_update_row)?;
    rows.collect()
}

// ---------------------------------------------------------------------
// post-match (R48)
// ---------------------------------------------------------------------

/// Has the just-ended match reached the data the Lobby card shows? For a
/// competitive match that means the user's `comp_updates` row (RR/rank —
/// Riot can publish match-history/details before competitiveupdates, and
/// the MMR refresh keys off the latest comp update); for any other queue,
/// the stored match itself.
pub fn match_landed(
    conn: &Connection,
    self_puuid: &str,
    match_id: &str,
    competitive: bool,
) -> Result<bool> {
    let sql = if competitive {
        "SELECT 1 FROM comp_updates WHERE puuid = ?1 AND match_id = ?2"
    } else {
        "SELECT 1 FROM matches WHERE ?1 IS NOT NULL AND match_id = ?2"
    };
    Ok(conn
        .query_row(sql, params![self_puuid, match_id], |_| Ok(()))
        .optional()?
        .is_some())
}

// ---------------------------------------------------------------------
// Player Details reads
// ---------------------------------------------------------------------

/// Riot map path -> valorant-api uuid on every row, in place; an unknown
/// map becomes `""` (the UI shows `—`), never the raw `/Game/...` path.
/// One lookup per distinct map (a lookup clones the asset tables).
fn to_map_uuids(rows: &mut [CompMatchRow], map_uuid: &dyn Fn(&str) -> Option<String>) {
    let mut seen: HashMap<String, String> = HashMap::new();
    for r in rows {
        let uuid = seen
            .entry(std::mem::take(&mut r.map_id))
            .or_insert_with_key(|path| map_uuid(path).unwrap_or_default());
        r.map_id = uuid.clone();
    }
}

/// Up to `n` newest competitive matches as `MatchSummary` (any act).
/// `map_uuid` turns the stored Riot map path into a valorant-api uuid.
/// `roundsLost` is the other team's rounds when there are exactly two
/// teams, else `None`.
pub fn recent_matches(
    repo: &Repo,
    puuid: &str,
    n: u32,
    map_uuid: &dyn Fn(&str) -> Option<String>,
) -> Result<Vec<MatchSummary>> {
    let mut rows = repo.comp_matches(puuid, i64::MIN..i64::MAX, n)?;
    to_map_uuids(&mut rows, map_uuid);
    Ok(rows.iter().map(match_summary).collect())
}

// ---------------------------------------------------------------------
// My Stats
// ---------------------------------------------------------------------

/// My Stats for the user (`me`) over `act`, from the DB only: the act's
/// competitive matches (read a day past its dates; `mystats::build` picks
/// the act's by Riot season), its RR rows plus any from the last 7 days,
/// and the stored MMR snapshot's rank.
/// `map_names` is valorant-api map uuid -> name (insight text);
/// `utc_offset_ms` gives the local offset at an instant (weekday and
/// time-of-day buckets).
pub fn load_my_stats(
    repo: &Repo,
    me: &str,
    act: &Act,
    now_ms: i64,
    map_uuid: &dyn Fn(&str) -> Option<String>,
    map_names: &HashMap<String, String>,
    utc_offset_ms: &dyn Fn(i64) -> i64,
) -> Result<MyStats> {
    // `build` decides act membership by each RR row's Riot season, which
    // can differ from valorant-api's dates by the rollout's hours.
    let dates = act.start_ms.saturating_sub(mystats::ACT_EDGE_MS)
        ..act.end_ms.saturating_add(mystats::ACT_EDGE_MS);
    let mut matches = repo.comp_matches(me, dates, u32::MAX)?;
    to_map_uuids(&mut matches, map_uuid);
    // The act's RR rows, plus any season's for "last 7 days".
    let comp_updates =
        season_or_recent_comp_updates(repo.conn(), me, &act.id, now_ms - mystats::LAST_7_DAYS_MS)?;
    let current_rank = repo
        .load_mmr_snapshot(me)?
        .and_then(|snap| parse_mmr_snapshot(&snap.json))
        .and_then(|resp| current_rank_from_mmr(&resp, &act.id));
    Ok(mystats::build(MyStatsInput {
        act,
        matches: &matches,
        comp_updates: &comp_updates,
        current_rank,
        map_names,
        now_ms,
        utc_offset_ms,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    use opd1_core::db::open_db;
    use opd1_core::db::repo::{HistoryEntry, Match, MatchPlayer, MatchTeam, MmrSnapshot};
    use opd1_core::model::{EncounterCount, MatchResultKind};

    fn repo() -> Repo {
        Repo::new(open_db(Path::new(":memory:")).unwrap())
    }

    fn upd(name: Option<(&str, &str)>, level: u32, card: &str) -> PlayerUpdate {
        PlayerUpdate {
            puuid: "p1".into(),
            name: name.map(|(n, t)| (n.into(), t.into())),
            account_level: level,
            card_id: card.into(),
        }
    }

    #[test]
    fn save_player_never_blanks_known_name_level_or_card() {
        let r = repo();
        save_player(r.conn(), &upd(Some(("Shanky", "OPD1")), 187, "card-a"), 1).unwrap();
        // Seen again hidden (no name), level hidden (0), card unchanged.
        save_player(r.conn(), &upd(None, 0, ""), 2).unwrap();
        let row = load_player(r.conn(), "p1").unwrap().unwrap();
        assert_eq!(row.name(), Some(("Shanky".into(), "OPD1".into())));
        assert_eq!(row.account_level, 187);
        assert_eq!(row.card_id, "card-a");
        // A real update does overwrite.
        save_player(r.conn(), &upd(Some(("New", "TAG")), 190, "card-b"), 3).unwrap();
        let row = load_player(r.conn(), "p1").unwrap().unwrap();
        assert_eq!(row.name(), Some(("New".into(), "TAG".into())));
        assert_eq!((row.account_level, row.card_id.as_str()), (190, "card-b"));
    }

    #[test]
    fn stored_player_without_a_full_name_has_none() {
        let r = repo();
        save_player(r.conn(), &upd(None, 5, "c"), 1).unwrap();
        assert_eq!(load_player(r.conn(), "p1").unwrap().unwrap().name(), None);
    }

    #[test]
    fn trimmed_mmr_round_trips_through_riots_type_and_drops_other_queues() {
        let resp: PlayerMmrResponse = serde_json::from_value(json!({
            "Subject": "p1",
            "QueueSkills": {
                "competitive": {"SeasonalInfoBySeasonID": {"act-1": {
                    "SeasonID": "act-1", "CompetitiveTier": 15, "RankedRating": 42,
                    "NumberOfWins": 6, "NumberOfGames": 10,
                    "WinsByTier": {"14": 2, "15": 4}, "LeaderboardRank": 0,
                    "Rank": 15, "CapstoneWins": 0
                }}},
                "unrated": {"SeasonalInfoBySeasonID": {"act-1": {
                    "SeasonID": "act-1", "CompetitiveTier": 0, "RankedRating": 0,
                    "NumberOfWins": 50, "NumberOfGames": 99, "WinsByTier": null
                }}}
            }
        }))
        .unwrap();
        let trimmed = trim_mmr(&resp);
        let text = trimmed.to_string();
        assert!(!text.contains("unrated"));
        assert!(!text.contains("CapstoneWins"));
        let back = parse_mmr_snapshot(&text).unwrap();
        let s = &back.queue_skills["competitive"].seasonal_info_by_season_id["act-1"];
        assert_eq!((s.competitive_tier, s.ranked_rating), (15, 42));
        assert_eq!((s.number_of_wins, s.number_of_games), (6, 10));
        assert_eq!(s.wins_by_tier.as_ref().unwrap()["15"], 4);
    }

    #[test]
    fn load_player_db_reads_snapshot_last5_and_row() {
        let r = repo();
        r.save_mmr_snapshot(&MmrSnapshot {
            puuid: "p1".into(),
            json: r#"{"Subject":"p1","QueueSkills":{}}"#.into(),
            last_comp_match_id: Some("m1".into()),
            fetched_ms: 1,
        })
        .unwrap();
        save_player(r.conn(), &upd(Some(("A", "B")), 3, "c"), 1).unwrap();
        let db = load_player_db(&r, Some("p1"), "p1").unwrap();
        assert!(db.mmr.is_some());
        assert!(db.last5.is_empty());
        assert_eq!(db.row.unwrap().account_level, 3);
        assert!(load_player_db(&r, Some("p1"), "nobody")
            .unwrap()
            .mmr
            .is_none());
    }

    fn mp(puuid: &str, team: &str, party: &str, name: Option<(&str, &str)>) -> MatchPlayer {
        MatchPlayer {
            puuid: puuid.into(),
            team_id: team.into(),
            party_id: Some(party.into()),
            agent_id: None,
            kills: 0,
            deaths: 0,
            assists: 0,
            score: 0,
            rounds: None,
            hs: 0,
            bs: 0,
            ls: 0,
            game_name: name.map(|(n, _)| n.to_string()),
            tag_line: name.map(|(_, t)| t.to_string()),
        }
    }

    fn stored_match(r: &Repo, id: &str, queue: &str, start_ms: i64, players: &[MatchPlayer]) {
        let m = Match {
            match_id: id.into(),
            map_id: "/Game/Maps/Ascent/Ascent".into(),
            queue_id: queue.into(),
            start_ms,
            winning_team: None,
            is_ranked: false,
            rounds_played: None,
        };
        r.upsert_match(&m, &[], players).unwrap();
    }

    fn indexed(r: &Repo, puuid: &str, entries: &[(&str, &str, i64)]) {
        let entries: Vec<HistoryEntry> = entries
            .iter()
            .map(|(id, q, start)| HistoryEntry {
                match_id: id.to_string(),
                queue_id: q.to_string(),
                start_ms: *start,
            })
            .collect();
        r.index_history(puuid, &entries).unwrap();
    }

    #[test]
    fn load_player_db_reads_remembered_name_encounters_and_recent_matches() {
        let r = repo();
        // m1: the user and h1 together; h1's name as shown after the match.
        stored_match(
            &r,
            "m1",
            "competitive",
            100,
            &[
                mp("me", "Blue", "pa", Some(("Me", "1"))),
                mp("h1", "Blue", "pa", Some(("Seen", "AFTER"))),
            ],
        );
        // m2 (newer): a deathmatch h1 played *without* the user, under
        // another name — never a remembered name (someone else's match).
        stored_match(
            &r,
            "m2",
            "deathmatch",
            200,
            &[mp("h1", "h1", "pb", Some(("Other", "NAME")))],
        );
        // m3: the user against h1.
        stored_match(
            &r,
            "m3",
            "unrated",
            50,
            &[mp("me", "Blue", "x", None), mp("h1", "Red", "y", None)],
        );
        indexed(
            &r,
            "h1",
            &[
                ("m0", "competitive", 10),
                ("m1", "competitive", 100),
                ("m2", "deathmatch", 200),
            ],
        );

        let db = load_player_db(&r, Some("me"), "h1").unwrap();
        assert_eq!(db.remembered, Some(("Seen".into(), "AFTER".into())));
        assert_eq!(
            db.encounters,
            Some(EncounterCount {
                with: 1,
                against: 1
            })
        );
        // Newest first; Deathmatch has no team.
        assert_eq!(
            db.recent,
            [
                Some(SharedRef {
                    match_id: "m2".into(),
                    team_id: None,
                    party_id: Some("pb".into()),
                }),
                Some(SharedRef {
                    match_id: "m1".into(),
                    team_id: Some("Blue".into()),
                    party_id: Some("pa".into()),
                }),
            ]
        );

        // A newest history entry without stored details is unknown.
        indexed(&r, "h1", &[("m9", "competitive", 300)]);
        let db = load_player_db(&r, Some("me"), "h1").unwrap();
        assert_eq!(db.recent[0], None);
        assert_eq!(db.recent[1].as_ref().unwrap().match_id, "m2");

        // The user's own bundle: nothing relative to the user.
        let own = load_player_db(&r, Some("me"), "me").unwrap();
        assert_eq!((own.remembered, own.encounters), (None, None));
        // User unknown (first run): nothing relative to them either.
        let db = load_player_db(&r, None, "h1").unwrap();
        assert_eq!((db.remembered, db.encounters), (None, None));
        // A stranger: no encounters (no badge), no name, no history.
        let db = load_player_db(&r, Some("me"), "nobody").unwrap();
        assert_eq!((db.remembered, db.encounters), (None, None));
        assert_eq!(db.recent, [None, None]);
    }

    #[test]
    fn recent_matches_fill_score_rr_and_convert_the_map() {
        let r = repo();
        let m = Match {
            match_id: "m1".into(),
            map_id: "/Game/Maps/Ascent/Ascent".into(),
            queue_id: "competitive".into(),
            start_ms: 100,
            winning_team: Some("Blue".into()),
            is_ranked: true,
            rounds_played: Some(23),
        };
        let teams = [
            MatchTeam {
                team_id: "Blue".into(),
                won: true,
                rounds_won: Some(13),
            },
            MatchTeam {
                team_id: "Red".into(),
                won: false,
                rounds_won: Some(10),
            },
        ];
        let p = MatchPlayer {
            puuid: "p1".into(),
            team_id: "Blue".into(),
            party_id: None,
            agent_id: Some("agent".into()),
            kills: 20,
            deaths: 10,
            assists: 4,
            score: 5000,
            rounds: Some(23),
            hs: 1,
            bs: 2,
            ls: 3,
            game_name: None,
            tag_line: None,
        };
        r.upsert_match(&m, &teams, &[p]).unwrap();
        r.upsert_comp_updates(
            "p1",
            &[CompUpdateRow {
                match_id: "m1".into(),
                season_id: "act".into(),
                tier_after: 15,
                tier_before: Some(15),
                rr_after: 50,
                rr_earned: 21,
                map_id: None,
                start_ms: 100,
            }],
        )
        .unwrap();
        let lookup =
            |path: &str| (path == "/Game/Maps/Ascent/Ascent").then(|| "uuid-ascent".to_string());
        let out = recent_matches(&r, "p1", 8, &lookup).unwrap();
        assert_eq!(out.len(), 1);
        let s = &out[0];
        assert_eq!(s.map_id, "uuid-ascent");
        assert_eq!(s.result, MatchResultKind::Win);
        assert_eq!((s.rounds_won, s.rounds_lost), (Some(13), Some(10)));
        assert_eq!(s.rr_earned, Some(21));
        assert_eq!((s.kills, s.deaths, s.assists), (20, 10, 4));
        assert_eq!(s.kd, Some(2.0));
        assert_eq!(s.acs, Some(5000.0 / 23.0));
        assert_eq!(s.hs_pct, Some(1.0 / 6.0));

        // Unknown map: empty id, never the Riot path.
        let none = |_: &str| None;
        assert_eq!(recent_matches(&r, "p1", 8, &none).unwrap()[0].map_id, "");
    }

    const ASCENT: &str = "/Game/Maps/Ascent/Ascent";

    /// A match "me" played on Blue (13-7 win or 7-13 loss) with a
    /// 20/10/5 stat line.
    fn played(r: &Repo, id: &str, queue: &str, start_ms: i64, won: bool) {
        let m = Match {
            match_id: id.into(),
            map_id: ASCENT.into(),
            queue_id: queue.into(),
            start_ms,
            winning_team: Some(if won { "Blue" } else { "Red" }.into()),
            is_ranked: queue == "competitive",
            rounds_played: Some(20),
        };
        let (blue, red) = if won { (13, 7) } else { (7, 13) };
        let teams = [
            MatchTeam {
                team_id: "Blue".into(),
                won,
                rounds_won: Some(blue),
            },
            MatchTeam {
                team_id: "Red".into(),
                won: !won,
                rounds_won: Some(red),
            },
        ];
        let me = MatchPlayer {
            agent_id: Some("agent-jett".into()),
            kills: 20,
            deaths: 10,
            assists: 5,
            score: 4000,
            rounds: Some(20),
            hs: 5,
            bs: 15,
            ls: 5,
            ..mp("me", "Blue", "p", Some(("Me", "1")))
        };
        r.upsert_match(&m, &teams, &[me]).unwrap();
    }

    fn rr_row(id: &str, season: &str, start_ms: i64, earned: i64) -> CompUpdateRow {
        CompUpdateRow {
            match_id: id.into(),
            season_id: season.into(),
            tier_after: 15,
            tier_before: Some(15),
            rr_after: 42,
            rr_earned: earned,
            map_id: Some(ASCENT.into()),
            start_ms,
        }
    }

    /// Recursively: no object key with an underscore (camelCase only).
    fn assert_camel_case(v: &Value, path: &str) {
        match v {
            Value::Object(map) => {
                for (k, child) in map {
                    assert!(!k.contains('_'), "snake_case key {path}.{k}");
                    assert_camel_case(child, &format!("{path}.{k}"));
                }
            }
            Value::Array(items) => items.iter().for_each(|i| assert_camel_case(i, path)),
            _ => {}
        }
    }

    #[test]
    fn my_stats_json_is_camel_case_and_current_act_only() {
        let r = repo();
        let day = 86_400_000;
        let act = Act {
            id: "act-cur".into(),
            name: "ACT V".into(),
            episode_name: "V26".into(),
            start_ms: 100 * day,
            end_ms: 160 * day,
        };
        played(&r, "prev", "competitive", 99 * day, true);
        played(&r, "cur-1", "competitive", 101 * day, true);
        played(&r, "cur-2", "competitive", 102 * day, false);
        played(&r, "unrated", "unrated", 103 * day, true);
        r.upsert_comp_updates(
            "me",
            &[
                rr_row("prev", "act-prev", 99 * day, 30),
                rr_row("cur-1", "act-cur", 101 * day, 21),
                rr_row("cur-2", "act-cur", 102 * day, -17),
            ],
        )
        .unwrap();
        let mmr = json!({
            "Subject": "me",
            "QueueSkills": {"competitive": {"SeasonalInfoBySeasonID": {
                "act-cur": {"SeasonID": "act-cur", "CompetitiveTier": 15, "RankedRating": 42,
                            "NumberOfWins": 1, "NumberOfGames": 2, "WinsByTier": {"15": 1}}
            }}}
        });
        r.save_mmr_snapshot(&MmrSnapshot {
            puuid: "me".into(),
            json: mmr.to_string(),
            last_comp_match_id: Some("cur-2".into()),
            fetched_ms: 1,
        })
        .unwrap();
        let lookup = |path: &str| (path == ASCENT).then(|| "uuid-ascent".to_string());
        let names: HashMap<String, String> =
            [("uuid-ascent".to_string(), "Ascent".to_string())].into();
        let utc = |_: i64| 0;

        let stats = load_my_stats(&r, "me", &act, 110 * day, &lookup, &names, &utc).unwrap();
        // Through a JSON string, as the IPC bridge sends it.
        let v: Value = serde_json::from_str(&serde_json::to_string(&stats).unwrap()).unwrap();

        assert_camel_case(&v, "");
        let mut keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "agents",
                "byDaypart",
                "byWeekday",
                "insights",
                "last5",
                "maps",
                "rank",
                "rrSeries",
                "session"
            ]
        );
        assert_eq!(v["rank"]["tier"], json!(15));
        assert_eq!(v["rank"]["rr"], json!(42));
        assert_eq!(v["rank"]["rrToNext"], json!(58));
        assert_eq!(
            (&v["rank"]["wins"], &v["rank"]["losses"]),
            (&json!(1), &json!(1))
        );
        assert_eq!(
            v["rank"]["netRr"],
            json!(4),
            "21 - 17; the previous act's +30 excluded"
        );
        let ids: Vec<&str> = v["last5"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["matchId"].as_str().unwrap())
            .collect();
        assert_eq!(ids, ["cur-2", "cur-1"], "current act, competitive only");
        let newest = &v["last5"][0];
        assert_eq!(newest["mapId"], json!("uuid-ascent"));
        assert_eq!(newest["result"], json!("loss"));
        assert_eq!(
            (&newest["roundsWon"], &newest["roundsLost"]),
            (&json!(7), &json!(13))
        );
        assert_eq!(newest["rrEarned"], json!(-17));
        assert_eq!(newest["acs"], json!(200.0));
        assert_eq!(newest["hsPct"], json!(0.2));
        assert_eq!(v["maps"][0]["id"], json!("uuid-ascent"));
        assert_eq!(v["maps"][0]["games"], json!(2));
        assert_eq!(v["rrSeries"].as_array().unwrap().len(), 2);
    }

    /// 9a math review M2: act membership follows each RR row's Riot
    /// season, so the matches Riot counted for the act on either side of
    /// valorant-api's dates must reach `mystats::build`.
    #[test]
    fn my_stats_reads_matches_riot_counted_for_the_act_past_its_dates() {
        let r = repo();
        let (day, hour) = (86_400_000, 3_600_000);
        let act = Act {
            id: "act-cur".into(),
            name: "ACT V".into(),
            episode_name: "V26".into(),
            start_ms: 100 * day,
            end_ms: 160 * day,
        };
        // Riot rolled this act out 2h before valorant-api's start …
        played(&r, "eve", "competitive", 100 * day - 2 * hour, true);
        // … while here the previous act was still on 3h after it.
        played(&r, "late-prev", "competitive", 100 * day + 3 * hour, true);
        played(&r, "cur", "competitive", 102 * day, false);
        // No RR row: valorant-api's dates decide.
        played(&r, "no-rr", "competitive", 100 * day - hour, true);
        r.upsert_comp_updates(
            "me",
            &[
                rr_row("eve", "act-cur", 100 * day - 2 * hour, 10),
                rr_row("late-prev", "act-prev", 100 * day + 3 * hour, 28),
                rr_row("cur", "act-cur", 102 * day, -17),
            ],
        )
        .unwrap();
        let none = |_: &str| None;
        let utc = |_: i64| 0;
        let stats = load_my_stats(&r, "me", &act, 110 * day, &none, &HashMap::new(), &utc).unwrap();
        let ids: Vec<&str> = stats.last5.iter().map(|m| m.match_id.as_str()).collect();
        assert_eq!(ids, ["cur", "eve"]);
        assert_eq!((stats.rank.wins, stats.rank.losses), (Some(1), Some(1)));
        assert_eq!(stats.rank.net_rr, Some(10 - 17));
        assert_eq!(stats.rr_series.len(), 2);
    }

    /// 9a math review m4: "last 7 days" reads the RR rows of the last 7
    /// days in any season, not only the act's.
    #[test]
    fn my_stats_last_7_days_reads_the_previous_acts_recent_rr_rows() {
        let r = repo();
        let (day, hour) = (86_400_000, 3_600_000);
        let act = Act {
            id: "act-cur".into(),
            name: "ACT V".into(),
            episode_name: "V26".into(),
            start_ms: 100 * day,
            end_ms: 160 * day,
        };
        r.upsert_comp_updates(
            "me",
            &[
                rr_row("c1", "act-cur", 100 * day + hour, 10),
                rr_row("c2", "act-cur", 100 * day + 2 * hour, 10),
                rr_row("c3", "act-cur", 101 * day, 10),
                rr_row("p1", "act-prev", 99 * day, -20),
                rr_row("p2", "act-prev", 97 * day, -20),
                rr_row("p3", "act-prev", 96 * day, -20),
                // Older than 7 days.
                rr_row("p4", "act-prev", 94 * day, -20),
            ],
        )
        .unwrap();
        let none = |_: &str| None;
        let utc = |_: i64| 0;
        let stats = load_my_stats(&r, "me", &act, 102 * day, &none, &HashMap::new(), &utc).unwrap();
        let v = serde_json::to_value(&stats).unwrap();
        assert_eq!(
            v["insights"],
            json!([{"kind": "last7Days", "text": "-30 RR over 6 games in the last 7 days.",
                    "matches": 6, "positive": false}])
        );
        assert_eq!(v["rank"]["netRr"], json!(30), "the act's rows only");
    }

    #[test]
    fn my_stats_without_any_data_is_unknown_not_zero() {
        let r = repo();
        let act = Act {
            id: "act-cur".into(),
            name: "ACT V".into(),
            episode_name: "V26".into(),
            start_ms: 0,
            end_ms: 1_000,
        };
        let none = |_: &str| None;
        let utc = |_: i64| 0;
        let stats = load_my_stats(&r, "me", &act, 10, &none, &HashMap::new(), &utc).unwrap();
        let v = serde_json::to_value(&stats).unwrap();
        assert_eq!(v["rank"]["tier"], json!(null));
        assert_eq!(v["rank"]["wins"], json!(null));
        assert_eq!(v["rank"]["netRr"], json!(null));
        assert_eq!(v["session"], json!(null));
    }

    #[test]
    fn competitive_match_lands_only_with_its_comp_update() {
        let r = repo();
        let m = Match {
            match_id: "m1".into(),
            map_id: "/Game/Maps/Ascent/Ascent".into(),
            queue_id: "competitive".into(),
            start_ms: 1,
            winning_team: None,
            is_ranked: true,
            rounds_played: None,
        };
        r.upsert_match(&m, &[], &[]).unwrap();
        // Details arrived first: an unrated match would count as landed…
        assert!(match_landed(r.conn(), "me", "m1", false).unwrap());
        // …but a competitive one isn't until its RR row exists (R48).
        assert!(!match_landed(r.conn(), "me", "m1", true).unwrap());
        r.upsert_comp_updates(
            "me",
            &[CompUpdateRow {
                match_id: "m1".into(),
                season_id: "act".into(),
                tier_after: 10,
                tier_before: Some(10),
                rr_after: 1,
                rr_earned: 1,
                map_id: None,
                start_ms: 1,
            }],
        )
        .unwrap();
        assert!(match_landed(r.conn(), "me", "m1", true).unwrap());
        assert!(!match_landed(r.conn(), "someone-else", "m1", true).unwrap());
        assert!(!match_landed(r.conn(), "me", "m2", false).unwrap());
    }
}
