//! R165's pipeline tests: the rank phase of the whole roster before any
//! match-details download, a failed live names lookup asked again (bounded),
//! and a partial sync (history or match-details failed) retried by the R155
//! schedule — against the mock Riot client. A child of `pipeline_tests`.
//!
//! Waits are never slept: the live model's test clock moves forward.

use std::time::Duration;

use super::rank::{advance, drain, lobby, mmr_429_once, open_details_of_me, retries_done};
use super::*;

const S: Duration = Duration::from_secs(1);

/// Answers `status` on `p` (the first `times` requests, or all) ahead of the
/// harness's own mocks.
async fn fails(h: &Harness, verb: &str, p: &str, status: u16, times: Option<u64>) {
    let mock = Mock::given(method(verb))
        .and(path(p))
        .respond_with(ResponseTemplate::new(status))
        .with_priority(1);
    match times {
        Some(n) => mock.up_to_n_times(n).mount(&h.server).await,
        None => mock.mount(&h.server).await,
    }
}

/// Working endpoints for `puuid` (the harness's default for a stranger is 500
/// everywhere): one competitive match `m-<puuid>`, its details, an MMR.
async fn mount_player(h: &Harness, puuid: &str) {
    let id = format!("m-{puuid}");
    let start = now_ms() - 3_600_000;
    let prio = |m: Mock| m.with_priority(1);
    prio(
        Mock::given(method("GET"))
            .and(path(
                format!("/mmr/v1/players/{puuid}/competitiveupdates").as_str(),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "Subject": puuid,
                "Matches": [{
                    "MatchID": id, "MapID": "/Game/Maps/Ascent/Ascent", "SeasonID": ACT,
                    "MatchStartTime": start, "TierAfterUpdate": 15, "TierBeforeUpdate": 15,
                    "RankedRatingAfterUpdate": 42, "RankedRatingBeforeUpdate": 30,
                    "RankedRatingEarned": 12
                }]
            }))),
    )
    .mount(&h.server)
    .await;
    prio(
        Mock::given(method("GET"))
            .and(path(format!("/mmr/v1/players/{puuid}").as_str()))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "Subject": puuid,
                "QueueSkills": {"competitive": {"SeasonalInfoBySeasonID": {ACT: {
                    "SeasonID": ACT, "CompetitiveTier": 15, "RankedRating": 42,
                    "NumberOfWins": 6, "NumberOfGames": 10, "WinsByTier": {"15": 4}
                }}}}
            }))),
    )
    .mount(&h.server)
    .await;
    prio(
        Mock::given(method("GET"))
            .and(path(format!("/match-history/v1/history/{puuid}").as_str()))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "Subject": puuid,
                "History": [{"MatchID": id, "GameStartTime": start, "QueueID": "competitive"}]
            }))),
    )
    .mount(&h.server)
    .await;
    prio(
        Mock::given(method("GET"))
            .and(path(format!("/match-details/v1/matches/{id}").as_str()))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "matchInfo": {
                    "matchId": id, "mapId": "/Game/Maps/Ascent/Ascent",
                    "gameStartMillis": start, "queueId": "competitive",
                    "isRanked": true, "completionState": "Completed"
                },
                "players": [{
                    "subject": puuid, "gameName": "N", "tagLine": "T", "teamId": "Red",
                    "partyId": "q", "characterId": "agent-o", "competitiveTier": 15,
                    "stats": {"score": 1, "roundsPlayed": 20, "kills": 5, "deaths": 15, "assists": 1}
                }],
                "teams": [{"teamId": "Red", "won": false, "roundsWon": 7}],
                "roundResults": []
            }))),
    )
    .mount(&h.server)
    .await;
}

fn roster_of(h: &Harness, puuids: &[&str]) {
    let mut live = lock(&h.st.live);
    live.stage = Stage::Ingame;
    live.in_match = true;
    live.slots = puuids
        .iter()
        .map(|p| Slot {
            roster: roster(p, false),
            db: PlayerDb::default(),
        })
        .collect();
    live.names_attempted
        .extend(puuids.iter().map(|p| p.to_string()));
    live.self_roster = Some(roster(ME, false));
}

// ---------------------------------------------------------------------
// finding 1: rank first
// ---------------------------------------------------------------------

#[tokio::test]
async fn every_players_rank_requests_go_out_before_any_history_or_match_details() {
    let h = harness().await;
    mount_player(&h, OTHER).await;
    mount_player(&h, MATE).await;
    roster_of(&h, &[ME, OTHER, MATE]);
    drain(&h).await;

    let reqs = h.server.received_requests().await.unwrap();
    let pos = |pred: &dyn Fn(&str) -> bool| -> Vec<usize> {
        reqs.iter()
            .enumerate()
            .filter(|(_, r)| pred(r.url.path()))
            .map(|(i, _)| i)
            .collect()
    };
    let rank = pos(&|p| p.starts_with("/mmr/"));
    let rest = pos(&|p| p.starts_with("/match-history/") || p.starts_with("/match-details/"));
    assert_eq!(rank.len(), 6, "comp updates + mmr for three players");
    assert!(!rest.is_empty());
    assert!(
        rank.iter().max() < rest.iter().min(),
        "all rank requests {rank:?} before the first of {rest:?}"
    );
    // Same request count as before the split: nothing extra, nothing less.
    assert_eq!(
        count(
            &h.server,
            "GET",
            &format!("/match-history/v1/history/{OTHER}")
        )
        .await,
        1
    );
    assert_eq!(
        count(&h.server, "GET", &format!("/mmr/v1/players/{MATE}")).await,
        1
    );
}

#[tokio::test]
async fn a_card_shows_its_rank_before_its_matches_have_been_fetched() {
    let h = harness().await;
    mount_player(&h, OTHER).await;
    roster_of(&h, &[ME, OTHER]);
    drain(&h).await;

    let states = h.rec.live.lock().unwrap().clone();
    let other = |s: &LiveState| s.players.iter().find(|c| c.puuid == OTHER).cloned();
    // Some published state has OTHER's rank as a value while their K/D and
    // last 5 are still skeletons, and it comes before the final one.
    let early = states.iter().position(|s| {
        other(s).is_some_and(|c| matches!(c.rank, Field::Value(_)) && c.last5.is_loading())
    });
    assert!(early.is_some(), "rank published after the rank phase");
    let last = other(states.last().unwrap()).unwrap();
    assert!(matches!(last.rank, Field::Value(_)));
    assert!(matches!(last.last5, Field::Value(_)), "then the matches");
}

#[tokio::test]
async fn a_rank_phase_that_failed_still_lets_the_matches_phase_run_and_the_card_settle() {
    let h = harness().await;
    roster_of(&h, &[ME]);
    fails(&h, "GET", &format!("/mmr/v1/players/{ME}"), 500, None).await;
    fails(
        &h,
        "GET",
        &format!("/mmr/v1/players/{ME}/competitiveupdates"),
        500,
        None,
    )
    .await;
    drain(&h).await;
    let me = card_json(&h.st, ME);
    assert_eq!(me["peak"], Value::Null);
    assert!(me["last5"].is_array(), "the matches came regardless");
    assert!(lock(&h.st.live).synced.contains(ME));
}

// ---------------------------------------------------------------------
// finding 3: names retry
// ---------------------------------------------------------------------

fn put_names() -> &'static str {
    "/name-service/v2/players"
}

/// OTHER visible and synced: only the names job is left.
fn names_only(h: &Harness) {
    let mut live = lock(&h.st.live);
    live.stage = Stage::Ingame;
    live.in_match = true;
    live.slots = vec![
        Slot {
            roster: roster(ME, false),
            db: PlayerDb::default(),
        },
        Slot {
            roster: roster(OTHER, false),
            db: PlayerDb::default(),
        },
    ];
    live.synced.extend([ME.to_string(), OTHER.to_string()]);
    live.names_attempted.insert(ME.into());
    live.self_name_attempted = true;
}

#[tokio::test]
async fn a_failed_names_request_is_asked_again_after_the_delay_and_the_name_arrives() {
    let h = harness().await;
    fails(&h, "PUT", put_names(), 500, Some(1)).await;
    names_only(&h);

    assert!(drain(&h).await);
    assert_eq!(count(&h.server, "PUT", put_names()).await, 1);
    assert!(
        !lock(&h.st.live).names_attempted.contains(OTHER),
        "an error is not an answer"
    );
    let card = card_json(&h.st, OTHER);
    assert_eq!(card["name"], Value::Null, "`—` once synced, no skeleton");

    // Waiting is not work.
    assert!(!drain(&h).await);
    advance(&h, S);
    assert!(!drain(&h).await, "not before the delay");
    advance(&h, 2 * S);
    assert!(drain(&h).await, "the retry");
    assert_eq!(count(&h.server, "PUT", put_names()).await, 2);
    assert!(lock(&h.st.live).names_attempted.contains(OTHER));
    let card = card_json(&h.st, OTHER);
    assert_eq!(card["name"]["name"], "Other", "the name filled in");

    // Answered: never asked again.
    advance(&h, 1_000 * S);
    assert!(!drain(&h).await);
    assert_eq!(count(&h.server, "PUT", put_names()).await, 2);
}

#[tokio::test]
async fn a_names_request_that_always_fails_is_asked_three_times_and_then_given_up() {
    let h = harness().await;
    fails(&h, "PUT", put_names(), 500, None).await;
    names_only(&h);

    assert!(drain(&h).await);
    advance(&h, 3 * S);
    assert!(drain(&h).await);
    advance(&h, 7 * S);
    assert!(drain(&h).await, "the third and last try");
    assert_eq!(count(&h.server, "PUT", put_names()).await, 3);
    assert!(lock(&h.st.live).names_attempted.contains(OTHER));
    advance(&h, 100_000 * S);
    assert!(!drain(&h).await, "no fourth try");
    assert_eq!(count(&h.server, "PUT", put_names()).await, 3);
}

#[tokio::test]
async fn a_hidden_player_is_never_sent_to_name_service_by_the_retry_either() {
    let h = harness().await;
    fails(&h, "PUT", put_names(), 500, Some(1)).await;
    {
        let mut live = lock(&h.st.live);
        live.stage = Stage::Ingame;
        live.in_match = true;
        live.slots = vec![
            Slot {
                roster: roster(ME, false),
                db: PlayerDb::default(),
            },
            Slot {
                roster: roster(OTHER, false),
                db: PlayerDb::default(),
            },
            Slot {
                roster: roster(HIDDEN, true),
                db: PlayerDb::default(),
            },
        ];
        live.synced
            .extend([ME.to_string(), OTHER.to_string(), HIDDEN.to_string()]);
        live.names_attempted.insert(ME.into());
    }
    drain(&h).await;
    advance(&h, 3 * S);
    drain(&h).await;
    for r in h.server.received_requests().await.unwrap() {
        if r.url.path() == put_names() {
            let body = String::from_utf8_lossy(&r.body);
            assert!(!body.contains(HIDDEN), "{body}");
        }
    }
    assert_eq!(count(&h.server, "PUT", put_names()).await, 2);
}

// ---------------------------------------------------------------------
// finding 5: partial sync
// ---------------------------------------------------------------------

fn history(p: &str) -> String {
    format!("/match-history/v1/history/{p}")
}

#[tokio::test]
async fn a_match_details_download_that_failed_is_retried_and_only_the_missing_id_is_fetched() {
    let h = harness().await;
    fails(&h, "GET", "/match-details/v1/matches/m2", 500, Some(1)).await;
    lobby(&h);

    assert!(drain(&h).await);
    assert_eq!(
        count(&h.server, "GET", "/match-details/v1/matches/m2").await,
        1
    );
    assert_eq!(
        count(&h.server, "GET", "/match-details/v1/matches/m1").await,
        1
    );
    assert_eq!(
        lock(&h.st.live).rank_retries.retries_done(ME),
        Some(0),
        "a retry is scheduled"
    );
    assert!(!drain(&h).await, "20 s away");
    advance(&h, 20 * S);
    assert!(drain(&h).await, "the retry");

    // m2 again, m1 not; the lists again; mmr not (no new competitive match).
    assert_eq!(
        count(&h.server, "GET", "/match-details/v1/matches/m2").await,
        2
    );
    assert_eq!(
        count(&h.server, "GET", "/match-details/v1/matches/m1").await,
        1
    );
    assert_eq!(count(&h.server, "GET", &history(ME)).await, 2);
    assert_eq!(
        count(&h.server, "GET", &format!("/mmr/v1/players/{ME}")).await,
        1
    );
    // Complete now: the round ended.
    assert!(lock(&h.st.live).rank_retries.is_empty());
    advance(&h, 100_000 * S);
    assert!(!drain(&h).await);
}

#[tokio::test]
async fn a_failed_history_call_is_retried_and_party_marks_wait_for_it() {
    let h = harness().await;
    fails(&h, "GET", &history(ME), 500, Some(1)).await;
    lobby(&h);

    assert!(drain(&h).await);
    {
        let live = lock(&h.st.live);
        assert!(live.synced.contains(ME), "the card settled");
        assert!(!live.party_ready.contains(ME), "no fresh history");
        assert_eq!(live.rank_retries.retries_done(ME), Some(0));
    }
    advance(&h, 20 * S);
    assert!(drain(&h).await);
    assert!(lock(&h.st.live).party_ready.contains(ME), "fresh now");
    assert!(lock(&h.st.live).rank_retries.is_empty());
}

#[tokio::test]
async fn a_partial_sync_that_keeps_failing_is_retried_twice_in_all() {
    let h = harness().await;
    fails(&h, "GET", "/match-details/v1/matches/m2", 500, None).await;
    lobby(&h);

    assert!(drain(&h).await);
    advance(&h, 20 * S);
    assert!(drain(&h).await);
    advance(&h, 60 * S);
    assert!(drain(&h).await);
    assert_eq!(
        count(&h.server, "GET", "/match-details/v1/matches/m2").await,
        3
    );
    advance(&h, 100_000 * S);
    assert!(!drain(&h).await, "no third retry");
}

#[tokio::test]
async fn a_complete_sync_schedules_no_retry() {
    let h = harness().await;
    lobby(&h);
    assert!(drain(&h).await);
    assert!(lock(&h.st.live).rank_retries.is_empty());
}

#[tokio::test]
async fn a_match_details_404_is_terminal_and_not_a_partial_sync() {
    // Riot no longer has the match: a retry cannot get it, so no round is
    // scheduled for it (the backfill treats 404 the same way).
    let h = harness().await;
    fails(&h, "GET", "/match-details/v1/matches/m2", 404, None).await;
    lobby(&h);
    assert!(drain(&h).await);
    assert_eq!(
        count(&h.server, "GET", "/match-details/v1/matches/m2").await,
        1
    );
    assert!(lock(&h.st.live).rank_retries.is_empty(), "no retry round");
    assert!(lock(&h.st.live).synced.contains(ME));
    advance(&h, 100_000 * S);
    assert!(!drain(&h).await);
}

// ---------------------------------------------------------------------
// fix round 1: atomic notes, batch edge cases
// ---------------------------------------------------------------------

#[tokio::test]
async fn a_rank_failure_is_recorded_with_the_ranked_mark_even_if_the_worker_is_aborted_right_after()
{
    // The rank phase failed (mmr 500) and Player Details of ME is open, so the
    // publish awaits (the details rebuild). The mark and the retry note are
    // one critical section: whenever the player is seen ranked, the retry is
    // already on the schedule, so an abort at that await loses nothing.
    let h = harness().await;
    fails(&h, "GET", &format!("/mmr/v1/players/{ME}"), 500, None).await;
    open_details_of_me(&h).await;
    let (st, session) = (h.st.clone(), h.session.clone());
    let task = tokio::spawn(async move { crate::sync::rank_step(&st, &session, ME, true).await });
    let mut seen = false;
    for _ in 0..10_000 {
        if lock(&h.st.live).synced.rank_done(ME) {
            seen = true;
            assert_eq!(retries_done(&h, ME), Some(0), "noted with the mark");
            task.abort();
            break;
        }
        tokio::task::yield_now().await;
    }
    assert!(seen, "the rank phase was marked");
}

#[tokio::test]
async fn a_partial_matches_phase_is_recorded_with_the_synced_mark_even_if_aborted_right_after() {
    let h = harness().await;
    fails(&h, "GET", "/match-details/v1/matches/m2", 500, None).await;
    open_details_of_me(&h).await;
    let (st, session) = (h.st.clone(), h.session.clone());
    let task = tokio::spawn(async move {
        crate::sync::rank_step(&st, &session, ME, true).await?;
        crate::sync::matches_step(&st, &session, ME, true).await
    });
    let mut seen = false;
    for _ in 0..10_000 {
        if lock(&h.st.live).synced.contains(ME) {
            seen = true;
            assert_eq!(retries_done(&h, ME), Some(0), "noted with the mark");
            task.abort();
            break;
        }
        tokio::task::yield_now().await;
    }
    assert!(seen);
}

#[tokio::test]
async fn a_batch_member_that_meets_the_mmr_hold_waits_for_it_with_nothing_used() {
    // The mmr family is on hold before the batch starts: all three players'
    // rank calls are refused locally (nothing sent), each waits for the hold
    // with both retries unused, and their matches phases still run.
    let h = harness().await;
    mmr_429_once(&h, ME, "60").await;
    assert!(h.session.mmr(ME).await.is_err(), "the 429 sets the hold");
    roster_of(&h, &[ME, OTHER, MATE]);
    tokio::time::timeout(Duration::from_secs(10), drain(&h))
        .await
        .expect("the worker is not parked by the hold");
    for p in [ME, OTHER, MATE] {
        assert_eq!(retries_done(&h, p), Some(0), "{p}: held, nothing used");
        assert!(
            lock(&h.st.live).synced.contains(p),
            "{p}: matches phase ran"
        );
    }
    assert_eq!(
        count(&h.server, "GET", &format!("/mmr/v1/players/{ME}")).await,
        1
    );
    for p in [ME, OTHER, MATE] {
        assert_eq!(
            count(
                &h.server,
                "GET",
                &format!("/mmr/v1/players/{p}/competitiveupdates")
            )
            .await,
            0,
            "{p}: not sent"
        );
    }
}

#[tokio::test]
async fn a_client_that_goes_away_mid_batch_ends_the_worker_and_marks_nobody() {
    let h = harness().await;
    roster_of(&h, &[ME, OTHER, MATE]);
    std::fs::remove_file(
        h._dir
            .path()
            .join("Riot Games")
            .join("Riot Client")
            .join("Config")
            .join("lockfile"),
    )
    .unwrap();
    assert!(drain(&h).await);
    let live = lock(&h.st.live);
    for p in [ME, OTHER, MATE] {
        assert!(!live.synced.rank_done(p), "{p}: nothing was answered");
    }
    assert!(live.rank_retries.is_empty(), "no retry round");
}

#[tokio::test]
async fn every_failed_names_try_leaves_a_wake_up_for_the_next_one() {
    // The worker exits after a failed names step and never waits; the poll
    // tick's `names_wakeup_due` is what starts the next one, so each failure
    // must leave a deferral that ends at the next retry.
    let h = harness().await;
    fails(&h, "PUT", put_names(), 500, Some(2)).await;
    names_only(&h);

    assert!(drain(&h).await, "try 1 fails");
    assert!(!lock(&h.st.live).names_wakeup_due(), "not before the delay");
    advance(&h, 3 * S);
    assert!(lock(&h.st.live).names_wakeup_due(), "woken for try 2");
    assert!(drain(&h).await, "try 2 fails");
    assert!(!lock(&h.st.live).names_wakeup_due());
    advance(&h, 7 * S);
    assert!(lock(&h.st.live).names_wakeup_due(), "woken for try 3");
    assert!(drain(&h).await, "try 3 answers");
    assert_eq!(count(&h.server, "PUT", put_names()).await, 3);
    assert_eq!(card_json(&h.st, OTHER)["name"]["name"], "Other");
}

/// The PUT bodies sent to name-service, in order, as the puuids they carried.
async fn name_asks(h: &Harness) -> Vec<Vec<String>> {
    h.server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.method.as_str() == "PUT" && r.url.path() == put_names())
        .map(|r| serde_json::from_slice::<Vec<String>>(&r.body).expect("a list of puuids"))
        .collect()
}

fn join(h: &Harness, puuid: &str) {
    let mut live = lock(&h.st.live);
    live.slots.push(Slot {
        roster: roster(puuid, false),
        db: PlayerDb::default(),
    });
    live.synced.insert(puuid.into());
}

#[tokio::test]
async fn a_new_player_is_asked_at_once_while_an_older_retry_waits_and_the_retry_still_wakes() {
    let h = harness().await;
    fails(&h, "PUT", put_names(), 500, Some(1)).await;
    names_only(&h);
    assert!(drain(&h).await, "OTHER's first try fails");

    join(&h, MATE);
    assert!(
        drain(&h).await,
        "MATE is asked now, not after OTHER's delay"
    );
    let asks = name_asks(&h).await;
    assert_eq!(asks.len(), 2);
    assert_eq!(asks[1], vec![MATE.to_string()], "alone: OTHER waits");

    // OTHER's retry still wakes on time.
    assert!(!lock(&h.st.live).names_wakeup_due());
    advance(&h, 3 * S);
    assert!(lock(&h.st.live).names_wakeup_due());
    assert!(drain(&h).await);
    assert_eq!(name_asks(&h).await[2], vec![OTHER.to_string()]);
    assert_eq!(card_json(&h.st, OTHER)["name"]["name"], "Other");
}

#[tokio::test]
async fn a_staggered_retry_is_woken_after_a_names_step_that_succeeded() {
    // OTHER fails twice (try 2 due +6 s) while MATE, who joined between,
    // fails once (due +2 s). MATE answers alone at +2 s: the step that
    // answered must leave the wake-up for OTHER's retry.
    let h = harness().await;
    fails(&h, "PUT", put_names(), 500, Some(2)).await;
    names_only(&h);
    assert!(drain(&h).await, "request 1: [OTHER] fails");
    advance(&h, 3 * S);
    join(&h, MATE);
    assert!(drain(&h).await, "request 2: [OTHER, MATE] fails");
    assert_eq!(name_asks(&h).await[1].len(), 2);

    assert!(!lock(&h.st.live).names_wakeup_due());
    advance(&h, 3 * S);
    assert!(lock(&h.st.live).names_wakeup_due(), "MATE is due");
    assert!(drain(&h).await, "request 3: [MATE] is answered");
    assert_eq!(name_asks(&h).await[2], vec![MATE.to_string()]);

    // Only the success arm can have armed this one.
    assert!(!lock(&h.st.live).names_wakeup_due());
    advance(&h, 4 * S);
    assert!(lock(&h.st.live).names_wakeup_due(), "OTHER is due");
    assert!(drain(&h).await, "request 4");
    assert_eq!(name_asks(&h).await[3], vec![OTHER.to_string()]);
}

#[tokio::test]
async fn a_task_that_waited_on_anothers_404_does_not_count_the_match_as_missing() {
    let h = harness().await;
    let id = "m-dead";
    let guard = match h.st.claim_details_fetch(id) {
        crate::state::DetailsClaim::Mine(g) => g,
        _ => panic!("free"),
    };
    let waiter = crate::sync::fetch_missing_details_full(&h.st, &h.session, vec![id.to_string()]);
    let owner = async {
        tokio::time::sleep(Duration::from_millis(50)).await;
        // The owner's download answered 404.
        h.st.note_details_404(id);
        drop(guard);
    };
    let (fetched, ()) = tokio::join!(waiter, owner);
    let fetched = fetched.unwrap();
    assert!(
        !fetched.incomplete,
        "a dead id is terminal for the waiter too"
    );
    assert_eq!(
        count(&h.server, "GET", &format!("/match-details/v1/matches/{id}")).await,
        0
    );
}

#[tokio::test]
async fn a_task_that_waited_on_a_download_that_failed_still_counts_it_as_missing() {
    let h = harness().await;
    let id = "m-flaky";
    let guard = match h.st.claim_details_fetch(id) {
        crate::state::DetailsClaim::Mine(g) => g,
        _ => panic!("free"),
    };
    let waiter = crate::sync::fetch_missing_details_full(&h.st, &h.session, vec![id.to_string()]);
    let owner = async {
        tokio::time::sleep(Duration::from_millis(50)).await;
        drop(guard); // failed in its own task: nothing stored, no 404
    };
    let (fetched, ()) = tokio::join!(waiter, owner);
    assert!(fetched.unwrap().incomplete);
}
