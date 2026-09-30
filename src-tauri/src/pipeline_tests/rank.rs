//! R154 and R155's pipeline tests: a player's rank when Riot's MMR request
//! has no answer, and the bounded retry of a rank sync that failed — against
//! the mock Riot client. A child of `pipeline_tests`, so it shares that
//! module's harness (mock Riot, in-memory DB) and helpers through `super::*`.
//!
//! The waits (20 s, then 60 s) are never slept: the live model's test clock
//! (`LiveModel::advance_clock`) moves forward instead.

use std::time::Duration;

use super::*;

const S: Duration = Duration::from_secs(1);

fn mmr_path(puuid: &str) -> String {
    format!("/mmr/v1/players/{puuid}")
}

fn comp_path(puuid: &str) -> String {
    format!("/mmr/v1/players/{puuid}/competitiveupdates")
}

/// The user alone in the Lobby, nothing synced yet.
fn lobby(h: &Harness) {
    let mut live = lock(&h.st.live);
    live.stage = Stage::Menus;
    live.in_match = false;
    live.slots = vec![Slot {
        roster: roster(ME, false),
        db: PlayerDb::default(),
    }];
    live.self_roster = Some(roster(ME, false));
}

/// Starts a worker if there is work, lets it drain; `false`: no work.
async fn drain(h: &Harness) -> bool {
    let Some(id) = lock(&h.st.live).try_start_worker() else {
        return false;
    };
    run_worker(h.st.clone(), h.session.clone(), id).await;
    assert!(!lock(&h.st.live).worker_running(), "claim released on exit");
    true
}

fn advance(h: &Harness, by: Duration) {
    lock(&h.st.live).advance_clock(by);
}

/// `/mmr/v1/players/{puuid}` answers `status` (the first `times` requests, or
/// every one when `None`) ahead of the harness's own mock.
async fn mmr_answers(h: &Harness, puuid: &str, status: u16, times: Option<u64>) {
    let mock = Mock::given(method("GET"))
        .and(path(mmr_path(puuid).as_str()))
        .respond_with(ResponseTemplate::new(status))
        .with_priority(1);
    match times {
        Some(n) => mock.up_to_n_times(n).mount(&h.server).await,
        None => mock.mount(&h.server).await,
    }
}

/// An MMR answer that differs from the RR rows' rank (tier 16, 50 RR): once
/// it is there, the card must show it and not the fallback.
async fn mmr_says_tier_16(h: &Harness) {
    Mock::given(method("GET"))
        .and(path(mmr_path(ME).as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "Subject": ME,
            "QueueSkills": {"competitive": {"SeasonalInfoBySeasonID": {ACT: {
                "SeasonID": ACT, "CompetitiveTier": 16, "RankedRating": 50,
                "NumberOfWins": 7, "NumberOfGames": 11, "WinsByTier": {"16": 5}
            }}}}
        })))
        .with_priority(2)
        .mount(&h.server)
        .await;
}

// ---------------------------------------------------------------------
// R154: the rank from stored matches
// ---------------------------------------------------------------------

#[tokio::test]
async fn a_rank_request_with_no_answer_still_shows_the_rank_from_the_rr_rows() {
    let h = harness().await;
    mmr_answers(&h, ME, 500, None).await;
    lobby(&h);
    drain(&h).await;

    assert_eq!(count(&h.server, "GET", &mmr_path(ME)).await, 1, "asked");
    let me = card_json(&h.st, ME);
    // Path (a): the newest current-act RR row — tier 15, 42 RR after it.
    assert_eq!(me["rank"], json!({"tier": 15, "rr": 42}));
    // Everything else that comes from MMR is `—`, not skeletons.
    for key in ["peak", "prevActTier", "actWinrate", "actWins", "actGames"] {
        assert_eq!(me[key], Value::Null, "{key}");
    }
    // What does not come from MMR is unchanged.
    assert!(me["kd"].is_number() && me["last5"].is_array());
}

#[tokio::test]
async fn with_no_rr_row_either_the_rank_is_the_tier_of_a_stored_match_and_the_rr_is_null() {
    // OTHER: comp updates and MMR both fail (the harness's default), but
    // their match history lists a competitive match of the current act whose
    // details carry their tier (14): path (b).
    let h = harness().await;
    let start = now_ms() - 3_600_000;
    Mock::given(method("GET"))
        .and(path(format!("/match-history/v1/history/{OTHER}").as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "Subject": OTHER,
            "History": [{"MatchID": "m-other", "GameStartTime": start, "QueueID": "competitive"}]
        })))
        .with_priority(1)
        .mount(&h.server)
        .await;
    get(
        &h.server,
        "/match-details/v1/matches/m-other",
        json!({
            "matchInfo": {
                "matchId": "m-other", "mapId": "/Game/Maps/Ascent/Ascent",
                "gameStartMillis": start, "queueId": "competitive",
                "isRanked": true, "completionState": "Completed"
            },
            "players": [{
                "subject": OTHER, "gameName": "Other", "tagLine": "OTH", "teamId": "Red",
                "partyId": "q", "characterId": "agent-o", "competitiveTier": 14,
                "stats": {"score": 1, "roundsPlayed": 20, "kills": 5, "deaths": 15, "assists": 1}
            }],
            "teams": [{"teamId": "Red", "won": false, "roundsWon": 7}],
            "roundResults": []
        }),
    )
    .await;
    {
        let mut live = lock(&h.st.live);
        live.stage = Stage::Ingame;
        live.slots = vec![Slot {
            roster: roster(OTHER, false),
            db: PlayerDb::default(),
        }];
    }
    drain(&h).await;

    let card = card_json(&h.st, OTHER);
    assert_eq!(
        card["rank"],
        json!({"tier": 14, "rr": null}),
        "the tier of the stored match, its RR unknown"
    );
    assert_eq!(card["peak"], Value::Null);
}

#[tokio::test]
async fn a_later_mmr_answer_replaces_the_fallback() {
    let h = harness().await;
    mmr_answers(&h, ME, 500, Some(1)).await;
    mmr_says_tier_16(&h).await;
    lobby(&h);
    drain(&h).await;
    assert_eq!(card_json(&h.st, ME)["rank"], json!({"tier": 15, "rr": 42}));

    advance(&h, 20 * S);
    assert!(drain(&h).await, "the retry");
    let me = card_json(&h.st, ME);
    assert_eq!(me["rank"], json!({"tier": 16, "rr": 50}), "MMR wins");
    assert!(me["peak"].is_object(), "the other MMR fields fill in too");
    assert_eq!(me["actGames"], json!(11));
}

// ---------------------------------------------------------------------
// R155: the bounded retry
// ---------------------------------------------------------------------

#[tokio::test]
async fn an_mmr_call_that_fails_once_is_asked_again_after_20s_and_then_never_again() {
    let h = harness().await;
    mmr_answers(&h, ME, 500, Some(1)).await;
    mmr_says_tier_16(&h).await;
    lobby(&h);

    // The first pass: mmr answers 500.
    assert!(drain(&h).await);
    assert_eq!(count(&h.server, "GET", &mmr_path(ME)).await, 1);
    let me = card_json(&h.st, ME);
    assert_eq!(me["peak"], Value::Null, "no MMR yet: `—`");

    // Waiting for the retry is not work; the worker is gone and time is not.
    assert!(!drain(&h).await, "nothing due");
    advance(&h, 19 * S);
    assert!(!drain(&h).await, "not before 20 s");
    assert_eq!(count(&h.server, "GET", &mmr_path(ME)).await, 1);
    advance(&h, S);
    assert!(drain(&h).await, "20 s: the retry");

    // Exactly two mmr requests, and the card has its MMR fields now.
    assert_eq!(count(&h.server, "GET", &mmr_path(ME)).await, 2);
    let me = card_json(&h.st, ME);
    assert!(me["peak"].is_object() && me["prevActTier"].is_null());
    assert_eq!(me["actWins"], json!(7));

    // Nothing more, however long the wait: the round ended with the answer.
    advance(&h, 100_000 * S);
    assert!(!drain(&h).await);
    assert_eq!(count(&h.server, "GET", &mmr_path(ME)).await, 2);
}

#[tokio::test]
async fn an_mmr_call_that_always_fails_is_asked_three_times_in_all() {
    let h = harness().await;
    mmr_answers(&h, ME, 500, None).await;
    lobby(&h);

    assert!(drain(&h).await, "first pass");
    assert_eq!(count(&h.server, "GET", &mmr_path(ME)).await, 1);

    advance(&h, 20 * S);
    assert!(drain(&h).await, "first retry, 20 s after the failure");
    assert_eq!(count(&h.server, "GET", &mmr_path(ME)).await, 2);

    // The second is 60 s after the first retry failed, not 20.
    advance(&h, 59 * S);
    assert!(!drain(&h).await);
    advance(&h, S);
    assert!(drain(&h).await, "second retry");
    assert_eq!(count(&h.server, "GET", &mmr_path(ME)).await, 3);

    // No more, whatever happens afterwards on the same roster: time passes,
    // the roster is installed again, the worker is asked for again.
    for _ in 0..3 {
        advance(&h, 10_000 * S);
        {
            let mut live = lock(&h.st.live);
            live.prune_rank_retries();
            live.note_hidden();
        }
        assert!(!drain(&h).await);
    }
    assert_eq!(count(&h.server, "GET", &mmr_path(ME)).await, 3);
    // Every sync also asked comp updates once: three syncs, three asks.
    assert_eq!(count(&h.server, "GET", &comp_path(ME)).await, 3);
    // The card still shows the rank the RR rows give.
    assert_eq!(card_json(&h.st, ME)["rank"], json!({"tier": 15, "rr": 42}));
}

#[tokio::test]
async fn a_comp_updates_call_that_fails_once_is_retried_too() {
    // The comp-updates failure skips the mmr call in that pass (they share
    // the mmr bucket): the retry asks both.
    let h = harness().await;
    Mock::given(method("GET"))
        .and(path(comp_path(ME).as_str()))
        .respond_with(ResponseTemplate::new(500))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&h.server)
        .await;
    lobby(&h);
    assert!(drain(&h).await);
    assert_eq!(count(&h.server, "GET", &mmr_path(ME)).await, 0, "skipped");
    assert_eq!(card_json(&h.st, ME)["rank"], Value::Null);

    advance(&h, 20 * S);
    assert!(drain(&h).await);
    assert_eq!(count(&h.server, "GET", &comp_path(ME)).await, 2);
    assert_eq!(count(&h.server, "GET", &mmr_path(ME)).await, 1);
    assert!(card_json(&h.st, ME)["peak"].is_object());
}

#[tokio::test]
async fn a_player_who_left_the_roster_is_not_retried() {
    // ME and OTHER; OTHER's calls fail (the harness's default).
    let h = harness().await;
    {
        let mut live = lock(&h.st.live);
        live.stage = Stage::Ingame;
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
    }
    drain(&h).await;
    assert_eq!(count(&h.server, "GET", &comp_path(OTHER)).await, 1);

    // OTHER leaves (a new roster without them).
    {
        let mut live = lock(&h.st.live);
        live.slots.retain(|s| s.roster.puuid != OTHER);
        live.prune_rank_retries();
    }
    advance(&h, 100_000 * S);
    assert!(!drain(&h).await, "nothing to retry");
    assert_eq!(count(&h.server, "GET", &comp_path(OTHER)).await, 1);
}

#[tokio::test]
async fn no_retry_runs_while_valorant_is_closed() {
    let h = harness().await;
    mmr_answers(&h, ME, 500, None).await;
    lobby(&h);
    drain(&h).await;
    assert_eq!(count(&h.server, "GET", &mmr_path(ME)).await, 1);

    lock(&h.st.live).stage = Stage::Closed;
    advance(&h, 100_000 * S);
    assert!(!drain(&h).await, "Closed: no retry");
    assert_eq!(count(&h.server, "GET", &mmr_path(ME)).await, 1);
}
