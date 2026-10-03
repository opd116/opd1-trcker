//! B1's pipeline tests: the user's own competitive-updates backfill against
//! the mock Riot client. A child of `pipeline_tests`, so it shares that
//! module's harness and helpers through `super::*`.
//!
//! The mock's pd side serves ME's `competitiveupdates` from a list of
//! synthetic rows (newest first, one minute apart, `cb000`, `cb001`, …),
//! honouring `startIndex` / `endIndex`, and the tests read the requests it
//! saw back in arrival order.

use std::collections::HashMap;

use super::rank::{advance, drain};
use super::*;
use crate::pipeline_tests::backfill::{base_ms, entry_start, requests};
use crate::sync::{
    anchor_json, comp_anchor_key, comp_backfill_step, parse_anchor, start_comp_backfill_run, Gone,
    COMP_BACKFILL_MAX_PAGES, COMP_BACKFILL_PAGE, COMP_BACKFILL_PAGE_GAP,
};

/// Row `i` of the synthetic competitive updates.
pub(crate) fn row_id(i: usize) -> String {
    format!("cb{i:03}")
}

fn comp_path() -> String {
    format!("/mmr/v1/players/{ME}/competitiveupdates")
}

/// Mounts ME's competitive updates: `rows` of them, a minute apart, beating the
/// harness's own two-answer mock (`with_priority(1)`); pages from `fail_from`
/// on answer 500.
pub(crate) async fn mount_comp_feed(server: &MockServer, rows: usize, fail_from: Option<usize>) {
    mount_comp_feed_spaced(server, rows, fail_from, 60_000).await;
}

/// [`mount_comp_feed`] with the rows `step_ms` apart.
pub(crate) async fn mount_comp_feed_spaced(
    server: &MockServer,
    rows: usize,
    fail_from: Option<usize>,
    step_ms: i64,
) {
    let base = base_ms();
    Mock::given(method("GET"))
        .and(path(comp_path().as_str()))
        .respond_with(move |req: &Request| {
            let q: HashMap<String, String> = req
                .url
                .query_pairs()
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect();
            let num =
                |k: &str, default: usize| q.get(k).and_then(|v| v.parse().ok()).unwrap_or(default);
            let start = num("startIndex", 0);
            if fail_from.is_some_and(|from| start >= from) {
                return ResponseTemplate::new(500);
            }
            let end = num("endIndex", start + COMP_BACKFILL_PAGE as usize).min(rows);
            let matches: Vec<Value> = (start..end.max(start))
                .map(|i| {
                    json!({
                        "MatchID": row_id(i), "MapID": "/Game/Maps/Ascent/Ascent",
                        "SeasonID": ACT, "MatchStartTime": base - i as i64 * step_ms,
                        "TierAfterUpdate": 15, "TierBeforeUpdate": 15,
                        "RankedRatingAfterUpdate": 42, "RankedRatingBeforeUpdate": 30,
                        "RankedRatingEarned": 12
                    })
                })
                .collect();
            ResponseTemplate::new(200).set_body_json(json!({"Subject": ME, "Matches": matches}))
        })
        .with_priority(1)
        .mount(server)
        .await;
}

/// The `startIndex` of every competitive-updates request, in arrival order.
async fn comp_starts(h: &Harness) -> Vec<String> {
    requests(&h.server)
        .await
        .iter()
        .filter(|r| r.url.path() == comp_path())
        .map(|r| {
            r.url
                .query_pairs()
                .find(|(k, _)| k == "startIndex")
                .map(|(_, v)| v.into_owned())
                .unwrap()
        })
        .collect()
}

/// How many rows ME has stored in `comp_updates`.
pub(crate) async fn stored_rows(st: &AppState) -> usize {
    st.db(|repo| {
        repo.conn().query_row(
            "SELECT COUNT(*) FROM comp_updates WHERE puuid = ?1",
            [ME],
            |r| r.get::<_, i64>(0),
        )
    })
    .await
    .unwrap() as usize
}

/// The competitive-updates anchor stored for `puuid`.
pub(crate) async fn stored_comp_anchor_of(st: &AppState, puuid: &str) -> Option<(String, i64)> {
    let key = comp_anchor_key(puuid);
    st.db(move |repo| repo.get_state(&key))
        .await
        .flatten()
        .and_then(|raw| parse_anchor(&raw))
}

pub(crate) async fn stored_comp_anchor(st: &AppState) -> Option<(String, i64)> {
    stored_comp_anchor_of(st, ME).await
}

/// Stores row `i` as `puuid`'s competitive-updates anchor.
pub(crate) async fn store_comp_anchor_for(st: &AppState, puuid: &str, i: usize) {
    let (key, raw) = (
        comp_anchor_key(puuid),
        anchor_json(&(row_id(i), entry_start(base_ms(), i))),
    );
    st.db(move |repo| repo.set_state(&key, &raw)).await.unwrap();
}

fn run_active(st: &AppState) -> bool {
    lock(&st.live).comp_backfill.is_some()
}

/// What the poll loop does on connect: start a run from the stored anchor,
/// then — a page a job, the next one not before the gap — claim the worker
/// and let it drain, moving the model's clock past the gap between the pages
/// (the poll tick's wake-up). Ends when the run has.
pub(crate) async fn run_comp_backfill(h: &Harness) {
    start_comp_backfill_run(&h.st).await;
    drain_run(h).await;
}

/// The worker passes of a started run (see [`run_comp_backfill`]).
async fn drain_run(h: &Harness) {
    let mut passes = 0;
    loop {
        advance(h, COMP_BACKFILL_PAGE_GAP + Duration::from_millis(500));
        let Some(id) = lock(&h.st.live).try_start_worker() else {
            break;
        };
        run_worker(h.st.clone(), h.session.clone(), id).await;
        assert!(!lock(&h.st.live).worker_running(), "claim released on exit");
        passes += 1;
        assert!(passes <= 20, "the run never ends");
    }
    assert!(!run_active(&h.st), "the run ended");
}

#[tokio::test]
async fn a_first_run_pages_back_to_the_act_start_and_leaves_an_anchor() {
    const H6: i64 = 6 * 3_600_000;
    let h = harness().await;
    // Rows six hours apart: row i is 6 h * i before the base.
    mount_comp_feed_spaced(&h.server, 100, None, H6).await;
    // My Stats takes rows up to a day (the edge) before the act's start, so
    // the run reaches that far: the act starts a day after the point between
    // rows 45 and 46.
    h.st.set_acts_for_test(vec![Act {
        id: ACT.into(),
        name: "ACT V".into(),
        episode_name: "V26".into(),
        start_ms: base_ms() - 45 * H6 - H6 / 2 + opd1_core::mystats::ACT_EDGE_MS,
        end_ms: now_ms() + 30 * 86_400_000,
    }]);

    run_comp_backfill(&h).await;

    // The third page (rows 40..60) holds rows before the edge: the last.
    assert_eq!(comp_starts(&h).await, ["0", "20", "40"]);
    let reqs = requests(&h.server).await;
    let param = |r: &Request, key: &str| -> Option<String> {
        r.url
            .query_pairs()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.into_owned())
    };
    for r in reqs.iter().filter(|r| r.url.path() == comp_path()) {
        assert_eq!(param(r, "queue").as_deref(), Some("competitive"));
        let (start, end) = (
            param(r, "startIndex").unwrap(),
            param(r, "endIndex").unwrap(),
        );
        assert_eq!(
            end.parse::<u32>().unwrap(),
            start.parse::<u32>().unwrap() + COMP_BACKFILL_PAGE,
            "20 rows a page"
        );
    }
    assert_eq!(stored_rows(&h.st).await, 60, "the three pages are stored");
    assert_eq!(
        stored_comp_anchor(&h.st).await,
        Some((row_id(0), base_ms()))
    );
    assert!(!run_active(&h.st));
}

#[tokio::test]
async fn rows_before_the_act_start_but_within_the_edge_are_still_paged_for() {
    // Review round 1: My Stats counts a row up to a day before valorant-api's
    // act start (Riot's rollout is hours off), decided by its season. The
    // minute-apart rows are all within the edge: the run pages to the end of
    // the history, five full pages and an empty one.
    let h = harness().await;
    mount_comp_feed(&h.server, 100, None).await;
    h.st.set_acts_for_test(vec![Act {
        id: ACT.into(),
        name: "ACT V".into(),
        episode_name: "V26".into(),
        start_ms: entry_start(base_ms(), 45) - 30_000,
        end_ms: now_ms() + 30 * 86_400_000,
    }]);

    run_comp_backfill(&h).await;

    assert_eq!(comp_starts(&h).await.len(), 6);
    assert_eq!(stored_rows(&h.st).await, 100);
}

#[tokio::test]
async fn a_run_stops_at_the_anchor_and_the_next_one_asks_a_single_page() {
    let h = harness().await;
    mount_comp_feed(&h.server, 100, None).await;
    // The last finished run saw row 25 as its newest.
    store_comp_anchor_for(&h.st, ME, 25).await;

    run_comp_backfill(&h).await;

    // Page 2 (rows 20..40) holds the anchor: the last page.
    assert_eq!(comp_starts(&h).await, ["0", "20"]);
    assert_eq!(stored_rows(&h.st).await, 40);
    assert_eq!(
        stored_comp_anchor(&h.st).await.map(|a| a.0),
        Some(row_id(0))
    );

    // The anchor is row 0 now: one page, nothing new, anchor unchanged.
    run_comp_backfill(&h).await;
    assert_eq!(comp_starts(&h).await, ["0", "20", "0"]);
    assert_eq!(stored_rows(&h.st).await, 40);
    assert_eq!(
        stored_comp_anchor(&h.st).await.map(|a| a.0),
        Some(row_id(0))
    );
}

#[tokio::test]
async fn a_run_stops_at_an_anchor_that_left_the_history_by_its_start_time() {
    // The anchor's own row is no longer served (older than the window): the
    // first row older than its start ends the run, like the match backfill.
    let h = harness().await;
    mount_comp_feed(&h.server, 100, None).await;
    h.st.db(|repo| {
        repo.set_state(
            &comp_anchor_key(ME),
            &anchor_json(&("gone".to_string(), entry_start(base_ms(), 25) + 1)),
        )
    })
    .await
    .unwrap();

    run_comp_backfill(&h).await;

    assert_eq!(
        comp_starts(&h).await,
        ["0", "20"],
        "row 25 is older than it"
    );
}

#[tokio::test]
async fn a_run_stops_after_ten_pages() {
    let h = harness().await;
    mount_comp_feed(&h.server, 300, None).await;

    run_comp_backfill(&h).await;

    let starts = comp_starts(&h).await;
    assert_eq!(starts.len() as u32, COMP_BACKFILL_MAX_PAGES);
    assert_eq!(starts.last().map(String::as_str), Some("180"));
    assert_eq!(stored_rows(&h.st).await, 200);
    assert_eq!(
        stored_comp_anchor(&h.st).await.map(|a| a.0),
        Some(row_id(0))
    );
    assert!(!run_active(&h.st));
}

#[tokio::test]
async fn a_short_page_is_the_end_of_the_history() {
    let h = harness().await;
    mount_comp_feed(&h.server, 7, None).await;
    run_comp_backfill(&h).await;
    assert_eq!(comp_starts(&h).await, ["0"]);
    assert_eq!(stored_rows(&h.st).await, 7);
    assert_eq!(
        stored_comp_anchor(&h.st).await.map(|a| a.0),
        Some(row_id(0))
    );

    // A player with no competitive match: nothing to store, no anchor.
    let h = harness().await;
    mount_comp_feed(&h.server, 0, None).await;
    run_comp_backfill(&h).await;
    assert_eq!(comp_starts(&h).await, ["0"]);
    assert_eq!(stored_comp_anchor(&h.st).await, None);
    assert!(!run_active(&h.st));
}

#[tokio::test]
async fn a_real_failure_stops_the_run_keeps_the_anchor_and_the_next_trigger_starts_over() {
    let h = harness().await;
    // The second page fails for real.
    mount_comp_feed(&h.server, 100, Some(20)).await;
    store_comp_anchor_for(&h.st, ME, 90).await;

    run_comp_backfill(&h).await;

    assert_eq!(comp_starts(&h).await, ["0", "20"]);
    assert_eq!(stored_rows(&h.st).await, 20, "page 1 stays stored");
    assert!(!run_active(&h.st), "the run ended");
    assert_eq!(
        stored_comp_anchor(&h.st).await.map(|a| a.0),
        Some(row_id(90)),
        "the anchor moves only when a run finishes"
    );

    // The next trigger: a new run from the same anchor, from page 0.
    start_comp_backfill_run(&h.st).await;
    let run = lock(&h.st.live).comp_backfill.clone().expect("a new run");
    assert_eq!(run.next_index, 0);
    assert_eq!(run.anchor.map(|a| a.0), Some(row_id(90)));
}

#[tokio::test]
async fn a_hold_pauses_the_run_without_a_failure_and_it_resumes_after_it() {
    let h = harness().await;
    // A 429 on an earlier mmr call (the live rank phase of someone) sets the
    // mmr family's hold: a minute, which nothing waits out — the model's clock
    // moves past the pause, and the hold itself is ended explicitly. Mounted
    // first: at the same priority the first mock that matches answers.
    Mock::given(method("GET"))
        .and(path(comp_path().as_str()))
        .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "60"))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&h.server)
        .await;
    mount_comp_feed(&h.server, 30, None).await;
    assert!(h.session.comp_updates(ME, Some(0), Some(5)).await.is_err());
    assert_eq!(comp_starts(&h).await.len(), 1, "the 429");

    // The run starts inside the hold: its page is refused locally and paused.
    start_comp_backfill_run(&h.st).await;
    drain(&h).await;
    assert_eq!(comp_starts(&h).await.len(), 1, "nothing sent while held");
    assert!(run_active(&h.st), "the run waits, it did not fail");
    assert!(!lock(&h.st.live).comp_backfill_ready());
    assert!(lock(&h.st.live).try_start_worker().is_none(), "no work yet");

    // The pause is the hold's rest plus slack, not just the slack.
    advance(&h, Duration::from_secs(30));
    assert!(
        !lock(&h.st.live).comp_backfill_ready(),
        "still the hold's time"
    );
    assert!(!lock(&h.st.live).comp_backfill_resume_due());

    // The pause is over but Riot's hold still is (it is real time): the page
    // is refused again, not sent and not failed, and the run waits again.
    advance(&h, Duration::from_secs(40));
    assert!(
        lock(&h.st.live).comp_backfill_resume_due(),
        "the tick wakes it"
    );
    assert!(drain(&h).await);
    assert_eq!(comp_starts(&h).await.len(), 1, "still nothing sent");
    assert!(run_active(&h.st), "paused again, not failed");

    // The hold ends: the run goes on and finishes (30 rows: a full page, then
    // a short one).
    h.session.end_holds();
    advance(&h, Duration::from_secs(70));
    assert!(lock(&h.st.live).comp_backfill_resume_due());
    drain_run(&h).await;
    assert_eq!(comp_starts(&h).await, ["0", "0", "20"]);
    assert_eq!(stored_rows(&h.st).await, 30);
    assert_eq!(
        stored_comp_anchor(&h.st).await.map(|a| a.0),
        Some(row_id(0))
    );
}

#[tokio::test]
async fn no_page_goes_out_during_a_match() {
    for (stage, in_match) in [
        (Stage::Pregame, true),
        (Stage::Ingame, true),
        // A free-for-all match keeps the Lobby view but is still a match.
        (Stage::Menus, true),
    ] {
        let h = harness().await;
        mount_comp_feed(&h.server, 30, None).await;
        {
            let mut live = lock(&h.st.live);
            live.stage = stage;
            live.in_match = in_match;
        }
        start_comp_backfill_run(&h.st).await;

        assert!(
            lock(&h.st.live).try_start_worker().is_none(),
            "{stage:?}: a run alone is no work in a match"
        );
        // A step that was handed out just before the match began.
        comp_backfill_step(&h.st, &h.session).await.unwrap();
        assert!(comp_starts(&h).await.is_empty(), "{stage:?}: no request");
        assert!(run_active(&h.st), "{stage:?}: the cursor waits");

        // The match ends: the same run goes on.
        {
            let mut live = lock(&h.st.live);
            live.stage = Stage::Menus;
            live.in_match = false;
        }
        drain_run(&h).await;
        assert_eq!(stored_rows(&h.st).await, 30, "{stage:?}");
    }
}

#[tokio::test]
async fn one_page_per_job_so_a_live_job_goes_between_the_pages() {
    let h = harness().await;
    mount_comp_feed(&h.server, 100, None).await;
    start_comp_backfill_run(&h.st).await;
    comp_backfill_step(&h.st, &h.session).await.unwrap();
    assert_eq!(comp_starts(&h).await, ["0"], "one page per step");
    assert_eq!(
        lock(&h.st.live).comp_backfill.as_ref().unwrap().next_index,
        20
    );

    // A roster player arrives: the worker's next job is theirs, not page 2.
    {
        let mut live = lock(&h.st.live);
        live.stage = Stage::Menus;
        live.slots = vec![Slot {
            roster: roster(OTHER, false),
            db: PlayerDb::default(),
        }];
        live.names_attempted.insert(OTHER.into());
        let id = live.try_start_worker().expect("work");
        assert!(matches!(
            live.next_job(id),
            Some(crate::state::Job::Rank(_))
        ));
    }
}

#[tokio::test]
async fn anchors_are_kept_per_account() {
    let h = harness().await;
    mount_comp_feed(&h.server, 100, None).await;
    let other = "11111111-2222-3333-4444-555555555555";
    // Another account's anchor, at a row that would stop ME's run early.
    store_comp_anchor_for(&h.st, other, 3).await;

    run_comp_backfill(&h).await;

    // ME's run ignored it: no anchor of ME's own, so it went to the act
    // start (all 100 rows are in the harness's act: five full pages end at
    // the short... exactly 100 rows = five full pages and an empty sixth).
    assert_eq!(comp_starts(&h).await.len(), 6);
    assert_eq!(
        stored_comp_anchor(&h.st).await.map(|a| a.0),
        Some(row_id(0))
    );
    assert_eq!(
        stored_comp_anchor_of(&h.st, other).await.map(|a| a.0),
        Some(row_id(3)),
        "the other account's anchor is untouched"
    );
}

#[tokio::test]
async fn a_step_of_another_account_is_gone() {
    // The session belongs to ME, the model has switched to someone else.
    let h = harness().await;
    mount_comp_feed(&h.server, 30, None).await;
    start_comp_backfill_run(&h.st).await;
    lock(&h.st.live).self_puuid = Some("another-account".into());
    assert_eq!(comp_backfill_step(&h.st, &h.session).await, Err(Gone));
    assert!(comp_starts(&h).await.is_empty());
}

#[tokio::test]
async fn the_next_page_waits_for_the_gap_and_the_tick_wakes_it() {
    // Review round 1: a run never sends two pages closer than the gap; the
    // job does not sleep, the run is not ready until then.
    let h = harness().await;
    mount_comp_feed(&h.server, 100, None).await;
    start_comp_backfill_run(&h.st).await;
    assert!(drain(&h).await);
    assert_eq!(comp_starts(&h).await, ["0"], "one page, then it waits");

    assert!(lock(&h.st.live).try_start_worker().is_none(), "no work yet");
    advance(&h, COMP_BACKFILL_PAGE_GAP - Duration::from_millis(100));
    assert!(!lock(&h.st.live).comp_backfill_resume_due());
    assert!(lock(&h.st.live).try_start_worker().is_none());

    advance(&h, Duration::from_millis(200));
    assert!(
        lock(&h.st.live).comp_backfill_resume_due(),
        "the tick wakes it"
    );
    assert!(drain(&h).await);
    assert_eq!(comp_starts(&h).await, ["0", "20"]);

    // In a match the wake-up is no wake-up.
    advance(&h, COMP_BACKFILL_PAGE_GAP);
    lock(&h.st.live).in_match = true;
    assert!(!lock(&h.st.live).comp_backfill_resume_due());
}

/// ME's own rank sync as the worker runs it, against the comp feed: stores the
/// newest five rows (`comp_updates(0..5)`).
async fn own_rank_sync(h: &Harness) {
    crate::sync::rank_step(&h.st, &h.session, ME, true)
        .await
        .unwrap();
}

#[tokio::test]
async fn the_first_page_is_skipped_when_the_rank_syncs_rows_hold_the_anchor() {
    let h = harness().await;
    mount_comp_feed(&h.server, 100, None).await;
    // The last finished run saw row 3 as its newest; the rank sync's five
    // newest rows (0..5) hold it.
    store_comp_anchor_for(&h.st, ME, 3).await;
    own_rank_sync(&h).await;
    assert_eq!(comp_starts(&h).await, ["0"], "the rank sync's call");

    run_comp_backfill(&h).await;

    assert_eq!(comp_starts(&h).await, ["0"], "no page for the run");
    assert_eq!(stored_rows(&h.st).await, 5, "the rank sync's rows");
    assert_eq!(
        stored_comp_anchor(&h.st).await.map(|a| a.0),
        Some(row_id(0)),
        "the anchor moved to the newest row"
    );
}

#[tokio::test]
async fn the_first_page_is_asked_for_when_the_rank_syncs_rows_miss_the_anchor() {
    let h = harness().await;
    mount_comp_feed(&h.server, 100, None).await;
    // Row 25 is behind the rank sync's five rows: pages are needed.
    store_comp_anchor_for(&h.st, ME, 25).await;
    own_rank_sync(&h).await;

    run_comp_backfill(&h).await;

    assert_eq!(
        comp_starts(&h).await,
        ["0", "0", "20"],
        "rank sync, then the pages"
    );
    assert_eq!(stored_rows(&h.st).await, 40);
}

#[tokio::test]
async fn another_accounts_head_or_no_anchor_never_skips_the_first_page() {
    // A head of another account holding the same id is not ME's.
    let h = harness().await;
    mount_comp_feed(&h.server, 30, None).await;
    store_comp_anchor_for(&h.st, ME, 3).await;
    lock(&h.st.live).own_comp_head = Some((
        "another-account".into(),
        vec![(row_id(3), entry_start(base_ms(), 3))],
    ));
    run_comp_backfill(&h).await;
    assert_eq!(comp_starts(&h).await, ["0"], "page 0 was asked for");

    // No anchor (a first run): the head is no reason to skip.
    let h = harness().await;
    mount_comp_feed(&h.server, 30, None).await;
    own_rank_sync(&h).await;
    run_comp_backfill(&h).await;
    assert_eq!(comp_starts(&h).await, ["0", "0", "20"]);
}
