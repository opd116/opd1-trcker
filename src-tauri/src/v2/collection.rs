//! `collection` (R189, user 2026-10-22): the user's full owned weapon skin
//! collection, with standard VP prices per edition.
//!
//! - Riot's owned skin levels (`pd …/store/v1/entitlements/{self}/{skin
//!   levels}`, one read-only `GET` in the `other` throttle family) are asked
//!   only while VALORANT runs, at most once per [`REFRESH_EVERY`] and only
//!   when the Skins screen asks (never from the poll loop). Only a stored
//!   answer counts as an ask: a call that failed or was refused by a request
//!   hold is not counted, so the screen's own retry gets through instead of
//!   waiting out the ten minutes behind one transient Riot error.
//! - The owned level ids are stored in `app_state` under [`store_key`], one
//!   key per account, with the owner's puuid and the fetch time (a second
//!   account never erases the first one's copy), so the screen still shows with
//!   the game closed. Only ids are stored: names, weapons, editions and
//!   prices come from valorant-api's skin catalog
//!   (`AssetCache::collection`), cached on disk per valorant-api version.
//! - `None` = no ask was made for this account yet (VALORANT has not run
//!   since it was first seen), so the screen shows its "no collection yet"
//!   empty state. A failed ask is `Err` instead: the screen must not tell the
//!   user to open the page while the game runs when it already was running.

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

/// The least time between two asks to Riot.
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

/// The last ask to Riot: for whom and when (process memory only, so a
/// restart may ask once more).
static LAST_ASK: Mutex<Option<(String, Instant)>> = Mutex::new(None);
/// One refresh at a time: a second `collection` call waits for the first
/// and then finds it fresh.
static REFRESH: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Whether Riot may be asked again for `puuid` at `now`.
fn due(last: Option<&(String, Instant)>, puuid: &str, now: Instant) -> bool {
    match last {
        Some((who, at)) if who == puuid => now.saturating_duration_since(*at) >= REFRESH_EVERY,
        _ => true,
    }
}

/// The user's owned skins; `None` while no ask was made for this account yet
/// (the game has not run since it was first seen), `Err` when an ask was made
/// and failed.
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
    let unanswered = refresh_if_due(st, &me).await;

    let owner = me.clone();
    let stored = st
        .db(move |repo| Ok(load_stored(repo, &owner)))
        .await
        .flatten();
    let Some(stored) = stored else {
        // An ask that failed is not "never fetched": the screen shows its
        // retry state and asks again in a minute, instead of the empty state
        // that tells the user to open the page while VALORANT runs.
        return if unanswered {
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
/// last stored ask is [`REFRESH_EVERY`] old, and stores the answer. Every
/// failure is logged and leaves the stored copy as it was.
///
/// Returns whether an ask was made and did not end in a stored answer (a Riot
/// error, a request hold, or a store that did not happen). Not an ask at all
/// (no session, the game closed, the last answer too recent) is `false`, so
/// the caller keeps the "no collection yet" state for those.
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
        // R160: a hold is "not now", not an answer; nothing was sent and the
        // next ask tries again. The screen still retries rather than claiming
        // the collection was never fetched.
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
    // Only a stored answer counts as the ten-minute refresh: a failure must
    // not keep the screen's next retry from reaching Riot.
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
    /// `body` as JSON when given.
    async fn mount_store(
        h: &crate::pipeline_tests::Harness,
        status: u16,
        times: Option<u64>,
        body: Option<serde_json::Value>,
    ) {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, ResponseTemplate};

        let mut template = ResponseTemplate::new(status);
        if let Some(body) = body {
            template = template.set_body_json(body);
        }
        let mut mock = Mock::given(method("GET"))
            .and(path(store_key_path()))
            .respond_with(template);
        if let Some(n) = times {
            mock = mock.up_to_n_times(n);
        }
        mock.mount(&h.server).await;
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
        mount_store(&h, 500, Some(1), None).await;
        mount_store(&h, 200, None, Some(one_level())).await;

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
    }

    /// No ask at all (no session, or the game closed and the session stale,
    /// R46) stays the "no collection yet" empty state: `Ok(None)`, Riot not
    /// asked. Only a failed ask is an error.
    #[tokio::test]
    async fn no_ask_at_all_is_still_the_empty_state() {
        let _serial = COMMANDS.lock().await;
        let h = crate::pipeline_tests::harness().await;
        *lock(&LAST_ASK) = None;
        // A session exists, but the game is closed: nothing is asked.
        h.st.set_session(Some(h.session.clone()));
        lock(&h.st.live).stage = Stage::Closed;
        mount_store(&h, 200, None, Some(one_level())).await;
        assert!(collection_for(&h.st).await.unwrap().is_none());
        assert_eq!(store_requests(&h, &store_key_path()).await, 0);

        // No session at all (Riot client gone) is the same state.
        h.st.set_session(None);
        lock(&h.st.live).stage = Stage::Menus;
        assert!(collection_for(&h.st).await.unwrap().is_none());
        assert_eq!(store_requests(&h, &store_key_path()).await, 0);
    }

    /// An ask that failed with no stored copy is an error, not `None`: the
    /// screen shows its retry state instead of "No collection yet" telling
    /// the user to open the page while VALORANT is already running.
    #[tokio::test]
    async fn a_failed_ask_is_not_reported_as_never_fetched() {
        let _serial = COMMANDS.lock().await;
        let h = crate::pipeline_tests::harness().await;
        h.st.set_session(Some(h.session.clone()));
        lock(&h.st.live).stage = Stage::Menus;
        *lock(&LAST_ASK) = None;
        mount_store(&h, 500, None, None).await;
        assert_eq!(
            collection_for(&h.st).await.expect_err("a 500 is a failure"),
            "skins unavailable"
        );
        assert_eq!(store_requests(&h, &store_key_path()).await, 1);
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
