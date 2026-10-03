//! B2's pipeline tests: the `"mystats"` push. A child of `pipeline_tests`, so
//! it shares that module's harness (mock Riot, in-memory DB, a recording event
//! sink) and helpers through `super::*`.
//!
//! What was emitted is read from the recorder; whether a change *asked* for a
//! recompute is read from `MyStatsPush::computed` (a recompute whose result
//! did not change emits nothing, so the events alone cannot tell).

use opd1_core::model::MyStats;

use super::backfill::{mount_feed, run_backfill, Feed};
use super::comp_backfill::{mount_comp_feed, run_comp_backfill};
use super::names_fill::DbHold;
use super::*;
use crate::my_stats::{self, idle};
use crate::sync::{matches_step, rank_step};

fn pushed(h: &Harness) -> Vec<MyStats> {
    h.rec.my_stats.lock().unwrap().clone()
}

/// How many times My Stats was computed for a push.
fn computed(st: &AppState) -> usize {
    lock(&st.my_stats_push).computed
}

fn json_of(stats: &MyStats) -> Value {
    serde_json::to_value(stats).unwrap()
}

fn the_act(id: &str, start_ms: i64, end_ms: i64) -> Act {
    Act {
        id: id.into(),
        name: "ACT V".into(),
        episode_name: "V26".into(),
        start_ms,
        end_ms,
    }
}

#[tokio::test]
async fn nothing_is_pushed_before_the_user_and_the_act_are_known() {
    let h = harness().await;

    // No user yet (a first start, never connected).
    lock(&h.st.live).self_puuid = None;
    my_stats::notify(&h.st);
    idle(&h.st).await;
    assert!(pushed(&h).is_empty(), "no puuid");

    // The user is known, the asset tables are not (no act).
    lock(&h.st.live).self_puuid = Some(ME.into());
    h.st.set_acts_for_test(Vec::new());
    my_stats::notify(&h.st);
    idle(&h.st).await;
    assert!(pushed(&h).is_empty(), "no act");

    // Both known: the first push.
    let n = now_ms();
    h.st.set_acts_for_test(vec![the_act(ACT, n - 86_400_000, n + 86_400_000)]);
    my_stats::notify(&h.st);
    idle(&h.st).await;
    assert_eq!(pushed(&h).len(), 1, "once it became computable");
}

#[tokio::test]
async fn the_same_data_is_pushed_once() {
    let h = harness().await;
    for _ in 0..3 {
        my_stats::notify(&h.st);
        idle(&h.st).await;
    }
    assert_eq!(pushed(&h).len(), 1, "same bytes: one emission");
    assert!(computed(&h.st) >= 3, "each notification recomputed");
}

#[tokio::test]
async fn the_push_is_what_the_command_returns() {
    let h = harness().await;
    rank_step(&h.st, &h.session, ME, true).await.unwrap();
    idle(&h.st).await;
    let command = crate::commands::build_my_stats(&h.st)
        .await
        .unwrap()
        .expect("computable");
    let events = pushed(&h);
    assert_eq!(json_of(events.last().expect("pushed")), json_of(&command));
    // The camel-case wire shape the UI reads.
    assert!(json_of(&command).get("rrSeries").is_some());
}

/// ME's newest competitive match `id`, played an hour ago, inside the
/// harness's act: the competitive updates, the history and the details all
/// say so (ahead of the harness's own 1970 matches).
async fn mount_own_match(h: &Harness, id: &str, start_ms: i64) {
    let on = |p: String, body: Value| {
        Mock::given(method("GET"))
            .and(path(p))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .with_priority(1)
    };
    on(
        format!("/mmr/v1/players/{ME}/competitiveupdates"),
        comp_updates(&[(id, start_ms, 21)]),
    )
    .mount(&h.server)
    .await;
    on(
        format!("/match-history/v1/history/{ME}"),
        json!({"Subject": ME, "History": [
            {"MatchID": id, "GameStartTime": start_ms, "QueueID": "competitive"}
        ]}),
    )
    .mount(&h.server)
    .await;
    on(
        format!("/match-details/v1/matches/{id}"),
        details(id, "competitive", start_ms),
    )
    .mount(&h.server)
    .await;
}

#[tokio::test]
async fn the_users_own_rank_and_matches_phases_push_and_a_phase_with_nothing_new_does_not() {
    let h = harness().await;
    mount_own_match(&h, "mx", now_ms() - 3_600_000).await;
    assert!(pushed(&h).is_empty());

    // The rank phase stores comp updates and the MMR snapshot.
    rank_step(&h.st, &h.session, ME, true).await.unwrap();
    idle(&h.st).await;
    let after_rank = pushed(&h);
    assert_eq!(after_rank.len(), 1);
    assert_eq!(after_rank[0].rank.tier, Some(15), "the stored snapshot");

    // The matches phase stores m1 (competitive) and m2: new matches.
    matches_step(&h.st, &h.session, ME, true).await.unwrap();
    idle(&h.st).await;
    let after_matches = pushed(&h);
    assert_eq!(after_matches.len(), 2, "new matches: a new emission");
    assert_ne!(
        json_of(&after_matches[0]),
        json_of(&after_matches[1]),
        "the competitive match shows up"
    );

    // The same phase again: nothing new, so the bytes are the same.
    let before = computed(&h.st);
    matches_step(&h.st, &h.session, ME, false).await.unwrap();
    idle(&h.st).await;
    assert!(computed(&h.st) > before, "it asked for a recompute…");
    assert_eq!(pushed(&h).len(), 2, "…which changed nothing: no event");
}

#[tokio::test]
async fn another_players_sync_asks_for_nothing() {
    let h = harness().await;
    // OTHER's calls fail in the harness: their steps still end and publish.
    rank_step(&h.st, &h.session, OTHER, true).await.unwrap();
    matches_step(&h.st, &h.session, OTHER, true).await.unwrap();
    idle(&h.st).await;
    assert_eq!(computed(&h.st), 0, "no recompute for a stranger");
    assert!(pushed(&h).is_empty());
}

#[tokio::test]
async fn a_competitive_updates_page_pushes() {
    let h = harness().await;
    mount_comp_feed(&h.server, 30, None).await;
    run_comp_backfill(&h).await;
    idle(&h.st).await;

    let events = pushed(&h);
    assert!(!events.is_empty());
    assert_eq!(
        events.last().unwrap().rr_series.len(),
        30,
        "the chart has every row the run stored"
    );
}

#[tokio::test]
async fn a_match_backfill_pass_that_stored_matches_asks_for_a_recompute() {
    let h = harness().await;
    super::backfill::mount_feed(&h.server, Feed::new(5)).await;
    assert_eq!(computed(&h.st), 0);

    run_backfill(&h).await;
    idle(&h.st).await;
    assert!(computed(&h.st) >= 1, "the pass stored matches");

    // A second run: the anchor is the newest entry, nothing new is stored.
    let before = computed(&h.st);
    run_backfill(&h).await;
    idle(&h.st).await;
    assert_eq!(
        computed(&h.st),
        before,
        "a pass that stored nothing is silent"
    );
}

#[tokio::test]
async fn a_change_of_act_pushes() {
    let h = harness().await;
    let n = now_ms();
    const D: i64 = 86_400_000;
    h.st.set_acts_for_test(vec![
        the_act("act-old", n - 60 * D, n + D),
        the_act("act-new", n + D, n + 50 * D),
    ]);
    Mock::given(method("GET"))
        .and(path("/content-service/v3/content"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"Seasons": [
            {"ID": "act-old", "Type": "act", "Name": "ACT IV", "IsActive": false,
             "StartTime": "2020-01-01T00:00:00.000Z", "EndTime": "2020-02-01T00:00:00.000Z"},
            {"ID": "act-new", "Type": "act", "Name": "ACT V", "IsActive": true,
             "StartTime": "2020-02-01T00:00:00.000Z", "EndTime": "2099-01-01T00:00:00.000Z"},
        ]})))
        .mount(&h.server)
        .await;
    h.st.set_session(Some(h.session.clone()));
    assert_eq!(h.st.current_act().map(|a| a.id).as_deref(), Some("act-old"));

    h.st.refresh_content().await;
    idle(&h.st).await;

    assert_eq!(h.st.current_act().map(|a| a.id).as_deref(), Some("act-new"));
    assert!(computed(&h.st) >= 1, "the new act asked for a recompute");
}

#[tokio::test]
async fn the_users_own_details_sync_pushes_and_a_strangers_does_not() {
    let h = harness().await;
    super::rank::open_details_of_me(&h).await;
    assert!(h.st.claim_details_sync(ME));
    crate::commands::details_sync_job(
        h.st.clone(),
        h.session.clone(),
        ME.to_string(),
        Duration::from_millis(30),
    )
    .await;
    idle(&h.st).await;
    assert!(computed(&h.st) >= 1, "the user's own sync");

    let before = computed(&h.st);
    assert!(h.st.claim_details_sync(OTHER));
    crate::commands::details_sync_job(
        h.st.clone(),
        h.session.clone(),
        OTHER.to_string(),
        Duration::from_millis(30),
    )
    .await;
    idle(&h.st).await;
    assert_eq!(computed(&h.st), before, "a stranger's Details sync");
}

/// ACCOUNT_A has one stored competitive RR row, ME none.
const ACCOUNT_A: &str = "11111111-2222-3333-4444-555555555555";

async fn store_rr_row_of_a(h: &Harness) {
    h.st.db(|repo| {
        repo.upsert_comp_updates(
            ACCOUNT_A,
            &[CompUpdateRow {
                match_id: "a-match".into(),
                season_id: ACT.into(),
                tier_after: 12,
                tier_before: Some(12),
                rr_after: 50,
                rr_earned: 18,
                map_id: None,
                start_ms: now_ms() - 3_600_000,
            }],
        )
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn an_account_switch_replaces_the_old_accounts_stats_at_once() {
    let h = harness().await;
    store_rr_row_of_a(&h).await;
    lock(&h.st.live).self_puuid = Some(ACCOUNT_A.into());
    my_stats::notify(&h.st);
    idle(&h.st).await;
    assert_eq!(pushed(&h).len(), 1);
    assert_eq!(pushed(&h)[0].rr_series.len(), 1, "A's row");

    // ME connects: the switch itself pushes, DB only (the asset tables never
    // load in this harness, so nothing else would).
    let mut poller = crate::poll::Poller::new(h.st.clone());
    poller.on_connected(h.session.clone()).await;
    idle(&h.st).await;

    let events = pushed(&h);
    assert_eq!(events.len(), 2, "the new account's stats are out");
    assert!(events[1].rr_series.is_empty(), "ME has no rows: not A's");
    poller.stop_worker();
}

#[tokio::test]
async fn a_read_for_the_old_account_that_ends_after_a_switch_is_never_emitted() {
    let h = harness().await;
    store_rr_row_of_a(&h).await;
    lock(&h.st.live).self_puuid = Some(ACCOUNT_A.into());

    // A's read is under way (it waits for the DB) when the account changes.
    let hold = DbHold::take(&h.st);
    my_stats::notify(&h.st);
    wait_until("the read started", || computed(&h.st) == 1).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    lock(&h.st.live).self_puuid = Some(ME.into());
    my_stats::notify(&h.st);
    hold.release();
    idle(&h.st).await;

    let events = pushed(&h);
    assert_eq!(events.len(), 1, "only the new account's");
    assert!(events[0].rr_series.is_empty(), "never A's row");
}

#[tokio::test]
async fn a_push_task_that_dies_does_not_stop_later_pushes() {
    let h = harness().await;
    h.rec
        .panic_on_my_stats
        .store(true, std::sync::atomic::Ordering::SeqCst);
    my_stats::notify(&h.st);
    // The task panicked in the sink: its guard cleared `running`.
    idle(&h.st).await;
    assert!(pushed(&h).is_empty());

    // The next notification starts a new task, and the bytes the dead one
    // never delivered are emitted.
    my_stats::notify(&h.st);
    idle(&h.st).await;
    assert_eq!(pushed(&h).len(), 1);
}

#[tokio::test]
async fn a_match_backfill_aborted_after_it_stored_a_match_still_pushes() {
    // Review round 1: the notification comes when the match is stored, not
    // when the pass returns — the worker's abort ends a pass mid-await.
    let h = harness().await;
    let mut feed = Feed::new(50);
    // Entry 0 answers at once, entry 1 takes a minute: the pass is parked on
    // it, and gets aborted there.
    feed.details_slow.insert(1, Duration::from_secs(60));
    mount_feed(&h.server, feed).await;
    crate::sync::start_backfill_run(&h.st).await;
    assert_eq!(computed(&h.st), 0);

    let step = tokio::spawn({
        let (st, session) = (h.st.clone(), h.session.clone());
        async move { crate::sync::backfill_step(&st, &session).await }
    });
    wait_until("the first match was stored and My Stats asked", || {
        computed(&h.st) >= 1
    })
    .await;
    step.abort();
    assert!(step.await.unwrap_err().is_cancelled(), "it never returned");
    idle(&h.st).await;
    assert!(computed(&h.st) >= 1);
}

#[tokio::test]
async fn notifications_inside_the_trailing_wait_are_one_read() {
    // Review round 2: a backfill that stores many matches notifies once per
    // match; they must not become one payload (one read) each.
    let h = harness().await;
    my_stats::set_delay(&h.st, Duration::from_millis(400));
    for _ in 0..8 {
        my_stats::notify(&h.st);
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(computed(&h.st), 0, "nothing is read during the wait");
    idle(&h.st).await;
    assert_eq!(computed(&h.st), 1, "eight notifications, one read");
    assert_eq!(pushed(&h).len(), 1);

    // After the read a new change is a new wait and a new read.
    my_stats::notify(&h.st);
    idle(&h.st).await;
    assert_eq!(computed(&h.st), 2);
}

#[tokio::test]
async fn a_change_that_lands_after_the_wait_began_is_in_the_read() {
    // The wait is before the read, and `dirty` is cleared when the read
    // starts: a notification during the wait does not cost a second read.
    let h = harness().await;
    my_stats::set_delay(&h.st, Duration::from_millis(300));
    my_stats::notify(&h.st);
    tokio::time::sleep(Duration::from_millis(150)).await;
    my_stats::notify(&h.st);
    idle(&h.st).await;
    assert_eq!(computed(&h.st), 1);
}

#[tokio::test]
async fn an_account_switch_does_not_wait() {
    let h = harness().await;
    // A wait that would fail the test if anything waited for it.
    my_stats::set_delay(&h.st, Duration::from_secs(120));
    let quick = Duration::from_secs(10);

    // A fresh task.
    my_stats::notify_now(&h.st);
    tokio::time::timeout(quick, idle(&h.st))
        .await
        .expect("a fresh task read at once");
    assert_eq!(computed(&h.st), 1);

    // A task already in its wait is woken by it.
    my_stats::notify(&h.st);
    wait_until("the task is waiting", || lock(&h.st.my_stats_push).waiting).await;
    my_stats::notify_now(&h.st);
    tokio::time::timeout(quick, idle(&h.st))
        .await
        .expect("the waiting task was woken");
    assert_eq!(computed(&h.st), 2);
}

#[tokio::test]
async fn the_slow_tick_recomputes_and_the_unchanged_bytes_are_not_emitted_again() {
    let h = harness().await;
    let tick = tokio::spawn(my_stats::periodic(h.st.clone(), Duration::from_millis(40)));
    // The first tick is the startup's own: the interval's immediate one does
    // not ask. Then one recompute per period.
    wait_until("three periods asked for a recompute", || {
        computed(&h.st) >= 3
    })
    .await;
    tick.abort();
    idle(&h.st).await;
    assert_eq!(pushed(&h).len(), 1, "same bytes: one emission");
    assert!(computed(&h.st) >= 3);
}

#[tokio::test]
async fn the_slow_tick_picks_up_a_change_nothing_notified() {
    // The reason for the tick: what the DB says changes with the clock or
    // with a write nobody announced.
    let h = harness().await;
    my_stats::notify(&h.st);
    idle(&h.st).await;
    assert_eq!(pushed(&h).len(), 1);
    assert!(pushed(&h)[0].rr_series.is_empty());

    store_rr_row_of_a(&h).await;
    lock(&h.st.live).self_puuid = Some(ACCOUNT_A.into());
    let tick = tokio::spawn(my_stats::periodic(h.st.clone(), Duration::from_millis(40)));
    wait_until("the tick's recompute was emitted", || pushed(&h).len() >= 2).await;
    tick.abort();
    assert_eq!(pushed(&h).last().unwrap().rr_series.len(), 1);
}
