//! B7: the shell's DB reads at the size a heavy user reaches.
//!
//! `opd1-core`'s `tests/db_scale.rs` times the repository's own queries on a
//! synthetic 8 000-match database; this times what the shell composes from them
//! on the very same database (`common/scale.rs`, included below, built through
//! the real `open_db` and `Repo` write paths): a card's `load_player_db`, My
//! Stats, and Player Details' `details::build` on a real `AppState`. The queries
//! whose SQL lives in `store.rs` rather than in `Repo` (public constants there)
//! get their `EXPLAIN QUERY PLAN` tests here.
//!
//! A step slower than the loose 1 s tripwire fails the test; the table it
//! prints (stderr: `cargo test -p opd1-trcker -- --nocapture` shows it, and so
//! does a failure) marks every step over the 50 ms goal. Not a harness test: no
//! mock Riot client, no sync worker.

#[path = "../../../crates/core/tests/common/scale.rs"]
mod scale;

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use opd1_core::stats::Act;
use opd1_riot::assets::AssetCache;

use super::{Recorder, Sink};
use crate::state::{now_ms, AppState};
use crate::store::{
    act_hero_stats, load_my_stats, load_player_db, recent_comp_updates, recent_matches,
    recent_refs, RECENT_COMP_UPDATES_SQL, RECENT_REFS_SQL, SEASON_OR_RECENT_COMP_UPDATES_SQL,
};
use crate::sync::RECENT_COUNT;
use scale::{Timings, DUO, USER};

const DAY_MS: i64 = 86_400_000;

/// The scale database's current act as the shell's type.
fn act_of(db: &scale::Scale) -> Act {
    Act {
        id: db.act.id.clone(),
        name: "ACT".into(),
        episode_name: "E".into(),
        start_ms: db.act.start_ms,
        end_ms: db.act.end_ms,
    }
}

/// An `AppState` over the scale database (a second connection to the file), as
/// the app has it: only its own current act is set, so the act contains the
/// real clock `details::build` reads.
fn app_state(db: &scale::Scale, dir: &Path) -> Arc<AppState> {
    let st = Arc::new(AppState::new(
        Box::new(Sink(Arc::new(Recorder::default()))),
        db.open(),
        // Never asked: no table download starts from a read.
        AssetCache::with_base(dir.join("assets"), "http://127.0.0.1:9".into()),
        dir.join("debug"),
        Some(USER.into()),
    ));
    st.set_acts_for_test(vec![Act {
        end_ms: now_ms() + 30 * DAY_MS,
        ..act_of(db)
    }]);
    st
}

#[tokio::test]
async fn the_shells_reads_stay_fast_on_a_heavy_users_database() {
    let db = scale::build();
    assert_eq!(db.counts.matches, 8_000);
    assert_eq!(db.counts.match_players, 80_000);
    let repo = db.open();
    let act = act_of(&db);
    let stranger = scale::stranger();
    let mut t = Timings::default();

    // --- the plans, with statistics -------------------------------------
    // The plans pinned below on the schema alone are still the plans once
    // `ANALYZE` has run (a full `ANALYZE`, standing in for the statistics the app's `PRAGMA optimize` will write).
    let changed = scale::plans_with_statistics(
        repo.conn(),
        &[
            ("recent_comp_updates", RECENT_COMP_UPDATES_SQL),
            (
                "season_or_recent_comp_updates",
                SEASON_OR_RECENT_COMP_UPDATES_SQL,
            ),
            ("recent_refs", RECENT_REFS_SQL),
        ],
    );
    assert!(
        changed.is_empty(),
        "statistics changed a plan: {changed:#?}"
    );

    // --- a card: everything `load_player_db` reads ---------------------
    let me = t.time("load_player_db: user", || {
        load_player_db(&repo, Some(USER), USER).unwrap()
    });
    assert!(me.mmr.is_some(), "the stored snapshot parses");
    assert_eq!(me.last5.len(), 5);
    assert_eq!(me.comp.len(), 10);
    assert!(me.row.is_some() && me.encounters.is_none() && me.remembered.is_none());
    assert!(me.recent.iter().all(Option::is_some));

    let duo = t.time("load_player_db: duo (2 000+ stored rows)", || {
        load_player_db(&repo, Some(USER), DUO).unwrap()
    });
    assert!(duo.mmr.is_some() && duo.remembered.is_some());
    let shared = duo.encounters.expect("the duo and the user shared matches");
    assert!(shared.with > 600 && shared.against > 100);

    let no_mmr = t.time("load_player_db: stranger without MMR", || {
        load_player_db(&repo, Some(USER), &stranger).unwrap()
    });
    assert!(no_mmr.mmr.is_none(), "this stranger has no snapshot");
    t.time("load_player_db: stranger with MMR", || {
        load_player_db(&repo, Some(USER), &scale::pu(5)).unwrap()
    });
    t.time("load_player_db: a whole roster (10 players)", || {
        scale::roster()
            .iter()
            .map(|p| load_player_db(&repo, Some(USER), p).unwrap())
            .collect::<Vec<_>>()
    });

    // --- the reads `store.rs` runs itself ---------------------------------
    t.time("recent_refs: user", || {
        recent_refs(repo.conn(), USER).unwrap()
    });
    t.time("recent_refs: duo", || {
        recent_refs(repo.conn(), DUO).unwrap()
    });
    let comp = t.time("recent_comp_updates(10): user", || {
        recent_comp_updates(repo.conn(), USER, 10).unwrap()
    });
    assert_eq!(comp.len(), 10);
    t.time("recent_matches(8): user", || {
        recent_matches(&repo, USER, RECENT_COUNT, &|p: &str| Some(p.to_string())).unwrap()
    });
    let hero = t.time("act_hero_stats: user", || {
        act_hero_stats(&repo, USER, act.start_ms).unwrap()
    });
    assert_eq!(hero.matches, 20, "the 20 newest of the act");

    // --- My Stats: the whole current act, from the DB only ------------
    let map_names: HashMap<String, String> = [
        "/Game/Maps/Ascent/Ascent",
        "/Game/Maps/Bonsai/Bonsai",
        "/Game/Maps/Canyon/Canyon",
    ]
    .into_iter()
    .map(|p| (p.to_string(), p.rsplit('/').next().unwrap().to_string()))
    .collect();
    let stats = t.time("load_my_stats: user, current act", || {
        load_my_stats(
            &repo,
            USER,
            &act,
            scale::NOW_MS,
            &|p: &str| Some(p.to_string()),
            &map_names,
            &|_| 0,
        )
        .unwrap()
    });
    assert_eq!(stats.last5.len(), 5);
    assert!(stats.rank.tier.is_some());
    let games = stats.rank.wins.unwrap_or(0) + stats.rank.losses.unwrap_or(0);
    assert!((200..=260).contains(&games), "{games} games in the act");
    assert!(!stats.rr_series.is_empty() && !stats.maps.is_empty());

    // --- Player Details: the real `details::build` -------------------------
    let dir = tempfile::tempdir().unwrap();
    let st = app_state(&db, dir.path());
    for (name, who) in [
        ("user", USER.to_string()),
        ("duo", DUO.to_string()),
        ("stranger", stranger.clone()),
    ] {
        let mut best = f64::MAX;
        let mut built = None;
        for _ in 0..3 {
            let started = Instant::now();
            let d = crate::details::build(&st, &who).await.unwrap();
            best = best.min(started.elapsed().as_secs_f64() * 1000.0);
            built = Some(d);
        }
        t.record(&format!("details::build: {name}"), best);
        let d = built.unwrap();
        match who.as_str() {
            USER => {
                assert_eq!(d.recent.len(), RECENT_COUNT as usize);
                assert_eq!(d.top_teammates.len(), 3);
                assert_eq!(d.top_teammates[0].puuid, DUO, "the duo is the top teammate");
                assert!(d.encounters.is_empty(), "no encounters with yourself");
                assert_eq!(d.hero_matches, 20);
            }
            DUO => {
                assert_eq!(d.recent.len(), RECENT_COUNT as usize);
                assert_eq!(d.top_teammates.len(), 3);
                assert!(d.encounters.len() > 800, "{}", d.encounters.len());
            }
            _ => assert!(d.encounters.len() < 40),
        }
    }

    t.finish("the shell's reads on the scale db");
}

// ---------------------------------------------------------------------
// the plans of the queries `store.rs` writes itself
// ---------------------------------------------------------------------
//
// `Repo`'s own queries have their plans pinned in `opd1-core`'s `tests/db.rs`.
// These three are SQL in `store.rs` (run through `Repo::conn()`), so their
// plans are pinned here, next to the timings above. None needed a new index.

#[test]
fn the_comp_updates_reads_are_index_backed_and_need_no_sort() {
    // `idx_comp_updates_puuid_start (puuid, start_ms)` hands the player's rows
    // over newest first (the index is read backwards): no sort step. The
    // season is a filter on those rows, not a seek (a player's own rows only:
    // 2 000 of them read in about a millisecond, see the timings above).
    let one_index = ["SEARCH comp_updates USING INDEX idx_comp_updates_puuid_start (puuid=?)"];
    // `recent_comp_updates`: the card's RR rows.
    assert_eq!(
        scale::schema_only_plan_of(RECENT_COMP_UPDATES_SQL),
        one_index
    );
    // `season_or_recent_comp_updates`: My Stats' act rows plus the last 7 days.
    assert_eq!(
        scale::schema_only_plan_of(SEASON_OR_RECENT_COMP_UPDATES_SQL),
        one_index
    );
}

#[test]
fn recent_refs_is_index_backed_and_needs_no_sort() {
    // Every card's last two matches (party detection): the player's newest two
    // rows of `history_index` by `idx_history_index_puuid_start`, each match's
    // team by the `(match_id, puuid)` primary key.
    assert_eq!(
        scale::schema_only_plan_of(RECENT_REFS_SQL),
        [
            "SEARCH h USING INDEX idx_history_index_puuid_start (puuid=?)",
            "SEARCH mp USING INDEX sqlite_autoindex_match_players_1 (match_id=? AND puuid=?) LEFT-JOIN",
        ]
    );
}
