//! The `live` state's account signal (Riot account switch): the wire state carries the session's own puuid as
//! `selfPuuid`, so the UI learns a switch even when the new account has no self card — a `closed` state for an account
//! the DB has no `players` row for (the chat session gives no Riot ID yet, so not even the name row exists, and My
//! Stats may not be computable either). The poller is the real one; only Riot is mocked.

use opd1_core::wire::HistoryQueue;

use super::*;

/// The second Riot account in this installation, and the two players the accounts met: X in account A's match (hidden
/// on B's screen later), Y in account B's own match.
const B: &str = "bbbbbbbb-1111-2222-3333-444444444444";
const X: &str = HIDDEN;
const Y: &str = STRANGER;
const A_MATCH: &str = "am1";
const B_MATCH: &str = "bm1";

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
    // The switch had to produce a live state although the stage did not change. The account is what makes it a
    // different state (`json["selfPuuid"]` is pinned above); the roster went from ME's card to empty at the same time,
    // so the byte diff is not account-only. Stripping those two keys, the two states are otherwise identical.
    assert_eq!(before.stage, switched.stage);
    let before_json = serde_json::to_value(&before).unwrap();
    let strip = |v: &Value| {
        let mut v = v.clone();
        let o = v.as_object_mut().unwrap();
        o.remove("selfPuuid");
        o.remove("players");
        v
    };
    assert_eq!(
        strip(&before_json),
        strip(&json),
        "only the account and the roster changed"
    );
    assert_ne!(
        before_json, json,
        "the serialized state must change for the switch to be published"
    );
}

fn mplayer(puuid: &str, team_id: &str, name: &str, tag: &str) -> MatchPlayer {
    MatchPlayer {
        puuid: puuid.into(),
        team_id: team_id.into(),
        party_id: None,
        agent_id: Some("agent-jett".into()),
        kills: 10,
        deaths: 8,
        assists: 2,
        score: 4_000,
        rounds: Some(20),
        hs: 4,
        bs: 20,
        ls: 10,
        game_name: Some(name.into()),
        tag_line: Some(tag.into()),
        competitive_tier: Some(15),
        damage: None,
    }
}

fn match_row(match_id: &str, start_ms: i64) -> Match {
    Match {
        match_id: match_id.into(),
        map_id: "/Game/Maps/Ascent/Ascent".into(),
        queue_id: "competitive".into(),
        start_ms,
        winning_team: Some("Blue".into()),
        is_ranked: true,
        rounds_played: Some(20),
        game_length_ms: Some(2_000_000),
    }
}

/// Stores one immutable match and the user's own history-list row (a real details store writes both tables).
async fn store_played(st: &AppState, m: &Match, players: &[MatchPlayer], owner: &str) {
    let (m, players, owner) = (m.clone(), players.to_vec(), owner.to_string());
    let entry = HistoryEntry {
        match_id: m.match_id.clone(),
        queue_id: m.queue_id.clone(),
        start_ms: m.start_ms,
    };
    st.db(move |repo| {
        repo.upsert_match(&m, &[], &players)?;
        repo.index_history(&owner, &[entry])
    })
    .await
    .unwrap();
}

async fn history_ids(st: &AppState) -> Vec<String> {
    crate::v2::history::match_history_for(st, HistoryQueue::All, 0, 50)
        .await
        .unwrap()
        .rows
        .into_iter()
        .map(|r| r.match_id)
        .collect()
}

async fn last5_ids(st: &Arc<AppState>) -> (String, Vec<String>) {
    let s = crate::commands::build_my_stats(st)
        .await
        .unwrap()
        .expect("computable");
    (
        s.puuid.clone(),
        s.last5.into_iter().map(|m| m.match_id).collect(),
    )
}

/// The user's cross-account rule: names learned locally are shared across Riot accounts on the installation (keyed by
/// PUUID, from any stored match — and a live-incognito player is never asked for a name), while matches, history, My
/// Stats and encounters follow the current account only.
#[tokio::test]
async fn names_learned_on_one_account_are_shared_but_account_data_follows_the_session() {
    let h = harness().await;
    let now = now_ms();
    let a_start = now - 7_200_000;
    let b_start = now - 3_600_000;

    // 1. Account A (the harness's ME) played a competitive match naming X; account B has its own match naming Y. The
    //    DB is installation-wide: both are stored while A is the session.
    store_played(
        &h.st,
        &match_row(A_MATCH, a_start),
        &[
            mplayer(ME, "Blue", "Me", "TAG"),
            mplayer(X, "Red", "SharedName", "SHRD"),
        ],
        ME,
    )
    .await;
    store_played(
        &h.st,
        &match_row(B_MATCH, b_start),
        &[
            mplayer(B, "Blue", "Bee", "BEE"),
            mplayer(Y, "Red", "YName", "YNAM"),
        ],
        B,
    )
    .await;
    assert_eq!(
        h.st.db(|repo| repo.encounter_counts(ME, X)).await.unwrap(),
        (0, 1),
        "A met X"
    );
    assert_eq!(
        h.st.db(|repo| repo.encounter_counts(B, X)).await.unwrap(),
        (0, 0),
        "B never met X"
    );

    // 2. The Riot client switches to account B; B's Agent Select roster carries X incognito.
    let mut poller = crate::poll::Poller::new(h.st.clone());
    let b_server = mock_client_of(B, Some(("Bee", "BEE"))).await;
    let (_b_dir, b_session) = session_of(&b_server, B).await;
    poller.drop_session().await;
    poller.on_connected(b_session.clone()).await;
    let mut self_b = roster(B, false);
    self_b.team_id = "Blue".into();
    self_b.team = Team::Ally;
    self_b.is_self_party = true;
    let mut hidden_x = roster(X, true);
    hidden_x.team_id = "Red".into();
    hidden_x.team = Team::Enemy;
    poller
        .apply_roster(
            &b_session,
            Stage::Pregame,
            Some("/Game/Maps/Haven/Haven".into()),
            vec![self_b, hidden_x],
        )
        .await;
    // The roster install publishes from the DB bundles before it starts the worker; stop it so this stays a snapshot
    // of the live stage (nothing here needs a sync).
    poller.stop_worker();

    // 3. X's live card: the name A's match taught the installation, in the remembered style, through X's PUUID.
    let live = h.live_events().pop().expect("a live state for B's roster");
    assert_eq!(live.self_puuid.as_deref(), Some(B));
    assert_eq!(live.stage, Stage::Pregame);
    let x_card = live
        .players
        .iter()
        .find(|p| p.puuid == X)
        .expect("X is on B's roster");
    assert!(!x_card.is_self && !x_card.is_self_party);
    assert_eq!(
        x_card.name,
        Field::Value(DisplayName::Remembered {
            name: "SharedName".into(),
            tag: "SHRD".into(),
            // B was not in A's match: the source is the other match, not B's own.
            source: RememberedSource::OtherMatch,
            from_map_id: None, // the asset tables are not loaded in this harness
            from_start_ms: Some(a_start),
        }),
        "the name learned on A must show on B's screen (shared, keyed by PUUID)"
    );
    assert_eq!(
        x_card.encounters, None,
        "A's match with X is not B's encounter"
    );

    // 4. No name-service request was made for X (live-incognito names are never requested) — and none was needed at
    //    all for this roster: the remembered name came from the DB.
    let mut puts = Vec::new();
    for server in [&h.server, &b_server] {
        puts.extend(
            server
                .received_requests()
                .await
                .unwrap()
                .into_iter()
                .filter(|r| r.method.as_str() == "PUT"),
        );
    }
    assert!(
        puts.iter()
            .all(|r| !String::from_utf8_lossy(&r.body).contains(X)),
        "X was live-incognito: their name must never be requested"
    );
    assert!(puts.is_empty(), "the card needed no name-service answer");

    // 5. The shared name is known to both accounts, with the source of each side; the per-account data stays apart.
    let known_b =
        h.st.db(move |repo| repo.known_name(B, X))
            .await
            .unwrap()
            .expect("known on B");
    assert_eq!(
        (known_b.name.as_str(), known_b.tag.as_str()),
        ("SharedName", "SHRD")
    );
    assert_eq!(known_b.source, RememberedSource::OtherMatch);
    assert_eq!(known_b.start_ms, Some(a_start));
    let known_a =
        h.st.db(move |repo| repo.known_name(ME, X))
            .await
            .unwrap()
            .expect("known on A");
    assert_eq!(
        known_a.source,
        RememberedSource::OwnMatch,
        "A played that match"
    );

    // 6. B's history / My Stats / encounters use only B's match; A's use only A's.
    lock(&h.st.live).self_puuid = Some(B.into());
    assert_eq!(
        history_ids(&h.st).await,
        vec![B_MATCH.to_string()],
        "B's history: only B's match"
    );
    assert_eq!(
        last5_ids(&h.st).await,
        (B.to_string(), vec![B_MATCH.to_string()])
    );
    let x_from_b = crate::details::build(&h.st, X).await.unwrap();
    assert_eq!(x_from_b.card.puuid, X);
    assert!(
        x_from_b.encounters.is_empty(),
        "B never shared a match with X"
    );

    lock(&h.st.live).self_puuid = Some(ME.into());
    assert_eq!(
        history_ids(&h.st).await,
        vec![A_MATCH.to_string()],
        "A's history: only A's match"
    );
    assert_eq!(
        last5_ids(&h.st).await,
        (ME.to_string(), vec![A_MATCH.to_string()])
    );
    let x_from_a = crate::details::build(&h.st, X).await.unwrap();
    assert_eq!(x_from_a.encounters.len(), 1, "A shared that match with X");
    assert_eq!(x_from_a.encounters[0].match_id, A_MATCH);
    assert!(!x_from_a.encounters[0].with);
}
