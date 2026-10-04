//! The synthetic "heavy user" database the scale tests run against (B7).
//!
//! Shared by `crates/core/tests/db_scale.rs` and the shell's
//! `src-tauri/src/pipeline_tests/db_scale.rs` (which pulls this very file in
//! with `#[path]`), so both measure the same data. It lives in a
//! subdirectory of `tests/` so cargo does not take it for a test target of
//! its own.
//!
//! Built through the real [`open_db`] and the real [`Repo`] write paths
//! (`upsert_match`, `upsert_comp_updates`, `index_history`, `upsert_player`,
//! `save_mmr_snapshot`, `record_name_answers`), so the rows are the ones the
//! app itself stores. Only `synchronous` is lowered to `OFF` while building
//! (a build-time speed-up; the file is closed and reopened by `open_db`
//! afterwards, with the app's own pragmas). Everything is arithmetic on a
//! fixed-seed generator: the same database on every run and machine.
//!
//! The shape (see [`Counts`] for the measured row counts):
//! * 5 000 matches the user played, 2 hours apart (about 14 months), 40 %
//!   competitive, 10 players each (Deathmatch: ten teams of one);
//! * 3 000 more matches the user is NOT in (80 % competitive): what the live
//!   sync stores about the strangers on screen (their recent matches) and the
//!   duo's own games;
//! * the other 2 000 players are drawn with a squared-uniform skew, so a
//!   few recurring teammates and opponents dominate and most are strangers
//!   seen a handful of times; [`DUO`] is in 45 % of the user's team matches
//!   (36 % with the user, 9 % against: a duo partner, thousands of shared
//!   rows) and in about one in ten of the others;
//! * the user's own competitive updates, 8 stored rows each for 300 strangers
//!   and the duo, the history list of every player (the user's whole history;
//!   a stranger's last two matches and last five competitive ones), an MMR
//!   snapshot for the same players, and a `players` row for nearly everyone,
//!   a tenth of them with a remembered blank name-service answer;
//! * `ANALYZE` has run, so `sqlite_stat1` is there as it will be in the app
//!   once `PRAGMA optimize` has run on exit.

#![allow(dead_code)] // each test target uses its own part of this file

use std::path::{Path, PathBuf};
use std::time::Instant;

use opd1_core::db::open_db;
use opd1_core::db::repo::{
    CompUpdateRow, HistoryEntry, Match, MatchPlayer, MatchTeam, MmrSnapshot, NameAsk, Player, Repo,
};
use rusqlite::types::Null;
use rusqlite::Connection;
use serde_json::json;

/// The matches the user played.
pub const MATCHES: usize = 5_000;
/// The matches stored without the user (strangers' recent games, the duo's).
pub const OTHER_MATCHES: usize = 3_000;
/// The players other than the user: pool indexes `0..OTHERS`, the last one
/// being [`DUO`].
pub const OTHERS: usize = 2_000;
const HOUR_MS: i64 = 3_600_000;
const DAY_MS: i64 = 24 * HOUR_MS;
const SPACING_MS: i64 = 2 * HOUR_MS;
const BASE_MS: i64 = 1_735_689_600_000; // 2025-01-01
/// "Now" for the tests: just after the newest match.
pub const NOW_MS: i64 = BASE_MS + MATCHES as i64 * SPACING_MS + HOUR_MS;
const ACT_LEN_MS: i64 = 60 * DAY_MS;
/// The current act began 50 days ago, so about 600 of the user's matches
/// (240 competitive) are in it.
pub const CURRENT_ACT_START_MS: i64 = NOW_MS - 50 * DAY_MS;

pub const USER: &str = "a0000000-0000-4000-8000-000000000000";
/// The recurring duo partner: pool index `OTHERS - 1`.
pub const DUO: &str = "f00007cf-0000-4000-8000-000000000000";

/// The pool player with index `i` (`0..OTHERS`).
pub fn pu(i: usize) -> String {
    assert!(i < OTHERS);
    format!("f{i:07x}-0000-4000-8000-000000000000")
}

/// A stranger with a handful of stored matches and nothing else known.
pub fn stranger() -> String {
    pu(1_900)
}

/// A ten-player roster: the user and nine others from the most to the least
/// frequent in the DB.
pub fn roster() -> Vec<String> {
    let mut r = vec![USER.to_string(), DUO.to_string()];
    r.extend([0, 3, 40, 300, 900, 1_200, 1_500, 1_900].map(pu));
    r
}

/// The act the current time is in.
#[derive(Debug, Clone)]
pub struct ActWindow {
    pub id: String,
    pub start_ms: i64,
    pub end_ms: i64,
}

/// Riot's season id of the act `start_ms` falls in.
pub fn season_id(start_ms: i64) -> String {
    format!(
        "season-{}",
        100 + (start_ms - CURRENT_ACT_START_MS).div_euclid(ACT_LEN_MS)
    )
}

/// Row counts of the built database.
#[derive(Debug, Clone, Default)]
pub struct Counts {
    pub matches: i64,
    pub match_teams: i64,
    pub match_players: i64,
    pub comp_matches: i64,
    pub comp_updates: i64,
    pub user_comp_updates: i64,
    pub history_index: i64,
    pub players: i64,
    pub mmr_snapshots: i64,
    pub stat1_rows: i64,
    pub user_version: i64,
    pub file_bytes: u64,
}

/// The built database; the temp directory (and the file) goes away with it.
pub struct Scale {
    _dir: tempfile::TempDir,
    pub path: PathBuf,
    pub act: ActWindow,
    pub counts: Counts,
    /// A question whose puuid has a remembered blank name-service answer, so
    /// `names_to_ask` drops it.
    pub blank_ask: NameAsk,
}

/// SplitMix64: tiny, fixed-seed, the same sequence everywhere.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    /// A pool index, skewed to the low end (`u²`): the head recurs a lot,
    /// the tail is strangers.
    fn skewed(&mut self) -> usize {
        let r = self.below(1_000_000);
        (r * r * (OTHERS as u64 - 1) / 1_000_000_000_000) as usize
    }
    fn pick_new(&mut self, used: &mut Vec<usize>) -> usize {
        loop {
            let i = self.skewed();
            if !used.contains(&i) {
                used.push(i);
                return i;
            }
        }
    }
}

const MAPS: [&str; 10] = [
    "/Game/Maps/Ascent/Ascent",
    "/Game/Maps/Bonsai/Bonsai",
    "/Game/Maps/Canyon/Canyon",
    "/Game/Maps/Duality/Duality",
    "/Game/Maps/Foxtrot/Foxtrot",
    "/Game/Maps/Triad/Triad",
    "/Game/Maps/Port/Port",
    "/Game/Maps/Jam/Jam",
    "/Game/Maps/Pitt/Pitt",
    "/Game/Maps/Infinity/Infinity",
];

fn agent(n: u64) -> String {
    format!("agent-{:02}", n % 24)
}

pub fn match_id(i: usize) -> String {
    format!("{i:08x}-1111-4000-8000-{i:012x}")
}

/// A stranger's rank in matches: stable per player.
fn tier_of(idx: usize) -> i64 {
    3 + ((idx * 7 + 3) % 22) as i64
}

struct GenMatch {
    m: Match,
    teams: Vec<MatchTeam>,
    players: Vec<MatchPlayer>,
    /// The pool indexes of the non-user players.
    others: Vec<usize>,
    /// The user played it.
    user_in: bool,
}

/// One other player's row. A tenth of the rows have no name (hidden when the
/// match was played), half of those `NULL`, half empty.
fn other_row(
    rng: &mut Rng,
    o: usize,
    team_id: String,
    party_id: Option<String>,
    rounds: i64,
    competitive_tier: Option<i64>,
    phase2: bool,
) -> MatchPlayer {
    let (k, d, a, score, hs, bs, ls, dmg) = stat_line(rng, rounds);
    let hidden = rng.below(100) < 10;
    let (game_name, tag_line) = if !hidden {
        (Some(format!("Name{o}")), Some(format!("{o:04}")))
    } else if rng.below(2) == 0 {
        (None, None)
    } else {
        (Some(String::new()), Some(String::new()))
    };
    MatchPlayer {
        puuid: pu(o),
        team_id,
        party_id,
        agent_id: Some(agent(rng.next())),
        kills: k,
        deaths: d,
        assists: a,
        score,
        rounds: Some(rounds),
        hs,
        bs,
        ls,
        game_name,
        tag_line,
        competitive_tier,
        damage: phase2.then_some(dmg),
    }
}

fn stat_line(rng: &mut Rng, rounds: i64) -> (i64, i64, i64, i64, i64, i64, i64, i64) {
    let kills = rng.below(30) as i64;
    let deaths = 4 + rng.below(20) as i64;
    let assists = rng.below(12) as i64;
    let hs = kills * (10 + rng.below(30) as i64) / 10;
    let bs = kills * 4 + rng.below(20) as i64;
    let ls = rng.below(12) as i64;
    let score = kills * 220 + assists * 60 + rounds * 20;
    let damage = kills * 140 + rounds * 25;
    (kills, deaths, assists, score, hs, bs, ls, damage)
}

/// Builds the database. Takes a few seconds in a debug build.
pub fn build() -> Scale {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("scale.db");
    let conn = open_db(&path).expect("open_db");
    // Build-time speed only; `open_db` below sets the app's pragmas again.
    conn.pragma_update(None, "synchronous", "OFF").unwrap();
    let repo = Repo::new(conn);

    let mut rng = Rng(0x0BD1_5CA1_E000_0007);
    let mut gen: Vec<GenMatch> = Vec::with_capacity(MATCHES);
    let mut user_updates: Vec<CompUpdateRow> = Vec::new();
    let (mut tier, mut rr) = (12_i64, 40_i64);

    for i in 0..MATCHES {
        let start_ms = BASE_MS + i as i64 * SPACING_MS + rng.below(20 * 60_000) as i64;
        let queue = match i % 5 {
            0 | 2 => "competitive",
            1 => "unrated",
            3 => "deathmatch",
            _ => "spikerush",
        };
        let dm = queue == "deathmatch";
        let comp = queue == "competitive";
        let (mine, theirs) = if rng.below(2) == 0 {
            ("Blue", "Red")
        } else {
            ("Red", "Blue")
        };
        let draw = comp && rng.below(100) < 1;
        let user_won = !draw && rng.below(100) < 50;
        let loser_rounds = if draw { 12 } else { rng.below(12) as i64 };
        let rounds_played = if dm {
            1
        } else if draw {
            24
        } else {
            13 + loser_rounds
        };
        let map = MAPS[rng.below(MAPS.len() as u64) as usize];
        let id = match_id(i);

        // Who is in it: the duo in ~45 % of the team matches, the rest from
        // the skewed pool.
        let mut used: Vec<usize> = Vec::new();
        let duo_in = rng.below(100) < if dm { 20 } else { 45 };
        if duo_in {
            used.push(OTHERS - 1);
        }
        let n_mates = if dm { 0 } else { 4 };
        while used.len() < 9 {
            rng.pick_new(&mut used);
        }
        // `used` = [duo?] + picks; in a team match the first `n_mates` are
        // the user's team (the duo, when in, is the first of them) — but one
        // time in five the duo is on the other team.
        let mut others = used.clone();
        let duo_against = duo_in && !dm && rng.below(5) == 0;
        if duo_against {
            others.swap(0, n_mates);
        }

        let winner_dm = if user_won {
            USER.to_string()
        } else {
            pu(others[0])
        };
        let (winning_team, teams): (Option<String>, Vec<MatchTeam>) = if dm {
            let teams = std::iter::once(USER.to_string())
                .chain(others.iter().map(|&o| pu(o)))
                .map(|t| MatchTeam {
                    won: t == winner_dm,
                    team_id: t,
                    rounds_won: None,
                    rounds_won_h1: None,
                })
                .collect();
            (Some(winner_dm), teams)
        } else {
            let (mine_rounds, their_rounds) = if draw {
                (12, 12)
            } else if user_won {
                (13, loser_rounds)
            } else {
                (loser_rounds, 13)
            };
            let h1 = |r: i64| Some((r + 1) / 2);
            let teams = vec![
                MatchTeam {
                    team_id: mine.into(),
                    won: user_won,
                    rounds_won: Some(mine_rounds),
                    rounds_won_h1: h1(mine_rounds),
                },
                MatchTeam {
                    team_id: theirs.into(),
                    won: !user_won && !draw,
                    rounds_won: Some(their_rounds),
                    rounds_won_h1: h1(their_rounds),
                },
            ];
            let winning = (!draw).then(|| if user_won { mine } else { theirs }.to_string());
            (winning, teams)
        };

        // The user's own tier ladder moves only in competitive matches.
        let user_tier = tier;
        if comp {
            let before = tier;
            let earned = if draw {
                0
            } else if user_won {
                18 + rng.below(5) as i64
            } else {
                -(18 + rng.below(5) as i64)
            };
            rr += earned;
            if rr >= 100 {
                if tier < 20 {
                    tier += 1;
                    rr -= 100;
                } else {
                    rr = 99;
                }
            } else if rr < 0 {
                if tier > 12 {
                    tier -= 1;
                    rr += 100;
                } else {
                    rr = 0;
                }
            }
            user_updates.push(CompUpdateRow {
                match_id: id.clone(),
                season_id: season_id(start_ms),
                tier_after: tier,
                tier_before: Some(before),
                rr_after: rr,
                rr_earned: earned,
                map_id: Some(map.to_string()),
                start_ms,
            });
        }

        let pre_migration2 = i < 250;
        let tier_in_match = |t: i64| {
            if pre_migration2 {
                None
            } else if comp {
                Some(t)
            } else {
                Some(0)
            }
        };

        let mut players = Vec::with_capacity(10);
        let (k, d, a, score, hs, bs, ls, dmg) = stat_line(&mut rng, rounds_played);
        players.push(MatchPlayer {
            puuid: USER.to_string(),
            team_id: if dm {
                USER.to_string()
            } else {
                mine.to_string()
            },
            party_id: Some(format!("party-{i}")),
            agent_id: Some(agent(rng.next())),
            kills: k,
            deaths: d,
            assists: a,
            score,
            rounds: Some(rounds_played),
            hs,
            bs,
            ls,
            game_name: Some("OPD1".into()),
            tag_line: Some("TRCK".into()),
            competitive_tier: tier_in_match(user_tier),
            damage: (!pre_migration2).then_some(dmg),
        });
        for (slot, &o) in others.iter().enumerate() {
            let team = if dm {
                pu(o)
            } else if slot < n_mates {
                mine.to_string()
            } else {
                theirs.to_string()
            };
            let party = if duo_in && !duo_against && o == OTHERS - 1 {
                players[0].party_id.clone()
            } else {
                Some(format!("q-{i}-{slot}"))
            };
            players.push(other_row(
                &mut rng,
                o,
                team,
                party,
                rounds_played,
                tier_in_match(tier_of(o)),
                !pre_migration2,
            ));
        }

        gen.push(GenMatch {
            m: Match {
                match_id: id,
                map_id: map.to_string(),
                queue_id: queue.to_string(),
                start_ms,
                winning_team,
                is_ranked: comp,
                rounds_played: Some(rounds_played),
                game_length_ms: (!pre_migration2).then(|| 1_800_000 + rng.below(900_000) as i64),
            },
            teams,
            players,
            others,
            user_in: true,
        });
    }

    // Matches the user is not in: what the sync stores for the strangers on
    // screen, spread over the same months.
    for j in 0..OTHER_MATCHES {
        let start_ms = BASE_MS + rng.below(MATCHES as u64 * SPACING_MS as u64) as i64;
        let queue = match j % 10 {
            0..=7 => "competitive",
            8 => "unrated",
            _ => "spikerush",
        };
        let comp = queue == "competitive";
        let blue_won = rng.below(2) == 0;
        let loser_rounds = rng.below(12) as i64;
        let rounds_played = 13 + loser_rounds;
        let map = MAPS[rng.below(MAPS.len() as u64) as usize];
        let mut used: Vec<usize> = Vec::new();
        if rng.below(100) < 10 {
            used.push(OTHERS - 1);
        }
        while used.len() < 10 {
            rng.pick_new(&mut used);
        }
        let h1 = |r: i64| Some((r + 1) / 2);
        let (blue_rounds, red_rounds) = if blue_won {
            (13, loser_rounds)
        } else {
            (loser_rounds, 13)
        };
        let players: Vec<MatchPlayer> = used
            .iter()
            .enumerate()
            .map(|(slot, &o)| {
                let team = if slot < 5 { "Blue" } else { "Red" };
                let tier = if comp { tier_of(o) } else { 0 };
                other_row(
                    &mut rng,
                    o,
                    team.to_string(),
                    Some(format!("q-{j}-{slot}")),
                    rounds_played,
                    Some(tier),
                    true,
                )
            })
            .collect();
        gen.push(GenMatch {
            m: Match {
                match_id: match_id(MATCHES + j),
                map_id: map.to_string(),
                queue_id: queue.to_string(),
                start_ms,
                winning_team: Some(if blue_won { "Blue" } else { "Red" }.to_string()),
                is_ranked: comp,
                rounds_played: Some(rounds_played),
                game_length_ms: Some(1_800_000 + rng.below(900_000) as i64),
            },
            teams: vec![
                MatchTeam {
                    team_id: "Blue".into(),
                    won: blue_won,
                    rounds_won: Some(blue_rounds),
                    rounds_won_h1: h1(blue_rounds),
                },
                MatchTeam {
                    team_id: "Red".into(),
                    won: !blue_won,
                    rounds_won: Some(red_rounds),
                    rounds_won_h1: h1(red_rounds),
                },
            ],
            players,
            others: used,
            user_in: false,
        });
    }
    // Oldest first, so "the newest of a player's matches" is the last one.
    gen.sort_by_key(|g| g.m.start_ms);

    // matches, teams, players: the real insert path, one transaction each.
    for g in &gen {
        repo.upsert_match(&g.m, &g.teams, &g.players).unwrap();
    }

    // The user's competitive updates and whole history (the own-history
    // backfill indexes everything).
    repo.upsert_comp_updates(USER, &user_updates).unwrap();
    let all_history: Vec<HistoryEntry> = gen
        .iter()
        .filter(|g| g.user_in)
        .map(|g| HistoryEntry {
            match_id: g.m.match_id.clone(),
            queue_id: g.m.queue_id.clone(),
            start_ms: g.m.start_ms,
        })
        .collect();
    repo.index_history(USER, &all_history).unwrap();

    // Each other player's appearances, oldest first.
    let mut seen_in: Vec<Vec<usize>> = vec![Vec::new(); OTHERS];
    for (i, g) in gen.iter().enumerate() {
        for &o in &g.others {
            seen_in[o].push(i);
        }
    }
    let with_snapshot = |o: usize| o == OTHERS - 1 || (o.is_multiple_of(5) && o < 1_500);
    for (o, appearances) in seen_in.iter().enumerate() {
        if appearances.is_empty() {
            continue;
        }
        let who = pu(o);
        // The sync's own picks: last 2 of any queue + last 5 competitive.
        let mut keep: Vec<usize> = appearances.iter().rev().take(2).copied().collect();
        let comp: Vec<usize> = appearances
            .iter()
            .rev()
            .copied()
            .filter(|&i| gen[i].m.queue_id == "competitive")
            .collect();
        for &i in comp.iter().take(5) {
            if !keep.contains(&i) {
                keep.push(i);
            }
        }
        let entries: Vec<HistoryEntry> = keep
            .iter()
            .map(|&i| HistoryEntry {
                match_id: gen[i].m.match_id.clone(),
                queue_id: gen[i].m.queue_id.clone(),
                start_ms: gen[i].m.start_ms,
            })
            .collect();
        repo.index_history(&who, &entries).unwrap();

        if with_snapshot(o) {
            let rows: Vec<CompUpdateRow> = comp
                .iter()
                .take(8)
                .enumerate()
                .map(|(n, &i)| CompUpdateRow {
                    match_id: gen[i].m.match_id.clone(),
                    season_id: season_id(gen[i].m.start_ms),
                    tier_after: tier_of(o),
                    tier_before: Some(tier_of(o)),
                    rr_after: 20 + ((o + n) % 70) as i64,
                    rr_earned: if n % 2 == 0 { 21 } else { -17 },
                    map_id: Some(gen[i].m.map_id.clone()),
                    start_ms: gen[i].m.start_ms,
                })
                .collect();
            repo.upsert_comp_updates(&who, &rows).unwrap();
            let newest = comp.first().map(|&i| gen[i].m.match_id.clone());
            repo.save_mmr_snapshot(&MmrSnapshot {
                puuid: who.clone(),
                json: mmr_json(&who, tier_of(o)),
                last_comp_match_id: newest,
                fetched_ms: NOW_MS - o as i64,
            })
            .unwrap();
        }
    }
    repo.save_mmr_snapshot(&MmrSnapshot {
        puuid: USER.into(),
        json: mmr_json(USER, tier),
        last_comp_match_id: user_updates.last().map(|u| u.match_id.clone()),
        fetched_ms: NOW_MS,
    })
    .unwrap();

    // `players`: the user and 90 % of the others with a name (a visible
    // sighting); a tenth of the others with a remembered blank
    // name-service answer; the rest not at all.
    repo.upsert_player(&Player {
        puuid: USER.into(),
        game_name: "OPD1".into(),
        tag_line: "TRCK".into(),
        account_level: 312,
        card_id: "card-user".into(),
        updated_ms: NOW_MS,
    })
    .unwrap();
    let mut blank_asks = Vec::new();
    for o in 0..OTHERS {
        // (The duo has been on screen a thousand times: always a row.)
        match if o == OTHERS - 1 { 0 } else { o % 20 } {
            0..=17 => repo
                .upsert_player(&Player {
                    puuid: pu(o),
                    game_name: format!("Name{o}"),
                    tag_line: format!("{o:04}"),
                    account_level: 20 + (o % 400) as i64,
                    card_id: format!("card-{}", o % 50),
                    updated_ms: NOW_MS - (o as i64 % 90) * DAY_MS,
                })
                .unwrap(),
            18 => blank_asks.push(NameAsk {
                puuid: pu(o),
                match_start_ms: Some(NOW_MS - (o as i64 % 300) * DAY_MS),
            }),
            _ => {}
        }
    }
    repo.record_name_answers(&[], &blank_asks, NOW_MS).unwrap();
    repo.set_state("self_puuid", USER).unwrap();
    // The statistics the app's `PRAGMA optimize` will have written by then.
    repo.conn().execute_batch("ANALYZE").unwrap();

    let count = |sql: &str| -> i64 { repo.conn().query_row(sql, [], |r| r.get(0)).unwrap() };
    let mut counts = Counts {
        matches: count("SELECT COUNT(*) FROM matches"),
        match_teams: count("SELECT COUNT(*) FROM match_teams"),
        match_players: count("SELECT COUNT(*) FROM match_players"),
        comp_matches: count("SELECT COUNT(*) FROM matches WHERE queue_id = 'competitive'"),
        comp_updates: count("SELECT COUNT(*) FROM comp_updates"),
        user_comp_updates: 0,
        history_index: count("SELECT COUNT(*) FROM history_index"),
        players: count("SELECT COUNT(*) FROM players"),
        mmr_snapshots: count("SELECT COUNT(*) FROM mmr_snapshots"),
        stat1_rows: count("SELECT COUNT(*) FROM sqlite_stat1"),
        user_version: count("PRAGMA user_version"),
        file_bytes: 0,
    };
    counts.user_comp_updates = user_updates.len() as i64;
    drop(repo); // closes the connection: the WAL is folded into the file
    counts.file_bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);

    Scale {
        _dir: dir,
        path,
        act: ActWindow {
            id: season_id(NOW_MS),
            start_ms: CURRENT_ACT_START_MS,
            end_ms: CURRENT_ACT_START_MS + ACT_LEN_MS,
        },
        counts,
        blank_ask: blank_asks.swap_remove(0),
    }
}

impl Scale {
    /// A fresh connection the way the app opens it.
    pub fn open(&self) -> Repo {
        Repo::new(open_db(&self.path).expect("reopen"))
    }
}

/// A trimmed MMR snapshot (the shell's own stored shape: the competitive
/// queue's seasons, 7 fields each, `SnapshotVersion` 2), 8 acts.
fn mmr_json(puuid: &str, tier: i64) -> String {
    let mut seasons = serde_json::Map::new();
    for k in 93..=100_i64 {
        let id = format!("season-{k}");
        seasons.insert(
            id.clone(),
            json!({
                "SeasonID": id,
                "CompetitiveTier": (tier - (100 - k)).max(3),
                "RankedRating": 42 + k,
                "NumberOfWins": 90 + k,
                "NumberOfWinsWithPlacements": 92 + k,
                "NumberOfGames": 180 + k,
                "WinsByTier": {"12": 20, "13": 30, "14": 25, "15": 17},
            }),
        );
    }
    json!({
        "Subject": puuid,
        "SnapshotVersion": 2,
        "QueueSkills": {"competitive": {"SeasonalInfoBySeasonID": seasons}}
    })
    .to_string()
}

/// The tripwire for one measured step, in milliseconds. Deliberately loose: a
/// timing test shares the machine with the rest of a parallel test run (the
/// shell's 500+ tests on a 4-core machine measured a 2-5x slowdown of a 50 ms
/// step), and what it must still catch is a query that began to read a whole
/// table, which costs seconds at this size, not a few percent. The finer
/// guards are the plan tests (no `SCAN`) and the relative checks in
/// `db_scale.rs`.
pub const BOUND_MS: f64 = 1000.0;
/// The goal for one measured step: the table marks every line above it.
pub const TARGET_MS: f64 = 50.0;

/// The measured steps of one test, in the order they ran.
#[derive(Default)]
pub struct Timings(Vec<(String, f64)>);

impl Timings {
    /// Runs `f` three times, and four times more when the fastest was under a
    /// millisecond (a step that short is mostly noise), keeping the fastest
    /// (the first run also warms the OS cache); returns the last result.
    pub fn time<T>(&mut self, name: &str, mut f: impl FnMut() -> T) -> T {
        let mut best = f64::MAX;
        let mut out = None;
        let mut runs = 3;
        let mut done = 0;
        while done < runs {
            let t = Instant::now();
            let r = f();
            best = best.min(t.elapsed().as_secs_f64() * 1000.0);
            out = Some(r);
            done += 1;
            if done == 3 && best < 1.0 {
                runs = 7;
            }
        }
        self.record(name, best);
        out.unwrap()
    }

    /// Adds a step measured by the caller (an async one), in milliseconds.
    pub fn record(&mut self, name: &str, ms: f64) {
        self.0.push((name.to_string(), ms));
    }

    /// The milliseconds of the step `name` (the one recorded last).
    pub fn ms_of(&self, name: &str) -> f64 {
        self.0
            .iter()
            .rev()
            .find(|(n, _)| n == name)
            .unwrap_or_else(|| panic!("no step `{name}`"))
            .1
    }

    /// Prints every step (stderr: shown by `cargo test -- --nocapture` and
    /// whenever the test fails) and fails when one is over [`BOUND_MS`].
    pub fn finish(&self, title: &str) {
        let width = self.0.iter().map(|(n, _)| n.len()).max().unwrap_or(0);
        eprintln!(
            "{title} (best of 3, of 7 under 1 ms; tripwire {BOUND_MS} ms, goal {TARGET_MS} ms):"
        );
        for (name, ms) in &self.0 {
            let flag = if *ms > TARGET_MS {
                "  <- over the goal"
            } else {
                ""
            };
            eprintln!("  {name:<width$}  {ms:8.2} ms{flag}");
        }
        let slow: Vec<String> = self
            .0
            .iter()
            .filter(|(_, ms)| *ms > BOUND_MS)
            .map(|(n, ms)| format!("{n}: {ms:.1} ms"))
            .collect();
        assert!(
            slow.is_empty(),
            "slower than {BOUND_MS} ms on the scale db: {slow:#?}"
        );
    }
}

// ---------------------------------------------------------------------
// query plans
// ---------------------------------------------------------------------

/// The `detail` column of `EXPLAIN QUERY PLAN <sql>` on `conn`, every
/// parameter bound to `NULL` (the plan does not depend on the values).
pub fn plan_of(conn: &Connection, sql: &str) -> Vec<String> {
    let mut stmt = conn
        .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
        .unwrap_or_else(|e| panic!("`{sql}` does not prepare: {e}"));
    let nulls = std::iter::repeat_n(Null, stmt.parameter_count());
    let rows = stmt
        .query_map(rusqlite::params_from_iter(nulls), |r| r.get::<_, String>(3))
        .unwrap();
    rows.map(Result::unwrap).collect()
}

/// A plan on a migrated database with no rows and no statistics: the plan
/// SQLite chooses from the schema alone.
pub fn schema_only_plan_of(sql: &str) -> Vec<String> {
    plan_of(&open_db(Path::new(":memory:")).unwrap(), sql)
}

/// The steps that read a base table in full: a `SCAN`, but not the read-back
/// of a CTE or subquery the plan built first (`CO-ROUTINE x` / `MATERIALIZE x`
/// then `SCAN x`, or `SCAN (subquery-N)`): that is a small result set. (`SCAN
/// CONSTANT ROW` is a `SELECT` without a `FROM`.)
pub fn base_table_scans(plan: &[String]) -> Vec<&String> {
    let built: Vec<&str> = plan
        .iter()
        .filter_map(|d| {
            d.strip_prefix("CO-ROUTINE ")
                .or_else(|| d.strip_prefix("MATERIALIZE "))
        })
        .collect();
    plan.iter()
        .filter(|d| {
            d.strip_prefix("SCAN ").is_some_and(|target| {
                !target.starts_with("(subquery")
                    && target != "CONSTANT ROW"
                    && !built.iter().any(|b| target.split(' ').next() == Some(*b))
            })
        })
        .collect()
}

/// Checks that `ANALYZE` (the statistics of the scale database `conn`) did
/// not change any of the `(name, sql)` plans from the schema-only one the plan
/// tests pin, and that none of them scans a base table. A plan is unchanged
/// when it has the same steps and starts from the same table: the order of the
/// key seeks after the first step is SQLite's to pick (only the first, the
/// table that drives, is pinned by `CROSS JOIN`), so a swap of two of them is
/// printed, not returned. Returns the queries whose plan changed otherwise
/// (an empty list: none).
pub fn plans_with_statistics(conn: &Connection, queries: &[(&str, &str)]) -> Vec<String> {
    let stats: i64 = conn
        .query_row("SELECT COUNT(*) FROM sqlite_stat1", [], |r| r.get(0))
        .unwrap();
    assert!(stats > 0, "the scale database was not analyzed");
    let mut changed = Vec::new();
    for (name, sql) in queries {
        let with = plan_of(conn, sql);
        let without = schema_only_plan_of(sql);
        assert!(
            base_table_scans(&with).is_empty(),
            "{name}: a table scan with statistics: {with:#?}"
        );
        if with == without {
            continue;
        }
        let sorted = |plan: &[String]| {
            let mut v = plan.to_vec();
            v.sort();
            v
        };
        if with.first() == without.first() && sorted(&with) == sorted(&without) {
            eprintln!("plan with statistics, same steps in another order: {name}: {without:?} -> {with:?}");
        } else {
            changed.push(format!("{name}: {without:?} -> {with:?}"));
        }
    }
    changed
}
