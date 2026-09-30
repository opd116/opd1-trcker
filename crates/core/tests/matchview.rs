//! Tests for `opd1_core::matchview::build_match_view` (Task 15): the
//! Match Details scoreboard as the UI receives it. Pure input/output: a
//! `StoredMatch` plus who is looking at it and what the live screen hides.

use std::collections::HashMap;

use opd1_core::db::repo::{Match, MatchPlayer, MatchTeam, StoredMatch};
use opd1_core::matchview::{build_match_view, needs_known_name, MatchViewInput};
use opd1_core::model::{DisplayName, MatchDetailsView, MatchResultKind, Perspective};
use opd1_core::names::KnownName;

const ME: &str = "me";
const SPLIT_PATH: &str = "/Game/Maps/Bonsai/Bonsai";
const SPLIT_UUID: &str = "d960549e-485c-e861-8d71-aa9d1aed12a2";

fn info(queue: &str) -> Match {
    Match {
        match_id: "m1".into(),
        map_id: SPLIT_PATH.into(),
        queue_id: queue.into(),
        start_ms: 1_758_970_000_000,
        winning_team: Some("Blue".into()),
        is_ranked: queue == "competitive",
        rounds_played: Some(24),
        game_length_ms: Some(2_280_000),
    }
}

fn team(id: &str, won: bool, rounds_won: i64, h1: Option<i64>) -> MatchTeam {
    MatchTeam {
        team_id: id.into(),
        won,
        rounds_won: Some(rounds_won),
        rounds_won_h1: h1,
    }
}

/// A player with a full stat line (24 rounds, 4800 score, 3120 damage,
/// 10/20/10 shots) and a stored name.
fn player(puuid: &str, team_id: &str, name: &str) -> MatchPlayer {
    MatchPlayer {
        puuid: puuid.into(),
        team_id: team_id.into(),
        party_id: None,
        agent_id: Some(format!("agent-{puuid}")),
        kills: 20,
        deaths: 15,
        assists: 5,
        score: 4800,
        rounds: Some(24),
        hs: 10,
        bs: 20,
        ls: 10,
        game_name: (!name.is_empty()).then(|| name.to_string()),
        tag_line: (!name.is_empty()).then(|| "TAG".to_string()),
        competitive_tier: Some(15),
        damage: Some(3_120),
    }
}

fn scored(mut p: MatchPlayer, score: i64) -> MatchPlayer {
    p.score = score;
    p
}

fn stored(queue: &str, teams: Vec<MatchTeam>, players: Vec<MatchPlayer>) -> StoredMatch {
    StoredMatch {
        info: info(queue),
        teams,
        players,
    }
}

/// A standard 5v5-shaped match: Blue (won 13-11) and Red.
fn duel(players: Vec<MatchPlayer>) -> StoredMatch {
    stored(
        "competitive",
        vec![
            team("Blue", true, 13, Some(8)),
            team("Red", false, 11, Some(4)),
        ],
        players,
    )
}

fn split_lookup(path: &str) -> Option<String> {
    (path == SPLIT_PATH).then(|| SPLIT_UUID.to_string())
}

struct Ctx {
    self_puuid: Option<&'static str>,
    focus: &'static str,
    hidden_now: HashMap<String, Option<String>>,
    remembered: HashMap<String, KnownName>,
}

impl Ctx {
    fn new(self_puuid: Option<&'static str>, focus: &'static str) -> Self {
        Self {
            self_puuid,
            focus,
            hidden_now: HashMap::new(),
            remembered: HashMap::new(),
        }
    }

    fn view(&self, stored: &StoredMatch) -> MatchDetailsView {
        build_match_view(MatchViewInput {
            stored,
            map_uuid: &split_lookup,
            self_puuid: self.self_puuid,
            self_name: self
                .self_puuid
                .map(|_| ("Me".to_string(), "TAG".to_string())),
            focus_puuid: self.focus,
            hidden_now: &self.hidden_now,
            remembered: &self.remembered,
        })
    }
}

fn names(view: &MatchDetailsView) -> HashMap<String, DisplayName> {
    view.teams
        .iter()
        .flat_map(|t| t.players.iter())
        .map(|p| (p.puuid.clone(), p.name.clone()))
        .collect()
}

fn known(name: &str, map: Option<&str>) -> KnownName {
    KnownName {
        name: name.into(),
        tag: "OLD".into(),
        map_id: map.map(str::to_string),
        start_ms: Some(99),
    }
}

fn visible(name: &str, tag: &str) -> DisplayName {
    DisplayName::Visible {
        name: name.into(),
        tag: tag.into(),
    }
}

// ---------------------------------------------------------------------

#[test]
fn needs_known_name_is_exactly_the_rows_drawn_in_the_hidden_style() {
    // One predicate decides both "look up a remembered name" (the shell)
    // and "draw the row in the hidden style" (`build_match_view`): for every
    // row but the user's, they agree.
    let mut no_tag = player("no-tag", "Red", "X");
    no_tag.tag_line = Some(String::new());
    let mut no_name = player("no-name", "Red", "X");
    no_name.game_name = None;
    let mut empty_name = player("empty-name", "Red", "X");
    empty_name.game_name = Some(String::new());
    let mut null_tag = player("null-tag", "Red", "X");
    null_tag.tag_line = None;
    let rows = vec![
        player("shown", "Blue", "A"),
        player("live-hidden", "Blue", "B"),
        no_tag,
        no_name,
        empty_name,
        null_tag,
    ];
    let m = duel(rows.clone());
    let mut ctx = Ctx::new(Some(ME), "shown");
    ctx.hidden_now.insert("live-hidden".into(), None);

    let drawn = names(&ctx.view(&m));
    let expect_hidden = [
        ("shown", false),
        ("live-hidden", true),
        ("no-tag", true),
        ("no-name", true),
        ("empty-name", true),
        ("null-tag", true),
    ];
    for (puuid, hidden) in expect_hidden {
        let p = rows.iter().find(|p| p.puuid == puuid).unwrap();
        assert_eq!(needs_known_name(p, &ctx.hidden_now), hidden, "{puuid}");
        assert_eq!(
            !matches!(drawn[puuid], DisplayName::Visible { .. }),
            hidden,
            "{puuid}: the predicate and the drawn row must agree"
        );
    }
}

#[test]
fn perspective_self_puts_user_team_first() {
    // The user is on Red, the losing team.
    let m = duel(vec![
        player("a", "Blue", "A"),
        player("b", "Blue", "B"),
        player(ME, "Red", "Me"),
        player("c", "Red", "C"),
    ]);
    let v = Ctx::new(Some(ME), "c").view(&m);
    assert_eq!(v.perspective, Perspective::SelfView);
    let ids: Vec<&str> = v.teams.iter().map(|t| t.team_id.as_str()).collect();
    assert_eq!(
        ids,
        ["Red", "Blue"],
        "the user's team first, even if the focus is a teammate"
    );
    assert_eq!(v.teams[0].result, Some(MatchResultKind::Loss));
    assert_eq!(v.teams[0].rounds_won, Some(11));
    assert_eq!(v.teams[1].result, Some(MatchResultKind::Win));
    assert_eq!(v.teams[1].rounds_won, Some(13));
    // Header facts of the match itself.
    assert_eq!(v.match_id, "m1");
    assert_eq!(v.queue_id, "competitive");
    assert_eq!(v.start_ms, 1_758_970_000_000);
    assert_eq!(v.duration_ms, Some(2_280_000));
    assert_eq!(v.map_id, SPLIT_UUID);

    // The user on the other team flips the order.
    let v = Ctx::new(Some("a"), ME).view(&m);
    assert_eq!(v.teams[0].team_id, "Blue");
}

#[test]
fn perspective_focus_when_user_absent() {
    let m = duel(vec![
        player("a", "Blue", "A"),
        player("c", "Red", "C"),
        player("d", "Red", "D"),
    ]);
    // The user has no row in this match: the focus player's team leads.
    let v = Ctx::new(Some(ME), "c").view(&m);
    assert_eq!(v.perspective, Perspective::Focus);
    assert_eq!(v.teams[0].team_id, "Red");
    assert_eq!(v.teams[1].team_id, "Blue");
    // The same when the user is not known at all (never connected).
    let v = Ctx::new(None, "a").view(&m);
    assert_eq!(v.perspective, Perspective::Focus);
    assert_eq!(v.teams[0].team_id, "Blue");
}

#[test]
fn rows_sorted_by_acs_desc_nulls_last() {
    let mut no_rounds = player("z-null", "Blue", "N");
    no_rounds.rounds = None;
    let mut zero_rounds = player("y-zero", "Blue", "Z");
    zero_rounds.rounds = Some(0);
    let m = duel(vec![
        scored(player("low", "Blue", "L"), 2400),
        no_rounds,
        scored(player("high", "Blue", "H"), 7200),
        zero_rounds,
        // Equal ACS (4800 / 24 = 200): ties break by puuid.
        scored(player("tie-b", "Blue", "B"), 4800),
        scored(player("tie-a", "Blue", "A"), 4800),
        player("enemy", "Red", "E"),
    ]);
    let v = Ctx::new(None, "high").view(&m);
    let order: Vec<&str> = v.teams[0]
        .players
        .iter()
        .map(|p| p.puuid.as_str())
        .collect();
    assert_eq!(
        order,
        ["high", "tie-a", "tie-b", "low", "y-zero", "z-null"],
        "ACS descending, rows without ACS last (by puuid)"
    );
    let acs: Vec<Option<f32>> = v.teams[0].players.iter().map(|p| p.acs).collect();
    assert_eq!(
        acs,
        [
            Some(300.0),
            Some(200.0),
            Some(200.0),
            Some(100.0),
            None,
            None
        ]
    );
}

#[test]
fn deathmatch_is_one_block_without_scores() {
    // Every player is their own team; Riot still lists a row per team.
    let mut players = vec![
        scored(player(ME, ME, "Me"), 1200),
        scored(player("p2", "p2", "Two"), 3600),
        scored(player("p3", "p3", "Three"), 2400),
    ];
    for p in &mut players {
        p.rounds = Some(1);
        p.damage = None;
    }
    let m = stored(
        "deathmatch",
        vec![
            team(ME, false, 10, None),
            team("p2", true, 40, None),
            team("p3", false, 20, None),
        ],
        players,
    );
    let v = Ctx::new(Some(ME), ME).view(&m);
    assert_eq!(v.teams.len(), 1);
    let block = &v.teams[0];
    assert_eq!(block.team_id, "");
    assert_eq!(block.result, None);
    assert_eq!(block.rounds_won, None);
    assert_eq!(block.first_half, None);
    let order: Vec<&str> = block.players.iter().map(|p| p.puuid.as_str()).collect();
    assert_eq!(order, ["p2", "p3", ME], "still sorted by ACS");
    assert_eq!(v.queue_id, "deathmatch");
    // The user played, so it is their view even without teams.
    assert_eq!(v.perspective, Perspective::SelfView);
    let json = serde_json::to_value(&v).unwrap();
    assert_eq!(json["teams"][0]["result"], serde_json::Value::Null);
    assert_eq!(json["teams"][0]["roundsWon"], serde_json::Value::Null);
    assert_eq!(json["teams"][0]["firstHalf"], serde_json::Value::Null);
}

#[test]
fn user_played_names_as_stored_and_empty_name_hidden() {
    let m = duel(vec![
        player(ME, "Blue", "StoredMe"),
        player("a", "Blue", "Ally"),
        player("nameless", "Red", ""),
        {
            // No name and no agent: nothing to show but "Hidden Player".
            let mut p = player("ghost", "Red", "");
            p.agent_id = None;
            p
        },
    ]);
    let v = Ctx::new(Some(ME), ME).view(&m);
    let n = names(&v);
    // The user is the user's own (chat-session) name, not the stored one.
    assert_eq!(n[ME], visible("Me", "TAG"));
    assert_eq!(n["a"], visible("Ally", "TAG"));
    assert_eq!(
        n["nameless"],
        DisplayName::HiddenAgent {
            agent_id: "agent-nameless".into()
        }
    );
    assert_eq!(n["ghost"], DisplayName::HiddenPlayer);
}

#[test]
fn user_absent_hides_live_incognito_players() {
    // The user did not play this match; "leaker" is incognito on the live
    // screen now, and this match's stored row has the real name.
    let m = duel(vec![
        player("leaker", "Blue", "RealName"),
        player("plain", "Red", "Plain"),
    ]);
    let mut ctx = Ctx::new(Some(ME), "plain");
    ctx.hidden_now.insert("leaker".into(), None);
    let v = ctx.view(&m);
    assert_eq!(
        names(&v)["leaker"],
        DisplayName::HiddenAgent {
            agent_id: "agent-leaker".into()
        }
    );
    assert_eq!(names(&v)["plain"], visible("Plain", "TAG"));
    assert!(
        !serde_json::to_string(&v).unwrap().contains("RealName"),
        "a live-hidden player's stored name never reaches the wire"
    );
}

#[test]
fn hidden_now_rows_use_display_name_in_every_view() {
    // R93: a row whose puuid is hidden on the live screen follows the same
    // rule as its card, whether or not the user played the match.
    let played = duel(vec![
        player(ME, "Blue", "Me"),
        player("known", "Red", "RealName1"),
        player("unknown", "Red", "RealName2"),
    ]);
    let absent = duel(vec![
        player("known", "Red", "RealName1"),
        player("unknown", "Red", "RealName2"),
        player("other", "Blue", "Other"),
    ]);
    for (m, self_puuid) in [(&played, Some(ME)), (&absent, Some(ME))] {
        let mut ctx = Ctx::new(self_puuid, "known");
        ctx.hidden_now
            .insert("known".into(), Some("ignored".into()));
        ctx.hidden_now.insert("unknown".into(), None);
        ctx.remembered
            .insert("known".into(), known("Seen", Some(SPLIT_PATH)));
        let v = ctx.view(m);
        let n = names(&v);
        assert_eq!(
            n["known"],
            DisplayName::Remembered {
                name: "Seen".into(),
                tag: "OLD".into(),
                from_map_id: Some(SPLIT_UUID.into()),
                from_start_ms: Some(99),
            }
        );
        let json = serde_json::to_value(&v).unwrap();
        let known_row = json["teams"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|t| t["players"].as_array().unwrap())
            .find(|p| p["puuid"] == "known")
            .unwrap();
        assert_eq!(known_row["name"]["kind"], "remembered");
        assert_eq!(known_row["name"]["fromMapId"], SPLIT_UUID);
        assert_eq!(
            n["unknown"],
            DisplayName::HiddenAgent {
                agent_id: "agent-unknown".into()
            },
            "hidden now, no remembered name: the row's own agent"
        );
        let text = serde_json::to_string(&v).unwrap();
        assert!(
            !text.contains("RealName"),
            "neither stored real name may appear: {text}"
        );
    }
}

#[test]
fn user_absent_remembered_only_from_users_own_matches() {
    // The user did not play this match. A row without a stored name is
    // remembered only when the user's OWN finished match taught the name
    // (`remembered` comes from `Repo::known_name`, which never looks at
    // matches the user did not play); otherwise it is the agent.
    let m = duel(vec![
        player("taught", "Blue", ""),
        player("stranger", "Blue", ""),
        player("plain", "Red", "Plain"),
    ]);
    let mut ctx = Ctx::new(Some(ME), "plain");
    ctx.remembered
        .insert("taught".into(), known("FromMyMatch", Some(SPLIT_PATH)));
    let n = names(&ctx.view(&m));
    assert_eq!(
        n["taught"],
        DisplayName::Remembered {
            name: "FromMyMatch".into(),
            tag: "OLD".into(),
            from_map_id: Some(SPLIT_UUID.into()),
            from_start_ms: Some(99),
        }
    );
    assert_eq!(
        n["stranger"],
        DisplayName::HiddenAgent {
            agent_id: "agent-stranger".into()
        }
    );
    // R74: match-details has no incognito flag, so a stored name is shown.
    assert_eq!(n["plain"], visible("Plain", "TAG"));
}

#[test]
fn remembered_source_is_uuid_in_match_view() {
    let m = duel(vec![player("h", "Red", ""), player("a", "Blue", "A")]);
    let mut ctx = Ctx::new(Some(ME), "a");
    ctx.hidden_now.insert("h".into(), None);
    // The DB hands over the stored Riot path (`Repo::known_name`).
    ctx.remembered
        .insert("h".into(), known("Seen", Some(SPLIT_PATH)));
    let v = ctx.view(&m);
    match &names(&v)["h"] {
        DisplayName::Remembered { from_map_id, .. } => {
            assert_eq!(from_map_id.as_deref(), Some(SPLIT_UUID));
        }
        other => panic!("expected a remembered name, got {other:?}"),
    }
    assert!(!serde_json::to_string(&v).unwrap().contains("/Game/"));

    // A map the lookup doesn't know: no source map, never the path.
    let mut unknown_map = Ctx::new(Some(ME), "a");
    unknown_map.hidden_now.insert("h".into(), None);
    unknown_map.remembered.insert(
        "h".into(),
        known("Seen", Some("/Game/Maps/Unknown/Unknown")),
    );
    let v = unknown_map.view(&m);
    match &names(&v)["h"] {
        DisplayName::Remembered { from_map_id, .. } => assert_eq!(*from_map_id, None),
        other => panic!("expected a remembered name, got {other:?}"),
    }
    assert!(!serde_json::to_string(&v).unwrap().contains("/Game/"));
}

#[test]
fn first_half_and_result_per_team() {
    let m = duel(vec![player(ME, "Blue", "Me"), player("e", "Red", "E")]);
    let v = Ctx::new(Some(ME), ME).view(&m);
    let blue = &v.teams[0];
    assert_eq!(
        (
            blue.team_id.as_str(),
            blue.result,
            blue.rounds_won,
            blue.first_half
        ),
        ("Blue", Some(MatchResultKind::Win), Some(13), Some(8))
    );
    let red = &v.teams[1];
    assert_eq!(
        (
            red.team_id.as_str(),
            red.result,
            red.rounds_won,
            red.first_half
        ),
        ("Red", Some(MatchResultKind::Loss), Some(11), Some(4))
    );

    // A drawn match (no winning team): both sides draw; no half data in
    // another queue.
    let mut draw = duel(vec![player(ME, "Blue", "Me"), player("e", "Red", "E")]);
    draw.info.winning_team = None;
    draw.teams[0].rounds_won_h1 = None;
    draw.teams[1].rounds_won_h1 = None;
    let v = Ctx::new(Some(ME), ME).view(&draw);
    assert_eq!(v.teams[0].result, Some(MatchResultKind::Draw));
    assert_eq!(v.teams[1].result, Some(MatchResultKind::Draw));
    assert_eq!(v.teams[0].first_half, None);

    // No team rows at all (Riot sent `teams: null`): the players still
    // split by team, with nothing known about the result.
    let no_rows = stored(
        "competitive",
        vec![],
        vec![player(ME, "Blue", "Me"), player("e", "Red", "E")],
    );
    let v = Ctx::new(Some(ME), ME).view(&no_rows);
    assert_eq!(v.teams.len(), 2);
    assert_eq!(v.teams[0].result, None);
    assert_eq!(v.teams[0].rounds_won, None);
}

#[test]
fn acs_adr_hs_computed_per_round() {
    let mut no_damage = player("nd", "Blue", "ND");
    no_damage.damage = None;
    let mut no_shots = player("ns", "Blue", "NS");
    (no_shots.hs, no_shots.bs, no_shots.ls) = (0, 0, 0);
    let mut no_stats = player("nostats", "Blue", "NoStats");
    (no_stats.rounds, no_stats.damage) = (None, None);
    (no_stats.hs, no_stats.bs, no_stats.ls) = (0, 0, 0);
    no_stats.competitive_tier = Some(2); // below 3: unranked
    let mut zero_rounds = player("zero-rounds", "Blue", "Zero");
    zero_rounds.rounds = Some(0);
    let m = duel(vec![
        player("full", "Blue", "Full"),
        no_damage,
        no_shots,
        no_stats,
        zero_rounds,
        player("e", "Red", "E"),
    ]);
    let v = Ctx::new(None, "full").view(&m);
    let row = |puuid: &str| {
        v.teams
            .iter()
            .flat_map(|t| t.players.iter())
            .find(|p| p.puuid == puuid)
            .unwrap()
    };
    let full = row("full");
    assert_eq!(full.acs, Some(200.0), "4800 score / 24 rounds");
    assert_eq!(full.adr, Some(130.0), "3120 damage / 24 rounds");
    assert_eq!(full.hs_pct, Some(0.25), "10 / (10 + 20 + 10)");
    assert_eq!(
        (full.kills, full.deaths, full.assists),
        (Some(20), Some(15), Some(5))
    );
    assert_eq!(full.tier, Some(15));
    assert_eq!(full.agent_id.as_deref(), Some("agent-full"));

    assert_eq!(row("nd").adr, None, "no damage stored: no ADR");
    assert_eq!(row("nd").acs, Some(200.0));
    assert_eq!(row("ns").hs_pct, None, "no shot landed: no HS%");
    let none = row("nostats");
    assert_eq!((none.acs, none.adr, none.hs_pct), (None, None, None));
    // R124: kills, deaths and assists are unknown too (whatever was stored),
    // never a counted 0 — for a missing round count and for a zero one.
    assert_eq!(
        (none.kills, none.deaths, none.assists),
        (None, None, None),
        "no stats row: K/D/A are null"
    );
    let zero = row("zero-rounds");
    assert_eq!(
        (zero.kills, zero.deaths, zero.assists, zero.acs),
        (None, None, None, None),
        "a zero round count is no stats row either"
    );
    assert_eq!(none.tier, None, "tiers below 3 are null");
}

#[test]
fn self_and_focus_flags() {
    let m = duel(vec![
        player(ME, "Blue", "Me"),
        player("friend", "Blue", "Friend"),
        player("e", "Red", "E"),
    ]);
    let v = Ctx::new(Some(ME), "friend").view(&m);
    let flags: HashMap<&str, (bool, bool)> = v
        .teams
        .iter()
        .flat_map(|t| t.players.iter())
        .map(|p| (p.puuid.as_str(), (p.is_self, p.is_focus)))
        .collect();
    assert_eq!(flags[ME], (true, false));
    assert_eq!(flags["friend"], (false, true));
    assert_eq!(flags["e"], (false, false));

    // Looking at yourself: one row is both.
    let v = Ctx::new(Some(ME), ME).view(&m);
    let me = v
        .teams
        .iter()
        .flat_map(|t| t.players.iter())
        .find(|p| p.puuid == ME)
        .unwrap();
    assert!(me.is_self && me.is_focus);

    // The wire spells `perspective` "self" / "focus" and the flags in
    // camelCase.
    let json = serde_json::to_value(&v).unwrap();
    assert_eq!(json["perspective"], "self");
    let first = &json["teams"][0]["players"][0];
    assert!(first.get("isSelf").is_some() && first.get("isFocus").is_some());
    assert!(first.get("hsPct").is_some());
    let v = Ctx::new(None, "friend").view(&m);
    assert_eq!(serde_json::to_value(&v).unwrap()["perspective"], "focus");
}
