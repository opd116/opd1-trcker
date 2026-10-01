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
async fn an_answer_with_no_active_act_keeps_the_date_rule() {
    let h = harness().await;
    let n = now_ms();
    h.st.set_acts_for_test(vec![api_act("act-date-d", "ACT V", n - D, n + D)]);
    mount_content(&h, 200, json!({"Seasons": [], "Odd": [1]})).await;
    h.st.set_session(Some(h.session.clone()));
    h.st.refresh_content().await;
    assert_eq!(current_id(&h.st).as_deref(), Some("act-date-d"));
}

#[tokio::test]
async fn a_later_failure_drops_riots_old_answer() {
    let h = harness().await;
    let n = now_ms();
    h.st.set_acts_for_test(vec![
        api_act("act-old-e", "ACT IV", n - 60 * D, n + D),
        api_act("act-new-e", "ACT V", n + D, n + 50 * D),
    ]);
    h.st.set_riot_acts_for_test(
        Some(ContentActs {
            current: ContentAct {
                id: "act-new-e".into(),
                name: "ACT V".into(),
                episode_name: "V26".into(),
                start_ms: Some(n - H),
                end_ms: Some(n + 50 * D),
            },
            previous: None,
        }),
        n,
    );
    assert_eq!(current_id(&h.st).as_deref(), Some("act-new-e"));
    mount_content(&h, 500, json!({})).await;
    h.st.set_session(Some(h.session.clone()));
    h.st.refresh_content().await;
    assert_eq!(current_id(&h.st).as_deref(), Some("act-old-e"));
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
