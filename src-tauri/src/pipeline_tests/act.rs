//! R162's tests: the current act comes from Riot's content-service, with
//! valorant-api's dates as the fallback. A child of `pipeline_tests`, so it
//! shares that module's harness (mock Riot) and helpers through `super::*`.
//!
//! The mock content-service bodies are invented (there is no real dump on
//! disk): the shape is Scout's parser's (`Seasons` with `ID`, `Type`,
//! `IsActive`, `StartTime`, `EndTime`).

use opd1_riot::endpoints::{ContentAct, ContentActs};

use super::*;

const H: i64 = 3_600_000;
const D: i64 = 24 * H;

fn iso(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .unwrap()
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn api_act(id: &str, name: &str, start: i64, end: i64) -> Act {
    Act {
        id: id.into(),
        name: name.into(),
        episode_name: "V26".into(),
        start_ms: start,
        end_ms: end,
    }
}

fn season(id: &str, name: &str, start: i64, end: i64, active: bool) -> Value {
    json!({"ID": id, "Type": "act", "Name": name, "IsActive": active,
           "StartTime": iso(start), "EndTime": iso(end)})
}

async fn mount_content(h: &Harness, status: u16, body: Value) {
    Mock::given(method("GET"))
        .and(path("/content-service/v3/content"))
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .mount(&h.server)
        .await;
}

fn current_id(st: &AppState) -> Option<String> {
    st.current_act().map(|a| a.id)
}

fn log_lines(marker: &str) -> Vec<String> {
    crate::logbuf::snapshot()
        .lines()
        .filter(|l| l.contains(marker))
        .map(str::to_string)
        .collect()
}

#[tokio::test]
async fn riots_answer_decides_the_current_act_when_valorant_api_still_has_the_old_one() {
    crate::logbuf::init();
    let h = harness().await;
    let n = now_ms();
    // valorant-api: act-old runs until tomorrow; Riot flipped two hours ago.
    h.st.set_acts_for_test(vec![
        api_act("act-old-a", "ACT IV", n - 60 * D, n + D),
        api_act("act-new-a", "ACT V", n + D, n + 50 * D),
    ]);
    assert_eq!(current_id(&h.st).as_deref(), Some("act-old-a"));
    mount_content(
        &h,
        200,
        json!({"Seasons": [
            season("act-old-a", "ACT IV", n - 60 * D, n - 2 * H, false),
            season("act-new-a", "ACT V", n - 2 * H, n + 50 * D, true),
        ]}),
    )
    .await;
    h.st.set_session(Some(h.session.clone()));

    h.st.refresh_content().await;

    assert_eq!(current_id(&h.st).as_deref(), Some("act-new-a"));
    let acts = h.st.acts();
    let ids: Vec<_> = acts.iter().map(|a| a.id.as_str()).collect();
    assert_eq!(
        ids,
        ["act-old-a", "act-new-a"],
        "previous act is second to last"
    );
    assert_eq!(acts[0].end_ms, acts[1].start_ms);
    assert_eq!(
        count(&h.server, "GET", "/content-service/v3/content").await,
        1
    );
    let logged = log_lines("act-new-a");
    assert!(
        logged
            .iter()
            .any(|l| l.contains("from riot content-service")),
        "{logged:?}"
    );
}

#[tokio::test]
async fn riots_answer_wins_when_valorant_api_is_ahead() {
    let h = harness().await;
    let n = now_ms();
    // valorant-api already flipped; Riot has not (its act runs 3 more hours).
    h.st.set_acts_for_test(vec![
        api_act("act-old-b", "ACT IV", n - 60 * D, n - H),
        api_act("act-new-b", "ACT V", n - H, n + 50 * D),
    ]);
    assert_eq!(current_id(&h.st).as_deref(), Some("act-new-b"));
    mount_content(
        &h,
        200,
        json!({"Seasons": [season("act-old-b", "ACT IV", n - 60 * D, n + 3 * H, true)]}),
    )
    .await;
    h.st.set_session(Some(h.session.clone()));

    h.st.refresh_content().await;

    assert_eq!(current_id(&h.st).as_deref(), Some("act-old-b"));
    // The act valorant-api dated later is not in the list at all.
    assert!(h.st.acts().iter().all(|a| a.id != "act-new-b"));
}

#[tokio::test]
async fn a_failed_content_call_keeps_the_date_rule_and_logs_one_line_with_the_reason() {
    crate::logbuf::init();
    let h = harness().await;
    let n = now_ms();
    h.st.set_acts_for_test(vec![api_act("act-date-c", "ACT V", n - D, n + D)]);
    mount_content(&h, 503, json!({"errorCode": "SERVICE_DOWN_C"})).await;
    h.st.set_session(Some(h.session.clone()));

    h.st.refresh_content().await;

    assert_eq!(current_id(&h.st).as_deref(), Some("act-date-c"));
    let lines = log_lines("act-date-c");
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].contains("503"), "{}", lines[0]);
    assert!(lines[0].contains("from valorant-api"), "{}", lines[0]);
}

#[tokio::test]
async fn an_ask_met_by_a_carried_hold_is_asked_again_right_after_it_not_after_the_failure_retry() {
    // R185: a hold carried into this session (the old one met a 429 on the
    // `other` family) refuses the ask with nothing sent. That is "not now":
    // logged as held (info), due again just after the hold. A real failure
    // keeps its 5-minute retry.
    crate::logbuf::init();
    let h = harness().await;
    let n = now_ms();
    h.st.set_acts_for_test(vec![api_act("act-date-held", "ACT V", n - D, n + D)]);
    mount_content(&h, 200, json!({"Seasons": []})).await;
    h.session.seed_holds(None, Some(Duration::from_secs(60)));
    h.st.set_session(Some(h.session.clone()));
    let t0 = std::time::Instant::now();

    h.st.refresh_content().await;

    assert_eq!(
        count(&h.server, "GET", "/content-service/v3/content").await,
        0,
        "refused locally, nothing sent"
    );
    assert_eq!(current_id(&h.st).as_deref(), Some("act-date-held"));
    let lines = log_lines("act-date-held");
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].contains("on hold"), "{}", lines[0]);
    assert!(
        lines[0].contains(" INFO ") && !lines[0].contains(" WARN "),
        "held is not a failure: {}",
        lines[0]
    );
    let due = |after: u64| h.st.content_due_at(n, t0 + Duration::from_secs(after));
    assert!(!due(30), "the hold (60 s) is still on");
    // Margins of about 10 s each way: a stalled runner between seeding the hold and the ask cannot flip them.
    assert!(!due(50), "still held near its end");
    assert!(due(70), "due right after the hold, not after the 5-minute failure retry");
    assert!(!due(0), "not at once");

    // The control: the same ask failing for real waits the 5 minutes.
    h.session.end_holds();
    Mock::given(method("GET"))
        .and(path("/content-service/v3/content"))
        .respond_with(ResponseTemplate::new(503).set_body_json(json!({"errorCode": "DOWN"})))
        .mount(&h.server)
        .await;
    let t1 = std::time::Instant::now();
    h.st.refresh_content().await;
    assert_eq!(
        count(&h.server, "GET", "/content-service/v3/content").await,
        1
    );
    assert!(
        !h.st.content_due_at(n, t1 + Duration::from_secs(70)),
        "a failure is retried after 5 minutes, and the old hold's wait is gone"
    );
    assert!(h.st.content_due_at(n, t1 + Duration::from_secs(310)));
}

#[tokio::test]
async fn an_answer_with_no_active_act_keeps_the_date_rule() {
    let h = harness().await;
    let n = now_ms();
    h.st.set_acts_for_test(vec![api_act("act-date-d", "ACT V", n - D, n + D)]);
    mount_content(&h, 200, json!({"Seasons": [], "Odd": [1]})).await;
    h.st.set_session(Some(h.session.clone()));
    h.st.refresh_content().await;
    assert_eq!(current_id(&h.st).as_deref(), Some("act-date-d"));
}

fn riot_new_act(n: i64, end: i64) -> ContentActs {
    ContentActs {
        current: ContentAct {
            id: "act-new-e".into(),
            name: "ACT V".into(),
            episode_name: "V26".into(),
            start_ms: Some(n - H),
            end_ms: Some(end),
        },
        previous: None,
    }
}

async fn riot_new_act_world(h: &Harness, n: i64, end: i64) {
    h.st.set_acts_for_test(vec![
        api_act("act-old-e", "ACT IV", n - 60 * D, n + D),
        api_act("act-new-e", "ACT V", n + D, n + 50 * D),
    ]);
    h.st.set_riot_acts_for_test(Some(riot_new_act(n, end)), n);
    assert_eq!(current_id(&h.st).as_deref(), Some("act-new-e"));
    mount_content(h, 500, json!({})).await;
    h.st.set_session(Some(h.session.clone()));
}

#[tokio::test]
async fn a_later_failure_keeps_riots_answer_while_its_act_has_not_ended() {
    // A blip at a reconnect must not swap the act for valorant-api's dates.
    let h = harness().await;
    let n = now_ms();
    riot_new_act_world(&h, n, n + 50 * D).await;
    h.st.refresh_content().await;
    assert_eq!(current_id(&h.st).as_deref(), Some("act-new-e"));
}

/// B2: the log header names where the current act came from.
#[tokio::test]
async fn the_log_header_names_riot_or_valorant_api_as_the_acts_source() {
    let h = harness().await;
    let n = now_ms();
    let header = |h: &Harness| {
        crate::commands::log_text(&h.st)
            .lines()
            .next()
            .unwrap()
            .to_string()
    };
    h.st.set_acts_for_test(vec![
        api_act("act-old-h", "ACT IV", n - 60 * D, n + D),
        api_act("act-new-h", "ACT V", n + D, n + 50 * D),
    ]);
    assert_eq!(h.st.act_source(), "valorant-api");
    assert!(
        header(&h).ends_with(" · act act-old- V26 A4 (valorant-api)"),
        "{}",
        header(&h)
    );

    // Riot says the next act already runs: it is the current one, from Riot.
    let mut riot = riot_new_act(n, n + 50 * D);
    riot.current.id = "act-new-h".into();
    h.st.set_riot_acts_for_test(Some(riot), n);
    assert_eq!(h.st.act_source(), "riot");
    assert!(
        header(&h).ends_with(" · act act-new- V26 A5 (riot)"),
        "{}",
        header(&h)
    );

    // Riot's answer withdrawn: back to valorant-api's dates.
    h.st.set_riot_acts_for_test(None, n);
    assert_eq!(h.st.act_source(), "valorant-api");
}

#[tokio::test]
async fn a_failure_after_riots_act_ended_falls_back_to_the_date_rule() {
    let h = harness().await;
    let n = now_ms();
    riot_new_act_world(&h, n, n + 50 * D).await;
    // The same answer, but its act ended and the answer was fetched long ago.
    h.st.set_riot_acts_for_test(Some(riot_new_act(n - 2 * D, n - D)), n - 2 * D);
    h.st.refresh_content().await;
    assert_eq!(current_id(&h.st).as_deref(), Some("act-old-e"));
}

#[tokio::test]
async fn the_true_start_is_what_date_readers_get_even_in_the_first_hour() {
    let h = harness().await;
    let n = now_ms();
    // Riot's act began just after the floored hour: the card list says it began at the
    // floored hour so cards switch now, but readers of dates want the truth.
    let start = crate::state::card_clock(n) + 1;
    h.st.set_acts_for_test(vec![api_act("act-old-g", "ACT IV", n - 60 * D, n + D)]);
    h.st.set_riot_acts_for_test(
        Some(ContentActs {
            current: ContentAct {
                id: "act-new-g".into(),
                name: "ACT V".into(),
                episode_name: "V26".into(),
                start_ms: Some(start),
                end_ms: Some(n + 50 * D),
            },
            previous: Some(ContentAct {
                id: "act-old-g".into(),
                name: "ACT IV".into(),
                episode_name: "V26".into(),
                start_ms: Some(n - 60 * D),
                end_ms: Some(start),
            }),
        }),
        n,
    );
    let cur = h.st.current_act().unwrap();
    assert_eq!(cur.id, "act-new-g");
    assert_eq!(cur.start_ms, start, "the exact start, not the clock floor");
    let listed = h.st.acts();
    assert!(listed.last().unwrap().start_ms <= crate::state::card_clock(n));
}

#[tokio::test]
async fn the_act_crosses_its_end_without_a_restart() {
    let h = harness().await;
    let n = now_ms();
    // Riot: act A until n+1h. valorant-api says B began 30 minutes ago.
    h.st.set_acts_for_test(vec![
        api_act("act-a-f", "ACT IV", n - 60 * D, n - 30 * 60_000),
        api_act("act-b-f", "ACT V", n - 30 * 60_000, n + 50 * D),
    ]);
    h.st.set_riot_acts_for_test(
        Some(ContentActs {
            current: ContentAct {
                id: "act-a-f".into(),
                name: "ACT IV".into(),
                episode_name: "V26".into(),
                start_ms: Some(n - 60 * D),
                end_ms: Some(n + H),
            },
            previous: None,
        }),
        n,
    );
    let at = |t: i64| {
        let acts = h.st.acts_at(t);
        crate::roster::current_act(&acts, t).map(|a| a.id.clone())
    };
    assert_eq!(at(n).as_deref(), Some("act-a-f"), "Riot while it is fresh");
    assert_eq!(
        at(n + H + 1).as_deref(),
        Some("act-b-f"),
        "past Riot's end the date rule answers until Riot is asked again"
    );
    assert!(!h.st.content_due(n + H - 1), "fresh: nothing to ask");
    assert!(h.st.content_due(n + H + 1), "ended: ask again");
}

#[tokio::test]
async fn content_is_asked_before_any_answer_but_backs_off_after_one() {
    let h = harness().await;
    let n = now_ms();
    assert!(h.st.content_due(n), "never asked");
    mount_content(&h, 500, json!({})).await;
    h.st.set_session(Some(h.session.clone()));
    h.st.refresh_content().await;
    assert!(!h.st.content_due(n), "a failure waits before the next ask");
    assert!(!h.st.content_due(n + 10 * D));
    // Not connected: nothing is spawned (and nothing panics).
    h.st.set_session(None);
    h.st.spawn_content_refresh(true);
    assert_eq!(
        count(&h.server, "GET", "/content-service/v3/content").await,
        1
    );
}

/// Finding 10 of the backend review: after a patch (a new version, which is
/// also when a new act starts) one table failing must not leave the app
/// without act tables when a disk copy exists.
#[tokio::test]
async fn a_failed_table_download_after_a_patch_keeps_the_old_acts_and_asks_again() {
    let h = harness().await;
    mount_asset_tables(&h.assets_server).await;
    h.st.ensure_tables(false).await.expect("tables load");
    let before = h.st.acts().len();
    assert!(before > 0);
    assert!(!h.st.assets.tables_stale());

    // Patched: a new version, the seasons table down (priority 1 beats the
    // v1 mocks above).
    Mock::given(method("GET"))
        .and(path("/v1/version"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"status": 200, "data": {"version": "test-2"}})),
        )
        .with_priority(1)
        .mount(&h.assets_server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/seasons"))
        .respond_with(ResponseTemplate::new(500))
        .with_priority(1)
        .mount(&h.assets_server)
        .await;
    // A fresh process after the patch: nothing in memory, the v1 copy on disk.
    let fresh = AssetCache::with_base(h._dir.path().join("assets"), h.assets_server.uri());
    assert!(!fresh.refresh_tables().await.unwrap());
    assert!(fresh.tables_stale());
    assert_eq!(fresh.acts().unwrap().len(), before, "the old acts load");

    // First launch after the patch, through the app's own gate: tables load
    // (the old ones), and the retry is owed.
    let st = AppState::new(
        Box::new(Sink(Arc::new(Recorder::default()))),
        Repo::new(open_db(Path::new(":memory:")).unwrap()),
        AssetCache::with_base(h._dir.path().join("assets"), h.assets_server.uri()),
        h._dir.path().join("debug"),
        Some(ME.into()),
    );
    crate::logbuf::init();
    let stale_lines = || log_lines("asset table fetch failed").len();
    let said = stale_lines();
    st.ensure_tables(false).await.expect("the old tables load");
    assert_eq!(st.acts().len(), before);
    assert!(st.assets.tables_stale());
    // The retry (a connect bypasses the 30 s guard) fails the same way: the
    // stale-table line is not said again.
    st.ensure_tables(true).await.expect("still the old tables");
    st.ensure_tables(true).await.expect("still the old tables");
    assert_eq!(stale_lines(), said + 1, "once per stale period");

    // Healed: a full download of the new version clears the stale mark.
    h.assets_server.reset().await;
    mount_asset_tables(&h.assets_server).await;
    Mock::given(method("GET"))
        .and(path("/v1/version"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"status": 200, "data": {"version": "test-2"}})),
        )
        .with_priority(1)
        .mount(&h.assets_server)
        .await;
    st.ensure_tables(true).await.expect("healed");
    assert!(!st.assets.tables_stale());
    assert_eq!(st.acts().len(), before);
}

#[tokio::test]
async fn an_exact_start_ahead_of_our_clock_is_never_after_now() {
    let h = harness().await;
    let n = now_ms();
    // Riot's clock is 5 minutes ahead: its act "starts" in our future.
    h.st.set_acts_for_test(vec![api_act("act-old-h", "ACT IV", n - 60 * D, n + D)]);
    h.st.set_riot_acts_for_test(
        Some(ContentActs {
            current: ContentAct {
                id: "act-new-h".into(),
                name: "ACT V".into(),
                episode_name: "V26".into(),
                start_ms: Some(n + 5 * 60_000),
                end_ms: Some(n + 50 * D),
            },
            previous: None,
        }),
        n,
    );
    let cur = h.st.current_act().expect("the act is current");
    assert_eq!(cur.id, "act-new-h");
    assert!(cur.start_ms <= now_ms(), "start {} > now", cur.start_ms);
}
