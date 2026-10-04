//! B7: the hot queries at the size a heavy user reaches.
//!
//! Builds the synthetic database of `common/scale.rs` (8 000 matches, 5 000 of
//! them the user's, 80 000 `match_players` rows, 2 000 own competitive updates,
//! ...: see that file) through the real `open_db` and `Repo` write paths, then
//! times every hot read, best of three runs (seven for the sub-millisecond
//! ones). A step over the loose tripwire (1 s, any build) fails the test; the
//! table it prints marks every step over the 50 ms goal. The table goes to
//! stderr: `cargo test -p opd1-core --test db_scale -- --nocapture` shows it (a
//! failing test shows it too).
//!
//! One test, not one per query: the database is built once and removed with
//! the test, and the queries run one after the other, so the timings do not
//! disturb one another.
//!
//! Whether each query is index-backed is `tests/db.rs`' job (`EXPLAIN QUERY
//! PLAN` on the schema alone); this file measures what that costs, and
//! re-checks every one of those plans on the analyzed scale database (the
//! statistics a full `ANALYZE` writes, like the ones the app's `PRAGMA optimize` will write, must not change a plan).

#[path = "common/scale.rs"]
mod scale;

use std::time::Instant;

use opd1_core::db::repo::{
    self, HistoryEntry, Match, MatchPlayer, MatchTeam, NameAsk, COMP_MATCHES_SQL,
};
use scale::{pu, stranger, Timings, DUO, USER};

#[test]
fn hot_queries_stay_fast_on_a_heavy_users_database() {
    let built = Instant::now();
    let db = scale::build();
    let c = &db.counts;
    eprintln!(
        "scale db built in {:.1} s: {} matches ({} competitive), {} match_players, {} match_teams, \
         {} comp_updates ({} the user's), {} history_index, {} players, {} mmr snapshots, \
         {} sqlite_stat1 rows, user_version {}, {:.1} MB",
        built.elapsed().as_secs_f64(),
        c.matches,
        c.comp_matches,
        c.match_players,
        c.match_teams,
        c.comp_updates,
        c.user_comp_updates,
        c.history_index,
        c.players,
        c.mmr_snapshots,
        c.stat1_rows,
        c.user_version,
        c.file_bytes as f64 / 1e6,
    );
    // The test measures what it says it measures.
    assert_eq!(c.matches, 8_000, "5 000 with the user, 3 000 without");
    assert_eq!(c.match_players, 80_000);
    assert_eq!(c.comp_matches, 4_400);
    assert_eq!(c.user_comp_updates, 2_000);
    assert!(c.comp_updates > 2_000 && c.history_index > 10_000);
    assert!(c.players >= 1_800 && c.mmr_snapshots >= 300 && c.stat1_rows > 0);

    let repo = db.open();
    let act = &db.act;
    let stranger = stranger();
    let roster = scale::roster();
    let mut t = Timings::default();

    // --- the plans, with statistics -------------------------------------
    // Every plan `tests/db.rs` pins on the schema alone is still the plan
    // once `ANALYZE` has run, and none scans a base table.
    let changed = scale::plans_with_statistics(
        repo.conn(),
        &[
            ("comp_matches", COMP_MATCHES_SQL),
            ("last_n_comp", repo::LAST_N_COMP_SQL),
            ("top_teammates", repo::TOP_TEAMMATES_SQL),
            ("comp_updates_for_season", repo::COMP_UPDATES_FOR_SEASON_SQL),
            ("encounters", repo::ENCOUNTERS_SQL),
            ("encounter_counts", repo::ENCOUNTER_COUNTS_SQL),
            ("known_name", repo::KNOWN_NAME_SQL),
            (
                "newest_ranked_match_tier",
                repo::NEWEST_RANKED_MATCH_TIER_SQL,
            ),
            ("names_to_ask", repo::NAMES_TO_ASK_SQL),
        ],
    );
    assert!(
        changed.is_empty(),
        "statistics changed a plan: {changed:#?}"
    );

    // --- My Stats / Player Details: the competitive match reads ---------
    // My Stats' form: the whole current act (a day of slack each side).
    let act_rows = t.time("comp_matches: user, current act (My Stats)", || {
        repo.comp_matches(
            USER,
            act.start_ms - 86_400_000..act.end_ms + 86_400_000,
            u32::MAX,
        )
        .unwrap()
    });
    assert!(
        (230..=260).contains(&act_rows.len()),
        "the user's current act: {} competitive matches",
        act_rows.len()
    );
    assert!(act_rows.iter().all(|r| r.rounds_won.is_some()));
    // Player Details' recent matches (8, any act), the user and the duo.
    for (name, who) in [("user", USER), ("duo", DUO)] {
        let rows = t.time(&format!("comp_matches: {name}, newest 8 (Details)"), || {
            repo.comp_matches(who, i64::MIN..i64::MAX, 8).unwrap()
        });
        assert_eq!(rows.len(), 8);
    }
    // The hero's K/D and HS% (20 newest of the current act).
    let rows = t.time("comp_matches: duo, current act, 20 (hero stats)", || {
        repo.comp_matches(DUO, act.start_ms..i64::MAX, 20).unwrap()
    });
    assert_eq!(rows.len(), 20);

    // --- the card's last 5 ---------------------------------------------
    for (name, who) in [("user", USER.to_string()), ("duo", DUO.to_string())] {
        let rows = t.time(&format!("last_n_comp(5): {name}"), || {
            repo.last_n_comp(&who, 5).unwrap()
        });
        assert_eq!(rows.len(), 5);
    }
    t.time("last_n_comp(5): stranger", || {
        repo.last_n_comp(&stranger, 5).unwrap()
    });

    // --- comp_updates ---------------------------------------------------
    let season = t.time("comp_updates_for_season: user, current act", || {
        repo.comp_updates_for_season(USER, &act.id).unwrap()
    });
    assert!((230..=260).contains(&season.len()), "{}", season.len());

    // --- Top 3 Teammates ------------------------------------------------
    for (name, who) in [
        ("user", USER.to_string()),
        ("duo", DUO.to_string()),
        ("stranger", stranger.clone()),
    ] {
        let top = t.time(&format!("top_teammates(3): {name}"), || {
            repo.top_teammates(&who, act.start_ms, 3).unwrap()
        });
        if who == USER {
            assert_eq!(top.len(), 3);
            assert_eq!(top[0].puuid, DUO, "the duo partner is the top teammate");
            assert!(top[0].count > 80, "{}", top[0].count);
        }
    }

    // --- encounters -----------------------------------------------------
    // B7: the other player's rows drive both queries (`tests/db.rs` pins the
    // plan), so what a card pays follows the OTHER player's stored matches, not
    // the thousands the user played. Driven from the user's rows a stranger cost as much
    // as the duo (~8 ms each here); now a stranger is ~100x cheaper.
    const ENC_DUO: &str = "encounters: user vs duo (Details)";
    const ENC_STRANGER: &str = "encounters: user vs stranger (Details)";
    const COUNT_DUO: &str = "encounter_counts: user vs duo";
    const COUNT_STRANGER: &str = "encounter_counts: user vs stranger";
    let shared = t.time(ENC_DUO, || repo.encounters(USER, DUO).unwrap());
    assert!(shared.with > 600 && shared.against > 100, "{shared:?}");
    t.time(ENC_STRANGER, || repo.encounters(USER, &stranger).unwrap());
    t.time(COUNT_DUO, || repo.encounter_counts(USER, DUO).unwrap());
    t.time(COUNT_STRANGER, || {
        repo.encounter_counts(USER, &stranger).unwrap()
    });
    let counts = t.time("encounter_counts: the 9 others of a roster", || {
        roster[1..]
            .iter()
            .map(|p| repo.encounter_counts(USER, p).unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(counts[0], (shared.with, shared.against), "duo first");
    for (stranger_step, duo_step) in [(ENC_STRANGER, ENC_DUO), (COUNT_STRANGER, COUNT_DUO)] {
        let (s, d) = (t.ms_of(stranger_step), t.ms_of(duo_step));
        // Far under the goal in absolute terms, and (on any machine) a fraction
        // of the duo's, whose 2 000+ shared rows are the real work.
        assert!(
            s < 10.0 && s * 5.0 < d,
            "a stranger's encounter read follows the user's history again: \
             {stranger_step} {s:.2} ms vs {duo_step} {d:.2} ms"
        );
    }

    // --- names ------------------------------------------------------------
    let known = t.time("known_name: the 9 others of a roster", || {
        roster[1..]
            .iter()
            .map(|p| repo.known_name(USER, p).unwrap())
            .collect::<Vec<_>>()
    });
    assert!(known.iter().all(Option::is_some));
    t.time("known_name: user vs duo (2 000+ stored rows)", || {
        repo.known_name(USER, DUO).unwrap()
    });
    let asks: Vec<NameAsk> = (1_950..1_960)
        .map(|i| NameAsk {
            puuid: pu(i),
            match_start_ms: Some(scale::NOW_MS),
        })
        .chain([db.blank_ask.clone()])
        .collect();
    t.time("names_to_ask: 11 asks", || {
        repo.names_to_ask(&asks).unwrap()
    });
    assert!(
        repo.names_to_ask(std::slice::from_ref(&db.blank_ask))
            .unwrap()
            .is_empty(),
        "a remembered blank answer is not asked again"
    );

    // --- rank fallback, snapshots, history diff -----------------------
    for (name, who) in [("user", USER.to_string()), ("duo", DUO.to_string())] {
        let tier = t.time(&format!("newest_ranked_match_tier: {name}"), || {
            repo.newest_ranked_match_tier(&who).unwrap()
        });
        assert!(tier.is_some());
    }
    t.time("newest_ranked_match_tier: stranger", || {
        repo.newest_ranked_match_tier(&stranger).unwrap()
    });
    t.time("load_mmr_snapshot + needs_mmr_refresh: user", || {
        let snap = repo.load_mmr_snapshot(USER).unwrap().unwrap();
        repo.needs_mmr_refresh(USER, snap.last_comp_match_id.as_deref().unwrap())
            .unwrap()
    });
    // "Sync, don't refetch": a history list of 500 ids, half stored.
    let ids: Vec<String> = (0..250)
        .map(|i| scale::match_id(i * 20))
        .chain((0..250).map(|i| format!("new-{i}")))
        .collect();
    let fresh = t.time("new_match_ids: 500 ids, 250 new", || {
        repo.new_match_ids(&ids).unwrap()
    });
    assert_eq!(fresh.len(), 250);
    let stored = t.time("match_rows: one stored match (Match Details)", || {
        repo.match_rows(&scale::match_id(2_500)).unwrap().unwrap()
    });
    assert_eq!(stored.players.len(), 10);

    // --- the write after a match: details + RR + history ---------------
    let new_id = "00000000-9999-4000-8000-000000000000".to_string();
    let start_ms = scale::NOW_MS + 60_000;
    let players: Vec<MatchPlayer> = std::iter::once(USER.to_string())
        .chain((0..9).map(pu))
        .enumerate()
        .map(|(n, puuid)| MatchPlayer {
            puuid,
            team_id: if n < 5 { "Blue" } else { "Red" }.into(),
            party_id: None,
            agent_id: Some("agent-01".into()),
            kills: 20,
            deaths: 10,
            assists: 5,
            score: 4_000,
            rounds: Some(24),
            hs: 8,
            bs: 20,
            ls: 3,
            game_name: Some("Name".into()),
            tag_line: Some("TAG".into()),
            competitive_tier: Some(15),
            damage: Some(3_000),
        })
        .collect();
    let m = Match {
        match_id: new_id.clone(),
        map_id: "/Game/Maps/Ascent/Ascent".into(),
        queue_id: "competitive".into(),
        start_ms,
        winning_team: Some("Blue".into()),
        is_ranked: true,
        rounds_played: Some(24),
        game_length_ms: Some(2_000_000),
    };
    let teams = [("Blue", true, 13), ("Red", false, 11)].map(|(id, won, r)| MatchTeam {
        team_id: id.into(),
        won,
        rounds_won: Some(r),
        rounds_won_h1: Some(6),
    });
    t.time("write: upsert_match (10 players) + index_history", || {
        repo.upsert_match(&m, &teams, &players).unwrap();
        repo.index_history(
            USER,
            &[HistoryEntry {
                match_id: new_id.clone(),
                queue_id: "competitive".into(),
                start_ms,
            }],
        )
        .unwrap();
    });

    t.finish("hot queries on the scale db");
}
