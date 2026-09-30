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
    assert_eq!(season.ranked_rating, 42);
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
    // R90, built from techchrism's `player-loadout/response.json` (field
    // names per `PlayerLoadout.ts`): `Subject`, `Guns[].{ID, SkinID,
    // ChromaID}`; the other keys (`Sprays`, `Identity`, charms ...) are
    // ignored.
    let parsed: PlayerLoadoutResponse =
        serde_json::from_str(&fixture("player_loadout.json")).unwrap();
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
