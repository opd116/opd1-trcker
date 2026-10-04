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
    connect_against, delete_lockfile, mount_entitlements, mount_presence_private,
    mount_presence_with_version, mount_valorant_api_version, presence_private_b64, write_lockfile,
    write_shooter_log, write_shooter_log_with_version, TempDir, SELF_PUUID,
};

#[tokio::test]
async fn session_connects_and_exposes_self_puuid() {
    let tmp = TempDir::new();
    write_shooter_log(tmp.path());
    let server = MockServer::start().await;
    write_lockfile(tmp.path(), "http", server.address().port(), "pw");
    mount_entitlements(&server, SELF_PUUID).await;

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

    let session = connect_against(tmp.path(), &server).await;

    write_lockfile(tmp.path(), "http", port, "pw-2");

    let err = session
        .local::<Value>("/chat/v4/presences")
        .await
        .unwrap_err();
    assert!(matches!(err, RiotError::ClientGone));
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
async fn session_reauths_once_on_401_then_succeeds() {
    let tmp = TempDir::new();
    write_shooter_log(tmp.path());
    let server = MockServer::start().await;
    write_lockfile(tmp.path(), "http", server.address().port(), "pw");
    mount_entitlements(&server, SELF_PUUID).await;

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

const HISTORY_PATH: &str = "/match-history/v1/history/bbbbbbbb-0000-0000-0000-000000000002";
const OTHER_MMR_PATH: &str = "/mmr/v1/players/bbbbbbbb-0000-0000-0000-000000000002";

#[tokio::test]
async fn session_retries_a_non_mmr_call_after_429_honoring_retry_after() {
    // Every family but mmr keeps its in-call retries (R160).
    let tmp = TempDir::new();
    write_shooter_log(tmp.path());
    let server = MockServer::start().await;
    write_lockfile(tmp.path(), "http", server.address().port(), "pw");
    mount_entitlements(&server, SELF_PUUID).await;

    let session = connect_against(tmp.path(), &server).await;

    Mock::given(method("GET"))
        .and(path(HISTORY_PATH))
        .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "0.3"))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(HISTORY_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "History": [] })))
        .mount(&server)
        .await;

    let start = Instant::now();
    let result: Value = session
        .pd(HISTORY_PATH)
        .await
        .expect("should retry after the hold and succeed");
    let elapsed = start.elapsed();

    assert_eq!(result["History"], json!([]));
    assert!(
        elapsed >= Duration::from_millis(250),
        "expected the 429's Retry-After to be honored, got {elapsed:?}"
    );
}

#[tokio::test]
async fn session_gives_up_after_exhausting_429_retries_on_a_non_mmr_call() {
    let tmp = TempDir::new();
    write_shooter_log(tmp.path());
    let server = MockServer::start().await;
    write_lockfile(tmp.path(), "http", server.address().port(), "pw");
    mount_entitlements(&server, SELF_PUUID).await;

    let session = connect_against(tmp.path(), &server).await;

    // Always rate-limited, with a short Retry-After so the test stays fast
    // — this proves the field "eventually becomes an error, never hangs
    // forever" per CLAUDE.md, rather than actually waiting out real
    // Riot-scale backoffs. 3 attempts: the first and two retries.
    Mock::given(method("GET"))
        .and(path(HISTORY_PATH))
        .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "0.05"))
        .expect(3)
        .mount(&server)
        .await;

    let start = Instant::now();
    let err = session.pd::<Value>(HISTORY_PATH).await.unwrap_err();
    let elapsed = start.elapsed();

    assert!(matches!(err, RiotError::RateLimited));
    assert!(
        elapsed < Duration::from_secs(5),
        "bounded retries should not hang, took {elapsed:?}"
    );
}

#[tokio::test]
async fn an_mmr_429_holds_the_full_retry_after_and_returns_at_once() {
    // R160: Riot's /mmr/ block is ~60 s. The hold is the header's value in
    // full (no 30 s cap), the 429'd call is not retried in-call, and the next
    // mmr call is refused locally with what is left of the hold, nothing sent.
    let tmp = TempDir::new();
    write_shooter_log(tmp.path());
    let server = MockServer::start().await;
    write_lockfile(tmp.path(), "http", server.address().port(), "pw");
    mount_entitlements(&server, SELF_PUUID).await;
    let session = connect_against(tmp.path(), &server).await;

    Mock::given(method("GET"))
        .and(path(OTHER_MMR_PATH))
        .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "60"))
        .expect(1)
        .mount(&server)
        .await;

    let start = Instant::now();
    let first = session.pd::<Value>(OTHER_MMR_PATH).await.unwrap_err();
    assert!(
        matches!(first, RiotError::RateLimited),
        "the sent 429 is a plain rate-limit: {first:?}"
    );

    let second = session.pd::<Value>(OTHER_MMR_PATH).await.unwrap_err();
    let RiotError::Held(left) = second else {
        panic!("expected Held, got {second:?}");
    };
    assert!(
        left > Duration::from_secs(55) && left <= Duration::from_secs(60),
        "the whole 60 s is held, not 30: {left:?}"
    );
    assert!(
        start.elapsed() < Duration::from_secs(2),
        "neither call waited: {:?}",
        start.elapsed()
    );
    assert!(RiotError::Held(left).to_string().contains("on hold"));
    // `.expect(1)` is checked when the server drops: nothing was sent the
    // second time.
}

#[tokio::test]
async fn every_mmr_path_shares_the_mmr_hold_and_other_calls_do_not() {
    // A /competitiveupdates 429 holds the whole /mmr/ family (one server
    // bucket); a match-history call still goes out.
    let tmp = TempDir::new();
    write_shooter_log(tmp.path());
    let server = MockServer::start().await;
    write_lockfile(tmp.path(), "http", server.address().port(), "pw");
    mount_entitlements(&server, SELF_PUUID).await;
    let session = connect_against(tmp.path(), &server).await;

    let comp = format!("{OTHER_MMR_PATH}/competitiveupdates?queue=competitive");
    Mock::given(method("GET"))
        .and(path(format!("{OTHER_MMR_PATH}/competitiveupdates")))
        .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "60"))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(HISTORY_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "History": [] })))
        .mount(&server)
        .await;

    assert!(matches!(
        session.pd::<Value>(&comp).await.unwrap_err(),
        RiotError::RateLimited
    ));
    // The plain mmr path is refused locally (no mock mounted: a sent request
    // would be a 404 `Status`).
    assert!(matches!(
        session.pd::<Value>(OTHER_MMR_PATH).await.unwrap_err(),
        RiotError::Held(_)
    ));
    // Another family is not held.
    let start = Instant::now();
    let _: Value = session.pd(HISTORY_PATH).await.expect("not held");
    assert!(start.elapsed() < Duration::from_secs(1));
}

#[tokio::test]
async fn an_other_429_holds_only_the_other_family_and_mmr_still_answers() {
    // R160: a match-history 429 used to hold the global bucket and so delayed
    // mmr too. A hold over the 10 s in-call bound returns at once: the 429'd
    // call is not retried in-call, the next `other` call is refused locally
    // (nothing sent), and mmr is not touched.
    let tmp = TempDir::new();
    write_shooter_log(tmp.path());
    let server = MockServer::start().await;
    write_lockfile(tmp.path(), "http", server.address().port(), "pw");
    mount_entitlements(&server, SELF_PUUID).await;
    let session = connect_against(tmp.path(), &server).await;

    Mock::given(method("GET"))
        .and(path(HISTORY_PATH))
        .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "60"))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(OTHER_MMR_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "Subject": "x", "QueueSkills": {}
        })))
        .mount(&server)
        .await;

    let start = Instant::now();
    let first = session.pd::<Value>(HISTORY_PATH).await.unwrap_err();
    assert!(
        matches!(first, RiotError::RateLimited),
        "a 60 s hold is not slept out: {first:?}"
    );
    let second = session.pd::<Value>(HISTORY_PATH).await.unwrap_err();
    let RiotError::Held(left) = second else {
        panic!("expected Held, got {second:?}");
    };
    assert!(left > Duration::from_secs(55) && left <= Duration::from_secs(60));

    let mmr: Value = session
        .pd(OTHER_MMR_PATH)
        .await
        .expect("mmr must not wait on an `other` hold");
    assert_eq!(mmr["Subject"], "x");
    assert!(
        start.elapsed() < Duration::from_secs(2),
        "nothing waited: {:?}",
        start.elapsed()
    );
    // `.expect(1)`: the second history call sent nothing.
}

#[tokio::test]
async fn an_other_call_waits_for_a_short_hold_but_not_a_long_one() {
    // R160's bound: a hold of at most 10 s is waited out in the call (here
    // 0.4 s, set by a first 429); one with more left is refused at once.
    let tmp = TempDir::new();
    write_shooter_log(tmp.path());
    let server = MockServer::start().await;
    write_lockfile(tmp.path(), "http", server.address().port(), "pw");
    mount_entitlements(&server, SELF_PUUID).await;
    let session = connect_against(tmp.path(), &server).await;

    Mock::given(method("GET"))
        .and(path(HISTORY_PATH))
        .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "0.4"))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(HISTORY_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "History": [] })))
        .mount(&server)
        .await;
    let start = Instant::now();
    let _: Value = session.pd(HISTORY_PATH).await.expect("retried after 0.4 s");
    assert!(start.elapsed() >= Duration::from_millis(350));
}

#[tokio::test]
async fn an_mmr_hold_does_not_delay_other_calls() {
    let tmp = TempDir::new();
    write_shooter_log(tmp.path());
    let server = MockServer::start().await;
    write_lockfile(tmp.path(), "http", server.address().port(), "pw");
    mount_entitlements(&server, SELF_PUUID).await;
    let session = connect_against(tmp.path(), &server).await;

    Mock::given(method("GET"))
        .and(path(OTHER_MMR_PATH))
        .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "60"))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(HISTORY_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "History": [] })))
        .mount(&server)
        .await;

    let _ = session.pd::<Value>(OTHER_MMR_PATH).await.unwrap_err();
    let start = Instant::now();
    let _: Value = session.pd(HISTORY_PATH).await.expect("other family");
    assert!(start.elapsed() < Duration::from_secs(1));
}

#[tokio::test]
async fn a_429_hold_read_from_one_session_seeds_the_next_one() {
    // R185: Riot's block is per client, not per session. The holds a session met
    // are read when it is replaced and given to its successor, which then
    // refuses both families at once with nothing sent.
    let tmp = TempDir::new();
    write_shooter_log(tmp.path());
    let server = MockServer::start().await;
    write_lockfile(tmp.path(), "http", server.address().port(), "pw");
    mount_entitlements(&server, SELF_PUUID).await;
    let first = connect_against(tmp.path(), &server).await;

    Mock::given(method("GET"))
        .and(path(OTHER_MMR_PATH))
        .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "60"))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(HISTORY_PATH))
        .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "40"))
        .expect(1)
        .mount(&server)
        .await;
    assert!(matches!(
        first.pd::<Value>(OTHER_MMR_PATH).await.unwrap_err(),
        RiotError::RateLimited
    ));
    assert!(matches!(
        first.pd::<Value>(HISTORY_PATH).await.unwrap_err(),
        RiotError::RateLimited
    ));
    let (mmr, other) = first.hold_remaining();
    let (mmr, other) = (mmr.expect("mmr held"), other.expect("other held"));
    assert!(mmr > Duration::from_secs(50) && other > Duration::from_secs(30));
    drop(first);

    let second = connect_against(tmp.path(), &server).await;
    assert_eq!(
        second.hold_remaining(),
        (None, None),
        "a new session starts free until it is seeded"
    );
    second.seed_holds(Some(mmr), Some(other));

    assert!(matches!(
        second.pd::<Value>(OTHER_MMR_PATH).await.unwrap_err(),
        RiotError::Held(_)
    ));
    assert!(matches!(
        second.pd::<Value>(HISTORY_PATH).await.unwrap_err(),
        RiotError::Held(_)
    ));
    // `.expect(1)` on both mocks is checked when the server drops: the
    // second session sent nothing.
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

    let base = server.uri();
    let session = RiotSession::connect_with_overrides(
        tmp.path(),
        ConnectOverrides {
            pd_base: Some(base.clone()),
            glz_base: Some(base.clone()),
            valorant_api_base: Some(base),
            local_timeout: Some(Duration::from_millis(100)),
            remote_timeout: None,
            ..Default::default()
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

    let base = server.uri();
    let session = RiotSession::connect_with_overrides(
        tmp.path(),
        ConnectOverrides {
            pd_base: Some(base.clone()),
            glz_base: Some(base.clone()),
            valorant_api_base: Some(base),
            local_timeout: None,
            remote_timeout: Some(Duration::from_millis(100)),
            ..Default::default()
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
    let base = server.uri();
    let session = RiotSession::connect_with_overrides(
        tmp.path(),
        ConnectOverrides {
            pd_base: Some(base.clone()),
            glz_base: Some(base.clone()),
            valorant_api_base: Some(base),
            local_timeout: Some(Duration::from_millis(100)),
            remote_timeout: Some(Duration::from_millis(100)),
            ..Default::default()
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

/// Like [`session_for`], for a test that wrote `ShooterGame.log` itself.
async fn session_for_written_log(server: &MockServer, tmp: &TempDir) -> RiotSession {
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

    // A token quoted inside a longer string value goes too, in the error text
    // (the log line) and in the dump.
    Mock::given(method("GET"))
        .and(path("/match-details/v1/matches/abc"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "errorCode": "BAD_CLAIMS",
            "message": format!("invalid bearer {jwt} for player"),
        })))
        .mount(&server)
        .await;
    let quoted = session
        .pd::<Value>("/match-details/v1/matches/abc")
        .await
        .unwrap_err()
        .to_string();
    assert!(
        quoted.contains("BAD_CLAIMS") && quoted.contains("invalid bearer"),
        "{quoted}"
    );
    assert!(!quoted.contains(jwt) && !quoted.contains("eyJ"), "{quoted}");
    let dump = session
        .raw_error_captures()
        .into_iter()
        .find(|d| d.key == "match_details")
        .unwrap();
    assert!(
        !dump.contents.contains(jwt) && !dump.contents.contains("eyJ"),
        "{}",
        dump.contents
    );
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

// ---------------------------------------------------------------------
// R159: the client version and User-Agent, as Valorant-Scout sends them
// ---------------------------------------------------------------------

/// The string the game itself reports (a decoded presence blob of a real
/// session).
const GAME_VERSION: &str = "release-13.06-shipping-18-5590001";
const SCOUT_UA: &str = "ShooterGame/13 Windows/10.0.19043.1.256.64bit";

#[tokio::test]
async fn every_remote_request_sends_scouts_user_agent_and_the_presence_version() {
    let tmp = TempDir::new();
    let server = MockServer::start().await;
    mount_presence_with_version(&server, SELF_PUUID, GAME_VERSION).await;
    let session = session_for(&server, &tmp).await;
    assert_eq!(session.client_version(), GAME_VERSION);

    // Only answers when both headers are exactly as sent: one pd path and
    // one glz path.
    for p in [mmr_path(), format!("/pregame/v1/players/{SELF_PUUID}")] {
        Mock::given(method("GET"))
            .and(path(p.as_str()))
            .and(header("User-Agent", SCOUT_UA))
            .and(header("X-Riot-ClientVersion", GAME_VERSION))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
            .mount(&server)
            .await;
    }
    let _: Value = session
        .pd(&mmr_path())
        .await
        .expect("pd carries both headers");
    let _: Value = session
        .glz(&format!("/pregame/v1/players/{SELF_PUUID}"))
        .await
        .expect("glz carries both headers");

    // The local client's calls are unchanged: no User-Agent of ours.
    let requests = server.received_requests().await.unwrap();
    let local: Vec<_> = requests
        .iter()
        .filter(|r| {
            r.url.path() == "/entitlements/v1/token" || r.url.path() == "/chat/v4/presences"
        })
        .collect();
    assert!(!local.is_empty());
    assert!(
        local.iter().all(|r| r.headers.get("user-agent").is_none()),
        "local calls send no User-Agent"
    );
    // Nor do they ask for gzip: compression is for the remote client only
    // (reqwest's `gzip` feature would otherwise make it every client's
    // default). The remote requests do ask.
    let gzip = |r: &&wiremock::Request| {
        r.headers
            .get("accept-encoding")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.contains("gzip"))
    };
    assert!(
        local.iter().all(|r| !gzip(r)),
        "local calls send no Accept-Encoding: gzip"
    );
    let remote: Vec<_> = requests
        .iter()
        .filter(|r| r.url.path() == mmr_path() || r.url.path().starts_with("/pregame/"))
        .collect();
    assert_eq!(remote.len(), 2);
    assert!(remote.iter().all(gzip), "remote calls accept gzip");
}

#[tokio::test]
async fn the_version_source_order_is_presence_then_valorant_api_then_the_log_then_the_literal() {
    // Nothing answers: Scout's literal, named as such.
    let tmp = TempDir::new();
    let server = MockServer::start().await;
    let session = session_for(&server, &tmp).await;
    assert_eq!(session.client_version_source(), VersionSource::Fallback);
    assert_eq!(session.client_version(), "release-09.00");

    // The log's `CI server version:` line alone: the rest of the line, trimmed.
    let tmp = TempDir::new();
    let server = MockServer::start().await;
    write_shooter_log_with_version(tmp.path(), "   release-log-9  ");
    let session = session_for_written_log(&server, &tmp).await;
    assert_eq!(session.client_version_source(), VersionSource::Log);
    assert_eq!(session.client_version(), "release-log-9");

    // valorant-api beats the log.
    let tmp = TempDir::new();
    let server = MockServer::start().await;
    write_shooter_log_with_version(tmp.path(), " release-log-9");
    mount_valorant_api_version(&server, "release-api-9").await;
    let session = session_for_written_log(&server, &tmp).await;
    assert_eq!(session.client_version_source(), VersionSource::ValorantApi);
    assert_eq!(session.client_version(), "release-api-9");

    // The user's own presence beats both, nested or flat.
    for private in [
        json!({"partyPresenceData": {"partyClientVersion": "release-presence-9"}}),
        json!({"partyClientVersion": "release-presence-9"}),
    ] {
        let tmp = TempDir::new();
        let server = MockServer::start().await;
        write_shooter_log_with_version(tmp.path(), " release-log-9");
        mount_valorant_api_version(&server, "release-api-9").await;
        mount_presence_private(&server, SELF_PUUID, private).await;
        let session = session_for_written_log(&server, &tmp).await;
        assert_eq!(session.client_version_source(), VersionSource::Presence);
        assert_eq!(session.client_version(), "release-presence-9");
    }
}

#[tokio::test]
async fn an_empty_or_blank_version_falls_through_at_every_step() {
    for blank in ["", "   "] {
        // presence blank -> valorant-api
        let tmp = TempDir::new();
        let server = MockServer::start().await;
        write_shooter_log_with_version(tmp.path(), blank);
        mount_presence_with_version(&server, SELF_PUUID, blank).await;
        mount_valorant_api_version(&server, "release-api-1").await;
        let session = session_for_written_log(&server, &tmp).await;
        assert_eq!(
            session.client_version_source(),
            VersionSource::ValorantApi,
            "{blank:?}"
        );

        // presence and valorant-api blank -> the log
        let tmp = TempDir::new();
        let server = MockServer::start().await;
        write_shooter_log_with_version(tmp.path(), " release-log-1");
        mount_presence_with_version(&server, SELF_PUUID, blank).await;
        mount_valorant_api_version(&server, blank).await;
        let session = session_for_written_log(&server, &tmp).await;
        assert_eq!(
            session.client_version_source(),
            VersionSource::Log,
            "{blank:?}"
        );
        assert_eq!(session.client_version(), "release-log-1");

        // every step blank -> the literal, and the header is never blank
        let tmp = TempDir::new();
        let server = MockServer::start().await;
        write_shooter_log_with_version(tmp.path(), blank);
        mount_presence_with_version(&server, SELF_PUUID, blank).await;
        mount_valorant_api_version(&server, blank).await;
        let session = session_for_written_log(&server, &tmp).await;
        assert_eq!(
            session.client_version_source(),
            VersionSource::Fallback,
            "{blank:?}"
        );
        assert_eq!(session.client_version(), "release-09.00");
    }
}

#[tokio::test]
async fn a_version_that_could_not_be_sent_as_a_header_falls_through() {
    // Non-ASCII / control characters would fail every remote call at `send()`.
    // Presence -> valorant-api:
    let tmp = TempDir::new();
    let server = MockServer::start().await;
    mount_presence_with_version(&server, SELF_PUUID, "release-é-1").await;
    mount_valorant_api_version(&server, "release-api-ok").await;
    let session = session_for(&server, &tmp).await;
    assert_eq!(session.client_version_source(), VersionSource::ValorantApi);
    // ... valorant-api -> the log:
    let tmp = TempDir::new();
    let server = MockServer::start().await;
    write_shooter_log_with_version(tmp.path(), " release-log-ok");
    mount_valorant_api_version(&server, "bad\u{1}version").await;
    let session = session_for_written_log(&server, &tmp).await;
    assert_eq!(session.client_version_source(), VersionSource::Log);
    // ... the log -> the literal, and a remote call still goes out:
    let tmp = TempDir::new();
    let server = MockServer::start().await;
    write_shooter_log_with_version(tmp.path(), " release-\u{7f}-log");
    let session = session_for_written_log(&server, &tmp).await;
    assert_eq!(session.client_version_source(), VersionSource::Fallback);
    Mock::given(method("GET"))
        .and(path(mmr_path()))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&server)
        .await;
    let _: Value = session.pd(&mmr_path()).await.expect("the call is sendable");
    // And a bad presence value never replaces the version in use.
    let bad = presence_private_b64(&json!({"partyClientVersion": "release-é-2"}));
    assert!(!session.note_presence_version(&bad));
    assert_eq!(session.client_version_source(), VersionSource::Fallback);
}

#[tokio::test]
async fn external_sessions_is_never_asked_for_the_version() {
    let tmp = TempDir::new();
    let server = MockServer::start().await;
    // It would answer with a version; it must not be asked.
    Mock::given(method("GET"))
        .and(path("/product-session/v1/external-sessions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "s": {"productId": "valorant", "version": "release-sessions-must-not-win"}
        })))
        .mount(&server)
        .await;
    mount_valorant_api_version(&server, GAME_VERSION).await;
    let session = session_for(&server, &tmp).await;

    assert_eq!(session.client_version(), GAME_VERSION);
    let asked = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .any(|r| r.url.path().contains("external-sessions"));
    assert!(!asked, "no request to external-sessions any more");
}

#[tokio::test]
async fn the_presence_version_replaces_a_lower_source_once_the_game_shows_up() {
    // The session is built before the game's presence exists: the version
    // comes from valorant-api. The first presence with a version — read by
    // the poll loop, no request of ours — replaces it for the next request.
    let tmp = TempDir::new();
    let server = MockServer::start().await;
    mount_valorant_api_version(&server, "release-api-old").await;
    let session = session_for(&server, &tmp).await;
    assert_eq!(session.client_version(), "release-api-old");
    assert_eq!(session.client_version_source(), VersionSource::ValorantApi);
    assert_eq!(
        session.client_version_and_source(),
        ("release-api-old".to_string(), VersionSource::ValorantApi),
        "both under one lock"
    );

    for (version, p) in [
        ("release-api-old", mmr_path()),
        (GAME_VERSION, format!("/pregame/v1/players/{SELF_PUUID}")),
    ] {
        Mock::given(method("GET"))
            .and(path(p.as_str()))
            .and(header("X-Riot-ClientVersion", version))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
            .mount(&server)
            .await;
    }
    let _: Value = session
        .pd(&mmr_path())
        .await
        .expect("the old version so far");

    let asked_before = server.received_requests().await.unwrap().len();
    let private = presence_private_b64(&json!({
        "partyPresenceData": {"partyClientVersion": GAME_VERSION},
    }));
    assert!(session.note_presence_version(&private), "it changed");
    assert_eq!(
        server.received_requests().await.unwrap().len(),
        asked_before,
        "no request to resolve it"
    );
    assert_eq!(session.client_version(), GAME_VERSION);
    assert_eq!(session.client_version_source(), VersionSource::Presence);
    assert_eq!(
        session.client_version_and_source(),
        (GAME_VERSION.to_string(), VersionSource::Presence)
    );
    let _: Value = session
        .glz(&format!("/pregame/v1/players/{SELF_PUUID}"))
        .await
        .expect("the next request carries the presence version");

    // Nothing more to do once the top source has spoken.
    assert!(!session.note_presence_version(&private));
    let other = presence_private_b64(&json!({"partyClientVersion": "release-other"}));
    assert!(!session.note_presence_version(&other));
    assert_eq!(session.client_version(), GAME_VERSION);
}

#[tokio::test]
async fn a_presence_without_a_usable_version_changes_nothing() {
    let tmp = TempDir::new();
    let server = MockServer::start().await;
    mount_valorant_api_version(&server, "release-api-old").await;
    let session = session_for(&server, &tmp).await;
    for private in [
        json!({"sessionLoopState": "MENUS"}),
        json!({"partyClientVersion": "  "}),
        json!({"partyPresenceData": {"partyClientVersion": ""}}),
    ] {
        assert!(!session.note_presence_version(&presence_private_b64(&private)));
    }
    assert!(!session.note_presence_version("not base64 at all!"));
    assert!(!session.note_presence_version(&presence_private_b64(&json!("not an object"))));
    assert_eq!(session.client_version(), "release-api-old");
    assert_eq!(session.client_version_source(), VersionSource::ValorantApi);

    // The same string from the presence: the source is now the presence,
    // the header value did not change.
    let same = presence_private_b64(&json!({"partyClientVersion": "release-api-old"}));
    assert!(!session.note_presence_version(&same), "no change to report");
    assert_eq!(session.client_version_source(), VersionSource::Presence);
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

fn captured(marker: &str) -> Vec<String> {
    CAPTURE
        .0
        .lock()
        .unwrap()
        .iter()
        .filter(|l| l.contains(marker))
        .cloned()
        .collect()
}

#[tokio::test]
async fn the_client_version_and_its_source_are_logged_once_and_a_change_once() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        log::set_logger(&CAPTURE).unwrap();
        log::set_max_level(log::LevelFilter::Info);
    });
    let tmp = TempDir::new();
    let server = MockServer::start().await;
    write_shooter_log_with_version(tmp.path(), " release-logged-once-1");
    let session = session_for_written_log(&server, &tmp).await;
    // A few requests must not log it again.
    Mock::given(method("GET"))
        .and(path(mmr_path()))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&server)
        .await;
    for _ in 0..3 {
        let _: Value = session.pd(&mmr_path()).await.unwrap();
    }
    let lines = captured("release-logged-once-1");
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].starts_with("INFO "), "info level: {}", lines[0]);
    assert!(lines[0].contains("from log"), "the source: {}", lines[0]);

    // The presence changes it: one more line, naming both, once.
    let private = presence_private_b64(&json!({"partyClientVersion": "release-logged-once-2"}));
    assert!(session.note_presence_version(&private));
    assert!(!session.note_presence_version(&private));
    let lines = captured("release-logged-once-2");
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].starts_with("INFO "), "{}", lines[0]);
    assert!(
        lines[0].contains("presence") && lines[0].contains("release-logged-once-1"),
        "{}",
        lines[0]
    );
}

// ---------------------------------------------------------------------
// R159: gzip on remote calls
// ---------------------------------------------------------------------

fn gzip(bytes: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    enc.write_all(bytes).unwrap();
    enc.finish().unwrap()
}

#[tokio::test]
async fn a_gzip_answer_is_decoded_and_captured_decoded() {
    let tmp = TempDir::new();
    let server = MockServer::start().await;
    let session = session_for(&server, &tmp).await;
    let body = json!({"Subject": SELF_PUUID, "QueueSkills": {}, "pad": "z".repeat(2_000)});
    Mock::given(method("GET"))
        .and(path(mmr_path()))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-encoding", "gzip")
                .set_body_bytes(gzip(body.to_string().as_bytes())),
        )
        .mount(&server)
        .await;

    let got: Value = session.pd(&mmr_path()).await.expect("a gzip body decodes");
    assert_eq!(got["Subject"], SELF_PUUID);
    // The raw capture stores the decoded JSON, not the compressed bytes.
    let captures = session.raw_captures();
    assert_eq!(captures.len(), 1);
    let stored: Value = serde_json::from_str(&captures[0].1).expect("decoded JSON");
    assert_eq!(stored["Subject"], SELF_PUUID);

    // The request said it accepts gzip.
    let asked = server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .find(|r| r.url.path() == mmr_path())
        .unwrap();
    let accept = asked
        .headers
        .get("accept-encoding")
        .expect("Accept-Encoding")
        .to_str()
        .unwrap()
        .to_string();
    assert!(accept.contains("gzip"), "{accept}");

    // A plain (uncompressed) answer still works: `buffer_response` is the same.
    Mock::given(method("GET"))
        .and(path("/match-details/v1/matches/abc"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true})))
        .mount(&server)
        .await;
    let plain: Value = session.pd("/match-details/v1/matches/abc").await.unwrap();
    assert_eq!(plain["ok"], true);
}

#[tokio::test]
async fn a_gzip_error_body_is_decoded_for_the_excerpt() {
    let tmp = TempDir::new();
    let server = MockServer::start().await;
    let session = session_for(&server, &tmp).await;
    Mock::given(method("GET"))
        .and(path(mmr_path()))
        .respond_with(
            ResponseTemplate::new(500)
                .insert_header("content-encoding", "gzip")
                .set_body_bytes(gzip(br#"{"errorCode":"GZIPPED_ERROR"}"#)),
        )
        .mount(&server)
        .await;
    let msg = session
        .pd::<Value>(&mmr_path())
        .await
        .unwrap_err()
        .to_string();
    assert!(msg.contains("GZIPPED_ERROR"), "{msg}");
}

// ---------------------------------------------------------------------
// token refresh (R166)
// ---------------------------------------------------------------------

/// A mock "Riot" whose entitlements answer `access-token-1` once (the connect)
/// and `access-token-2` from then on, and whose `/mmr` path answers 200 only to
/// `Bearer access-token-2` and `bad` to anything else.
async fn token_server(bad: ResponseTemplate) -> (TempDir, MockServer) {
    let tmp = TempDir::new();
    write_shooter_log(tmp.path());
    let server = MockServer::start().await;
    write_lockfile(tmp.path(), "http", server.address().port(), "pw");
    for (token, once) in [("access-token-1", true), ("access-token-2", false)] {
        let m = Mock::given(method("GET"))
            .and(path("/entitlements/v1/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "accessToken": token, "entitlements": [], "issuer": "i",
                "subject": SELF_PUUID, "token": format!("jwt-for-{token}"),
            })));
        if once {
            m.up_to_n_times(1).mount(&server).await;
        } else {
            m.mount(&server).await;
        }
    }
    Mock::given(method("GET"))
        .and(path(mmr_path()))
        .and(header("Authorization", "Bearer access-token-2"))
        .and(header("X-Riot-Entitlements-JWT", "jwt-for-access-token-2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"Subject": SELF_PUUID})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(mmr_path()))
        .respond_with(bad)
        .mount(&server)
        .await;
    (tmp, server)
}

async fn connect_with_age(
    tmp: &TempDir,
    server: &MockServer,
    token_max_age: Option<Duration>,
) -> RiotSession {
    let base = server.uri();
    RiotSession::connect_with_overrides(
        tmp.path(),
        ConnectOverrides {
            pd_base: Some(base.clone()),
            glz_base: Some(base.clone()),
            valorant_api_base: Some(base),
            token_max_age,
            ..Default::default()
        },
    )
    .await
    .expect("connect")
}

async fn count(server: &MockServer, p: &str) -> usize {
    server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.url.path() == p)
        .count()
}

fn bad_claims() -> ResponseTemplate {
    ResponseTemplate::new(400).set_body_json(json!({
        "httpStatus": 400, "errorCode": "BAD_CLAIMS",
        "message": "Failure validating/decoding RSO Access Token"
    }))
}

#[tokio::test]
async fn a_400_bad_claims_refreshes_the_token_and_retries_once() {
    let (tmp, server) = token_server(bad_claims()).await;
    let session = connect_with_age(&tmp, &server, None).await;

    let result: Value = session
        .pd(&mmr_path())
        .await
        .expect("retried with the new token");
    assert_eq!(result["Subject"], SELF_PUUID);
    assert_eq!(
        count(&server, "/entitlements/v1/token").await,
        2,
        "connect + one refresh"
    );
    assert_eq!(
        count(&server, &mmr_path()).await,
        2,
        "the call and its one retry"
    );
}

#[tokio::test]
async fn a_401_and_a_403_refresh_the_token_too() {
    for status in [401u16, 403] {
        let (tmp, server) = token_server(ResponseTemplate::new(status)).await;
        let session = connect_with_age(&tmp, &server, None).await;
        let result: Value = session.pd(&mmr_path()).await.expect("retried");
        assert_eq!(result["Subject"], SELF_PUUID, "{status}");
        assert_eq!(
            count(&server, "/entitlements/v1/token").await,
            2,
            "{status}"
        );
    }
}

#[tokio::test]
async fn a_400_with_another_error_code_or_none_is_not_a_token_problem() {
    for bad in [
        ResponseTemplate::new(400).set_body_json(json!({"errorCode": "BAD_REQUEST"})),
        ResponseTemplate::new(400),
        ResponseTemplate::new(400).set_body_string("not json BAD_CLAIMS"),
    ] {
        let (tmp, server) = token_server(bad).await;
        let session = connect_with_age(&tmp, &server, None).await;
        let err = session.pd::<Value>(&mmr_path()).await.unwrap_err();
        assert!(matches!(err, RiotError::Status(400, _)), "{err:?}");
        assert_eq!(
            count(&server, "/entitlements/v1/token").await,
            1,
            "no refresh"
        );
        assert_eq!(count(&server, &mmr_path()).await, 1, "no retry");
    }
}

#[tokio::test]
async fn a_token_that_stays_rejected_is_refreshed_once_per_call_not_in_a_loop() {
    // The server rejects even the new token (a different problem): one refresh,
    // one retry, then the error.
    let tmp = TempDir::new();
    write_shooter_log(tmp.path());
    let server = MockServer::start().await;
    write_lockfile(tmp.path(), "http", server.address().port(), "pw");
    mount_entitlements(&server, SELF_PUUID).await;
    Mock::given(method("GET"))
        .and(path(mmr_path()))
        .respond_with(bad_claims())
        .mount(&server)
        .await;
    let session = connect_with_age(&tmp, &server, None).await;

    let err = session.pd::<Value>(&mmr_path()).await.unwrap_err();
    assert!(matches!(err, RiotError::Status(400, _)), "{err:?}");
    assert_eq!(count(&server, "/entitlements/v1/token").await, 2);
    assert_eq!(count(&server, &mmr_path()).await, 2);
}

#[tokio::test]
async fn a_token_older_than_the_limit_is_refreshed_before_the_call() {
    // A zero limit: every token is already too old, no waiting needed.
    let (tmp, server) = token_server(bad_claims()).await;
    let session = connect_with_age(&tmp, &server, Some(Duration::ZERO)).await;

    // The mock rejects token 1, so an answer proves the new one was in place
    // before the first request left.
    let result: Value = session.pd(&mmr_path()).await.expect("answered");
    assert_eq!(result["Subject"], SELF_PUUID);
    assert_eq!(
        count(&server, "/entitlements/v1/token").await,
        2,
        "one refresh"
    );
    assert_eq!(
        count(&server, &mmr_path()).await,
        1,
        "refreshed before the call: the call itself never failed"
    );
}

#[tokio::test]
async fn a_fresh_token_is_not_refreshed_again_and_again() {
    // The default limit (50 minutes): a token just fetched is used as it is.
    let tmp = TempDir::new();
    write_shooter_log(tmp.path());
    let server = MockServer::start().await;
    write_lockfile(tmp.path(), "http", server.address().port(), "pw");
    mount_entitlements(&server, SELF_PUUID).await;
    Mock::given(method("GET"))
        .and(path(mmr_path()))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"Subject": SELF_PUUID})))
        .mount(&server)
        .await;
    let session = connect_with_age(&tmp, &server, None).await;
    for _ in 0..3 {
        let _: Value = session.pd(&mmr_path()).await.unwrap();
    }
    assert_eq!(
        count(&server, "/entitlements/v1/token").await,
        1,
        "connect only"
    );
}

#[tokio::test]
async fn a_failing_proactive_refresh_goes_on_with_the_cached_token() {
    let tmp = TempDir::new();
    write_shooter_log(tmp.path());
    let server = MockServer::start().await;
    write_lockfile(tmp.path(), "http", server.address().port(), "pw");
    Mock::given(method("GET"))
        .and(path("/entitlements/v1/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "accessToken": "access-token-1", "entitlements": [], "issuer": "i",
            "subject": SELF_PUUID, "token": "jwt-1",
        })))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/entitlements/v1/token"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(mmr_path()))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"Subject": SELF_PUUID})))
        .mount(&server)
        .await;
    let session = connect_with_age(&tmp, &server, Some(Duration::ZERO)).await;

    for _ in 0..3 {
        let result: Value = session
            .pd(&mmr_path())
            .await
            .expect("the old token still works");
        assert_eq!(result["Subject"], SELF_PUUID);
    }
    assert_eq!(
        count(&server, "/entitlements/v1/token").await,
        2,
        "connect + one failed attempt; the next try waits"
    );
}

#[tokio::test]
async fn concurrent_calls_share_one_refresh() {
    // Reactive: eight calls all hit the expired token at once.
    let (tmp, server) = token_server(bad_claims()).await;
    let session = connect_with_age(&tmp, &server, None).await;
    for result in fan_out(session, 8).await {
        assert_eq!(result.expect("answered")["Subject"], SELF_PUUID);
    }
    assert_eq!(
        count(&server, "/entitlements/v1/token").await,
        2,
        "connect + exactly one shared refresh"
    );

    // Proactive: eight calls find the token too old at once.
    let (tmp, server) = token_server(bad_claims()).await;
    let session = connect_with_age(&tmp, &server, Some(Duration::ZERO)).await;
    for result in fan_out(session, 8).await {
        assert_eq!(result.expect("answered")["Subject"], SELF_PUUID);
    }
    assert_eq!(count(&server, "/entitlements/v1/token").await, 2);
    assert_eq!(
        count(&server, &mmr_path()).await,
        8,
        "no call needed a retry"
    );
}

/// `n` overlapping calls of the `/mmr` path on one session.
async fn fan_out(session: RiotSession, n: usize) -> Vec<Result<Value, RiotError>> {
    let session = std::sync::Arc::new(session);
    let mut set = tokio::task::JoinSet::new();
    for _ in 0..n {
        let session = session.clone();
        set.spawn(async move { session.pd::<Value>(&mmr_path()).await });
    }
    let mut out = Vec::new();
    while let Some(done) = set.join_next().await {
        out.push(done.unwrap());
    }
    out
}
