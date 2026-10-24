//! `collection` (R189, user 2026-10-22): the user's full owned weapon skin
//! collection, with standard VP prices per edition.
//!
//! - Riot's owned skin levels (`pd …/store/v1/entitlements/{self}/{skin
//!   levels}`, one read-only `GET` in the `other` throttle family) are asked
//!   only while VALORANT runs, at most once per [`REFRESH_EVERY`] and only
//!   when the Skins screen asks (never from the poll loop). A call refused by
//!   a request hold is not counted: the next ask tries again.
//! - The owned level ids are stored in `app_state` under [`STORE_KEY`] with
//!   the owner's puuid and the fetch time, so the screen still shows with
//!   the game closed. Only ids are stored: names, weapons, editions and
//!   prices come from valorant-api's skin catalog
//!   (`AssetCache::collection`), cached on disk per valorant-api version.
//! - `None` = never fetched for this account.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use opd1_core::presence::Stage;
use opd1_core::wire::Collection;
use opd1_riot::endpoints::SKIN_LEVEL_ITEM_TYPE;
use opd1_riot::session::RiotError;

use super::AppStateRef;
use crate::state::{lock, now_ms, AppState};

/// The key in `app_state`.
pub const STORE_KEY: &str = "self_collection";

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

/// The user's owned skins; `None` while they were never fetched (the game
/// has not run since this account was first seen).
#[tauri::command]
pub async fn collection(state: AppStateRef<'_>) -> Result<Option<Collection>, String> {
    let st = state.inner().clone();
    let Some(me) = lock(&st.live).self_puuid.clone() else {
        return Ok(None);
    };
    refresh_if_due(&st, &me).await;

    let owner = me.clone();
    let stored = st
        .db(move |repo| repo.get_state(STORE_KEY))
        .await
        .flatten()
        .and_then(|json| StoredOwned::parse(&json, &owner));
    let Some(stored) = stored else {
        return Ok(None);
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
/// last ask is [`REFRESH_EVERY`] old, and stores the answer. Every failure
/// is logged and leaves the stored copy as it was.
async fn refresh_if_due(st: &Arc<AppState>, me: &str) {
    let Some(session) = st.session() else {
        return;
    };
    // R46: the session is stale while the game is closed.
    if lock(&st.live).stage == Stage::Closed || session.self_puuid() != me {
        return;
    }
    let _one = REFRESH.lock().await;
    if !due(lock(&LAST_ASK).as_ref(), me, Instant::now()) {
        return;
    }
    let answer = session.owned_skin_levels().await;
    if let Err(RiotError::Held(left)) = &answer {
        log::debug!("owned skins not asked: hold, {left:?} left");
        return;
    }
    *lock(&LAST_ASK) = Some((me.to_string(), Instant::now()));
    let levels = match answer {
        Ok(resp) => resp.item_ids(SKIN_LEVEL_ITEM_TYPE),
        Err(e) => {
            log::warn!("owned skins: {e}");
            return;
        }
    };
    log::debug!("owned skins: {} levels", levels.len());
    let stored = StoredOwned {
        puuid: me.to_string(),
        fetched_ms: now_ms(),
        levels,
    };
    let Ok(json) = serde_json::to_string(&stored) else {
        return;
    };
    let owner = st.clone();
    let me = me.to_string();
    st.db(move |repo| {
        // An account switch while Riot answered: not this account's list.
        if lock(&owner.live).self_puuid.as_deref() != Some(me.as_str()) {
            return Ok(());
        }
        repo.set_state(STORE_KEY, &json)
    })
    .await;
}

#[cfg(test)]
mod tests {
    use super::*;

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
