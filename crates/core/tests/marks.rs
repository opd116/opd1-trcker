//! R189: migration 4's tables through `opd1_core::db::marks` — saved
//! players, notes, smurf flags, the Saved screen's rows and the ASCII
//! gallery.

use std::path::Path;

use opd1_core::db::marks::{saved_player, NOTE_MAX_CHARS};
use opd1_core::db::open_db;
use opd1_core::db::repo::{CompUpdateRow, Match, MatchPlayer, MatchTeam, Player, Repo};
use opd1_core::model::{DisplayName, RememberedSource};
use opd1_core::wire::{PlayerMarks, SharedRecord};

const ME: &str = "me";
const DUO: &str = "duo";
const ASCENT_PATH: &str = "/Game/Maps/Ascent/Ascent";
const ASCENT_UUID: &str = "7eaecc1b-4337-bbf6-6ab9-04b8f06b3319";

fn repo() -> Repo {
    Repo::new(open_db(Path::new(":memory:")).unwrap())
}

fn ascent(path: &str) -> Option<String> {
    (path == ASCENT_PATH).then(|| ASCENT_UUID.to_string())
}

fn player(
    puuid: &str,
    team: &str,
    agent: &str,
    name: Option<&str>,
    tier: Option<i64>,
) -> MatchPlayer {
    MatchPlayer {
        puuid: puuid.into(),
        team_id: team.into(),
        party_id: None,
        agent_id: Some(agent.into()),
        kills: 10,
        deaths: 10,
        assists: 2,
        score: 3000,
        rounds: Some(20),
        hs: 5,
        bs: 10,
        ls: 5,
        game_name: name.map(str::to_string),
        tag_line: name.map(|_| "TAG".to_string()),
        competitive_tier: tier,
        damage: None,
    }
}

/// A two-team match `Blue` vs `Red`; `winner` `None` is a draw.
fn played(
    r: &Repo,
    id: &str,
    queue: &str,
    start_ms: i64,
    winner: Option<&str>,
    players: &[MatchPlayer],
) {
    let team = |t: &str| MatchTeam {
        team_id: t.into(),
        won: winner == Some(t),
        rounds_won: Some(if winner == Some(t) { 13 } else { 7 }),
        rounds_won_h1: None,
    };
    r.upsert_match(
        &Match {
            match_id: id.into(),
            map_id: ASCENT_PATH.into(),
            queue_id: queue.into(),
            start_ms,
            winning_team: winner.map(str::to_string),
            is_ranked: queue == "competitive",
            rounds_played: Some(20),
            game_length_ms: None,
        },
        &[team("Blue"), team("Red")],
        players,
    )
    .unwrap();
}

#[test]
fn marks_start_empty_and_follow_every_write() {
    let r = repo();
    assert_eq!(r.player_marks().unwrap(), PlayerMarks::default());

    r.set_saved("a", true, 100).unwrap();
    r.set_saved("b", true, 300).unwrap();
    r.set_saved("c", true, 200).unwrap();
    // Saving again keeps the first time: the order does not jump.
    r.set_saved("a", true, 999).unwrap();
    r.set_note("b", "dry-peeks A long", 1).unwrap();
    r.set_note(ME, "my own notes", 1).unwrap();
    r.dismiss_smurf("x", 5).unwrap();
    r.dismiss_smurf("y", 9).unwrap();

    let m = r.player_marks().unwrap();
    assert_eq!(m.saved, ["b", "c", "a"], "newest saved first");
    assert_eq!(m.notes.len(), 2);
    assert_eq!(m.notes["b"], "dry-peeks A long");
    assert_eq!(m.notes[ME], "my own notes");
    assert_eq!(m.not_smurf, ["y", "x"]);
    assert!(r.is_saved("a").unwrap());
    assert_eq!(r.note("b").unwrap().as_deref(), Some("dry-peeks A long"));

    r.set_saved("c", false, 0).unwrap();
    r.set_note("b", "  \n ", 2).unwrap();
    let m = r.player_marks().unwrap();
    assert_eq!(m.saved, ["b", "a"]);
    assert!(!m.notes.contains_key("b"), "a blank note deletes the row");
    assert!(!r.is_saved("c").unwrap());
    assert_eq!(r.note("b").unwrap(), None);
}

#[test]
fn a_note_is_replaced_and_capped() {
    let r = repo();
    r.set_note("p", "first", 1).unwrap();
    r.set_note("p", "second", 2).unwrap();
    assert_eq!(r.note("p").unwrap().as_deref(), Some("second"));
    r.set_note("p", &"x".repeat(NOTE_MAX_CHARS * 2), 3).unwrap();
    assert_eq!(r.note("p").unwrap().unwrap().len(), NOTE_MAX_CHARS);
}

#[test]
fn saved_rows_read_name_agent_tier_records_and_last_seen() {
    let r = repo();
    // Competitive: a win and a loss with the duo, a draw and a win against.
    played(
        &r,
        "c1",
        "competitive",
        1_000,
        Some("Blue"),
        &[
            player(ME, "Blue", "a-me", Some("Me"), Some(12)),
            player(DUO, "Blue", "a-old", Some("Duo"), Some(14)),
        ],
    );
    played(
        &r,
        "c2",
        "competitive",
        2_000,
        Some("Red"),
        &[
            player(ME, "Blue", "a-me", Some("Me"), Some(12)),
            player(DUO, "Blue", "a-old", Some("Duo"), Some(15)),
        ],
    );
    played(
        &r,
        "c3",
        "competitive",
        3_000,
        None,
        &[
            player(ME, "Blue", "a-me", Some("Me"), Some(12)),
            player(DUO, "Red", "a-old", Some("Duo"), Some(15)),
        ],
    );
    played(
        &r,
        "c4",
        "competitive",
        4_000,
        Some("Blue"),
        &[
            player(ME, "Blue", "a-me", Some("Me"), Some(12)),
            player(DUO, "Red", "a-old", Some("Duo"), Some(16)),
        ],
    );
    // A newer unrated together: no record, but it is "last seen".
    played(
        &r,
        "u1",
        "unrated",
        5_000,
        Some("Blue"),
        &[
            player(ME, "Blue", "a-me", Some("Me"), None),
            player(DUO, "Blue", "a-mid", Some("Duo"), None),
        ],
    );
    // The duo's own newest match, without the user: the agent comes from it.
    played(
        &r,
        "o1",
        "swiftplay",
        6_000,
        Some("Blue"),
        &[player(DUO, "Blue", "a-new", Some("Duo"), None)],
    );
    r.set_saved(DUO, true, 77).unwrap();
    r.set_note(DUO, "plays op", 1).unwrap();

    let rows = r.saved_rows(Some(ME)).unwrap();
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!(row.agent_id.as_deref(), Some("a-new"));
    assert_eq!(row.tier, Some(16), "the newest ranked match's tier");
    assert_eq!(
        row.with,
        SharedRecord {
            games: 2,
            wins: 1,
            losses: 1
        }
    );
    assert_eq!(
        row.against,
        SharedRecord {
            games: 2,
            wins: 1,
            losses: 0
        },
        "a draw is a game only"
    );
    assert_eq!(row.last_seen_ms, Some(5_000), "any queue counts as seen");
    assert_eq!(row.note, "plays op");
    assert_eq!(row.saved_ms, 77);

    let wire = saved_player(row.clone(), None, &ascent);
    assert_eq!(
        wire.name,
        DisplayName::Visible {
            name: "Duo".into(),
            tag: "TAG".into()
        }
    );
    assert_eq!(wire.last_seen_map_id.as_deref(), Some(ASCENT_UUID));

    // A newer RR row beats the stored match tier; an older one does not.
    r.upsert_comp_updates(DUO, &[rr("x", 9_000, 18)]).unwrap();
    assert_eq!(r.saved_rows(Some(ME)).unwrap()[0].tier, Some(18));
    r.upsert_comp_updates(DUO, &[rr("x", 500, 18)]).unwrap();
    assert_eq!(r.saved_rows(Some(ME)).unwrap()[0].tier, Some(16));
}

fn rr(id: &str, start_ms: i64, tier: i64) -> CompUpdateRow {
    CompUpdateRow {
        match_id: id.into(),
        season_id: "s".into(),
        tier_after: tier,
        tier_before: Some(tier),
        rr_after: 50,
        rr_earned: 10,
        map_id: None,
        start_ms,
    }
}

#[test]
fn a_saved_player_the_db_knows_nothing_about_is_hidden_with_empty_records() {
    let r = repo();
    r.set_saved("ghost", true, 1).unwrap();
    let rows = r.saved_rows(Some(ME)).unwrap();
    let wire = saved_player(rows[0].clone(), None, &ascent);
    assert_eq!(wire.name, DisplayName::HiddenPlayer);
    assert_eq!((wire.agent_id, wire.tier), (None, None));
    assert_eq!(wire.with, SharedRecord::default());
    assert_eq!(wire.against, SharedRecord::default());
    assert_eq!((wire.last_seen_ms, wire.last_seen_map_id), (None, None));
    assert_eq!(wire.note, "");
    // Before the app ever knew the user: no record, no last-seen, still a row.
    assert_eq!(r.saved_rows(None).unwrap().len(), 1);
}

#[test]
fn a_saved_player_hidden_live_follows_r18_and_r145() {
    let r = repo();
    played(
        &r,
        "c1",
        "competitive",
        1_000,
        Some("Blue"),
        &[
            player(ME, "Blue", "a", Some("Me"), None),
            player(DUO, "Red", "a", Some("Duo"), None),
        ],
    );
    r.set_saved(DUO, true, 1).unwrap();
    r.set_saved("nameless", true, 0).unwrap();
    r.upsert_player(&Player {
        puuid: "nameless".into(),
        game_name: String::new(),
        tag_line: String::new(),
        account_level: 1,
        card_id: "c".into(),
        updated_ms: 1,
    })
    .unwrap();
    let rows = r.saved_rows(Some(ME)).unwrap();

    // Known name, incognito now: the remembered style, with its source map.
    let duo = saved_player(rows[0].clone(), Some(Some("agent-jett")), &ascent);
    assert_eq!(
        duo.name,
        DisplayName::Remembered {
            name: "Duo".into(),
            tag: "TAG".into(),
            source: RememberedSource::OwnMatch,
            from_map_id: Some(ASCENT_UUID.into()),
            from_start_ms: Some(1_000),
        }
    );
    // No known name: the locked agent, else Hidden Player (R99).
    let agent = saved_player(rows[1].clone(), Some(Some("agent-jett")), &ascent);
    assert_eq!(
        agent.name,
        DisplayName::HiddenAgent {
            agent_id: "agent-jett".into()
        }
    );
    let before_lock = saved_player(rows[1].clone(), Some(None), &ascent);
    assert_eq!(before_lock.name, DisplayName::HiddenPlayer);
    assert!(!serde_json::to_string(&before_lock)
        .unwrap()
        .contains("/Game/"));
}

#[test]
fn saving_yourself_counts_no_record() {
    let r = repo();
    played(
        &r,
        "c1",
        "competitive",
        1_000,
        Some("Blue"),
        &[player(ME, "Blue", "a", Some("Me"), None)],
    );
    r.set_saved(ME, true, 1).unwrap();
    let row = &r.saved_rows(Some(ME)).unwrap()[0];
    assert_eq!(row.with, SharedRecord::default());
    assert_eq!(row.last_seen_ms, None);
}

#[test]
fn the_ascii_gallery_lists_newest_first_and_deletes_by_id() {
    let r = repo();
    assert!(r.ascii_list().unwrap().is_empty());
    let a = r.ascii_save("first", "#\n#", 100).unwrap();
    let b = r.ascii_save("second", "@", 200).unwrap();
    let c = r.ascii_save("same time", "*", 200).unwrap();
    assert!(a.id < b.id && b.id < c.id);
    let ids: Vec<i64> = r.ascii_list().unwrap().iter().map(|x| x.id).collect();
    assert_eq!(ids, [c.id, b.id, a.id]);
    assert_eq!(r.ascii_list().unwrap()[2], a);

    assert!(r.ascii_delete(b.id).unwrap());
    assert!(!r.ascii_delete(b.id).unwrap(), "already gone");
    // AUTOINCREMENT: a deleted id is never handed out again.
    r.ascii_delete(c.id).unwrap();
    let d = r.ascii_save("later", "+", 300).unwrap();
    assert!(d.id > c.id);
    assert_eq!(r.ascii_list().unwrap().len(), 2);
}
