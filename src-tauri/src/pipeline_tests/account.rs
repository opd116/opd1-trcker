//! The `live` state's account signal (Riot account switch): the wire state carries the session's own puuid as
//! `selfPuuid`, so the UI learns a switch even when the new account has no self card — a `closed` state for an account
//! the DB has no `players` row for (the chat session gives no Riot ID yet, so not even the name row exists, and My
//! Stats may not be computable either). The poller is the real one; only Riot is mocked.

use super::*;

/// The mock Riot client signed in as `puuid` (what a second `RiotSession` connects to): the entitlement subject is the
/// account. With `name`, the local chat session answers the user's own Riot ID too (which creates their `players` row);
/// without it the chat session has no Riot ID yet, as during a fresh switch.
async fn mock_client_of(puuid: &str, name: Option<(&str, &str)>) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/entitlements/v1/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "accessToken": "a", "entitlements": [], "issuer": "i", "subject": puuid, "token": "t"
        })))
        .mount(&server)
        .await;
    let (game_name, game_tag) = name.unwrap_or(("", ""));
    get(
        &server,
        "/chat/v1/session",
        json!({"federated": true, "game_name": game_name, "game_tag": game_tag, "loaded": true,
               "name": "", "pid": "", "puuid": puuid, "region": "eu", "resource": "", "state": "connected"}),
    )
    .await;
    server
}

/// A session against `server` for `puuid`; its own lockfile dir (the harness's one points at its own mock).
async fn session_of(server: &MockServer, puuid: &str) -> (tempfile::TempDir, Arc<RiotSession>) {
    let dir = tempfile::tempdir().unwrap();
    write_client_files(dir.path(), server.address().port());
    let session = connect_mock_session(dir.path(), &server.uri()).await;
    assert_eq!(session.self_puuid(), puuid, "the session's own account");
    (dir, session)
}

/// A switch to an account the DB has never seen, while the game is closed: the live state has no self card at all (no
/// `players` row for the new account), so only `selfPuuid` can tell the UI the account changed.
#[tokio::test]
async fn a_switch_to_a_never_seen_account_is_on_the_live_state_without_a_self_card() {
    let h = harness().await;
    let mut poller = crate::poll::Poller::new(h.st.clone());

    // ME connects (the harness's own session, with a chat-session Riot ID): its Closed live state, with ME's card.
    poller.on_connected(h.session.clone()).await;
    let before = h
        .live_events()
        .into_iter()
        .rev()
        .find(|s| s.self_puuid.as_deref() == Some(ME))
        .expect("a live state for ME");
    assert_eq!(before.stage, Stage::Closed);
    assert!(!before.players.is_empty(), "ME's own card is on screen");

    // The Riot client goes away and comes back signed in as another account the DB has never seen: the chat session
    // gives no Riot ID yet, so no players row is created and the Closed state names nobody on screen.
    poller.drop_session().await;
    let other_server = mock_client_of(OTHER, None).await;
    let (_other_dir, other_session) = session_of(&other_server, OTHER).await;
    assert_eq!(
        lock(&h.st.live).self_puuid.as_deref(),
        Some(ME),
        "the last known account while disconnected"
    );
    poller.on_connected(other_session).await;
    poller.stop_worker();

    let events = h.live_events();
    let switched = events.last().expect("the switch's live state");
    assert_eq!(switched.stage, Stage::Closed);
    assert!(
        switched.players.is_empty(),
        "a never-seen account has no self card: {}",
        serde_json::to_string(switched).unwrap()
    );
    assert_eq!(
        switched.self_puuid.as_deref(),
        Some(OTHER),
        "the new account is on the wire even without a self card"
    );
    // Its JSON names the account (the UI's signal), and the previous account's puuid is gone.
    let json = serde_json::to_value(switched).unwrap();
    assert_eq!(json["selfPuuid"], OTHER);
    assert!(json["players"].as_array().unwrap().is_empty());
    let first_other = events
        .iter()
        .position(|s| s.self_puuid.as_deref() == Some(OTHER))
        .expect("an event for the new account");
    assert!(
        events[first_other..]
            .iter()
            .all(|s| s.self_puuid.as_deref() != Some(ME)),
        "the previous account's puuid survived the switch"
    );
    // No card of the previous account is ever published under the new puuid (the switch clears the old roster before
    // anything is emitted for the new session).
    for e in &events[first_other..] {
        assert!(
            e.players.is_empty(),
            "an old account's card went out under the new puuid: {}",
            serde_json::to_string(e).unwrap()
        );
    }
    // The event was emitted although nothing on screen changed: only the account did.
    assert_eq!(before.stage, switched.stage);
    assert_ne!(
        serde_json::to_value(before).unwrap(),
        json,
        "the serialized state must change for the switch to be published"
    );
}
