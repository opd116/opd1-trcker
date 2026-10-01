//! Tests for `opd1_core::model`: the R23 tri-state [`Field<T>`] contract
//! and the pure [`build_card`]. No DB, no network — every input is a
//! plain `opd1_core` value.

use opd1_core::db::repo::{CompMatchRow, CompUpdateRow, MatchPlayerRow, StoredTier};
use opd1_core::model::{
    build_card, hero_stats, match_summary, shown_agent, BuildCardInput, CurrentRank, Field,
    Identity, MatchResultKind, MmrInput, RankInfo, RememberedSource, Team,
};
use opd1_core::names::KnownName;
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
        is_self: false,
        is_self_party: false,
        remembered: None,
        party: None,
        encounters: None,
        stored_tier: None,
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
fn card_shows_agent_only_when_locked() {
    // R99: only a locked pick counts. `""` (nothing yet) and `"selected"`
    // (a hover) show no agent; `character_id` being filled changes nothing.
    assert_eq!(shown_agent("agent-1", ""), None);
    assert_eq!(shown_agent("agent-1", "selected"), None);
    assert_eq!(shown_agent("agent-1", "locked"), Some("agent-1"));
    assert_eq!(shown_agent("agent-1", "LOCKED"), Some("agent-1"));
    assert_eq!(shown_agent("", "locked"), None);

    for (state, expected) in [("", None), ("selected", None), ("locked", Some("agent-1"))] {
        let mut id = identity();
        id.character_selection_state = state.to_string();
        let card = build_card(base_input(id));
        assert_eq!(card.agent_id.as_deref(), expected, "state {state:?}");
    }
}

#[test]
fn hovered_agent_is_not_selected() {
    // R99: hovering an agent changes nothing on the card.
    let mut hover = identity();
    hover.character_selection_state = "selected".to_string();
    let card = build_card(base_input(hover.clone()));
    assert_eq!(card.agent_id, None);
    let json = serde_json::to_value(&card).unwrap();
    assert_eq!(json["agentId"], serde_json::Value::Null);

    // A hidden player who only hovers stays `Hidden Player`, not the agent.
    let mut hidden_hover = hover;
    hidden_hover.incognito = true;
    let json = serde_json::to_value(build_card(base_input(hidden_hover))).unwrap();
    assert_eq!(json["name"], serde_json::json!({ "kind": "hiddenPlayer" }));
    assert_eq!(json["agentId"], serde_json::Value::Null);

    // Once locked: the agent shows, and a hidden player shows the agent name.
    let mut locked = identity();
    locked.character_selection_state = "locked".to_string();
    let json = serde_json::to_value(build_card(base_input(locked.clone()))).unwrap();
    assert_eq!(json["agentId"], serde_json::json!("agent-1"));
    locked.incognito = true;
    let json = serde_json::to_value(build_card(base_input(locked))).unwrap();
    assert_eq!(
        json["name"],
        serde_json::json!({ "kind": "hiddenAgent", "agentId": "agent-1" })
    );
    assert_eq!(json["agentId"], serde_json::json!("agent-1"));
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
        Field::Value(RankInfo {
            tier: 15,
            rr: Some(42)
        })
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
fn card_peak_is_never_below_the_current_rank_r164() {
    use opd1_core::stats::SeasonInfo;
    use std::collections::HashMap;

    // Promoted to tier 15 this act, with every win recorded at tier 14.
    let mut seasons = HashMap::new();
    seasons.insert(
        CURRENT_ACT_ID.to_string(),
        SeasonInfo {
            competitive_tier: 15,
            wins_by_tier: HashMap::from([(14, 4)]),
            number_of_wins: 4,
            number_of_games: 6,
        },
    );
    let mut input = base_input(identity());
    input.mmr = Field::Value(MmrInput {
        seasons,
        current_rank: Some(CurrentRank { tier: 15, rr: 3 }),
    });

    let card = build_card(input);

    match card.peak {
        Field::Value(peak) => {
            assert_eq!(peak.tier, 15);
            assert_eq!(peak.since_act_end_ms, None, "the current act: This Act");
        }
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
            source: RememberedSource::OwnMatch,
            from_map_id: None,
            from_start_ms: None,
        })
        .unwrap(),
        serde_json::json!({
            "kind": "remembered", "name": "Rival", "tag": "ENEMY",
            "source": "ownMatch", "fromMapId": null, "fromStartMs": null
        })
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

fn remembered(name: &str, tag: &str) -> Option<KnownName> {
    Some(KnownName {
        name: name.to_string(),
        tag: tag.to_string(),
        map_id: None,
        start_ms: None,
        source: RememberedSource::OwnMatch,
    })
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
                tag: "ENEMY".into(),
                source: RememberedSource::OwnMatch,
                from_map_id: None,
                from_start_ms: None,
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

// ---------------------------------------------------------------------
// Task 14: isSelf, act W/L (with placements), remembered-name source
// ---------------------------------------------------------------------

fn mmr_with_current_act(wins: u32, games: u32) -> Field<MmrInput> {
    use opd1_core::stats::SeasonInfo;
    use std::collections::HashMap;

    Field::Value(MmrInput {
        seasons: HashMap::from([(
            CURRENT_ACT_ID.to_string(),
            SeasonInfo {
                competitive_tier: 15,
                wins_by_tier: HashMap::from([(15, 3)]),
                number_of_wins: wins,
                number_of_games: games,
            },
        )]),
        current_rank: Some(CurrentRank { tier: 15, rr: 42 }),
    })
}

#[test]
fn card_is_self_true_only_for_self() {
    let mut mine = base_input(identity());
    mine.is_self = true;
    let json = serde_json::to_value(build_card(mine)).unwrap();
    assert_eq!(json["isSelf"], serde_json::json!(true));

    let json = serde_json::to_value(build_card(base_input(identity()))).unwrap();
    assert_eq!(json["isSelf"], serde_json::json!(false));
}

#[test]
fn act_wins_games_tri_state() {
    // Loading: both keys absent, like every other MMR-derived field.
    let mut input = base_input(identity());
    input.mmr = Field::Loading;
    let json = serde_json::to_value(build_card(input)).unwrap();
    let obj = json.as_object().unwrap();
    assert!(!obj.contains_key("actWins"), "got {json}");
    assert!(!obj.contains_key("actGames"), "got {json}");

    // Missing MMR: present and null.
    let json = serde_json::to_value(build_card(base_input(identity()))).unwrap();
    assert_eq!(json["actWins"], serde_json::Value::Null);
    assert_eq!(json["actGames"], serde_json::Value::Null);

    // MMR without a current-act entry: null too, and so is the win rate.
    let mut input = base_input(identity());
    input.mmr = Field::Value(MmrInput {
        seasons: std::collections::HashMap::new(),
        current_rank: None,
    });
    let json = serde_json::to_value(build_card(input)).unwrap();
    assert_eq!(json["actWins"], serde_json::Value::Null);
    assert_eq!(json["actGames"], serde_json::Value::Null);
    assert_eq!(json["actWinrate"], serde_json::Value::Null);

    // A current act with 39 wins (placements included) in 61 games.
    let mut input = base_input(identity());
    input.mmr = mmr_with_current_act(39, 61);
    let json = serde_json::to_value(build_card(input)).unwrap();
    assert_eq!(json["actWins"], serde_json::json!(39));
    assert_eq!(json["actGames"], serde_json::json!(61));
    let rate = json["actWinrate"].as_f64().unwrap();
    assert!((rate - 39.0 / 61.0).abs() < 1e-3, "actWinrate {rate}");
}

#[test]
fn act_wins_games_null_when_no_games() {
    let mut input = base_input(identity());
    input.mmr = mmr_with_current_act(0, 0);
    let json = serde_json::to_value(build_card(input)).unwrap();
    assert_eq!(json["actWins"], serde_json::Value::Null);
    assert_eq!(json["actGames"], serde_json::Value::Null);
    assert_eq!(json["actWinrate"], serde_json::Value::Null);
}

#[test]
fn remembered_name_serializes_source() {
    let mut hidden = identity();
    hidden.incognito = true;

    let mut input = base_input(hidden.clone());
    input.remembered = Some(KnownName {
        name: "Rival".into(),
        tag: "ENEMY".into(),
        map_id: Some("uuid-split".into()),
        start_ms: Some(1_234),
        source: RememberedSource::OwnMatch,
    });
    let json = serde_json::to_value(build_card(input)).unwrap();
    assert_eq!(
        json["name"],
        serde_json::json!({
            "kind": "remembered", "name": "Rival", "tag": "ENEMY",
            "source": "ownMatch", "fromMapId": "uuid-split", "fromStartMs": 1234
        })
    );

    // Without a source both keys are still there, as null.
    let mut input = base_input(hidden);
    input.remembered = remembered("Rival", "ENEMY");
    let json = serde_json::to_value(build_card(input)).unwrap();
    let name = json["name"].as_object().unwrap();
    assert_eq!(name["fromMapId"], serde_json::Value::Null);
    assert_eq!(name["fromStartMs"], serde_json::Value::Null);
}

#[test]
fn remembered_source_kind_reaches_the_wire_camel_case() {
    // R145: the UI tells where a hidden player's name came from.
    let mut hidden = identity();
    hidden.incognito = true;
    for (source, wire, map) in [
        (RememberedSource::OwnMatch, "ownMatch", Some("uuid-split")),
        (
            RememberedSource::OtherMatch,
            "otherMatch",
            Some("uuid-split"),
        ),
        // A name-service answer has no map.
        (RememberedSource::Seen, "seen", None),
    ] {
        let mut input = base_input(hidden.clone());
        input.remembered = Some(KnownName {
            name: "Rival".into(),
            tag: "ENEMY".into(),
            map_id: map.map(str::to_string),
            start_ms: Some(1_234),
            source,
        });
        let json = serde_json::to_value(build_card(input)).unwrap();
        assert_eq!(json["name"]["kind"], "remembered", "{wire}");
        assert_eq!(json["name"]["source"], wire);
        assert_eq!(
            json["name"]["fromMapId"],
            map.map_or(serde_json::Value::Null, |m| serde_json::json!(m)),
            "{wire}"
        );
        assert_eq!(json["name"]["fromStartMs"], 1_234, "{wire}");
    }
}

// ---------------------------------------------------------------------
// Task 15: MatchSummary.tier (R81) and the no-stats match (R102)
// ---------------------------------------------------------------------

/// A stored competitive match of the player: 20/10/5, 4000 score over 20
/// rounds (ACS 200), 10/30/10 shots (HS 20%).
fn comp_row() -> CompMatchRow {
    CompMatchRow {
        match_id: "m1".into(),
        map_id: "map-uuid".into(),
        start_ms: 1_000,
        winning_team: Some("Blue".into()),
        team_id: "Blue".into(),
        agent_id: Some("agent-jett".into()),
        kills: 20,
        deaths: 10,
        assists: 5,
        score: 4000,
        rounds: Some(20),
        hs: 10,
        bs: 30,
        ls: 10,
        rounds_won: Some(13),
        rounds_lost: Some(7),
        rr_earned: Some(18),
        season_id: Some("act".into()),
        tier_before: Some(14),
        tier_after: Some(15),
        tier: Some(14),
    }
}

#[test]
fn match_summary_tier_below_3_is_null() {
    let tier_of = |tier: Option<i64>| {
        let summary = match_summary(&CompMatchRow { tier, ..comp_row() });
        serde_json::to_value(summary).unwrap()["tier"].clone()
    };
    assert_eq!(tier_of(Some(15)), serde_json::json!(15));
    assert_eq!(tier_of(Some(3)), serde_json::json!(3));
    // Below 3 is unranked (placements, unrated queues): no rank shown.
    for unranked in [Some(0), Some(1), Some(2), None] {
        assert_eq!(tier_of(unranked), serde_json::Value::Null, "{unranked:?}");
    }
    // A value no tier table has never reaches the wire as a wrapped u8.
    assert_eq!(tier_of(Some(300)), serde_json::Value::Null);
    assert_eq!(tier_of(Some(-4)), serde_json::Value::Null);
}

#[test]
fn match_summary_without_a_stats_row_is_null_not_zero() {
    // R102: match-details had no stats for the player (stored 0/0/0, no
    // rounds): every stat is unknown, never a counted zero.
    let mut blank = comp_row();
    (blank.kills, blank.deaths, blank.assists, blank.score) = (0, 0, 0, 0);
    blank.rounds = None;
    let json = serde_json::to_value(match_summary(&blank)).unwrap();
    for key in ["kills", "deaths", "assists", "kd", "acs", "hsPct"] {
        assert_eq!(json[key], serde_json::Value::Null, "{key} must be null");
        assert!(json.get(key).is_some(), "{key} is present, not absent");
    }
    // The rest of the row still shows.
    assert_eq!(json["result"], "win");
    assert_eq!(json["roundsWon"], serde_json::json!(13));
    assert_eq!(json["rrEarned"], serde_json::json!(18));

    // A zero round count is the same "no stats" (never a divide by zero).
    let mut zero_rounds = comp_row();
    zero_rounds.rounds = Some(0);
    let json = serde_json::to_value(match_summary(&zero_rounds)).unwrap();
    assert_eq!(json["kills"], serde_json::Value::Null);
    assert_eq!(json["acs"], serde_json::Value::Null);

    // With stats a real zero stays a zero: 0 kills is a counted result.
    let mut winless = comp_row();
    (winless.kills, winless.deaths, winless.assists) = (0, 12, 0);
    let json = serde_json::to_value(match_summary(&winless)).unwrap();
    assert_eq!(json["kills"], serde_json::json!(0));
    assert_eq!(json["assists"], serde_json::json!(0));
    assert_eq!(json["kd"], serde_json::json!(0.0));
}

// ---------------------------------------------------------------------
// R116: Player Details hero K/D and HS% over the act's recent matches
// ---------------------------------------------------------------------

fn stat_row(id: &str, kills: i64, deaths: i64, shots: (i64, i64, i64)) -> CompMatchRow {
    let mut r = comp_row();
    r.match_id = id.into();
    (r.kills, r.deaths) = (kills, deaths);
    (r.hs, r.bs, r.ls) = shots;
    r
}

fn no_stats_row(id: &str) -> CompMatchRow {
    let mut r = stat_row(id, 0, 0, (0, 0, 0));
    r.score = 0;
    r.rounds = None;
    r
}

#[test]
fn hero_stats_pool_kills_deaths_and_shots_over_the_matches() {
    // 30 kills / 20 deaths; 15 headshots of 60 shots.
    let rows = [
        stat_row("a", 20, 10, (10, 30, 10)),
        stat_row("b", 10, 10, (5, 5, 0)),
    ];
    let hero = hero_stats(&rows);
    assert_eq!(hero.matches, 2);
    assert_eq!(hero.kd, Some(1.5), "pooled kills / pooled deaths");
    assert_eq!(hero.hs_pct, Some(0.25), "pooled headshots / pooled shots");
}

#[test]
fn hero_stats_count_only_matches_with_a_stats_row() {
    // R102: a match without stats is no counted zero — it adds neither a
    // death-free kill/death pair nor to the match count.
    let rows = [
        stat_row("a", 20, 10, (10, 30, 10)),
        no_stats_row("blank"),
        stat_row("b", 10, 10, (5, 5, 0)),
    ];
    let hero = hero_stats(&rows);
    assert_eq!(hero.matches, 2);
    assert_eq!(hero.kd, Some(1.5));
    assert_eq!(hero.hs_pct, Some(0.25));
}

#[test]
fn hero_stats_are_null_without_act_matches() {
    let none = hero_stats(&[]);
    assert_eq!((none.kd, none.hs_pct, none.matches), (None, None, 0));
    // Matches exist but none has stats: still nothing to average.
    let blank = hero_stats(&[no_stats_row("x"), no_stats_row("y")]);
    assert_eq!((blank.kd, blank.hs_pct, blank.matches), (None, None, 0));
}

#[test]
fn hero_stats_edge_cases() {
    // No death at all: K/D is the kill count (R5), not infinity.
    let flawless = hero_stats(&[stat_row("a", 25, 0, (10, 10, 5))]);
    assert_eq!(flawless.kd, Some(25.0));
    // Stats but not one shot landed: K/D known, HS% unknown.
    let no_shots = hero_stats(&[stat_row("a", 4, 8, (0, 0, 0))]);
    assert_eq!(
        (no_shots.kd, no_shots.hs_pct, no_shots.matches),
        (Some(0.5), None, 1)
    );
}

// ---------------------------------------------------------------------
// R154: the rank from stored matches when there is no MMR answer
// ---------------------------------------------------------------------

/// One stored `comp_updates` row: `season`, the tier after the match, the
/// RR after it and when the match started.
fn update(id: &str, season: &str, start_ms: i64, tier_after: i64, rr_after: i64) -> CompUpdateRow {
    CompUpdateRow {
        match_id: id.to_string(),
        season_id: season.to_string(),
        tier_after,
        tier_before: Some(tier_after),
        rr_after,
        rr_earned: 10,
        map_id: None,
        start_ms,
    }
}

/// A card of a player with no MMR snapshot (`mmr` as given) and `updates`.
fn no_snapshot(mmr: Field<MmrInput>, updates: Vec<CompUpdateRow>) -> BuildCardInput<'static> {
    let mut input = base_input(identity());
    input.mmr = mmr;
    input.comp_updates = Field::Value(updates);
    input
}

fn rank(tier: u8, rr: Option<u16>) -> Field<RankInfo> {
    Field::Value(RankInfo { tier, rr })
}

#[test]
fn no_snapshot_the_rank_is_the_newest_current_act_comp_update() {
    // The rows come oldest first here on purpose: "newest" is by match
    // start, not by position. An older act's row with a later start is
    // impossible, but one from another act must never win.
    let updates = vec![
        update("a", CURRENT_ACT_ID, 2_100, 14, 90),
        update("c", CURRENT_ACT_ID, 2_400, 15, 42),
        update("b", CURRENT_ACT_ID, 2_300, 14, 99),
        update("old", PREV_ACT_ID, 1_500, 20, 5),
    ];
    let card = build_card(no_snapshot(Field::Missing, updates));
    assert_eq!(
        card.rank,
        rank(15, Some(42)),
        "tier and RR of the newest row"
    );
}

#[test]
fn no_snapshot_the_rank_skips_current_act_rows_that_are_not_ranked() {
    // Placement games end at tier 0..=2: the newest *ranked* row counts.
    let updates = vec![
        update("placement", CURRENT_ACT_ID, 2_400, 2, 0),
        update("ranked", CURRENT_ACT_ID, 2_300, 13, 7),
    ];
    let card = build_card(no_snapshot(Field::Missing, updates));
    assert_eq!(card.rank, rank(13, Some(7)));
}

#[test]
fn no_snapshot_the_rank_shows_while_the_rest_still_loads() {
    // It is cached data: known before the sync ends, so the rank does not
    // wait for it. Everything else that comes from MMR is still loading.
    let card = build_card(no_snapshot(
        Field::Loading,
        vec![update("m", CURRENT_ACT_ID, 2_400, 15, 42)],
    ));
    assert_eq!(card.rank, rank(15, Some(42)));
    assert_eq!(card.prev_act_tier, Field::Loading);
    assert_eq!(card.peak, Field::Loading);
    assert_eq!(card.act_winrate, Field::Loading);
    assert_eq!(card.act_wins, Field::Loading);
    assert_eq!(card.act_games, Field::Loading);
}

#[test]
fn no_snapshot_only_the_rank_is_filled_in_everything_else_stays_as_it_was() {
    let mut input = no_snapshot(
        Field::Missing,
        vec![update("m", CURRENT_ACT_ID, 2_400, 15, 42)],
    );
    input.stored_tier = Some(StoredTier {
        start_ms: 2_400,
        tier: 15,
    });
    let card = build_card(input);
    assert_eq!(card.rank, rank(15, Some(42)));
    assert_eq!(card.prev_act_tier, Field::Missing);
    assert_eq!(card.peak, Field::Missing);
    assert_eq!(card.act_winrate, Field::Missing);
    assert_eq!(card.act_wins, Field::Missing);
    assert_eq!(card.act_games, Field::Missing);
}

#[test]
fn no_snapshot_a_previous_act_row_is_no_rank() {
    // Only last act's rows (and a last act's stored match): nothing says
    // what the player is *now*.
    let mut input = no_snapshot(
        Field::Missing,
        vec![update("old", PREV_ACT_ID, 1_500, 20, 5)],
    );
    input.stored_tier = Some(StoredTier {
        start_ms: 1_800,
        tier: 20,
    });
    assert_eq!(build_card(input).rank, Field::Missing);

    // Still loading: stays a skeleton, not a wrong rank.
    let card = build_card(no_snapshot(
        Field::Loading,
        vec![update("old", PREV_ACT_ID, 1_500, 20, 5)],
    ));
    assert_eq!(card.rank, Field::Loading);
}

#[test]
fn no_snapshot_and_no_comp_update_the_rank_is_the_tier_of_a_stored_match_with_no_rr() {
    // Path (b): the tier Match Details shows, in a match of the current
    // act. The RR is not known.
    let mut input = no_snapshot(Field::Missing, vec![]);
    input.stored_tier = Some(StoredTier {
        start_ms: 2_100,
        tier: 14,
    });
    let card = build_card(input);
    assert_eq!(card.rank, rank(14, None));

    // The act's own start counts as in the act; a match before it does not.
    for (start_ms, shown) in [(2_000, true), (1_999, false)] {
        let mut input = no_snapshot(Field::Missing, vec![]);
        input.stored_tier = Some(StoredTier { start_ms, tier: 14 });
        assert_eq!(
            build_card(input).rank,
            if shown {
                rank(14, None)
            } else {
                Field::Missing
            },
            "match at {start_ms}"
        );
    }
}

#[test]
fn a_comp_update_beats_a_stored_match_tier() {
    let mut input = no_snapshot(
        Field::Missing,
        vec![update("m", CURRENT_ACT_ID, 2_100, 15, 42)],
    );
    input.stored_tier = Some(StoredTier {
        start_ms: 2_400,
        tier: 17,
    });
    assert_eq!(build_card(input).rank, rank(15, Some(42)));
}

#[test]
fn a_stored_match_tier_needs_the_current_act_to_be_known() {
    // No act tables: "the current act" is unknown, so nothing is in it.
    let mut input = no_snapshot(Field::Missing, vec![]);
    input.current_act_id = "";
    input.stored_tier = Some(StoredTier {
        start_ms: 2_400,
        tier: 14,
    });
    assert_eq!(build_card(input).rank, Field::Missing);

    let mut input = no_snapshot(Field::Missing, vec![update("m", "", 2_400, 15, 42)]);
    input.current_act_id = "";
    assert_eq!(
        build_card(input).rank,
        Field::Missing,
        "an unknown act is not a season named \"\""
    );
}

#[test]
fn a_snapshot_is_the_authority_over_newer_stored_rows() {
    // MMR says tier 15 / 42 RR; a newer comp-update row and a stored match
    // say otherwise. The snapshot wins.
    let mmr = Field::Value(MmrInput {
        seasons: std::collections::HashMap::new(),
        current_rank: Some(CurrentRank { tier: 15, rr: 42 }),
    });
    let mut input = no_snapshot(mmr, vec![update("m", CURRENT_ACT_ID, 2_400, 18, 77)]);
    input.stored_tier = Some(StoredTier {
        start_ms: 2_450,
        tier: 19,
    });
    assert_eq!(build_card(input).rank, rank(15, Some(42)));
}

#[test]
fn a_snapshot_with_no_rank_this_act_stays_missing() {
    // MMR answered: the player has no rank this act (placements). That is
    // an answer; stored rows must not contradict it.
    let mmr = Field::Value(MmrInput {
        seasons: std::collections::HashMap::new(),
        current_rank: None,
    });
    let mut input = no_snapshot(mmr, vec![update("m", CURRENT_ACT_ID, 2_400, 18, 77)]);
    input.stored_tier = Some(StoredTier {
        start_ms: 2_450,
        tier: 19,
    });
    assert_eq!(build_card(input).rank, Field::Missing);
}

#[test]
fn the_rank_rr_is_always_on_the_wire_number_or_null() {
    // The key is present either way; `null` is "the tier is known, the RR
    // is not".
    let exact = serde_json::to_value(build_card(no_snapshot(
        Field::Missing,
        vec![update("m", CURRENT_ACT_ID, 2_400, 15, 42)],
    )))
    .unwrap();
    assert_eq!(exact["rank"], serde_json::json!({ "tier": 15, "rr": 42 }));

    let mut input = no_snapshot(Field::Missing, vec![]);
    input.stored_tier = Some(StoredTier {
        start_ms: 2_100,
        tier: 14,
    });
    let unknown = serde_json::to_value(build_card(input)).unwrap();
    assert_eq!(
        unknown["rank"],
        serde_json::json!({ "tier": 14, "rr": null })
    );
}
