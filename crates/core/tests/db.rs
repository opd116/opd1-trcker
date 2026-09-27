//! Tests for `opd1_core::db` — schema, migrations, and the typed sync
//! repository (Task 4).

use std::path::Path;

use opd1_core::db::open_db;
use opd1_core::db::repo::{
    CompUpdateRow, HistoryEntry, Match, MatchPlayer, MatchTeam, MmrSnapshot, Player, Repo,
};

fn repo_in_memory() -> Repo {
    let conn = open_db(Path::new(":memory:")).expect("open in-memory db");
    Repo::new(conn)
}

fn sample_match(id: &str, start_ms: i64, winning_team: Option<&str>) -> Match {
    Match {
        match_id: id.to_string(),
        map_id: "Ascent".to_string(),
        queue_id: "competitive".to_string(),
        start_ms,
        winning_team: winning_team.map(str::to_string),
        is_ranked: true,
        rounds_played: Some(24),
    }
}

fn sample_player(puuid: &str, team_id: &str, game_name: &str, tag_line: &str) -> MatchPlayer {
    MatchPlayer {
        puuid: puuid.to_string(),
        team_id: team_id.to_string(),
        party_id: None,
        agent_id: Some("jett".to_string()),
        kills: 20,
        deaths: 15,
        assists: 5,
        score: 4000,
        rounds: Some(24),
        hs: 10,
        bs: 20,
        ls: 5,
        game_name: Some(game_name.to_string()),
        tag_line: Some(tag_line.to_string()),
    }
}

// ---------------------------------------------------------------------
// Migrations
// ---------------------------------------------------------------------

#[test]
fn migrations_idempotent() {
    let tmp = tempfile::NamedTempFile::new().expect("temp file");
    let path = tmp.path();

    {
        let conn = open_db(path).expect("first open applies migrations");
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, 1);
    }

    // Reopening an already-migrated file must succeed, not re-run schema
    // creation (which would error on `CREATE TABLE` of an existing table).
    let conn = open_db(path).expect("second open is idempotent");
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, 1);

    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM players", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn open_db_sets_wal_journal_mode() {
    let tmp = tempfile::NamedTempFile::new().expect("temp file");
    let conn = open_db(tmp.path()).unwrap();
    let mode: String = conn
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap();
    assert_eq!(mode.to_lowercase(), "wal");
}

// ---------------------------------------------------------------------
// players
// ---------------------------------------------------------------------

#[test]
fn upsert_player_then_query_directly() {
    let repo = repo_in_memory();
    repo.upsert_player(&Player {
        puuid: "p1".into(),
        game_name: "Foo".into(),
        tag_line: "123".into(),
        account_level: 42,
        card_id: "card-a".into(),
        updated_ms: 1_000,
    })
    .unwrap();

    // A second upsert overwrites in place rather than duplicating.
    repo.upsert_player(&Player {
        puuid: "p1".into(),
        game_name: "Bar".into(),
        tag_line: "456".into(),
        account_level: 43,
        card_id: "card-b".into(),
        updated_ms: 2_000,
    })
    .unwrap();

    let conn = repo.conn();
    let (count, game_name, level): (i64, String, i64) = conn
        .query_row(
            "SELECT COUNT(*), MAX(game_name), MAX(account_level) FROM players WHERE puuid = 'p1'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(count, 1);
    assert_eq!(game_name, "Bar");
    assert_eq!(level, 43);
}

// ---------------------------------------------------------------------
// matches / match_teams / match_players
// ---------------------------------------------------------------------

#[test]
fn upsert_match_stores_teams_and_players() {
    let repo = repo_in_memory();
    let m = sample_match("m1", 1_000, Some("Blue"));
    let teams = vec![
        MatchTeam {
            team_id: "Blue".into(),
            won: true,
            rounds_won: Some(13),
        },
        MatchTeam {
            team_id: "Red".into(),
            won: false,
            rounds_won: Some(9),
        },
    ];
    let players = vec![
        sample_player("self", "Blue", "Self", "111"),
        sample_player("other", "Red", "Other", "222"),
    ];
    repo.upsert_match(&m, &teams, &players).unwrap();

    // Calling again with changed data overwrites rather than duplicating
    // (match details are immutable in practice, but the repo call itself
    // must still be safely repeatable).
    let mut players2 = players.clone();
    players2[0].kills = 30;
    repo.upsert_match(&m, &teams, &players2).unwrap();

    let conn = repo.conn();
    let match_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM matches", [], |r| r.get(0))
        .unwrap();
    let team_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM match_teams WHERE match_id='m1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let player_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM match_players WHERE match_id='m1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let kills: i64 = conn
        .query_row(
            "SELECT kills FROM match_players WHERE match_id='m1' AND puuid='self'",
            [],
            |r| r.get(0),
        )
        .unwrap();

    assert_eq!(match_count, 1);
    assert_eq!(team_count, 2);
    assert_eq!(player_count, 2);
    assert_eq!(kills, 30);
}

#[test]
fn new_match_ids_filters_known() {
    let repo = repo_in_memory();
    repo.upsert_match(&sample_match("m1", 1_000, None), &[], &[])
        .unwrap();

    let ids = vec!["m2".to_string(), "m1".to_string(), "m3".to_string()];
    let result = repo.new_match_ids(&ids).unwrap();

    // "m1" is known and dropped; input order is preserved otherwise.
    assert_eq!(result, vec!["m2".to_string(), "m3".to_string()]);
}

#[test]
fn index_history_dedups_on_conflict() {
    let repo = repo_in_memory();
    let entries = vec![
        HistoryEntry {
            match_id: "m1".into(),
            queue_id: "competitive".into(),
            start_ms: 1_000,
        },
        HistoryEntry {
            match_id: "m2".into(),
            queue_id: "deathmatch".into(),
            start_ms: 2_000,
        },
    ];
    repo.index_history("self", &entries).unwrap();
    // Re-indexing the same match_id overwrites, not duplicates.
    repo.index_history(
        "self",
        &[HistoryEntry {
            match_id: "m1".into(),
            queue_id: "unrated".into(),
            start_ms: 1_000,
        }],
    )
    .unwrap();

    let conn = repo.conn();
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM history_index WHERE puuid='self'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let queue: String = conn
        .query_row(
            "SELECT queue_id FROM history_index WHERE puuid='self' AND match_id='m1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 2);
    assert_eq!(queue, "unrated");
}

#[test]
fn last_n_comp_filters_competitive_and_orders_newest_first() {
    let repo = repo_in_memory();
    repo.upsert_match(
        &sample_match("comp1", 1_000, Some("Blue")),
        &[],
        &[sample_player("self", "Blue", "Self", "111")],
    )
    .unwrap();
    repo.upsert_match(
        &sample_match("comp2", 2_000, Some("Red")),
        &[],
        &[sample_player("self", "Blue", "Self", "111")],
    )
    .unwrap();
    let mut dm = sample_match("dm1", 3_000, None);
    dm.queue_id = "deathmatch".into();
    repo.upsert_match(&dm, &[], &[sample_player("self", "Blue", "Self", "111")])
        .unwrap();

    let rows = repo.last_n_comp("self", 5).unwrap();
    assert_eq!(rows.len(), 2, "deathmatch match must be excluded");
    assert_eq!(rows[0].match_id, "comp2"); // newest first
    assert_eq!(rows[1].match_id, "comp1");
}

// ---------------------------------------------------------------------
// mmr_snapshots
// ---------------------------------------------------------------------

#[test]
fn needs_mmr_refresh_false_when_same_latest() {
    let repo = repo_in_memory();
    assert!(repo.needs_mmr_refresh("self", "m1").unwrap()); // no snapshot yet

    repo.save_mmr_snapshot(&MmrSnapshot {
        puuid: "self".into(),
        json: "{}".into(),
        last_comp_match_id: Some("m1".into()),
        fetched_ms: 1_000,
    })
    .unwrap();

    assert!(!repo.needs_mmr_refresh("self", "m1").unwrap());
    assert!(repo.needs_mmr_refresh("self", "m2").unwrap());
}

#[test]
fn mmr_snapshot_round_trip() {
    let repo = repo_in_memory();
    assert!(repo.load_mmr_snapshot("self").unwrap().is_none());

    repo.save_mmr_snapshot(&MmrSnapshot {
        puuid: "self".into(),
        json: "{\"rank\":42}".into(),
        last_comp_match_id: Some("m1".into()),
        fetched_ms: 5_000,
    })
    .unwrap();

    let loaded = repo
        .load_mmr_snapshot("self")
        .unwrap()
        .expect("snapshot exists");
    assert_eq!(loaded.json, "{\"rank\":42}");
    assert_eq!(loaded.last_comp_match_id.as_deref(), Some("m1"));
    assert_eq!(loaded.fetched_ms, 5_000);
}

// ---------------------------------------------------------------------
// comp_updates
// ---------------------------------------------------------------------

#[test]
fn comp_updates_round_trip_orders_newest_first() {
    let repo = repo_in_memory();
    let rows = vec![
        CompUpdateRow {
            match_id: "m1".into(),
            season_id: "s1".into(),
            tier_after: 18,
            tier_before: Some(17),
            rr_after: 40,
            rr_earned: 20,
            map_id: Some("Ascent".into()),
            start_ms: 1_000,
        },
        CompUpdateRow {
            match_id: "m2".into(),
            season_id: "s1".into(),
            tier_after: 19,
            tier_before: Some(18),
            rr_after: 10,
            rr_earned: -10,
            map_id: Some("Bind".into()),
            start_ms: 2_000,
        },
    ];
    repo.upsert_comp_updates("self", &rows).unwrap();

    let fetched = repo.comp_updates_for_season("self", "s1").unwrap();
    assert_eq!(fetched.len(), 2);
    assert_eq!(fetched[0].match_id, "m2"); // newest first
    assert_eq!(fetched[1].match_id, "m1");
    assert_eq!(fetched[0].tier_before, Some(18));
}

// ---------------------------------------------------------------------
// known_name — privacy rule: only from a match self also played in
// ---------------------------------------------------------------------

#[test]
fn known_name_requires_shared_match() {
    let repo = repo_in_memory();

    // Match A: self and other share this match; other's name here is
    // "Foo#123".
    repo.upsert_match(
        &sample_match("shared", 1_000, None),
        &[],
        &[
            sample_player("self", "Blue", "Self", "111"),
            sample_player("other", "Blue", "Foo", "123"),
        ],
    )
    .unwrap();

    // Match B: only "other" has a row here (self never played it), with a
    // different, more recent name. This name must never leak to self.
    repo.upsert_match(
        &sample_match("not-shared", 2_000, None),
        &[],
        &[sample_player("other", "Red", "Bar", "456")],
    )
    .unwrap();

    let name = repo.known_name("self", "other").unwrap();
    assert_eq!(name, Some(("Foo".to_string(), "123".to_string())));
}

#[test]
fn known_name_none_when_never_shared() {
    let repo = repo_in_memory();
    repo.upsert_match(
        &sample_match("solo", 1_000, None),
        &[],
        &[sample_player("other", "Blue", "Foo", "123")],
    )
    .unwrap();

    assert_eq!(repo.known_name("self", "other").unwrap(), None);
}

// ---------------------------------------------------------------------
// encounters
// ---------------------------------------------------------------------

#[test]
fn encounters_counts_with_against() {
    let repo = repo_in_memory();

    // with: same team, self's team wins -> self_won = Some(true)
    repo.upsert_match(
        &sample_match("m1", 1_000, Some("Blue")),
        &[],
        &[
            sample_player("self", "Blue", "Self", "111"),
            sample_player("other", "Blue", "Other", "222"),
        ],
    )
    .unwrap();

    // against: different teams, self's team loses -> self_won = Some(false)
    repo.upsert_match(
        &sample_match("m2", 2_000, Some("Red")),
        &[],
        &[
            sample_player("self", "Blue", "Self", "111"),
            sample_player("other", "Red", "Other", "222"),
        ],
    )
    .unwrap();

    // with: same team, draw/no winner -> self_won = None
    repo.upsert_match(
        &sample_match("m3", 3_000, None),
        &[],
        &[
            sample_player("self", "Blue", "Self", "111"),
            sample_player("other", "Blue", "Other", "222"),
        ],
    )
    .unwrap();

    let encounters = repo.encounters("self", "other").unwrap();
    assert_eq!(encounters.with, 2);
    assert_eq!(encounters.against, 1);
    assert_eq!(encounters.matches.len(), 3);

    // Newest first.
    assert_eq!(encounters.matches[0].match_id, "m3");
    assert!(encounters.matches[0].with);
    assert_eq!(encounters.matches[0].self_won, None);

    assert_eq!(encounters.matches[1].match_id, "m2");
    assert!(!encounters.matches[1].with);
    assert_eq!(encounters.matches[1].self_won, Some(false));

    assert_eq!(encounters.matches[2].match_id, "m1");
    assert!(encounters.matches[2].with);
    assert_eq!(encounters.matches[2].self_won, Some(true));
}

// ---------------------------------------------------------------------
// top_teammates
// ---------------------------------------------------------------------

#[test]
fn top_teammates_orders_by_count() {
    let repo = repo_in_memory();
    let season_start = 1_000;

    for (id, start) in [("a1", 1_000), ("a2", 2_000), ("a3", 3_000)] {
        repo.upsert_match(
            &sample_match(id, start, None),
            &[],
            &[
                sample_player("self", "Blue", "Self", "111"),
                sample_player("teamA", "Blue", "AName", "AAA"),
            ],
        )
        .unwrap();
    }

    for (id, start) in [("b1", 4_000), ("b2", 9_000)] {
        repo.upsert_match(
            &sample_match(id, start, None),
            &[],
            &[
                sample_player("self", "Blue", "Self", "111"),
                sample_player("teamB", "Blue", "BName", "BBB"),
            ],
        )
        .unwrap();
    }

    // Before the season window -> must not count at all.
    repo.upsert_match(
        &sample_match("c1", 0, None),
        &[],
        &[
            sample_player("self", "Blue", "Self", "111"),
            sample_player("teamC", "Blue", "CName", "CCC"),
        ],
    )
    .unwrap();

    let top = repo.top_teammates("self", season_start, 2).unwrap();
    assert_eq!(top.len(), 2, "limit must be respected");
    assert_eq!(top[0].puuid, "teamA");
    assert_eq!(top[0].count, 3);
    assert_eq!(top[0].game_name.as_deref(), Some("AName"));
    assert_eq!(top[1].puuid, "teamB");
    assert_eq!(top[1].count, 2);

    let all = repo.top_teammates("self", season_start, 10).unwrap();
    assert_eq!(
        all.len(),
        2,
        "teamC's only match is before the season window"
    );
}

#[test]
fn top_teammates_tie_breaks_by_recency() {
    let repo = repo_in_memory();

    // teamA and teamB both share exactly 1 match; teamB's is more recent.
    repo.upsert_match(
        &sample_match("older", 1_000, None),
        &[],
        &[
            sample_player("self", "Blue", "Self", "111"),
            sample_player("teamA", "Blue", "AName", "AAA"),
        ],
    )
    .unwrap();
    repo.upsert_match(
        &sample_match("newer", 2_000, None),
        &[],
        &[
            sample_player("self", "Blue", "Self", "111"),
            sample_player("teamB", "Blue", "BName", "BBB"),
        ],
    )
    .unwrap();

    let top = repo.top_teammates("self", 0, 10).unwrap();
    assert_eq!(top[0].puuid, "teamB"); // more recent shared match wins the tie
    assert_eq!(top[1].puuid, "teamA");
}
