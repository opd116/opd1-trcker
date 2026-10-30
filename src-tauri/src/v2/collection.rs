//! `collection` (R189, user 2026-10-22): the user's full owned weapon skin
//! collection, with standard VP prices per edition.
//!
//! - Riot's owned skin levels (`pd …/store/v1/entitlements/{self}/{skin
//!   levels}`, one read-only `GET` in the `other` throttle family) are asked
//!   only while VALORANT runs, at most once per [`REFRESH_EVERY`] and only
//!   when the Skins screen asks (never from the poll loop). Only a stored
//!   answer counts as the refresh: a refresh attempt that failed or was
//!   refused by a request hold (nothing sent) is not counted, so the screen's
//!   normal one-minute retry reaches Riot instead of waiting out the ten
//!   minutes behind one transient Riot error.
//! - The owned level ids are stored in `app_state` under [`store_key`], one
//!   key per account, with the owner's puuid and the fetch time (a second
//!   account never erases the first one's copy), so the screen still shows with
//!   the game closed. Only ids are stored: names, weapons, editions and
//!   prices come from valorant-api's skin catalog
//!   (`AssetCache::collection`), cached on disk per valorant-api version.
//! - `None` = no refresh was attempted for this account yet (VALORANT has
//!   not run since it was first seen), so the screen shows its "no collection
//!   yet" empty state. A refresh attempt that left no stored answer (Riot
//!   error, request hold, store failure) is `Err` instead: the screen must not
//!   tell the user to open the page while the game runs when it already was.
//!   A call superseded by an account switch is `Err` too: it is not an
//!   answer for the account on screen, so the screen keeps its retry state
//!   and re-asks for the current account.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use opd1_core::presence::Stage;
use opd1_core::wire::Collection;
use opd1_riot::endpoints::SKIN_LEVEL_ITEM_TYPE;
use opd1_riot::session::RiotError;

use super::AppStateRef;
use crate::state::{lock, now_ms, AppState};

/// The key in `app_state` before it was kept per account: still read, for
/// its owner, until that account's own key is written.
pub const STORE_KEY: &str = "self_collection";

/// `puuid`'s key in `app_state`.
pub fn store_key(puuid: &str) -> String {
    format!("{STORE_KEY}:{puuid}")
}

/// `puuid`'s stored copy: their own key, else the shared key of older
/// versions when that copy is theirs.
fn load_stored(repo: &opd1_core::db::repo::Repo, puuid: &str) -> Option<StoredOwned> {
    [store_key(puuid), STORE_KEY.to_string()]
        .iter()
        .find_map(|key| {
            let json = repo.get_state(key).ok().flatten()?;
            StoredOwned::parse(&json, puuid)
        })
}

/// The least time between two stored answers for the same account; a refresh
/// attempt that stored nothing does not start this window.
pub const REFRESH_EVERY: Duration = Duration::from_secs(10 * 60);

/// The most owned level ids a stored copy may hold (the riot crate keeps at
/// most this many too; ~1400 skins exist).
const MAX_STORED: usize = 10_000;

/// What `app_state` keeps: whose collection, when Riot answered, and the
/// owned skin level ids (lower-case uuids).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredOwned {
    pub puuid: String,
    pub fetched_ms: i64,
    pub levels: Vec<String>,
}

impl StoredOwned {
    /// The stored copy for `puuid`, validated: a copy that doesn't parse,
    /// belongs to another account or holds a non-uuid id is ignored.
    fn parse(json: &str, puuid: &str) -> Option<Self> {
        let stored: StoredOwned = serde_json::from_str(json).ok()?;
        (stored.puuid == puuid
            && stored.fetched_ms > 0
            && stored.levels.len() <= MAX_STORED
            && stored.levels.iter().all(|l| is_uuid(l)))
        .then_some(stored)
    }
}

fn is_uuid(s: &str) -> bool {
    s.len() == 36
        && s.char_indices().all(|(i, c)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                c == '-'
            } else {
                c.is_ascii_hexdigit()
            }
        })
}

/// The last stored answer: for whom and when (process memory only, so a
/// restart may ask once more). A refresh attempt that stored nothing is not
/// recorded here.
static LAST_ASK: Mutex<Option<(String, Instant)>> = Mutex::new(None);
/// One refresh at a time: a second `collection` call waits for the first and
/// then uses its stored answer, or retries when it stored nothing.
static REFRESH: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Whether a refresh may be attempted for `puuid` at `now`.
fn due(last: Option<&(String, Instant)>, puuid: &str, now: Instant) -> bool {
    match last {
        Some((who, at)) if who == puuid => now.saturating_duration_since(*at) >= REFRESH_EVERY,
        _ => true,
    }
}

/// The user's owned skins; `None` while no refresh was attempted for this
/// account yet (the game has not run since it was first seen), `Err` when a
/// refresh attempt left no stored answer or the account switched while the
/// call was in flight.
#[tauri::command]
pub async fn collection(state: AppStateRef<'_>) -> Result<Option<Collection>, String> {
    let st = state.inner().clone();
    collection_for(&st).await
}

/// [`collection`]'s body, over `st` so the tests can drive it without Tauri's
/// `State`.
async fn collection_for(st: &Arc<AppState>) -> Result<Option<Collection>, String> {
    let Some(me) = lock(&st.live).self_puuid.clone() else {
        return Ok(None);
    };
    let no_stored_answer = refresh_if_due(st, &me).await;

    // The Riot client can switch account while Riot answers: this call was
    // made for the account on screen when it started, and its stored copy (or
    // its failure) belongs to that account, not to the one on screen now. An
    // `Err` is not an answer for the new account either, so the screen keeps
    // its retry state (never the "no collection yet" empty state) and the
    // account handler re-asks for the account that is on screen now.
    if lock(&st.live).self_puuid.as_deref() != Some(me.as_str()) {
        return Err("skins unavailable".into());
    }

    let owner = me.clone();
    let stored = st
        .db(move |repo| Ok(load_stored(repo, &owner)))
        .await
        .flatten();
    let Some(stored) = stored else {
        // A refresh attempt that stored nothing is not "never fetched": the
        // screen shows its retry state and asks again in a minute, instead of
        // the empty state that tells the user to open the page while
        // VALORANT runs.
        return if no_stored_answer {
            Err("skins unavailable".into())
        } else {
            Ok(None)
        };
    };
    // The catalog needs the tables' version; offline the disk copies serve.
    let _ = st.ensure_tables(false).await;
    match st
        .assets
        .collection(&stored.levels, stored.fetched_ms)
        .await
    {
        Ok(collection) => Ok(Some(collection)),
        Err(e) => {
            log::warn!("skins collection unavailable: {e}");
            Err("skins unavailable".into())
        }
    }
}

/// Asks Riot for the owned skin levels when VALORANT runs for `me` and the
/// last stored answer is [`REFRESH_EVERY`] old, and stores the answer. Every
/// failure is logged and leaves the stored copy as it was.
///
/// Returns whether a refresh attempt left no stored answer: a Riot error, a
/// `Held` answer (nothing was sent), or a store that did not happen. `false`
/// when no attempt was made at all (no session, the game closed, the last
/// stored answer too recent), so the caller keeps the "no collection yet"
/// state for those.
async fn refresh_if_due(st: &Arc<AppState>, me: &str) -> bool {
    let Some(session) = st.session() else {
        return false;
    };
    // R46: the session is stale while the game is closed.
    if lock(&st.live).stage == Stage::Closed || session.self_puuid() != me {
        return false;
    }
    let _one = REFRESH.lock().await;
    if !due(lock(&LAST_ASK).as_ref(), me, Instant::now()) {
        return false;
    }
    let answer = session.owned_skin_levels().await;
    if let Err(RiotError::Held(left)) = &answer {
        // R160: a hold is "not now", not an answer, and nothing was sent. No
        // stored answer results, so the screen shows its retry state rather
        // than claiming the collection was never fetched.
        log::debug!("owned skins not asked: hold, {left:?} left");
        return true;
    }
    let levels = match answer {
        Ok(resp) => resp.item_ids(SKIN_LEVEL_ITEM_TYPE),
        Err(e) => {
            log::warn!("owned skins: {e}");
            return true;
        }
    };
    log::debug!("owned skins: {} levels", levels.len());
    let stored = StoredOwned {
        puuid: me.to_string(),
        fetched_ms: now_ms(),
        levels,
    };
    let Ok(json) = serde_json::to_string(&stored) else {
        return true;
    };
    let owner = st.clone();
    let me = me.to_string();
    let stored_for = me.clone();
    let saved = st
        .db(move |repo| {
            // An account switch while Riot answered: not this account's list.
            if lock(&owner.live).self_puuid.as_deref() != Some(stored_for.as_str()) {
                return Ok(false);
            }
            repo.set_state(&store_key(&stored_for), &json)?;
            Ok(true)
        })
        .await;
    // Only a stored answer starts the ten-minute window: an attempt that
    // stored nothing must not keep the screen's next retry from reaching
    // Riot.
    if saved == Some(true) {
        *lock(&LAST_ASK) = Some((me, Instant::now()));
        false
    } else {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_account_keeps_its_own_stored_copy() {
        use opd1_core::db::{open_db, repo::Repo};
        let repo = Repo::new(open_db(std::path::Path::new(":memory:")).unwrap());
        let other = "bbbbbbbb-0000-0000-0000-000000000002";
        let copy = |puuid: &str| {
            serde_json::to_string(&StoredOwned {
                puuid: puuid.into(),
                fetched_ms: 1,
                levels: vec![],
            })
            .unwrap()
        };
        // An older version's shared key, ME's: still read for ME.
        repo.set_state(STORE_KEY, &copy(ME)).unwrap();
        assert!(load_stored(&repo, ME).is_some());
        // The other account's fetch never erases ME's copy.
        repo.set_state(&store_key(ME), &copy(ME)).unwrap();
        repo.set_state(STORE_KEY, &copy(other)).unwrap();
        repo.set_state(&store_key(other), &copy(other)).unwrap();
        assert_eq!(load_stored(&repo, ME).unwrap().puuid, ME);
        assert_eq!(load_stored(&repo, other).unwrap().puuid, other);
    }

    const ME: &str = "aaaaaaaa-0000-0000-0000-000000000001";
    const L1: &str = "578e9077-4f88-260c-e54c-b988425c60e4";
    /// The other account a Riot client can switch to mid-call.
    const OTHER_ACCOUNT: &str = "bbbbbbbb-0000-0000-0000-000000000002";

    #[test]
    fn riot_is_asked_again_only_after_ten_minutes_or_for_another_account() {
        let now = Instant::now();
        assert!(due(None, ME, now));
        let recent = (ME.to_string(), now);
        assert!(!due(Some(&recent), ME, now + Duration::from_secs(599)));
        assert!(due(Some(&recent), ME, now + REFRESH_EVERY));
        assert!(due(Some(&recent), "someone-else", now));
    }

    /// The stored copy of `ME` in the harness's DB.
    async fn stored(h: &crate::pipeline_tests::Harness) -> Option<StoredOwned> {
        h.st.db(|repo| Ok(load_stored(repo, ME))).await.flatten()
    }

    /// The statics [`LAST_ASK`] and [`REFRESH`] are process-wide and every
    /// harness session answers for the same puuid: the command-level tests
    /// take turns.
    static COMMANDS: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    /// How often the mock's store-endpoints path was asked.
    async fn store_requests(h: &crate::pipeline_tests::Harness, key: &str) -> usize {
        h.server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|r| r.url.path() == key)
            .count()
    }

    fn store_key_path() -> String {
        format!("/store/v1/entitlements/{ME}/{SKIN_LEVEL_ITEM_TYPE}")
    }

    /// Mounts the mock store endpoint: `status`, at most `times`, then
    /// `body` as JSON when given. `priority` decides between the overlapping
    /// mocks of a test explicitly (wiremock: 1 beats 2).
    async fn mount_store(
        h: &crate::pipeline_tests::Harness,
        status: u16,
        times: Option<u64>,
        body: Option<serde_json::Value>,
        priority: u8,
    ) {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, ResponseTemplate};

        let mut template = ResponseTemplate::new(status);
        if let Some(body) = body {
            template = template.set_body_json(body);
        }
        let mut mock = Mock::given(method("GET"))
            .and(path(store_key_path()))
            .respond_with(template)
            .with_priority(priority);
        if let Some(n) = times {
            mock = mock.up_to_n_times(n);
        }
        mock.mount(&h.server).await;
    }

    /// Mounts the one weapon the collection tests own a skin of: its default
    /// skin (dropped by the catalog) and the Reaver Vandal on `L1` (Premium).
    async fn mount_weapons(server: &wiremock::MockServer) {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, ResponseTemplate};

        Mock::given(method("GET"))
            .and(path("/v1/weapons"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "status": 200,
                "data": [{
                    "uuid": "9c82e19d-4575-0200-1a81-3eacf00cf872",
                    "displayName": "Vandal",
                    "category": "EEquippableCategory::Rifle",
                    "defaultSkinUuid": "27f21d97-4c4b-bd1c-1f08-31830ab0be84",
                    "displayIcon": null,
                    "skins": [{
                        "uuid": "30388628-42f0-606c-82c0-73ad43de997f",
                        "displayName": "Reaver Vandal",
                        "contentTierUuid": "60bca009-4182-7998-dee7-b8a2558dc369",
                        "levels": [{"uuid": L1, "displayIcon": null}]
                    }, {
                        "uuid": "27f21d97-4c4b-bd1c-1f08-31830ab0be84",
                        "displayName": "Standard Vandal",
                        "contentTierUuid": null,
                        "levels": [{"uuid": "bbbb0000-0000-0000-0000-000000000001",
                                    "displayIcon": null}]
                    }]
                }]
            })))
            .mount(server)
            .await;
    }

    fn one_level() -> serde_json::Value {
        serde_json::json!({
            "ItemTypeID": SKIN_LEVEL_ITEM_TYPE,
            "Entitlements": [{"ItemID": L1}]
        })
    }

    /// R189: a failed ask is not a refresh. The Skins screen asks again at
    /// its own one-minute retry, and that ask has to reach Riot: before this
    /// test's fix the failure set the ten-minute timer too, so the retry got
    /// "no collection yet" again and the screen showed its empty state for
    /// the rest of the wait. A 500 for the first ask and a real answer for
    /// the second is the reproduction.
    #[tokio::test]
    async fn a_failed_ask_is_not_counted_as_the_ten_minute_refresh() {
        let _serial = COMMANDS.lock().await;
        let h = crate::pipeline_tests::harness().await;
        h.st.set_session(Some(h.session.clone()));
        lock(&h.st.live).stage = Stage::Menus;
        *lock(&LAST_ASK) = None;
        mount_store(&h, 500, Some(1), None, 1).await;
        mount_store(&h, 200, None, Some(one_level()), 2).await;

        refresh_if_due(&h.st, ME).await;
        assert_eq!(store_requests(&h, &store_key_path()).await, 1);
        assert!(stored(&h).await.is_none(), "a 500 is not an answer");
        // The screen's next ask must reach Riot again, not be skipped for ten
        // minutes because the failed call was counted as the refresh.
        refresh_if_due(&h.st, ME).await;
        assert_eq!(
            store_requests(&h, &store_key_path()).await,
            2,
            "the retry after a failure reaches Riot"
        );
        assert_eq!(
            stored(&h).await.expect("the second ask answered").levels,
            [L1]
        );
        // …but a stored answer still counts as the ten-minute refresh: the
        // next ask is not sent (the gate is relaxed only after a failure).
        refresh_if_due(&h.st, ME).await;
        assert_eq!(
            store_requests(&h, &store_key_path()).await,
            2,
            "a stored answer gates the next ask"
        );
    }

    /// No refresh attempt at all (no session, or the game closed and the
    /// session stale, R46) stays the "no collection yet" empty state:
    /// `Ok(None)`, Riot not asked. Only an attempt that stored nothing is an
    /// error.
    #[tokio::test]
    async fn no_ask_at_all_is_still_the_empty_state() {
        let _serial = COMMANDS.lock().await;
        let h = crate::pipeline_tests::harness().await;
        *lock(&LAST_ASK) = None;
        // A session exists, but the game is closed: nothing is asked.
        h.st.set_session(Some(h.session.clone()));
        lock(&h.st.live).stage = Stage::Closed;
        mount_store(&h, 200, None, Some(one_level()), 1).await;
        assert!(collection_for(&h.st).await.unwrap().is_none());
        assert_eq!(store_requests(&h, &store_key_path()).await, 0);

        // No session at all (Riot client gone) is the same state.
        h.st.set_session(None);
        lock(&h.st.live).stage = Stage::Menus;
        assert!(collection_for(&h.st).await.unwrap().is_none());
        assert_eq!(store_requests(&h, &store_key_path()).await, 0);
    }

    /// An attempt that stored nothing is an error, not `None`: the screen
    /// shows its retry state instead of "No collection yet" telling the user
    /// to open the page while VALORANT is already running.
    #[tokio::test]
    async fn a_failed_ask_is_not_reported_as_never_fetched() {
        let _serial = COMMANDS.lock().await;
        let h = crate::pipeline_tests::harness().await;
        h.st.set_session(Some(h.session.clone()));
        lock(&h.st.live).stage = Stage::Menus;
        *lock(&LAST_ASK) = None;
        mount_store(&h, 500, None, None, 1).await;
        assert_eq!(
            collection_for(&h.st).await.expect_err("a 500 is a failure"),
            "skins unavailable"
        );
        assert_eq!(store_requests(&h, &store_key_path()).await, 1);
    }

    /// A failed refresh does not throw away what the screen already has: an
    /// older stored copy is served, and the failure costs exactly one store
    /// request.
    #[tokio::test]
    async fn a_failed_refresh_still_serves_the_stored_copy() {
        let _serial = COMMANDS.lock().await;
        let h = crate::pipeline_tests::harness().await;
        h.st.set_session(Some(h.session.clone()));
        lock(&h.st.live).stage = Stage::Menus;
        *lock(&LAST_ASK) = None;
        // An earlier answer, kept in `app_state` under ME's key.
        let copy = StoredOwned {
            puuid: ME.into(),
            fetched_ms: 5,
            levels: vec![L1.into()],
        };
        let json = serde_json::to_string(&copy).unwrap();
        h.st.db(move |repo| repo.set_state(&store_key(ME), &json))
            .await
            .expect("the stored copy is written");
        // The tables and the skin catalog, from the mocked valorant-api.
        crate::pipeline_tests::mount_asset_tables(&h.assets_server).await;
        mount_weapons(&h.assets_server).await;
        // Riot's store endpoint fails now.
        mount_store(&h, 500, None, None, 1).await;

        let got = collection_for(&h.st)
            .await
            .expect("a failed refresh serves the stored copy, not an error")
            .expect("the stored copy still shows");
        assert_eq!(got.fetched_ms, 5);
        assert_eq!(got.items.len(), 1);
        assert_eq!(got.items[0].level_id, L1);
        assert_eq!(store_requests(&h, &store_key_path()).await, 1);
    }

    /// A call in flight across a Riot account switch belongs to the account
    /// that started it: it must not return the previous account's stored copy
    /// for the account on screen now.
    #[tokio::test]
    async fn a_call_across_an_account_switch_does_not_return_the_old_accounts_copy() {
        let _serial = COMMANDS.lock().await;
        let h = crate::pipeline_tests::harness().await;
        h.st.set_session(Some(h.session.clone()));
        lock(&h.st.live).stage = Stage::Menus;
        *lock(&LAST_ASK) = None;
        // Both accounts have a stored copy: A's one skin, B's nothing.
        let a = serde_json::to_string(&StoredOwned {
            puuid: ME.into(),
            fetched_ms: 5,
            levels: vec![L1.into()],
        })
        .unwrap();
        h.st.db(move |repo| repo.set_state(&store_key(ME), &a))
            .await
            .expect("A's copy is written");
        let b = serde_json::to_string(&StoredOwned {
            puuid: OTHER_ACCOUNT.into(),
            fetched_ms: 7,
            levels: vec![],
        })
        .unwrap();
        h.st.db(move |repo| repo.set_state(&store_key(OTHER_ACCOUNT), &b))
            .await
            .expect("B's copy is written");
        // So both copies can be joined with the catalog.
        crate::pipeline_tests::mount_asset_tables(&h.assets_server).await;
        mount_weapons(&h.assets_server).await;
        // Riot's answer is slow enough to switch account while it is in flight.
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, ResponseTemplate};
        Mock::given(method("GET"))
            .and(path(store_key_path()))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(one_level())
                    .set_delay(Duration::from_secs(1)),
            )
            .with_priority(1)
            .mount(&h.server)
            .await;

        // A starts the call; B takes over while Riot is still answering. The
        // mock records a request before waiting its configured delay, so
        // waiting for the request (not for wall-clock time) makes the switch
        // land mid-answer on any runner.
        let st = h.st.clone();
        let in_flight = tokio::spawn(async move { collection_for(&st).await });
        let deadline = Instant::now() + Duration::from_secs(5);
        while store_requests(&h, &store_key_path()).await == 0 {
            assert!(
                Instant::now() < deadline,
                "the entitlements request never reached the mock"
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        lock(&h.st.live).self_puuid = Some(OTHER_ACCOUNT.into());
        assert_eq!(
            in_flight.await.unwrap(),
            Err("skins unavailable".to_string()),
            "a superseded call keeps the retry state, never the old account's copy"
        );

        // The screen's next call for the account on screen now gets its copy.
        let got = collection_for(&h.st)
            .await
            .expect("B's call")
            .expect("B's stored copy still shows");
        assert_eq!(got.fetched_ms, 7);
        assert!(got.items.is_empty(), "B owns no skins");
    }

    #[test]
    fn a_stored_copy_counts_only_for_its_owner_and_when_valid() {
        let ok = serde_json::to_string(&StoredOwned {
            puuid: ME.into(),
            fetched_ms: 5,
            levels: vec![L1.into()],
        })
        .unwrap();
        assert_eq!(StoredOwned::parse(&ok, ME).unwrap().levels, [L1]);
        assert!(StoredOwned::parse(&ok, "another").is_none());
        for bad in [
            "not json".to_string(),
            format!(r#"{{"puuid":"{ME}","fetchedMs":5,"levels":["../x"]}}"#),
            format!(r#"{{"puuid":"{ME}","fetchedMs":0,"levels":[]}}"#),
            format!(r#"{{"puuid":"{ME}","levels":[]}}"#),
        ] {
            assert!(StoredOwned::parse(&bad, ME).is_none(), "{bad}");
        }
    }

    #[test]
    fn the_stored_copy_is_camel_case_json() {
        let json = serde_json::to_string(&StoredOwned {
            puuid: ME.into(),
            fetched_ms: 7,
            levels: vec![],
        })
        .unwrap();
        assert_eq!(
            json,
            format!(r#"{{"puuid":"{ME}","fetchedMs":7,"levels":[]}}"#)
        );
    }
}
