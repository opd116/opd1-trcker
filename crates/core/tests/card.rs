//! Tests for `opd1_core::model`: the R23 tri-state [`Field<T>`] contract
//! and the pure [`build_card`]. No DB, no network — every input is a
//! plain `opd1_core` value.

use opd1_core::db::repo::{CompUpdateRow, MatchPlayerRow};
use opd1_core::model::{
    build_card, BuildCardInput, CurrentRank, Field, Identity, MatchResultKind, MmrInput, Team,
};
use opd1_core::stats::Act;

const CURRENT_ACT_ID: &str = "act-current";
const PREV_ACT_ID: &str = "act-prev";

fn acts() -> Vec<Act> {
    vec![
        Act {
            id: PREV_ACT_ID.to_string(),
            name: "ACT IV".to_string(),
            episode_name: "V26".to_string(),
            start_ms: 1_000,
            end_ms: 2_000,
        },
        Act {
            id: CURRENT_ACT_ID.to_string(),
            name: "ACT V".to_string(),
            episode_name: "V26".to_string(),
            start_ms: 2_000,
            end_ms: 3_000,
        },
    ]
}

fn identity() -> Identity {
    Identity {
        puuid: "puuid-1".to_string(),
        incognito: false,
        hide_account_level: false,
        account_level: 187,
        card_id: "card-1".to_string(),
        character_id: "agent-1".to_string(),
        character_selection_state: "locked".to_string(),
        game_name: "Shanky".to_string(),
        tag_line: "OPD1".to_string(),
    }
}

fn match_row(
    match_id: &str,
    start_ms: i64,
    team_id: &str,
    winning_team: Option<&str>,
) -> MatchPlayerRow {
    MatchPlayerRow {
        match_id: match_id.to_string(),
        map_id: "map-1".to_string(),
        start_ms,
        winning_team: winning_team.map(str::to_string),
        team_id: team_id.to_string(),
        party_id: None,
        agent_id: Some("agent-1".to_string()),
        kills: 20,
        deaths: 10,
        assists: 4,
        score: 4000,
        rounds: Some(24),
        hs: 30,
        bs: 60,
        ls: 10,
        game_name: Some("Shanky".to_string()),
        tag_line: Some("OPD1".to_string()),
    }
}

fn base_input(identity: Identity) -> BuildCardInput<'static> {
    // Leaked once per test call — tests are short-lived processes, and
    // this avoids fighting `ordered_acts: &'a [Act]`'s lifetime with a
    // locally-owned Vec that would need to outlive the returned struct.
    let acts: &'static [Act] = Box::leak(acts().into_boxed_slice());
    BuildCardInput {
        identity,
        mmr: Field::Missing,
        comp_updates: Field::Missing,
        last5: Field::Missing,
        ordered_acts: acts,
        current_act_id: CURRENT_ACT_ID,
        now_ms: 2_500,
        team: Team::Ally,
        is_self_party: false,
        remembered: None,
        party: None,
        encounters: None,
    }
}

// ---------------------------------------------------------------------
// The brief's four named tests
// ---------------------------------------------------------------------

#[test]
fn card_builds_with_partial_data() {
    let mut input = base_input(identity());
    input.last5 = Field::Value(vec![match_row("m1", 100, "Blue", Some("Blue"))]);

    let card = build_card(input);

    // No MMR -> every MMR-derived field is known-missing...
    assert_eq!(card.rank, Field::Missing);
    assert_eq!(card.prev_act_tier, Field::Missing);
    assert_eq!(card.peak, Field::Missing);
    assert_eq!(card.act_winrate, Field::Missing);

    // ...but everything else (identity-derived, last5-derived) is present.
    assert_eq!(card.level, Field::Value(187));
    assert_eq!(card.agent_id.as_deref(), Some("agent-1"));
    assert_eq!(card.kd, Field::Value(2.0));
    match card.hs_pct {
        Field::Value(v) => assert!((v - 0.3).abs() < 1e-6),
        other => panic!("expected hsPct to be present, got {other:?}"),
    }
    match &card.last5 {
        Field::Value(entries) => assert_eq!(entries.len(), 1),
        other => panic!("expected last5 to be present, got {other:?}"),
    }
}

#[test]
fn card_hides_agent_until_selected() {
    let mut not_selected = identity();
    not_selected.character_selection_state = String::new();
    let card = build_card(base_input(not_selected));
    assert_eq!(card.agent_id, None);

    for state in ["selected", "locked"] {
        let mut selected = identity();
        selected.character_selection_state = state.to_string();
        let card = build_card(base_input(selected));
        assert_eq!(card.agent_id.as_deref(), Some("agent-1"));
    }
}

#[test]
fn last5_newest_first() {
    let mut input = base_input(identity());
    // Deliberately out of order on input.
    input.last5 = Field::Value(vec![
        match_row("older", 100, "Blue", Some("Blue")),
        match_row("newest", 300, "Blue", Some("Red")),
        match_row("middle", 200, "Blue", None),
    ]);

    let card = build_card(input);

    let entries = match card.last5 {
        Field::Value(entries) => entries,
        other => panic!("expected last5 to be present, got {other:?}"),
    };
    let start_times: Vec<i64> = entries.iter().map(|e| e.start_ms).collect();
    assert_eq!(start_times, vec![300, 200, 100]);
    assert_eq!(entries[0].result, MatchResultKind::Loss); // newest: Blue lost to Red
    assert_eq!(entries[1].result, MatchResultKind::Draw); // middle: no winner
    assert_eq!(entries[2].result, MatchResultKind::Win); // older: Blue won
}

#[test]
fn hide_account_level_gives_none() {
    let mut hidden = identity();
    hidden.hide_account_level = true;
    let card = build_card(base_input(hidden));
    assert_eq!(card.level, Field::Missing);

    let mut shown = identity();
    shown.hide_account_level = false;
    let card = build_card(base_input(shown));
    assert_eq!(card.level, Field::Value(187));
}

// ---------------------------------------------------------------------
// Extra coverage: rrEarned joins comp_updates by match_id; MMR present
// resolves rank/peak/prevActTier/actWinrate; Loading propagates.
// ---------------------------------------------------------------------

#[test]
fn last5_rr_earned_joins_comp_updates_by_match_id() {
    let mut input = base_input(identity());
    input.last5 = Field::Value(vec![match_row("m1", 100, "Blue", Some("Blue"))]);
    input.comp_updates = Field::Value(vec![CompUpdateRow {
        match_id: "m1".to_string(),
        season_id: CURRENT_ACT_ID.to_string(),
        tier_after: 15,
        tier_before: Some(14),
        rr_after: 42,
        rr_earned: 24,
        map_id: Some("map-1".to_string()),
        start_ms: 100,
    }]);

    let card = build_card(input);

    match card.last5 {
        Field::Value(entries) => assert_eq!(entries[0].rr_earned, Some(24)),
        other => panic!("expected last5 to be present, got {other:?}"),
    }
}

#[test]
fn card_resolves_rank_peak_prev_act_tier_and_winrate_from_mmr() {
    use opd1_core::stats::SeasonInfo;
    use std::collections::HashMap;

    let mut seasons = HashMap::new();
    seasons.insert(
        PREV_ACT_ID.to_string(),
        SeasonInfo {
            competitive_tier: 14,
            wins_by_tier: HashMap::from([(14, 2)]),
            number_of_wins: 10,
            number_of_games: 20,
        },
    );
    seasons.insert(
        CURRENT_ACT_ID.to_string(),
        SeasonInfo {
            competitive_tier: 15,
            wins_by_tier: HashMap::from([(15, 3)]),
            number_of_wins: 6,
            number_of_games: 10,
        },
    );

    let mut input = base_input(identity());
    input.mmr = Field::Value(MmrInput {
        seasons,
        current_rank: Some(CurrentRank { tier: 15, rr: 42 }),
    });

    let card = build_card(input);

    assert_eq!(
        card.rank,
        Field::Value(opd1_core::model::RankInfo { tier: 15, rr: 42 })
    );
    assert_eq!(card.prev_act_tier, Field::Value(14));
    match card.act_winrate {
        Field::Value(v) => assert!((v - 0.6).abs() < 1e-6),
        other => panic!("expected actWinrate to be present, got {other:?}"),
    }
    match card.peak {
        Field::Value(peak) => assert_eq!(peak.tier, 15),
        other => panic!("expected peak to be present, got {other:?}"),
    }
}

#[test]
fn card_mmr_loading_propagates_to_dependent_fields() {
    let mut input = base_input(identity());
    input.mmr = Field::Loading;
    let card = build_card(input);

    assert_eq!(card.rank, Field::Loading);
    assert_eq!(card.prev_act_tier, Field::Loading);
    assert_eq!(card.peak, Field::Loading);
    assert_eq!(card.act_winrate, Field::Loading);
}

// ---------------------------------------------------------------------
// R23 tri-state serialization: the JSON shape of all three states.
// ---------------------------------------------------------------------

#[derive(serde::Serialize)]
struct FieldWrapper {
    #[serde(skip_serializing_if = "Field::is_loading")]
    value: Field<i32>,
}

#[test]
fn tri_state_loading_serializes_with_key_absent() {
    let json = serde_json::to_value(FieldWrapper {
        value: Field::Loading,
    })
    .unwrap();
    assert_eq!(json, serde_json::json!({}));
}

#[test]
fn tri_state_missing_serializes_as_null() {
    let json = serde_json::to_value(FieldWrapper {
        value: Field::Missing,
    })
    .unwrap();
    assert_eq!(json, serde_json::json!({ "value": null }));
}

#[test]
fn tri_state_value_serializes_as_the_value() {
    let json = serde_json::to_value(FieldWrapper {
        value: Field::Value(42),
    })
    .unwrap();
    assert_eq!(json, serde_json::json!({ "value": 42 }));
}

#[test]
fn player_card_json_reflects_all_three_states_together() {
    // Loading mmr -> rank/peak/prevActTier/actWinrate keys absent from the
    // wire, while identity-derived fields still serialize normally.
    let card_loading = build_card({
        let mut i = base_input(identity());
        i.mmr = Field::Loading;
        i
    });
    let json = serde_json::to_value(&card_loading).unwrap();
    let obj = json.as_object().unwrap();
    assert!(
        !obj.contains_key("rank"),
        "loading rank must be absent, got {json}"
    );
    assert!(
        !obj.contains_key("peak"),
        "loading peak must be absent, got {json}"
    );
    assert!(!obj.contains_key("prevActTier"));
    assert!(!obj.contains_key("actWinrate"));
    assert_eq!(obj.get("level"), Some(&serde_json::json!(187)));

    // Missing mmr -> those same keys are present, but null.
    let card_missing = build_card(base_input(identity()));
    let json = serde_json::to_value(&card_missing).unwrap();
    assert_eq!(json["rank"], serde_json::Value::Null);
    assert_eq!(json["peak"], serde_json::Value::Null);
    assert_eq!(json["prevActTier"], serde_json::Value::Null);
    assert_eq!(json["actWinrate"], serde_json::Value::Null);
}

// ---------------------------------------------------------------------
// Fix round 1, item 1: DisplayName field-name casing (rename_all_fields).
// ---------------------------------------------------------------------

#[test]
fn display_name_json_covers_all_four_variants() {
    use opd1_core::model::DisplayName;

    assert_eq!(
        serde_json::to_value(DisplayName::Visible {
            name: "Shanky".to_string(),
            tag: "OPD1".to_string(),
        })
        .unwrap(),
        serde_json::json!({ "kind": "visible", "name": "Shanky", "tag": "OPD1" })
    );
    assert_eq!(
        serde_json::to_value(DisplayName::Remembered {
            name: "Rival".to_string(),
            tag: "ENEMY".to_string(),
        })
        .unwrap(),
        serde_json::json!({ "kind": "remembered", "name": "Rival", "tag": "ENEMY" })
    );
    assert_eq!(
        serde_json::to_value(DisplayName::HiddenPlayer).unwrap(),
        serde_json::json!({ "kind": "hiddenPlayer" })
    );
    // The bug this test guards: `rename_all` alone renames only the
    // variant tag, not this variant's own `agent_id` field — without
    // `rename_all_fields`, this would serialize as `"agent_id"`.
    assert_eq!(
        serde_json::to_value(DisplayName::HiddenAgent {
            agent_id: "agent-1".to_string(),
        })
        .unwrap(),
        serde_json::json!({ "kind": "hiddenAgent", "agentId": "agent-1" })
    );
}

// ---------------------------------------------------------------------
// Fix round 1, item 2 / R34: incognito -> Field<DisplayName>, and the
// hidden player's real name/tag must never appear anywhere in the card's
// JSON, even defensively (identity carrying a stale name for some reason
// must not leak it).
// ---------------------------------------------------------------------

#[test]
fn incognito_before_agent_select_serializes_as_hidden_player() {
    let mut hidden = identity();
    hidden.incognito = true;
    hidden.character_selection_state = String::new();

    let card = build_card(base_input(hidden));
    let json = serde_json::to_value(&card).unwrap();

    assert_eq!(json["name"], serde_json::json!({ "kind": "hiddenPlayer" }));
}

#[test]
fn incognito_after_lock_serializes_as_hidden_agent_with_agent_id() {
    let mut hidden = identity();
    hidden.incognito = true;
    hidden.character_selection_state = "locked".to_string();
    hidden.character_id = "agent-1".to_string();

    let card = build_card(base_input(hidden));
    let json = serde_json::to_value(&card).unwrap();

    assert_eq!(
        json["name"],
        serde_json::json!({ "kind": "hiddenAgent", "agentId": "agent-1" })
    );
}

#[test]
fn hidden_player_json_never_contains_their_game_name_or_tag_anywhere() {
    // Defensive: even if `identity` somehow still carries a real name for
    // an incognito player (it shouldn't — Riot's pregame/coregame identity
    // doesn't include one for other players — but CLAUDE.md's "never...
    // from any endpoint" deserves a belt-and-suspenders check here), it
    // must never reach the wire.
    let mut hidden = identity();
    hidden.incognito = true;
    hidden.game_name = "ShouldNeverAppear".to_string();
    hidden.tag_line = "SECRET".to_string();

    let card = build_card(base_input(hidden));
    let json_string = serde_json::to_string(&card).unwrap();

    assert!(!json_string.contains("ShouldNeverAppear"));
    assert!(!json_string.contains("SECRET"));
}

#[test]
fn visible_player_name_is_loading_until_both_game_name_and_tag_are_known() {
    let mut not_yet_resolved = identity();
    not_yet_resolved.game_name = String::new();
    not_yet_resolved.tag_line = String::new();

    let card = build_card(base_input(not_yet_resolved));
    assert_eq!(card.name, Field::Loading);

    let json = serde_json::to_value(&card).unwrap();
    assert!(
        !json.as_object().unwrap().contains_key("name"),
        "a still-loading name must be an absent key, got {json}"
    );

    // Once both halves resolve, it becomes a real value — never an empty
    // Visible{}.
    let resolved = identity(); // game_name "Shanky", tag_line "OPD1"
    let card = build_card(base_input(resolved));
    match card.name {
        Field::Value(opd1_core::model::DisplayName::Visible { name, tag }) => {
            assert_eq!(name, "Shanky");
            assert_eq!(tag, "OPD1");
        }
        other => panic!("expected a resolved Visible name, got {other:?}"),
    }
}

// ---------------------------------------------------------------------
// Fix round 1, R35: future acts are clipped before prev-act/peak logic;
// last5 is capped at 5 even if more rows are passed in.
// ---------------------------------------------------------------------

#[test]
fn future_acts_are_clipped_before_prev_act_tier_and_peak() {
    use opd1_core::stats::SeasonInfo;
    use std::collections::HashMap;

    const FUTURE_ACT_ID: &str = "act-future";

    // Same acts as `acts()`, plus one more that starts *after* `now_ms`
    // (valorant-api really does list these — the act after the current
    // one is published months ahead of its own start date).
    let mut all_acts = acts();
    all_acts.push(Act {
        id: FUTURE_ACT_ID.to_string(),
        name: "ACT VI".to_string(),
        episode_name: "V26".to_string(),
        start_ms: 10_000, // now_ms below is 2_500 — this act hasn't started
        end_ms: 11_000,
    });
    let leaked: &'static [Act] = Box::leak(all_acts.into_boxed_slice());

    let mut seasons = HashMap::new();
    seasons.insert(
        PREV_ACT_ID.to_string(),
        SeasonInfo {
            competitive_tier: 14,
            wins_by_tier: HashMap::from([(14, 2)]),
            number_of_wins: 10,
            number_of_games: 20,
        },
    );
    seasons.insert(
        CURRENT_ACT_ID.to_string(),
        SeasonInfo {
            competitive_tier: 15,
            wins_by_tier: HashMap::from([(15, 3)]),
            number_of_wins: 6,
            number_of_games: 10,
        },
    );

    let mut input = base_input(identity());
    input.ordered_acts = leaked; // now has 3 entries, future one last
    input.mmr = Field::Value(MmrInput {
        seasons,
        current_rank: Some(CurrentRank { tier: 15, rr: 42 }),
    });

    let card = build_card(input);

    // Without clipping, `ordered_acts`'s *last* element (the future act)
    // would be treated as "current", making CURRENT_ACT look like the
    // "previous" one — this must still resolve to PREV_ACT's tier (14).
    assert_eq!(card.prev_act_tier, Field::Value(14));
}

#[test]
fn last5_is_capped_at_five_even_with_more_rows_passed_in() {
    let mut input = base_input(identity());
    input.last5 = Field::Value(vec![
        match_row("m1", 100, "Blue", Some("Blue")),
        match_row("m2", 200, "Blue", Some("Blue")),
        match_row("m3", 300, "Blue", Some("Blue")),
        match_row("m4", 400, "Blue", Some("Blue")),
        match_row("m5", 500, "Blue", Some("Blue")),
        match_row("m6", 600, "Blue", Some("Blue")),
        match_row("m7", 700, "Blue", Some("Blue")),
    ]);

    let card = build_card(input);

    match card.last5 {
        Field::Value(entries) => {
            assert_eq!(entries.len(), 5);
            let start_times: Vec<i64> = entries.iter().map(|e| e.start_ms).collect();
            assert_eq!(start_times, vec![700, 600, 500, 400, 300]);
        }
        other => panic!("expected last5 to be present, got {other:?}"),
    }
}

// ---------------------------------------------------------------------
// Task 6: remembered names, party marks, encounters
// ---------------------------------------------------------------------

fn remembered(name: &str, tag: &str) -> Option<(String, String)> {
    Some((name.to_string(), tag.to_string()))
}

#[test]
fn incognito_card_shows_a_remembered_name_before_and_after_pick() {
    use opd1_core::model::DisplayName;

    for state in ["", "locked"] {
        let mut hidden = identity();
        hidden.incognito = true;
        hidden.character_selection_state = state.to_string();
        // A stale name on identity must still never be used.
        hidden.game_name = "ShouldNeverAppear".to_string();
        let mut input = base_input(hidden);
        input.remembered = remembered("Rival", "ENEMY");

        let card = build_card(input);
        assert_eq!(
            card.name,
            Field::Value(DisplayName::Remembered {
                name: "Rival".into(),
                tag: "ENEMY".into()
            }),
            "selection state {state:?}"
        );
        assert!(!serde_json::to_string(&card)
            .unwrap()
            .contains("ShouldNeverAppear"));
    }
}

#[test]
fn visible_card_ignores_a_remembered_name() {
    use opd1_core::model::DisplayName;

    let mut input = base_input(identity());
    input.remembered = remembered("Old", "NAME");
    assert_eq!(
        build_card(input).name,
        Field::Value(DisplayName::Visible {
            name: "Shanky".into(),
            tag: "OPD1".into()
        })
    );

    // Still loading stays loading: a remembered name never stands in for a
    // visible player's pending Riot ID.
    let mut pending = identity();
    pending.game_name = String::new();
    let mut input = base_input(pending);
    input.remembered = remembered("Old", "NAME");
    assert_eq!(build_card(input).name, Field::Loading);
}

#[test]
fn card_carries_party_mark_and_encounters_for_other_players() {
    use opd1_core::model::{EncounterCount, PartyMark};

    let mut input = base_input(identity());
    input.party = Some(PartyMark {
        group: 1,
        confirmed: true,
    });
    input.encounters = Some(EncounterCount {
        with: 2,
        against: 3,
    });
    let card = build_card(input);
    assert_eq!(
        card.party,
        Some(PartyMark {
            group: 1,
            confirmed: true
        })
    );
    assert_eq!(
        card.encounters,
        Some(EncounterCount {
            with: 2,
            against: 3
        })
    );
    let json = serde_json::to_value(&card).unwrap();
    assert_eq!(
        json["party"],
        serde_json::json!({"group": 1, "confirmed": true})
    );
    assert_eq!(
        json["encounters"],
        serde_json::json!({"with": 2, "against": 3})
    );
}

#[test]
fn self_party_card_never_gets_party_mark_or_encounters() {
    use opd1_core::model::{EncounterCount, PartyMark};

    let mut input = base_input(identity());
    input.is_self_party = true;
    input.party = Some(PartyMark {
        group: 0,
        confirmed: false,
    });
    input.encounters = Some(EncounterCount {
        with: 9,
        against: 0,
    });
    let card = build_card(input);
    assert_eq!(card.party, None);
    assert_eq!(card.encounters, None);
}
