//! DB helpers the Tauri shell needs on top of `opd1_core::db::repo::Repo`:
//! the per-player bundle a card is built from ([`load_player_db`], incl.
//! the remembered name, encounter count and the last two matches party
//! detection reads), a name/level-preserving `players` upsert
//! ([`save_player`]), the trimmed MMR snapshot format
//! ([`trim_mmr`]/[`parse_mmr_snapshot`]), and the Player Details reads
//! ([`recent_matches`]). Everything here is plain
//! synchronous rusqlite — callers run it through `AppState::db`
//! (`spawn_blocking`), never on an async task directly.
//!
//! Extra SQL goes through `Repo::conn()` (the repo's documented escape
//! hatch) rather than new `Repo` methods, keeping `opd1-core`'s repo API
//! as Task 4 shipped it (+ `app_state`).

use rusqlite::{params, Connection, OptionalExtension, Result};
use serde_json::{json, Map, Value};

use opd1_core::db::repo::{CompUpdateRow, MatchPlayerRow, Repo};
use opd1_core::model::{match_result, EncounterCount, MatchSummary};
use opd1_core::party::SharedRef;
use opd1_riot::endpoints::PlayerMmrResponse;

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

/// A player's newest `limit` `comp_updates` rows, any season.
pub fn recent_comp_updates(
    conn: &Connection,
    puuid: &str,
    limit: u32,
) -> Result<Vec<CompUpdateRow>> {
    let mut stmt = conn.prepare(
        "SELECT match_id, season_id, tier_after, tier_before, rr_after, rr_earned, map_id, start_ms
         FROM comp_updates WHERE puuid = ?1 ORDER BY start_ms DESC LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![puuid, limit], |row| {
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

fn team_rounds(conn: &Connection, match_id: &str) -> Result<Vec<(String, Option<i64>)>> {
    let mut stmt =
        conn.prepare("SELECT team_id, rounds_won FROM match_teams WHERE match_id = ?1")?;
    let rows = stmt.query_map(params![match_id], |row| Ok((row.get(0)?, row.get(1)?)))?;
    rows.collect()
}

fn rr_earned(conn: &Connection, puuid: &str, match_id: &str) -> Result<Option<i64>> {
    conn.query_row(
        "SELECT rr_earned FROM comp_updates WHERE puuid = ?1 AND match_id = ?2",
        params![puuid, match_id],
        |row| row.get(0),
    )
    .optional()
}

/// Up to `n` newest competitive matches as `MatchSummary`. `map_uuid`
/// turns the stored Riot map path into a valorant-api uuid; an unknown
/// map becomes `""` (the UI shows `—`), never the raw `/Game/...` path.
/// `roundsLost` is the other team's rounds when there are exactly two
/// teams, else `None`.
pub fn recent_matches(
    repo: &Repo,
    puuid: &str,
    n: u32,
    map_uuid: &dyn Fn(&str) -> Option<String>,
) -> Result<Vec<MatchSummary>> {
    let rows = repo.last_n_comp(puuid, n)?;
    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let teams = team_rounds(repo.conn(), &r.match_id)?;
        let rounds_won = teams
            .iter()
            .find(|(t, _)| *t == r.team_id)
            .and_then(|(_, w)| *w);
        let others: Vec<Option<i64>> = teams
            .iter()
            .filter(|(t, _)| *t != r.team_id)
            .map(|(_, w)| *w)
            .collect();
        let rounds_lost = match others.as_slice() {
            [single] => *single,
            _ => None,
        };
        out.push(MatchSummary {
            map_id: map_uuid(&r.map_id).unwrap_or_default(),
            start_ms: r.start_ms,
            result: match_result(&r.team_id, r.winning_team.as_deref()),
            rounds_won,
            rounds_lost,
            agent_id: r.agent_id.clone(),
            kills: r.kills,
            deaths: r.deaths,
            assists: r.assists,
            rr_earned: rr_earned(repo.conn(), puuid, &r.match_id)?.map(|v| v as i32),
            match_id: r.match_id,
        });
    }
    Ok(out)
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

        // Unknown map: empty id, never the Riot path.
        let none = |_: &str| None;
        assert_eq!(recent_matches(&r, "p1", 8, &none).unwrap()[0].map_id, "");
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
