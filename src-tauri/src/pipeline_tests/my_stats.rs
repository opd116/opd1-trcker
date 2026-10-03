//! B2's pipeline tests: the `"mystats"` push. A child of `pipeline_tests`, so
//! it shares that module's harness (mock Riot, in-memory DB, a recording event
//! sink) and helpers through `super::*`.
//!
//! What was emitted is read from the recorder; whether a change *asked* for a
//! recompute is read from `MyStatsPush::computed` (a recompute whose result
//! did not change emits nothing, so the events alone cannot tell).

use opd1_core::model::MyStats;

use super::backfill::{run_backfill, Feed};
use super::comp_backfill::mount_comp_feed;
use super::rank::drain;
use super::*;
use crate::my_stats::{self, idle};
use crate::sync::{matches_step, rank_step, start_comp_backfill_run};

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
    start_comp_backfill_run(&h.st).await;
    drain(&h).await;
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
