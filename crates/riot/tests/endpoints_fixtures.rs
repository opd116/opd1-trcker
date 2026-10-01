//! Deserialization tests: every typed response struct against a
//! real-shaped fixture. Fixture provenance is listed in
//! `.superpowers/sdd/plan/task-3-report.md`.
//!
//! These tests never touch the network — they only prove our `serde`
//! structs line up with the JSON shapes documented by techchrism's
//! valorant-api-docs.

use opd1_riot::endpoints::{
    CompetitiveUpdatesResponse, CoreGameMatchResponse, MatchDetailsResponse, MatchHistoryResponse,
    MatchRef, NameServiceEntry, PartyOfResponse, PartyResponse, PlayerLoadoutResponse,
    PlayerMmrResponse, PregameLoadoutsResponse, PregameMatchResponse, PresencesResponse,
};

fn fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("missing fixture {name}: {e}"))
}

#[test]
fn pregame_match_deserializes() {
    let parsed: PregameMatchResponse =
        serde_json::from_str(&fixture("pregame_match.json")).unwrap();
    assert_eq!(parsed.map_id, "/Game/Maps/Ascent/Ascent");
    assert_eq!(parsed.teams.len(), 1);
    assert_eq!(parsed.teams[0].team_id, "Blue");
    assert_eq!(parsed.teams[0].players.len(), 2);

    let me = &parsed.teams[0].players[0];
    assert_eq!(me.subject, "aaaaaaaa-0000-0000-0000-000000000001");
    assert_eq!(me.character_id, "5f8d3a7f-467b-97f3-062c-13acf203c006");
    assert_eq!(me.character_selection_state, "selected");
    assert!(!me.player_identity.incognito);
    assert_eq!(me.player_identity.account_level, 187);
    assert_eq!(
        me.player_identity.player_card_id,
        "1711d20d-4b1c-c64a-14be-d4ae58a457c6"
    );

    let hidden = &parsed.teams[0].players[1];
    assert!(hidden.player_identity.incognito);
    assert!(hidden.player_identity.hide_account_level);

    // R161: each player's current tier rides along (Riot's `CompetitiveTier`).
    assert_eq!(me.competitive_tier, Some(15));
    assert_eq!(hidden.competitive_tier, Some(18));

    let ally = parsed.ally_team.expect("AllyTeam should be present");
    assert_eq!(ally.players.len(), 1);
    assert!(parsed.enemy_team.is_none());
}

#[test]
fn coregame_match_deserializes() {
    let parsed: CoreGameMatchResponse =
        serde_json::from_str(&fixture("coregame_match.json")).unwrap();
    assert_eq!(parsed.map_id, "/Game/Maps/Ascent/Ascent");
    assert_eq!(parsed.players.len(), 2);

    let blue = parsed.players.iter().find(|p| p.team_id == "Blue").unwrap();
    assert_eq!(blue.subject, "aaaaaaaa-0000-0000-0000-000000000001");
    assert!(!blue.player_identity.incognito);

    let red = parsed.players.iter().find(|p| p.team_id == "Red").unwrap();
    assert!(red.player_identity.incognito);
    assert!(red.player_identity.hide_account_level);
}

#[test]
fn match_details_deserializes() {
    let parsed: MatchDetailsResponse =
        serde_json::from_str(&fixture("match_details.json")).unwrap();
    assert_eq!(parsed.players.len(), 2);

    let me = parsed
        .players
        .iter()
        .find(|p| p.subject == "aaaaaaaa-0000-0000-0000-000000000001")
        .unwrap();
    assert_eq!(me.game_name, "Shanky");
    assert_eq!(me.tag_line, "OPD1");
    assert_eq!(me.team_id, "Blue");
    assert_eq!(me.party_id, "cccccccc-0000-0000-0000-000000000001");
    assert_eq!(me.character_id, "5f8d3a7f-467b-97f3-062c-13acf203c006");
    let stats = me.stats.as_ref().expect("stats present in this fixture");
    assert_eq!(stats.kills, 22);
    assert_eq!(stats.deaths, 15);
    assert_eq!(stats.assists, 6);
    assert_eq!(stats.score, 4800);
    assert_eq!(stats.rounds_played, 24);

    let blue_team = parsed.teams.iter().find(|t| t.team_id == "Blue").unwrap();
    assert!(blue_team.won);
    let red_team = parsed.teams.iter().find(|t| t.team_id == "Red").unwrap();
    assert!(!red_team.won);

    // Phase 2 fields (techchrism `MatchDetails.ts`): the game length, each
    // player's rank during the match, and who won which round.
    assert_eq!(parsed.match_info.game_length_millis, Some(2_280_000));
    assert_eq!(me.competitive_tier, Some(15));
    let rounds: Vec<(Option<i64>, Option<&str>)> = parsed
        .round_results
        .iter()
        .map(|r| (r.round_num, r.winning_team.as_deref()))
        .collect();
    assert_eq!(
        rounds,
        [
            (Some(0), Some("Blue")),
            (Some(11), Some("Red")),
            (Some(12), Some("Blue"))
        ]
    );
    assert_eq!(parsed.round_results.len(), 3);
    let round = &parsed.round_results[0];
    let my_round_stats = round
        .player_stats
        .iter()
        .find(|p| p.subject == "aaaaaaaa-0000-0000-0000-000000000001")
        .unwrap();
    let damage_to_enemy = my_round_stats
        .damage
        .iter()
        .find(|d| d.receiver == "bbbbbbbb-0000-0000-0000-000000000001")
        .unwrap();
    assert_eq!(damage_to_enemy.headshots, 1);
    assert_eq!(damage_to_enemy.bodyshots, 1);
    assert_eq!(damage_to_enemy.legshots, 0);
}

#[test]
fn match_details_deserializes_when_stats_teams_and_round_results_are_null() {
    // `players[].stats`, `teams`, and `roundResults` are all `.nullable()`
    // in techchrism's `valorant-api-types` (`MatchDetails.ts`) — none of
    // them should fail the decode when Riot actually sends `null`
    // (fix-round item 2).
    let parsed: MatchDetailsResponse =
        serde_json::from_str(&fixture("match_details_nulls.json")).unwrap();
    assert_eq!(parsed.players.len(), 1);
    assert_eq!(parsed.players[0].stats, None);
    assert!(parsed.teams.is_empty());
    assert!(parsed.round_results.is_empty());
    // None of the Phase 2 fields is in this payload: all default to `None`.
    assert_eq!(parsed.match_info.game_length_millis, None);
    assert_eq!(parsed.players[0].competitive_tier, None);
}

#[test]
fn mmr_deserializes() {
    let parsed: PlayerMmrResponse = serde_json::from_str(&fixture("mmr.json")).unwrap();
    assert_eq!(parsed.subject, "aaaaaaaa-0000-0000-0000-000000000001");

    let competitive = parsed.queue_skills.get("competitive").unwrap();
    let season = competitive
        .seasonal_info_by_season_id
        .get("3e47230a-463c-a301-eb7d-67bb60357d4f")
        .unwrap();
    assert_eq!(season.competitive_tier, 15);
    assert_eq!(season.ranked_rating, Some(42));
    assert_eq!(season.number_of_wins, 34);
    let wins_by_tier = season.wins_by_tier.as_ref().unwrap();
    assert_eq!(wins_by_tier.get("15"), Some(&20));

    let unrated = parsed.queue_skills.get("unrated").unwrap();
    assert!(unrated.seasonal_info_by_season_id.is_empty());
}

#[test]
fn mmr_deserializes_when_seasonal_info_by_season_id_is_null() {
    // Scout null-guards this exact field
    // (`(...).get("SeasonalInfoBySeasonID")) or {}` in `live_match.py`) —
    // `null` must become an empty map, not a decode failure (fix-round
    // item 2).
    let parsed: PlayerMmrResponse = serde_json::from_str(&fixture("mmr_nulls.json")).unwrap();
    let competitive = parsed.queue_skills.get("competitive").unwrap();
    assert!(competitive.seasonal_info_by_season_id.is_empty());
}

#[test]
fn mmr_deserializes_when_queue_skills_itself_is_null() {
    // Scout null-guards `QueueSkills` too (`r.get("QueueSkills") or {}`) —
    // same treatment one level up.
    let parsed: PlayerMmrResponse =
        serde_json::from_str(&fixture("mmr_no_queue_skills.json")).unwrap();
    assert!(parsed.queue_skills.is_empty());
}

#[test]
fn comp_updates_deserializes() {
    let parsed: CompetitiveUpdatesResponse =
        serde_json::from_str(&fixture("comp_updates.json")).unwrap();
    assert_eq!(parsed.subject, "aaaaaaaa-0000-0000-0000-000000000001");
    assert_eq!(parsed.matches.len(), 2);
    assert_eq!(parsed.matches[0].ranked_rating_earned, 24);
    assert_eq!(parsed.matches[0].tier_after_update, 15);
    assert_eq!(parsed.matches[1].ranked_rating_earned, -22);
    assert_eq!(parsed.matches[1].map_id, "/Game/Maps/Bonsai/Bonsai");
}

#[test]
fn pregame_and_coregame_player_share_match_ref_shape() {
    let pregame: MatchRef = serde_json::from_str(&fixture("pregame_player.json")).unwrap();
    assert_eq!(pregame.subject, "aaaaaaaa-0000-0000-0000-000000000001");
    assert_eq!(pregame.match_id, "1a2b3c4d-0000-0000-0000-000000000010");

    let coregame: MatchRef = serde_json::from_str(&fixture("coregame_player.json")).unwrap();
    assert_eq!(coregame.match_id, "1a2b3c4d-0000-0000-0000-000000000010");
}

#[test]
fn match_history_deserializes() {
    let parsed: MatchHistoryResponse =
        serde_json::from_str(&fixture("match_history.json")).unwrap();
    assert_eq!(parsed.subject, "aaaaaaaa-0000-0000-0000-000000000001");
    assert_eq!(parsed.history.len(), 2);
    assert_eq!(parsed.history[0].queue_id, "competitive");
    assert_eq!(parsed.history[0].game_start_time, 1758970000000);
    // The page holds 2 of the 214 matches: `Total` is the whole history.
    assert_eq!(parsed.total, Some(214));
}

#[test]
fn match_history_without_total_still_deserializes() {
    // A payload that lacks (or nulls) `Total` must not fail the decode: the
    // backfill then falls back to "a short page is the last one".
    let bare = r#"{"Subject": "s", "History": []}"#;
    let parsed: MatchHistoryResponse = serde_json::from_str(bare).unwrap();
    assert_eq!(parsed.total, None);
    let null = r#"{"Subject": "s", "Total": null, "History": []}"#;
    let parsed: MatchHistoryResponse = serde_json::from_str(null).unwrap();
    assert_eq!(parsed.total, None);
}

#[test]
fn party_deserializes() {
    let parsed: PartyResponse = serde_json::from_str(&fixture("party.json")).unwrap();
    assert_eq!(parsed.id, "cccccccc-0000-0000-0000-000000000001");
    assert_eq!(parsed.state.as_deref(), Some("DEFAULT"));
    assert_eq!(parsed.members.len(), 1);
    assert_eq!(parsed.members[0].is_owner, Some(true));
    assert_eq!(parsed.members[0].is_ready, Some(true));
    // R161: a member's current tier rides along.
    assert_eq!(parsed.members[0].competitive_tier, Some(15));
    assert!(parsed.eligible_queues.contains(&"competitive".to_string()));
}

#[test]
fn party_deserializes_when_unlisted_optional_fields_are_missing() {
    // `State`/`EligibleQueues`/`IsReady` aren't in the brief's field list
    // and aren't consumed by this crate yet — an absent value must not
    // blank the whole `PartyResponse` (fix-round item 2).
    let value = serde_json::json!({
        "ID": "cccccccc-0000-0000-0000-000000000001",
        "Members": [{
            "Subject": "aaaaaaaa-0000-0000-0000-000000000001",
            "PlayerIdentity": {
                "Incognito": false,
                "HideAccountLevel": false,
                "AccountLevel": 187,
                "PlayerCardID": "1711d20d-4b1c-c64a-14be-d4ae58a457c6",
            },
        }],
    });
    let parsed: PartyResponse = serde_json::from_value(value).unwrap();
    assert_eq!(parsed.state, None);
    assert!(parsed.eligible_queues.is_empty());
    assert_eq!(parsed.members[0].is_ready, None);
    assert_eq!(parsed.members[0].is_owner, None);
}

#[test]
fn party_of_deserializes() {
    let parsed: PartyOfResponse = serde_json::from_str(&fixture("party_of.json")).unwrap();
    assert_eq!(parsed.subject, "aaaaaaaa-0000-0000-0000-000000000001");
    assert_eq!(
        parsed.current_party_id,
        "cccccccc-0000-0000-0000-000000000001"
    );
}

#[test]
fn presences_deserializes() {
    let parsed: PresencesResponse = serde_json::from_str(&fixture("presences.json")).unwrap();
    assert_eq!(parsed.presences.len(), 1);
    assert_eq!(
        parsed.presences[0].puuid,
        "aaaaaaaa-0000-0000-0000-000000000001"
    );
    assert_eq!(parsed.presences[0].product, "valorant");
    assert!(parsed.presences[0].private.is_some());
}

#[test]
fn names_deserializes() {
    let parsed: Vec<NameServiceEntry> = serde_json::from_str(&fixture("names.json")).unwrap();
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0].game_name, "Shanky");
    assert_eq!(parsed[0].tag_line, "OPD1");
    assert_eq!(parsed[0].display_name.as_deref(), Some("Shanky#OPD1"));
}

#[test]
fn names_deserializes_when_display_name_is_missing() {
    // `DisplayName` isn't in the brief's field list (`GameName`/`TagLine`
    // already give the Riot ID display) — an absent value must not fail
    // the whole `names()` decode (fix-round item 2).
    let value = serde_json::json!([{
        "Subject": "aaaaaaaa-0000-0000-0000-000000000001",
        "GameName": "Shanky",
        "TagLine": "OPD1",
    }]);
    let parsed: Vec<NameServiceEntry> = serde_json::from_value(value).unwrap();
    assert_eq!(parsed[0].display_name, None);
}

#[test]
fn player_loadout_fixture_parses() {
    // R90, the v3 shape of `personalization/v3/.../playerloadout` (Riot moved
    // it from v2; `Guns[]` is unchanged): `Subject`, `Guns[].{ID, SkinID,
    // ChromaID}`. The other keys (`ActiveExpressions`, `DynamicOptions`,
    // `Identity`, charms ...) are ignored. The fixture carries the new keys
    // and no `Sprays`.
    let raw = fixture("player_loadout.json");
    assert!(raw.contains("\"ActiveExpressions\""), "v3 fixture shape");
    assert!(!raw.contains("\"Sprays\""), "v3 fixture has no Sprays");
    let parsed: PlayerLoadoutResponse = serde_json::from_str(&raw).unwrap();
    assert_eq!(parsed.subject, "49e4dd9b-f2b5-4155-96a6-9ae7af3dd95d");
    assert_eq!(parsed.guns.len(), 18, "the fixture's gun count (knife too)");
    for gun in &parsed.guns {
        assert!(!gun.id.is_empty(), "{gun:?}");
        assert!(!gun.skin_id.is_empty(), "{gun:?}");
        assert!(!gun.chroma_id.is_empty(), "{gun:?}");
    }
    // The four weapons the app reads carry the real weapon ids (the Sheriff
    // in upper case, to prove matching ignores case).
    let ids: Vec<&str> = parsed.guns.iter().take(4).map(|g| g.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "9c82e19d-4575-0200-1a81-3eacf00cf872",
            "ee8e8d15-496b-07ac-e5f6-8fae5d4c7b1a",
            "a03b24d3-4319-996d-0f8c-94bbfba1dfc7",
            "E336C6B8-418D-9340-D77F-7A9E4CFE0702",
        ]
    );
}

#[test]
fn player_loadout_v2_shaped_body_still_parses() {
    // The old v2 body (`Sprays[]` instead of `ActiveExpressions[]`, no
    // `DynamicOptions`) decodes to the same guns: only `Guns[]` is read.
    let parsed: PlayerLoadoutResponse = serde_json::from_str(
        r#"{
            "Subject": "p",
            "Guns": [{"ID": "g", "SkinID": "s", "SkinLevelID": "l", "ChromaID": "c",
                      "Attachments": []}],
            "Sprays": [{"EquipSlotID": "x", "SprayID": "y", "SprayLevelID": null}],
            "Identity": {"PlayerCardID": "c"},
            "Incognito": false
        }"#,
    )
    .unwrap();
    assert_eq!(parsed.subject, "p");
    assert_eq!(parsed.guns.len(), 1);
    assert_eq!(parsed.guns[0].skin_id, "s");
    assert_eq!(parsed.guns[0].chroma_id, "c");
}

#[test]
fn player_loadout_v3_ignores_unknown_and_new_fields() {
    // v3 adds `ActiveExpressions[{TypeID, AssetID}]` and `DynamicOptions`;
    // whatever else Riot adds later must not break the decode either.
    let parsed: PlayerLoadoutResponse = serde_json::from_str(
        r#"{
            "Subject": "p",
            "Guns": [{"ID": "g", "SkinID": "s", "ChromaID": "c", "Attachments": [],
                      "SomethingNew": {"a": 1}}],
            "ActiveExpressions": [{"TypeID": "t", "AssetID": "a", "Extra": 1}],
            "DynamicOptions": [{"any": "thing"}],
            "BrandNewTopLevelKey": [1, 2, 3]
        }"#,
    )
    .unwrap();
    assert_eq!(parsed.guns.len(), 1);
    assert_eq!(parsed.guns[0].id, "g");
}

#[test]
fn player_loadout_without_guns_or_with_null_guns_parses() {
    let parsed: PlayerLoadoutResponse =
        serde_json::from_str(r#"{"Subject": "p", "Guns": null}"#).unwrap();
    assert!(parsed.guns.is_empty());
    let parsed: PlayerLoadoutResponse = serde_json::from_str(r#"{"Subject": "p"}"#).unwrap();
    assert!(parsed.guns.is_empty());
}

#[test]
fn pregame_loadouts_fixture_parses() {
    // Each `Loadouts[]` entry carries `Subject` + `Items` at the top level
    // (no `Loadout` wrapper as in core-game); one entry has no `Subject`.
    let parsed: PregameLoadoutsResponse =
        serde_json::from_str(&fixture("pregame_loadouts.json")).unwrap();
    assert!(parsed.loadouts_valid);
    assert_eq!(parsed.loadouts.len(), 3);
    let subjects: Vec<Option<&str>> = parsed
        .loadouts
        .iter()
        .map(|l| l.get("Subject").and_then(|s| s.as_str()))
        .collect();
    assert_eq!(
        subjects,
        [
            Some("a6bbe1f1-a9be-457b-9b58-35db705f7499"),
            Some("b7c1a2d3-4e5f-4a6b-8c7d-9e0f1a2b3c4d"),
            None
        ]
    );
}

#[test]
fn tier_fields_missing_or_null_never_fail_the_response() {
    // R161: `CompetitiveTier` is optional everywhere it is read — an absent
    // key, a `null`, or a stray string leaves the tier unknown, never fails
    // the whole roster answer.
    let identity = serde_json::json!({
        "Incognito": false,
        "HideAccountLevel": false,
        "AccountLevel": 5,
        "PlayerCardID": "card",
    });
    let player = |tier: Option<serde_json::Value>| {
        let mut p = serde_json::json!({
            "Subject": "p",
            "CharacterID": "",
            "CharacterSelectionState": "",
            "PlayerIdentity": identity,
        });
        if let Some(t) = tier {
            p["CompetitiveTier"] = t;
        }
        p
    };
    let pregame = |tier: Option<serde_json::Value>| {
        serde_json::json!({
            "MapID": "m",
            "Teams": [{"TeamID": "Blue", "Players": [player(tier)]}],
            "AllyTeam": null,
            "EnemyTeam": null,
        })
    };
    for (tier, want) in [
        (None, None),
        (Some(serde_json::Value::Null), None),
        (Some(serde_json::json!(0)), Some(0)),
        (Some(serde_json::json!(24)), Some(24)),
        (Some(serde_json::json!("x")), None),
    ] {
        let parsed: PregameMatchResponse = serde_json::from_value(pregame(tier.clone()))
            .unwrap_or_else(|e| panic!("pregame tier {tier:?}: {e}"));
        assert_eq!(
            parsed.teams[0].players[0].competitive_tier, want,
            "{tier:?}"
        );

        let mut member = serde_json::json!({"Subject": "p", "PlayerIdentity": identity});
        if let Some(t) = tier.clone() {
            member["CompetitiveTier"] = t;
        }
        let party = serde_json::json!({"ID": "x", "Members": [member]});
        let parsed: PartyResponse =
            serde_json::from_value(party).unwrap_or_else(|e| panic!("party tier {tier:?}: {e}"));
        assert_eq!(parsed.members[0].competitive_tier, want, "{tier:?}");
    }
}

// ---------------------------------------------------------------------
// content-service (R162). The fixture is invented: there is no real
// content-service dump on disk; the shape follows Scout's parser
// (`ID`/`Type`/`IsActive`/`StartTime`/`EndTime`) and Riot's documented
// `Seasons` list.
// ---------------------------------------------------------------------

use opd1_riot::endpoints::ContentSeasons;
use serde_json::json;

fn content(value: serde_json::Value) -> ContentSeasons {
    ContentSeasons::from_value(&value)
}

const T_APR: &str = "2026-08-19T00:00:00Z";
const T_OCT: &str = "2026-10-14T00:00:00Z";

#[test]
fn content_picks_the_active_act_and_the_one_that_ended_when_it_began() {
    let seasons = content(serde_json::from_str(&fixture("content.json")).unwrap());
    let acts = seasons.active_acts().expect("an active act");
    // `Type` is compared case-insensitively ("Act" in the fixture), and
    // the active *episode* is not the act.
    assert_eq!(acts.current.id, "22222222-0000-0000-0000-00000000a905");
    assert_eq!(acts.current.name, "ACT V");
    assert_eq!(acts.current.episode_name, "V26");
    assert_eq!(acts.current.start_ms, Some(1_787_097_600_000));
    assert_eq!(acts.current.end_ms, Some(1_791_936_000_000));
    let prev = acts.previous.expect("a previous act");
    assert_eq!(prev.id, "22222222-0000-0000-0000-00000000a904");
    assert_eq!(prev.end_ms, acts.current.start_ms);
}

#[test]
fn content_without_an_active_act_is_none() {
    let none = content(json!({"Seasons": [
        {"ID": "e", "Type": "episode", "IsActive": true, "StartTime": T_APR, "EndTime": T_OCT},
        {"ID": "a", "Type": "act", "IsActive": false, "StartTime": T_APR, "EndTime": T_OCT}
    ]}));
    assert!(none.active_acts().is_none());
}

#[test]
fn content_decoding_never_fails_on_odd_shapes() {
    for odd in [
        json!(null),
        json!([]),
        json!("x"),
        json!({}),
        json!({"Seasons": null}),
        json!({"Seasons": "no"}),
        json!({"Seasons": [1, null, "s", [], {"ID": 5, "Type": 7, "IsActive": "yes"}]}),
    ] {
        assert!(content(odd).active_acts().is_none());
    }
    // One bad entry does not hide a good one; missing times stay unknown
    // rather than failing the answer, and the id alone still names the act.
    let seasons = content(json!({"Seasons": [
        42,
        {"ID": "good", "Type": "act", "IsActive": true, "StartTime": "garbage"},
        {"Type": "act", "IsActive": true, "StartTime": T_APR, "EndTime": T_OCT}
    ]}));
    let acts = seasons.active_acts().expect("the good one");
    assert_eq!(acts.current.id, "good");
    assert_eq!(acts.current.start_ms, None);
    assert_eq!(acts.current.end_ms, None);
    assert!(acts.previous.is_none());
}

#[test]
fn content_previous_act_needs_matching_times_not_just_order() {
    let seasons = content(json!({"Seasons": [
        {"ID": "old", "Type": "act", "IsActive": false,
         "StartTime": "2026-01-01T00:00:00Z", "EndTime": "2026-06-01T00:00:00Z"},
        {"ID": "now", "Type": "act", "IsActive": true,
         "StartTime": T_APR, "EndTime": T_OCT}
    ]}));
    assert!(seasons.active_acts().unwrap().previous.is_none());
}

#[test]
fn content_names_a_roman_act_from_riots_digits() {
    // The real entries have exactly ID, Name, Type, StartTime, EndTime and
    // IsActive: the episode is found by time, not by a parent id.
    let seasons = content(json!({"Seasons": [
        {"ID": "p", "Type": "episode", "Name": "Episode 9", "IsActive": true,
         "StartTime": "2026-01-06T00:00:00Z", "EndTime": "2026-12-01T00:00:00Z"},
        {"ID": "a", "Type": "act", "Name": "act 3", "IsActive": true,
         "StartTime": T_APR, "EndTime": T_OCT}
    ]}));
    let cur = seasons.active_acts().unwrap().current;
    assert_eq!(cur.name, "ACT III");
    assert_eq!(cur.episode_name, "EPISODE 9", "act_label shortens it to E9");
}

#[test]
fn content_episode_is_the_one_containing_the_acts_start() {
    let seasons = content(json!({"Seasons": [
        // Ended exactly when the act began: not the act's episode.
        {"ID": "e1", "Type": "episode", "Name": "EPISODE 8",
         "StartTime": "2026-01-01T00:00:00Z", "EndTime": T_APR},
        // Two containing entries: the shorter span wins.
        {"ID": "e0", "Type": "episode", "Name": "WIDE",
         "StartTime": "2025-01-01T00:00:00Z", "EndTime": "2027-01-01T00:00:00Z"},
        {"ID": "e2", "Type": "episode", "Name": "  V27 ",
         "StartTime": T_APR, "EndTime": "2026-12-01T00:00:00Z"},
        {"ID": "a", "Type": "act", "Name": "ACT VI", "IsActive": true,
         "StartTime": T_APR, "EndTime": T_OCT}
    ]}));
    assert_eq!(seasons.active_acts().unwrap().current.episode_name, "V27");

    // No episode contains it: empty, never a guess.
    let none = content(json!({"Seasons": [
        {"ID": "e", "Type": "episode", "Name": "EPISODE 1",
         "StartTime": "2020-01-01T00:00:00Z", "EndTime": "2020-06-01T00:00:00Z"},
        {"ID": "a", "Type": "act", "Name": "ACT VI", "IsActive": true,
         "StartTime": T_APR, "EndTime": T_OCT}
    ]}));
    assert_eq!(none.active_acts().unwrap().current.episode_name, "");
}
