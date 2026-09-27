//! Deserialization tests: every typed response struct against a
//! real-shaped fixture. Fixture provenance is listed in
//! `.superpowers/sdd/plan/task-3-report.md`.
//!
//! These tests never touch the network — they only prove our `serde`
//! structs line up with the JSON shapes documented by techchrism's
//! valorant-api-docs.

use opd1_riot::endpoints::{
    CompetitiveUpdatesResponse, CoreGameMatchResponse, MatchDetailsResponse, MatchHistoryResponse,
    MatchRef, NameServiceEntry, PartyOfResponse, PartyResponse, PlayerMmrResponse,
    PregameMatchResponse, PresencesResponse,
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
    assert_eq!(me.stats.kills, 22);
    assert_eq!(me.stats.deaths, 15);
    assert_eq!(me.stats.assists, 6);
    assert_eq!(me.stats.score, 4800);
    assert_eq!(me.stats.rounds_played, 24);

    let blue_team = parsed.teams.iter().find(|t| t.team_id == "Blue").unwrap();
    assert!(blue_team.won);
    let red_team = parsed.teams.iter().find(|t| t.team_id == "Red").unwrap();
    assert!(!red_team.won);

    assert_eq!(parsed.round_results.len(), 1);
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
}

#[test]
fn party_deserializes() {
    let parsed: PartyResponse = serde_json::from_str(&fixture("party.json")).unwrap();
    assert_eq!(parsed.id, "cccccccc-0000-0000-0000-000000000001");
    assert_eq!(parsed.state, "DEFAULT");
    assert_eq!(parsed.members.len(), 1);
    assert_eq!(parsed.members[0].is_owner, Some(true));
    assert!(parsed.members[0].is_ready);
    assert!(parsed.eligible_queues.contains(&"competitive".to_string()));
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
    assert_eq!(parsed[0].display_name, "Shanky#OPD1");
}
