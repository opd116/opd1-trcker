//! R150's pipeline tests: the names fill against the mock Riot client.
//! Outside live stages the app asks Riot's name-service for the players of a
//! match it has no name for — at a match end (the main trigger), when a
//! Details page needs names, and for the matches the own-history backfill
//! stores. A child of `pipeline_tests`, so it shares that module's harness
//! (mock Riot, in-memory DB) and helpers through `super::*`.
//!
//! The mock's name-service answers a name for the puuids a test lists and a
//! blank for everyone else, as Riot does for anyone hiding their name.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use opd1_core::db::repo::NameAsk;
use opd1_riot::endpoints::MatchDetailsResponse;

use super::*;
use crate::state::{Job, NAME_FILL_QUEUE_MAX};
use crate::sync::{fill_names_step, store_details, NAME_BATCH_MAX};

/// A valid uuid for test player `i`.
fn pid(i: u32) -> String {
    format!("f{i:07x}-0000-0000-0000-000000000000")
}

/// The user, then `others` with blank names — what match-details returns
/// for everyone but the user on a real account.
fn unnamed_match(match_id: &str, queue: &str, start: i64, others: &[String]) -> Value {
    let mut players = vec![json!({
        "subject": ME, "gameName": "Me", "tagLine": "TAG", "teamId": "Blue",
        "partyId": "p", "characterId": "agent-me",
        "stats": {"score": 1, "roundsPlayed": 20, "kills": 20, "deaths": 10, "assists": 3}
    })];
    for (i, o) in others.iter().enumerate() {
        players.push(json!({
            "subject": o, "gameName": "", "tagLine": "",
            "teamId": if i % 2 == 0 { "Red" } else { "Blue" },
            "partyId": format!("q{i}"), "characterId": format!("agent-{i}"),
            "stats": {"score": 1, "roundsPlayed": 20, "kills": 5, "deaths": 15, "assists": 1}
        }));
    }
    json!({
        "matchInfo": {
            "matchId": match_id, "mapId": "/Game/Maps/Ascent/Ascent",
            "gameStartMillis": start, "queueId": queue,
            "isRanked": queue == "competitive", "completionState": "Completed"
        },
        "players": players,
        "teams": [
            {"teamId": "Blue", "won": true, "roundsWon": 13},
            {"teamId": "Red", "won": false, "roundsWon": 7}
        ],
        "roundResults": []
    })
}

async fn mount_match(server: &MockServer, match_id: &str, queue: &str, start: i64, o: &[String]) {
    get(
        server,
        &format!("/match-details/v1/matches/{match_id}"),
        unnamed_match(match_id, queue, start, o),
    )
    .await;
}

/// The names name-service knows: a test can add one between two requests.
type NameBook = Arc<std::sync::Mutex<HashMap<String, (String, String)>>>;

/// Name-service: a name for each puuid in `named`, a blank for the rest.
async fn mount_name_service(server: &MockServer, named: &[(String, &str, &str)]) {
    let names: HashMap<String, (String, String)> = named
        .iter()
        .map(|(p, n, t)| (p.clone(), (n.to_string(), t.to_string())))
        .collect();
    mount_name_book(server, Arc::new(std::sync::Mutex::new(names))).await;
}

/// [`mount_name_service`] over a book the test keeps a handle on.
async fn mount_name_book(server: &MockServer, book: NameBook) {
    Mock::given(method("PUT"))
        .and(path("/name-service/v2/players"))
        .respond_with(move |req: &Request| {
            let asked: Vec<String> = serde_json::from_slice(&req.body).unwrap();
            let names = book.lock().unwrap();
            let answer: Vec<Value> = asked
                .iter()
                .map(|p| {
                    let (n, t) = names.get(p).cloned().unwrap_or_default();
                    json!({"DisplayName": "", "Subject": p, "GameName": n, "TagLine": t})
                })
                .collect();
            ResponseTemplate::new(200).set_body_json(answer)
        })
        .with_priority(1)
        .mount(server)
        .await;
}

/// The repo mutex held by a plain thread: whatever asks the DB next (a fill
/// step's `names_to_ask`) waits until [`DbHold::release`]. Deterministic
/// stand-in for "the step is between taking its asks and the request".
struct DbHold {
    release: std::sync::mpsc::Sender<()>,
    thread: std::thread::JoinHandle<()>,
}

impl DbHold {
    fn take(st: &Arc<AppState>) -> Self {
        let (held_tx, held_rx) = std::sync::mpsc::channel::<()>();
        let (release, release_rx) = std::sync::mpsc::channel::<()>();
        let st = st.clone();
        let thread = std::thread::spawn(move || {
            st.db_blocking(|_| {
                held_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                Ok(())
            });
        });
        held_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("the DB is held");
        Self { release, thread }
    }

    fn release(self) {
        self.release.send(()).unwrap();
        self.thread.join().unwrap();
    }
}

/// Every name-service request so far: the puuids it carried, in order.
async fn puts(server: &MockServer) -> Vec<Vec<String>> {
    server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.method.as_str() == "PUT" && r.url.path() == "/name-service/v2/players")
        .map(|r| serde_json::from_slice(&r.body).unwrap())
        .collect()
}

/// The Lobby, between matches, with the user's own card synced: only the
/// names fill is left to do.
fn lobby(h: &Harness) {
    let mut live = lock(&h.st.live);
    live.stage = Stage::Menus;
    live.in_match = false;
    live.slots = vec![Slot {
        roster: roster(ME, false),
        db: PlayerDb::default(),
    }];
    live.synced.insert(ME.into());
}

/// The poll loop's match end (In Game -> Menus): arms the match-end fill.
fn match_ends_now(h: &Harness) {
    lock(&h.st.live).match_end_ms = Some(now_ms());
}

/// Claims the worker and lets it drain.
async fn run_fill(h: &Harness) {
    let id = lock(&h.st.live)
        .try_start_worker()
        .expect("a fill is pending work");
    run_worker(h.st.clone(), h.session.clone(), id).await;
    assert!(!lock(&h.st.live).worker_running(), "claim released on exit");
}

/// The details of `match_id` arrive and are stored, as the post-match sync
/// or the backfill does.
async fn store_match(h: &Harness, match_id: &str) {
    let details: MatchDetailsResponse = h.session.match_details(match_id).await.unwrap();
    assert!(store_details(&h.st, details).await.is_some());
}

async fn marker(st: &AppState, puuid: String) -> Option<i64> {
    st.db(move |repo| {
        repo.conn().query_row(
            "SELECT name_asked_ms FROM players WHERE puuid = ?1",
            [puuid],
            |r| r.get(0),
        )
    })
    .await
    .flatten()
}

async fn seen(st: &AppState, puuid: String) -> Option<(String, String)> {
    st.db(move |repo| repo.seen_name(&puuid)).await.flatten()
}

/// An ask raised by a match that started at `match_start_ms`, or on demand.
fn ask(puuid: &str, match_start_ms: Option<i64>) -> NameAsk {
    NameAsk {
        puuid: puuid.into(),
        match_start_ms,
    }
}

// ---------------------------------------------------------------------
// the match-end trigger
// ---------------------------------------------------------------------

#[tokio::test]
async fn a_match_end_asks_name_service_once_for_every_unnamed_player() {
    let h = harness().await;
    lobby(&h);
    let others: Vec<String> = (1..=9).map(pid).collect();
    let start = now_ms() - 30 * 60_000;
    mount_match(&h.server, "m-end", "competitive", start, &others).await;
    mount_name_service(&h.server, &[(pid(1), "One", "T1"), (pid(2), "Two", "T2")]).await;
    match_ends_now(&h);

    // The match's details are stored: its nine other players are queued.
    store_match(&h, "m-end").await;
    {
        let live = lock(&h.st.live);
        assert_eq!(live.name_fill.len(), 9);
        assert!(live
            .name_fill
            .iter()
            .all(|a| a.match_start_ms == Some(start) && a.puuid != ME));
    }
    assert!(
        puts(&h.server).await.is_empty(),
        "nothing is sent by a store"
    );

    run_fill(&h).await;

    // One batched request for the nine — never the user.
    let sent = puts(&h.server).await;
    assert_eq!(sent.len(), 1, "one PUT per match end");
    assert_eq!(sent[0], others);
    assert!(lock(&h.st.live).name_fill.is_empty());

    // Every non-empty answer is kept, keyed by puuid; the blank ones are
    // marked with the start of the match that asked.
    assert_eq!(
        seen(&h.st, pid(1)).await,
        Some(("One".to_string(), "T1".to_string()))
    );
    assert_eq!(
        seen(&h.st, pid(2)).await,
        Some(("Two".to_string(), "T2".to_string()))
    );
    for i in 3..=9 {
        assert_eq!(seen(&h.st, pid(i)).await, None);
        assert_eq!(marker(&h.st, pid(i)).await, Some(start));
    }
    assert_eq!(
        marker(&h.st, pid(1)).await,
        None,
        "a named puuid needs none"
    );
}

#[tokio::test]
async fn a_blank_answer_is_asked_again_only_at_a_later_match_end() {
    let h = harness().await;
    lobby(&h);
    let start = now_ms() - 30 * 60_000;
    let first: Vec<String> = (1..=4).map(pid).collect();
    mount_match(&h.server, "n1", "competitive", start, &first).await;
    mount_name_service(&h.server, &[(pid(1), "One", "T1")]).await;

    match_ends_now(&h);
    store_match(&h, "n1").await;
    run_fill(&h).await;
    assert_eq!(puts(&h.server).await, std::slice::from_ref(&first));

    // The same match stored again (the backfill reaching a match the match
    // end already covered): nobody is asked twice for it.
    store_match(&h, "n1").await;
    assert_eq!(lock(&h.st.live).name_fill.len(), 4, "queued again…");
    run_fill(&h).await;
    assert_eq!(puts(&h.server).await.len(), 1, "…but the markers hold");

    // The next match: pid(3) and pid(4) are in it again (they answered blank
    // for n1, not yet for n2), plus a newcomer; pid(1) has a name by now.
    let second = vec![pid(1), pid(3), pid(4), pid(10)];
    mount_match(&h.server, "n2", "unrated", start + 60_000, &second).await;
    match_ends_now(&h);
    store_match(&h, "n2").await;
    run_fill(&h).await;
    let sent = puts(&h.server).await;
    assert_eq!(sent.len(), 2);
    assert_eq!(sent[1], [pid(3), pid(4), pid(10)], "asked again, no timer");
    assert_eq!(marker(&h.st, pid(3)).await, Some(start + 60_000));

    // n1 stored once more, now that n2 is on record: an older match than the
    // last answer adds nothing — name-service answers with the status now.
    store_match(&h, "n1").await;
    run_fill(&h).await;
    assert_eq!(puts(&h.server).await.len(), 2, "no third request");
}

#[tokio::test]
async fn only_the_match_that_just_ended_and_that_the_user_played_queues_a_fill() {
    let h = harness().await;
    lobby(&h);
    let others: Vec<String> = (1..=3).map(pid).collect();
    let now = now_ms();

    // Not a match end: nothing armed.
    mount_match(&h.server, "m-unarmed", "unrated", now - 60_000, &others).await;
    store_match(&h, "m-unarmed").await;
    assert!(
        lock(&h.st.live).name_fill.is_empty(),
        "no match end, no fill"
    );

    lock(&h.st.live).match_end_ms = Some(now);
    // Started long before the match end: an old match, not the one that ended.
    mount_match(&h.server, "m-old", "unrated", now - 3 * 3_600_000, &others).await;
    store_match(&h, "m-old").await;
    // Started after the match end: the next match, not this one.
    mount_match(&h.server, "m-next", "unrated", now + 60_000, &others).await;
    store_match(&h, "m-next").await;
    assert!(lock(&h.st.live).name_fill.is_empty(), "outside the window");

    // A match the user isn't in (someone else's history): not theirs.
    let mut foreign = unnamed_match("m-foreign", "unrated", now - 60_000, &others);
    foreign["players"].as_array_mut().unwrap().remove(0);
    get(&h.server, "/match-details/v1/matches/m-foreign", foreign).await;
    store_match(&h, "m-foreign").await;
    assert!(
        lock(&h.st.live).name_fill.is_empty(),
        "not the user's match"
    );

    // The one that just ended, whatever its mode (a Deathmatch too).
    mount_match(&h.server, "m-dm", "deathmatch", now - 20 * 60_000, &others).await;
    store_match(&h, "m-dm").await;
    assert_eq!(lock(&h.st.live).name_fill.len(), 3);
}

// ---------------------------------------------------------------------
// R153: players hidden during the match are asked after it
// ---------------------------------------------------------------------

/// The roster of a running match: the user, `hidden` incognito, everyone in
/// `visible` not — and `note_hidden`, as every roster install does.
async fn match_roster(h: &Harness, stage: Stage, hidden: &[&str], visible: &[&str]) {
    let mut slots = vec![slot_for(&h.st, ME, false).await];
    for p in hidden {
        slots.push(slot_for(&h.st, p, true).await);
    }
    for p in visible {
        slots.push(slot_for(&h.st, p, false).await);
    }
    let mut live = lock(&h.st.live);
    live.stage = stage;
    live.in_match = true;
    // Every card synced and named: the names fill is the only work there
    // could be.
    live.synced
        .extend(slots.iter().map(|s| s.roster.puuid.clone()));
    live.names_attempted
        .extend(slots.iter().map(|s| s.roster.puuid.clone()));
    live.slots = slots;
    live.note_hidden();
}

/// The match is over: the Lobby, with `party_hidden` (a puuid of the match)
/// still queueing with the user, hidden — or nobody. Arms the match-end fill
/// as the poll loop does.
async fn back_to_the_lobby(h: &Harness, party_hidden: Option<&str>) {
    let mut slots = vec![slot_for(&h.st, ME, false).await];
    if let Some(p) = party_hidden {
        slots.push(slot_for(&h.st, p, true).await);
    }
    let mut live = lock(&h.st.live);
    live.stage = Stage::Menus;
    live.in_match = false;
    live.slots = slots;
    live.synced.insert(ME.into());
    if let Some(p) = party_hidden {
        live.synced.insert(p.into());
    }
    live.note_hidden();
    live.match_end_ms = Some(now_ms());
}

#[tokio::test]
async fn a_player_hidden_in_the_match_that_just_ended_is_asked_and_remembered_next_time() {
    // The user's requirement (R153): after a match the game's Match History
    // shows the real names of the players who hid theirs, and the fill exists
    // to capture exactly those names, by puuid.
    //
    // Twice: the hidden player left after the match, and the hidden player
    // is still in the user's Lobby party, shown hidden there.
    for party_hidden in [false, true] {
        let h = harness().await;
        let (start, others) = (
            now_ms() - 30 * 60_000,
            vec![HIDDEN.to_string(), STRANGER.to_string(), pid(1)],
        );
        mount_match(&h.server, "m-hid", "competitive", start, &others).await;
        mount_name_service(
            &h.server,
            &[
                (HIDDEN.to_string(), "Real", "HID"),
                (STRANGER.to_string(), "Other", "STR"),
                (pid(1), "Plain", "VIS"),
            ],
        )
        .await;

        // In Game: HIDDEN and STRANGER are shown hidden, pid(1) visible.
        match_roster(&h, Stage::Ingame, &[HIDDEN, STRANGER], &[&pid(1)]).await;
        assert!(lock(&h.st.live).seen_hidden.contains_key(HIDDEN));
        assert!(lock(&h.st.live).seen_hidden.contains_key(STRANGER));
        assert!(puts(&h.server).await.is_empty(), "nothing during the match");

        // The match ends; the details of the match that just ended arrive.
        back_to_the_lobby(&h, party_hidden.then_some(HIDDEN)).await;
        {
            let live = lock(&h.st.live);
            assert!(live.seen_hidden.contains_key(HIDDEN), "still on record");
            assert_eq!(live.hidden_players().contains_key(HIDDEN), party_hidden);
        }
        store_match(&h, "m-hid").await;
        assert_eq!(lock(&h.st.live).name_fill.len(), 3, "all three are queued");

        run_fill(&h).await;

        // The hidden players are in the match-end PUT (never the user).
        let sent = puts(&h.server).await;
        assert_eq!(sent.len(), 1, "party_hidden={party_hidden}");
        assert_eq!(sent[0], others);
        assert!(sent[0].iter().any(|p| p == HIDDEN));
        assert!(sent[0].iter().any(|p| p == STRANGER));
        assert!(!sent[0].iter().any(|p| p == ME));

        // The names answered are stored, keyed by puuid.
        assert_eq!(
            seen(&h.st, HIDDEN.to_string()).await,
            Some(("Real".to_string(), "HID".to_string()))
        );
        assert_eq!(
            seen(&h.st, STRANGER.to_string()).await,
            Some(("Other".to_string(), "STR".to_string()))
        );

        // The next match: both are hidden again. Their cards show the stored
        // name in the remembered style, source `seen`; nothing is sent.
        match_roster(&h, Stage::Ingame, &[HIDDEN, STRANGER], &[&pid(1)]).await;
        h.st.publish();
        for (p, name, tag) in [(HIDDEN, "Real", "HID"), (STRANGER, "Other", "STR")] {
            let shown = card_json(&h.st, p)["name"].clone();
            assert_eq!(shown["kind"], "remembered", "{p}");
            assert_eq!(shown["name"], name);
            assert_eq!(shown["tag"], tag);
            assert_eq!(shown["source"], "seen");
            assert_eq!(shown["fromMapId"], Value::Null);
        }
        assert_eq!(puts(&h.server).await.len(), 1, "no request mid-match");
    }
}

#[tokio::test]
async fn the_user_and_an_empty_id_are_never_asked() {
    let h = harness().await;
    lobby(&h);
    mount_name_service(&h.server, &[]).await;
    lock(&h.st.live).queue_name_fill([ask(ME, Some(1)), ask("", Some(1)), ask(OTHER, Some(1))]);

    run_fill(&h).await;

    let sent = puts(&h.server).await;
    assert_eq!(sent, [vec![OTHER.to_string()]]);
    assert_eq!(marker(&h.st, ME.to_string()).await, None);
}

// ---------------------------------------------------------------------
// never during a match
// ---------------------------------------------------------------------

#[tokio::test]
async fn the_fill_never_runs_in_agent_select_in_game_or_a_free_for_all_match() {
    let h = harness().await;
    mount_name_service(&h.server, &[(pid(1), "One", "T1")]).await;
    lock(&h.st.live).queue_name_fill([ask(&pid(1), Some(1))]);

    for (stage, in_match) in [
        (Stage::Pregame, true),
        (Stage::Ingame, true),
        (Stage::Menus, true), // a Deathmatch keeps the Lobby view
    ] {
        {
            let mut live = lock(&h.st.live);
            live.stage = stage;
            live.in_match = in_match;
            live.synced.insert(ME.into());
        }
        assert!(
            lock(&h.st.live).try_start_worker().is_none(),
            "{stage:?}: no worker for a fill"
        );
        // Even a step that was already running sends nothing…
        fill_names_step(&h.st, &h.session).await.unwrap();
        assert!(puts(&h.server).await.is_empty(), "{stage:?}: no request");
        // …and the ask waits for the match to end.
        assert_eq!(lock(&h.st.live).name_fill.len(), 1, "{stage:?}: kept");
    }

    // The match ended: the same ask goes out.
    {
        let mut live = lock(&h.st.live);
        live.stage = Stage::Menus;
        live.in_match = false;
    }
    run_fill(&h).await;
    assert_eq!(puts(&h.server).await, [vec![pid(1)]]);
}

#[tokio::test]
async fn a_queued_fill_pauses_when_the_next_match_starts_and_sends_nothing_for_its_hidden_players()
{
    // R153: the guard is the stage. Asks queued after one match (players who
    // were hidden in it) wait while the next one runs — Agent Select and In
    // Game, with a puuid of that very roster shown hidden — and go out at its
    // end.
    let h = harness().await;
    mount_name_service(&h.server, &[(HIDDEN.to_string(), "Real", "HID")]).await;
    back_to_the_lobby(&h, None).await;
    lock(&h.st.live).seen_hidden.insert(STRANGER.into(), None);
    lock(&h.st.live).queue_name_fill([
        ask(HIDDEN, Some(1)),
        ask(STRANGER, Some(1)),
        ask(OTHER, Some(1)),
    ]);
    assert_eq!(lock(&h.st.live).name_fill.len(), 3);

    // The next match starts before the fill ran: HIDDEN is on its roster,
    // shown hidden.
    for stage in [Stage::Pregame, Stage::Ingame] {
        match_roster(&h, stage, &[HIDDEN], &[&pid(1)]).await;
        assert!(lock(&h.st.live).hidden_players().contains_key(HIDDEN));
        assert!(
            lock(&h.st.live).try_start_worker().is_none(),
            "{stage:?}: a fill alone starts no worker"
        );
        // Even a fill step already on its way sends nothing…
        fill_names_step(&h.st, &h.session).await.unwrap();
        assert!(puts(&h.server).await.is_empty(), "{stage:?}: no request");
        // …and it takes nothing off the queue.
        assert_eq!(lock(&h.st.live).name_fill.len(), 3, "{stage:?}: kept");
    }

    // A worker that had claimed the fill before the match began: its next job
    // is never the fill either.
    back_to_the_lobby(&h, None).await;
    let id = lock(&h.st.live)
        .try_start_worker()
        .expect("between matches");
    match_roster(&h, Stage::Pregame, &[HIDDEN], &[]).await;
    assert_eq!(lock(&h.st.live).next_job(id), None::<Job>, "never a fill");
    assert!(puts(&h.server).await.is_empty());

    // The match is over: everything queued goes out, in one request.
    back_to_the_lobby(&h, None).await;
    run_fill(&h).await;
    assert_eq!(
        puts(&h.server).await,
        [vec![
            HIDDEN.to_string(),
            STRANGER.to_string(),
            OTHER.to_string()
        ]]
    );
    assert_eq!(
        seen(&h.st, HIDDEN.to_string()).await,
        Some(("Real".to_string(), "HID".to_string()))
    );
}

#[tokio::test]
async fn a_match_that_starts_between_taking_the_asks_and_the_request_sends_nothing() {
    // R153: the stage is checked where asks leave the queue (`take_fill`) and
    // once more under the `live` lock right before the request is built. The
    // window between the two is the DB read (`names_to_ask`): a match that
    // starts in it must send nothing and keep the asks.
    let h = harness().await;
    mount_name_service(&h.server, &[(HIDDEN.to_string(), "Real", "HID")]).await;
    back_to_the_lobby(&h, None).await;
    lock(&h.st.live).queue_name_fill([ask(HIDDEN, Some(1)), ask(OTHER, Some(1))]);
    // The next match's roster, read from the DB now: the DB is held below.
    let roster_of_the_next_match = vec![
        slot_for(&h.st, ME, false).await,
        slot_for(&h.st, HIDDEN, true).await,
        slot_for(&h.st, &pid(1), false).await,
    ];

    // Hold the DB: the step's `names_to_ask` waits behind this, i.e. after
    // `take_fill` (the queue is empty by then) and before the request.
    let hold = DbHold::take(&h.st);
    let step = tokio::spawn({
        let (st, session) = (h.st.clone(), h.session.clone());
        async move { fill_names_step(&st, &session).await }
    });
    for _ in 0..1000 {
        if lock(&h.st.live).name_fill.is_empty() {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert!(
        lock(&h.st.live).name_fill.is_empty(),
        "the step took the asks and is waiting for the DB"
    );

    // The match starts now, with HIDDEN on its roster shown hidden.
    {
        let mut live = lock(&h.st.live);
        live.stage = Stage::Pregame;
        live.in_match = true;
        live.slots = roster_of_the_next_match;
        live.note_hidden();
    }
    hold.release();
    step.await.unwrap().unwrap();

    assert!(puts(&h.server).await.is_empty(), "nothing is sent");
    let mut want = vec![(HIDDEN.to_string(), Some(1)), (OTHER.to_string(), Some(1))];
    want.sort();
    assert_eq!(queue_of(&h), want, "the asks are back in the queue");
    assert_eq!(seen(&h.st, HIDDEN.to_string()).await, None);

    // The match is over: the same asks go out.
    back_to_the_lobby(&h, None).await;
    run_fill(&h).await;
    let sent = puts(&h.server).await;
    assert_eq!(sent.len(), 1);
    let mut got = sent[0].clone();
    got.sort();
    let mut want_ids = vec![HIDDEN.to_string(), OTHER.to_string()];
    want_ids.sort();
    assert_eq!(got, want_ids);
    assert_eq!(
        seen(&h.st, HIDDEN.to_string()).await,
        Some(("Real".to_string(), "HID".to_string()))
    );
}

#[tokio::test]
async fn a_step_that_is_dropped_before_the_send_keeps_its_asks() {
    // The poll loop aborts the worker on a stage change, which drops the step
    // wherever it waits. The asks it had taken are the only route to a hidden
    // player's name (a stored match queues only once): they go back.
    let h = harness().await;
    mount_name_service(&h.server, &[(HIDDEN.to_string(), "Real", "HID")]).await;
    back_to_the_lobby(&h, None).await;
    let mut want = vec![(HIDDEN.to_string(), Some(1)), (OTHER.to_string(), Some(1))];
    want.sort();
    lock(&h.st.live).queue_name_fill([ask(HIDDEN, Some(1)), ask(OTHER, Some(1))]);

    // Dropped while it waits for the DB (after taking the asks).
    let hold = DbHold::take(&h.st);
    let step = tokio::time::timeout(
        Duration::from_millis(100),
        fill_names_step(&h.st, &h.session),
    )
    .await;
    assert!(step.is_err(), "the step was still waiting for the DB");
    hold.release();
    assert_eq!(queue_of(&h), want, "kept: dropped on the DB");
    assert!(puts(&h.server).await.is_empty());

    // Dropped while it waits for a throttle token (the DB check is done).
    let slow = RiotSession::connect_with_overrides(
        h._dir.path(),
        ConnectOverrides {
            pd_base: Some(h.server.uri()),
            glz_base: Some(h.server.uri()),
            valorant_api_base: Some(h.server.uri()),
            ..Default::default()
        },
    )
    .await
    .unwrap()
    .with_global_rate(2.0);
    for _ in 0..2 {
        slow.match_history(ME, Some(0), Some(1), None)
            .await
            .unwrap();
    }
    let step = tokio::time::timeout(Duration::from_millis(60), fill_names_step(&h.st, &slow)).await;
    assert!(step.is_err(), "the step was still waiting for a token");
    assert_eq!(queue_of(&h), want, "kept: dropped in the throttle");
    assert!(puts(&h.server).await.is_empty(), "nothing was sent");

    // Nothing is lost for good: the next step asks for both.
    run_fill(&h).await;
    let sent = puts(&h.server).await;
    assert_eq!(sent.len(), 1);
    let mut got = sent[0].clone();
    got.sort();
    assert_eq!(got, [HIDDEN.to_string(), OTHER.to_string()]);
    assert_eq!(
        seen(&h.st, HIDDEN.to_string()).await,
        Some(("Real".to_string(), "HID".to_string()))
    );
}

#[tokio::test]
async fn a_completed_step_does_not_requeue_and_a_lost_client_keeps_the_asks() {
    let h = harness().await;
    lobby(&h);
    mount_name_service(&h.server, &[(HIDDEN.to_string(), "Real", "HID")]).await;
    // A puuid whose name is already known is consumed without a request.
    h.st.db(|repo| {
        save_player(
            repo.conn(),
            &PlayerUpdate {
                puuid: pid(3),
                name: Some(("Known".into(), "KNW".into())),
                account_level: 5,
                card_id: "c".into(),
            },
            1,
        )
    })
    .await
    .unwrap();
    lock(&h.st.live).queue_name_fill([
        ask(HIDDEN, Some(1)),
        ask(OTHER, Some(1)),
        ask(&pid(3), None),
    ]);

    // Answered (one name, one blank) and one not worth asking: nothing comes
    // back to the queue.
    fill_names_step(&h.st, &h.session).await.unwrap();
    assert_eq!(
        puts(&h.server).await,
        [vec![HIDDEN.to_string(), OTHER.to_string()]]
    );
    assert!(queue_of(&h).is_empty(), "a completed step keeps nothing");
    assert_eq!(marker(&h.st, OTHER.to_string()).await, Some(1));

    // The Riot client goes away before the answer: nothing was answered, so
    // the asks stay for the next connection.
    std::fs::remove_file(
        h._dir
            .path()
            .join("Riot Games")
            .join("Riot Client")
            .join("Config")
            .join("lockfile"),
    )
    .unwrap();
    lock(&h.st.live).queue_name_fill([ask(&pid(1), Some(2)), ask(&pid(2), Some(2))]);
    assert!(fill_names_step(&h.st, &h.session).await.is_err());
    assert_eq!(
        queue_of(&h),
        [(pid(1), Some(2)), (pid(2), Some(2))],
        "kept: the client was gone"
    );
    assert_eq!(puts(&h.server).await.len(), 1, "no second request");
}

// ---------------------------------------------------------------------
// batching, failures, the throttle
// ---------------------------------------------------------------------

#[tokio::test]
async fn asks_go_out_in_batches_of_at_most_a_hundred() {
    let h = harness().await;
    lobby(&h);
    mount_name_service(&h.server, &[]).await;
    let total = NAME_BATCH_MAX * 2 + 30;
    assert!(total <= NAME_FILL_QUEUE_MAX);
    lock(&h.st.live).queue_name_fill((0..total as u32).map(|i| ask(&pid(i + 1), None)));

    run_fill(&h).await;

    let sent = puts(&h.server).await;
    assert_eq!(
        sent.iter().map(Vec::len).collect::<Vec<_>>(),
        [100, 100, 30],
        "Riot's limit is 100 per request"
    );
    let all: std::collections::HashSet<&String> = sent.iter().flatten().collect();
    assert_eq!(all.len(), total, "every puuid once");
}

#[tokio::test]
async fn known_names_are_not_asked_and_batches_fill_up_behind_them() {
    let h = harness().await;
    lobby(&h);
    mount_name_service(&h.server, &[]).await;
    // 150 already have a name: they are dropped before the request, and the
    // batch is filled from the rest, not sent short.
    h.st.db(|repo| {
        for i in 1..=150u32 {
            repo.record_name_answers(&[(pid(i), format!("N{i}"), "T".into())], &[], 1)?;
        }
        Ok(())
    })
    .await
    .unwrap();
    lock(&h.st.live).queue_name_fill((1..=250u32).map(|i| ask(&pid(i), None)));

    run_fill(&h).await;

    let sent = puts(&h.server).await;
    assert_eq!(sent.len(), 1, "the 100 unnamed ones fit one request");
    assert_eq!(sent[0].len(), 100);
    assert!(sent[0].iter().all(|p| *p > pid(150)));
}

#[tokio::test]
async fn a_failed_request_stores_no_marker_and_is_not_retried_in_a_loop() {
    let h = harness().await;
    lobby(&h);
    Mock::given(method("PUT"))
        .and(path("/name-service/v2/players"))
        .respond_with(ResponseTemplate::new(500))
        .with_priority(1)
        .mount(&h.server)
        .await;
    lock(&h.st.live).queue_name_fill([ask(&pid(1), Some(1))]);

    run_fill(&h).await;

    assert_eq!(puts(&h.server).await.len(), 1, "one attempt");
    assert!(lock(&h.st.live).name_fill.is_empty(), "not kept for a loop");
    assert_eq!(
        marker(&h.st, pid(1)).await,
        None,
        "an error is not an answer: the next trigger may ask"
    );
}

#[tokio::test]
async fn the_fill_takes_its_turn_in_the_sessions_throttle() {
    let h = harness().await;
    lobby(&h);
    mount_name_service(&h.server, &[]).await;
    // A slow bucket: 2 requests a second, burst 2.
    let slow = RiotSession::connect_with_overrides(
        h._dir.path(),
        ConnectOverrides {
            pd_base: Some(h.server.uri()),
            glz_base: Some(h.server.uri()),
            valorant_api_base: Some(h.server.uri()),
            ..Default::default()
        },
    )
    .await
    .unwrap()
    .with_global_rate(2.0);
    for _ in 0..2 {
        slow.match_history(ME, Some(0), Some(1), None)
            .await
            .unwrap();
    }
    lock(&h.st.live).queue_name_fill([ask(&pid(1), None)]);

    let t = Instant::now();
    fill_names_step(&h.st, &slow).await.unwrap();

    assert_eq!(puts(&h.server).await.len(), 1);
    assert!(
        t.elapsed() >= Duration::from_millis(300),
        "the PUT waited for a token like every other pd call: {:?}",
        t.elapsed()
    );
}

// ---------------------------------------------------------------------
// names learned this way in live stages (R145)
// ---------------------------------------------------------------------

#[tokio::test]
async fn a_learned_name_shows_as_seen_when_that_player_is_hidden_live() {
    let h = harness().await;
    lobby(&h);
    mount_name_service(&h.server, &[(pid(1), "Learned", "NOW")]).await;
    lock(&h.st.live).queue_name_fill([ask(&pid(1), Some(1))]);
    run_fill(&h).await;

    // Next match: the same player is incognito. Their card shows the name in
    // the remembered style, from the `players` table (source: seen).
    {
        let slots = vec![
            slot_for(&h.st, ME, false).await,
            slot_for(&h.st, &pid(1), true).await,
        ];
        let mut live = lock(&h.st.live);
        live.stage = Stage::Ingame;
        live.in_match = true;
        live.slots = slots;
    }
    h.st.publish();
    let name = card_json(&h.st, &pid(1))["name"].clone();
    assert_eq!(name["kind"], "remembered");
    assert_eq!(name["name"], "Learned");
    assert_eq!(name["tag"], "NOW");
    assert_eq!(name["source"], "seen");
    assert_eq!(name["fromMapId"], Value::Null);
    // And nothing is asked mid-match: the fill has nothing to run in Ingame.
    assert_eq!(puts(&h.server).await.len(), 1);
}

// ---------------------------------------------------------------------
// Player Details and Match Details ask for the names they lack
// ---------------------------------------------------------------------

/// A competitive match of the current act: `team` on Blue with blank names,
/// `foes` on Red with blank names.
async fn store_unnamed_match(st: &AppState, match_id: &str, team: &[&str], foes: &[&str]) {
    let m = Match {
        match_id: match_id.into(),
        map_id: "/Game/Maps/Ascent/Ascent".into(),
        queue_id: "competitive".into(),
        start_ms: now_ms() - 60_000,
        winning_team: None,
        is_ranked: true,
        rounds_played: None,
        game_length_ms: None,
    };
    let row = |p: &&str, t: &str| MatchPlayer {
        game_name: None,
        tag_line: None,
        ..player_row(p, t, "", "")
    };
    let players: Vec<MatchPlayer> = team
        .iter()
        .map(|p| row(p, "Blue"))
        .chain(foes.iter().map(|p| row(p, "Red")))
        .collect();
    st.db(move |repo| repo.upsert_match(&m, &[], &players))
        .await
        .unwrap();
}

fn queue_of(h: &Harness) -> Vec<(String, Option<i64>)> {
    let mut q: Vec<_> = lock(&h.st.live)
        .name_fill
        .iter()
        .map(|a| (a.puuid.clone(), a.match_start_ms))
        .collect();
    q.sort();
    q
}

#[tokio::test]
async fn player_details_asks_for_the_names_it_lacks_and_refreshes_when_they_arrive() {
    let h = harness().await;
    lobby(&h);
    let (t1, t2) = (pid(1), pid(2));
    // OTHER (the hero, seen once with no name) played a competitive match of
    // the act with two teammates whose names match-details left blank.
    store_unnamed_match(&h.st, "n-team", &[OTHER, &t1, &t2], &[&pid(3)]).await;
    h.st.db(|repo| {
        save_player(
            repo.conn(),
            &PlayerUpdate {
                puuid: OTHER.into(),
                name: None,
                account_level: 30,
                card_id: "card".into(),
            },
            1,
        )
    })
    .await
    .unwrap();
    mount_name_service(
        &h.server,
        &[(t1.clone(), "Mate", "ONE"), (OTHER.into(), "Hero", "HRO")],
    )
    .await;

    let d = crate::commands::player_details_for(&h.st, OTHER)
        .await
        .unwrap();
    assert_eq!(d.card.name, Field::Missing, "the hero has no name yet");
    assert_eq!(d.top_teammates.len(), 2);
    assert!(d
        .top_teammates
        .iter()
        .all(|t| t.name == DisplayName::HiddenPlayer));
    // The hero and both teammates are queued on demand; the foe is not shown
    // on this page, so not asked.
    let mut want = vec![
        (OTHER.to_string(), None),
        (t1.clone(), None),
        (t2.clone(), None),
    ];
    want.sort();
    assert_eq!(queue_of(&h), want);
    assert!(
        h.st.take_worker_kick(),
        "the poll loop is told to start a worker"
    );
    assert!(!h.st.take_worker_kick(), "once");
    assert!(
        puts(&h.server).await.is_empty(),
        "the answer never waits for Riot"
    );

    run_fill(&h).await;

    assert_eq!(puts(&h.server).await.len(), 1);
    // The open Details was rebuilt and emitted with the names that arrived;
    // t2 answered blank and stays unnamed.
    let events = h.rec.details.lock().unwrap().clone();
    let last = events.last().expect("a details event after the fill");
    assert_eq!(
        last.card.name,
        Field::Value(DisplayName::Visible {
            name: "Hero".into(),
            tag: "HRO".into()
        })
    );
    let mate = |d: &PlayerDetails, p: &str| {
        d.top_teammates
            .iter()
            .find(|t| t.puuid == p)
            .unwrap()
            .name
            .clone()
    };
    assert_eq!(
        mate(last, &t1),
        DisplayName::Visible {
            name: "Mate".into(),
            tag: "ONE".into()
        }
    );
    assert_eq!(mate(last, &t2), DisplayName::HiddenPlayer);

    // Opened again: the blank answer is not asked again on demand.
    crate::commands::player_details_for(&h.st, OTHER)
        .await
        .unwrap();
    run_fill(&h).await;
    assert_eq!(puts(&h.server).await.len(), 1, "never-asked puuids only");
}

#[tokio::test]
async fn details_asks_for_a_teammate_hidden_now_too_and_shows_the_answer_hidden() {
    // R153: a teammate shown hidden in the Lobby is asked about between
    // matches like anyone else; what comes back is a remembered name, never a
    // plain one (R18).
    let h = harness().await;
    lobby(&h);
    store_unnamed_match(&h.st, "n-team", &[OTHER, HIDDEN, &pid(1)], &[]).await;
    h.st.db(|repo| {
        save_player(
            repo.conn(),
            &PlayerUpdate {
                puuid: OTHER.into(),
                name: Some(("Hero".into(), "HRO".into())),
                account_level: 30,
                card_id: "card".into(),
            },
            1,
        )
    })
    .await
    .unwrap();
    mount_name_service(&h.server, &[(HIDDEN.to_string(), "Mate", "HID")]).await;
    {
        let mut live = lock(&h.st.live);
        live.slots.push(Slot {
            roster: roster(HIDDEN, true),
            db: PlayerDb::default(),
        });
        live.synced.insert(HIDDEN.into());
    }

    let d = crate::commands::player_details_for(&h.st, OTHER)
        .await
        .unwrap();
    assert_eq!(d.top_teammates.len(), 2);
    let mut want = vec![(HIDDEN.to_string(), None), (pid(1), None)];
    want.sort();
    assert_eq!(queue_of(&h), want, "the hidden teammate is queued too");

    run_fill(&h).await;

    let mut sent = puts(&h.server).await;
    assert_eq!(sent.len(), 1);
    sent[0].sort();
    assert_eq!(sent[0], [HIDDEN.to_string(), pid(1)]);
    let events = h.rec.details.lock().unwrap().clone();
    let last = events.last().expect("a details event after the fill");
    let mate = last
        .top_teammates
        .iter()
        .find(|t| t.puuid == HIDDEN)
        .unwrap();
    assert!(
        matches!(&mate.name, DisplayName::Remembered { name, tag, .. } if name == "Mate" && tag == "HID"),
        "shown in the remembered style: {:?}",
        mate.name
    );
}

#[tokio::test]
async fn details_asks_for_a_hero_who_was_hidden_earlier_this_run() {
    // R153: a player shown hidden in a match earlier this run (`seen_hidden`)
    // has a Hidden Player card off screen. Their Details asks about them
    // between matches, and the page shows the answer as a remembered name.
    let h = harness().await;
    lobby(&h);
    h.st.db(|repo| {
        save_player(
            repo.conn(),
            &PlayerUpdate {
                puuid: STRANGER.into(),
                name: None,
                account_level: 42,
                card_id: "card".into(),
            },
            1,
        )
    })
    .await
    .unwrap();
    lock(&h.st.live).seen_hidden.insert(STRANGER.into(), None);
    mount_name_service(&h.server, &[(STRANGER.to_string(), "Real", "STR")]).await;

    let d = crate::commands::player_details_for(&h.st, STRANGER)
        .await
        .unwrap();
    assert_eq!(d.card.name, Field::Value(DisplayName::HiddenPlayer));
    assert_eq!(queue_of(&h), [(STRANGER.to_string(), None)]);

    run_fill(&h).await;

    assert_eq!(puts(&h.server).await, [vec![STRANGER.to_string()]]);
    let events = h.rec.details.lock().unwrap().clone();
    let last = events.last().expect("a details event after the fill");
    assert!(
        matches!(
            &last.card.name,
            Field::Value(DisplayName::Remembered { name, tag, .. }) if name == "Real" && tag == "STR"
        ),
        "{:?}",
        last.card.name
    );
}

#[tokio::test]
async fn a_top_3_teammate_who_was_hidden_earlier_is_asked_and_the_open_page_refreshes() {
    // R153: a Top 3 teammate shown as Hidden Player is queued when the page
    // opens, also when that player was hidden in a match earlier this run
    // (and is on no roster now); when the name arrives the open page is
    // rebuilt and emitted with it (off the live screen a name shows plainly,
    // R145).
    let h = harness().await;
    lobby(&h);
    store_unnamed_match(&h.st, "n-team", &[OTHER, STRANGER, &pid(1)], &[]).await;
    h.st.db(|repo| {
        save_player(
            repo.conn(),
            &PlayerUpdate {
                puuid: OTHER.into(),
                name: Some(("Hero".into(), "HRO".into())),
                account_level: 30,
                card_id: "card".into(),
            },
            1,
        )
    })
    .await
    .unwrap();
    lock(&h.st.live).seen_hidden.insert(STRANGER.into(), None);
    mount_name_service(&h.server, &[(STRANGER.to_string(), "Mate", "STR")]).await;

    let d = crate::commands::player_details_for(&h.st, OTHER)
        .await
        .unwrap();
    let mate = |d: &PlayerDetails| {
        d.top_teammates
            .iter()
            .find(|t| t.puuid == STRANGER)
            .expect("a Top 3 row")
            .name
            .clone()
    };
    assert_eq!(mate(&d), DisplayName::HiddenPlayer);
    assert!(
        queue_of(&h).contains(&(STRANGER.to_string(), None)),
        "queued on opening the page: {:?}",
        queue_of(&h)
    );

    run_fill(&h).await;

    assert!(puts(&h.server)
        .await
        .concat()
        .contains(&STRANGER.to_string()));
    let events = h.rec.details.lock().unwrap().clone();
    let last = events.last().expect("a details event after the fill");
    assert_eq!(
        mate(last),
        DisplayName::Visible {
            name: "Mate".into(),
            tag: "STR".into()
        }
    );
}

#[tokio::test]
async fn top_3_teammates_a_details_sync_brings_in_are_asked_too() {
    // The first build of the page had no teammates to ask about. The Riot
    // sync behind it stores new matches and rebuilds the page: the teammates
    // that now show as Hidden Player are queued then, not only at the next
    // open — once each, and the name refreshes the open page.
    let h = harness().await;
    lobby(&h);
    h.st.set_session(Some(h.session.clone()));
    h.st.db(|repo| {
        save_player(
            repo.conn(),
            &PlayerUpdate {
                puuid: OTHER.into(),
                name: Some(("Hero".into(), "HRO".into())),
                account_level: 30,
                card_id: "card".into(),
            },
            1,
        )
    })
    .await
    .unwrap();
    // OTHER's history: one competitive match of the act, with a teammate
    // (`unnamed_match` puts the odd slots on OTHER's team, Blue) whose name
    // match-details left blank.
    let start = now_ms() - 60_000;
    let others = vec![pid(9), OTHER.to_string(), pid(8), pid(1)];
    mount_match(&h.server, "n-sync", "competitive", start, &others).await;
    // Beats the harness's 500 for OTHER's routes (lower number = first).
    Mock::given(method("GET"))
        .and(path(format!("/mmr/v1/players/{OTHER}/competitiveupdates")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "Subject": OTHER,
            "Matches": [{
                "MatchID": "n-sync", "MapID": "/Game/Maps/Ascent/Ascent", "SeasonID": ACT,
                "MatchStartTime": start, "TierAfterUpdate": 15, "TierBeforeUpdate": 15,
                "RankedRatingAfterUpdate": 42, "RankedRatingBeforeUpdate": 30,
                "RankedRatingEarned": 12
            }]
        })))
        .with_priority(1)
        .mount(&h.server)
        .await;
    mount_name_service(&h.server, &[(pid(1), "Mate", "ONE")]).await;

    let d = crate::commands::player_details_for(&h.st, OTHER)
        .await
        .unwrap();
    assert!(d.top_teammates.is_empty(), "no matches stored yet");
    assert!(queue_of(&h).is_empty(), "nothing to ask at the first build");

    // The sync's rebuild shows the teammate as Hidden Player; queued then.
    let mut queued = false;
    for _ in 0..500 {
        if queue_of(&h).iter().any(|(p, _)| p == &pid(1)) {
            queued = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(queued, "the rebuilt page's unnamed teammate is queued");
    let queue = queue_of(&h);
    assert_eq!(
        queue.iter().filter(|(p, _)| p == &pid(1)).count(),
        1,
        "no duplicate ask: {queue:?}"
    );
    assert!(
        !queue.iter().any(|(p, _)| p == ME),
        "the named teammate (the user) is not asked: {queue:?}"
    );

    run_fill(&h).await;
    let events = h.rec.details.lock().unwrap().clone();
    let last = events.last().expect("a details event");
    let mate = last
        .top_teammates
        .iter()
        .find(|t| t.puuid == pid(1))
        .expect("a Top 3 row");
    assert_eq!(
        mate.name,
        DisplayName::Visible {
            name: "Mate".into(),
            tag: "ONE".into()
        }
    );
}

/// One scoreboard row's name.
fn board_name(view: &MatchDetailsView, puuid: &str) -> DisplayName {
    view.teams
        .iter()
        .flat_map(|t| t.players.iter())
        .find(|p| p.puuid == puuid)
        .unwrap_or_else(|| panic!("{puuid} not on the scoreboard"))
        .name
        .clone()
}

#[tokio::test]
async fn match_details_asks_once_for_nine_unknown_players_and_refreshes() {
    let h = harness().await;
    lobby(&h);
    // A typical Match Details: the user and nine players whose names
    // match-details left blank, stored earlier (no match end on record, so
    // only opening the scoreboard asks).
    let others: Vec<String> = (1..=9).map(pid).collect();
    mount_match(
        &h.server,
        "n-typ",
        "competitive",
        now_ms() - 30 * 60_000,
        &others,
    )
    .await;
    store_match(&h, "n-typ").await;
    assert!(queue_of(&h).is_empty());
    mount_name_service(&h.server, &[(pid(1), "One", "T1"), (pid(2), "Two", "T2")]).await;

    let view = crate::match_details::load(&h.st, "n-typ", &pid(1))
        .await
        .unwrap();
    assert!(matches!(
        board_name(&view, &pid(1)),
        DisplayName::HiddenAgent { .. }
    ));
    assert_eq!(queue_of(&h).len(), 9, "everyone but the user, on demand");
    assert!(h.st.take_worker_kick());

    run_fill(&h).await;

    // The request count of a typical Match Details: one PUT, nine puuids.
    let sent = puts(&h.server).await;
    assert_eq!(sent.len(), 1, "at most one PUT for nine unknown players");
    assert_eq!(sent[0], others);
    // The open scoreboard was rebuilt and emitted with the new names.
    let events = h.rec.matches.lock().unwrap().clone();
    let last = events.last().expect("a match event after the fill");
    assert_eq!(last.match_id, "n-typ");
    assert_eq!(
        board_name(last, &pid(1)),
        DisplayName::Visible {
            name: "One".into(),
            tag: "T1".into()
        }
    );
    assert!(matches!(
        board_name(last, &pid(3)),
        DisplayName::HiddenAgent { .. }
    ));

    // Opened again: the seven blank answers are not asked again.
    crate::match_details::load(&h.st, "n-typ", &pid(1))
        .await
        .unwrap();
    run_fill(&h).await;
    assert_eq!(puts(&h.server).await.len(), 1);
}

#[tokio::test]
async fn a_closed_match_details_gets_no_match_event_and_reopens_as_before() {
    // The UI calls `close_match` when the scoreboard is left: the open-match
    // slot is empty, so a names fill that stores a name for that match builds
    // and emits nothing. Opening it again works as before.
    let h = harness().await;
    lobby(&h);
    let others: Vec<String> = (1..=3).map(pid).collect();
    mount_match(
        &h.server,
        "n-close",
        "competitive",
        now_ms() - 30 * 60_000,
        &others,
    )
    .await;
    store_match(&h, "n-close").await;
    let book: NameBook = Arc::new(std::sync::Mutex::new(HashMap::from([(
        pid(1),
        ("One".to_string(), "T1".to_string()),
    )])));
    mount_name_book(&h.server, book.clone()).await;

    crate::match_details::load(&h.st, "n-close", &pid(1))
        .await
        .unwrap();
    assert!(h.st.open_match_args().is_some());
    crate::commands::close_match_for(&h.st);
    assert_eq!(h.st.open_match_args(), None, "nothing is open");
    crate::commands::close_match_for(&h.st); // idempotent

    run_fill(&h).await;
    assert_eq!(puts(&h.server).await, [others]);
    assert_eq!(
        seen(&h.st, pid(1)).await,
        Some(("One".to_string(), "T1".to_string())),
        "the name is stored"
    );
    assert!(
        h.rec.matches.lock().unwrap().is_empty(),
        "no match event for a page nobody sees"
    );

    // Opened again: the view has the stored name, and a name that arrives
    // while it is open is emitted as before.
    let view = crate::match_details::load(&h.st, "n-close", &pid(1))
        .await
        .unwrap();
    assert_eq!(
        board_name(&view, &pid(1)),
        DisplayName::Visible {
            name: "One".into(),
            tag: "T1".into()
        }
    );
    book.lock()
        .unwrap()
        .insert(pid(2), ("Two".to_string(), "T2".to_string()));
    lock(&h.st.live).queue_name_fill([ask(&pid(2), Some(now_ms()))]);
    run_fill(&h).await;
    let events = h.rec.matches.lock().unwrap().clone();
    assert_eq!(events.len(), 1, "one event, for the reopened page");
    assert_eq!(
        board_name(&events[0], &pid(2)),
        DisplayName::Visible {
            name: "Two".into(),
            tag: "T2".into()
        }
    );
}

#[tokio::test]
async fn match_details_asks_for_a_player_hidden_now_too() {
    // R153: a row for a player shown hidden in the Lobby is queued like the
    // others; the fill sends between matches, and the scoreboard shows the
    // answer in the hidden style (remembered).
    let h = harness().await;
    lobby(&h);
    let others = vec![HIDDEN.to_string(), pid(1)];
    mount_match(
        &h.server,
        "n-hid",
        "competitive",
        now_ms() - 30 * 60_000,
        &others,
    )
    .await;
    store_match(&h, "n-hid").await;
    mount_name_service(&h.server, &[(HIDDEN.to_string(), "Real", "HID")]).await;
    {
        let mut live = lock(&h.st.live);
        live.slots.push(Slot {
            roster: roster(HIDDEN, true),
            db: PlayerDb::default(),
        });
        live.synced.insert(HIDDEN.into());
    }

    crate::match_details::load(&h.st, "n-hid", ME)
        .await
        .unwrap();

    let mut want = vec![(HIDDEN.to_string(), None), (pid(1), None)];
    want.sort();
    assert_eq!(queue_of(&h), want);
    run_fill(&h).await;
    assert_eq!(puts(&h.server).await, [others]);
    let events = h.rec.matches.lock().unwrap().clone();
    let last = events.last().expect("a match event after the fill");
    assert!(
        matches!(board_name(last, HIDDEN), DisplayName::Remembered { .. }),
        "shown in the hidden style: {:?}",
        board_name(last, HIDDEN)
    );
}

#[tokio::test]
async fn going_back_to_the_player_details_ends_the_match_refreshes() {
    let h = harness().await;
    lobby(&h);
    mount_match(
        &h.server,
        "n-typ",
        "competitive",
        now_ms() - 30 * 60_000,
        &[pid(1)],
    )
    .await;
    store_match(&h, "n-typ").await;
    mount_name_service(&h.server, &[(pid(1), "One", "T1")]).await;
    h.st.publish(); // the user's own card is on screen: their Details opens
    crate::match_details::load(&h.st, "n-typ", &pid(1))
        .await
        .unwrap();

    // Back from the match page: the Player Details opens again, the match is
    // no longer open. The fill still stores the name, but no `match` event
    // is emitted for a page nobody has open.
    crate::commands::player_details_for(&h.st, ME)
        .await
        .unwrap();
    run_fill(&h).await;

    assert_eq!(puts(&h.server).await.len(), 1);
    assert!(h.rec.matches.lock().unwrap().is_empty());
    assert_eq!(
        seen(&h.st, pid(1)).await,
        Some(("One".to_string(), "T1".to_string()))
    );
}

// ---------------------------------------------------------------------
// the own-history backfill
// ---------------------------------------------------------------------

use super::backfill::{run_backfill, stranger, Feed};
use crate::sync::BACKFILL_FILL_MAX;

#[tokio::test]
async fn the_backfill_asks_for_the_players_of_the_matches_it_stored_bounded() {
    let h = harness().await;
    let mut feed = Feed::new(30);
    feed.strangers = 10; // 300 players nobody has a name for
    super::backfill::mount_feed(&h.server, feed).await;
    mount_name_service(&h.server, &[]).await;

    run_backfill(&h).await;

    let reqs = h.server.received_requests().await.unwrap();
    let last_details = reqs
        .iter()
        .rposition(|r| r.url.path().starts_with("/match-details/"))
        .expect("the run fetched matches");
    let first_put = reqs
        .iter()
        .position(|r| r.method.as_str() == "PUT")
        .expect("and then asked for names");
    assert!(first_put > last_details, "the fill follows the run");

    let sent = puts(&h.server).await;
    assert_eq!(
        sent.iter().map(Vec::len).collect::<Vec<_>>(),
        [100, 100],
        "bounded: {BACKFILL_FILL_MAX} puuids in batches of 100, not 300"
    );
    // The newest matches' players come first (the history is newest first).
    let asked: std::collections::HashSet<&String> = sent.iter().flatten().collect();
    assert_eq!(asked.len(), BACKFILL_FILL_MAX);
    assert!(asked.contains(&stranger(0, 0)), "the newest match is in");
    assert!(!asked.contains(&stranger(29, 0)), "the oldest is cut");
    // Each is marked with its newest match's start.
    let base = super::backfill::base_ms();
    assert_eq!(
        marker(&h.st, stranger(0, 0)).await,
        Some(super::backfill::entry_start(base, 0))
    );
}

#[tokio::test]
async fn the_match_end_fill_runs_before_the_backfill_goes_on() {
    let h = harness().await;
    let mut feed = Feed::new(4);
    feed.strangers = 3;
    super::backfill::mount_feed(&h.server, feed).await;
    mount_name_service(&h.server, &[]).await;
    match_ends_now(&h);

    run_backfill(&h).await;

    let reqs = h.server.received_requests().await.unwrap();
    let first_put = reqs
        .iter()
        .position(|r| r.method.as_str() == "PUT")
        .expect("the ended match's players were asked");
    let last_details = reqs
        .iter()
        .rposition(|r| r.url.path().starts_with("/match-details/"))
        .unwrap();
    assert!(
        first_put < last_details,
        "the fill is ahead of the backfill's next match: PUT at {first_put}, last details at {last_details}"
    );
    // One PUT per stored match (each match's three strangers), nothing twice.
    let sent = puts(&h.server).await;
    assert_eq!(sent.len(), 4);
    assert!(sent.iter().all(|b| b.len() == 3));
}

#[tokio::test]
async fn a_match_published_after_the_next_one_started_still_queues_the_match_end_fill() {
    // R165: the poll loop clears the match-end marker the moment the next
    // match starts (a quick queue: the end, then Agent Select within a minute
    // or two). Riot publishes the match later, at +40 s or more: its players
    // are still asked about, from the copy the model kept at the end.
    let h = harness().await;
    lobby(&h);
    let others: Vec<String> = (1..=3).map(pid).collect();
    let now = now_ms();
    mount_match(
        &h.server,
        "m-late",
        "competitive",
        now - 20 * 60_000,
        &others,
    )
    .await;

    match_ends_now(&h);
    // Any publish or worker job between the end and the next match sees it.
    h.st.publish();
    // The next match starts: `match_end_ms` is cleared.
    lock(&h.st.live).match_end_ms = None;

    store_match(&h, "m-late").await;
    let queued: Vec<String> = queue_of(&h).into_iter().map(|(p, _)| p).collect();
    assert_eq!(queued, others, "the match that ended is still asked about");
}

#[tokio::test]
async fn with_no_match_end_ever_seen_no_fill_is_queued() {
    let h = harness().await;
    lobby(&h);
    let others: Vec<String> = (1..=2).map(pid).collect();
    mount_match(&h.server, "m-x", "competitive", now_ms() - 60_000, &others).await;
    h.st.publish();
    store_match(&h, "m-x").await;
    assert!(lock(&h.st.live).name_fill.is_empty());
}

#[tokio::test]
async fn a_match_open_that_lost_queues_no_names() {
    // R163: the page was left while the open was being built (the real
    // `match_details::load` path); the answer is still returned, but its
    // unnamed rows are not queued for a page nobody sees. The first open is
    // the control: the same match, opened normally, does queue them.
    let h = harness().await;
    lobby(&h);
    let others: Vec<String> = (1..=3).map(pid).collect();
    mount_match(
        &h.server,
        "n-lost",
        "competitive",
        now_ms() - 30 * 60_000,
        &others,
    )
    .await;
    store_match(&h, "n-lost").await;
    assert!(!h.st.take_worker_kick());

    let control = crate::match_details::load(&h.st, "n-lost", &pid(1))
        .await
        .unwrap();
    assert!(h.st.take_worker_kick(), "control: unnamed rows are queued");
    crate::commands::close_match_for(&h.st);

    // The DB is held, so the open takes its ticket and cannot finish; the
    // close then voids it; only then does the read run.
    let focus = pid(1);
    let hold = super::ui_open::hold_db(&h.st);
    let open = super::ui_open::parked(crate::match_details::load(&h.st, "n-lost", &focus)).await;
    crate::commands::close_match_key_for(&h.st, Some("n-lost"));
    drop(hold);
    let view = open.await;
    assert_eq!(view.unwrap(), control, "still answered");
    assert_eq!(h.st.open_match_args(), None, "not open");
    assert!(!h.st.take_worker_kick(), "no name ask for a closed page");
}
