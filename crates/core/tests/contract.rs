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
    PlayerCard, PlayerDetails, RememberedSource, Team, TeammateSummary, WeaponSkin,
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
    };
    check_golden("live_state", &serde_json::to_value(&state).unwrap());
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
        },
        TeammateSummary {
            puuid: "puuid-mate-3".into(),
            name: DisplayName::HiddenAgent {
                agent_id: "agent-sova".into(),
            },
            count: 1,
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
/// and today (a Wednesday, 4 matches within 6h = the session). It is built so that all six insights fire,
/// the ladder promotes and demotes (6 rank changes, 5 sent), and Morning and Night are empty buckets.
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

    let act = ms_act();
    let names = ms_map_names();
    let utc = |_: i64| 0;
    let stats = mystats::build(MyStatsInput {
        act: &act,
        matches: &matches,
        comp_updates: &updates,
        current_rank: Some(CurrentRank {
            tier: tier as u8,
            rr: rr as u16,
        }),
        map_names: &names,
        now_ms: ms_at(30, 19, 0),
        utc_offset_ms: &utc,
    });

    // What the golden is meant to show, so a change to the builder cannot quietly hollow it out.
    assert_eq!(stats.rr_series.len(), 16);
    assert_eq!(stats.rank.changes.len(), 5, "5 of 6 rank changes are sent");
    assert_eq!(stats.rank.rr_to_next, Some(12));
    assert_eq!(stats.rank.net_rr, Some(38));
    assert_eq!(stats.insights.len(), 6, "every insight fires");
    assert_eq!(stats.session.as_ref().map(|s| s.streak.len()), Some(4));
    assert_eq!(stats.last5.len(), 5);
    assert_eq!((stats.maps.len(), stats.agents.len()), (2, 2));
    assert!(stats.by_daypart.iter().any(|c| c.winrate.is_none()));
    serde_json::to_value(&stats).unwrap()
}

/// The edge case: no MMR snapshot (`tier`, `rr` and `rrToNext` null), no stored competitive update (empty
/// `rrSeries`, null net RR and averages, no rank change), a match with no stats row (the whole stat line
/// null, R102) on an unknown map with no agent, a draw, and nothing in the last 6h (`session` null) or enough
/// to say anything (`insights` empty).
fn my_stats_edge() -> Value {
    let row = |id: &str, day: i64, hour: i64, result: Option<&str>, map: &str, agent: Option<&str>| CompMatchRow {
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
    };
    let mut draw = row("edge-draw", 3, 20, None, SPLIT_UUID, Some("agent-jett"));
    (draw.kills, draw.deaths, draw.assists) = (12, 12, 3);
    (draw.score, draw.rounds) = (4320, Some(24));
    (draw.hs, draw.bs, draw.ls) = (8, 24, 8);
    (draw.rounds_won, draw.rounds_lost) = (Some(12), Some(12));
    let matches = vec![
        draw,
        row("edge-bare", 5, 9, Some("Blue"), "", None),
        row("edge-loss", 6, 22, Some("Red"), ASCENT_UUID, Some("agent-omen")),
    ];

    let act = ms_act();
    let names = ms_map_names();
    let utc = |_: i64| 0;
    let stats = mystats::build(MyStatsInput {
        act: &act,
        matches: &matches,
        comp_updates: &[],
        current_rank: None,
        map_names: &names,
        now_ms: ms_at(40, 12, 0),
        utc_offset_ms: &utc,
    });

    assert!(stats.rank.tier.is_none() && stats.rank.rr.is_none() && stats.rank.rr_to_next.is_none());
    assert!(stats.rank.net_rr.is_none() && stats.rank.avg_win.is_none() && stats.rank.avg_loss.is_none());
    assert!(stats.rr_series.is_empty() && stats.rank.changes.is_empty());
    assert!(stats.session.is_none() && stats.insights.is_empty());
    assert_eq!(stats.last5.len(), 3);
    assert!(
        stats.last5.iter().any(|m| m.kills.is_none() && m.acs.is_none()),
        "a match with no stats row"
    );
    serde_json::to_value(&stats).unwrap()
}

#[test]
fn my_stats_matches_golden() {
    check_golden("my_stats", &json!([my_stats_full(), my_stats_edge()]));
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
        spec(&["puuid", "name", "count"])
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
    assert_eq!(keys(&stats["rank"]["changes"][0]), spec(&["tier", "up", "startMs"]));
    assert_eq!(keys(&stats["rrSeries"][0]), spec(&["startMs", "absoluteRr"]));
    assert_eq!(
        keys(&stats["session"]),
        spec(&["netRr", "wins", "losses", "startedMs", "streak"])
    );
    assert_eq!(keys(&stats["last5"][0]), keys(&golden["recent"][0]));
    assert_eq!(
        keys(&stats["maps"][0]),
        spec(&["id", "games", "wins", "losses", "winrate", "netRr", "kd", "acs"])
    );
    assert_eq!(keys(&stats["byWeekday"][0]), spec(&["games", "wins", "winrate"]));
    assert_eq!(stats["byWeekday"].as_array().map(Vec::len), Some(7));
    assert_eq!(stats["byDaypart"].as_array().map(Vec::len), Some(4));
    assert_eq!(
        keys(&stats["insights"][0]),
        spec(&["kind", "text", "matches", "positive"])
    );
}
