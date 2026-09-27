//! Tests for `opd1_core::mystats` — the My Stats aggregation (Task 9a).
//! Everything is pure: rows in, `MyStats` out, no DB.

use std::collections::HashMap;

use serde_json::json;

use opd1_core::db::repo::{CompMatchRow, CompUpdateRow};
use opd1_core::model::{CurrentRank, InsightKind, MatchResultKind, StreakMark};
use opd1_core::mystats::{build, rr_to_next, MyStatsInput};
use opd1_core::stats::Act;

const MINUTE: i64 = 60_000;
const HOUR: i64 = 60 * MINUTE;
const DAY: i64 = 24 * HOUR;
/// Monday 2025-08-18 00:00 UTC.
const ACT_START: i64 = 20_318 * DAY;
const ACT: &str = "act-cur";
const PREV: &str = "act-prev";

fn act() -> Act {
    Act {
        id: ACT.into(),
        name: "ACT V".into(),
        episode_name: "V26".into(),
        start_ms: ACT_START,
        end_ms: ACT_START + 60 * DAY,
    }
}

#[derive(Clone, Copy)]
enum R {
    W,
    L,
    D,
}

/// A stored competitive match of the user: 20/10/5, 4000 score over 20
/// rounds (ACS 200), 10/30/10 shots (HS 20%); +20 RR on a win, -15 on a
/// loss, 0 on a draw.
fn row(id: &str, start_ms: i64, r: R) -> CompMatchRow {
    let (winning_team, won, lost, rr) = match r {
        R::W => (Some("Blue"), 13, 7, 20),
        R::L => (Some("Red"), 7, 13, -15),
        R::D => (None, 12, 12, 0),
    };
    CompMatchRow {
        match_id: id.into(),
        map_id: "map-ascent".into(),
        start_ms,
        winning_team: winning_team.map(str::to_string),
        team_id: "Blue".into(),
        agent_id: Some("agent-jett".into()),
        kills: 20,
        deaths: 10,
        assists: 5,
        score: 4000,
        rounds: Some(20),
        hs: 10,
        bs: 30,
        ls: 10,
        rounds_won: Some(won),
        rounds_lost: Some(lost),
        rr_earned: Some(rr),
    }
}

fn on_map(mut r: CompMatchRow, map: &str) -> CompMatchRow {
    r.map_id = map.into();
    r
}

fn update(
    id: &str,
    season: &str,
    start_ms: i64,
    tiers: (i64, i64),
    rr_after: i64,
    rr_earned: i64,
) -> CompUpdateRow {
    CompUpdateRow {
        match_id: id.into(),
        season_id: season.into(),
        tier_before: Some(tiers.0),
        tier_after: tiers.1,
        rr_after,
        rr_earned,
        map_id: None,
        start_ms,
    }
}

fn utc(_: i64) -> i64 {
    0
}

fn gulf(_: i64) -> i64 {
    3 * HOUR
}

fn names() -> HashMap<String, String> {
    [
        ("map-ascent", "Ascent"),
        ("map-bind", "Bind"),
        ("map-split", "Split"),
        ("map-haven", "Haven"),
    ]
    .into_iter()
    .map(|(id, name)| (id.to_string(), name.to_string()))
    .collect()
}

fn input<'a>(
    act: &'a Act,
    matches: &'a [CompMatchRow],
    comp_updates: &'a [CompUpdateRow],
    map_names: &'a HashMap<String, String>,
) -> MyStatsInput<'a> {
    MyStatsInput {
        act,
        matches,
        comp_updates,
        current_rank: None,
        map_names,
        now_ms: ACT_START + 30 * DAY,
        utc_offset_ms: &utc,
    }
}

// ---------------------------------------------------------------------
// the brief's named tests
// ---------------------------------------------------------------------

#[test]
fn mystats_current_act_only() {
    let a = act();
    let n = names();
    let matches = vec![
        row("old-1", ACT_START - 2 * DAY, R::W),
        row("old-2", ACT_START - HOUR, R::W),
        row("cur-1", ACT_START + DAY, R::L),
        row("cur-2", ACT_START + 2 * DAY, R::W),
        // The act's end is exclusive: this one belongs to the next act.
        row("next-1", ACT_START + 60 * DAY, R::W),
    ];
    let updates = vec![
        update("old-1", PREV, ACT_START - 2 * DAY, (15, 16), 10, 20),
        update("old-2", PREV, ACT_START - HOUR, (16, 16), 30, 20),
        update("cur-1", ACT, ACT_START + DAY, (16, 15), 85, -15),
        update("cur-2", ACT, ACT_START + 2 * DAY, (15, 16), 5, 20),
    ];
    let s = build(input(&a, &matches, &updates, &n));

    let last5: Vec<&str> = s.last5.iter().map(|m| m.match_id.as_str()).collect();
    assert_eq!(last5, ["cur-2", "cur-1"]);
    assert_eq!((s.rank.wins, s.rank.losses), (Some(1), Some(1)));
    assert_eq!(s.rank.net_rr, Some(5));
    let series: Vec<i64> = s.rr_series.iter().map(|p| p.start_ms).collect();
    assert_eq!(series, [ACT_START + DAY, ACT_START + 2 * DAY]);
    let changes: Vec<(u8, bool)> = s.rank.changes.iter().map(|c| (c.tier, c.up)).collect();
    assert_eq!(changes, [(16, true), (15, false)]);
    assert_eq!(s.maps.iter().map(|r| r.games).sum::<u32>(), 2);
    assert_eq!(s.agents.iter().map(|r| r.games).sum::<u32>(), 2);
    assert_eq!(s.by_weekday.iter().map(|c| c.games).sum::<u32>(), 2);
    assert_eq!(s.by_daypart.iter().map(|c| c.games).sum::<u32>(), 2);
    // A day apart: the session is the newest match alone.
    assert_eq!(s.session.unwrap().started_ms, ACT_START + 2 * DAY);
}

#[test]
fn rr_to_next_immortal_edge() {
    // Ascendant 3 -> Immortal 1 is still a fixed 100 RR step.
    assert_eq!(rr_to_next(23, 60), Some(40));
    assert_eq!(rr_to_next(3, 0), Some(100));
    // Immortal RR accumulates past 100 with no fixed next-tier threshold,
    // and Radiant has no next tier at all.
    assert_eq!(rr_to_next(24, 10), None);
    assert_eq!(rr_to_next(26, 150), None);
    assert_eq!(rr_to_next(27, 480), None);
    // Unranked / placements.
    assert_eq!(rr_to_next(0, 0), None);

    let a = act();
    let n = names();
    let radiant = build(MyStatsInput {
        current_rank: Some(CurrentRank { tier: 27, rr: 480 }),
        ..input(&a, &[], &[], &n)
    });
    assert_eq!((radiant.rank.tier, radiant.rank.rr), (Some(27), Some(480)));
    assert_eq!(radiant.rank.rr_to_next, None);
    let asc3 = build(MyStatsInput {
        current_rank: Some(CurrentRank { tier: 23, rr: 60 }),
        ..input(&a, &[], &[], &n)
    });
    assert_eq!(asc3.rank.rr_to_next, Some(40));
}

#[test]
fn daypart_buckets() {
    // Local = UTC+3. Monday a week into the act, at local h:m.
    let at = |h: i64, m: i64| ACT_START + 7 * DAY + h * HOUR + m * MINUTE - 3 * HOUR;
    let matches = vec![
        row("night-early", at(4, 59), R::L),
        // 05:00 local is 02:00 UTC — Night if local time were ignored.
        row("morning-start", at(5, 0), R::W),
        row("morning-end", at(11, 59), R::L),
        row("afternoon-start", at(12, 0), R::W),
        row("afternoon-end", at(16, 59), R::W),
        row("evening-start", at(17, 0), R::L),
        row("evening-end", at(21, 59), R::L),
        row("night-late", at(22, 0), R::L),
        row("midnight", at(24, 0), R::L),
    ];
    let a = act();
    let n = names();
    let s = build(MyStatsInput {
        utc_offset_ms: &gulf,
        ..input(&a, &matches, &[], &n)
    });

    let games: Vec<u32> = s.by_daypart.iter().map(|c| c.games).collect();
    assert_eq!(games, [2, 2, 2, 3], "Morning, Afternoon, Evening, Night");
    let wins: Vec<u32> = s.by_daypart.iter().map(|c| c.wins).collect();
    assert_eq!(wins, [1, 2, 0, 0]);
    assert_eq!(s.by_daypart[0].winrate, Some(0.5));
    assert_eq!(s.by_daypart[1].winrate, Some(1.0));
    // A counted zero stays a zero.
    assert_eq!(s.by_daypart[3].winrate, Some(0.0));
}

#[test]
fn insight_best_worst_map_needs_min_5_games() {
    let a = act();
    let n = names();
    let mut matches = Vec::new();
    let mut add = |map: &str, results: &[R]| {
        for r in results {
            let i = matches.len() as i64;
            let id = format!("m{i}");
            matches.push(on_map(row(&id, ACT_START + i * DAY, *r), map));
        }
    };
    // Bind: 100% but only 4 games — never the best map.
    add("map-bind", &[R::W, R::W, R::W, R::W]);
    add("map-split", &[R::W, R::W, R::W, R::W, R::L]);
    add("map-haven", &[R::W, R::L, R::L, R::L, R::L]);
    add("map-ascent", &[R::W, R::W, R::W, R::L, R::L, R::L]);

    let s = build(input(&a, &matches, &[], &n));
    let best = s
        .insights
        .iter()
        .find(|i| i.kind == InsightKind::BestMap)
        .expect("best map");
    assert_eq!(best.text, "Split is your best map: 80% wins in 5 games.");
    assert_eq!((best.matches, best.positive), (5, true));
    let worst = s
        .insights
        .iter()
        .find(|i| i.kind == InsightKind::WorstMap)
        .expect("worst map");
    assert_eq!(
        worst.text,
        "Haven is your weakest map: 20% wins in 5 games."
    );
    assert_eq!((worst.matches, worst.positive), (5, false));
    assert!(s.insights.iter().all(|i| !i.text.contains("Bind")));

    // Only one map with 5+ games: nothing to compare, no map insight.
    let few: Vec<CompMatchRow> = matches
        .iter()
        .filter(|m| m.map_id != "map-haven" && m.map_id != "map-ascent")
        .cloned()
        .collect();
    let s = build(input(&a, &few, &[], &n));
    assert!(s
        .insights
        .iter()
        .all(|i| i.kind != InsightKind::BestMap && i.kind != InsightKind::WorstMap));
}

#[test]
fn session_uses_6h_gap() {
    let t0 = ACT_START + 10 * DAY;
    let gap = 6 * HOUR;
    let c = t0 + HOUR + gap + 1; // just over 6h after the previous match
    let matches = vec![
        row("a", t0, R::W),
        row("b", t0 + HOUR, R::L),
        row("c", c, R::W),
        row("d", c + gap, R::L), // exactly 6h: still the same session
        row("e", c + gap + 2 * HOUR, R::D),
    ];
    let a = act();
    let n = names();
    let s = build(input(&a, &matches, &[], &n));
    let session = s.session.expect("a session");
    assert_eq!(session.started_ms, c);
    assert_eq!((session.wins, session.losses), (1, 1));
    // Oldest first, draws included.
    assert_eq!(
        session.streak,
        [StreakMark::Win, StreakMark::Loss, StreakMark::Draw]
    );
    assert_eq!(session.net_rr, Some(20 - 15));
}

// ---------------------------------------------------------------------
// the rest of the eight sections
// ---------------------------------------------------------------------

#[test]
fn rank_card_counts_averages_and_recent_changes() {
    let d = |i: i64| ACT_START + i * DAY;
    let mut no_rr = row("w3", d(6), R::W);
    no_rr.rr_earned = None;
    let matches = vec![
        {
            let mut w = row("w1", d(1), R::W);
            w.rr_earned = Some(20);
            w
        },
        {
            let mut w = row("w2", d(2), R::W);
            w.rr_earned = Some(24);
            w
        },
        {
            let mut l = row("l1", d(3), R::L);
            l.rr_earned = Some(-15);
            l
        },
        {
            let mut l = row("l2", d(4), R::L);
            l.rr_earned = Some(-19);
            l
        },
        {
            let mut x = row("x1", d(5), R::D);
            x.rr_earned = Some(2);
            x
        },
        // A win without a stored RR row doesn't move the average.
        no_rr,
    ];
    let updates = vec![
        // Placement result: unranked -> ranked is not a rank change.
        update("u0", ACT, d(1), (0, 12), 10, 0),
        update("u1", ACT, d(2), (12, 13), 5, 20),
        update("u2", ACT, d(3), (13, 13), 30, 25),
        update("u3", ACT, d(4), (13, 12), 90, -40),
        update("u4", ACT, d(5), (12, 13), 10, 20),
        update("u5", ACT, d(6), (13, 14), 3, 93),
        update("u6", ACT, d(7), (14, 13), 80, -23),
        update("u7", ACT, d(8), (13, 14), 1, 21),
    ];
    let a = act();
    let n = names();
    let s = build(MyStatsInput {
        current_rank: Some(CurrentRank { tier: 14, rr: 1 }),
        ..input(&a, &matches, &updates, &n)
    });
    let r = &s.rank;
    assert_eq!((r.tier, r.rr, r.rr_to_next), (Some(14), Some(1), Some(99)));
    // The draw counts as neither.
    assert_eq!((r.wins, r.losses), (Some(3), Some(2)));
    assert_eq!(r.net_rr, Some(116));
    assert_eq!(r.avg_win, Some(22.0));
    assert_eq!(r.avg_loss, Some(-17.0));
    // Newest first, capped at 5, placements excluded.
    let changes: Vec<(u8, bool, i64)> = r
        .changes
        .iter()
        .map(|c| (c.tier, c.up, c.start_ms))
        .collect();
    assert_eq!(
        changes,
        [
            (14, true, d(8)),
            (13, false, d(7)),
            (14, true, d(6)),
            (13, true, d(5)),
            (12, false, d(4)),
        ]
    );
}

#[test]
fn rank_card_without_data_is_all_unknown() {
    let a = act();
    let n = names();
    let s = build(input(&a, &[], &[], &n));
    let r = &s.rank;
    assert_eq!((r.tier, r.rr, r.rr_to_next), (None, None, None));
    assert_eq!((r.wins, r.losses, r.net_rr), (None, None, None));
    assert_eq!((r.avg_win, r.avg_loss), (None, None));
    assert!(r.changes.is_empty());
    assert!(s.session.is_none());
}

#[test]
fn rr_series_is_absolute_and_oldest_first() {
    let d = |i: i64| ACT_START + i * DAY;
    let updates = vec![
        update("u4", ACT, d(4), (23, 24), 20, 30),
        update("u2", ACT, d(2), (12, 12), 40, 18),
        // Immortal RR is cumulative from Immortal 1's floor.
        update("u5", ACT, d(5), (25, 26), 210, 25),
        update("u1", ACT, d(1), (0, 0), 0, 0), // placement: no rank yet
        update("u3", ACT, d(3), (22, 23), 90, 21),
        update("old", PREV, ACT_START - DAY, (20, 20), 50, 10),
    ];
    let a = act();
    let n = names();
    let s = build(input(&a, &[], &updates, &n));
    let series: Vec<(i64, i32)> = s
        .rr_series
        .iter()
        .map(|p| (p.start_ms, p.absolute_rr))
        .collect();
    assert_eq!(
        series,
        [(d(2), 1240), (d(3), 2390), (d(4), 2420), (d(5), 2610)]
    );
}

#[test]
fn maps_and_agents_tables() {
    let d = |i: i64| ACT_START + i * DAY;
    let mut bind = on_map(row("m4", d(4), R::D), "map-bind");
    bind.agent_id = Some("agent-sova".into());
    bind.rounds = None;
    bind.rr_earned = None;
    let mut unknown = on_map(row("m5", d(5), R::W), "");
    unknown.agent_id = None;
    let matches = vec![
        row("m1", d(1), R::W),
        row("m2", d(2), R::W),
        row("m3", d(3), R::L),
        bind,
        unknown,
    ];
    let a = act();
    let n = names();
    let s = build(input(&a, &matches, &[], &n));

    let maps: Vec<&str> = s.maps.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(
        maps,
        ["map-ascent", "map-bind"],
        "most played first; unknown map skipped"
    );
    let ascent = &s.maps[0];
    assert_eq!((ascent.games, ascent.wins, ascent.losses), (3, 2, 1));
    assert!((ascent.winrate - 2.0 / 3.0).abs() < 1e-6);
    assert_eq!(ascent.net_rr, Some(25));
    assert_eq!(ascent.kd, 2.0);
    assert_eq!(ascent.acs, Some(200.0));
    let bind = &s.maps[1];
    assert_eq!((bind.games, bind.wins, bind.losses), (1, 0, 0));
    assert_eq!((bind.net_rr, bind.acs), (None, None), "unknown, not zero");

    let agents: Vec<(&str, u32)> = s.agents.iter().map(|r| (r.id.as_str(), r.games)).collect();
    assert_eq!(agents, [("agent-jett", 3), ("agent-sova", 1)]);
}

#[test]
fn last5_newest_first_with_match_stats() {
    let matches: Vec<CompMatchRow> = (0..7)
        .map(|i| row(&format!("m{i}"), ACT_START + i * DAY, R::W))
        .collect();
    let a = act();
    let n = names();
    let s = build(input(&a, &matches, &[], &n));
    let ids: Vec<&str> = s.last5.iter().map(|m| m.match_id.as_str()).collect();
    assert_eq!(ids, ["m6", "m5", "m4", "m3", "m2"]);
    let m = &s.last5[0];
    assert_eq!(m.result, MatchResultKind::Win);
    assert_eq!((m.rounds_won, m.rounds_lost), (Some(13), Some(7)));
    assert_eq!((m.kills, m.deaths, m.assists), (20, 10, 5));
    assert_eq!(m.kd, 2.0);
    assert_eq!(m.acs, Some(200.0));
    assert_eq!(m.hs_pct, Some(0.2));
    assert_eq!(m.rr_earned, Some(20));
    assert_eq!(m.agent_id.as_deref(), Some("agent-jett"));
    assert_eq!(m.map_id, "map-ascent");
}

#[test]
fn weekday_buckets_monday_first_in_local_time() {
    // Monday 22:00 UTC is Tuesday 01:00 at UTC+3.
    let matches = vec![
        row("mon", ACT_START + 7 * DAY + 10 * HOUR, R::W),
        row("tue", ACT_START + 7 * DAY + 22 * HOUR, R::L),
        row("sun", ACT_START + 13 * DAY + 12 * HOUR, R::W),
    ];
    let a = act();
    let n = names();
    let s = build(MyStatsInput {
        utc_offset_ms: &gulf,
        ..input(&a, &matches, &[], &n)
    });
    let games: Vec<u32> = s.by_weekday.iter().map(|c| c.games).collect();
    assert_eq!(games, [1, 1, 0, 0, 0, 0, 1]);
    assert_eq!(s.by_weekday[1].winrate, Some(0.0));
    assert_eq!(s.by_weekday[2].winrate, None, "no games: unknown");
}

#[test]
fn insights_time_weekday_economy_and_last_7_days() {
    let a = act();
    let n = names();
    let now = ACT_START + 30 * DAY; // Wednesday 00:00
    let sat = ACT_START + 26 * DAY;
    let sun = ACT_START + 27 * DAY;
    let mon = ACT_START + 28 * DAY;
    let tue = ACT_START + 29 * DAY;
    let mut matches = Vec::new();
    // Six weekend night games, five won.
    for (i, (day, r)) in [
        (sat, R::W),
        (sat, R::W),
        (sat, R::W),
        (sun, R::W),
        (sun, R::W),
        (sun, R::L),
    ]
    .into_iter()
    .enumerate()
    {
        let start = day + 22 * HOUR + (i as i64 % 3) * 40 * MINUTE;
        matches.push(row(&format!("n{i}"), start, r));
    }
    // Five weekday afternoon games, one won.
    for (i, (day, h, r)) in [
        (mon, 13, R::W),
        (mon, 14, R::L),
        (mon, 15, R::L),
        (tue, 13, R::L),
        (tue, 14, R::L),
    ]
    .into_iter()
    .enumerate()
    {
        matches.push(row(&format!("a{i}"), day + h * HOUR, r));
    }
    let mut updates: Vec<CompUpdateRow> = matches
        .iter()
        .map(|m| {
            update(
                &m.match_id,
                ACT,
                m.start_ms,
                (15, 15),
                50,
                m.rr_earned.unwrap(),
            )
        })
        .collect();
    // Older than 7 days: not part of "last 7 days".
    updates.push(update("old", ACT, now - 8 * DAY, (15, 15), 50, 99));

    let s = build(MyStatsInput {
        now_ms: now,
        ..input(&a, &matches, &updates, &n)
    });
    let got: Vec<(InsightKind, &str, u32, bool)> = s
        .insights
        .iter()
        .map(|i| (i.kind, i.text.as_str(), i.matches, i.positive))
        .collect();
    assert_eq!(
        got,
        [
            (
                InsightKind::TimeOfDay,
                "You win 83% of 6 night games, compared with 20% of 5 afternoon games.",
                11,
                true
            ),
            (
                InsightKind::WeekdayWeekend,
                "You win 83% of 6 weekend games, compared with 20% of 5 weekday games.",
                11,
                true
            ),
            (
                InsightKind::RrEconomy,
                "Average +20 RR on wins and -15 RR on losses.",
                11,
                true
            ),
            (
                InsightKind::Last7Days,
                "+45 RR over 11 games in the last 7 days.",
                11,
                true
            ),
        ]
    );
}

#[test]
fn insights_omitted_with_too_little_data() {
    let matches: Vec<CompMatchRow> = [R::W, R::L, R::W, R::L]
        .into_iter()
        .enumerate()
        .map(|(i, r)| row(&format!("m{i}"), ACT_START + 29 * DAY + i as i64 * HOUR, r))
        .collect();
    let updates: Vec<CompUpdateRow> = matches
        .iter()
        .map(|m| {
            update(
                &m.match_id,
                ACT,
                m.start_ms,
                (15, 15),
                50,
                m.rr_earned.unwrap(),
            )
        })
        .collect();
    let a = act();
    let n = names();
    let s = build(input(&a, &matches, &updates, &n));
    assert!(s.insights.is_empty(), "{:?}", s.insights);
}

#[test]
fn json_is_camel_case_with_nulls_for_unknowns() {
    let a = act();
    let n = names();
    let empty = serde_json::to_value(build(input(&a, &[], &[], &n))).unwrap();
    assert_eq!(
        empty["rank"],
        json!({
            "tier": null, "rr": null, "wins": null, "losses": null, "rrToNext": null,
            "netRr": null, "avgWin": null, "avgLoss": null, "changes": []
        })
    );
    assert_eq!(empty["session"], json!(null));
    assert_eq!(empty["rrSeries"], json!([]));
    assert_eq!(empty["last5"], json!([]));
    assert_eq!(empty["maps"], json!([]));
    assert_eq!(empty["agents"], json!([]));
    assert_eq!(empty["insights"], json!([]));
    assert_eq!(empty["byWeekday"].as_array().unwrap().len(), 7);
    let no_games = json!({"games": 0, "wins": 0, "winrate": null});
    assert_eq!(empty["byDaypart"], json!(vec![no_games; 4]));

    let matches = vec![row("m1", ACT_START + DAY, R::W)];
    let updates = vec![update("m1", ACT, ACT_START + DAY, (12, 13), 5, 20)];
    let full = serde_json::to_value(build(input(&a, &matches, &updates, &n))).unwrap();
    assert_eq!(full["session"]["streak"], json!(["W"]));
    assert_eq!(full["session"]["startedMs"], json!(ACT_START + DAY));
    assert_eq!(
        full["rank"]["changes"],
        json!([{"tier": 13, "up": true, "startMs": ACT_START + DAY}])
    );
    assert_eq!(
        full["rrSeries"],
        json!([{"startMs": ACT_START + DAY, "absoluteRr": 1305}])
    );
    let m = &full["last5"][0];
    for key in [
        "matchId",
        "mapId",
        "startMs",
        "result",
        "roundsWon",
        "roundsLost",
        "agentId",
        "kills",
        "deaths",
        "assists",
        "kd",
        "acs",
        "hsPct",
        "rrEarned",
    ] {
        assert!(m.get(key).is_some(), "last5 entry lacks {key}: {m}");
    }
    let map = &full["maps"][0];
    for key in [
        "id", "games", "wins", "losses", "winrate", "netRr", "kd", "acs",
    ] {
        assert!(map.get(key).is_some(), "map row lacks {key}: {map}");
    }
}
