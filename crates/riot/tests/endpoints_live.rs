//! Integration tests for the `endpoints.rs` typed wrappers: correct
//! method/base/path routing (pd vs glz vs local), query-string building,
//! and the PUT body for `names()`. Response *shapes* are covered by
//! `tests/endpoints_fixtures.rs`; these tests are about wiring, so they
//! mostly return trivial bodies. Most share one mock server for every
//! base; `every_endpoint_goes_to_its_own_host` gives each base its own, so
//! a call sent to the wrong host fails there.

mod support;

use serde_json::json;
use wiremock::matchers::{body_json, method, path, query_param, query_param_is_missing};
use wiremock::{Mock, MockServer, ResponseTemplate};

use opd1_riot::endpoints::VisiblePuuid;
use support::{
    connect_against, mount_entitlements, write_lockfile, write_shooter_log, TempDir, SELF_PUUID,
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
    let session = connect_against(tmp.path(), &server).await;
    (tmp, server, session)
}

#[tokio::test]
async fn every_endpoint_goes_to_its_own_host() {
    let tmp = TempDir::new();
    write_shooter_log(tmp.path());
    let (local, pd, glz) = (
        MockServer::start().await,
        MockServer::start().await,
        MockServer::start().await,
    );
    write_lockfile(tmp.path(), "http", local.address().port(), "pw");
    mount_entitlements(&local, SELF_PUUID).await;
    // Each route answers only on its own host; anywhere else it is a 404.
    let routes: [(&MockServer, &str, String, serde_json::Value); 9] = [
        (
            &local,
            "GET",
            "/chat/v4/presences".into(),
            json!({"presences": []}),
        ),
        (
            &pd,
            "GET",
            format!("/mmr/v1/players/{SELF_PUUID}"),
            json!({"Subject": SELF_PUUID}),
        ),
        (
            &pd,
            "GET",
            format!("/match-history/v1/history/{SELF_PUUID}"),
            json!({"Subject": SELF_PUUID, "History": []}),
        ),
        (&pd, "PUT", "/name-service/v2/players".into(), json!([])),
        // The content service is on the shared host, which a pd override
        // also moves.
        (
            &pd,
            "GET",
            "/content-service/v3/content".into(),
            json!({"Seasons": []}),
        ),
        (
            &glz,
            "GET",
            format!("/pregame/v1/players/{SELF_PUUID}"),
            json!({"Subject": SELF_PUUID, "MatchID": MATCH_ID}),
        ),
        (
            &glz,
            "GET",
            format!("/core-game/v1/matches/{MATCH_ID}"),
            json!({"MapID": "/Game/Maps/Ascent/Ascent", "Players": []}),
        ),
        (
            &glz,
            "GET",
            format!("/parties/v1/players/{SELF_PUUID}"),
            json!({"Subject": SELF_PUUID, "CurrentPartyID": PARTY_ID}),
        ),
        (
            &glz,
            "GET",
            format!("/parties/v1/parties/{PARTY_ID}"),
            json!({"ID": PARTY_ID, "Members": []}),
        ),
    ];
    for (server, verb, route, body) in routes {
        Mock::given(method(verb))
            .and(path(route))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(server)
            .await;
    }
    let session = opd1_riot::session::RiotSession::connect_with_overrides(
        tmp.path(),
        opd1_riot::session::ConnectOverrides {
            pd_base: Some(pd.uri()),
            glz_base: Some(glz.uri()),
            valorant_api_base: Some(local.uri()),
            ..Default::default()
        },
    )
    .await
    .expect("connect");

    session.presences().await.expect("local");
    session.mmr(SELF_PUUID).await.expect("pd");
    session
        .match_history(SELF_PUUID, None, None, None)
        .await
        .expect("pd");
    session.names(&one_visible()).await.expect("pd");
    session.content().await.expect("shared");
    session.pregame_player(SELF_PUUID).await.expect("glz");
    session.coregame_match(MATCH_ID).await.expect("glz");
    session.party_of(SELF_PUUID).await.expect("glz");
    session.party(PARTY_ID).await.expect("glz");
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
            "players": [], "teams": [{"teamId": "Blue", "won": true}], "roundResults": []
        })))
        .mount(&server)
        .await;

    let result = session.match_details(MATCH_ID).await.unwrap();
    assert!(result.players.is_empty());
}

fn details_with(queue: &str, completion: &str, teams: serde_json::Value) -> serde_json::Value {
    json!({
        "matchInfo": {"matchId": MATCH_ID, "mapId": "/Game/Maps/Ascent/Ascent",
            "gameStartMillis": 1_758_970_000_000i64, "queueID": queue, "isRanked": true,
            "completionState": completion},
        "players": [], "teams": teams, "roundResults": []
    })
}

#[tokio::test]
async fn a_match_with_no_teams_or_two_winners_is_a_failed_fetch_a_draw_is_not() {
    // R166: stored for good, so an answer with no teams or two winners is
    // not stored; both teams `won: false` is a draw (R29).
    let (_tmp, server, session) = setup().await;
    let blue_red = |blue: bool, red: bool| json!([{"teamId": "Blue", "won": blue}, {"teamId": "Red", "won": red}]);
    let cases = [
        // (queue, completion, teams, accepted)
        ("competitive", "Completed", blue_red(true, false), true),
        // Both teams lost: a draw (R29), accepted and stored as one.
        ("competitive", "Completed", blue_red(false, false), true),
        ("competitive", "Completed", blue_red(true, true), false),
        ("competitive", "Completed", json!([]), false),
        ("competitive", "Completed", serde_json::Value::Null, false),
        ("unrated", "Surrendered", blue_red(false, true), true),
        ("unrated", "Completed", json!([]), false),
        ("unrated", "Completed", blue_red(false, false), true),
        ("premier", "Completed", json!([]), false),
        // A vote-draw has no winner by definition.
        ("competitive", "VoteDraw", blue_red(false, false), true),
        ("competitive", "VoteDraw", json!([]), true),
        // Deathmatch has no teams.
        ("deathmatch", "Completed", serde_json::Value::Null, true),
        ("deathmatch", "Completed", json!([]), true),
    ];
    for (queue, completion, teams, accepted) in cases {
        server.reset().await;
        mount_entitlements(&server, SELF_PUUID).await;
        Mock::given(method("GET"))
            .and(path(format!("/match-details/v1/matches/{MATCH_ID}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(details_with(
                queue,
                completion,
                teams.clone(),
            )))
            .mount(&server)
            .await;
        let got = session.match_details(MATCH_ID).await;
        assert_eq!(
            got.is_ok(),
            accepted,
            "{queue} {completion} {teams}: {got:?}"
        );
        if !accepted {
            assert!(
                matches!(got, Err(opd1_riot::session::RiotError::Decode(_))),
                "{got:?}"
            );
        } else if teams
            .as_array()
            .is_some_and(|t| t.len() == 2 && t.iter().all(|t| t["won"] == json!(false)))
            && completion != "VoteDraw"
        {
            // Stored as a draw: no winner, and every player's result is `D`.
            let (stored, _, _) = opd1_riot::to_core::match_from_details(&got.unwrap());
            assert_eq!(stored.winning_team, None, "{queue}");
            assert_eq!(
                opd1_core::model::match_result("Blue", stored.winning_team.as_deref()),
                opd1_core::model::MatchResultKind::Draw
            );
        }
    }
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
async fn names_honors_a_429_retry_after_like_every_pd_call() {
    // R150: the names fill goes through the session's throttle, so a 429 on
    // the PUT holds the bucket for the Retry-After, then the call succeeds.
    let (_tmp, server, session) = setup().await;
    Mock::given(method("PUT"))
        .and(path("/name-service/v2/players"))
        .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "0.3"))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/name-service/v2/players"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            { "DisplayName": "", "Subject": OTHER_PUUID, "GameName": "Rival", "TagLine": "ENEMY" },
        ])))
        .mount(&server)
        .await;

    let visible = vec![VisiblePuuid::new(OTHER_PUUID.to_string(), false).unwrap()];
    let start = std::time::Instant::now();
    let result = session.names(&visible).await.expect("retried and answered");
    assert_eq!(result[0].game_name, "Rival");
    assert!(
        start.elapsed() >= std::time::Duration::from_millis(250),
        "the Retry-After was honored: {:?}",
        start.elapsed()
    );
}

#[tokio::test]
async fn names_sends_a_hundred_puuids_in_one_request() {
    // Riot's limit is 100 per request; the names fill sends full batches.
    let (_tmp, server, session) = setup().await;
    let ids: Vec<String> = (0..100)
        .map(|i| format!("{i:08x}-0000-0000-0000-000000000000"))
        .collect();
    Mock::given(method("PUT"))
        .and(path("/name-service/v2/players"))
        .and(body_json(json!(ids)))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .expect(1)
        .mount(&server)
        .await;

    let visible: Vec<VisiblePuuid> = ids
        .iter()
        .map(|i| VisiblePuuid::new(i.clone(), false).unwrap())
        .collect();
    let result = session.names(&visible).await.unwrap();
    assert!(result.is_empty(), "a blank answer is an empty list too");
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

#[tokio::test]
async fn content_hits_shared_with_the_remote_headers_and_never_fails_on_shape() {
    use wiremock::matchers::header_exists;
    let (_tmp, server, session) = setup().await;
    Mock::given(method("GET"))
        .and(path("/content-service/v3/content"))
        .and(header_exists("Authorization"))
        .and(header_exists("X-Riot-Entitlements-JWT"))
        .and(header_exists("X-Riot-ClientPlatform"))
        .and(header_exists("X-Riot-ClientVersion"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "Seasons": [{"ID": "a", "Type": "act", "IsActive": true,
                         "StartTime": "2026-08-19T00:00:00.000Z",
                         "EndTime": "2026-10-14T00:00:00.000Z"}],
            "SomethingNew": {"x": 1}
        })))
        .mount(&server)
        .await;

    let acts = session.content().await.unwrap().active_acts().unwrap();
    assert_eq!(acts.current.id, "a");

    // A body that is not even an object is an answer with no active act,
    // not an error.
    server.reset().await;
    Mock::given(method("GET"))
        .and(path("/content-service/v3/content"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([1, 2])))
        .mount(&server)
        .await;
    assert!(session.content().await.unwrap().active_acts().is_none());
}

#[tokio::test]
async fn content_error_status_is_an_error() {
    let (_tmp, server, session) = setup().await;
    Mock::given(method("GET"))
        .and(path("/content-service/v3/content"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    assert!(session.content().await.is_err());
}

// ---------------------------------------------------------------------
// name-service resilience and tolerance (R166)
// ---------------------------------------------------------------------

fn one_visible() -> Vec<VisiblePuuid> {
    vec![VisiblePuuid::new(OTHER_PUUID.to_string(), false).unwrap()]
}

async fn names_puts(server: &MockServer) -> Vec<serde_json::Value> {
    server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.method.as_str() == "PUT" && r.url.path() == "/name-service/v2/players")
        .map(|r| serde_json::from_slice(&r.body).unwrap())
        .collect()
}

#[tokio::test]
async fn names_retries_once_after_a_5xx_with_the_same_body() {
    let (_tmp, server, session) = setup().await;
    Mock::given(method("PUT"))
        .and(path("/name-service/v2/players"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/name-service/v2/players"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            { "Subject": OTHER_PUUID, "GameName": "Rival", "TagLine": "ENEMY" },
        ])))
        .mount(&server)
        .await;

    let result = session
        .names(&one_visible())
        .await
        .expect("second try answers");
    assert_eq!(result[0].game_name, "Rival");
    let sent = names_puts(&server).await;
    assert_eq!(sent.len(), 2);
    assert_eq!(sent[0], sent[1], "the same ids both times");
    assert_eq!(sent[0], json!([OTHER_PUUID]));
}

#[tokio::test]
async fn names_gives_up_after_the_second_5xx() {
    let (_tmp, server, session) = setup().await;
    Mock::given(method("PUT"))
        .and(path("/name-service/v2/players"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;

    let err = session.names(&one_visible()).await.unwrap_err();
    assert!(
        matches!(err, opd1_riot::session::RiotError::Status(500, _)),
        "{err:?}"
    );
    assert_eq!(names_puts(&server).await.len(), 2, "one retry, no more");
}

#[tokio::test]
async fn names_does_not_retry_a_client_error() {
    let (_tmp, server, session) = setup().await;
    Mock::given(method("PUT"))
        .and(path("/name-service/v2/players"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    assert!(session.names(&one_visible()).await.is_err());
    assert_eq!(names_puts(&server).await.len(), 1);
}

#[tokio::test]
async fn names_retries_once_after_a_transport_failure() {
    // The first answer comes after the (shortened) remote timeout: a
    // transport-level error, retried; the second is fast.
    let tmp = TempDir::new();
    write_shooter_log(tmp.path());
    let server = MockServer::start().await;
    write_lockfile(tmp.path(), "http", server.address().port(), "pw");
    mount_entitlements(&server, SELF_PUUID).await;
    let base = server.uri();
    let session = opd1_riot::session::RiotSession::connect_with_overrides(
        tmp.path(),
        opd1_riot::session::ConnectOverrides {
            pd_base: Some(base.clone()),
            glz_base: Some(base.clone()),
            valorant_api_base: Some(base),
            remote_timeout: Some(std::time::Duration::from_millis(300)),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    Mock::given(method("PUT"))
        .and(path("/name-service/v2/players"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!([]))
                .set_delay(std::time::Duration::from_secs(2)),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/name-service/v2/players"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            { "Subject": OTHER_PUUID, "GameName": "Rival", "TagLine": "ENEMY" },
        ])))
        .mount(&server)
        .await;

    let result = session.names(&one_visible()).await.expect("retried");
    assert_eq!(result.len(), 1);
    assert_eq!(names_puts(&server).await.len(), 2);
}

#[tokio::test]
async fn names_decoding_is_tolerant() {
    let (_tmp, server, session) = setup().await;
    let answer = |body: serde_json::Value| async {
        server.reset().await;
        mount_entitlements(&server, SELF_PUUID).await;
        Mock::given(method("PUT"))
            .and(path("/name-service/v2/players"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;
        session.names(&one_visible()).await.unwrap()
    };
    // Not a list (null, an object): no names.
    assert!(answer(serde_json::Value::Null).await.is_empty());
    assert!(answer(json!({"errorCode": "X"})).await.is_empty());
    // An entry without a Subject is skipped; a missing or null name is "".
    let result = answer(json!([
        { "GameName": "NoId", "TagLine": "X" },
        { "Subject": OTHER_PUUID, "GameName": null },
        { "Subject": SELF_PUUID },
    ]))
    .await;
    assert_eq!(result.len(), 2);
    assert_eq!(
        (result[0].game_name.as_str(), result[0].tag_line.as_str()),
        ("", "")
    );
    assert_eq!(result[1].subject, SELF_PUUID);
}
