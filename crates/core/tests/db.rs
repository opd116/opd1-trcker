//! Tests for `opd1_core::db` — schema, migrations, and the typed sync
//! repository (Task 4).

use std::path::Path;

use opd1_core::db::open_db;
use opd1_core::db::repo::{
    self, CompUpdateRow, EncounterRow, HistoryEntry, Match, MatchPlayer, MatchTeam, MmrSnapshot,
    Player, Repo,
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
        game_length_ms: None,
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
        competitive_tier: None,
        damage: None,
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
        assert_eq!(version, 2);
    }

    // Reopening an already-migrated file must succeed, not re-run schema
    // creation (which would error on `CREATE TABLE` of an existing table).
    let conn = open_db(path).expect("second open is idempotent");
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, 2);

    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM players", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
}

/// Migration 1 is frozen (R89): it is the schema exactly as it stood at
/// `c9ad7f4`. A DB from an earlier build has run it; editing it would make
/// fresh and upgraded DBs differ. Every later change is a new migration.
#[test]
fn migration1_is_frozen() {
    assert_eq!(
        include_str!("../src/db/schema.sql"),
        include_str!("fixtures/schema_v1.sql"),
        "migration 1 (schema.sql) is frozen: append a new migration instead"
    );
}

fn column_names(conn: &rusqlite::Connection, table: &str) -> Vec<String> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .unwrap();
    let names = stmt.query_map([], |r| r.get::<_, String>(1)).unwrap();
    names.map(Result::unwrap).collect()
}

fn user_version(conn: &rusqlite::Connection) -> i64 {
    conn.query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap()
}

#[test]
fn fresh_db_has_phase2_columns_at_user_version_2() {
    let conn = open_db(Path::new(":memory:")).unwrap();
    assert_eq!(user_version(&conn), 2);
    for (table, column) in [
        ("matches", "game_length_ms"),
        ("match_teams", "rounds_won_h1"),
        ("match_players", "competitive_tier"),
        ("match_players", "damage"),
    ] {
        assert!(
            column_names(&conn, table).iter().any(|c| c == column),
            "{table}.{column} missing"
        );
    }
}

#[test]
fn v1_db_upgrades_to_user_version_2() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    {
        // A DB as an earlier build left it: migration 1 only, one match.
        let conn = rusqlite::Connection::open(tmp.path()).unwrap();
        conn.execute_batch(include_str!("fixtures/schema_v1.sql"))
            .unwrap();
        conn.execute_batch(
            "PRAGMA user_version = 1;
             INSERT INTO matches VALUES ('old', 'Ascent', 'competitive', 1000, 'Blue', 1, 24);
             INSERT INTO match_teams VALUES ('old', 'Blue', 1, 13);
             INSERT INTO match_players VALUES
                ('old', 'self', 'Blue', NULL, 'jett', 20, 15, 5, 4000, 24, 10, 20, 5, 'Self', '111');",
        )
        .unwrap();
        for (table, column) in [("matches", "game_length_ms"), ("match_players", "damage")] {
            assert!(!column_names(&conn, table).iter().any(|c| c == column));
        }
    }

    let conn = open_db(tmp.path()).expect("upgrade runs migration 2 only");
    assert_eq!(user_version(&conn), 2);
    let old: [Option<i64>; 4] = conn
        .query_row(
            "SELECT m.game_length_ms, t.rounds_won_h1, p.competitive_tier, p.damage
             FROM matches m
             JOIN match_teams t ON t.match_id = m.match_id
             JOIN match_players p ON p.match_id = m.match_id
             WHERE m.match_id = 'old'",
            [],
            |r| Ok([r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?]),
        )
        .unwrap();
    assert_eq!(old, [None; 4], "the old rows keep NULL in the new columns");

    let repo = Repo::new(conn);
    let mut m = sample_match("new", 2_000, Some("Blue"));
    m.game_length_ms = Some(2_280_000);
    let mut blue = team("Blue", true, Some(13));
    blue.rounds_won_h1 = Some(7);
    let mut me = sample_player("self", "Blue", "Self", "111");
    me.competitive_tier = Some(15);
    me.damage = Some(3_100);
    repo.upsert_match(&m, &[blue], &[me]).unwrap();
    let stored = repo.match_rows("new").unwrap().unwrap();
    assert_eq!(stored.info.game_length_ms, Some(2_280_000));
    assert_eq!(stored.teams[0].rounds_won_h1, Some(7));
    assert_eq!(stored.players[0].competitive_tier, Some(15));
    assert_eq!(stored.players[0].damage, Some(3_100));
    drop(repo);

    // A second open runs no migration: `ADD COLUMN` twice would fail.
    let conn = open_db(tmp.path()).expect("reopen is a no-op");
    assert_eq!(user_version(&conn), 2);
    let stored: i64 = conn
        .query_row("SELECT COUNT(*) FROM matches", [], |r| r.get(0))
        .unwrap();
    assert_eq!(stored, 2);
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
            rounds_won_h1: None,
        },
        MatchTeam {
            team_id: "Red".into(),
            won: false,
            rounds_won: Some(9),
            rounds_won_h1: None,
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

fn team(id: &str, won: bool, rounds_won: Option<i64>) -> MatchTeam {
    MatchTeam {
        team_id: id.into(),
        won,
        rounds_won,
        rounds_won_h1: None,
    }
}

#[test]
fn comp_matches_join_round_score_and_rr_within_range() {
    let repo = repo_in_memory();
    let me = || sample_player("self", "Blue", "Self", "111");
    // m1: a 13-9 win with its RR row.
    repo.upsert_match(
        &sample_match("m1", 1_000, Some("Blue")),
        &[team("Blue", true, Some(13)), team("Red", false, Some(9))],
        &[me(), sample_player("enemy", "Red", "E", "1")],
    )
    .unwrap();
    // m2: no team rows and no RR row stored.
    repo.upsert_match(&sample_match("m2", 2_000, None), &[], &[me()])
        .unwrap();
    // m3: three teams — "rounds lost" has no single opponent.
    repo.upsert_match(
        &sample_match("m3", 3_000, Some("Red")),
        &[
            team("Blue", false, Some(5)),
            team("Red", true, Some(13)),
            team("Green", false, Some(2)),
        ],
        &[me()],
    )
    .unwrap();
    // Outside the range, and another queue: both excluded.
    repo.upsert_match(&sample_match("m4", 4_000, None), &[], &[me()])
        .unwrap();
    let mut dm = sample_match("dm", 2_500, None);
    dm.queue_id = "deathmatch".into();
    repo.upsert_match(&dm, &[], &[me()]).unwrap();
    repo.upsert_comp_updates(
        "self",
        &[CompUpdateRow {
            match_id: "m1".into(),
            season_id: "act".into(),
            tier_after: 15,
            tier_before: Some(14),
            rr_after: 40,
            rr_earned: 21,
            map_id: None,
            start_ms: 1_000,
        }],
    )
    .unwrap();
    // Another player's RR row for m2 never leaks into the user's row.
    repo.upsert_comp_updates(
        "enemy",
        &[CompUpdateRow {
            match_id: "m2".into(),
            season_id: "act".into(),
            tier_after: 9,
            tier_before: Some(9),
            rr_after: 10,
            rr_earned: -18,
            map_id: None,
            start_ms: 2_000,
        }],
    )
    .unwrap();

    let rows = repo.comp_matches("self", 1_000..4_000, 10).unwrap();
    let ids: Vec<&str> = rows.iter().map(|r| r.match_id.as_str()).collect();
    assert_eq!(
        ids,
        ["m3", "m2", "m1"],
        "newest first; range end exclusive; competitive only"
    );
    // (rounds won, rounds lost, RR change)
    let score_rr: Vec<[Option<i64>; 3]> = rows
        .iter()
        .map(|r| [r.rounds_won, r.rounds_lost, r.rr_earned])
        .collect();
    assert_eq!(
        score_rr,
        [
            [Some(5), None, None],
            [None, None, None],
            [Some(13), Some(9), Some(21)],
        ]
    );
    // The same RR row's Riot season and tiers ride along (My Stats decides
    // act membership by season and spots placements by tier); no RR row
    // -> all `None`.
    let season_tiers: Vec<(Option<&str>, Option<i64>, Option<i64>)> = rows
        .iter()
        .map(|r| (r.season_id.as_deref(), r.tier_before, r.tier_after))
        .collect();
    assert_eq!(
        season_tiers,
        [
            (None, None, None),
            (None, None, None),
            (Some("act"), Some(14), Some(15)),
        ]
    );
    let m1 = &rows[2];
    assert_eq!(
        (m1.team_id.as_str(), m1.winning_team.as_deref()),
        ("Blue", Some("Blue"))
    );
    assert_eq!(
        (m1.kills, m1.deaths, m1.assists, m1.score),
        (20, 15, 5, 4000)
    );
    assert_eq!((m1.rounds, m1.hs, m1.bs, m1.ls), (Some(24), 10, 20, 5));
    assert_eq!(
        (m1.map_id.as_str(), m1.agent_id.as_deref()),
        ("Ascent", Some("jett"))
    );

    let newest = repo.comp_matches("self", i64::MIN..i64::MAX, 1).unwrap();
    assert_eq!(newest.len(), 1);
    assert_eq!(newest[0].match_id, "m4");
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

    let name = repo.known_name("self", "other").unwrap().unwrap();
    assert_eq!((name.name.as_str(), name.tag.as_str()), ("Foo", "123"));
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

#[test]
fn known_name_returns_source_match() {
    let repo = repo_in_memory();
    let on = |id: &str, start_ms: i64, map: &str| Match {
        map_id: map.to_string(),
        ..sample_match(id, start_ms, None)
    };

    // Two shared matches: the older on Duality, the newer on Split. The
    // name (and where it was seen) comes from the newer row.
    repo.upsert_match(
        &on("older", 1_000, "/Game/Maps/Duality/Duality"),
        &[],
        &[
            sample_player("self", "Blue", "Self", "111"),
            sample_player("other", "Red", "OldName", "OLD"),
        ],
    )
    .unwrap();
    repo.upsert_match(
        &on("newer", 2_000, "/Game/Maps/Bonsai/Bonsai"),
        &[],
        &[
            sample_player("self", "Blue", "Self", "111"),
            sample_player("other", "Red", "NewName", "NEW"),
        ],
    )
    .unwrap();

    let known = repo.known_name("self", "other").unwrap().unwrap();
    assert_eq!(
        (known.name.as_str(), known.tag.as_str()),
        ("NewName", "NEW")
    );
    // The stored Riot path, not a uuid: the shell converts it later (R84).
    assert_eq!(known.map_id.as_deref(), Some("/Game/Maps/Bonsai/Bonsai"));
    assert_eq!(known.start_ms, Some(2_000));

    // A name seen only in a match the user did not play stays unknown.
    repo.upsert_match(
        &on("stranger-match", 3_000, "/Game/Maps/Ascent/Ascent"),
        &[],
        &[sample_player("loner", "Blue", "Hidden", "H1")],
    )
    .unwrap();
    assert_eq!(repo.known_name("self", "loner").unwrap(), None);
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

// ---------------------------------------------------------------------
// app_state (Task 5b, ruling R31)
// ---------------------------------------------------------------------

#[test]
fn app_state_get_set_round_trip() {
    let repo = repo_in_memory();
    assert_eq!(repo.get_state("self_puuid").unwrap(), None);

    repo.set_state("self_puuid", "aaaa").unwrap();
    assert_eq!(
        repo.get_state("self_puuid").unwrap().as_deref(),
        Some("aaaa")
    );

    // Upsert: a second set overwrites, never duplicates.
    repo.set_state("self_puuid", "bbbb").unwrap();
    assert_eq!(
        repo.get_state("self_puuid").unwrap().as_deref(),
        Some("bbbb")
    );
    let rows: i64 = repo
        .conn()
        .query_row("SELECT COUNT(*) FROM app_state", [], |r| r.get(0))
        .unwrap();
    assert_eq!(rows, 1);
}

// ---------------------------------------------------------------------
// Task 15: Phase 2 columns, R81 tier rule, R88 comp_matches, encounters
// ---------------------------------------------------------------------

fn comp_update(match_id: &str, start_ms: i64, tier_after: i64) -> CompUpdateRow {
    CompUpdateRow {
        match_id: match_id.into(),
        season_id: "act".into(),
        tier_after,
        tier_before: Some(tier_after),
        rr_after: 40,
        rr_earned: 10,
        map_id: None,
        start_ms,
    }
}

#[test]
fn upsert_match_round_trips_phase2_columns() {
    let repo = repo_in_memory();
    let mut m = sample_match("m1", 1_000, Some("Blue"));
    m.game_length_ms = Some(2_280_000);
    let mut blue = team("Blue", true, Some(13));
    blue.rounds_won_h1 = Some(8);
    let red = team("Red", false, Some(9)); // rounds_won_h1 stays NULL
    let mut me = sample_player("self", "Blue", "Self", "111");
    me.competitive_tier = Some(21);
    me.damage = Some(3_456);
    let other = sample_player("other", "Red", "Other", "222"); // both stay NULL
    repo.upsert_match(&m, &[blue.clone(), red], &[me.clone(), other])
        .unwrap();

    let stored = repo.match_rows("m1").unwrap().unwrap();
    assert_eq!(stored.info.game_length_ms, Some(2_280_000));
    let h1: Vec<(&str, Option<i64>)> = stored
        .teams
        .iter()
        .map(|t| (t.team_id.as_str(), t.rounds_won_h1))
        .collect();
    assert_eq!(h1, [("Blue", Some(8)), ("Red", None)]);
    let by_puuid = |p: &str| stored.players.iter().find(|x| x.puuid == p).unwrap();
    assert_eq!(
        (by_puuid("self").competitive_tier, by_puuid("self").damage),
        (Some(21), Some(3_456))
    );
    assert_eq!(
        (by_puuid("other").competitive_tier, by_puuid("other").damage),
        (None, None)
    );

    // Replaying the upsert overwrites the columns like the others.
    let mut me2 = me;
    me2.damage = Some(9);
    m.game_length_ms = Some(1);
    repo.upsert_match(&m, &[blue], &[me2]).unwrap();
    let again = repo.match_rows("m1").unwrap().unwrap();
    assert_eq!(again.info.game_length_ms, Some(1));
    assert_eq!(
        again
            .players
            .iter()
            .find(|p| p.puuid == "self")
            .unwrap()
            .damage,
        Some(9)
    );
}

#[test]
fn comp_matches_tier_prefers_match_tier_then_comp_update() {
    let repo = repo_in_memory();
    // A promotion game: Riot's at-match tier is 20, the RR row's tier
    // after the match is 21 (R81).
    let mut promoted = sample_player("self", "Blue", "Self", "111");
    promoted.competitive_tier = Some(20);
    repo.upsert_match(
        &sample_match("promo", 2_000, Some("Blue")),
        &[],
        &[promoted],
    )
    .unwrap();
    // An older row without Riot's tier: the RR row's tier stands in.
    repo.upsert_match(
        &sample_match("legacy", 1_000, Some("Blue")),
        &[],
        &[sample_player("self", "Blue", "Self", "111")],
    )
    .unwrap();
    repo.upsert_comp_updates(
        "self",
        &[
            comp_update("promo", 2_000, 21),
            comp_update("legacy", 1_000, 21),
        ],
    )
    .unwrap();

    let rows = repo.comp_matches("self", i64::MIN..i64::MAX, 10).unwrap();
    let tiers: Vec<(&str, Option<i64>, Option<i64>)> = rows
        .iter()
        .map(|r| (r.match_id.as_str(), r.tier, r.tier_after))
        .collect();
    assert_eq!(
        tiers,
        [
            ("promo", Some(20), Some(21)),
            ("legacy", Some(21), Some(21))
        ],
        "tier = the at-match rank first; tier_after stays the RR row's, for My Stats' math"
    );
}

/// Binds the parameters `COMP_MATCHES_SQL` expects (puuid, from, to,
/// limit) and returns the `detail` column of each plan row.
fn comp_matches_plan(conn: &rusqlite::Connection) -> Vec<String> {
    let mut stmt = conn
        .prepare(&format!("EXPLAIN QUERY PLAN {}", repo::COMP_MATCHES_SQL))
        .unwrap();
    let rows = stmt
        .query_map(rusqlite::params!["self", i64::MIN, i64::MAX, 8u32], |r| {
            r.get::<_, String>(3)
        })
        .unwrap();
    rows.map(Result::unwrap).collect()
}

#[test]
fn comp_matches_has_no_correlated_subqueries() {
    // R88: the player's rows are limited first, then joined. A correlated
    // scalar subquery re-runs once per row of the whole competitive
    // history; the join must read `match_teams` by key only.
    let conn = open_db(Path::new(":memory:")).unwrap();
    let plan = comp_matches_plan(&conn);
    assert!(!plan.is_empty());
    for detail in &plan {
        assert!(
            !detail.contains("CORRELATED"),
            "correlated subquery in the plan: {detail}\nfull plan: {plan:#?}"
        );
    }
    let team_reads: Vec<&String> = plan.iter().filter(|d| d.contains("match_teams")).collect();
    assert!(
        !team_reads.is_empty(),
        "the plan must read match_teams: {plan:#?}"
    );
    for detail in team_reads {
        assert!(
            detail.starts_with("SEARCH"),
            "match_teams is read by key, never scanned: {detail}"
        );
    }
}

#[test]
fn comp_matches_limits_before_joining() {
    let repo = repo_in_memory();
    // 30 competitive matches, 13 - i' rounds lost so every row is distinct;
    // the 27th has a third team, so it has no single opponent.
    for i in 1..=30_i64 {
        let start = i * 1_000;
        let mut teams = vec![
            team("Blue", true, Some(13)),
            team("Red", false, Some(i % 12)),
        ];
        if i == 27 {
            teams.push(team("Green", false, Some(4)));
        }
        repo.upsert_match(
            &sample_match(&format!("m{i:02}"), start, Some("Blue")),
            &teams,
            &[sample_player("self", "Blue", "Self", "111")],
        )
        .unwrap();
    }

    let rows = repo.comp_matches("self", i64::MIN..i64::MAX, 8).unwrap();
    let got: Vec<(String, Option<i64>, Option<i64>)> = rows
        .iter()
        .map(|r| (r.match_id.clone(), r.rounds_won, r.rounds_lost))
        .collect();
    let want: Vec<(String, Option<i64>, Option<i64>)> = (23..=30_i64)
        .rev()
        .map(|i| {
            let lost = if i == 27 { None } else { Some(i % 12) };
            (format!("m{i:02}"), Some(13), lost)
        })
        .collect();
    assert_eq!(got, want, "the 8 newest, newest first, scores intact");
    assert_eq!(
        repo.comp_matches("self", i64::MIN..i64::MAX, 0).unwrap(),
        [],
        "limit 0 reads nothing"
    );
}

#[test]
fn encounters_rows_carry_agents_tiers_score_length() {
    let repo = repo_in_memory();
    // Against: self (Jett, tier 21 by Riot's match value) loses 10-13 to
    // other (Reyna, tier 24 only from the RR row's tier after the match).
    let mut m = sample_match("m1", 1_000, Some("Red"));
    m.game_length_ms = Some(2_460_000);
    let mut me = sample_player("self", "Blue", "Self", "111");
    me.agent_id = Some("jett".into());
    me.competitive_tier = Some(21);
    let mut other = sample_player("other", "Red", "Other", "222");
    other.agent_id = Some("reyna".into());
    repo.upsert_match(
        &m,
        &[team("Blue", false, Some(10)), team("Red", true, Some(13))],
        &[me, other],
    )
    .unwrap();
    repo.upsert_comp_updates("other", &[comp_update("m1", 1_000, 24)])
        .unwrap();
    // With: same team, no length, no tiers, no teams stored.
    let mut me2 = sample_player("self", "Blue", "Self", "111");
    me2.agent_id = None;
    repo.upsert_match(
        &sample_match("m2", 2_000, Some("Blue")),
        &[],
        &[me2, sample_player("other", "Blue", "Other", "222")],
    )
    .unwrap();

    let rows = repo.encounters("self", "other").unwrap().matches;
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows[1],
        EncounterRow {
            match_id: "m1".into(),
            map_id: "Ascent".into(),
            start_ms: 1_000,
            with: false,
            self_won: Some(false),
            rounds_won: Some(10),
            rounds_lost: Some(13),
            game_length_ms: Some(2_460_000),
            self_agent_id: Some("jett".into()),
            other_agent_id: Some("reyna".into()),
            self_tier: Some(21),
            other_tier: Some(24),
        }
    );
    assert_eq!(
        rows[0],
        EncounterRow {
            match_id: "m2".into(),
            map_id: "Ascent".into(),
            start_ms: 2_000,
            with: true,
            self_won: Some(true),
            rounds_won: None,
            rounds_lost: None,
            game_length_ms: None,
            self_agent_id: None,
            other_agent_id: Some("jett".into()),
            self_tier: None,
            other_tier: None,
        }
    );
}

#[test]
fn encounter_counts_equal_encounters_len() {
    let repo = repo_in_memory();
    assert_eq!(repo.encounter_counts("self", "other").unwrap(), (0, 0));
    for (id, start, other_team) in [
        ("m1", 1_000, "Blue"),
        ("m2", 2_000, "Red"),
        ("m3", 3_000, "Red"),
        ("m4", 4_000, "Blue"),
        ("m5", 5_000, "Blue"),
    ] {
        repo.upsert_match(
            &sample_match(id, start, None),
            &[],
            &[
                sample_player("self", "Blue", "Self", "111"),
                sample_player("other", other_team, "Other", "222"),
            ],
        )
        .unwrap();
    }
    // A match the other player did not play never counts.
    repo.upsert_match(
        &sample_match("solo", 6_000, None),
        &[],
        &[sample_player("self", "Blue", "Self", "111")],
    )
    .unwrap();

    let e = repo.encounters("self", "other").unwrap();
    assert_eq!(
        repo.encounter_counts("self", "other").unwrap(),
        (e.with, e.against)
    );
    assert_eq!((e.with, e.against), (3, 2));
    assert_eq!(e.with + e.against, e.matches.len() as u32);
}

#[test]
fn match_rows_returns_teams_players_with_tiers() {
    let repo = repo_in_memory();
    let mut m = sample_match("m1", 1_000, Some("Blue"));
    m.game_length_ms = Some(2_280_000);
    let mut blue = team("Blue", true, Some(13));
    blue.rounds_won_h1 = Some(7);
    let mut a = sample_player("a", "Blue", "A", "1");
    a.competitive_tier = Some(20); // Riot's at-match value beats tier_after 21
    a.damage = Some(3_000);
    let b = sample_player("b", "Blue", "B", "2"); // no match tier: tier_after
    let c = sample_player("c", "Red", "C", "3"); // neither
    repo.upsert_match(&m, &[blue, team("Red", false, Some(11))], &[a, b, c])
        .unwrap();
    repo.upsert_comp_updates("a", &[comp_update("m1", 1_000, 21)])
        .unwrap();
    repo.upsert_comp_updates("b", &[comp_update("m1", 1_000, 15)])
        .unwrap();
    // Another match's RR row must not leak into this one.
    repo.upsert_comp_updates("c", &[comp_update("elsewhere", 900, 9)])
        .unwrap();

    let stored = repo.match_rows("m1").unwrap().expect("stored");
    assert_eq!(stored.info.match_id, "m1");
    assert_eq!(stored.info.queue_id, "competitive");
    assert_eq!(stored.info.winning_team.as_deref(), Some("Blue"));
    assert_eq!(stored.info.game_length_ms, Some(2_280_000));
    assert_eq!(stored.teams.len(), 2);
    let blue = stored.teams.iter().find(|t| t.team_id == "Blue").unwrap();
    assert_eq!(
        (blue.won, blue.rounds_won, blue.rounds_won_h1),
        (true, Some(13), Some(7))
    );
    assert_eq!(stored.players.len(), 3);
    let tier = |p: &str| {
        stored
            .players
            .iter()
            .find(|x| x.puuid == p)
            .unwrap()
            .competitive_tier
    };
    assert_eq!(
        (tier("a"), tier("b"), tier("c")),
        (Some(20), Some(15), None)
    );
    let a = stored.players.iter().find(|p| p.puuid == "a").unwrap();
    assert_eq!(
        (a.agent_id.as_deref(), a.kills, a.score, a.rounds, a.damage),
        (Some("jett"), 20, 4000, Some(24), Some(3_000))
    );
    assert_eq!(a.game_name.as_deref(), Some("A"));

    assert_eq!(repo.match_rows("nope").unwrap(), None);
}
