//! The UI wire contract, pinned (Task 15): the JSON the backend sends for
//! the live state, Player Details and Match Details, compared with golden
//! files in `tests/fixtures/contract/`. The UI's TypeScript types
//! (`ui/src/lib/types.ts`, spec §7.1) are checked against the same files, so
//! a renamed or dropped key fails on one side or the other.
//!
//! Each test builds the values with the real builders (`build_card`,
//! `match_summary`, `build_match_view`), serializes them with
//! `serde_json::to_value` and compares with the golden. After an intended
//! change, `UPDATE_GOLDEN=1 cargo test -p opd1-core --test contract`
//! rewrites the files — and the diff is what gets reviewed against spec §7.1.
//!
//! An absent key stands for "still loading" (R23) and `null` for "known
//! missing"; both appear below, on purpose. Floats are dyadic fractions
//! (0.25, 1.5 ...) so the goldens read cleanly.

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;

use serde_json::{json, Value};

use opd1_core::db::repo::{
    CompMatchRow, CompUpdateRow, Match, MatchPlayer, MatchPlayerRow, MatchTeam, StoredMatch,
    StoredTier,
};
use opd1_core::matchview::{build_match_view, MatchViewInput};
use opd1_core::model::{
    build_card, match_summary, BuildCardInput, CurrentRank, DisplayName, EncounterCount,
    EncounterDetailRow, Field, Identity, LiveState, Loadout, MatchResultKind, MmrInput, PartyMark,
    PlayerCard, PlayerDetails, RememberedSource, StreakMark, Team, TeammateSummary, WeaponSkin,
};
use opd1_core::mystats::{self, MyStatsInput};
use opd1_core::names::KnownName;
use opd1_core::presence::Stage;
use opd1_core::stats::{Act, SeasonInfo};

const CURRENT_ACT_ID: &str = "act-current";
const PREV_ACT_ID: &str = "act-prev";
const SPLIT_PATH: &str = "/Game/Maps/Bonsai/Bonsai";
const SPLIT_UUID: &str = "d960549e-485c-e861-8d71-aa9d1aed12a2";
const ASCENT_UUID: &str = "7eaecc1b-4337-bbf6-6ab9-04b8f06b3319";
const TIER_PREMIUM: &str = "60bca009-4182-7998-dee7-b8a2558dc369";
/// A remembered name's source match: 2025-09-27 10:46:40 UTC.
const SEEN_MS: i64 = 1_758_970_000_000;

// ---------------------------------------------------------------------
// golden files
// ---------------------------------------------------------------------

fn golden_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/contract")
        .join(format!("{name}.json"))
}

/// Compares `actual` with `tests/fixtures/contract/<name>.json`, or
/// rewrites that file when `UPDATE_GOLDEN` is set.
fn check_golden(name: &str, actual: &Value) {
    let path = golden_path(name);
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut text = serde_json::to_string_pretty(actual).unwrap();
        text.push('\n');
        std::fs::write(&path, text).unwrap();
        return;
    }
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "missing golden {}: {e}\nrun `UPDATE_GOLDEN=1 cargo test -p opd1-core --test contract`",
            path.display()
        )
    });
    let expected: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        actual,
        &expected,
        "{name} differs from its golden; after an intended change run \
         `UPDATE_GOLDEN=1 cargo test -p opd1-core --test contract` and review the diff\n\
         actual:\n{}",
        serde_json::to_string_pretty(actual).unwrap()
    );
}

// ---------------------------------------------------------------------
// cards
// ---------------------------------------------------------------------

fn acts() -> &'static [Act] {
    let acts = vec![
        Act {
            id: PREV_ACT_ID.into(),
            name: "ACT IV".into(),
            episode_name: "V26".into(),
            start_ms: 1_000,
            end_ms: 2_000,
        },
        Act {
            id: CURRENT_ACT_ID.into(),
            name: "ACT V".into(),
            episode_name: "V26".into(),
            start_ms: 2_000,
            end_ms: 3_000,
        },
    ];
    Box::leak(acts.into_boxed_slice())
}

fn identity(puuid: &str, incognito: bool, locked: bool, name: (&str, &str)) -> Identity {
    Identity {
        puuid: puuid.into(),
        incognito,
        hide_account_level: false,
        account_level: 187,
        card_id: "card-uuid".into(),
        character_id: "agent-jett".into(),
        character_selection_state: if locked { "locked" } else { "" }.into(),
        game_name: name.0.into(),
        tag_line: name.1.into(),
    }
}

/// Two seasons: the previous act ended at tier 16 with wins up to tier 17
/// (the peak), the current act is tier 15, 30 wins in 60 games.
fn mmr() -> Field<MmrInput> {
    Field::Value(MmrInput {
        seasons: HashMap::from([
            (
                PREV_ACT_ID.to_string(),
                SeasonInfo {
                    competitive_tier: 16,
                    wins_by_tier: HashMap::from([(16, 4), (17, 1)]),
                    number_of_wins: 20,
                    number_of_games: 40,
                },
            ),
            (
                CURRENT_ACT_ID.to_string(),
                SeasonInfo {
                    competitive_tier: 15,
                    wins_by_tier: HashMap::from([(15, 3)]),
                    number_of_wins: 30,
                    number_of_games: 60,
                },
            ),
        ]),
        current_rank: Some(CurrentRank { tier: 15, rr: 42 }),
    })
}

/// Three competitive matches: W, L, W (newest first once sorted); 20
/// kills / 10 deaths each and 25 / 50 / 25 shots — K/D 2.0, HS 0.25.
fn last5() -> Field<Vec<MatchPlayerRow>> {
    let row = |id: &str, start_ms: i64, team: &str, winner: &str| MatchPlayerRow {
        match_id: id.into(),
        map_id: "map".into(),
        start_ms,
        winning_team: Some(winner.into()),
        team_id: team.into(),
        party_id: None,
        agent_id: Some("agent-jett".into()),
        kills: 20,
        deaths: 10,
        assists: 4,
        score: 4000,
        rounds: Some(24),
        hs: 25,
        bs: 50,
        ls: 25,
        game_name: None,
        tag_line: None,
    };
    Field::Value(vec![
        row("m1", 2_100, "Blue", "Blue"),
        row("m2", 2_200, "Blue", "Red"),
        row("m3", 2_300, "Blue", "Blue"),
    ])
}

fn comp_updates() -> Field<Vec<CompUpdateRow>> {
    let row = |id: &str, start_ms: i64, rr: i64| CompUpdateRow {
        match_id: id.into(),
        season_id: CURRENT_ACT_ID.into(),
        tier_after: 15,
        tier_before: Some(15),
        rr_after: 42,
        rr_earned: rr,
        map_id: None,
        start_ms,
    };
    Field::Value(vec![
        row("m1", 2_100, 21),
        row("m2", 2_200, -18),
        row("m3", 2_300, 19),
    ])
}

/// A card as `build_card` makes it, with everything the shell would feed it.
fn card(identity: Identity, team: Team) -> BuildCardInput<'static> {
    BuildCardInput {
        identity,
        mmr: mmr(),
        comp_updates: comp_updates(),
        last5: last5(),
        ordered_acts: acts(),
        current_act_id: CURRENT_ACT_ID,
        now_ms: 2_500,
        team,
        is_self: false,
        is_self_party: false,
        remembered: None,
        party: None,
        encounters: None,
        stored_tier: None,
        live_tier: None,
    }
}

fn split_lookup(path: &str) -> Option<String> {
    (path == SPLIT_PATH).then(|| SPLIT_UUID.to_string())
}

/// A remembered name, its source map converted the way the shell does
/// (`KnownName::with_map_uuid`, R84/R95): a uuid or nothing, never a path.
fn remembered(
    source: RememberedSource,
    map: Option<&str>,
    start_ms: Option<i64>,
) -> Option<KnownName> {
    Some(
        KnownName {
            name: "Rival".into(),
            tag: "ENEMY".into(),
            map_id: map.map(str::to_string),
            start_ms,
            source,
        }
        .with_map_uuid(&split_lookup),
    )
}

/// The eleven card shapes of the live state: the seven of R23/R32, one
/// remembered name of each source not yet shown (R145) and, at the end, the
/// two ranks read off stored matches when MMR has no answer (R154).
fn live_cards() -> Vec<PlayerCard> {
    // 1. The user: visible and full, outline flag, no party or encounters.
    let mut me = card(
        identity("puuid-self", false, true, ("Me", "TAG")),
        Team::Ally,
    );
    me.is_self = true;
    me.is_self_party = true;
    me.party = Some(PartyMark {
        group: 0,
        confirmed: true,
    });
    me.encounters = Some(EncounterCount {
        with: 3,
        against: 1,
    });
    let me = build_card(me);

    // 2. A hidden player the user's own match taught: remembered, with the
    //    source match (a uuid).
    let mut known = card(
        identity("puuid-known", true, true, ("Secret", "NAME")),
        Team::Enemy,
    );
    known.remembered = remembered(RememberedSource::OwnMatch, Some(SPLIT_PATH), Some(SEEN_MS));
    known.party = Some(PartyMark {
        group: 0,
        confirmed: true,
    });
    known.encounters = Some(EncounterCount {
        with: 2,
        against: 1,
    });
    let known = build_card(known);

    // 3. Remembered, but the source match's map is unknown (and its time).
    let mut unsourced = card(
        identity("puuid-unsourced", true, true, ("Secret", "NAME")),
        Team::Enemy,
    );
    unsourced.remembered = remembered(RememberedSource::OwnMatch, None, None);
    let unsourced = build_card(unsourced);

    // 4. Hidden, agent not locked yet: "Hidden Player", no agent.
    //    Only the Riot ID and tag are hidden: rank and stats stay visible,
    //    so it gets the same mmr / last5 inputs as every other card.
    let hidden_player = build_card(card(
        identity("puuid-hidden", true, false, ("", "")),
        Team::Enemy,
    ));

    // 5. Hidden, agent locked: the agent's name.
    let mut hidden_agent = card(identity("puuid-agent", true, true, ("", "")), Team::Enemy);
    hidden_agent.party = Some(PartyMark {
        group: 1,
        confirmed: false,
    });
    let hidden_agent = build_card(hidden_agent);

    // 6. Everything still loading: every tri-state key is absent.
    let mut loading = build_card(BuildCardInput {
        mmr: Field::Loading,
        comp_updates: Field::Loading,
        last5: Field::Loading,
        ..card(
            identity("puuid-loading", false, false, ("", "")),
            Team::Ally,
        )
    });
    loading.level = Field::Loading;

    // 7. Everything known to be missing: every tri-state key is `null`.
    let mut missing_identity = identity("puuid-missing", false, false, ("", ""));
    missing_identity.hide_account_level = true;
    let mut missing = build_card(BuildCardInput {
        mmr: Field::Missing,
        comp_updates: Field::Missing,
        last5: Field::Missing,
        ..card(missing_identity, Team::Ally)
    });
    missing.name = Field::Missing;

    // 8. Remembered from a match the user did not play (R145): a map and a
    //    time, like a match the user played.
    let mut other_match = card(
        identity("puuid-other-match", true, true, ("Secret", "NAME")),
        Team::Enemy,
    );
    other_match.remembered = remembered(
        RememberedSource::OtherMatch,
        Some(SPLIT_PATH),
        Some(SEEN_MS),
    );
    let other_match = build_card(other_match);

    // 9. Remembered from a name-service answer at a visible sighting
    //    (R145): no map, only the time it was last confirmed.
    let mut seen = card(
        identity("puuid-seen", true, true, ("Secret", "NAME")),
        Team::Enemy,
    );
    seen.remembered = remembered(RememberedSource::Seen, None, Some(SEEN_MS));
    let seen = build_card(seen);

    // 10. No MMR answer, but a stored RR row of the current act (R154, path
    //     a): the rank with its exact RR. Everything else from MMR is `null`.
    let from_update = build_card(BuildCardInput {
        mmr: Field::Missing,
        ..card(
            identity("puuid-rank-update", false, true, ("Silent", "MMR")),
            Team::Ally,
        )
    });

    // 11. No MMR answer and no RR row (R154, path b): the tier a stored
    //     match of the current act shows, the RR unknown — `"rr": null`.
    let from_match = build_card(BuildCardInput {
        mmr: Field::Missing,
        comp_updates: Field::Value(Vec::new()),
        stored_tier: Some(StoredTier {
            start_ms: 2_300,
            tier: 14,
        }),
        ..card(
            identity("puuid-rank-match", false, true, ("Silent", "MATCH")),
            Team::Enemy,
        )
    });

    vec![
        me,
        known,
        unsourced,
        hidden_player,
        hidden_agent,
        loading,
        missing,
        other_match,
        seen,
        from_update,
        from_match,
    ]
}

#[test]
fn live_state_matches_golden() {
    let players = live_cards();
    // The shapes the cards are supposed to have, so a golden regenerated
    // from a broken builder can't pass unnoticed.
    let json = serde_json::to_value(&players).unwrap();
    assert_eq!(json[0]["isSelf"], json!(true));
    assert_eq!(json[0]["name"]["kind"], "visible");
    assert_eq!(json[1]["name"]["kind"], "remembered");
    assert_eq!(json[1]["name"]["fromMapId"], SPLIT_UUID);
    assert_eq!(json[1]["name"]["source"], "ownMatch");
    assert_eq!(json[2]["name"]["fromMapId"], Value::Null);
    // R145: one remembered example of each source.
    assert_eq!(json[7]["name"]["source"], "otherMatch");
    assert_eq!(json[7]["name"]["fromMapId"], SPLIT_UUID);
    assert_eq!(json[8]["name"]["source"], "seen");
    assert_eq!(json[8]["name"]["fromMapId"], Value::Null);
    assert_eq!(json[8]["name"]["fromStartMs"], json!(SEEN_MS));
    // R154: with MMR unanswered the rank still shows — an exact RR from the
    // newest RR row, or `null` from a stored match's tier — and the other
    // MMR fields stay `null`; with MMR answered the RR is a plain number.
    assert_eq!(json[0]["rank"], json!({ "tier": 15, "rr": 42 }));
    assert_eq!(json[9]["rank"], json!({ "tier": 15, "rr": 42 }));
    assert_eq!(json[10]["rank"], json!({ "tier": 14, "rr": null }));
    for i in [9, 10] {
        for key in ["prevActTier", "peak", "actWinrate", "actWins", "actGames"] {
            assert_eq!(json[i][key], Value::Null, "card {i}: {key}");
        }
    }
    assert_eq!(json[3]["name"]["kind"], "hiddenPlayer");
    assert_eq!(json[3]["agentId"], Value::Null);
    assert_eq!(json[4]["name"]["kind"], "hiddenAgent");
    for key in [
        "name",
        "level",
        "rank",
        "prevActTier",
        "peak",
        "kd",
        "hsPct",
        "actWinrate",
        "actWins",
        "actGames",
        "last5",
    ] {
        assert!(json[5].get(key).is_none(), "loading card has {key}");
        assert_eq!(json[6][key], Value::Null, "missing card lacks null {key}");
    }
    assert!(
        !json.to_string().contains("/Game/"),
        "a Riot map path reached the wire"
    );

    let state = LiveState {
        stage: Stage::Ingame,
        map_id: Some(SPLIT_UUID.into()),
        players,
        self_puuid: Some("puuid-self".into()),
        settled: true,
    };
    check_golden("live_state", &serde_json::to_value(&state).unwrap());

    // `settled` is a plain boolean, on the wire both ways: the model's starting
    // Closed (the poll loop has not looked yet) is `false`. An unknown account
    // is absent from the JSON, not `null` (the live state's `selfPuuid`).
    let starting = LiveState {
        stage: Stage::Closed,
        map_id: None,
        players: Vec::new(),
        self_puuid: None,
        settled: false,
    };
    assert_eq!(
        serde_json::to_value(&starting).unwrap(),
        json!({"stage": "closed", "mapId": null, "players": [], "settled": false})
    );
}

// ---------------------------------------------------------------------
// Player Details
// ---------------------------------------------------------------------

fn comp_row(id: &str, start_ms: i64, tier: Option<i64>) -> CompMatchRow {
    CompMatchRow {
        match_id: id.into(),
        map_id: ASCENT_UUID.into(),
        start_ms,
        winning_team: Some("Blue".into()),
        team_id: "Blue".into(),
        agent_id: Some("agent-jett".into()),
        kills: 20,
        deaths: 10,
        assists: 4,
        score: 4800,
        rounds: Some(24),
        hs: 25,
        bs: 50,
        ls: 25,
        rounds_won: Some(13),
        rounds_lost: Some(11),
        rr_earned: Some(21),
        season_id: Some(CURRENT_ACT_ID.into()),
        tier_before: Some(14),
        tier_after: Some(15),
        tier,
    }
}

fn skin(name: &str, chroma: Option<&str>, tier: Option<&str>) -> WeaponSkin {
    WeaponSkin {
        chroma_id: "b2a065c0-4632-ccaa-f496-7681dc2a6185".into(),
        skin_name: name.into(),
        chroma_name: chroma.map(str::to_string),
        tier_id: tier.map(str::to_string),
    }
}

/// The Player Details of the remembered card, with every shape in it.
fn player_details_value() -> Value {
    let known = live_cards().remove(1);

    // Recent rows, newest first (spec §7.1: `recent` is "≤8 newest first"):
    // one whose match had no stats row (every stat null, R102), one with no
    // rank (tier below 3 is null), one with a rank.
    let no_stats = CompMatchRow {
        kills: 0,
        deaths: 0,
        assists: 0,
        score: 0,
        rounds: None,
        hs: 0,
        bs: 0,
        ls: 0,
        rr_earned: None,
        ..comp_row("m3", 2_300, Some(2))
    };
    let recent = vec![
        match_summary(&no_stats),
        match_summary(&CompMatchRow {
            winning_team: Some("Red".into()),
            rounds_won: Some(9),
            rounds_lost: Some(13),
            ..comp_row("m2", 2_200, None)
        }),
        match_summary(&comp_row("m1", 2_100, Some(15))),
    ];

    let top_teammates = vec![
        TeammateSummary {
            puuid: "puuid-mate-1".into(),
            name: DisplayName::Visible {
                name: "Mate".into(),
                tag: "ONE".into(),
            },
            count: 5,
            wins: 3,
            losses: 2,
        },
        TeammateSummary {
            puuid: "puuid-mate-2".into(),
            name: DisplayName::Remembered {
                name: "Rival".into(),
                tag: "ENEMY".into(),
                source: RememberedSource::OwnMatch,
                from_map_id: Some(SPLIT_UUID.into()),
                from_start_ms: Some(SEEN_MS),
            },
            count: 3,
            wins: 1,
            losses: 1,
        },
        TeammateSummary {
            puuid: "puuid-mate-3".into(),
            name: DisplayName::HiddenAgent {
                agent_id: "agent-sova".into(),
            },
            count: 1,
            wins: 0,
            losses: 0,
        },
    ];

    // Two named skins, one weapon known to have none (`null`) and one whose
    // name is still resolving (key absent, R97).
    let loadout = Loadout {
        vandal: Field::Value(skin("Reaver", Some("Red"), Some(TIER_PREMIUM))),
        phantom: Field::Value(skin("Standard", None, None)),
        operator: Field::Missing,
        sheriff: Field::Loading,
    };

    let encounters = vec![
        EncounterDetailRow {
            match_id: "enc-1".into(),
            map_id: SPLIT_UUID.into(),
            start_ms: SEEN_MS,
            with: false,
            result: MatchResultKind::Loss,
            rounds_won: Some(10),
            rounds_lost: Some(13),
            duration_ms: Some(2_460_000),
            them_agent_id: Some("agent-reyna".into()),
            them_tier: Some(24),
            self_agent_id: Some("agent-jett".into()),
            self_tier: Some(21),
        },
        EncounterDetailRow {
            match_id: "enc-2".into(),
            map_id: String::new(),
            start_ms: SEEN_MS - 86_400_000,
            with: true,
            result: MatchResultKind::Draw,
            rounds_won: None,
            rounds_lost: None,
            duration_ms: None,
            them_agent_id: None,
            them_tier: None,
            self_agent_id: None,
            self_tier: None,
        },
    ];

    let details = PlayerDetails {
        card: known,
        recent,
        top_teammates,
        loadout,
        encounters,
        // R116: over the most recent (up to 20) current-act competitive
        // matches; 12 of them have stats.
        hero_kd: Some(1.5),
        hero_hs: Some(0.25),
        hero_matches: 12,
    };
    let json = serde_json::to_value(&details).unwrap();
    // The R97 tri-state of the loadout, as the UI reads it.
    assert!(json["loadout"]["vandal"].is_object());
    assert_eq!(json["loadout"]["operator"], Value::Null);
    assert!(
        json["loadout"].get("sheriff").is_none(),
        "resolving = absent"
    );
    // R102: a match with no stats row sends `null` — never 0 — for the
    // whole stat line, so spec §7.1 types `MatchSummary.kills`, `deaths`,
    // `assists` and `kd` as `number | null`, like `acs` and `hsPct`. A
    // match with stats sends numbers.
    for key in ["kills", "deaths", "assists", "kd", "acs", "hsPct"] {
        assert_eq!(
            json["recent"][0][key],
            Value::Null,
            "no-stats row: `{key}` must be null, not a counted zero"
        );
        assert!(
            json["recent"][2][key].is_number(),
            "row with stats: `{key}` must be a number"
        );
    }
    let starts: Vec<i64> = json["recent"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["startMs"].as_i64().unwrap())
        .collect();
    assert_eq!(starts, [2_300, 2_200, 2_100], "recent is newest first");
    assert_eq!(json["recent"][1]["tier"], Value::Null);
    assert!(json["encounters"][0].get("selfWon").is_none());
    assert!(!json.to_string().contains("/Game/"));
    json
}

#[test]
fn player_details_matches_golden() {
    check_golden("player_details", &player_details_value());
}

// ---------------------------------------------------------------------
// Match Details
// ---------------------------------------------------------------------

fn player(puuid: &str, team: &str, name: &str, score: i64) -> MatchPlayer {
    MatchPlayer {
        puuid: puuid.into(),
        team_id: team.into(),
        party_id: None,
        agent_id: Some(format!("agent-{puuid}")),
        kills: 20,
        deaths: 15,
        assists: 5,
        score,
        rounds: Some(24),
        hs: 25,
        bs: 50,
        ls: 25,
        game_name: (!name.is_empty()).then(|| name.to_string()),
        tag_line: (!name.is_empty()).then(|| "TAG".to_string()),
        competitive_tier: Some(15),
        damage: Some(3_120),
    }
}

fn info(queue: &str) -> Match {
    Match {
        match_id: format!("{queue}-match"),
        map_id: SPLIT_PATH.into(),
        queue_id: queue.into(),
        start_ms: SEEN_MS,
        winning_team: Some("Blue".into()),
        is_ranked: queue == "competitive",
        rounds_played: Some(24),
        game_length_ms: Some(2_280_000),
    }
}

fn team(id: &str, won: bool, rounds: i64, h1: Option<i64>) -> MatchTeam {
    MatchTeam {
        team_id: id.into(),
        won,
        rounds_won: Some(rounds),
        rounds_won_h1: h1,
    }
}

fn view(
    stored: &StoredMatch,
    self_puuid: Option<&str>,
    focus: &str,
    hidden_now: &HashMap<String, Option<String>>,
    remembered: &HashMap<String, KnownName>,
    seen: &HashMap<String, (String, String)>,
) -> Value {
    let view = build_match_view(MatchViewInput {
        stored,
        map_uuid: &split_lookup,
        self_puuid,
        self_name: self_puuid.map(|_| ("Me".to_string(), "TAG".to_string())),
        focus_puuid: focus,
        hidden_now,
        remembered,
        seen,
    });
    serde_json::to_value(view).unwrap()
}

/// A 2-team competitive view (the user's perspective) and a deathmatch
/// view, as a 2-element array.
fn match_views_value() -> Value {
    // The user (on Red, the losing side) played a 13-11 competitive match:
    // a teammate with a stored name, a stored name that is live-hidden now
    // and remembered from the user's own match, an enemy without a stored
    // name whose visible sighting stored one (R145: shown as a plain
    // name), and one without any name and without stats.
    // A player whose details had no stats row: stored as 0/0/0 without
    // rounds, so ACS, ADR and HS% are null.
    let mut no_stats = player("enemy-3", "Blue", "", 0);
    (no_stats.kills, no_stats.deaths, no_stats.assists) = (0, 0, 0);
    (no_stats.rounds, no_stats.damage) = (None, None);
    (no_stats.hs, no_stats.bs, no_stats.ls) = (0, 0, 0);
    no_stats.competitive_tier = Some(2);
    let stored = StoredMatch {
        info: info("competitive"),
        teams: vec![
            team("Blue", true, 13, Some(8)),
            team("Red", false, 11, Some(4)),
        ],
        players: vec![
            player("puuid-self", "Red", "Me", 4_800),
            player("ally", "Red", "Ally", 6_000),
            player("enemy-1", "Blue", "Secret", 7_200),
            player("enemy-2", "Blue", "", 3_600),
            no_stats,
        ],
    };
    let hidden_now = HashMap::from([("enemy-1".to_string(), Some("agent-enemy-1".to_string()))]);
    let remembered = HashMap::from([(
        "enemy-1".to_string(),
        KnownName {
            name: "Rival".into(),
            tag: "ENEMY".into(),
            map_id: Some(SPLIT_PATH.into()),
            start_ms: Some(SEEN_MS),
            source: RememberedSource::OwnMatch,
        },
    )]);
    let seen = HashMap::from([(
        "enemy-2".to_string(),
        ("Friend".to_string(), "FR1".to_string()),
    )]);
    let competitive = view(
        &stored,
        Some("puuid-self"),
        "ally",
        &hidden_now,
        &remembered,
        &seen,
    );
    assert_eq!(competitive["perspective"], "self");
    assert_eq!(competitive["teams"][0]["teamId"], "Red");
    assert_eq!(competitive["teams"][0]["firstHalf"], json!(4));
    assert_eq!(
        competitive["teams"][1]["players"][0]["name"]["kind"],
        "remembered"
    );
    assert_eq!(
        competitive["teams"][1]["players"][1]["name"],
        json!({"kind": "visible", "name": "Friend", "tag": "FR1"}),
        "R145: the players-table name of a row without a stored name"
    );
    // R124: a scoreboard row whose details had no stats row sends `null`
    // (never a counted 0) for kills, deaths and assists as well as for ACS,
    // ADR and HS%, so the UI draws `—`. Rows with stats send numbers. The
    // stat-less row sorts last (ACS nulls last).
    let blue = &competitive["teams"][1]["players"];
    for key in ["kills", "deaths", "assists", "acs", "adr", "hsPct"] {
        assert_eq!(
            blue[2][key],
            Value::Null,
            "no-stats row: `{key}` must be null, not a counted zero"
        );
        assert!(
            blue[0][key].is_number(),
            "row with stats: `{key}` must be a number"
        );
    }

    // A deathmatch the user did not play: no teams, nothing scored.
    let mut dm_players = vec![
        player("dm-1", "dm-1", "One", 1_200),
        player("dm-2", "dm-2", "", 3_600),
        player("dm-3", "dm-3", "Three", 2_400),
    ];
    for p in &mut dm_players {
        p.rounds = Some(1);
        p.damage = None;
        p.competitive_tier = Some(0);
    }
    let dm = StoredMatch {
        info: info("deathmatch"),
        teams: vec![
            team("dm-1", false, 10, None),
            team("dm-2", true, 40, None),
            team("dm-3", false, 20, None),
        ],
        players: dm_players,
    };
    // dm-2 has no stored name; a match of someone else's history taught it
    // (R145): remembered, from another match.
    let dm_remembered = HashMap::from([(
        "dm-2".to_string(),
        KnownName {
            name: "Rival".into(),
            tag: "ENEMY".into(),
            map_id: Some(SPLIT_PATH.into()),
            start_ms: Some(SEEN_MS),
            source: RememberedSource::OtherMatch,
        },
    )]);
    let deathmatch = view(
        &dm,
        Some("puuid-self"),
        "dm-3",
        &HashMap::new(),
        &dm_remembered,
        &HashMap::new(),
    );
    assert_eq!(deathmatch["perspective"], "focus");
    assert_eq!(deathmatch["teams"].as_array().unwrap().len(), 1);
    assert_eq!(deathmatch["teams"][0]["teamId"], "");

    let both = json!([competitive, deathmatch]);
    assert!(!both.to_string().contains("/Game/"));
    both
}

#[test]
fn match_details_view_matches_golden() {
    check_golden("match_details_view", &match_views_value());
}

// ---------------------------------------------------------------------
// My Stats
// ---------------------------------------------------------------------

const MIN_MS: i64 = 60_000;
const HOUR_MS: i64 = 60 * MIN_MS;
const DAY_MS: i64 = 24 * HOUR_MS;
/// Monday 2025-08-18 00:00 UTC: the start of the My Stats act.
const MS_ACT_START: i64 = 20_318 * DAY_MS;
const MS_ACT_ID: &str = "act-my-stats";
/// The user's PUUID in the My Stats goldens.
const MS_PUUID: &str = "a1b2c3d4-0000-4000-8000-00000000cafe";

fn ms_act() -> Act {
    Act {
        id: MS_ACT_ID.into(),
        name: "ACT V".into(),
        episode_name: "V26".into(),
        start_ms: MS_ACT_START,
        end_ms: MS_ACT_START + 60 * DAY_MS,
    }
}

/// `day` days after the act's start, at `hour:minute` UTC.
fn ms_at(day: i64, hour: i64, minute: i64) -> i64 {
    MS_ACT_START + day * DAY_MS + hour * HOUR_MS + minute * MIN_MS
}

fn ms_map_names() -> HashMap<String, String> {
    HashMap::from([
        (SPLIT_UUID.to_string(), "Split".to_string()),
        (ASCENT_UUID.to_string(), "Ascent".to_string()),
    ])
}

/// The full case: 16 competitive matches (8 wins, 8 losses; every count a power of two, so every rate in
/// the golden is a dyadic fraction) on 2 maps with 2 agents, spread over a Monday, a Tuesday, a weekend
/// and today (a Wednesday, 4 matches within 6h = the session), plus a 17th: a draw just after midnight with no
/// RR row, map, agent or stats, which ends the session with a chip whose `rrEarned` and `mapId` are null (it
/// sits in buckets that were empty, so no float in the golden turns non-dyadic). It is built so that all six insights fire,
/// the ladder promotes and demotes (6 rank changes, 5 sent), and Morning is an empty bucket.
fn my_stats_full() -> Value {
    // (day, hour, minute, won, on Split): idx 0..16 in play order.
    const GAMES: [(i64, i64, i64, bool, bool); 16] = [
        (21, 13, 0, true, true),    // Mon
        (21, 19, 0, false, true),   // Mon
        (22, 14, 0, false, true),   // Tue
        (22, 20, 0, true, true),    // Tue
        (26, 12, 30, true, true),   // Sat
        (26, 13, 30, true, true),   // Sat
        (26, 18, 0, false, false),  // Sat
        (26, 19, 0, true, true),    // Sat
        (27, 12, 30, true, true),   // Sun
        (27, 15, 0, false, false),  // Sun
        (27, 18, 30, false, false), // Sun
        (27, 20, 30, true, false),  // Sun
        (30, 13, 0, false, false),  // Wed (the session: 13:00 ... 18:10)
        (30, 14, 10, true, false),  // Wed
        (30, 17, 0, false, false),  // Wed
        (30, 18, 10, false, false), // Wed
    ];
    const WIN_RR: [i64; 4] = [20, 18, 22, 19];
    const LOSS_RR: [i64; 2] = [16, 14];

    // The ladder: Ascendant 1 (tier 21) with 50 RR at the start; 100 RR promotes, below 0 demotes.
    let (mut tier, mut rr) = (21i64, 50i64);
    let (mut wins, mut losses) = (0usize, 0usize);
    let mut matches = Vec::new();
    let mut updates = Vec::new();
    for (i, &(day, hour, minute, won, split)) in GAMES.iter().enumerate() {
        let delta = if won {
            wins += 1;
            WIN_RR[(wins - 1) % WIN_RR.len()]
        } else {
            losses += 1;
            -LOSS_RR[(losses - 1) % LOSS_RR.len()]
        };
        let before = tier;
        rr += delta;
        if rr >= 100 {
            (tier, rr) = (tier + 1, rr - 100);
        } else if rr < 0 {
            (tier, rr) = (tier - 1, rr + 100);
        }
        let start_ms = ms_at(day, hour, minute);
        let id = format!("ms-{i:02}");
        // Jett on even positions, Omen on odd; 16 kills on even, 20 on odd: K/D 2.25 on both maps.
        let even = i % 2 == 0;
        matches.push(CompMatchRow {
            match_id: id.clone(),
            map_id: if split { SPLIT_UUID } else { ASCENT_UUID }.into(),
            start_ms,
            winning_team: Some(if won { "Blue" } else { "Red" }.into()),
            team_id: "Blue".into(),
            agent_id: Some(if even { "agent-jett" } else { "agent-omen" }.into()),
            kills: if even { 16 } else { 20 },
            deaths: 8,
            assists: 4,
            score: 4800,
            rounds: Some(24),
            hs: 25,
            bs: 50,
            ls: 25,
            rounds_won: Some(if won { 13 } else { 11 }),
            rounds_lost: Some(if won { 11 } else { 13 }),
            rr_earned: Some(delta),
            season_id: Some(MS_ACT_ID.into()),
            tier_before: Some(before),
            tier_after: Some(tier),
            tier: Some(before),
        });
        updates.push(CompUpdateRow {
            match_id: id,
            season_id: MS_ACT_ID.into(),
            tier_after: tier,
            tier_before: Some(before),
            rr_after: rr,
            rr_earned: delta,
            map_id: None,
            start_ms,
        });
    }
    // The newest match of the session is the odd one: Riot has no RR row for it yet (so `season_id` and the tiers
    // are null), no map the app can name, no agent, no stats row and no winner (a draw). On the wire its chip has
    // `rrEarned` and `mapId` null (never 0, never ""), and it is a D chip. It lands on a Thursday, in the Night
    // bucket, both empty before, and on no map or agent row, so every rate in the golden stays dyadic. It started
    // 5h55m after the last of the session's others: still the same session.
    matches.push(CompMatchRow {
        match_id: "ms-16".into(),
        map_id: String::new(),
        start_ms: ms_at(31, 0, 5),
        winning_team: None,
        team_id: "Blue".into(),
        agent_id: None,
        kills: 0,
        deaths: 0,
        assists: 0,
        score: 0,
        rounds: None,
        hs: 0,
        bs: 0,
        ls: 0,
        rounds_won: None,
        rounds_lost: None,
        rr_earned: None,
        season_id: None,
        tier_before: None,
        tier_after: None,
        tier: None,
    });

    let act = ms_act();
    let names = ms_map_names();
    let utc = |_: i64| 0;
    let stats = mystats::build(MyStatsInput {
        puuid: MS_PUUID,
        act: &act,
        matches: &matches,
        comp_updates: &updates,
        current_rank: Some(CurrentRank {
            tier: tier as u8,
            rr: rr as u16,
        }),
        mmr_tier_without_rr: None,
        has_mmr: true,
        stored_tier: None,
        map_names: &names,
        now_ms: ms_at(31, 1, 0),
        utc_offset_ms: &utc,
    });

    // What the golden is meant to show, so a change to the builder cannot quietly hollow it out.
    assert_eq!(stats.rr_series.len(), 16);
    assert_eq!(stats.rank.changes.len(), 5, "5 of 6 rank changes are sent");
    assert_eq!(stats.rank.rr_to_next, Some(12));
    assert_eq!(stats.rank.net_rr, Some(38));
    assert_eq!(stats.insights.len(), 6, "every insight fires");
    let session = stats.session.as_ref().expect("the full case has a session");
    assert_eq!(
        session
            .matches
            .iter()
            .map(|m| (
                m.match_id.as_str(),
                m.result,
                m.map_id.as_deref(),
                m.rr_earned
            ))
            .collect::<Vec<_>>(),
        [
            ("ms-12", StreakMark::Loss, Some(ASCENT_UUID), Some(-14)),
            ("ms-13", StreakMark::Win, Some(ASCENT_UUID), Some(19)),
            ("ms-14", StreakMark::Loss, Some(ASCENT_UUID), Some(-16)),
            ("ms-15", StreakMark::Loss, Some(ASCENT_UUID), Some(-14)),
            // No RR row and no known map: both null on the wire, never 0 and never "".
            ("ms-16", StreakMark::Draw, None, None),
        ],
        "the session's chips, oldest first: four with a map and an RR change, a draw with neither"
    );
    assert_eq!(
        session.net_rr,
        Some(-25),
        "the session's Net RR is the known RR changes' sum, the draw's unknown one left out"
    );
    assert_eq!(stats.last5.len(), 5);
    assert_eq!((stats.maps.len(), stats.agents.len()), (2, 2));
    assert!(stats.by_daypart.iter().any(|c| c.winrate.is_none()));
    serde_json::to_value(&stats).unwrap()
}

/// The edge case: no MMR snapshot, so the R167 fallback gives the stored match's tier with `rr` and `rrToNext`
/// null; no stored competitive update (empty
/// `rrSeries`, null net RR and averages, no rank change), a match with no stats row (the whole stat line
/// null, R102) on an unknown map with no agent, a draw, and nothing in the last 6h (`session` null) or enough
/// to say anything (`insights` empty).
fn my_stats_edge() -> Value {
    let row =
        |id: &str, day: i64, hour: i64, result: Option<&str>, map: &str, agent: Option<&str>| {
            CompMatchRow {
                match_id: id.into(),
                map_id: map.into(),
                start_ms: ms_at(day, hour, 0),
                winning_team: result.map(str::to_string),
                team_id: "Blue".into(),
                agent_id: agent.map(str::to_string),
                kills: 0,
                deaths: 0,
                assists: 0,
                score: 0,
                rounds: None,
                hs: 0,
                bs: 0,
                ls: 0,
                rounds_won: None,
                rounds_lost: None,
                rr_earned: None,
                season_id: None,
                tier_before: None,
                tier_after: None,
                tier: None,
            }
        };
    let mut draw = row("edge-draw", 3, 20, None, SPLIT_UUID, Some("agent-jett"));
    (draw.kills, draw.deaths, draw.assists) = (12, 12, 3);
    (draw.score, draw.rounds) = (4320, Some(24));
    (draw.hs, draw.bs, draw.ls) = (8, 24, 8);
    (draw.rounds_won, draw.rounds_lost) = (Some(12), Some(12));
    let matches = vec![
        draw,
        row("edge-bare", 5, 9, Some("Blue"), "", None),
        row(
            "edge-loss",
            6,
            22,
            Some("Red"),
            ASCENT_UUID,
            Some("agent-omen"),
        ),
    ];

    let act = ms_act();
    let names = ms_map_names();
    let utc = |_: i64| 0;
    let stats = mystats::build(MyStatsInput {
        puuid: MS_PUUID,
        act: &act,
        matches: &matches,
        comp_updates: &[],
        current_rank: None,
        mmr_tier_without_rr: None,
        // R167: no MMR snapshot and no RR row, so the rank card falls back to the newest stored ranked match's
        // tier: a known tier with `rr` and `rrToNext` null.
        has_mmr: false,
        stored_tier: Some(StoredTier {
            start_ms: ms_at(6, 22, 0),
            tier: 14,
        }),
        map_names: &names,
        now_ms: ms_at(40, 12, 0),
        utc_offset_ms: &utc,
    });

    assert_eq!(
        stats.rank.tier,
        Some(14),
        "R167 fallback: the stored match's tier"
    );
    assert!(stats.rank.rr.is_none() && stats.rank.rr_to_next.is_none());
    assert!(
        stats.rank.net_rr.is_none()
            && stats.rank.avg_win.is_none()
            && stats.rank.avg_loss.is_none()
    );
    assert!(stats.rr_series.is_empty() && stats.rank.changes.is_empty());
    assert!(stats.session.is_none() && stats.insights.is_empty());
    assert_eq!(stats.last5.len(), 3);
    assert!(
        stats
            .last5
            .iter()
            .any(|m| m.kills.is_none() && m.acs.is_none()),
        "a match with no stats row"
    );
    serde_json::to_value(&stats).unwrap()
}

#[test]
fn my_stats_matches_golden() {
    let both = json!([my_stats_full(), my_stats_edge()]);
    assert!(
        !both.to_string().contains("/Game/"),
        "a Riot map path reached the wire"
    );
    check_golden("my_stats", &both);
}

// ---------------------------------------------------------------------
// the keys, as spec §7.1 spells them
// ---------------------------------------------------------------------

fn keys(v: &Value) -> BTreeSet<String> {
    v.as_object()
        .unwrap_or_else(|| panic!("not an object: {v}"))
        .keys()
        .cloned()
        .collect()
}

fn spec(list: &[&str]) -> BTreeSet<String> {
    list.iter().map(|k| k.to_string()).collect()
}

#[test]
fn wire_keys_are_the_ones_spec_7_1_spells() {
    let cards = serde_json::to_value(live_cards()).unwrap();
    // LiveState itself (the `live_state.json` golden's top level).
    let live: Value =
        serde_json::from_str(&std::fs::read_to_string(golden_path("live_state")).unwrap()).unwrap();
    assert_eq!(
        keys(&live),
        spec(&["stage", "mapId", "players", "selfPuuid", "settled"])
    );
    // A fully populated card has exactly the spec's PlayerCard keys.
    assert_eq!(
        keys(&cards[0]),
        spec(&[
            "puuid",
            "name",
            "level",
            "cardId",
            "agentId",
            "rank",
            "prevActTier",
            "peak",
            "kd",
            "hsPct",
            "actWinrate",
            "actWins",
            "actGames",
            "last5",
            "party",
            "encounters",
            "team",
            "isSelfParty",
            "isSelf"
        ])
    );
    assert_eq!(
        keys(&cards[0]["peak"]),
        spec(&[
            "tier",
            "actLabel",
            "actName",
            "sinceActEndMs",
            "actStartMs",
            "actEndMs"
        ])
    );
    assert_eq!(keys(&cards[0]["rank"]), spec(&["tier", "rr"]));
    // R154: the RR key is there even when the RR is not known (`null`).
    assert_eq!(keys(&cards[10]["rank"]), spec(&["tier", "rr"]));
    assert_eq!(
        keys(&cards[0]["last5"][0]),
        spec(&["result", "rrEarned", "startMs"])
    );
    assert_eq!(keys(&cards[1]["party"]), spec(&["group", "confirmed"]));
    assert_eq!(keys(&cards[1]["encounters"]), spec(&["with", "against"]));
    assert_eq!(
        keys(&cards[1]["name"]),
        spec(&["kind", "name", "tag", "source", "fromMapId", "fromStartMs"])
    );

    // Player Details and its parts.
    let golden = player_details_value();
    assert_eq!(
        keys(&golden),
        spec(&[
            "card",
            "recent",
            "topTeammates",
            "loadout",
            "encounters",
            "heroKd",
            "heroHs",
            "heroMatches"
        ])
    );
    assert_eq!(
        keys(&golden["recent"][0]),
        spec(&[
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
            "tier"
        ])
    );
    assert_eq!(
        keys(&golden["topTeammates"][0]),
        spec(&["puuid", "name", "count", "wins", "losses"])
    );
    assert_eq!(
        keys(&golden["loadout"]["vandal"]),
        spec(&["chromaId", "skinName", "chromaName", "tierId"])
    );
    assert_eq!(
        keys(&golden["encounters"][0]),
        spec(&[
            "matchId",
            "mapId",
            "startMs",
            "with",
            "result",
            "roundsWon",
            "roundsLost",
            "durationMs",
            "themAgentId",
            "themTier",
            "selfAgentId",
            "selfTier"
        ])
    );

    // Match Details.
    let views = match_views_value();
    assert_eq!(
        keys(&views[0]),
        spec(&[
            "matchId",
            "mapId",
            "queueId",
            "startMs",
            "durationMs",
            "perspective",
            "teams"
        ])
    );
    assert_eq!(
        keys(&views[0]["teams"][0]),
        spec(&["teamId", "result", "roundsWon", "firstHalf", "players"])
    );
    assert_eq!(
        keys(&views[0]["teams"][0]["players"][0]),
        spec(&[
            "puuid", "name", "agentId", "tier", "kills", "deaths", "assists", "acs", "adr",
            "hsPct", "isSelf", "isFocus"
        ])
    );

    // My Stats and its parts (spec §9).
    let stats = my_stats_full();
    assert_eq!(
        keys(&stats),
        spec(&[
            "puuid",
            "rank",
            "rrSeries",
            "session",
            "last5",
            "maps",
            "agents",
            "byWeekday",
            "byDaypart",
            "insights"
        ])
    );
    assert_eq!(
        keys(&stats["rank"]),
        spec(&[
            "tier", "rr", "wins", "losses", "rrToNext", "netRr", "avgWin", "avgLoss", "changes"
        ])
    );
    assert_eq!(
        keys(&stats["rank"]["changes"][0]),
        spec(&["tier", "up", "startMs"])
    );
    assert_eq!(
        keys(&stats["rrSeries"][0]),
        spec(&["startMs", "absoluteRr"])
    );
    assert_eq!(
        keys(&stats["session"]),
        spec(&["netRr", "wins", "losses", "startedMs", "matches"])
    );
    assert_eq!(
        keys(&stats["session"]["matches"][0]),
        spec(&["matchId", "mapId", "rrEarned", "startMs", "result"])
    );
    assert_eq!(keys(&stats["last5"][0]), keys(&golden["recent"][0]));
    assert_eq!(
        keys(&stats["maps"][0]),
        spec(&["id", "games", "wins", "losses", "winrate", "netRr", "kd", "acs"])
    );
    assert_eq!(
        keys(&stats["byWeekday"][0]),
        spec(&["games", "wins", "losses", "winrate"])
    );
    assert_eq!(stats["byWeekday"].as_array().map(Vec::len), Some(7));
    assert_eq!(stats["byDaypart"].as_array().map(Vec::len), Some(4));
    assert_eq!(
        keys(&stats["insights"][0]),
        spec(&["kind", "text", "matches", "positive"])
    );
}

// ---------------------------------------------------------------------
// v2 redesign (R189): HistoryPage, SavedPlayer, PlayerMarks, Settings,
// AsciiArt, AppInfo
// ---------------------------------------------------------------------

use opd1_core::db::marks::saved_player;
use opd1_core::db::repo::{HistoryEntry, Player, Repo};
use opd1_core::wire::{AppInfo, HistoryQueue, Settings, SmurfLevel};

const V2_ME: &str = "a1b2c3d4-0000-4000-8000-000000000001";
const V2_DUO: &str = "a1b2c3d4-0000-4000-8000-000000000002";
const V2_HIDDEN: &str = "a1b2c3d4-0000-4000-8000-000000000003";
const V2_NAMELESS: &str = "a1b2c3d4-0000-4000-8000-000000000004";
const V2_NEVER_SEEN: &str = "a1b2c3d4-0000-4000-8000-000000000005";
const V2_JETT: &str = "add6443a-41bd-e414-f6ad-e58d267f4e95";
const V2_OMEN: &str = "8e253930-4c05-31dd-1b6c-968525494517";
/// The first match's start (2025-09-27 10:46:40 UTC); the others follow by
/// the hour.
const V2_T0: i64 = SEEN_MS;
const V2_HOUR: i64 = 3_600_000;

fn v2_match_id(n: u32) -> String {
    format!("0000000{n}-1111-4111-8111-111111111111")
}

fn v2_repo() -> Repo {
    Repo::new(opd1_core::db::open_db(std::path::Path::new(":memory:")).unwrap())
}

/// One other player of a stored match: puuid, team, name, tier.
type Other<'a> = (&'a str, &'a str, &'a str, Option<i64>);

/// One stored match of the user's, indexed in their history list; the user
/// is on Blue, `winner` `None` is a draw. The user's stat line is 20/16/4,
/// 5000 score over 25 rounds, 10/25/5 shots (K/D 1.25, ACS 200, HS 0.25);
/// `no_stats` stores none (R102).
fn v2_store(
    r: &Repo,
    n: u32,
    queue: &str,
    map: &str,
    winner: Option<&str>,
    others: &[Other],
    no_stats: bool,
) {
    let mut me = player(V2_ME, "Blue", "Me", 5_000);
    (me.kills, me.deaths, me.assists) = (20, 16, 4);
    (me.hs, me.bs, me.ls, me.rounds) = (10, 25, 5, Some(25));
    me.agent_id = Some(V2_JETT.into());
    if no_stats {
        (me.kills, me.deaths, me.assists, me.score) = (0, 0, 0, 0);
        (me.hs, me.bs, me.ls, me.rounds) = (0, 0, 0, None);
    }
    let mut rows = vec![me];
    for (puuid, team_id, name, tier) in others {
        let mut p = player(puuid, team_id, name, 3_000);
        p.competitive_tier = *tier;
        p.agent_id = Some(V2_OMEN.into());
        rows.push(p);
    }
    let id = v2_match_id(n);
    let start_ms = V2_T0 + i64::from(n) * V2_HOUR;
    let rounds = |side: &str| match winner {
        Some(w) if w == side => 13,
        Some(_) => 11,
        None => 12,
    };
    r.upsert_match(
        &Match {
            match_id: id.clone(),
            map_id: map.into(),
            queue_id: queue.into(),
            start_ms,
            winning_team: winner.map(str::to_string),
            is_ranked: queue == "competitive",
            rounds_played: Some(25),
            game_length_ms: Some(2_280_000),
        },
        &[
            team("Blue", winner == Some("Blue"), rounds("Blue"), Some(7)),
            team("Red", winner == Some("Red"), rounds("Red"), Some(5)),
        ],
        &rows,
    )
    .unwrap();
    r.index_history(
        V2_ME,
        &[HistoryEntry {
            match_id: id,
            queue_id: queue.into(),
            start_ms,
        }],
    )
    .unwrap();
}

/// The user's history: a competitive win with its RR row, a competitive draw
/// on a map the asset tables do not know, an unrated loss without a stats row
/// (R102), and a match only in the history list (skipped: no details).
fn v2_history_repo() -> Repo {
    let r = v2_repo();
    v2_store(
        &r,
        1,
        "competitive",
        SPLIT_PATH,
        Some("Blue"),
        &[(V2_DUO, "Blue", "Duo", Some(16))],
        false,
    );
    v2_store(
        &r,
        2,
        "competitive",
        "/Game/Maps/Unknown/Unknown",
        None,
        &[(V2_HIDDEN, "Red", "Secret", Some(18))],
        false,
    );
    v2_store(
        &r,
        3,
        "unrated",
        SPLIT_PATH,
        Some("Red"),
        &[(V2_DUO, "Red", "Duo", None)],
        true,
    );
    r.index_history(
        V2_ME,
        &[HistoryEntry {
            match_id: v2_match_id(4),
            queue_id: "competitive".into(),
            start_ms: V2_T0 + 4 * V2_HOUR,
        }],
    )
    .unwrap();
    r.upsert_comp_updates(
        V2_ME,
        &[CompUpdateRow {
            match_id: v2_match_id(1),
            season_id: CURRENT_ACT_ID.into(),
            tier_after: 16,
            tier_before: Some(15),
            rr_after: 9,
            rr_earned: 22,
            map_id: Some(SPLIT_PATH.into()),
            start_ms: V2_T0 + V2_HOUR,
        }],
    )
    .unwrap();
    r
}

/// `[the competitive tab, the all tab]`, first page each.
fn history_page_value() -> Value {
    let r = v2_history_repo();
    let page = |queue| r.match_history(V2_ME, queue, 0, 50, &split_lookup).unwrap();
    serde_json::to_value([page(HistoryQueue::Competitive), page(HistoryQueue::All)]).unwrap()
}

#[test]
fn history_page_matches_golden() {
    let v = history_page_value();
    assert!(
        !v.to_string().contains("/Game/"),
        "a Riot map path reached the wire"
    );
    check_golden("history_page", &v);
}

/// The Saved screen, newest saved first: the duo the DB knows (visible, with
/// a note), the opponent incognito on the live screen now (remembered, R145),
/// a nameless player who locked an agent live (hidden agent), and one the DB
/// never saw (hidden player, everything unknown).
fn saved_players_value() -> Value {
    let r = v2_history_repo();
    r.upsert_player(&Player {
        puuid: V2_NAMELESS.into(),
        game_name: String::new(),
        tag_line: String::new(),
        account_level: 12,
        card_id: "card".into(),
        updated_ms: V2_T0,
    })
    .unwrap();
    for (puuid, hours) in [
        (V2_DUO, 10),
        (V2_HIDDEN, 9),
        (V2_NAMELESS, 8),
        (V2_NEVER_SEEN, 7),
    ] {
        r.set_saved(puuid, true, V2_T0 + hours * V2_HOUR).unwrap();
    }
    r.set_note(V2_DUO, "Entry every round, dry-peeks A long", 1)
        .unwrap();
    let hidden_now: HashMap<&str, Option<&str>> =
        [(V2_HIDDEN, None), (V2_NAMELESS, Some(V2_OMEN))].into();
    let rows: Vec<_> = r
        .saved_rows(Some(V2_ME))
        .unwrap()
        .into_iter()
        .map(|row| {
            let hidden = hidden_now.get(row.puuid.as_str()).copied();
            saved_player(row, hidden, &split_lookup)
        })
        .collect();
    serde_json::to_value(rows).unwrap()
}

#[test]
fn saved_players_match_golden() {
    let v = saved_players_value();
    assert!(
        !v.to_string().contains("/Game/"),
        "a Riot map path reached the wire"
    );
    let kinds: Vec<&str> = v
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"]["kind"].as_str().unwrap())
        .collect();
    assert_eq!(
        kinds,
        ["visible", "remembered", "hiddenAgent", "hiddenPlayer"]
    );
    check_golden("saved_players", &v);
}

/// `[some marks, none]`.
fn player_marks_value() -> Value {
    let r = v2_repo();
    r.set_saved(V2_DUO, true, V2_T0 + 2).unwrap();
    r.set_saved(V2_HIDDEN, true, V2_T0 + 1).unwrap();
    r.set_note(V2_DUO, "Entry every round", 1).unwrap();
    r.set_note(V2_ME, "Warm up in Deathmatch first", 1).unwrap();
    r.dismiss_smurf(V2_NAMELESS, 1).unwrap();
    serde_json::to_value([r.player_marks().unwrap(), v2_repo().player_marks().unwrap()]).unwrap()
}

#[test]
fn player_marks_match_golden() {
    check_golden("player_marks", &player_marks_value());
}

/// `[the defaults, every value changed]`.
fn settings_value() -> Value {
    let changed = Settings {
        highlight: false,
        enemy_first: true,
        smurf: SmurfLevel::Eager,
        saved_alert: false,
        sound: true,
        on_top: false,
        boot: false,
        tray: false,
    };
    // What `set_settings` stores is what `get_settings` reads back.
    let back: Settings = serde_json::from_value(serde_json::to_value(&changed).unwrap()).unwrap();
    assert_eq!(back, changed);
    serde_json::to_value([Settings::default(), changed]).unwrap()
}

#[test]
fn settings_match_golden() {
    check_golden("settings", &settings_value());
}

/// The gallery as `ascii_list` returns it, newest first.
fn ascii_art_value() -> Value {
    let r = v2_repo();
    r.ascii_save("Jett", " /\\\n/__\\", V2_T0).unwrap();
    r.ascii_save("OPD1", "#  #\n####", V2_T0 + V2_HOUR).unwrap();
    serde_json::to_value(r.ascii_list().unwrap()).unwrap()
}

#[test]
fn ascii_art_matches_golden() {
    check_golden("ascii_art", &ascii_art_value());
}

/// `[everything known, a build without git with no game seen yet]`.
/// `AppInfo` has no builder in `opd1-core` (the shell fills it from the
/// build, the session and the DB file), so the values are written out.
fn app_info_value() -> Value {
    let full = AppInfo {
        version: "0.4.0".into(),
        commit: Some("cb58707".into()),
        build_ms: Some(V2_T0),
        riot_client_version: Some("release-11.08-shipping-14-3373442".into()),
        valorant_version: Some("release-11.08".into()),
        db_matches: 1_234,
        db_bytes: 3_145_728,
    };
    let unknown = AppInfo {
        commit: None,
        build_ms: None,
        riot_client_version: None,
        valorant_version: None,
        db_matches: 0,
        db_bytes: 0,
        ..full.clone()
    };
    serde_json::to_value([full, unknown]).unwrap()
}

#[test]
fn app_info_matches_golden() {
    check_golden("app_info", &app_info_value());
}

#[test]
fn v2_wire_keys_are_the_ones_types_ts_spells() {
    let history = history_page_value();
    assert_eq!(
        keys(&history[0]),
        spec(&["rows", "total", "competitiveTotal", "allTotal"])
    );
    assert_eq!(
        keys(&history[1]["rows"][0]),
        spec(&[
            "matchId",
            "mapId",
            "queueId",
            "startMs",
            "durationMs",
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
            "tierAfter"
        ])
    );
    let saved = saved_players_value();
    assert_eq!(
        keys(&saved[0]),
        spec(&[
            "puuid",
            "name",
            "agentId",
            "tier",
            "with",
            "against",
            "note",
            "savedMs",
            "lastSeenMs",
            "lastSeenMapId"
        ])
    );
    assert_eq!(keys(&saved[0]["with"]), spec(&["games", "wins", "losses"]));
    assert_eq!(
        keys(&player_marks_value()[0]),
        spec(&["saved", "notes", "notSmurf"])
    );
    assert_eq!(
        keys(&settings_value()[0]),
        spec(&[
            "highlight",
            "enemyFirst",
            "smurf",
            "savedAlert",
            "sound",
            "onTop",
            "boot",
            "tray"
        ])
    );
    assert_eq!(
        keys(&ascii_art_value()[0]),
        spec(&["id", "title", "raw", "createdMs"])
    );
    assert_eq!(
        keys(&app_info_value()[0]),
        spec(&[
            "version",
            "commit",
            "buildMs",
            "riotClientVersion",
            "valorantVersion",
            "dbMatches",
            "dbBytes"
        ])
    );
}

// ---------------------------------------------------------------------
// R189 (backend-live): AppStatus and ProfileExtras
// ---------------------------------------------------------------------

/// The `"status"` feed: OK in Agent Select (with and without Riot's timer),
/// OK queueing, OK in the Lobby with a recap, lost, and the error screen. Values, not a builder: the poll
/// loop sets the fields (its pipeline tests cover when); this pins the wire.
fn app_statuses_value() -> Value {
    use opd1_core::wire::{AppStatus, ConnState, PregameInfo, QueueInfo};
    let lockfile = r"C:\Users\me\AppData\Local\Riot Games\Riot Client\Config\lockfile".to_string();
    let mut agent_select = AppStatus::initial(lockfile.clone());
    agent_select.conn = ConnState::Ok;
    agent_select.riot_client_version = Some("release-11.08-shipping-9-3104338".into());
    agent_select.game_version = Some("release-11.08".into());
    agent_select.last_data_ms = Some(SEEN_MS);
    agent_select.pregame = Some(PregameInfo {
        phase_ends_ms: Some(SEEN_MS + 42_500),
        locked: 3,
        total: 5,
    });
    let mut queueing = agent_select.clone();
    queueing.pregame = None;
    queueing.queue = Some(QueueInfo {
        queue_id: "competitive".into(),
        searching_since_ms: SEEN_MS - 65_000,
    });
    // Back in the Lobby after a match: the recap points at it (until the
    // next queue, which clears it).
    let mut after_match = agent_select.clone();
    after_match.pregame = None;
    after_match.recap_match_id = Some("aaaa0000-1111-2222-3333-444455556666".into());
    let mut agent_select_no_timer = agent_select.clone();
    agent_select_no_timer.pregame = Some(PregameInfo {
        phase_ends_ms: None,
        locked: 0,
        total: 5,
    });
    let mut lost = agent_select.clone();
    lost.conn = ConnState::Lost;
    lost.pregame = None;
    lost.attempt = 2;
    lost.retry_in_ms = Some(2_000);
    let mut error = AppStatus::initial(lockfile);
    error.conn = ConnState::Error;
    error.error_code = Some("LOCKFILE_NOT_FOUND".into());
    error.error_at_ms = Some(SEEN_MS);
    error.retry_in_ms = Some(5_000);
    serde_json::to_value([
        agent_select,
        queueing,
        after_match,
        agent_select_no_timer,
        lost,
        error,
    ])
    .unwrap()
}

#[test]
fn app_status_matches_golden() {
    check_golden("app_status", &app_statuses_value());
}

/// The profile drawer's extras: a map row as My Stats builds it, and the
/// "in your match" chips of two recent matches.
fn profile_extras_value() -> Value {
    use opd1_core::model::StatRow;
    use opd1_core::wire::{LiveMates, ProfileExtras};
    let extras = ProfileExtras {
        puuid: "bbbbbbbb-0000-0000-0000-000000000002".into(),
        maps: vec![
            StatRow {
                id: SPLIT_UUID.into(),
                games: 4,
                wins: 3,
                losses: 1,
                winrate: 0.75,
                net_rr: Some(38),
                kd: Some(1.5),
                acs: Some(212.5),
            },
            StatRow {
                id: ASCENT_UUID.into(),
                games: 1,
                wins: 0,
                losses: 0,
                winrate: 0.0,
                net_rr: None,
                kd: Some(0.5),
                acs: None,
            },
        ],
        live_mates: vec![
            LiveMates {
                match_id: "aaaa0000-1111-2222-3333-444455556666".into(),
                puuids: vec![
                    "aaaaaaaa-0000-0000-0000-000000000001".into(),
                    "eeeeeeee-0000-0000-0000-000000000005".into(),
                ],
            },
            LiveMates {
                match_id: "aaaa0000-1111-2222-3333-777788889999".into(),
                puuids: vec!["eeeeeeee-0000-0000-0000-000000000005".into()],
            },
        ],
    };
    serde_json::to_value(extras).unwrap()
}

#[test]
fn profile_extras_matches_golden() {
    let extras = profile_extras_value();
    assert!(!extras.to_string().contains("/Game/"));
    check_golden("profile_extras", &extras);
}

#[test]
fn r189_live_wire_keys() {
    let statuses = app_statuses_value();
    assert_eq!(
        keys(&statuses[0]),
        spec(&[
            "conn",
            "riotClientVersion",
            "lastDataMs",
            "attempt",
            "maxAttempts",
            "retryInMs",
            "errorCode",
            "errorAtMs",
            "lockfilePath",
            "pregame",
            "queue",
            "recapMatchId",
            "gameVersion"
        ])
    );
    assert_eq!(
        keys(&statuses[0]["pregame"]),
        spec(&["phaseEndsMs", "locked", "total"])
    );
    assert_eq!(
        keys(&statuses[1]["queue"]),
        spec(&["queueId", "searchingSinceMs"])
    );
    let conns: Vec<&str> = statuses
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["conn"].as_str().unwrap())
        .collect();
    assert_eq!(conns, ["ok", "ok", "ok", "ok", "lost", "error"]);

    let extras = profile_extras_value();
    assert_eq!(keys(&extras), spec(&["puuid", "maps", "liveMates"]));
    assert_eq!(
        keys(&extras["maps"][0]),
        keys(&serde_json::to_value(&my_stats_full()["maps"][0]).unwrap())
    );
    assert_eq!(keys(&extras["liveMates"][0]), spec(&["matchId", "puuids"]));
}

// ---------------------------------------------------------------------
// R189 (backend-system): the Skins screen's `collection`
// ---------------------------------------------------------------------

/// `collection` as the backend sends it (`AssetCache::collection` builds
/// it from the skin catalog): a priced skin, a Champions skin, a melee at
/// twice its edition, and a skin with no content tier (no edition, no
/// price). Names drop their own weapon's name; melee names stay whole.
fn collection_value() -> Value {
    use opd1_core::wire::{Collection, Edition, OwnedSkin, WeaponInfo};
    const VANDAL: &str = "9c82e19d-4575-0200-1a81-3eacf00cf872";
    const MELEE: &str = "2f59173c-4bed-b6c3-2191-dea9b58be9c7";
    let skin =
        |skin_id: &str, level_id: &str, name: &str, weapon_id: &str, edition, vp| OwnedSkin {
            skin_id: skin_id.into(),
            level_id: level_id.into(),
            name: name.into(),
            weapon_id: weapon_id.into(),
            edition,
            vp,
        };
    serde_json::to_value(Collection {
        items: vec![
            skin(
                "30388628-42f0-606c-82c0-73ad43de997f",
                "578e9077-4f88-260c-e54c-b988425c60e4",
                "Reaver",
                VANDAL,
                Some(Edition::Premium),
                Some(1775),
            ),
            skin(
                "11111111-0000-0000-0000-000000000002",
                "22222222-0000-0000-0000-000000000002",
                "Champions 2025",
                VANDAL,
                Some(Edition::Exclusive),
                Some(2675),
            ),
            skin(
                "11111111-0000-0000-0000-000000000003",
                "22222222-0000-0000-0000-000000000003",
                "Battle Pass",
                VANDAL,
                None,
                None,
            ),
            skin(
                "11111111-0000-0000-0000-000000000004",
                "22222222-0000-0000-0000-000000000004",
                "Reaver Karambit",
                MELEE,
                Some(Edition::Premium),
                Some(3550),
            ),
        ],
        weapons: vec![
            WeaponInfo {
                id: VANDAL.into(),
                name: "Vandal".into(),
            },
            WeaponInfo {
                id: MELEE.into(),
                name: "Melee".into(),
            },
        ],
        fetched_ms: SEEN_MS,
    })
    .unwrap()
}

#[test]
fn collection_matches_its_golden() {
    check_golden("collection", &collection_value());
}

#[test]
fn collection_wire_keys() {
    let c = collection_value();
    assert_eq!(keys(&c), spec(&["items", "weapons", "fetchedMs"]));
    assert_eq!(
        keys(&c["items"][0]),
        spec(&["skinId", "levelId", "name", "weaponId", "edition", "vp"])
    );
    assert_eq!(keys(&c["weapons"][0]), spec(&["id", "name"]));
    // No tier: both `null`, never absent.
    assert_eq!(c["items"][2]["edition"], Value::Null);
    assert_eq!(c["items"][2]["vp"], Value::Null);
    assert_eq!(c["items"][0]["edition"], json!("premium"));
}
