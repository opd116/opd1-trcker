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
use crate::state::NAME_FILL_QUEUE_MAX;
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

/// Name-service: a name for each puuid in `named`, a blank for the rest.
async fn mount_name_service(server: &MockServer, named: &[(String, &str, &str)]) {
    let names: HashMap<String, (String, String)> = named
        .iter()
        .map(|(p, n, t)| (p.clone(), (n.to_string(), t.to_string())))
        .collect();
    Mock::given(method("PUT"))
        .and(path("/name-service/v2/players"))
        .respond_with(move |req: &Request| {
            let asked: Vec<String> = serde_json::from_slice(&req.body).unwrap();
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
// privacy: R16 stays
// ---------------------------------------------------------------------

#[tokio::test]
async fn a_player_hidden_on_the_live_screen_is_never_asked() {
    let h = harness().await;
    lobby(&h);
    mount_name_service(&h.server, &[]).await;
    {
        let mut live = lock(&h.st.live);
        // HIDDEN is in the Lobby party, incognito; STRANGER was hidden in
        // the match that just ended and is on no roster now.
        live.slots.push(Slot {
            roster: roster(HIDDEN, true),
            db: PlayerDb::default(),
        });
        live.synced.insert(HIDDEN.into());
        live.seen_hidden.insert(STRANGER.into(), None);
    }
    lock(&h.st.live).queue_name_fill([
        ask(HIDDEN, Some(1)),
        ask(STRANGER, Some(1)),
        ask(OTHER, Some(1)),
        ask(&pid(1), None),
    ]);

    run_fill(&h).await;

    let sent = puts(&h.server).await;
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0], [OTHER.to_string(), pid(1)]);
    let body = sent[0].join(",");
    assert!(
        !body.contains(HIDDEN) && !body.contains(STRANGER) && !body.contains(ME),
        "never a hidden player, never the user: {body}"
    );
    // Nothing is stored about the ones that weren't asked.
    assert_eq!(marker(&h.st, HIDDEN.to_string()).await, None);
    assert_eq!(marker(&h.st, STRANGER.to_string()).await, None);

    // Only hidden players asked for: not even queued, so no request at all.
    lock(&h.st.live).queue_name_fill([ask(HIDDEN, None), ask(STRANGER, None)]);
    assert!(lock(&h.st.live).name_fill.is_empty());
    assert!(lock(&h.st.live).try_start_worker().is_none());
    assert_eq!(puts(&h.server).await.len(), 1, "no second request");
}

#[tokio::test]
async fn a_player_who_turns_hidden_before_the_request_is_not_asked() {
    // The queue is checked again at the last moment before the request.
    let h = harness().await;
    lobby(&h);
    mount_name_service(&h.server, &[]).await;
    lock(&h.st.live).queue_name_fill([ask(OTHER, Some(1))]);
    // OTHER joins the party hidden after the ask was queued.
    lock(&h.st.live).slots.push(Slot {
        roster: roster(OTHER, true),
        db: PlayerDb::default(),
    });
    lock(&h.st.live).synced.insert(OTHER.into());

    fill_names_step(&h.st, &h.session).await.unwrap();

    assert!(puts(&h.server).await.is_empty());
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
async fn details_never_asks_for_a_teammate_hidden_on_the_live_screen() {
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
    mount_name_service(&h.server, &[]).await;
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
    assert!(
        !queue_of(&h).iter().any(|(p, _)| p == HIDDEN),
        "a teammate shown hidden is not even queued"
    );
    run_fill(&h).await;
    let sent = puts(&h.server).await;
    assert_eq!(sent, [vec![pid(1)]], "only the visible teammate is asked");
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
async fn match_details_never_asks_for_a_player_hidden_on_the_live_screen() {
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
    mount_name_service(&h.server, &[]).await;
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

    assert_eq!(queue_of(&h), [(pid(1), None)], "HIDDEN is not queued");
    run_fill(&h).await;
    assert_eq!(puts(&h.server).await, [vec![pid(1)]]);
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
