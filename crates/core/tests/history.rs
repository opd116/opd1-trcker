//! R189: the Match History page (`Repo::match_history`) — order, paging,
//! the queue tabs, the stat line rules (R102), map ids, the skipped
//! not-yet-stored matches, and a 300-match history that stays fast.

use std::path::Path;
use std::time::Instant;

#[path = "common/scale.rs"]
mod scale;

use opd1_core::db::history::{HISTORY_MAX_LIMIT, HISTORY_PAGE_SQL, HISTORY_TOTALS_SQL};
use opd1_core::db::open_db;
use opd1_core::db::repo::{CompUpdateRow, HistoryEntry, Match, MatchPlayer, MatchTeam, Repo};
use opd1_core::model::MatchResultKind;
use opd1_core::wire::{HistoryPage, HistoryQueue};

const ME: &str = "me";
const ASCENT_PATH: &str = "/Game/Maps/Ascent/Ascent";
const ASCENT_UUID: &str = "7eaecc1b-4337-bbf6-6ab9-04b8f06b3319";

fn repo() -> Repo {
    Repo::new(open_db(Path::new(":memory:")).unwrap())
}

fn ascent(path: &str) -> Option<String> {
    (path == ASCENT_PATH).then(|| ASCENT_UUID.to_string())
}

fn me_row(rounds: Option<i64>) -> MatchPlayer {
    MatchPlayer {
        puuid: ME.into(),
        team_id: "Blue".into(),
        party_id: None,
        agent_id: Some("agent-jett".into()),
        kills: 20,
        deaths: 16,
        assists: 4,
        score: 5000,
        rounds,
        hs: 10,
        bs: 25,
        ls: 5,
        game_name: Some("Me".into()),
        tag_line: Some("TAG".into()),
        competitive_tier: Some(15),
        damage: None,
    }
}

/// Stores one match of the user's (indexed in their history list) won by
/// `winner`, 13–`lost` from Blue's side.
fn played(r: &Repo, id: &str, queue: &str, start_ms: i64, winner: Option<&str>, me: MatchPlayer) {
    let blue_won = winner == Some("Blue");
    r.upsert_match(
        &Match {
            match_id: id.into(),
            map_id: ASCENT_PATH.into(),
            queue_id: queue.into(),
            start_ms,
            winning_team: winner.map(str::to_string),
            is_ranked: queue == "competitive",
            rounds_played: Some(25),
            game_length_ms: Some(2_000_000),
        },
        &[
            MatchTeam {
                team_id: "Blue".into(),
                won: blue_won,
                rounds_won: Some(if blue_won { 13 } else { 9 }),
                rounds_won_h1: None,
            },
            MatchTeam {
                team_id: "Red".into(),
                won: winner == Some("Red"),
                rounds_won: Some(if blue_won { 9 } else { 13 }),
                rounds_won_h1: None,
            },
        ],
        &[me],
    )
    .unwrap();
    index(r, id, queue, start_ms);
}

fn index(r: &Repo, id: &str, queue: &str, start_ms: i64) {
    r.index_history(
        ME,
        &[HistoryEntry {
            match_id: id.into(),
            queue_id: queue.into(),
            start_ms,
        }],
    )
    .unwrap();
}

fn page(r: &Repo, queue: HistoryQueue, offset: u32, limit: u32) -> HistoryPage {
    r.match_history(ME, queue, offset, limit, &ascent).unwrap()
}

fn ids(p: &HistoryPage) -> Vec<&str> {
    p.rows.iter().map(|x| x.match_id.as_str()).collect()
}

#[test]
fn rows_come_newest_first_with_both_tab_counts() {
    let r = repo();
    played(
        &r,
        "c1",
        "competitive",
        1_000,
        Some("Blue"),
        me_row(Some(25)),
    );
    played(&r, "u1", "unrated", 2_000, Some("Red"), me_row(Some(25)));
    played(&r, "c2", "competitive", 3_000, None, me_row(Some(25)));
    played(
        &r,
        "d1",
        "deathmatch",
        4_000,
        Some("Blue"),
        me_row(Some(25)),
    );

    let all = page(&r, HistoryQueue::All, 0, 50);
    assert_eq!(ids(&all), ["d1", "c2", "u1", "c1"]);
    assert_eq!((all.total, all.competitive_total, all.all_total), (4, 2, 4));

    let comp = page(&r, HistoryQueue::Competitive, 0, 50);
    assert_eq!(ids(&comp), ["c2", "c1"]);
    assert_eq!(
        (comp.total, comp.competitive_total, comp.all_total),
        (2, 2, 4)
    );

    let results: Vec<MatchResultKind> = all.rows.iter().map(|x| x.result).collect();
    assert_eq!(
        results,
        [
            MatchResultKind::Win,
            MatchResultKind::Draw,
            MatchResultKind::Loss,
            MatchResultKind::Win
        ]
    );
}

#[test]
fn a_row_carries_score_stats_rr_tier_and_a_map_uuid() {
    let r = repo();
    played(
        &r,
        "c1",
        "competitive",
        1_000,
        Some("Blue"),
        me_row(Some(25)),
    );
    r.upsert_comp_updates(
        ME,
        &[CompUpdateRow {
            match_id: "c1".into(),
            season_id: "s".into(),
            tier_after: 16,
            tier_before: Some(15),
            rr_after: 3,
            rr_earned: 21,
            map_id: None,
            start_ms: 1_000,
        }],
    )
    .unwrap();
    let row = page(&r, HistoryQueue::All, 0, 10).rows.remove(0);
    assert_eq!(row.map_id.as_deref(), Some(ASCENT_UUID));
    assert_eq!(row.queue_id, "competitive");
    assert_eq!(row.start_ms, 1_000);
    assert_eq!(row.duration_ms, Some(2_000_000));
    assert_eq!((row.rounds_won, row.rounds_lost), (Some(13), Some(9)));
    assert_eq!(row.agent_id.as_deref(), Some("agent-jett"));
    assert_eq!(
        (row.kills, row.deaths, row.assists),
        (Some(20), Some(16), Some(4))
    );
    assert_eq!(row.kd, Some(1.25));
    assert_eq!(row.acs, Some(200.0), "score over rounds");
    assert_eq!(row.hs_pct, Some(0.25), "headshots over every shot landed");
    assert_eq!((row.rr_earned, row.tier_after), (Some(21), Some(16)));
}

#[test]
fn a_match_without_a_stats_row_has_every_stat_null_r102() {
    let r = repo();
    let mut me = me_row(None);
    (
        me.kills, me.deaths, me.assists, me.score, me.hs, me.bs, me.ls,
    ) = (0, 0, 0, 0, 0, 0, 0);
    played(&r, "u1", "unrated", 1_000, Some("Blue"), me);
    let row = page(&r, HistoryQueue::All, 0, 10).rows.remove(0);
    assert_eq!(
        (
            row.kills,
            row.deaths,
            row.assists,
            row.kd,
            row.acs,
            row.hs_pct
        ),
        (None, None, None, None, None, None)
    );
    assert_eq!(
        row.result,
        MatchResultKind::Win,
        "the result is still known"
    );
    assert_eq!((row.rr_earned, row.tier_after), (None, None), "no RR row");
}

#[test]
fn an_unknown_map_is_null_never_the_riot_path() {
    let r = repo();
    played(
        &r,
        "c1",
        "competitive",
        1_000,
        Some("Blue"),
        me_row(Some(25)),
    );
    let p = r
        .match_history(ME, HistoryQueue::All, 0, 10, &|_| None)
        .unwrap();
    assert_eq!(p.rows[0].map_id, None);
    assert!(!serde_json::to_string(&p).unwrap().contains("/Game/"));
}

#[test]
fn a_match_only_in_the_history_list_is_skipped_from_rows_and_totals() {
    let r = repo();
    played(
        &r,
        "c1",
        "competitive",
        1_000,
        Some("Blue"),
        me_row(Some(25)),
    );
    index(&r, "pending", "competitive", 2_000);
    // Another player's history and a stored match the user is not in.
    r.index_history(
        "someone",
        &[HistoryEntry {
            match_id: "c1".into(),
            queue_id: "competitive".into(),
            start_ms: 1_000,
        }],
    )
    .unwrap();
    let p = page(&r, HistoryQueue::Competitive, 0, 10);
    assert_eq!(ids(&p), ["c1"]);
    assert_eq!((p.total, p.competitive_total, p.all_total), (1, 1, 1));
    let empty = r
        .match_history("nobody", HistoryQueue::All, 0, 10, &ascent)
        .unwrap();
    assert!(empty.rows.is_empty());
    assert_eq!(empty.all_total, 0);
}

#[test]
fn pages_follow_on_from_each_other_and_the_limit_is_capped() {
    let r = repo();
    for i in 0..10_i64 {
        let queue = if i % 2 == 0 { "competitive" } else { "unrated" };
        played(
            &r,
            &format!("m{i:02}"),
            queue,
            1_000 + i,
            Some("Blue"),
            me_row(Some(25)),
        );
    }
    let first = page(&r, HistoryQueue::All, 0, 4);
    let second = page(&r, HistoryQueue::All, 4, 4);
    let third = page(&r, HistoryQueue::All, 8, 4);
    assert_eq!(ids(&first), ["m09", "m08", "m07", "m06"]);
    assert_eq!(ids(&second), ["m05", "m04", "m03", "m02"]);
    assert_eq!(ids(&third), ["m01", "m00"]);
    assert!(page(&r, HistoryQueue::All, 10, 4).rows.is_empty());
    let comp = page(&r, HistoryQueue::Competitive, 2, 2);
    assert_eq!(ids(&comp), ["m04", "m02"]);
    // Limit 0: the counts only.
    let counts = page(&r, HistoryQueue::All, 0, 0);
    assert!(counts.rows.is_empty());
    assert_eq!((counts.all_total, counts.competitive_total), (10, 5));
    // A limit over the cap is the cap (checked on the 300 below too).
    assert_eq!(page(&r, HistoryQueue::All, 0, u32::MAX).rows.len(), 10);
}

#[test]
fn a_300_match_history_pages_correctly_and_fast() {
    let r = repo();
    for i in 0..300_i64 {
        let queue = ["competitive", "unrated", "deathmatch"][(i % 3) as usize];
        let winner = if i % 4 == 0 { "Red" } else { "Blue" };
        played(
            &r,
            &format!("m{i:03}"),
            queue,
            1_000_000 + i * 3_600_000,
            Some(winner),
            me_row(Some(25)),
        );
    }

    let started = Instant::now();
    let all = page(&r, HistoryQueue::All, 0, 500);
    let comp = page(&r, HistoryQueue::Competitive, 0, 50);
    let deep = page(&r, HistoryQueue::All, 250, 50);
    let took = started.elapsed();

    assert_eq!(all.rows.len(), HISTORY_MAX_LIMIT as usize);
    assert_eq!((all.all_total, all.competitive_total), (300, 100));
    assert_eq!(all.rows[0].match_id, "m299");
    assert!(all.rows.windows(2).all(|w| w[0].start_ms > w[1].start_ms));
    assert_eq!(comp.rows.len(), 50);
    assert!(comp.rows.iter().all(|x| x.queue_id == "competitive"));
    assert_eq!(comp.rows[0].match_id, "m297");
    assert_eq!(ids(&deep).first(), Some(&"m049"));
    assert_eq!(ids(&deep).last(), Some(&"m000"));
    assert!(
        took.as_millis() < 500,
        "three pages of a 300-match history took {took:?}"
    );
}

#[test]
fn the_page_query_walks_the_users_history_index_and_reads_the_rest_by_key() {
    for sql in [HISTORY_PAGE_SQL, HISTORY_TOTALS_SQL] {
        let plan = scale::schema_only_plan_of(sql);
        assert!(
            scale::base_table_scans(&plan).is_empty(),
            "a table scan: {plan:#?}"
        );
        assert!(
            plan.iter()
                .any(|d| d.contains("history_index") && d.contains("puuid=?")),
            "driven by the user's history_index rows: {plan:#?}"
        );
        assert!(
            !plan.iter().any(|d| d.contains("CORRELATED")),
            "correlated subquery: {plan:#?}"
        );
    }
}
