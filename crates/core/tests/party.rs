//! Tests for `opd1_core::party::detect_parties` — CLAUDE.md's "Party
//! detection": likely (most recent match shared, same team) vs confirmed
//! (last two shared, or the same `partyId` in the shared match), linked
//! transitively, the user's own party never marked.

use std::collections::{HashMap, HashSet};

use opd1_core::model::{PartyMark, Team};
use opd1_core::party::{detect_parties, SharedRef};

fn r(match_id: &str, team: Option<&str>, party: Option<&str>) -> Option<SharedRef> {
    Some(SharedRef {
        match_id: match_id.to_string(),
        team_id: team.map(str::to_string),
        party_id: party.map(str::to_string),
    })
}

/// A shorthand for a player's last two matches (newest first).
fn recent(entries: &[(&str, [Option<SharedRef>; 2])]) -> HashMap<String, [Option<SharedRef>; 2]> {
    entries
        .iter()
        .map(|(p, refs)| (p.to_string(), refs.clone()))
        .collect()
}

fn players(ids: &[&str], team: Team) -> Vec<(String, Team)> {
    ids.iter().map(|p| (p.to_string(), team)).collect()
}

fn no_party() -> HashSet<String> {
    HashSet::new()
}

fn likely(group: u8) -> PartyMark {
    PartyMark {
        group,
        confirmed: false,
    }
}

fn confirmed(group: u8) -> PartyMark {
    PartyMark {
        group,
        confirmed: true,
    }
}

#[test]
fn party_one_shared_is_likely() {
    let recent = recent(&[
        (
            "a",
            [r("m1", Some("Blue"), None), r("x1", Some("Red"), None)],
        ),
        (
            "b",
            [r("m1", Some("Blue"), None), r("y1", Some("Blue"), None)],
        ),
        (
            "c",
            [r("m9", Some("Blue"), None), r("x1", Some("Red"), None)],
        ),
    ]);
    let marks = detect_parties(
        &players(&["a", "b", "c"], Team::Enemy),
        &recent,
        &no_party(),
    );
    assert_eq!(marks.get("a"), Some(&likely(0)));
    assert_eq!(marks.get("b"), Some(&likely(0)));
    assert_eq!(
        marks.get("c"),
        None,
        "sharing only an older match is no link"
    );
    assert_eq!(marks.len(), 2);
}

#[test]
fn party_two_shared_is_confirmed() {
    let recent = recent(&[
        (
            "a",
            [r("m2", Some("Red"), None), r("m1", Some("Blue"), None)],
        ),
        (
            "b",
            [r("m2", Some("Red"), None), r("m1", Some("Blue"), None)],
        ),
    ]);
    let marks = detect_parties(&players(&["a", "b"], Team::Ally), &recent, &no_party());
    assert_eq!(marks.get("a"), Some(&confirmed(0)));
    assert_eq!(marks.get("b"), Some(&confirmed(0)));
}

#[test]
fn party_two_shared_needs_the_same_team_in_both() {
    // Both matches shared, but the older one on opposite teams: only the
    // most recent one counts as together, so it stays likely.
    let recent = recent(&[
        (
            "a",
            [r("m2", Some("Red"), None), r("m1", Some("Blue"), None)],
        ),
        (
            "b",
            [r("m2", Some("Red"), None), r("m1", Some("Red"), None)],
        ),
    ]);
    let marks = detect_parties(&players(&["a", "b"], Team::Ally), &recent, &no_party());
    assert_eq!(marks.get("a"), Some(&likely(0)));
    assert_eq!(marks.get("b"), Some(&likely(0)));
}

#[test]
fn party_same_party_id_one_game_is_confirmed() {
    let recent = recent(&[
        ("a", [r("m1", Some("Blue"), Some("party-7")), None]),
        (
            "b",
            [
                r("m1", Some("Blue"), Some("party-7")),
                r("zz", Some("Red"), None),
            ],
        ),
    ]);
    let marks = detect_parties(&players(&["a", "b"], Team::Enemy), &recent, &no_party());
    assert_eq!(marks.get("a"), Some(&confirmed(0)));
    assert_eq!(marks.get("b"), Some(&confirmed(0)));
}

#[test]
fn party_different_party_id_one_game_stays_likely() {
    let recent = recent(&[
        ("a", [r("m1", Some("Blue"), Some("party-7")), None]),
        ("b", [r("m1", Some("Blue"), Some("party-8")), None]),
        // An empty party id is no evidence either.
        ("c", [r("m5", Some("Red"), Some("")), None]),
        ("d", [r("m5", Some("Red"), Some("")), None]),
    ]);
    let marks = detect_parties(
        &players(&["a", "b", "c", "d"], Team::Enemy),
        &recent,
        &no_party(),
    );
    assert_eq!(marks.get("a"), Some(&likely(0)));
    assert_eq!(marks.get("b"), Some(&likely(0)));
    assert_eq!(marks.get("c"), Some(&likely(1)));
    assert_eq!(marks.get("d"), Some(&likely(1)));
}

#[test]
fn party_transitive_group() {
    // a–b confirmed (two shared), b–c likely (b's newest also shared with
    // c): one group a–b–c, likely because one of its links is. d–e are a
    // second group, colored after the first in roster order.
    let recent = recent(&[
        ("d", [r("m7", Some("Red"), Some("p")), None]),
        (
            "a",
            [r("m2", Some("Blue"), None), r("m1", Some("Blue"), None)],
        ),
        (
            "b",
            [r("m2", Some("Blue"), None), r("m1", Some("Blue"), None)],
        ),
        (
            "c",
            [r("m2", Some("Blue"), None), r("m0", Some("Red"), None)],
        ),
        ("e", [r("m7", Some("Red"), Some("p")), None]),
    ]);
    let marks = detect_parties(
        &players(&["a", "b", "c", "d", "e"], Team::Enemy),
        &recent,
        &no_party(),
    );
    for p in ["a", "b", "c"] {
        assert_eq!(marks.get(p), Some(&likely(0)), "{p}");
    }
    for p in ["d", "e"] {
        assert_eq!(marks.get(p), Some(&confirmed(1)), "{p}");
    }
}

#[test]
fn party_deathmatch_no_team() {
    // Deathmatch has no teams (team_id None): the same match is enough.
    let recent = recent(&[
        ("a", [r("dm1", None, None), r("m1", Some("Blue"), None)]),
        ("b", [r("dm1", None, None), r("m1", Some("Blue"), None)]),
        ("c", [r("dm1", None, None), r("m3", Some("Red"), None)]),
    ]);
    let marks = detect_parties(&players(&["a", "b", "c"], Team::Ally), &recent, &no_party());
    // a–b share both games -> confirmed; c only the deathmatch -> likely,
    // which makes the whole group likely.
    for p in ["a", "b", "c"] {
        assert_eq!(marks.get(p), Some(&likely(0)), "{p}");
    }

    let only_ab = recent_of(&["a", "b"], &recent);
    let marks = detect_parties(&players(&["a", "b"], Team::Ally), &only_ab, &no_party());
    assert_eq!(marks.get("a"), Some(&confirmed(0)));

    // A team match and a deathmatch are never "the same team".
    let mixed = self::recent(&[
        ("a", [r("m1", None, None), None]),
        ("b", [r("m1", Some("Blue"), None), None]),
    ]);
    let marks = detect_parties(&players(&["a", "b"], Team::Ally), &mixed, &no_party());
    assert!(marks.is_empty());
}

fn recent_of(
    ids: &[&str],
    all: &HashMap<String, [Option<SharedRef>; 2]>,
) -> HashMap<String, [Option<SharedRef>; 2]> {
    ids.iter()
        .map(|p| (p.to_string(), all[*p].clone()))
        .collect()
}

#[test]
fn party_different_team_not_linked() {
    let recent = recent(&[
        (
            "a",
            [
                r("m1", Some("Blue"), Some("p")),
                r("m0", Some("Blue"), None),
            ],
        ),
        (
            "b",
            [r("m1", Some("Red"), Some("p")), r("m0", Some("Blue"), None)],
        ),
    ]);
    let marks = detect_parties(&players(&["a", "b"], Team::Enemy), &recent, &no_party());
    assert!(marks.is_empty(), "opponents in the shared match: {marks:?}");
}

#[test]
fn party_opposite_live_teams_not_linked() {
    // Together last game, on opposite sides now: not a party right now.
    let recent = recent(&[
        ("a", [r("m1", Some("Blue"), Some("p")), None]),
        ("b", [r("m1", Some("Blue"), Some("p")), None]),
    ]);
    let roster = vec![
        ("a".to_string(), Team::Ally),
        ("b".to_string(), Team::Enemy),
    ];
    assert!(detect_parties(&roster, &recent, &no_party()).is_empty());
}

#[test]
fn party_unknown_history_is_no_link() {
    // A player whose recent matches aren't known (not synced, or details
    // missing) links with nobody.
    let recent = recent(&[
        ("a", [r("m1", Some("Blue"), None), None]),
        ("b", [None, r("m1", Some("Blue"), None)]),
    ]);
    let marks = detect_parties(&players(&["a", "b", "c"], Team::Ally), &recent, &no_party());
    assert!(marks.is_empty());
}

#[test]
fn self_party_never_marked() {
    // The user ("me") and their party member ("duo") share their last two
    // games with "x": x must not be linked through them, and neither of
    // them gets a stripe.
    let recent = recent(&[
        (
            "me",
            [
                r("m2", Some("Blue"), Some("p")),
                r("m1", Some("Red"), Some("p")),
            ],
        ),
        (
            "duo",
            [
                r("m2", Some("Blue"), Some("p")),
                r("m1", Some("Red"), Some("p")),
            ],
        ),
        (
            "x",
            [
                r("m2", Some("Blue"), Some("p")),
                r("m1", Some("Red"), Some("p")),
            ],
        ),
        ("y", [r("m4", Some("Red"), None), None]),
        ("z", [r("m4", Some("Red"), None), None]),
    ]);
    let self_party: HashSet<String> = ["me".to_string(), "duo".to_string()].into();
    let marks = detect_parties(
        &players(&["me", "duo", "x", "y", "z"], Team::Ally),
        &recent,
        &self_party,
    );
    assert_eq!(marks.get("me"), None);
    assert_eq!(marks.get("duo"), None);
    assert_eq!(marks.get("x"), None, "no link through the user's party");
    // Colors restart at 0 for the first group actually marked.
    assert_eq!(marks.get("y"), Some(&likely(0)));
    assert_eq!(marks.get("z"), Some(&likely(0)));
}
