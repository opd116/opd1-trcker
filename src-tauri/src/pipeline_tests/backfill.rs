//! Task 15b's pipeline tests: the user's own match-history backfill (R101)
//! against the mock Riot client. A child of `pipeline_tests`, so it shares
//! that module's harness (mock Riot, mock valorant-api, in-memory DB) and
//! helpers through `super::*`.
//!
//! The mock's pd side serves ME's `match-history` from a list of synthetic
//! entries (newest first, one minute apart, honouring `startIndex` /
//! `endIndex`, with Riot's `Total` — see [`TotalMode`] and `page_cap` for the
//! short-page and missing-`Total` cases), and answers `match-details` with a copy of
//! `crates/riot/tests/fixtures/match_details.json` carrying the requested id.
//! Requests are counted per path (`count`) or read in arrival order.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use wiremock::matchers::path_regex;

use super::*;
use crate::sync::{
    backfill_step, parse_anchor, start_backfill_run, Gone, BACKFILL_ANCHOR_KEY, BACKFILL_PAGE,
};

const DETAILS_FIXTURE: &str =
    include_str!("../../../crates/riot/tests/fixtures/match_details.json");

/// Entry `i` of the synthetic history.
pub(crate) fn entry_id(i: usize) -> String {
    format!("bf{i:03}")
}

pub(crate) fn entry_start(base: i64, i: usize) -> i64 {
    base - i as i64 * 60_000
}

/// `entry_start` of entry 0 in every test: an hour before the first call,
/// fixed for the whole test process (a test builds its expectations and its
/// mock from separate calls), and inside the harness's 30-day act.
pub(crate) fn base_ms() -> i64 {
    static BASE: std::sync::OnceLock<i64> = std::sync::OnceLock::new();
    *BASE.get_or_init(|| now_ms() - 3_600_000)
}

/// What the mock's `match-history` says for `Total`.
#[derive(Clone, Copy)]
pub(crate) enum TotalMode {
    /// The whole synthetic history, as Riot does (the fixture shows
    /// `"Total": 214` next to `"EndIndex": 2`).
    Entries,
    /// No `Total` field at all.
    Absent,
    /// A fixed claim, right or wrong.
    Fixed(usize),
}

/// What the mock serves. `on_details` runs with the 1-based count of
/// match-details requests the mock has answered, inside the responder.
pub(crate) struct Feed {
    entries: usize,
    base: i64,
    pub(crate) details_delay: Option<Duration>,
    on_details: Option<Arc<dyn Fn(usize) + Send + Sync>>,
    /// History pages starting at or after this index answer 500.
    history_fails_from: Option<usize>,
    /// Entry index -> the status its match-details answers instead of 200.
    details_status: HashMap<usize, u16>,
    /// A history page never holds more than this many entries, however many
    /// were asked for (a server-side page cap or clamping).
    pub(crate) page_cap: Option<usize>,
    pub(crate) total: TotalMode,
}

impl Feed {
    pub(crate) fn new(entries: usize) -> Self {
        Self {
            entries,
            base: base_ms(),
            details_delay: None,
            on_details: None,
            history_fails_from: None,
            details_status: HashMap::new(),
            page_cap: None,
            total: TotalMode::Entries,
        }
    }
}

/// Mounts ME's history and every `bf###` match's details, beating the
/// harness's static mocks (`with_priority(1)`).
pub(crate) async fn mount_feed(server: &MockServer, feed: Feed) {
    let Feed {
        entries,
        base,
        details_delay,
        on_details,
        history_fails_from,
        details_status,
        page_cap,
        total,
    } = feed;
    Mock::given(method("GET"))
        .and(path(format!("/match-history/v1/history/{ME}")))
        .respond_with(move |req: &Request| {
            let q: HashMap<String, String> = req
                .url
                .query_pairs()
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect();
            let num =
                |k: &str, default: usize| q.get(k).and_then(|v| v.parse().ok()).unwrap_or(default);
            let start = num("startIndex", 0);
            if history_fails_from.is_some_and(|from| start >= from) {
                return ResponseTemplate::new(500);
            }
            let mut end = num("endIndex", start + BACKFILL_PAGE as usize).min(entries);
            if let Some(cap) = page_cap {
                end = end.min(start + cap);
            }
            let history: Vec<Value> = (start..end.max(start))
                .map(|i| {
                    json!({"MatchID": entry_id(i), "GameStartTime": entry_start(base, i),
                           "QueueID": "unrated"})
                })
                .collect();
            let mut body = json!({"Subject": ME, "BeginIndex": start, "EndIndex": end.max(start),
                                  "History": history});
            match total {
                TotalMode::Entries => body["Total"] = json!(entries),
                TotalMode::Fixed(n) => body["Total"] = json!(n),
                TotalMode::Absent => {}
            }
            ResponseTemplate::new(200).set_body_json(body)
        })
        .with_priority(1)
        .mount(server)
        .await;

    let served = Arc::new(AtomicUsize::new(0));
    Mock::given(method("GET"))
        .and(path_regex(r"^/match-details/v1/matches/bf\d+$"))
        .respond_with(move |req: &Request| {
            let id = req.url.path().rsplit('/').next().unwrap().to_string();
            let i: usize = id[2..].parse().unwrap();
            if let Some(status) = details_status.get(&i) {
                return ResponseTemplate::new(*status);
            }
            let mut body: Value = serde_json::from_str(DETAILS_FIXTURE).unwrap();
            body["matchInfo"]["matchID"] = json!(id);
            body["matchInfo"]["gameStartMillis"] = json!(entry_start(base, i));
            if let Some(hook) = &on_details {
                hook(served.fetch_add(1, Ordering::SeqCst) + 1);
            }
            let mut resp = ResponseTemplate::new(200).set_body_json(body);
            if let Some(d) = details_delay {
                resp = resp.set_delay(d);
            }
            resp
        })
        .with_priority(1)
        .mount(server)
        .await;
}

pub(crate) async fn requests(server: &MockServer) -> Vec<Request> {
    server.received_requests().await.unwrap()
}

/// The `startIndex` of every history request, in arrival order.
fn history_starts(reqs: &[Request]) -> Vec<String> {
    reqs.iter()
        .filter(|r| r.url.path() == history_path())
        .map(|r| {
            r.url
                .query_pairs()
                .find(|(k, _)| k == "startIndex")
                .map(|(_, v)| v.into_owned())
                .unwrap()
        })
        .collect()
}

/// The `bf###` ids `match-details` was asked for, in arrival order.
fn details_asked(reqs: &[Request]) -> Vec<String> {
    reqs.iter()
        .filter(|r| r.url.path().starts_with("/match-details/v1/matches/"))
        .map(|r| r.url.path().rsplit('/').next().unwrap().to_string())
        .collect()
}

pub(crate) fn history_path() -> String {
    format!("/match-history/v1/history/{ME}")
}

pub(crate) async fn stored_anchor(st: &AppState) -> Option<(String, i64)> {
    st.db(|repo| repo.get_state(BACKFILL_ANCHOR_KEY))
        .await
        .flatten()
        .and_then(|raw| parse_anchor(&raw))
}

pub(crate) async fn store_anchor(st: &AppState, i: usize, base: i64) {
    let raw = crate::sync::anchor_json(&(entry_id(i), entry_start(base, i)));
    st.db(move |repo| repo.set_state(BACKFILL_ANCHOR_KEY, &raw))
        .await
        .unwrap();
}

/// A match row the DB already has (`new_match_ids` must skip it).
async fn store_match(st: &AppState, id: String, start_ms: i64) {
    let m = Match {
        match_id: id,
        map_id: "/Game/Maps/Ascent/Ascent".into(),
        queue_id: "unrated".into(),
        start_ms,
        winning_team: None,
        is_ranked: false,
        rounds_played: None,
        game_length_ms: None,
    };
    st.db(move |repo| repo.upsert_match(&m, &[], &[]))
        .await
        .unwrap();
}

async fn is_stored(st: &AppState, id: String) -> bool {
    st.db(move |repo| repo.new_match_ids(&[id]))
        .await
        .unwrap()
        .is_empty()
}

/// What the poll loop does on connect: start a run from the stored anchor,
/// claim the worker, and let it drain (it ends when nothing is left).
async fn run_backfill(h: &Harness) {
    start_backfill_run(&h.st).await;
    let id = lock(&h.st.live)
        .try_start_worker()
        .expect("a run is pending work");
    run_worker(h.st.clone(), h.session.clone(), id).await;
    assert!(!lock(&h.st.live).worker_running(), "claim released on exit");
}

pub(crate) fn run_active(st: &AppState) -> bool {
    lock(&st.live).backfill.is_some()
}

#[tokio::test]
async fn backfill_stops_at_first_known_id() {
    let h = harness().await;
    let base = base_ms();
    mount_feed(&h.server, Feed::new(30)).await;
    // The anchor is entry 22 of 30.
    store_anchor(&h.st, 22, base).await;

    run_backfill(&h).await;

    // Two pages (0..20, 20..40), the second ending at the anchor.
    assert_eq!(count(&h.server, "GET", &history_path()).await, 2);
    let reqs = requests(&h.server).await;
    assert_eq!(history_starts(&reqs), ["0", "20"]);
    assert!(
        reqs.iter()
            .filter(|r| r.url.path() == history_path())
            .all(|r| r.url.query_pairs().all(|(k, _)| k != "queue")),
        "all queues are paged, Deathmatch included"
    );

    // Never name-service, for anyone.
    assert!(
        reqs.iter().all(|r| r.method.as_str() == "GET"),
        "the backfill only reads (no name-service PUT)"
    );

    // Match-details for entries 0..22 only, each once.
    let mut asked = details_asked(&reqs);
    asked.sort();
    let expected: Vec<String> = (0..22).map(entry_id).collect();
    assert_eq!(asked, expected);
    assert!(
        !is_stored(&h.st, entry_id(22)).await,
        "the anchor and older are not fetched"
    );

    // The anchor moved to the newest entry; the run is over.
    assert_eq!(
        stored_anchor(&h.st).await,
        Some((entry_id(0), entry_start(base, 0)))
    );
    assert!(!run_active(&h.st));
}

#[tokio::test]
async fn backfill_skips_known_ids() {
    let h = harness().await;
    let base = base_ms();
    mount_feed(&h.server, Feed::new(30)).await;
    store_anchor(&h.st, 22, base).await;
    // 3 of entries 0..22 were stored by the live sync already.
    for i in [0usize, 1, 7] {
        store_match(&h.st, entry_id(i), entry_start(base, i)).await;
    }

    run_backfill(&h).await;

    let reqs = requests(&h.server).await;
    let asked = details_asked(&reqs);
    assert_eq!(asked.len(), 19, "22 entries - 3 known");
    for i in [0usize, 1, 7] {
        assert!(!asked.contains(&entry_id(i)), "{} is known", entry_id(i));
    }
    assert_eq!(
        stored_anchor(&h.st).await,
        Some((entry_id(0), entry_start(base, 0))),
        "the anchor is the newest entry, stored or not"
    );

    // A second run: the anchor is entry 0 — one history page, nothing new.
    let (history_before, details_before) = (
        count(&h.server, "GET", &history_path()).await,
        details_asked(&requests(&h.server).await).len(),
    );
    run_backfill(&h).await;
    assert_eq!(
        count(&h.server, "GET", &history_path()).await - history_before,
        1
    );
    assert_eq!(
        details_asked(&requests(&h.server).await).len() - details_before,
        0
    );
}

#[tokio::test]
async fn backfill_first_run_respects_bound() {
    // No anchor, 230 entries, all inside the act: the 200-entry cap ends it.
    let h = harness().await;
    mount_feed(&h.server, Feed::new(230)).await;
    run_backfill(&h).await;
    assert_eq!(count(&h.server, "GET", &history_path()).await, 10);
    assert_eq!(details_asked(&requests(&h.server).await).len(), 200);
    assert_eq!(
        stored_anchor(&h.st).await.map(|a| a.0),
        Some(entry_id(0)),
        "a finished first run leaves an anchor"
    );

    // The act starts between entries 30 and 31: entries 0..=30 are in it.
    let h = harness().await;
    let base = base_ms();
    mount_feed(&h.server, Feed::new(230)).await;
    let act_start = entry_start(base, 30) - 30_000;
    h.st.set_acts_for_test(vec![Act {
        id: ACT.into(),
        name: "ACT V".into(),
        episode_name: "V26".into(),
        start_ms: act_start,
        end_ms: now_ms() + 30 * 86_400_000,
    }]);
    run_backfill(&h).await;
    assert_eq!(count(&h.server, "GET", &history_path()).await, 2);
    assert_eq!(details_asked(&requests(&h.server).await).len(), 31);
}

#[tokio::test]
async fn backfill_first_run_picks_up_act_tables_that_load_late() {
    // The run starts before the asset tables are loaded (a fresh install:
    // `ensure_tables` runs beside the connect), so it has no act start yet;
    // the tables arrive before its first page — the act start still bounds it.
    let h = harness().await;
    let base = base_ms();
    mount_feed(&h.server, Feed::new(230)).await;
    h.st.set_acts_for_test(Vec::new());
    start_backfill_run(&h.st).await;
    assert_eq!(
        lock(&h.st.live).backfill.as_ref().unwrap().act_start_ms,
        None
    );
    h.st.set_acts_for_test(vec![Act {
        id: ACT.into(),
        name: "ACT V".into(),
        episode_name: "V26".into(),
        start_ms: entry_start(base, 30) - 30_000,
        end_ms: now_ms() + 30 * 86_400_000,
    }]);

    let id = lock(&h.st.live).try_start_worker().expect("run pending");
    run_worker(h.st.clone(), h.session.clone(), id).await;
    assert_eq!(count(&h.server, "GET", &history_path()).await, 2);
    assert_eq!(details_asked(&requests(&h.server).await).len(), 31);
}

#[tokio::test]
async fn backfill_without_act_tables_is_bounded_by_the_cap_alone() {
    // The act tables aren't loaded (`act_start_ms` is None): the 200 cap
    // still bounds the first run.
    let h = harness().await;
    mount_feed(&h.server, Feed::new(230)).await;
    h.st.set_acts_for_test(Vec::new());
    assert!(h.st.current_act().is_none());
    run_backfill(&h).await;
    assert_eq!(count(&h.server, "GET", &history_path()).await, 10);
    assert_eq!(details_asked(&requests(&h.server).await).len(), 200);
}

#[tokio::test]
async fn backfill_never_delays_live_card_sync() {
    let h = harness().await;
    // Every match-details answer takes a moment, so the run is provably
    // mid-flight when the roster arrives.
    let mut feed = Feed::new(60);
    feed.details_delay = Some(Duration::from_millis(25));
    mount_feed(&h.server, feed).await;
    {
        // The user's own card is settled: only the backfill is left to do.
        let me = slot_for(&h.st, ME, false).await;
        let mut live = lock(&h.st.live);
        live.stage = Stage::Menus;
        live.slots = vec![me];
        live.synced.insert(ME.into());
        live.names_attempted.insert(ME.into());
    }
    start_backfill_run(&h.st).await;
    let id = lock(&h.st.live)
        .try_start_worker()
        .expect("the run is work");
    let worker = tokio::spawn(run_worker(h.st.clone(), h.session.clone(), id));

    // Wait for the backfill to be well under way…
    for _ in 0..200 {
        if details_asked(&requests(&h.server).await).len() >= 3 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert!(
        details_asked(&requests(&h.server).await).len() >= 3,
        "run started"
    );
    assert!(!worker.is_finished(), "still backfilling");

    // …then a roster with a new, unsynced player arrives.
    let seen_before = requests(&h.server).await.len();
    {
        let mut live = lock(&h.st.live);
        live.slots.push(Slot {
            roster: roster(OTHER, false),
            db: PlayerDb::default(),
        });
        live.names_attempted.insert(OTHER.into());
    }
    worker
        .await
        .expect("the worker ends when everything is done");

    let all = requests(&h.server).await;
    let after = &all[seen_before..];
    let comp = format!("/mmr/v1/players/{OTHER}/competitiveupdates");
    let first_sync = after
        .iter()
        .position(|r| r.url.path() == comp)
        .expect("the new player was synced");
    let backfill_first = after[..first_sync]
        .iter()
        .filter(|r| r.url.path().starts_with("/match-details/v1/matches/"))
        .count();
    assert!(
        backfill_first <= 1,
        "at most the one in-flight backfill request precedes the live sync, got {backfill_first}"
    );
    assert!(lock(&h.st.live).synced.contains(OTHER));

    // The backfill resumed behind the live card and finished.
    assert_eq!(details_asked(&all).len(), 60);
    assert!(stored_anchor(&h.st).await.is_some());
}

#[tokio::test]
async fn backfill_interrupted_keeps_anchor() {
    let h = harness().await;
    let base = base_ms();
    let lockfile_dir = h._dir.path().to_path_buf();
    let mut feed = Feed::new(30);
    // The client goes away right after the 10th match-details answer.
    feed.on_details = Some(Arc::new(move |n| {
        if n == 10 {
            let lock_path = lockfile_dir
                .join("Riot Games")
                .join("Riot Client")
                .join("Config")
                .join("lockfile");
            std::fs::remove_file(lock_path).unwrap();
        }
    }));
    mount_feed(&h.server, feed).await;
    store_anchor(&h.st, 25, base).await;
    let old_anchor = stored_anchor(&h.st).await;
    assert!(old_anchor.is_some());

    run_backfill(&h).await;

    // Interrupted: the anchor did not move, the run is gone, and what was
    // fetched before the drop is stored.
    assert_eq!(stored_anchor(&h.st).await, old_anchor);
    assert!(!run_active(&h.st), "the next connect starts over");
    let first_run = details_asked(&requests(&h.server).await);
    assert_eq!(first_run.len(), 10);
    for id in &first_run {
        assert!(is_stored(&h.st, id.clone()).await);
    }

    // The next connect: a fresh session, the same anchor.
    write_client_files(h._dir.path(), h.server.address().port());
    let base_url = h.server.uri();
    let session = Arc::new(
        RiotSession::connect_with_overrides(
            h._dir.path(),
            ConnectOverrides {
                pd_base: Some(base_url.clone()),
                glz_base: Some(base_url.clone()),
                valorant_api_base: Some(base_url),
                ..Default::default()
            },
        )
        .await
        .expect("reconnect to the mock client")
        .with_global_rate(1_000.0),
    );
    start_backfill_run(&h.st).await;
    let id = lock(&h.st.live).try_start_worker().expect("run pending");
    run_worker(h.st.clone(), session, id).await;

    // Only the ids still missing were fetched, then the anchor moved.
    let all = details_asked(&requests(&h.server).await);
    assert_eq!(all.len(), 25, "entries 0..25, none twice: {all:?}");
    let mut unique = all.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), 25);
    assert_eq!(
        stored_anchor(&h.st).await,
        Some((entry_id(0), entry_start(base, 0)))
    );
    assert!(!run_active(&h.st));
}

#[tokio::test]
async fn backfill_history_error_ends_the_run_and_keeps_the_anchor() {
    let h = harness().await;
    let base = base_ms();
    // The second page fails (a Riot error, not a disconnect).
    let mut feed = Feed::new(30);
    feed.history_fails_from = Some(20);
    mount_feed(&h.server, feed).await;
    store_anchor(&h.st, 25, base).await;
    let old = stored_anchor(&h.st).await;

    run_backfill(&h).await;

    assert_eq!(
        stored_anchor(&h.st).await,
        old,
        "an unfinished run moves nothing"
    );
    assert!(!run_active(&h.st), "the run ended; the worker did not spin");
    assert_eq!(count(&h.server, "GET", &history_path()).await, 2);
    // Page 0's matches were fetched and stay stored for the retry.
    assert_eq!(details_asked(&requests(&h.server).await).len(), 20);
}

#[tokio::test]
async fn backfill_details_error_ends_the_run_but_a_missing_match_is_skipped() {
    let base = base_ms();
    // 404: Riot no longer has that match — skipped for good, or the anchor
    // would be stuck behind it forever.
    let h = harness().await;
    let mut feed = Feed::new(10);
    feed.details_status.insert(4, 404);
    mount_feed(&h.server, feed).await;
    store_anchor(&h.st, 9, base).await;
    run_backfill(&h).await;
    assert_eq!(
        details_asked(&requests(&h.server).await).len(),
        9,
        "all tried"
    );
    assert_eq!(
        stored_anchor(&h.st).await,
        Some((entry_id(0), entry_start(base, 0))),
        "the run completed past the 404"
    );

    // 500: a Riot error — the run ends, the anchor stays.
    let h = harness().await;
    let mut feed = Feed::new(10);
    feed.details_status.insert(4, 500);
    mount_feed(&h.server, feed).await;
    store_anchor(&h.st, 9, base).await;
    let old = stored_anchor(&h.st).await;
    run_backfill(&h).await;
    assert_eq!(stored_anchor(&h.st).await, old);
    assert!(!run_active(&h.st));
    assert_eq!(
        details_asked(&requests(&h.server).await).len(),
        5,
        "entries 0..=4, then it stopped"
    );
}

#[tokio::test]
async fn backfill_never_writes_another_accounts_anchor() {
    let h = harness().await;
    let base = base_ms();
    mount_feed(&h.server, Feed::new(5)).await;

    // (a) The live model moved to another account after the run started: the
    // step, still holding the old session, does nothing (and the worker
    // running it ends) — no requests, no anchor for OTHER from ME's run.
    start_backfill_run(&h.st).await;
    lock(&h.st.live).self_puuid = Some(OTHER.into());
    assert_eq!(backfill_step(&h.st, &h.session).await, Err(Gone));
    assert_eq!(stored_anchor(&h.st).await, None);
    assert_eq!(count(&h.server, "GET", &history_path()).await, 0);

    // (b) The switch cleared the run and the stored anchor (what
    // `Poller::on_connected` does); a stale write still in flight for the old
    // account is refused by the DB-side check.
    lock(&h.st.live).self_puuid = Some(ME.into());
    lock(&h.st.live).backfill = None;
    h.st.db(|repo| repo.set_state(crate::store::SELF_PUUID_KEY, OTHER))
        .await
        .unwrap();
    start_backfill_run(&h.st).await;
    backfill_step(&h.st, &h.session).await.unwrap();
    assert_eq!(
        stored_anchor(&h.st).await,
        None,
        "the stored account is OTHER: ME's anchor must not be written"
    );

    // Control: with ME stored as the account, the same run does write it.
    lock(&h.st.live).backfill = None;
    h.st.db(|repo| repo.set_state(crate::store::SELF_PUUID_KEY, ME))
        .await
        .unwrap();
    start_backfill_run(&h.st).await;
    backfill_step(&h.st, &h.session).await.unwrap();
    assert_eq!(
        stored_anchor(&h.st).await,
        Some((entry_id(0), entry_start(base, 0)))
    );
}

#[tokio::test]
async fn a_run_in_progress_keeps_going_when_another_trigger_fires() {
    let h = harness().await;
    mount_feed(&h.server, Feed::new(50)).await;
    {
        // A live job is waiting, so the step yields right after its page.
        let mut live = lock(&h.st.live);
        live.slots.push(Slot {
            roster: roster(OTHER, false),
            db: PlayerDb::default(),
        });
        live.names_attempted.insert(OTHER.into());
    }
    start_backfill_run(&h.st).await;
    backfill_step(&h.st, &h.session).await.unwrap();
    let cursor = |st: &AppState| {
        let live = lock(&st.live);
        let run = live.backfill.as_ref().expect("the run is still active");
        (run.next_index, run.scanned, run.pending.len())
    };
    assert_eq!(cursor(&h.st), (BACKFILL_PAGE, BACKFILL_PAGE, 20));
    assert_eq!(
        lock(&h.st.live).backfill.as_ref().unwrap().total,
        Some(50),
        "the run remembers the history's Total"
    );
    assert!(
        details_asked(&requests(&h.server).await).is_empty(),
        "yielded before any match-details request"
    );

    // A second trigger (a match ended) must not rewind or replace the run.
    start_backfill_run(&h.st).await;
    assert_eq!(cursor(&h.st), (BACKFILL_PAGE, BACKFILL_PAGE, 20));
    assert_eq!(count(&h.server, "GET", &history_path()).await, 1);
}

/// Sets the stored anchor to an entry that isn't in the history (`gone`),
/// older than every entry the mock serves: a run can never meet it.
async fn store_unreachable_anchor(st: &AppState, base: i64) {
    let raw = crate::sync::anchor_json(&("gone".to_string(), entry_start(base, 10_000)));
    st.db(move |repo| repo.set_state(BACKFILL_ANCHOR_KEY, &raw))
        .await
        .unwrap();
}

#[tokio::test]
async fn backfill_a_short_page_mid_history_does_not_end_the_run() {
    // Riot answers with 15 entries per page although 20 were asked for, in a
    // 40-match history whose `Total` says so. A short page is not the end:
    // the cursor advances by what the page held (0, 15, 30) and nothing
    // behind a short page is skipped — a gap the anchor would make permanent.
    let base = base_ms();

    // A later run: the anchor is entry 35.
    let h = harness().await;
    let mut feed = Feed::new(40);
    feed.page_cap = Some(15);
    mount_feed(&h.server, feed).await;
    store_anchor(&h.st, 35, base).await;
    run_backfill(&h).await;
    let reqs = requests(&h.server).await;
    assert_eq!(history_starts(&reqs), ["0", "15", "30"]);
    let mut asked = details_asked(&reqs);
    asked.sort();
    assert_eq!(asked, (0..35).map(entry_id).collect::<Vec<_>>());
    assert!(
        !is_stored(&h.st, entry_id(35)).await,
        "the anchor and older are not fetched"
    );
    assert_eq!(
        stored_anchor(&h.st).await,
        Some((entry_id(0), entry_start(base, 0)))
    );
    assert!(!run_active(&h.st));

    // A first run (the act covers all 40): the history's end, by `Total`,
    // stops it — after the page holding the last entry, with no extra one.
    let h = harness().await;
    let mut feed = Feed::new(40);
    feed.page_cap = Some(15);
    mount_feed(&h.server, feed).await;
    run_backfill(&h).await;
    let reqs = requests(&h.server).await;
    assert_eq!(history_starts(&reqs), ["0", "15", "30"]);
    assert_eq!(details_asked(&reqs).len(), 40);
    assert_eq!(
        stored_anchor(&h.st).await,
        Some((entry_id(0), entry_start(base, 0)))
    );
    assert!(!run_active(&h.st));
}

#[tokio::test]
async fn backfill_without_total_falls_back_to_the_short_page_rule() {
    // No `Total` in the answer: a page shorter than the page size is the end
    // (the second page holds the last 10 of 30).
    let h = harness().await;
    let base = base_ms();
    let mut feed = Feed::new(30);
    feed.total = TotalMode::Absent;
    mount_feed(&h.server, feed).await;
    run_backfill(&h).await;
    let reqs = requests(&h.server).await;
    assert_eq!(history_starts(&reqs), ["0", "20"]);
    assert_eq!(details_asked(&reqs).len(), 30);
    assert_eq!(
        stored_anchor(&h.st).await,
        Some((entry_id(0), entry_start(base, 0)))
    );
    assert!(!run_active(&h.st));
}

#[tokio::test]
async fn backfill_an_anchor_that_left_the_history_is_bounded_by_total() {
    // The anchor is older than everything Riot still holds and its id is
    // gone: nothing on the pages ever matches it. The history's `Total` is
    // what bounds the run — the page that reaches it is the last, so there
    // is no extra request for an empty page (40 = two full pages).
    let h = harness().await;
    let base = base_ms();
    mount_feed(&h.server, Feed::new(40)).await;
    store_unreachable_anchor(&h.st, base).await;
    run_backfill(&h).await;
    let reqs = requests(&h.server).await;
    assert_eq!(history_starts(&reqs), ["0", "20"]);
    assert_eq!(details_asked(&reqs).len(), 40);
    assert_eq!(
        stored_anchor(&h.st).await,
        Some((entry_id(0), entry_start(base, 0)))
    );
    assert!(!run_active(&h.st));
}

#[tokio::test]
async fn backfill_a_total_that_claims_too_much_still_ends_on_an_empty_page() {
    // `Total` says 100 but Riot holds 30: the empty page past the end stops
    // the run (three requests: 0, 20, 30) instead of paging on.
    let h = harness().await;
    let base = base_ms();
    let mut feed = Feed::new(30);
    feed.total = TotalMode::Fixed(100);
    mount_feed(&h.server, feed).await;
    store_unreachable_anchor(&h.st, base).await;
    run_backfill(&h).await;
    let reqs = requests(&h.server).await;
    assert_eq!(history_starts(&reqs), ["0", "20", "30"]);
    assert_eq!(details_asked(&reqs).len(), 30);
    assert_eq!(
        stored_anchor(&h.st).await,
        Some((entry_id(0), entry_start(base, 0)))
    );
    assert!(!run_active(&h.st));
}
