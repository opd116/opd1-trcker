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

use opd1_riot::session::{ConnectOverrides, RiotError, RiotSession};
use support::{
    connect_against, delete_lockfile, mount_entitlements, mount_presence_with_version,
    mount_sessions_with_version, mount_sessions_without_valorant_entry, write_lockfile,
    write_shooter_log, TempDir, SELF_PUUID,
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
