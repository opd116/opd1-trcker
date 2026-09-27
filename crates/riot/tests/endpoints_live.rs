//! Integration tests for the `endpoints.rs` typed wrappers: correct
//! method/base/path routing (pd vs glz vs local), query-string building,
//! and the PUT body for `names()`. Response *shapes* are covered by
//! `tests/endpoints_fixtures.rs`; these tests are about wiring, so they
//! mostly return trivial bodies.

mod support;

use serde_json::json;
use wiremock::matchers::{body_json, method, path, query_param, query_param_is_missing};
use wiremock::{Mock, MockServer, ResponseTemplate};

use opd1_riot::endpoints::VisiblePuuid;
use support::{
    connect_against, mount_entitlements, mount_sessions_with_version, write_lockfile,
    write_shooter_log, TempDir, SELF_PUUID,
};

const OTHER_PUUID: &str = "bbbbbbbb-0000-0000-0000-000000000001";
const MATCH_ID: &str = "1a2b3c4d-0000-0000-0000-000000000010";
const PARTY_ID: &str = "cccccccc-0000-0000-0000-000000000001";

async fn setup() -> (TempDir, MockServer, opd1_riot::session::RiotSession) {
    let tmp = TempDir::new();
    write_shooter_log(tmp.path());
    let server = MockServer::start().await;
    write_lockfile(tmp.path(), "http", server.address().port(), "pw");
    mount_entitlements(&server, SELF_PUUID).await;
    mount_sessions_with_version(&server, "v1").await;
    let session = connect_against(tmp.path(), &server).await;
    (tmp, server, session)
}

#[tokio::test]
async fn pregame_player_hits_glz() {
    let (_tmp, server, session) = setup().await;
    Mock::given(method("GET"))
        .and(path(format!("/pregame/v1/players/{SELF_PUUID}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "Subject": SELF_PUUID, "MatchID": MATCH_ID, "Version": 1
        })))
        .mount(&server)
        .await;

    let result = session.pregame_player(SELF_PUUID).await.unwrap();
    assert_eq!(result.subject, SELF_PUUID);
    assert_eq!(result.match_id, MATCH_ID);
}

#[tokio::test]
async fn coregame_match_hits_glz() {
    let (_tmp, server, session) = setup().await;
    Mock::given(method("GET"))
        .and(path(format!("/core-game/v1/matches/{MATCH_ID}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "MapID": "/Game/Maps/Ascent/Ascent",
            "Players": []
        })))
        .mount(&server)
        .await;

    let result = session.coregame_match(MATCH_ID).await.unwrap();
    assert_eq!(result.map_id, "/Game/Maps/Ascent/Ascent");
}

#[tokio::test]
async fn mmr_hits_pd() {
    let (_tmp, server, session) = setup().await;
    Mock::given(method("GET"))
        .and(path(format!("/mmr/v1/players/{SELF_PUUID}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "Subject": SELF_PUUID, "QueueSkills": {}
        })))
        .mount(&server)
        .await;

    let result = session.mmr(SELF_PUUID).await.unwrap();
    assert_eq!(result.subject, SELF_PUUID);
}

#[tokio::test]
async fn comp_updates_passes_start_and_end_index_and_queue_as_query_params() {
    let (_tmp, server, session) = setup().await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/mmr/v1/players/{SELF_PUUID}/competitiveupdates"
        )))
        .and(query_param("startIndex", "0"))
        .and(query_param("endIndex", "20"))
        .and(query_param("queue", "competitive"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "Subject": SELF_PUUID, "Matches": []
        })))
        .mount(&server)
        .await;

    let result = session
        .comp_updates(SELF_PUUID, Some(0), Some(20))
        .await
        .unwrap();
    assert_eq!(result.subject, SELF_PUUID);
}

#[tokio::test]
async fn comp_updates_always_sends_queue_competitive_even_without_start_end() {
    // Ruling R25: without `queue=competitive`, Riot mixes every queue's
    // matches together with no reliable way to filter them apart
    // afterwards, so this must be sent unconditionally — even when the
    // caller doesn't pass `start`/`end`.
    let (_tmp, server, session) = setup().await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/mmr/v1/players/{SELF_PUUID}/competitiveupdates"
        )))
        .and(query_param("queue", "competitive"))
        .and(query_param_is_missing("startIndex"))
        .and(query_param_is_missing("endIndex"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "Subject": SELF_PUUID, "Matches": []
        })))
        .mount(&server)
        .await;

    let result = session.comp_updates(SELF_PUUID, None, None).await.unwrap();
    assert_eq!(result.subject, SELF_PUUID);
}

#[tokio::test]
async fn match_history_passes_queue_filter() {
    let (_tmp, server, session) = setup().await;
    Mock::given(method("GET"))
        .and(path(format!("/match-history/v1/history/{SELF_PUUID}")))
        .and(query_param("queue", "competitive"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "Subject": SELF_PUUID, "BeginIndex": 0, "EndIndex": 0, "Total": 0, "History": []
        })))
        .mount(&server)
        .await;

    let result = session
        .match_history(SELF_PUUID, None, None, Some("competitive"))
        .await
        .unwrap();
    assert_eq!(result.subject, SELF_PUUID);
}

#[tokio::test]
async fn match_details_hits_pd() {
    let (_tmp, server, session) = setup().await;
    Mock::given(method("GET"))
        .and(path(format!("/match-details/v1/matches/{MATCH_ID}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "matchInfo": {
                "matchID": MATCH_ID,
                "mapId": "/Game/Maps/Ascent/Ascent",
                "gameStartMillis": 1_758_970_000_000i64,
                "queueId": "competitive",
                "isRanked": true
            },
            "players": [], "teams": [], "roundResults": []
        })))
        .mount(&server)
        .await;

    let result = session.match_details(MATCH_ID).await.unwrap();
    assert!(result.players.is_empty());
}

#[tokio::test]
async fn party_of_and_party_hit_glz() {
    let (_tmp, server, session) = setup().await;
    Mock::given(method("GET"))
        .and(path(format!("/parties/v1/players/{SELF_PUUID}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "Subject": SELF_PUUID, "Version": 1, "CurrentPartyID": PARTY_ID
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/parties/v1/parties/{PARTY_ID}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "ID": PARTY_ID, "Members": [], "State": "DEFAULT", "EligibleQueues": []
        })))
        .mount(&server)
        .await;

    let party_of = session.party_of(SELF_PUUID).await.unwrap();
    assert_eq!(party_of.current_party_id, PARTY_ID);

    let party = session.party(&party_of.current_party_id).await.unwrap();
    assert_eq!(party.id, PARTY_ID);
}

#[tokio::test]
async fn presences_hits_local_not_pd_or_glz() {
    let (_tmp, server, session) = setup().await;
    Mock::given(method("GET"))
        .and(path("/chat/v4/presences"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "presences": [] })))
        .mount(&server)
        .await;

    let result = session.presences().await.unwrap();
    assert!(result.presences.is_empty());
}

#[tokio::test]
async fn names_puts_the_puuid_list_and_hits_pd() {
    let (_tmp, server, session) = setup().await;
    Mock::given(method("PUT"))
        .and(path("/name-service/v2/players"))
        .and(body_json(json!([SELF_PUUID, OTHER_PUUID])))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            { "DisplayName": "Shanky#OPD1", "Subject": SELF_PUUID, "GameName": "Shanky", "TagLine": "OPD1" },
            { "DisplayName": "Rival#ENEMY", "Subject": OTHER_PUUID, "GameName": "Rival", "TagLine": "ENEMY" },
        ])))
        .mount(&server)
        .await;

    let visible = vec![
        VisiblePuuid::new(SELF_PUUID.to_string(), false).unwrap(),
        VisiblePuuid::new(OTHER_PUUID.to_string(), false).unwrap(),
    ];
    let result = session.names(&visible).await.unwrap();
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].game_name, "Shanky");
    assert_eq!(result[1].game_name, "Rival");
}
