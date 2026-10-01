//! R163's open/close tests: the page the UI has open is closed by key, and an
//! open whose build finishes after its page was left (or after a newer open)
//! never becomes the open page. A child of `pipeline_tests` (same harness).

use std::future::Future;
use std::pin::Pin;
use std::task::Poll;

use crate::store::{save_player, PlayerUpdate};

use super::*;

/// The DB lock, held by another thread until this guard is dropped: every
/// `AppState::db` call (the first await of a details or match open) parks
/// behind it. This is what makes a lost-open test deterministic: with the DB
/// held, an open that has taken its ticket cannot finish, whatever the
/// scheduler does; a `join!` of the open and the close does not give that
/// (`tokio::join!` rotates which branch it polls first, and a blocking-pool
/// read can finish before the first poll).
pub(super) struct DbHold {
    release: Option<std::sync::mpsc::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

pub(super) fn hold_db(st: &Arc<AppState>) -> DbHold {
    let (held_tx, held_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
    let st = st.clone();
    let thread = std::thread::spawn(move || {
        st.db_blocking(|_| {
            held_tx.send(()).unwrap();
            let _ = release_rx.recv(); // Err once the guard is dropped, too
            Ok(())
        });
    });
    held_rx.recv().expect("the DB lock is held");
    DbHold {
        release: Some(release_tx),
        thread: Some(thread),
    }
}

impl Drop for DbHold {
    fn drop(&mut self) {
        drop(self.release.take());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Polls `open` once, so it takes its ticket and parks on its first DB await,
/// and hands it back to be awaited later. Only meaningful while [`hold_db`]
/// holds the DB: then the first poll is always `Pending`.
pub(super) async fn parked<F: Future>(open: F) -> Pin<Box<F>> {
    let mut open = Box::pin(open);
    let first = std::future::poll_fn(|cx| Poll::Ready(open.as_mut().poll(cx))).await;
    assert!(first.is_pending(), "the open must be waiting on the DB");
    open
}

/// Both players are known to the DB, so their Details can be built.
async fn known(h: &Harness, puuids: &[&'static str]) {
    let puuids = puuids.to_vec();
    h.st.db(move |repo| {
        for puuid in puuids {
            save_player(
                repo.conn(),
                &PlayerUpdate {
                    puuid: puuid.into(),
                    name: Some(("Someone".into(), "TAG".into())),
                    account_level: 50,
                    card_id: "card".into(),
                },
                now_ms(),
            )
            .unwrap();
        }
        Ok(())
    })
    .await
    .unwrap();
}

/// The Player Details of `puuid` as `details::build` gives them, without
/// opening anything.
async fn built(h: &Harness, puuid: &str) -> opd1_core::model::PlayerDetails {
    crate::details::build(&h.st, puuid).await.unwrap()
}

#[tokio::test]
async fn a_close_for_another_player_leaves_the_open_details_alone() {
    let h = harness().await;
    known(&h, &[ME, OTHER]).await;
    crate::commands::player_details_for(&h.st, OTHER)
        .await
        .unwrap();
    assert!(h.st.details_open_for(OTHER));

    crate::commands::close_details_key_for(&h.st, Some(ME));
    assert!(h.st.details_open_for(OTHER), "ME's close is not OTHER's");
    crate::commands::close_details_key_for(&h.st, Some(OTHER));
    assert!(!h.st.details_open_for(OTHER));
    // Idempotent, and the unkeyed close still closes whatever is open.
    crate::commands::close_details_key_for(&h.st, Some(OTHER));
    crate::commands::player_details_for(&h.st, OTHER)
        .await
        .unwrap();
    crate::commands::close_details_for(&h.st);
    assert!(!h.st.details_open_for(OTHER));
}

#[tokio::test]
async fn a_details_open_finishing_after_its_close_never_opens() {
    // Scenario A of the UI review (I1): the build is still running when the
    // page is left; the late `set_open_details` must not re-open it.
    let h = harness().await;
    known(&h, &[ME, OTHER]).await;
    let d = built(&h, OTHER).await;
    let ticket = h.st.begin_open_details(OTHER);
    crate::commands::close_details_key_for(&h.st, Some(OTHER));
    assert!(!h.st.set_open_details(ticket, &d), "the open lost");
    assert!(!h.st.details_open_for(OTHER), "nothing is open");
    assert_eq!(h.st.open_details_puuid(), None);
}

#[tokio::test]
async fn a_late_details_open_never_overwrites_a_newer_one() {
    // Scenario B: open A (slow), Back, open B; A's build ends last.
    let h = harness().await;
    known(&h, &[ME, OTHER]).await;
    let (a, b) = (built(&h, OTHER).await, built(&h, ME).await);
    let first = h.st.begin_open_details(OTHER);
    crate::commands::close_details_key_for(&h.st, Some(OTHER));
    let second = h.st.begin_open_details(ME);
    assert!(h.st.set_open_details(second, &b));
    assert!(!h.st.set_open_details(first, &a), "A's late answer lost");
    assert_eq!(h.st.open_details_puuid().as_deref(), Some(ME));

    // A stale close for A (after B opened) does not close B.
    crate::commands::close_details_key_for(&h.st, Some(OTHER));
    assert_eq!(h.st.open_details_puuid().as_deref(), Some(ME));
    // And a newer open for the same player supersedes an older one.
    let older = h.st.begin_open_details(ME);
    let newer = h.st.begin_open_details(ME);
    assert!(!h.st.set_open_details(older, &b));
    assert!(h.st.set_open_details(newer, &b));
}

#[tokio::test]
async fn a_close_for_another_match_leaves_the_open_match_alone() {
    let h = harness().await;
    h.st.set_session(Some(h.session.clone()));
    crate::match_details::load(&h.st, "m1", HIDDEN)
        .await
        .unwrap();
    assert!(h.st.open_match_args().is_some());

    crate::commands::close_match_key_for(&h.st, Some("other-match"));
    assert!(h.st.open_match_args().is_some(), "another match's close");
    crate::commands::close_match_key_for(&h.st, Some("m1"));
    assert_eq!(h.st.open_match_args(), None);
    crate::commands::close_match_key_for(&h.st, Some("m1")); // idempotent
}

#[tokio::test]
async fn a_match_open_finishing_after_its_close_or_a_newer_open_never_opens() {
    let h = harness().await;
    h.st.set_session(Some(h.session.clone()));
    let view = crate::match_details::load(&h.st, "m1", HIDDEN)
        .await
        .unwrap();
    crate::commands::close_match_for(&h.st);

    // Late after its close.
    let ticket = h.st.begin_open_match("m1");
    crate::commands::close_match_key_for(&h.st, Some("m1"));
    assert!(!h.st.set_open_match(ticket, "m1", HIDDEN, &view));
    assert_eq!(h.st.open_match_args(), None);

    // Late after a newer open (another focus, so the two are told apart).
    let first = h.st.begin_open_match("m1");
    crate::commands::close_match_key_for(&h.st, Some("m1"));
    let second = h.st.begin_open_match("m1");
    assert!(h.st.set_open_match(second, "m1", OTHER, &view));
    assert!(!h.st.set_open_match(first, "m1", HIDDEN, &view));
    assert_eq!(
        h.st.open_match_args(),
        Some(("m1".to_string(), OTHER.to_string()))
    );
}

#[tokio::test]
async fn the_meta_event_goes_out_when_the_tables_first_load_and_when_they_refresh() {
    // R163 (M2, M3): the UI replaces its names and colours on this event, so
    // a table that loads by another route, or a patch's new ids, reach it
    // without waiting for its own retry.
    let h = harness().await;
    assert!(h.rec.metas.lock().unwrap().is_empty());
    mount_asset_tables(&h.assets_server).await;
    h.st.ensure_tables(false).await.unwrap();
    {
        let metas = h.rec.metas.lock().unwrap();
        assert_eq!(metas.len(), 1, "first load");
        assert_eq!(metas[0], h.st.assets.meta().unwrap());
    }
    h.st.ensure_tables(false).await.unwrap();
    h.st.ensure_tables(true).await.unwrap();
    assert_eq!(
        h.rec.metas.lock().unwrap().len(),
        1,
        "already loaded, same version: nothing to say"
    );

    // A new version: downloaded again, announced again.
    h.assets_server.reset().await;
    get(
        &h.assets_server,
        "/v1/version",
        json!({"status": 200, "data": {"version": "test-2"}}),
    )
    .await;
    mount_asset_tables(&h.assets_server).await;
    h.st.ensure_tables(true).await.unwrap();
    assert_eq!(h.rec.metas.lock().unwrap().len(), 2, "refreshed");
}

/// A player with no stored name: their Details raise a name ask for the hero.
async fn nameless(h: &Harness, puuids: &[&'static str]) {
    let puuids = puuids.to_vec();
    h.st.db(move |repo| {
        for puuid in puuids {
            save_player(
                repo.conn(),
                &PlayerUpdate {
                    puuid: puuid.into(),
                    name: None,
                    account_level: 50,
                    card_id: "card".into(),
                },
                now_ms(),
            )
            .unwrap();
        }
        Ok(())
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn a_details_open_that_lost_queues_no_names_and_ends_its_skin_mark() {
    // The real command path (`player_details_for`), with the page left while
    // the build runs: the answer is still returned, but nothing is queued for
    // a page nobody sees (R163). OTHER is the control: opened normally, the
    // same kind of player does raise the ask.
    let h = harness().await;
    nameless(&h, &[OTHER, STRANGER]).await;
    assert!(!h.st.take_worker_kick());

    crate::commands::player_details_for(&h.st, OTHER)
        .await
        .unwrap();
    assert!(h.st.take_worker_kick(), "control: a nameless hero is asked");

    // The DB is held, so the open takes its ticket and cannot finish; the
    // close then voids it; only then does the build run.
    let hold = hold_db(&h.st);
    let open = parked(crate::commands::player_details_for(&h.st, STRANGER)).await;
    crate::commands::close_details_key_for(&h.st, Some(STRANGER));
    drop(hold);
    let opened = open.await;
    assert_eq!(opened.unwrap().card.puuid, STRANGER, "still answered");
    assert!(!h.st.details_open_for(STRANGER), "not open");
    assert!(!h.st.take_worker_kick(), "no name ask for a closed page");
    assert!(!h.st.skins_resolving(STRANGER));
}
