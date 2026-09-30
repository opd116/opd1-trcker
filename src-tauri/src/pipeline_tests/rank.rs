//! R154's pipeline tests: a player's rank when Riot's MMR request has no
//! answer — against the mock Riot client. A child of `pipeline_tests`, so it
//! shares that module's harness (mock Riot, in-memory DB) and helpers through
//! `super::*`.

use super::*;

fn mmr_path(puuid: &str) -> String {
    format!("/mmr/v1/players/{puuid}")
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
