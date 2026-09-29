//! Tests for `opd1_riot::to_core`: Riot's/valorant-api's wire shapes
//! turned into plain `opd1_core` types, checked against the same fixtures
//! `endpoints_fixtures.rs` uses (provenance in task-3-report.md) plus
//! `opd1-core`'s `seasons.json` fixture.

use opd1_riot::endpoints::{CompetitiveUpdatesResponse, MatchDetailsResponse, PlayerMmrResponse};
use opd1_riot::to_core::{
    acts_from_seasons, comp_update_rows_from_response, current_rank_from_mmr, match_from_details,
    seasonal_map_from_mmr, SeasonEntry,
};

const SEASON_ID: &str = "3e47230a-463c-a301-eb7d-67bb60357d4f";

fn fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("missing fixture {name}: {e}"))
}

fn core_fixture(name: &str) -> String {
    let path = format!(
        "{}/../core/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("missing core fixture {name}: {e}"))
}

// ---------------------------------------------------------------------
// MMR -> SeasonalMap + current rank
// ---------------------------------------------------------------------

#[test]
fn seasonal_map_from_mmr_reads_the_mmr_fixture() {
    let resp: PlayerMmrResponse = serde_json::from_str(&fixture("mmr.json")).unwrap();
    let seasons = seasonal_map_from_mmr(&resp);

    let info = seasons.get(SEASON_ID).expect("season present");
    assert_eq!(info.competitive_tier, 15);
    assert_eq!(info.number_of_wins, 34);
    assert_eq!(info.number_of_games, 61);
    assert_eq!(info.wins_by_tier.get(&15), Some(&20));
    assert_eq!(info.wins_by_tier.get(&14), Some(&10));
    assert_eq!(info.wins_by_tier.get(&13), Some(&4));
}

#[test]
fn seasonal_map_counts_placement_wins() {
    // `NumberOfWinsWithPlacements` (39) is what the act win rate counts:
    // `NumberOfWins` (34) leaves the placement games out.
    let resp: PlayerMmrResponse = serde_json::from_str(&fixture("mmr_placements.json")).unwrap();
    let info = seasonal_map_from_mmr(&resp)
        .remove(SEASON_ID)
        .expect("season present");
    assert_eq!(info.number_of_wins, 39);
    assert_eq!(info.number_of_games, 61);
}

#[test]
fn seasonal_map_falls_back_without_placements() {
    // No `NumberOfWinsWithPlacements` key: `NumberOfWins` stands in.
    let resp: PlayerMmrResponse = serde_json::from_str(&fixture("mmr_no_placements.json")).unwrap();
    let info = seasonal_map_from_mmr(&resp)
        .remove(SEASON_ID)
        .expect("season present");
    assert_eq!(info.number_of_wins, 34);
    assert_eq!(info.number_of_games, 61);
}

#[test]
fn seasonal_map_from_mmr_empty_when_no_competitive_queue() {
    let resp: PlayerMmrResponse =
        serde_json::from_str(&fixture("mmr_no_queue_skills.json")).unwrap();
    assert!(seasonal_map_from_mmr(&resp).is_empty());
}

#[test]
fn seasonal_map_from_mmr_empty_when_seasonal_info_is_null() {
    let resp: PlayerMmrResponse = serde_json::from_str(&fixture("mmr_nulls.json")).unwrap();
    assert!(seasonal_map_from_mmr(&resp).is_empty());
}

#[test]
fn current_rank_from_mmr_resolves_tier_and_rr() {
    let resp: PlayerMmrResponse = serde_json::from_str(&fixture("mmr.json")).unwrap();
    let rank = current_rank_from_mmr(&resp, SEASON_ID).expect("rank present");
    assert_eq!(rank.tier, 15);
    assert_eq!(rank.rr, 42);
}

#[test]
fn current_rank_from_mmr_none_when_season_absent() {
    let resp: PlayerMmrResponse = serde_json::from_str(&fixture("mmr.json")).unwrap();
    assert!(current_rank_from_mmr(&resp, "no-such-season").is_none());
}

// ---------------------------------------------------------------------
// competitive updates -> CompUpdateRow
// ---------------------------------------------------------------------

#[test]
fn comp_update_rows_from_response_converts_every_match() {
    let resp: CompetitiveUpdatesResponse =
        serde_json::from_str(&fixture("comp_updates.json")).unwrap();
    let rows = comp_update_rows_from_response(&resp);

    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].match_id, "1a2b3c4d-0000-0000-0000-000000000010");
    assert_eq!(rows[0].tier_after, 15);
    assert_eq!(rows[0].tier_before, Some(14));
    assert_eq!(rows[0].rr_after, 42);
    assert_eq!(rows[0].rr_earned, 24);
    assert_eq!(rows[0].map_id.as_deref(), Some("/Game/Maps/Ascent/Ascent"));

    assert_eq!(rows[1].rr_earned, -22);
    assert_eq!(rows[1].tier_before, Some(14));
}

// ---------------------------------------------------------------------
// match-details -> Match + MatchTeam + MatchPlayer
// ---------------------------------------------------------------------

#[test]
fn match_from_details_aggregates_hs_bs_ls_and_resolves_winner() {
    let details: MatchDetailsResponse =
        serde_json::from_str(&fixture("match_details.json")).unwrap();
    let (m, teams, players) = match_from_details(&details);

    assert_eq!(m.match_id, "1a2b3c4d-0000-0000-0000-000000000010");
    assert_eq!(m.map_id, "/Game/Maps/Ascent/Ascent");
    assert_eq!(m.queue_id, "competitive");
    assert_eq!(m.winning_team.as_deref(), Some("Blue"));
    assert!(m.is_ranked);
    assert_eq!(m.rounds_played, Some(24));

    assert_eq!(teams.len(), 2);
    let blue = teams.iter().find(|t| t.team_id == "Blue").unwrap();
    assert!(blue.won);
    assert_eq!(blue.rounds_won, Some(13));
    let red = teams.iter().find(|t| t.team_id == "Red").unwrap();
    assert!(!red.won);
    assert_eq!(red.rounds_won, Some(11));

    assert_eq!(players.len(), 2);
    let me = players
        .iter()
        .find(|p| p.puuid == "aaaaaaaa-0000-0000-0000-000000000001")
        .unwrap();
    assert_eq!(me.team_id, "Blue");
    assert_eq!(
        me.party_id.as_deref(),
        Some("cccccccc-0000-0000-0000-000000000001")
    );
    assert_eq!(
        me.agent_id.as_deref(),
        Some("5f8d3a7f-467b-97f3-062c-13acf203c006")
    );
    assert_eq!(me.kills, 22);
    assert_eq!(me.deaths, 15);
    assert_eq!(me.assists, 6);
    assert_eq!(me.score, 4800);
    assert_eq!(me.rounds, Some(24));
    // Fixture's one round result: my one damage entry against the enemy is
    // 1 headshot, 1 bodyshot, 0 legshots.
    assert_eq!((me.hs, me.bs, me.ls), (1, 1, 0));
    assert_eq!(me.game_name.as_deref(), Some("Shanky"));
    assert_eq!(me.tag_line.as_deref(), Some("OPD1"));

    let enemy = players
        .iter()
        .find(|p| p.puuid == "bbbbbbbb-0000-0000-0000-000000000001")
        .unwrap();
    assert_eq!((enemy.hs, enemy.bs, enemy.ls), (0, 2, 0));
}

#[test]
fn match_from_details_handles_null_stats_teams_and_round_results() {
    let details: MatchDetailsResponse =
        serde_json::from_str(&fixture("match_details_nulls.json")).unwrap();
    let (m, teams, players) = match_from_details(&details);

    // matchInfo itself (present even in this "nulls" fixture) drives these
    // now, not caller-supplied overrides.
    assert_eq!(m.match_id, "1a2b3c4d-0000-0000-0000-000000000010");
    assert_eq!(m.map_id, "/Game/Maps/Ascent/Ascent");
    assert_eq!(m.queue_id, "competitive");
    assert_eq!(m.start_ms, 1_758_970_000_000);
    assert_eq!(m.winning_team, None); // no teams at all -> no winner
    assert!(m.is_ranked); // matchInfo.isRanked: true in this fixture too
    assert_eq!(m.rounds_played, None); // no player had `stats`
    assert!(teams.is_empty());

    assert_eq!(players.len(), 1);
    let p = &players[0];
    assert_eq!((p.kills, p.deaths, p.assists, p.score), (0, 0, 0, 0));
    assert_eq!(p.rounds, None);
    assert_eq!((p.hs, p.bs, p.ls), (0, 0, 0));
    assert_eq!(
        p.party_id.as_deref(),
        Some("cccccccc-0000-0000-0000-000000000001")
    );
}

// ---------------------------------------------------------------------
// Fix round 1, item 3: matchInfo drives Match now, including the
// isRanked/completionState fields no caller-supplied param could give us,
// and both documented casings of matchId/queueId are accepted.
// ---------------------------------------------------------------------

#[test]
fn is_ranked_false_derives_directly_from_match_info() {
    let mut value: serde_json::Value =
        serde_json::from_str(&fixture("match_details.json")).unwrap();
    value["matchInfo"]["isRanked"] = serde_json::json!(false);
    value["matchInfo"]["queueId"] = serde_json::json!("unrated");
    let details: MatchDetailsResponse = serde_json::from_value(value).unwrap();

    let (m, _teams, _players) = match_from_details(&details);
    assert!(!m.is_ranked);
    assert_eq!(m.queue_id, "unrated");
}

#[test]
fn vote_draw_completion_state_forces_no_winner_even_if_teams_disagree() {
    let mut value: serde_json::Value =
        serde_json::from_str(&fixture("match_details.json")).unwrap();
    value["matchInfo"]["completionState"] = serde_json::json!("VoteDraw");
    // `teams` still (spuriously) reports Blue as the winner — completionState
    // must win the cross-check.
    let details: MatchDetailsResponse = serde_json::from_value(value).unwrap();

    let (m, _teams, _players) = match_from_details(&details);
    assert_eq!(m.winning_team, None);
}

#[test]
fn match_info_accepts_techchrism_documented_casing_as_well_as_the_fixtures() {
    // techchrism's valorant-api-types (`MatchDetails.ts`) documents
    // `matchId`/`queueID`; the Task 3 fixtures use `matchID`/`queueId`.
    // Neither source is a live capture, so both must decode.
    let value = serde_json::json!({
        "matchInfo": {
            "matchId": "1a2b3c4d-0000-0000-0000-000000000010",
            "mapId": "/Game/Maps/Ascent/Ascent",
            "gameStartMillis": 1_758_970_000_000i64,
            "queueID": "competitive",
            "isRanked": true,
            "completionState": "Completed",
        },
        "players": [],
        "teams": [],
        "roundResults": [],
    });
    let details: MatchDetailsResponse = serde_json::from_value(value).unwrap();
    assert_eq!(
        details.match_info.match_id,
        "1a2b3c4d-0000-0000-0000-000000000010"
    );
    assert_eq!(details.match_info.queue_id, "competitive");
}

// ---------------------------------------------------------------------
// valorant-api /v1/seasons -> ordered Vec<Act>
// ---------------------------------------------------------------------

#[test]
fn acts_from_seasons_builds_ordered_acts_from_the_seasons_fixture() {
    let seasons: Vec<SeasonEntry> = serde_json::from_str(&core_fixture("seasons.json")).unwrap();
    let acts = acts_from_seasons(&seasons);

    // 5 acts in the fixture: ACT III/E4, ACT I/E5, ACT III/E8, ACT IV/V26,
    // ACT V/V26 — "Closed Beta"/"EPISODE 4"/"EPISODE 5"/"EPISODE 8"/"V26"
    // themselves are episodes (parentUuid null), not acts.
    assert_eq!(acts.len(), 5);

    // Sorted ascending by start_ms.
    for pair in acts.windows(2) {
        assert!(pair[0].start_ms <= pair[1].start_ms);
    }

    let v26_act_v = acts
        .iter()
        .find(|a| a.id == "8102cd81-43a0-d0d7-bd59-47b8fe9bed1b")
        .unwrap();
    assert_eq!(v26_act_v.name, "ACT V");
    assert_eq!(v26_act_v.episode_name, "V26");
    assert_eq!(opd1_core::stats::act_label(v26_act_v), "V26 A5");
    assert_eq!(opd1_core::stats::act_full_name(v26_act_v), "V26 ACT V");

    let old_style = acts
        .iter()
        .find(|a| a.id == "3e47230a-463c-a301-eb7d-67bb60357d4f")
        .unwrap();
    assert_eq!(old_style.episode_name, "EPISODE 4");
    assert_eq!(opd1_core::stats::act_label(old_style), "E4 A3");
}

#[test]
fn acts_from_seasons_skips_episodes_and_entries_with_no_parent() {
    let seasons: Vec<SeasonEntry> = serde_json::from_str(&core_fixture("seasons.json")).unwrap();
    let acts = acts_from_seasons(&seasons);
    // "Closed Beta" has parentUuid: null and type: null -> never an act.
    assert!(!acts.iter().any(|a| a.name == "Closed Beta"));
}
