//! Tests for `opd1_core::mystats` — the My Stats aggregation (Task 9a).
//! Everything is pure: rows in, `MyStats` out, no DB.

use std::collections::HashMap;

use serde_json::json;

use opd1_core::db::repo::{CompMatchRow, CompUpdateRow, StoredTier};
use opd1_core::model::{CurrentRank, InsightKind, MatchResultKind, StreakMark};
use opd1_core::mystats::{build, rr_to_next, MyStatsInput};
use opd1_core::stats::Act;

const MINUTE: i64 = 60_000;
const HOUR: i64 = 60 * MINUTE;
const DAY: i64 = 24 * HOUR;
/// Monday 2025-08-18 00:00 UTC.
const ACT_START: i64 = 20_318 * DAY;
const ACT: &str = "act-cur";
/// The user's PUUID in these tests.
const PUUID: &str = "11111111-2222-4333-8444-555555555555";
const PREV: &str = "act-prev";
const NEXT: &str = "act-next";

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

/// The Riot season of an RR row dated `start_ms` when Riot's rollout
/// matches valorant-api's act dates exactly.
fn season_at(start_ms: i64) -> &'static str {
    if start_ms < ACT_START {
        PREV
    } else if start_ms < ACT_START + 60 * DAY {
        ACT
    } else {
        NEXT
    }
}

/// A stored competitive match of the user: 20/10/5, 4000 score over 20
/// rounds (ACS 200), 10/30/10 shots (HS 20%); +20 RR on a win, -15 on a
/// loss, 0 on a draw, from an RR row in Plat 1 (tier 15) of the season
/// its date falls in.
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
        season_id: Some(season_at(start_ms).into()),
        tier_before: Some(15),
        tier_after: Some(15),
        tier: None,
    }
}

/// `r` with its RR row replaced by one from `season`, `tiers` and `rr`.
fn with_rr(mut r: CompMatchRow, season: &str, tiers: (i64, i64), rr: i64) -> CompMatchRow {
    r.season_id = Some(season.into());
    (r.tier_before, r.tier_after) = (Some(tiers.0), Some(tiers.1));
    r.rr_earned = Some(rr);
    r
}

/// `r` without a stored RR row.
fn no_rr(mut r: CompMatchRow) -> CompMatchRow {
    r.rr_earned = None;
    r.season_id = None;
    (r.tier_before, r.tier_after) = (None, None);
    r
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
        puuid: PUUID,
        act,
        matches,
        comp_updates,
        current_rank: None,
        // A stored snapshot answered (with no rank): no fallback unless a
        // test turns it off.
        has_mmr: true,
        stored_tier: None,
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
    let s = build(MyStatsInput {
        now_ms: ACT_START + 2 * DAY + HOUR,
        ..input(&a, &matches, &updates, &n)
    });

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

// ---------------------------------------------------------------------
// R167: the rank card without an MMR snapshot — the live card's chain
// ---------------------------------------------------------------------

fn no_mmr<'a>(
    a: &'a Act,
    updates: &'a [CompUpdateRow],
    n: &'a HashMap<String, String>,
    stored_tier: Option<StoredTier>,
) -> MyStatsInput<'a> {
    MyStatsInput {
        has_mmr: false,
        stored_tier,
        ..input(a, &[], updates, n)
    }
}

#[test]
fn rank_card_without_a_snapshot_uses_the_newest_current_act_rr_row() {
    let a = act();
    let n = names();
    let updates = vec![
        // Newest of the previous act: says nothing about the rank now.
        update("p1", PREV, ACT_START - HOUR, (20, 20), 90, 10),
        update("c1", ACT, ACT_START + DAY, (15, 16), 5, 20),
        update("c2", ACT, ACT_START + 3 * DAY, (16, 16), 41, 36),
        // Placement row of the act, unranked after: not a tier.
        update("c0", ACT, ACT_START + 2 * DAY, (0, 2), 0, 0),
    ];
    // The stored match tier would say something else: RR rows come first.
    let stored = StoredTier {
        start_ms: ACT_START + 5 * DAY,
        tier: 19,
    };
    let s = build(no_mmr(&a, &updates, &n, Some(stored)));
    assert_eq!((s.rank.tier, s.rank.rr), (Some(16), Some(41)));
    assert_eq!(s.rank.rr_to_next, Some(59));
}

#[test]
fn rank_card_without_a_snapshot_or_rr_row_uses_the_current_act_match_tier_without_rr() {
    let a = act();
    let n = names();
    let stored = StoredTier {
        start_ms: ACT_START + DAY,
        tier: 17,
    };
    let s = build(no_mmr(&a, &[], &n, Some(stored)));
    assert_eq!((s.rank.tier, s.rank.rr), (Some(17), None));
    // No exact RR: nothing to count down from.
    assert_eq!(s.rank.rr_to_next, None);
    let json = serde_json::to_value(&s.rank).unwrap();
    assert_eq!(json["rr"], serde_json::Value::Null);
    assert_eq!(json["tier"], 17);
}

#[test]
fn rank_card_fallback_ignores_a_previous_acts_rows_and_matches() {
    let a = act();
    let n = names();
    let updates = vec![update("p1", PREV, ACT_START - HOUR, (20, 20), 90, 10)];
    let old = StoredTier {
        start_ms: ACT_START - HOUR,
        tier: 20,
    };
    let s = build(no_mmr(&a, &updates, &n, Some(old)));
    assert_eq!((s.rank.tier, s.rank.rr), (None, None));
    let s = build(no_mmr(&a, &[], &n, None));
    assert_eq!((s.rank.tier, s.rank.rr), (None, None));
}

#[test]
fn rank_card_fallback_never_overrides_a_snapshot() {
    // A snapshot that answered is the authority, even with "no rank this
    // act" (placements): the RR rows do not fill it in.
    let a = act();
    let n = names();
    let updates = vec![update("c1", ACT, ACT_START + DAY, (15, 16), 5, 20)];
    let s = build(MyStatsInput {
        has_mmr: true,
        stored_tier: Some(StoredTier {
            start_ms: ACT_START + DAY,
            tier: 17,
        }),
        ..input(&a, &[], &updates, &n)
    });
    assert_eq!((s.rank.tier, s.rank.rr), (None, None));
    // And a snapshot's own rank is kept as it is.
    let s = build(MyStatsInput {
        has_mmr: true,
        current_rank: Some(CurrentRank { tier: 23, rr: 60 }),
        ..input(&a, &[], &updates, &n)
    });
    assert_eq!((s.rank.tier, s.rank.rr), (Some(23), Some(60)));
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
    let s = build(MyStatsInput {
        now_ms: c + gap + 3 * HOUR,
        ..input(&a, &matches, &[], &n)
    });
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
    let no_rr = no_rr(row("w3", d(6), R::W));
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
    let bind = no_rr(bind);
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
    assert_eq!(
        (m.kills, m.deaths, m.assists),
        (Some(20), Some(10), Some(5))
    );
    assert_eq!(m.kd, Some(2.0));
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
    let full = serde_json::to_value(build(MyStatsInput {
        now_ms: ACT_START + DAY + HOUR,
        ..input(&a, &matches, &updates, &n)
    }))
    .unwrap();
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

// ---------------------------------------------------------------------
// 9a math review fixes
// ---------------------------------------------------------------------

fn rr_economy(s: &opd1_core::model::MyStats) -> Option<(&str, u32, bool)> {
    s.insights
        .iter()
        .find(|i| i.kind == InsightKind::RrEconomy)
        .map(|i| (i.text.as_str(), i.matches, i.positive))
}

/// M1: placement games (unranked tiers, 0 RR) stay out of the average
/// win/loss RR and the RR-economy insight; a ranked draw is neither.
#[test]
fn placement_games_stay_out_of_rr_averages() {
    let a = act();
    let n = names();
    let d = |i: i64| ACT_START + i * DAY;
    let placement = |id: &str, i: i64, r: R, tiers| with_rr(row(id, d(i), r), ACT, tiers, 0);
    let ranked = |id: &str, i: i64, r: R, rr| with_rr(row(id, d(i), r), ACT, (12, 12), rr);
    // The review's counterexample.
    let matches = vec![
        placement("p1", 1, R::W, (0, 0)),
        placement("p2", 2, R::W, (0, 0)),
        // The last placement game: ranked only afterwards.
        placement("p3", 3, R::L, (0, 12)),
        ranked("w1", 4, R::W, 20),
        ranked("w2", 5, R::W, 24),
        ranked("l1", 6, R::L, -16),
        ranked("l2", 7, R::L, -18),
        ranked("x1", 8, R::D, 0),
    ];
    let s = build(input(&a, &matches, &[], &n));
    // Placement games are still real wins and losses.
    assert_eq!((s.rank.wins, s.rank.losses), (Some(4), Some(3)));
    assert_eq!(s.rank.avg_win, Some(22.0));
    assert_eq!(s.rank.avg_loss, Some(-17.0));
    // Two ranked games a side is too few for the insight.
    assert_eq!(rr_economy(&s), None);

    // With enough ranked games the insight shows their averages only.
    let mut matches: Vec<CompMatchRow> = matches[..3].to_vec();
    for (i, rr) in [20, 24, 22, 21, 23].into_iter().enumerate() {
        matches.push(ranked(&format!("w{i}"), 4 + i as i64, R::W, rr));
    }
    for (i, rr) in [-16, -18, -17, -15, -19].into_iter().enumerate() {
        matches.push(ranked(&format!("l{i}"), 9 + i as i64, R::L, rr));
    }
    let s = build(input(&a, &matches, &[], &n));
    assert_eq!((s.rank.avg_win, s.rank.avg_loss), (Some(22.0), Some(-17.0)));
    assert_eq!(
        rr_economy(&s),
        Some(("Average +22 RR on wins and -17 RR on losses.", 10, true))
    );
}

/// M2: a match belongs to the act its RR row's Riot season says, exactly
/// like the RR rows themselves; only a match without an RR row falls
/// back to valorant-api's act dates.
#[test]
fn act_membership_follows_riots_season_for_matches_and_rr_rows() {
    let a = act();
    let n = names();
    // The review's counterexample: Riot rolled the act out 3h+ after
    // valorant-api's midnight start.
    let early = ACT_START + 3 * HOUR;
    let later = ACT_START + 2 * DAY;
    let matches = vec![
        with_rr(row("early", early, R::W), PREV, (15, 15), 28),
        with_rr(row("later", later, R::W), ACT, (15, 15), 18),
    ];
    let updates = vec![
        update("early", PREV, early, (15, 15), 78, 28),
        update("later", ACT, later, (15, 15), 68, 18),
    ];
    let s = build(MyStatsInput {
        now_ms: later + HOUR,
        ..input(&a, &matches, &updates, &n)
    });
    assert_eq!((s.rank.wins, s.rank.losses), (Some(1), Some(0)));
    assert_eq!(s.rank.net_rr, Some(18));
    assert_eq!(s.rank.avg_win, Some(18.0));
    let last5: Vec<&str> = s.last5.iter().map(|m| m.match_id.as_str()).collect();
    assert_eq!(last5, ["later"]);
    assert_eq!(s.rr_series.len(), 1);
    assert_eq!(s.maps.iter().map(|r| r.games).sum::<u32>(), 1);
    assert_eq!(s.agents.iter().map(|r| r.games).sum::<u32>(), 1);
    assert_eq!(s.by_weekday.iter().map(|c| c.games).sum::<u32>(), 1);
    assert_eq!(s.by_daypart.iter().map(|c| c.games).sum::<u32>(), 1);
    assert_eq!(s.session.map(|x| x.wins), Some(1));

    // The reverse (Riot rolled out before midnight), and the date
    // fallback for matches without an RR row.
    let eve = ACT_START - 2 * HOUR;
    let matches = vec![
        with_rr(row("eve", eve, R::W), ACT, (15, 15), 10),
        no_rr(row("no-rr-before", ACT_START - HOUR, R::W)),
        no_rr(row("no-rr-inside", ACT_START + HOUR, R::L)),
    ];
    let updates = vec![update("eve", ACT, eve, (15, 15), 60, 10)];
    let s = build(input(&a, &matches, &updates, &n));
    let last5: Vec<&str> = s.last5.iter().map(|m| m.match_id.as_str()).collect();
    assert_eq!(last5, ["no-rr-inside", "eve"]);
    assert_eq!((s.rank.wins, s.rank.losses), (Some(1), Some(1)));
    assert_eq!(s.rank.net_rr, Some(10));
    assert_eq!(s.rr_series.len(), 1);
}

/// m2: once the newest comp match started more than 6h before `now`, its
/// session is over — no current session (`None`, shown as `—`).
#[test]
fn no_current_session_6h_after_the_last_match() {
    let a = act();
    let n = names();
    let last = ACT_START + 20 * DAY;
    // The review's counterexample: the last two games were 5 days ago.
    let matches = vec![row("a", last - HOUR, R::L), row("b", last, R::L)];
    let session_at = |now_ms| {
        build(MyStatsInput {
            now_ms,
            ..input(&a, &matches, &[], &n)
        })
        .session
    };
    assert_eq!(session_at(last + 5 * DAY), None);
    // The same rule as between matches: exactly 6h still continues it.
    let current = session_at(last + 6 * HOUR).expect("still the current session");
    assert_eq!(current.streak, [StreakMark::Loss, StreakMark::Loss]);
    assert_eq!(current.net_rr, Some(-30));
    assert_eq!(session_at(last + 6 * HOUR + 1), None);
}

/// m4: "last 7 days" is a time window — it counts the previous act's
/// games inside it too; everything else stays on the current act.
#[test]
fn last_7_days_crosses_the_act_start() {
    let a = act();
    let n = names();
    // The review's counterexample: two days into the act, 5 of its games
    // at +10, and 6 of the previous act's 3–6 days ago at -20.
    let now = ACT_START + 2 * DAY;
    let mut updates: Vec<CompUpdateRow> = (1..=5)
        .map(|i| {
            update(
                &format!("c{i}"),
                ACT,
                ACT_START + i * HOUR,
                (15, 15),
                50,
                10,
            )
        })
        .collect();
    for (i, days_ago) in [3, 3, 4, 5, 5, 6].into_iter().enumerate() {
        let start = now - days_ago * DAY - i as i64 * HOUR;
        updates.push(update(&format!("p{i}"), PREV, start, (15, 15), 50, -20));
    }
    // Older than 7 days: outside the window.
    updates.push(update("old", PREV, now - 8 * DAY, (15, 15), 50, -20));

    let s = build(MyStatsInput {
        now_ms: now,
        ..input(&a, &[], &updates, &n)
    });
    let last7: Vec<(&str, u32, bool)> = s
        .insights
        .iter()
        .filter(|i| i.kind == InsightKind::Last7Days)
        .map(|i| (i.text.as_str(), i.matches, i.positive))
        .collect();
    assert_eq!(
        last7,
        [("-70 RR over 11 games in the last 7 days.", 11, false)]
    );
    // The act's own numbers still leave the previous act out.
    assert_eq!(s.rank.net_rr, Some(50));
    assert_eq!(s.rr_series.len(), 5);
    assert!(s.rank.changes.is_empty());
}

/// m7 + R102: a match whose details had no stats for the user (stored
/// 0/0/0, no rounds) has an unknown K/D/A, K/D, ACS and HS% — not counted
/// zeros. The UI shows `—` for each.
#[test]
fn match_without_player_stats_has_unknown_kd() {
    let a = act();
    let n = names();
    let mut blank = row("blank", ACT_START + DAY, R::W);
    (blank.kills, blank.deaths, blank.assists, blank.score) = (0, 0, 0, 0);
    (blank.hs, blank.bs, blank.ls) = (0, 0, 0);
    blank.rounds = None;
    // Even stray shots recorded for a row without stats are no HS%.
    let mut stray = row("stray", ACT_START + 3 * DAY, R::W);
    (stray.kills, stray.deaths, stray.assists, stray.score) = (0, 0, 0, 0);
    stray.rounds = None;
    let matches = vec![blank, row("full", ACT_START + 2 * DAY, R::W), stray];
    let v = serde_json::to_value(build(input(&a, &matches, &[], &n))).unwrap();
    let null = json!(null);
    let stat_line: Vec<(&str, Vec<&serde_json::Value>)> = v["last5"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| {
            let cells = ["kills", "deaths", "assists", "kd", "acs"]
                .into_iter()
                .map(|key| &m[key])
                .collect();
            (m["matchId"].as_str().unwrap(), cells)
        })
        .collect();
    assert_eq!(
        stat_line,
        [
            ("stray", vec![&null; 5]),
            (
                "full",
                vec![
                    &json!(20),
                    &json!(10),
                    &json!(5),
                    &json!(2.0),
                    &json!(200.0)
                ]
            ),
            ("blank", vec![&null; 5]),
        ]
    );
    let hs: Vec<&serde_json::Value> = v["last5"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| &m["hsPct"])
        .collect();
    assert_eq!(hs[0], &null, "no stats row: no HS%, even with stray shots");
    assert!((hs[1].as_f64().unwrap() - 0.2).abs() < 1e-6);
    assert_eq!(hs[2], &null);
}

/// m8: the RR-economy insight needs 5 ranked games on each side, and its
/// good/bad flag follows the rounded numbers its text shows.
#[test]
fn rr_economy_needs_5_games_a_side_and_flags_what_it_shows() {
    let a = act();
    let n = names();
    let games = |wins: &[i64], losses: &[i64]| -> Vec<CompMatchRow> {
        let results = wins.iter().map(|&rr| (R::W, rr));
        let results = results.chain(losses.iter().map(|&rr| (R::L, rr)));
        results
            .enumerate()
            .map(|(i, (r, rr))| {
                let start = ACT_START + (1 + i as i64) * DAY;
                with_rr(row(&format!("m{i}"), start, r), ACT, (12, 12), rr)
            })
            .collect()
    };
    let economy = |matches: &[CompMatchRow]| {
        let s = build(input(&a, matches, &[], &n));
        rr_economy(&s).map(|(text, games, positive)| (text.to_string(), games, positive))
    };
    // The review's case: 5 games, but the loss average is one game.
    assert_eq!(economy(&games(&[20; 4], &[-15])), None);
    assert_eq!(economy(&games(&[20; 5], &[-15; 4])), None);
    assert_eq!(
        economy(&games(&[20; 5], &[-15; 5])),
        Some((
            "Average +20 RR on wins and -15 RR on losses.".into(),
            10,
            true
        ))
    );
    // +19.6 and -19.7 show as +20 / -20: even, so not a weak spot.
    let losses = [-20, -20, -20, -20, -20, -20, -20, -19, -19, -19];
    assert_eq!(
        economy(&games(&[20, 20, 20, 20, 18], &losses)),
        Some((
            "Average +20 RR on wins and -20 RR on losses.".into(),
            15,
            true
        ))
    );
    // +18 / -20 is.
    assert_eq!(
        economy(&games(&[18; 5], &[-20; 5])),
        Some((
            "Average +18 RR on wins and -20 RR on losses.".into(),
            10,
            false
        ))
    );
}
