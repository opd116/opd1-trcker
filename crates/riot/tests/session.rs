//! Integration tests for `RiotSession`: lockfile -> local auth ->
//! entitlements -> remote headers, `ClientGone` detection, 401/403
//! re-auth, and 429 retry/backoff. All HTTP is mocked with `wiremock` —
//! the lockfile's `protocol` field lets us point the "local" client at a
//! plain-http mock server (see task-3-brief.md's Testability note).

mod support;

use std::time::{Duration, Instant};

use serde_json::{json, Value};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use opd1_riot::session::{ConnectOverrides, RiotError, RiotSession, VersionSource};
use support::{
    connect_against, delete_lockfile, mount_entitlements, mount_presence_with_version,
    mount_sessions_with_version, mount_sessions_without_valorant_entry, mount_valorant_api_version,
    write_lockfile, write_shooter_log, TempDir, SELF_PUUID,
};

#[tokio::test]
async fn session_connects_and_exposes_self_puuid() {
    let tmp = TempDir::new();
    write_shooter_log(tmp.path());
    let server = MockServer::start().await;
    write_lockfile(tmp.path(), "http", server.address().port(), "pw");
    mount_entitlements(&server, SELF_PUUID).await;
    mount_sessions_with_version(&server, "release-09.08-shipping-11-3200000").await;

    let session = connect_against(tmp.path(), &server).await;

    assert_eq!(session.self_puuid(), SELF_PUUID);
}

#[tokio::test]
async fn session_connect_fails_when_lockfile_missing() {
    let tmp = TempDir::new();
    write_shooter_log(tmp.path());
    // No lockfile written at all.

    let err = RiotSession::connect(tmp.path()).await.unwrap_err();
    assert!(matches!(err, RiotError::ClientGone));
}

#[tokio::test]
async fn session_detects_lockfile_deleted_after_connect() {
    let tmp = TempDir::new();
    write_shooter_log(tmp.path());
    let server = MockServer::start().await;
    write_lockfile(tmp.path(), "http", server.address().port(), "pw");
    mount_entitlements(&server, SELF_PUUID).await;
    mount_sessions_with_version(&server, "v1").await;

    let session = connect_against(tmp.path(), &server).await;

    delete_lockfile(tmp.path());

    let err = session
        .local::<Value>("/chat/v4/presences")
        .await
        .unwrap_err();
    assert!(matches!(err, RiotError::ClientGone));
}

#[tokio::test]
async fn session_detects_port_change() {
    let tmp = TempDir::new();
    write_shooter_log(tmp.path());
    let server = MockServer::start().await;
    let original_port = server.address().port();
    write_lockfile(tmp.path(), "http", original_port, "pw");
    mount_entitlements(&server, SELF_PUUID).await;
    mount_sessions_with_version(&server, "v1").await;

    let session = connect_against(tmp.path(), &server).await;

    // The Riot client "restarts" with a fresh lockfile on a different port.
    write_lockfile(tmp.path(), "http", original_port.wrapping_add(1), "pw");

    let err = session
        .local::<Value>("/chat/v4/presences")
        .await
        .unwrap_err();
    assert!(matches!(err, RiotError::ClientGone));
}

#[tokio::test]
async fn session_detects_password_change_even_if_port_is_unchanged() {
    let tmp = TempDir::new();
    write_shooter_log(tmp.path());
    let server = MockServer::start().await;
    let port = server.address().port();
    write_lockfile(tmp.path(), "http", port, "pw-1");
    mount_entitlements(&server, SELF_PUUID).await;
    mount_sessions_with_version(&server, "v1").await;

    let session = connect_against(tmp.path(), &server).await;

    write_lockfile(tmp.path(), "http", port, "pw-2");

    let err = session
        .local::<Value>("/chat/v4/presences")
        .await
        .unwrap_err();
    assert!(matches!(err, RiotError::ClientGone));
}

#[tokio::test]
async fn session_threads_product_session_version_into_remote_headers() {
    let tmp = TempDir::new();
    write_shooter_log(tmp.path());
    let server = MockServer::start().await;
    write_lockfile(tmp.path(), "http", server.address().port(), "pw");
    mount_entitlements(&server, SELF_PUUID).await;
    mount_sessions_with_version(&server, "release-09.08-shipping-11-3200000").await;

    let session = connect_against(tmp.path(), &server).await;

    // Only responds if the client threaded the product-session version
    // through as X-Riot-ClientVersion on a *remote* (pd) call.
    Mock::given(method("GET"))
        .and(path(format!("/mmr/v1/players/{SELF_PUUID}")))
        .and(header(
            "X-Riot-ClientVersion",
            "release-09.08-shipping-11-3200000",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "Subject": SELF_PUUID, "QueueSkills": {}
        })))
        .mount(&server)
        .await;

    let result: Value = session
        .pd(&format!("/mmr/v1/players/{SELF_PUUID}"))
        .await
        .expect("mmr call should succeed with the right client version header");
    assert_eq!(result["Subject"], SELF_PUUID);
}

#[tokio::test]
async fn player_loadout_reads_only_the_sessions_own_puuid() {
    // R90: the personalization endpoint is asked for the user's own puuid
    // and nothing else; the method takes no puuid, so no caller can send
    // another player's.
    let tmp = TempDir::new();
    write_shooter_log(tmp.path());
    let server = MockServer::start().await;
    write_lockfile(tmp.path(), "http", server.address().port(), "pw");
    mount_entitlements(&server, SELF_PUUID).await;
    mount_sessions_with_version(&server, "release-09.08-shipping-11-3200000").await;
    let session = connect_against(tmp.path(), &server).await;

    let own = format!("/personalization/v3/players/{SELF_PUUID}/playerloadout");
    Mock::given(method("GET"))
        .and(path(own.clone()))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"Subject": SELF_PUUID, "Guns": []})),
        )
        .mount(&server)
        .await;

    let loadout = session.player_loadout().await.expect("own loadout");
    assert!(loadout.guns.is_empty());
    let asked: Vec<String> = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .map(|r| r.url.path().to_string())
        .filter(|p| p.starts_with("/personalization/"))
        .collect();
    assert_eq!(asked, [own]);
}

#[tokio::test]
async fn session_falls_back_to_presence_client_version() {
    let tmp = TempDir::new();
    write_shooter_log(tmp.path());
    let server = MockServer::start().await;
    write_lockfile(tmp.path(), "http", server.address().port(), "pw");
    mount_entitlements(&server, SELF_PUUID).await;
    // No `valorant` entry in product-session -> must fall through to the
    // presence-based fallback.
    mount_sessions_without_valorant_entry(&server).await;
    mount_presence_with_version(&server, SELF_PUUID, "release-fallback-1").await;

    let session = connect_against(tmp.path(), &server).await;

    Mock::given(method("GET"))
        .and(path(format!("/mmr/v1/players/{SELF_PUUID}")))
        .and(header("X-Riot-ClientVersion", "release-fallback-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "Subject": SELF_PUUID, "QueueSkills": {}
        })))
        .mount(&server)
        .await;

    let result: Value = session
        .pd(&format!("/mmr/v1/players/{SELF_PUUID}"))
        .await
        .expect("mmr call should succeed using the presence-fallback version");
    assert_eq!(result["Subject"], SELF_PUUID);
}

#[tokio::test]
async fn session_reauths_once_on_401_then_succeeds() {
    let tmp = TempDir::new();
    write_shooter_log(tmp.path());
    let server = MockServer::start().await;
    write_lockfile(tmp.path(), "http", server.address().port(), "pw");
    mount_entitlements(&server, SELF_PUUID).await;
    mount_sessions_with_version(&server, "v1").await;

    let session = connect_against(tmp.path(), &server).await;

    // First hit: stale token -> 401. Mounted first, so it is preferred
    // over the always-on success mock below while it still has an
    // available match (wiremock tries same-priority mocks in mount order).
    Mock::given(method("GET"))
        .and(path(format!("/mmr/v1/players/{SELF_PUUID}")))
        .respond_with(ResponseTemplate::new(401))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/mmr/v1/players/{SELF_PUUID}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "Subject": SELF_PUUID, "QueueSkills": {}
        })))
        .mount(&server)
        .await;

    let result: Value = session
        .pd(&format!("/mmr/v1/players/{SELF_PUUID}"))
        .await
        .expect("should transparently re-auth and succeed");
    assert_eq!(result["Subject"], SELF_PUUID);
}

#[tokio::test]
async fn session_retries_after_429_honoring_retry_after() {
    let tmp = TempDir::new();
    write_shooter_log(tmp.path());
    let server = MockServer::start().await;
    write_lockfile(tmp.path(), "http", server.address().port(), "pw");
    mount_entitlements(&server, SELF_PUUID).await;
    mount_sessions_with_version(&server, "v1").await;

    let session = connect_against(tmp.path(), &server).await;

    Mock::given(method("GET"))
        .and(path(format!("/mmr/v1/players/{SELF_PUUID}")))
        .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "0.3"))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/mmr/v1/players/{SELF_PUUID}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "Subject": SELF_PUUID, "QueueSkills": {}
        })))
        .mount(&server)
        .await;

    let start = Instant::now();
    let result: Value = session
        .pd(&format!("/mmr/v1/players/{SELF_PUUID}"))
        .await
        .expect("should retry after the hold and succeed");
    let elapsed = start.elapsed();

    assert_eq!(result["Subject"], SELF_PUUID);
    assert!(
        elapsed >= Duration::from_millis(250),
        "expected the 429's Retry-After to be honored, got {elapsed:?}"
    );
}

#[tokio::test]
async fn session_gives_up_after_exhausting_429_retries() {
    let tmp = TempDir::new();
    write_shooter_log(tmp.path());
    let server = MockServer::start().await;
    write_lockfile(tmp.path(), "http", server.address().port(), "pw");
    mount_entitlements(&server, SELF_PUUID).await;
    mount_sessions_with_version(&server, "v1").await;

    let session = connect_against(tmp.path(), &server).await;

    // Always rate-limited, with a short Retry-After so the test stays fast
    // — this proves the field "eventually becomes an error, never hangs
    // forever" per CLAUDE.md, rather than actually waiting out real
    // Riot-scale backoffs.
    Mock::given(method("GET"))
        .and(path(format!("/mmr/v1/players/{SELF_PUUID}")))
        .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "0.05"))
        .mount(&server)
        .await;

    let start = Instant::now();
    let err = session
        .pd::<Value>(&format!("/mmr/v1/players/{SELF_PUUID}"))
        .await
        .unwrap_err();
    let elapsed = start.elapsed();

    assert!(matches!(err, RiotError::RateLimited));
    assert!(
        elapsed < Duration::from_secs(5),
        "bounded retries should not hang, took {elapsed:?}"
    );
}

#[tokio::test]
async fn session_local_call_times_out_when_server_never_responds() {
    // Fix-round item 1: without a timeout, a non-responding local client
    // would hang `local()` forever. Uses the `local_timeout` override so
    // the test doesn't have to wait out the real 5s production timeout.
    let tmp = TempDir::new();
    write_shooter_log(tmp.path());
    let server = MockServer::start().await;
    write_lockfile(tmp.path(), "http", server.address().port(), "pw");
    mount_entitlements(&server, SELF_PUUID).await;
    mount_sessions_with_version(&server, "v1").await;

    let base = server.uri();
    let session = RiotSession::connect_with_overrides(
        tmp.path(),
        ConnectOverrides {
            pd_base: Some(base.clone()),
            glz_base: Some(base.clone()),
            valorant_api_base: Some(base),
            local_timeout: Some(Duration::from_millis(100)),
            remote_timeout: None,
        },
    )
    .await
    .expect("connect should succeed — entitlements/sessions respond instantly");

    Mock::given(method("GET"))
        .and(path("/chat/v4/presences"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_secs(2))
                .set_body_json(json!({ "presences": [] })),
        )
        .mount(&server)
        .await;

    let start = Instant::now();
    let err = session
        .local::<Value>("/chat/v4/presences")
        .await
        .unwrap_err();
    let elapsed = start.elapsed();

    assert!(matches!(err, RiotError::Http(_)));
    assert!(
        elapsed < Duration::from_secs(1),
        "should time out at ~100ms, well before the mock's 2s delay, got {elapsed:?}"
    );
}

#[tokio::test]
async fn session_remote_call_times_out_when_server_never_responds() {
    // Same as above, for a pd/glz call — this is the `Client` that also
    // backs the `valorant-api.com` client-version fallback.
    let tmp = TempDir::new();
    write_shooter_log(tmp.path());
    let server = MockServer::start().await;
    write_lockfile(tmp.path(), "http", server.address().port(), "pw");
    mount_entitlements(&server, SELF_PUUID).await;
    mount_sessions_with_version(&server, "v1").await;

    let base = server.uri();
    let session = RiotSession::connect_with_overrides(
        tmp.path(),
        ConnectOverrides {
            pd_base: Some(base.clone()),
            glz_base: Some(base.clone()),
            valorant_api_base: Some(base),
            local_timeout: None,
            remote_timeout: Some(Duration::from_millis(100)),
        },
    )
    .await
    .expect("connect should succeed");

    Mock::given(method("GET"))
        .and(path(format!("/mmr/v1/players/{SELF_PUUID}")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_secs(2))
                .set_body_json(json!({ "Subject": SELF_PUUID, "QueueSkills": {} })),
        )
        .mount(&server)
        .await;

    let start = Instant::now();
    let err = session
        .pd::<Value>(&format!("/mmr/v1/players/{SELF_PUUID}"))
        .await
        .unwrap_err();
    let elapsed = start.elapsed();

    assert!(matches!(err, RiotError::Http(_)));
    assert!(
        elapsed < Duration::from_secs(1),
        "should time out at ~100ms, well before the mock's 2s delay, got {elapsed:?}"
    );
}

#[tokio::test]
async fn a_network_error_never_carries_the_url() {
    // The user shares copy-log (CLAUDE.md), and ids there are shortened: a
    // timeout on a puuid-bearing path (match history, mmr, a player's
    // pregame/core-game/party path) must not print the full puuid through
    // reqwest's " for url (...)" suffix. The caller already knows which
    // endpoint failed.
    let tmp = TempDir::new();
    write_shooter_log(tmp.path());
    let server = MockServer::start().await;
    write_lockfile(tmp.path(), "http", server.address().port(), "pw");
    mount_entitlements(&server, SELF_PUUID).await;
    mount_sessions_with_version(&server, "v1").await;
    let base = server.uri();
    let session = RiotSession::connect_with_overrides(
        tmp.path(),
        ConnectOverrides {
            pd_base: Some(base.clone()),
            glz_base: Some(base.clone()),
            valorant_api_base: Some(base),
            local_timeout: Some(Duration::from_millis(100)),
            remote_timeout: Some(Duration::from_millis(100)),
        },
    )
    .await
    .expect("connect should succeed");
    let history = format!("/match-history/v1/history/{SELF_PUUID}");
    for p in [history.as_str(), "/chat/v4/presences"] {
        Mock::given(method("GET"))
            .and(path(p))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_delay(Duration::from_secs(2))
                    .set_body_json(json!({})),
            )
            .mount(&server)
            .await;
    }

    let remote = session
        .pd::<Value>(&format!("{history}?startIndex=0&endIndex=20"))
        .await
        .unwrap_err();
    let local = session
        .local::<Value>("/chat/v4/presences")
        .await
        .unwrap_err();

    for (err, p) in [(remote, "/match-history/"), (local, "/chat/")] {
        let msg = err.to_string();
        assert!(matches!(err, RiotError::Http(_)), "{msg}");
        assert!(!msg.contains(SELF_PUUID), "no full puuid: {msg}");
        assert!(!msg.contains(p), "no url: {msg}");
        // R158: the line says what kind of transport failure it was.
        assert!(msg.contains("timeout"), "the kind is named: {msg}");
    }
}

#[tokio::test]
async fn session_debug_output_redacts_secrets() {
    // Fix-round item 4: the user shares in-app logs, so a `{:?}` of a live
    // session (e.g. in a panic message) must never leak the lockfile
    // password, the Basic-auth header built from it, or the live Bearer
    // token/entitlements JWT.
    use base64::engine::general_purpose::STANDARD as BASE64;
    use base64::Engine as _;

    let tmp = TempDir::new();
    write_shooter_log(tmp.path());
    let server = MockServer::start().await;
    let password = "super-secret-lockfile-password-12345";
    write_lockfile(tmp.path(), "http", server.address().port(), password);
    // Fixed tokens from `mount_entitlements`: "access-token-1" / "entitlements-jwt-1".
    mount_entitlements(&server, SELF_PUUID).await;
    mount_sessions_with_version(&server, "v1").await;

    let session = connect_against(tmp.path(), &server).await;

    let debug_output = format!("{session:?}");
    let expected_basic_auth = format!("Basic {}", BASE64.encode(format!("riot:{password}")));

    assert!(
        !debug_output.contains(password),
        "raw lockfile password leaked into Debug output: {debug_output}"
    );
    assert!(
        !debug_output.contains(&expected_basic_auth),
        "Basic-auth header leaked into Debug output: {debug_output}"
    );
    assert!(
        !debug_output.contains("access-token-1"),
        "Bearer access token leaked into Debug output: {debug_output}"
    );
    assert!(
        !debug_output.contains("entitlements-jwt-1"),
        "entitlements JWT leaked into Debug output: {debug_output}"
    );
    // Sanity: Debug output should still be a real, non-trivial struct dump
    // (proves the whole thing wasn't blanked out rather than redacted).
    assert!(debug_output.contains("RiotSession"));
    assert!(debug_output.contains(SELF_PUUID));
    assert!(debug_output.contains("redacted"));
}

#[tokio::test]
async fn raw_captures_only_records_successful_responses() {
    // Fix-round item 9 / R36: an error response must never be captured
    // (nor allowed to overwrite a prior good capture for that endpoint).
    let tmp = TempDir::new();
    write_shooter_log(tmp.path());
    let server = MockServer::start().await;
    write_lockfile(tmp.path(), "http", server.address().port(), "pw");
    mount_entitlements(&server, SELF_PUUID).await;
    mount_sessions_with_version(&server, "v1").await;

    let session = connect_against(tmp.path(), &server).await;

    Mock::given(method("GET"))
        .and(path(format!("/mmr/v1/players/{SELF_PUUID}")))
        .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
        .up_to_n_times(1)
        .mount(&server)
        .await;

    let err = session
        .pd::<Value>(&format!("/mmr/v1/players/{SELF_PUUID}"))
        .await
        .unwrap_err();
    assert!(matches!(err, RiotError::Status(500, _)));
    assert!(
        session.raw_captures().iter().all(|(k, _)| k != "mmr"),
        "a 500 response must not be captured"
    );

    Mock::given(method("GET"))
        .and(path(format!("/mmr/v1/players/{SELF_PUUID}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "Subject": SELF_PUUID, "QueueSkills": {}
        })))
        .mount(&server)
        .await;

    let _: Value = session
        .pd(&format!("/mmr/v1/players/{SELF_PUUID}"))
        .await
        .expect("the follow-up 200 should succeed");

    let captures = session.raw_captures();
    let mmr_capture = captures
        .iter()
        .find(|(k, _)| k == "mmr")
        .expect("the successful 200 response must be captured");
    assert!(mmr_capture.1.contains(SELF_PUUID));
}

// ---------------------------------------------------------------------
// R158: what a failed answer says, and which client version is sent
// ---------------------------------------------------------------------

/// A session against `server` (its `ShooterGame.log` has shards only).
async fn session_for(server: &MockServer, tmp: &TempDir) -> RiotSession {
    write_shooter_log(tmp.path());
    write_lockfile(tmp.path(), "http", server.address().port(), "pw");
    mount_entitlements(server, SELF_PUUID).await;
    connect_against(tmp.path(), server).await
}

fn mmr_path() -> String {
    format!("/mmr/v1/players/{SELF_PUUID}")
}

#[tokio::test]
async fn a_non_2xx_answer_reports_its_status_and_riots_error_code() {
    let tmp = TempDir::new();
    let server = MockServer::start().await;
    let session = session_for(&server, &tmp).await;
    Mock::given(method("GET"))
        .and(path(mmr_path()))
        .respond_with(ResponseTemplate::new(500).set_body_json(json!({
            "httpStatus": 500,
            "errorCode": "INTERNAL_SERVER_ERROR",
            "message": format!("player {SELF_PUUID} could not be read"),
        })))
        .mount(&server)
        .await;

    let err = session.pd::<Value>(&mmr_path()).await.unwrap_err();
    let msg = err.to_string();
    assert!(matches!(&err, RiotError::Status(500, _)), "{msg}");
    assert!(msg.contains("500"), "{msg}");
    assert!(
        msg.contains("INTERNAL_SERVER_ERROR"),
        "the errorCode: {msg}"
    );
    // Nothing that must not be in a shared log.
    assert!(!msg.contains(SELF_PUUID), "no full puuid: {msg}");
    assert!(msg.contains("aaaaaaaa"), "the short puuid stays: {msg}");
    assert!(
        !msg.contains("http://") && !msg.contains("/mmr/"),
        "no url: {msg}"
    );
    assert!(
        !msg.contains("access-token-1") && !msg.contains("entitlements-jwt-1"),
        "{msg}"
    );
}

#[tokio::test]
async fn an_error_body_is_redacted_and_cut_in_the_error_text() {
    let tmp = TempDir::new();
    let server = MockServer::start().await;
    let session = session_for(&server, &tmp).await;
    let jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dQw4w9WgXcQ";
    Mock::given(method("GET"))
        .and(path(mmr_path()))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "errorCode": "BAD_CLAIMS",
            "accessToken": "opaque-secret-access-token",
            "message": jwt,
            "pad": "x".repeat(5_000),
        })))
        .mount(&server)
        .await;

    let msg = session
        .pd::<Value>(&mmr_path())
        .await
        .unwrap_err()
        .to_string();
    assert!(msg.contains("BAD_CLAIMS"), "{msg}");
    assert!(!msg.contains("opaque-secret-access-token"), "{msg}");
    assert!(!msg.contains(jwt), "{msg}");
    assert!(
        msg.chars().count() < 400,
        "bounded: {} chars",
        msg.chars().count()
    );
}

#[tokio::test]
async fn error_bodies_are_kept_for_the_dump_apart_from_the_2xx_captures() {
    let tmp = TempDir::new();
    let server = MockServer::start().await;
    let session = session_for(&server, &tmp).await;
    Mock::given(method("GET"))
        .and(path(mmr_path()))
        .respond_with(ResponseTemplate::new(500).set_body_json(json!({
            "httpStatus": 500,
            "errorCode": "INTERNAL_SERVER_ERROR",
            "accessToken": "opaque-secret-access-token",
        })))
        .up_to_n_times(1)
        .mount(&server)
        .await;

    session.pd::<Value>(&mmr_path()).await.unwrap_err();
    let dumps = session.raw_error_captures();
    assert_eq!(dumps.len(), 1);
    assert_eq!((dumps[0].key.as_str(), dumps[0].ext), ("mmr", "json"));
    let body: Value = serde_json::from_str(&dumps[0].contents).unwrap();
    assert_eq!(body["status"], 500);
    assert_eq!(body["body"]["errorCode"], "INTERNAL_SERVER_ERROR");
    assert!(!dumps[0].contents.contains("opaque-secret-access-token"));
    assert!(
        session.raw_captures().is_empty(),
        "an error is no 2xx capture"
    );

    // A later 2xx neither deletes the error nor is confused with it.
    Mock::given(method("GET"))
        .and(path(mmr_path()))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "Subject": SELF_PUUID, "QueueSkills": {}
        })))
        .mount(&server)
        .await;
    let _: Value = session.pd(&mmr_path()).await.unwrap();
    assert_eq!(session.raw_error_captures().len(), 1, "still there");
    assert_eq!(session.raw_captures().len(), 1);
    assert!(session.raw_captures()[0].1.contains(SELF_PUUID));

    // A non-JSON error page is kept as text, status first.
    Mock::given(method("GET"))
        .and(path("/match-details/v1/matches/abc"))
        .respond_with(ResponseTemplate::new(502).set_body_string("<html>Bad Gateway</html>"))
        .mount(&server)
        .await;
    session
        .pd::<Value>("/match-details/v1/matches/abc")
        .await
        .unwrap_err();
    let dumps = session.raw_error_captures();
    let page = dumps.iter().find(|d| d.key == "match_details").unwrap();
    assert_eq!(page.ext, "txt");
    assert!(page.contents.starts_with("HTTP 502\n"), "{}", page.contents);
}

#[tokio::test]
async fn a_refused_connection_says_connect() {
    let tmp = TempDir::new();
    let server = MockServer::start().await;
    write_shooter_log(tmp.path());
    write_lockfile(tmp.path(), "http", server.address().port(), "pw");
    mount_entitlements(&server, SELF_PUUID).await;
    // Remote calls go to a port nobody listens on.
    let session = RiotSession::connect_with_overrides(
        tmp.path(),
        ConnectOverrides {
            pd_base: Some("http://127.0.0.1:1".to_string()),
            glz_base: Some("http://127.0.0.1:1".to_string()),
            valorant_api_base: Some(server.uri()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let msg = session
        .pd::<Value>(&mmr_path())
        .await
        .unwrap_err()
        .to_string();
    assert!(msg.contains("connect"), "{msg}");
    assert!(
        !msg.contains(SELF_PUUID) && !msg.contains("127.0.0.1"),
        "{msg}"
    );
}

#[tokio::test]
async fn the_client_version_source_order_is_sessions_presence_valorant_api_fallback() {
    // Nothing answers: the literal fallback, named as such.
    let tmp = TempDir::new();
    let server = MockServer::start().await;
    let session = session_for(&server, &tmp).await;
    assert_eq!(session.client_version_source(), VersionSource::Fallback);
    assert_eq!(session.client_version(), "unknown");

    // valorant-api alone.
    let tmp = TempDir::new();
    let server = MockServer::start().await;
    mount_valorant_api_version(&server, "release-api-9").await;
    let session = session_for(&server, &tmp).await;
    assert_eq!(session.client_version_source(), VersionSource::ValorantApi);
    assert_eq!(session.client_version(), "release-api-9");

    // presence beats valorant-api.
    mount_presence_with_version(&server, SELF_PUUID, "release-presence-9").await;
    let tmp = TempDir::new();
    let session = session_for(&server, &tmp).await;
    assert_eq!(session.client_version_source(), VersionSource::Presence);
    assert_eq!(session.client_version(), "release-presence-9");

    // sessions beats presence.
    mount_sessions_with_version(&server, "release-sessions-9").await;
    let tmp = TempDir::new();
    let session = session_for(&server, &tmp).await;
    assert_eq!(session.client_version_source(), VersionSource::Sessions);
    assert_eq!(session.client_version(), "release-sessions-9");
}

#[tokio::test]
async fn an_empty_or_blank_version_falls_through_to_the_next_source() {
    for blank in ["", "   "] {
        // sessions blank -> presence
        let tmp = TempDir::new();
        let server = MockServer::start().await;
        mount_sessions_with_version(&server, blank).await;
        mount_presence_with_version(&server, SELF_PUUID, "release-presence-1").await;
        mount_valorant_api_version(&server, "release-api-1").await;
        let session = session_for(&server, &tmp).await;
        assert_eq!(
            session.client_version_source(),
            VersionSource::Presence,
            "{blank:?}"
        );
        assert_eq!(session.client_version(), "release-presence-1");

        // sessions and presence blank -> valorant-api
        let tmp = TempDir::new();
        let server = MockServer::start().await;
        mount_sessions_with_version(&server, blank).await;
        mount_presence_with_version(&server, SELF_PUUID, blank).await;
        mount_valorant_api_version(&server, "release-api-2").await;
        let session = session_for(&server, &tmp).await;
        assert_eq!(
            session.client_version_source(),
            VersionSource::ValorantApi,
            "{blank:?}"
        );

        // every source blank -> fallback, and the header is never blank
        let tmp = TempDir::new();
        let server = MockServer::start().await;
        mount_sessions_with_version(&server, blank).await;
        mount_presence_with_version(&server, SELF_PUUID, blank).await;
        mount_valorant_api_version(&server, blank).await;
        let session = session_for(&server, &tmp).await;
        assert_eq!(
            session.client_version_source(),
            VersionSource::Fallback,
            "{blank:?}"
        );
        assert!(!session.client_version().trim().is_empty());
    }
}

/// Every `opd1_riot` log line, for [`the_client_version_and_its_source_are_logged_once`].
struct Capture(std::sync::Mutex<Vec<String>>);

impl log::Log for Capture {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.target().starts_with("opd1_riot")
    }
    fn log(&self, r: &log::Record) {
        if self.enabled(r.metadata()) {
            self.0
                .lock()
                .unwrap()
                .push(format!("{} {}", r.level(), r.args()));
        }
    }
    fn flush(&self) {}
}

static CAPTURE: Capture = Capture(std::sync::Mutex::new(Vec::new()));

#[tokio::test]
async fn the_client_version_and_its_source_are_logged_once_per_session() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        log::set_logger(&CAPTURE).unwrap();
        log::set_max_level(log::LevelFilter::Info);
    });
    let tmp = TempDir::new();
    let server = MockServer::start().await;
    mount_presence_with_version(&server, SELF_PUUID, "release-logged-once-1").await;
    let session = session_for(&server, &tmp).await;
    // A few requests must not log it again.
    Mock::given(method("GET"))
        .and(path(mmr_path()))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"Subject": SELF_PUUID, "QueueSkills": {}})),
        )
        .mount(&server)
        .await;
    for _ in 0..3 {
        let _: Value = session.pd(&mmr_path()).await.unwrap();
    }

    let lines: Vec<String> = CAPTURE
        .0
        .lock()
        .unwrap()
        .iter()
        .filter(|l| l.contains("release-logged-once-1"))
        .cloned()
        .collect();
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].starts_with("INFO "), "info level: {}", lines[0]);
    assert!(
        lines[0].contains("from presence"),
        "the source: {}",
        lines[0]
    );
}
