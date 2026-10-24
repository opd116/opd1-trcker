//! R189's pipeline tests: the `"status"` feed the poll loop keeps (the
//! connection pill's `error` / `waiting` / `ok` / `lost`, the Agent Select
//! line, the queue timer, the recap pointer), "Retry now", the saved-player
//! alert and `profile_extras`. A child of `pipeline_tests`, so it shares that
//! module's harness (mock Riot client, in-memory DB) through `super::*`.
//!
//! A disconnected tick waits 5 s after it decides; every test asks for a
//! retry first (`AppState::request_retry`), which is exactly what cuts that
//! wait short, so the tick returns at once.

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;

use opd1_core::wire::{AppStatus, ConnState, PregameInfo, QueueInfo};

use super::*;
use crate::pipeline_tests::backfill::{entry_id, mount_feed, Feed};
use crate::poll::{
    Poller, CLIENT_UNREACHABLE, LOCKFILE_NOT_FOUND, LOST_MAX_ATTEMPTS, VALORANT_STARTED,
};

const VERSION: &str = "release-11.08-shipping-9-3104338";

fn status(h: &Harness) -> AppStatus {
    h.st.status()
}

/// The presence blob the switchable mock serves: any JSON object.
type Blob = Arc<Mutex<Option<Value>>>;

/// `/chat/v4/presences` answering the user's VALORANT presence with the blob
/// in the cell (`None`: no VALORANT presence at all), or HTTP 500 while
/// `fail` is set.
async fn presence(h: &Harness, blob: Value) -> (Blob, Arc<std::sync::atomic::AtomicBool>) {
    let cell: Blob = Arc::new(Mutex::new(Some(blob)));
    let fail = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (served, failing) = (cell.clone(), fail.clone());
    Mock::given(method("GET"))
        .and(path("/chat/v4/presences"))
        .respond_with(move |_: &Request| {
            if failing.load(std::sync::atomic::Ordering::SeqCst) {
                return ResponseTemplate::new(500);
            }
            let presences = match served.lock().unwrap().clone() {
                Some(blob) => vec![json!({
                    "puuid": ME, "product": "valorant",
                    "private": BASE64.encode(blob.to_string())
                })],
                None => Vec::new(),
            };
            ResponseTemplate::new(200).set_body_json(json!({ "presences": presences }))
        })
        .with_priority(1)
        .mount(&h.server)
        .await;
    (cell, fail)
}

fn blob(state: &str, queue: &str) -> Value {
    json!({"sessionLoopState": state, "queueId": queue, "partyId": "party-1",
           "partyClientVersion": VERSION})
}

/// A Riot Client folder whose lockfile names a port nothing listens on.
fn dead_client_dir() -> tempfile::TempDir {
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir
        .path()
        .join("Riot Games")
        .join("Riot Client")
        .join("Config");
    std::fs::create_dir_all(&cfg).unwrap();
    std::fs::write(
        cfg.join("lockfile"),
        format!("Riot Client:1:{port}:pw:http"),
    )
    .unwrap();
    dir
}

/// One disconnected tick, its 5 s wait cut short by a retry request.
async fn disconnected_tick_now(h: &Harness, poller: &mut Poller) {
    h.st.request_retry();
    tokio::time::timeout(Duration::from_secs(20), poller.disconnected_tick())
        .await
        .expect("the retry request cut the wait short");
}

// ---------------------------------------------------------------------
// the connection pill
// ---------------------------------------------------------------------

#[tokio::test]
async fn a_missing_lockfile_is_an_error_with_its_code_and_the_next_check() {
    let h = harness().await;
    let empty = tempfile::tempdir().unwrap();
    let mut poller = Poller::held(h.st.clone());
    poller.local_app_data = Some(empty.path().to_path_buf());
    disconnected_tick_now(&h, &mut poller).await;
    let s = status(&h);
    assert_eq!(s.conn, ConnState::Error);
    assert_eq!(s.error_code.as_deref(), Some(LOCKFILE_NOT_FOUND));
    assert_eq!(s.retry_in_ms, Some(5_000));
    let since = s.error_at_ms.expect("the error has a time");
    // The same error again: one event, and its time is the first one's.
    disconnected_tick_now(&h, &mut poller).await;
    assert_eq!(status(&h).error_at_ms, Some(since));
    assert_eq!(h.status_events().len(), 1, "no event without a change");

    // No Riot Client location at all is the same error.
    let h = harness().await;
    let mut poller = Poller::held(h.st.clone());
    poller.local_app_data = None;
    disconnected_tick_now(&h, &mut poller).await;
    assert_eq!(status(&h).error_code.as_deref(), Some(LOCKFILE_NOT_FOUND));
}

#[tokio::test]
async fn a_client_that_does_not_answer_is_client_unreachable() {
    let h = harness().await;
    let dir = dead_client_dir();
    let mut poller = Poller::held(h.st.clone());
    poller.local_app_data = Some(dir.path().to_path_buf());
    disconnected_tick_now(&h, &mut poller).await;
    let s = status(&h);
    assert_eq!(s.conn, ConnState::Error);
    assert_eq!(s.error_code.as_deref(), Some(CLIENT_UNREACHABLE));
    assert_eq!(s.retry_in_ms, Some(5_000), "the first connect backoff");
    assert_eq!(s.attempt, 0);
}

#[tokio::test]
async fn the_client_up_with_the_game_closed_is_waiting_and_the_game_up_is_ok() {
    let h = harness().await;
    let (cell, _) = presence(&h, blob("MENUS", "unrated")).await;
    *cell.lock().unwrap() = None;
    let mut poller = Poller::held(h.st.clone());
    poller.on_connected(h.session.clone()).await;
    assert_eq!(
        status(&h).conn,
        ConnState::Waiting,
        "connected, game unknown"
    );
    assert_eq!(status(&h).error_code, None);

    // VALORANT absent twice: closed, still waiting; the client answered.
    let before = now_ms();
    poller.presence_tick(&h.session).await.unwrap();
    poller.presence_tick(&h.session).await.unwrap();
    let s = status(&h);
    assert_eq!(s.conn, ConnState::Waiting);
    assert!(s.last_data_ms.is_some_and(|t| t >= before));
    assert_eq!(s.pregame, None);

    // The game starts: the game-less session is rebuilt on purpose (R46),
    // still waiting meanwhile; under the new one the game is up: ok, with the
    // versions the session knows (R159).
    *cell.lock().unwrap() = Some(blob("MENUS", "unrated"));
    assert!(matches!(
        poller.presence_tick(&h.session).await,
        Err(crate::poll::Stop::Reconnect(why)) if why == VALORANT_STARTED
    ));
    assert_eq!(status(&h).conn, ConnState::Waiting);
    poller.on_connected(h.session.clone()).await;
    poller.presence_tick(&h.session).await.unwrap();
    let s = status(&h);
    assert_eq!(s.conn, ConnState::Ok);
    assert_eq!(s.riot_client_version.as_deref(), Some(VERSION));
    assert_eq!(s.game_version.as_deref(), Some("release-11.08"));
    assert_eq!((s.attempt, s.retry_in_ms), (0, None));
}

#[tokio::test]
async fn a_presence_every_two_seconds_does_not_emit_a_status_every_two_seconds() {
    let h = harness().await;
    presence(&h, blob("MENUS", "unrated")).await;
    let mut poller = Poller::held(h.st.clone());
    poller.on_connected(h.session.clone()).await;
    poller.presence_tick(&h.session).await.unwrap();
    let events = h.status_events().len();
    for _ in 0..5 {
        poller.presence_tick(&h.session).await.unwrap();
    }
    assert_eq!(
        h.status_events().len(),
        events,
        "a newer last-data time alone waits out the 5 s"
    );
    // The command still answers the exact time.
    let emitted = h.status_events().last().unwrap().last_data_ms.unwrap();
    assert!(h.st.status().last_data_ms.unwrap() >= emitted);
}

#[tokio::test]
async fn failing_polls_while_the_game_runs_are_lost_with_attempts_then_the_lockfile_decides() {
    let h = harness().await;
    let (_, fail) = presence(&h, blob("MENUS", "unrated")).await;
    let mut poller = Poller::held(h.st.clone());
    poller.on_connected(h.session.clone()).await;
    poller.presence_tick(&h.session).await.unwrap();
    assert_eq!(status(&h).conn, ConnState::Ok);

    fail.store(true, std::sync::atomic::Ordering::SeqCst);
    for attempt in 1..LOST_MAX_ATTEMPTS {
        poller.presence_tick(&h.session).await.unwrap();
        let s = status(&h);
        assert_eq!(s.conn, ConnState::Lost, "attempt {attempt}");
        assert_eq!((s.attempt, s.max_attempts), (attempt, LOST_MAX_ATTEMPTS));
        assert_eq!(s.retry_in_ms, Some(2_000), "the next presence read");
    }
    // The last one rebuilds the session (the existing rule): retry at once.
    assert!(poller.presence_tick(&h.session).await.is_err());
    let s = status(&h);
    assert_eq!((s.conn, s.attempt), (ConnState::Lost, LOST_MAX_ATTEMPTS));
    assert_eq!(s.retry_in_ms, Some(0));

    // The poll loop drops the session; the lockfile is gone: error.
    poller.note_session_lost("presence keeps failing");
    poller.drop_session().await;
    let empty = tempfile::tempdir().unwrap();
    poller.local_app_data = Some(empty.path().to_path_buf());
    disconnected_tick_now(&h, &mut poller).await;
    let s = status(&h);
    assert_eq!(s.conn, ConnState::Error);
    assert_eq!(s.error_code.as_deref(), Some(LOCKFILE_NOT_FOUND));
    assert_eq!(s.attempt, 0);
}

#[tokio::test]
async fn a_reconnect_whose_presence_still_fails_is_unreachable_not_waiting() {
    let h = harness().await;
    let (_, fail) = presence(&h, blob("MENUS", "unrated")).await;
    let mut poller = Poller::held(h.st.clone());
    poller.on_connected(h.session.clone()).await;
    poller.presence_tick(&h.session).await.unwrap();
    fail.store(true, std::sync::atomic::Ordering::SeqCst);
    for _ in 1..LOST_MAX_ATTEMPTS {
        poller.presence_tick(&h.session).await.unwrap();
    }
    assert!(poller.presence_tick(&h.session).await.is_err());
    poller.note_session_lost("presence keeps failing");
    poller.drop_session().await;

    // The client answers a connect, but its presence still fails: the game
    // may be running, so the pill never says "waiting for VALORANT".
    poller.on_connected(h.session.clone()).await;
    assert_eq!(status(&h).conn, ConnState::Lost);
    poller.presence_tick(&h.session).await.unwrap();
    let s = status(&h);
    assert_eq!(s.conn, ConnState::Error);
    assert_eq!(s.error_code.as_deref(), Some(CLIENT_UNREACHABLE));

    // A presence answers again: the pill follows the game.
    fail.store(false, std::sync::atomic::Ordering::SeqCst);
    poller.presence_tick(&h.session).await.unwrap();
    assert_eq!(status(&h).conn, ConnState::Ok);
}

#[tokio::test]
async fn a_lost_client_counts_its_reconnects_then_falls_to_unreachable() {
    let h = harness().await;
    presence(&h, blob("MENUS", "unrated")).await;
    let mut poller = Poller::held(h.st.clone());
    poller.on_connected(h.session.clone()).await;
    poller.presence_tick(&h.session).await.unwrap();

    // The client vanished mid-session (ClientGone) while the game ran.
    poller.note_session_lost("Riot client closed");
    assert_eq!((status(&h).conn, status(&h).attempt), (ConnState::Lost, 1));
    poller.drop_session().await;
    let dir = dead_client_dir();
    poller.local_app_data = Some(dir.path().to_path_buf());
    // Each failed reconnect is one more attempt, its wait the backoff's.
    let waits = [5_000, 10_000, 20_000, 40_000];
    for (i, wait) in waits.iter().enumerate() {
        disconnected_tick_now(&h, &mut poller).await;
        let s = status(&h);
        assert_eq!(s.conn, ConnState::Lost, "reconnect {i}");
        assert_eq!(s.attempt, i as u32 + 2);
        assert_eq!(s.retry_in_ms, Some(*wait));
    }
    assert_eq!(status(&h).attempt, LOST_MAX_ATTEMPTS);
    // Out of attempts: what the lockfile says — there, but nobody answers.
    disconnected_tick_now(&h, &mut poller).await;
    let s = status(&h);
    assert_eq!(s.conn, ConnState::Error);
    assert_eq!(s.error_code.as_deref(), Some(CLIENT_UNREACHABLE));
}

#[tokio::test]
async fn a_lost_client_that_answers_again_is_waiting_then_ok() {
    let h = harness().await;
    presence(&h, blob("INGAME", "unrated")).await;
    let mut poller = Poller::held(h.st.clone());
    poller.on_connected(h.session.clone()).await;
    poller.note_session_lost("Riot client closed"); // stage Closed: not lost
    assert_eq!(status(&h).conn, ConnState::Waiting);
    poller.lost_attempt = 3;
    poller.on_connected(h.session.clone()).await;
    assert_eq!(
        (status(&h).conn, status(&h).attempt),
        (ConnState::Waiting, 0)
    );
    assert_eq!(poller.lost_attempt, 0);
}

#[tokio::test]
async fn valorant_starting_is_no_lost_connection_and_neither_is_a_failure_with_the_game_closed() {
    let h = harness().await;
    let (cell, fail) = presence(&h, blob("MENUS", "unrated")).await;
    let mut poller = Poller::held(h.st.clone());
    poller.on_connected(h.session.clone()).await;
    poller.presence_tick(&h.session).await.unwrap();
    poller.note_session_lost(VALORANT_STARTED);
    assert_eq!(status(&h).conn, ConnState::Ok, "a deliberate rebuild");

    // The game closes (absent twice): waiting; a failing presence then is
    // the client's business, not a lost game.
    *cell.lock().unwrap() = None;
    poller.presence_tick(&h.session).await.unwrap();
    poller.presence_tick(&h.session).await.unwrap();
    assert_eq!(status(&h).conn, ConnState::Waiting);
    fail.store(true, std::sync::atomic::Ordering::SeqCst);
    poller.presence_tick(&h.session).await.unwrap();
    assert_eq!(status(&h).conn, ConnState::Waiting);
}

#[tokio::test]
async fn retry_now_wakes_a_waiting_loop_at_once() {
    let h = harness().await;
    let empty = tempfile::tempdir().unwrap();
    let mut poller = Poller::held(h.st.clone());
    poller.local_app_data = Some(empty.path().to_path_buf());
    // No request pending: the tick sleeps its 5 s.
    let st = h.st.clone();
    let tick = tokio::spawn(async move {
        poller.disconnected_tick().await;
        poller
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!tick.is_finished(), "waiting out the 5 s check");
    st.request_retry();
    let poller = tokio::time::timeout(Duration::from_secs(2), tick)
        .await
        .expect("woken at once")
        .unwrap();
    drop(poller);
}

// ---------------------------------------------------------------------
// Agent Select, queue, recap
// ---------------------------------------------------------------------

fn pregame_player(puuid: &str, state: &str, incognito: bool) -> Value {
    json!({"Subject": puuid, "CharacterID": if state.is_empty() { "" } else { "agent" },
           "CharacterSelectionState": state,
           "PlayerIdentity": {"Incognito": incognito, "HideAccountLevel": false,
                              "AccountLevel": 10, "PlayerCardID": "card"}})
}

/// Agent Select `pg-1`; its answer is the cell's (switchable mid-test).
async fn mock_pregame(h: &Harness, body: Value) -> Arc<Mutex<Value>> {
    let cell = Arc::new(Mutex::new(body));
    let served = cell.clone();
    get(
        &h.server,
        &format!("/pregame/v1/players/{ME}"),
        json!({"Subject": ME, "MatchID": "pg-1"}),
    )
    .await;
    Mock::given(method("GET"))
        .and(path("/pregame/v1/matches/pg-1"))
        .respond_with(move |_: &Request| {
            ResponseTemplate::new(200).set_body_json(served.lock().unwrap().clone())
        })
        .mount(&h.server)
        .await;
    cell
}

fn pregame_body(players: Vec<Value>, remaining_ns: i64) -> Value {
    json!({"MapID": SPLIT_PATH, "AllyTeam": {"TeamID": "Blue", "Players": players},
           "PhaseTimeRemainingNS": remaining_ns})
}

#[tokio::test]
async fn agent_select_shows_its_timer_and_lock_count_and_ends_the_recap() {
    let h = harness().await;
    let (cell, _) = presence(&h, blob("MENUS", "competitive")).await;
    let pregame = mock_pregame(
        &h,
        pregame_body(
            vec![
                pregame_player(ME, "locked", false),
                pregame_player(OTHER, "selected", false),
                pregame_player(HIDDEN, "", true),
            ],
            40_000_000_000,
        ),
    )
    .await;
    let mut poller = Poller::held(h.st.clone());
    poller.on_connected(h.session.clone()).await;
    poller.presence_tick(&h.session).await.unwrap();
    h.st.set_status(|s| s.recap_match_id = Some("old-match".into()));

    *cell.lock().unwrap() = Some(blob("PREGAME", "competitive"));
    let before = now_ms();
    poller.presence_tick(&h.session).await.unwrap();
    let s = status(&h);
    let info = s.pregame.clone().expect("Agent Select line");
    assert_eq!((info.locked, info.total), (1, 3), "a hover is not a lock");
    let ends = info.phase_ends_ms.unwrap();
    assert!(ends >= before + 40_000 && ends <= now_ms() + 40_000);
    assert_eq!(s.recap_match_id, None, "Agent Select ends the recap");

    // A re-poll: two more locks, the same deadline (a little less left).
    *pregame.lock().unwrap() = pregame_body(
        vec![
            pregame_player(ME, "locked", false),
            pregame_player(OTHER, "locked", false),
            pregame_player(HIDDEN, "locked", true),
        ],
        39_800_000_000,
    );
    poller.pregame_repoll(&h.session).await.unwrap();
    assert_eq!(
        status(&h).pregame,
        Some(PregameInfo {
            phase_ends_ms: Some(ends),
            locked: 3,
            total: 3
        })
    );

    // Back to the Lobby (a dodge): no Agent Select line.
    *cell.lock().unwrap() = Some(blob("MENUS", "competitive"));
    poller.presence_tick(&h.session).await.unwrap();
    assert_eq!(status(&h).pregame, None);
}

fn queueing(state: &str, entry: &str) -> Value {
    json!({"sessionLoopState": "MENUS", "queueId": "competitive", "partyId": "party-1",
           "partyClientVersion": VERSION,
           "partyPresenceData": {"partyState": state, "queueEntryTime": entry}})
}

#[tokio::test]
async fn matchmaking_in_the_lobby_shows_the_queue_timer_and_ends_the_recap() {
    let h = harness().await;
    let (cell, _) = presence(&h, queueing("DEFAULT", "0001.01.01-00.00.00")).await;
    let mut poller = Poller::held(h.st.clone());
    poller.on_connected(h.session.clone()).await;
    poller.presence_tick(&h.session).await.unwrap();
    assert_eq!(status(&h).queue, None);
    h.st.set_status(|s| s.recap_match_id = Some("old-match".into()));

    *cell.lock().unwrap() = Some(queueing("MATCHMAKING", "2026.10.22-18.05.09"));
    poller.presence_tick(&h.session).await.unwrap();
    let s = status(&h);
    assert_eq!(
        s.queue,
        Some(QueueInfo {
            queue_id: "competitive".into(),
            searching_since_ms: 1_792_692_309_000
        })
    );
    assert_eq!(s.recap_match_id, None, "queueing again ends the recap");

    // No entry time: the timer starts when the app first saw the search,
    // and keeps that start.
    *cell.lock().unwrap() = Some(queueing("MATCHMAKING", "0001.01.01-00.00.00"));
    let before = now_ms();
    poller.presence_tick(&h.session).await.unwrap();
    let since = status(&h).queue.unwrap().searching_since_ms;
    assert!(since >= before);
    poller.presence_tick(&h.session).await.unwrap();
    assert_eq!(status(&h).queue.unwrap().searching_since_ms, since);

    // Search cancelled.
    *cell.lock().unwrap() = Some(queueing("DEFAULT", "0001.01.01-00.00.00"));
    poller.presence_tick(&h.session).await.unwrap();
    assert_eq!(status(&h).queue, None);
}

/// `core-game` player + match `bf000` (the feed's newest match) with ME and
/// OTHER.
async fn mock_coregame(h: &Harness) {
    let player = |puuid: &str, team: &str| {
        json!({"Subject": puuid, "TeamID": team, "CharacterID": "agent",
               "PlayerIdentity": {"Incognito": false, "HideAccountLevel": false,
                                  "AccountLevel": 10, "PlayerCardID": "card"}})
    };
    get(
        &h.server,
        &format!("/core-game/v1/players/{ME}"),
        json!({"Subject": ME, "MatchID": entry_id(0)}),
    )
    .await;
    get(
        &h.server,
        &format!("/core-game/v1/matches/{}", entry_id(0)),
        json!({"MapID": SPLIT_PATH, "Players": [player(ME, "Blue"), player(OTHER, "Red")]}),
    )
    .await;
}

async fn stored(h: &Harness, match_id: &str) -> bool {
    let id = match_id.to_string();
    h.st.db(move |repo| repo.match_rows(&id))
        .await
        .flatten()
        .is_some()
}

#[tokio::test]
async fn a_match_end_points_the_recap_at_it_and_its_details_are_stored() {
    let h = harness().await;
    mount_feed(&h.server, Feed::new(3)).await;
    mock_coregame(&h).await;
    let (cell, _) = presence(&h, blob("INGAME", "unrated")).await;
    let mut poller = Poller::new(h.st.clone());
    poller.ingame_loadouts_timeout = Duration::from_millis(50);
    poller.on_connected(h.session.clone()).await;
    poller.presence_tick(&h.session).await.unwrap();
    assert_eq!(h.st.live_state().stage, Stage::Ingame);
    assert_eq!(status(&h).recap_match_id, None);

    *cell.lock().unwrap() = Some(blob("MENUS", "unrated"));
    poller.presence_tick(&h.session).await.unwrap();
    let recap = status(&h)
        .recap_match_id
        .expect("recap set at the match end");
    assert_eq!(recap, entry_id(0));

    // The after-match backfill stores it (no extra request for the recap),
    // so `match_details(recap, self)` reads it from the DB.
    wait_until_async(&h, &recap).await;
    let details_path = format!("/match-details/v1/matches/{recap}");
    let fetched = count(&h.server, "GET", &details_path).await;
    assert_eq!(fetched, 1);
    let view = crate::match_details::load(&h.st, &recap, ME)
        .await
        .expect("the recap's match is found");
    assert_eq!(view.match_id, recap);
    assert_eq!(
        count(&h.server, "GET", &details_path).await,
        fetched,
        "found in the DB, not fetched again"
    );
    poller.stop_worker();
    // Agent Select of the next match ends the recap (see the Agent Select
    // test); the game closing does not.
    *cell.lock().unwrap() = None;
    poller.presence_tick(&h.session).await.unwrap();
    poller.presence_tick(&h.session).await.unwrap();
    assert_eq!(status(&h).recap_match_id.as_deref(), Some(recap.as_str()));
}

async fn wait_until_async(h: &Harness, match_id: &str) {
    for _ in 0..1_500 {
        if stored(h, match_id).await {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("{match_id} never stored");
}

#[tokio::test]
async fn a_match_end_forgets_this_matchs_name_answers() {
    let h = harness().await;
    mock_coregame(&h).await;
    let (cell, _) = presence(&h, blob("INGAME", "unrated")).await;
    let mut poller = Poller::held(h.st.clone());
    poller.ingame_loadouts_timeout = Duration::from_millis(50);
    poller.on_connected(h.session.clone()).await;
    poller.presence_tick(&h.session).await.unwrap();
    assert_eq!(h.st.live_state().stage, Stage::Ingame);
    lock(&h.st.live)
        .known_names
        .insert(OTHER.into(), ("Old".into(), "NAME".into()));

    // Back in the menus: the next match asks name-service again, so a
    // renamed player shows the new name.
    *cell.lock().unwrap() = Some(blob("MENUS", "unrated"));
    poller.presence_tick(&h.session).await.unwrap();
    assert!(lock(&h.st.live).known_names.is_empty());
}

#[tokio::test]
async fn the_game_closing_in_game_points_the_recap_at_the_match_too() {
    let h = harness().await;
    mock_coregame(&h).await;
    let (cell, _) = presence(&h, blob("INGAME", "unrated")).await;
    let mut poller = Poller::held(h.st.clone());
    poller.ingame_loadouts_timeout = Duration::from_millis(50);
    poller.on_connected(h.session.clone()).await;
    poller.presence_tick(&h.session).await.unwrap();
    *cell.lock().unwrap() = None;
    poller.presence_tick(&h.session).await.unwrap();
    poller.presence_tick(&h.session).await.unwrap();
    assert_eq!(h.st.live_state().stage, Stage::Closed);
    assert_eq!(status(&h).recap_match_id, Some(entry_id(0)));
    assert_eq!(status(&h).conn, ConnState::Waiting);
}

#[tokio::test]
async fn an_account_switch_ends_the_previous_accounts_recap() {
    let h = harness().await;
    lock(&h.st.live).self_puuid = Some("previous-account".into());
    h.st.set_status(|s| s.recap_match_id = Some("old-match".into()));
    let mut poller = Poller::held(h.st.clone());
    poller.on_connected(h.session.clone()).await;
    assert_eq!(status(&h).recap_match_id, None);
}

#[tokio::test]
async fn a_reconnect_of_the_same_account_keeps_its_recap() {
    let h = harness().await;
    lock(&h.st.live).self_puuid = Some(ME.into());
    h.st.set_status(|s| s.recap_match_id = Some("my-match".into()));
    let mut poller = Poller::held(h.st.clone());
    poller.on_connected(h.session.clone()).await;
    assert_eq!(status(&h).recap_match_id.as_deref(), Some("my-match"));
}

// ---------------------------------------------------------------------
// the saved-player alert
// ---------------------------------------------------------------------

/// Migration 4's two tables (another lane's; created here so the test runs
/// before that merge) with OTHER, HIDDEN and the user saved, and a note on
/// OTHER.
async fn save_players(h: &Harness) {
    h.st.db(|repo| {
        repo.conn().execute_batch(
            "CREATE TABLE IF NOT EXISTS saved_players (
                 puuid TEXT PRIMARY KEY, saved_ms INTEGER NOT NULL);
             CREATE TABLE IF NOT EXISTS player_notes (
                 puuid TEXT PRIMARY KEY, note TEXT NOT NULL, updated_ms INTEGER NOT NULL);",
        )?;
        for p in [OTHER, HIDDEN, ME] {
            repo.conn().execute(
                "INSERT INTO saved_players (puuid, saved_ms) VALUES (?1, 1)",
                [p],
            )?;
        }
        repo.conn().execute(
            "INSERT INTO player_notes (puuid, note, updated_ms) VALUES (?1, 'tilts fast', 1)",
            [OTHER],
        )?;
        save_player(
            repo.conn(),
            &PlayerUpdate {
                puuid: OTHER.into(),
                name: Some(("Other".into(), "OTH".into())),
                account_level: 10,
                card_id: "card".into(),
            },
            1,
        )
    })
    .await
    .unwrap();
}

type Alerts = Arc<Mutex<Vec<(String, String)>>>;

fn record_alerts(h: &Harness) -> Alerts {
    let seen: Alerts = Arc::default();
    let into = seen.clone();
    h.st.set_alert_hook(Box::new(move |name, note| {
        into.lock()
            .unwrap()
            .push((name.to_string(), note.to_string()));
    }));
    seen
}

#[tokio::test]
async fn a_saved_player_in_agent_select_is_announced_once_with_the_note() {
    let h = harness().await;
    save_players(&h).await;
    let alerts = record_alerts(&h);
    let (cell, _) = presence(&h, blob("MENUS", "competitive")).await;
    mock_pregame(
        &h,
        pregame_body(
            vec![
                pregame_player(ME, "locked", false),
                pregame_player(OTHER, "selected", false),
                pregame_player(HIDDEN, "", true),
            ],
            40_000_000_000,
        ),
    )
    .await;
    let mut poller = Poller::held(h.st.clone());
    poller.on_connected(h.session.clone()).await;
    poller.presence_tick(&h.session).await.unwrap();
    assert!(alerts.lock().unwrap().is_empty(), "nothing in the Lobby");

    *cell.lock().unwrap() = Some(blob("PREGAME", "competitive"));
    poller.presence_tick(&h.session).await.unwrap();
    let mut got = alerts.lock().unwrap().clone();
    got.sort();
    assert_eq!(
        got,
        [
            ("Hidden Player".to_string(), String::new()),
            ("Other#OTH".to_string(), "tilts fast".to_string()),
        ],
        "both saved players, never the user; the hidden one as the card shows"
    );
    // Re-polls of the same match announce nobody again.
    poller.pregame_repoll(&h.session).await.unwrap();
    poller.pregame_repoll(&h.session).await.unwrap();
    assert_eq!(alerts.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn the_alert_is_off_when_the_setting_is_off_and_nothing_breaks_without_the_tables() {
    // Without migration 4's tables: no alert, no error.
    let h = harness().await;
    let alerts = record_alerts(&h);
    let (cell, _) = presence(&h, blob("MENUS", "competitive")).await;
    mock_pregame(
        &h,
        pregame_body(vec![pregame_player(ME, "locked", false)], 1),
    )
    .await;
    let mut poller = Poller::held(h.st.clone());
    poller.on_connected(h.session.clone()).await;
    *cell.lock().unwrap() = Some(blob("PREGAME", "competitive"));
    poller.presence_tick(&h.session).await.unwrap();
    assert_eq!(h.st.live_state().stage, Stage::Pregame);
    assert!(alerts.lock().unwrap().is_empty());

    // Saved players, but Settings say no alert.
    let h = harness().await;
    save_players(&h).await;
    let off = serde_json::to_string(&opd1_core::wire::Settings {
        saved_alert: false,
        ..Default::default()
    })
    .unwrap();
    h.st.db(move |repo| repo.set_state(crate::v2::settings::SETTINGS_KEY, &off))
        .await
        .unwrap();
    let alerts = record_alerts(&h);
    let (cell, _) = presence(&h, blob("MENUS", "competitive")).await;
    mock_pregame(
        &h,
        pregame_body(
            vec![
                pregame_player(ME, "locked", false),
                pregame_player(OTHER, "locked", false),
            ],
            1,
        ),
    )
    .await;
    let mut poller = Poller::held(h.st.clone());
    poller.on_connected(h.session.clone()).await;
    *cell.lock().unwrap() = Some(blob("PREGAME", "competitive"));
    poller.presence_tick(&h.session).await.unwrap();
    assert!(alerts.lock().unwrap().is_empty());
}

// ---------------------------------------------------------------------
// profile_extras
// ---------------------------------------------------------------------

/// A competitive match of the current act on `map` with these players (all
/// Blue, Blue won).
async fn store_comp_match(h: &Harness, id: &str, map: &str, start_ms: i64, players: &[&str]) {
    let m = Match {
        match_id: id.into(),
        map_id: map.into(),
        queue_id: "competitive".into(),
        start_ms,
        winning_team: Some("Blue".into()),
        is_ranked: true,
        rounds_played: Some(20),
        game_length_ms: None,
    };
    let teams = vec![MatchTeam {
        team_id: "Blue".into(),
        won: true,
        rounds_won: Some(13),
        rounds_won_h1: None,
    }];
    let rows: Vec<MatchPlayer> = players
        .iter()
        .map(|p| MatchPlayer {
            puuid: (*p).into(),
            team_id: "Blue".into(),
            party_id: None,
            agent_id: Some("agent".into()),
            kills: 10,
            deaths: 5,
            assists: 2,
            score: 4_000,
            rounds: Some(20),
            hs: 1,
            bs: 1,
            ls: 1,
            game_name: None,
            tag_line: None,
            competitive_tier: None,
            damage: None,
        })
        .collect();
    h.st.db(move |repo| repo.upsert_match(&m, &teams, &rows))
        .await
        .unwrap();
}

#[tokio::test]
async fn profile_extras_gives_the_act_maps_and_who_on_screen_was_in_each_recent_match() {
    let h = harness().await;
    let now = now_ms();
    store_comp_match(&h, "pm-1", SPLIT_PATH, now - 3_000, &[OTHER, ME, MATE]).await;
    store_comp_match(&h, "pm-2", SPLIT_PATH, now - 2_000, &[OTHER, STRANGER]).await;
    store_comp_match(
        &h,
        "pm-3",
        "/Game/Maps/Ascent/Ascent",
        now - 1_000,
        &[OTHER, MATE],
    )
    .await;
    {
        let mut live = lock(&h.st.live);
        live.stage = Stage::Ingame;
        live.slots = [ME, OTHER, MATE]
            .iter()
            .map(|p| Slot {
                roster: roster(p, false),
                db: PlayerDb::default(),
            })
            .collect();
    }

    let extras = crate::v2::profile::build(&h.st, OTHER.into())
        .await
        .unwrap();
    assert_eq!(extras.puuid, OTHER);
    // Maps: the act's per-map record (map ids are uuids when the asset table
    // knows them; it is not loaded here, so they are empty and left out, as
    // in My Stats).
    assert!(extras.maps.len() <= crate::v2::profile::PROFILE_MAPS);
    // Newest first; the profile's own puuid never listed; the user is.
    let mates: Vec<(&str, Vec<&str>)> = extras
        .live_mates
        .iter()
        .map(|m| {
            (
                m.match_id.as_str(),
                m.puuids.iter().map(String::as_str).collect(),
            )
        })
        .collect();
    let mut first = vec![ME, MATE];
    first.sort_unstable();
    assert_eq!(mates, [("pm-3", vec![MATE]), ("pm-1", first)]);

    // The user's own profile: the user is not their own mate.
    let mine = crate::v2::profile::build(&h.st, ME.into()).await.unwrap();
    assert_eq!(mine.live_mates.len(), 1);
    assert_eq!(mine.live_mates[0].match_id, "pm-1");
    let mut want = vec![OTHER, MATE];
    want.sort_unstable();
    assert_eq!(mine.live_mates[0].puuids, want);

    // Closed: no live match, no chips.
    lock(&h.st.live).stage = Stage::Closed;
    let closed = crate::v2::profile::build(&h.st, OTHER.into())
        .await
        .unwrap();
    assert!(closed.live_mates.is_empty());
}

#[tokio::test]
async fn profile_extras_ranks_the_maps_by_games_with_the_asset_tables() {
    let h = harness().await;
    mount_asset_tables(&h.assets_server).await;
    h.st.ensure_tables(false).await.unwrap();
    let now = now_ms();
    for i in 0..3 {
        store_comp_match(
            &h,
            &format!("sp-{i}"),
            SPLIT_PATH,
            now - 10_000 + i,
            &[OTHER],
        )
        .await;
    }
    let extras = crate::v2::profile::build(&h.st, OTHER.into())
        .await
        .unwrap();
    assert_eq!(extras.maps.len(), 1);
    let split = &extras.maps[0];
    assert_eq!(split.id, SPLIT_UUID);
    assert_eq!((split.games, split.wins, split.losses), (3, 3, 0));
    assert_eq!(split.kd, 2.0);
    assert_eq!(split.acs, Some(200.0));
}

#[tokio::test]
async fn profile_extras_refuses_what_is_not_a_puuid() {
    let h = harness().await;
    let err = crate::v2::profile::build(&h.st, "../x".into())
        .await
        .unwrap_err();
    assert_eq!(err, "invalid puuid");
}
