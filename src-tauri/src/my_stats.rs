//! My Stats on the backend (B2): the DB-only computation behind the
//! `my_stats` command, and the `"mystats"` push that keeps the page current.
//!
//! The UI calls `my_stats` once at boot and then listens for `"mystats"`, whose
//! payload is the same `MyStats` JSON. [`notify`] is the one door every change
//! of the user's own data goes through (their own rank and matches phases, the
//! own-history backfills, Player Details of themselves, the act changing or
//! the asset tables loading). It never computes on the caller's task: a
//! detached task recomputes from the DB and emits **only** when the serialized
//! bytes differ from the last emission (the `AppState::publish` rule), so a
//! step that changed nothing costs one DB read and no event.
//!
//! One task at a time: a notification that arrives while it runs sets a flag
//! and the same task reads once more, so a burst (a backfill's pages) is one
//! or two reads, and emissions leave in the order of the reads, each read
//! strictly after the change that asked for it. Nothing here touches the poll
//! loop's presence tick.

use std::collections::HashMap;
use std::sync::Arc;

use opd1_core::model::MyStats;

use crate::state::{lock, now_ms, AppState};

/// Where the push stands, behind one lock (`AppState::my_stats_push`).
#[derive(Debug, Default)]
pub struct MyStatsPush {
    /// The bytes of the last emission: what a recompute is compared with.
    last: Vec<u8>,
    /// A push task is running (never more than one).
    running: bool,
    /// Something changed since the running task last started a read.
    dirty: bool,
    /// How many times My Stats was computed for a push (tests count them).
    #[cfg(test)]
    pub computed: usize,
}

/// Clears `running` however the task ends — a panic included — so a later
/// [`notify`] can start a new one.
struct RunningGuard<'a>(&'a AppState);

impl Drop for RunningGuard<'_> {
    fn drop(&mut self) {
        lock(&self.0.my_stats_push).running = false;
    }
}

/// My Stats for the current act, from the DB only (no Riot call, no table
/// download): `None` until the user's puuid is known (first run) or while no
/// act is current (the asset tables are not loaded yet).
pub async fn compute(st: &Arc<AppState>) -> Result<Option<MyStats>, String> {
    let Some(me) = lock(&st.live).self_puuid.clone() else {
        return Ok(None);
    };
    let Some(act) = st.current_act() else {
        return Ok(None);
    };
    let map_names: HashMap<String, String> = st
        .assets
        .meta()
        .map(|meta| meta.maps.into_iter().map(|(id, m)| (id, m.name)).collect())
        .unwrap_or_default();
    let assets = st.assets.clone();
    let now = now_ms();
    st.db(move |repo| {
        let lookup = |path: &str| {
            crate::roster::map_uuid(path, |p| assets.map_uuid_for_path(p).ok().flatten())
        };
        crate::store::load_my_stats(repo, &me, &act, now, &lookup, &map_names, &local_offset_ms)
    })
    .await
    .map(Some)
    .ok_or_else(|| "database unavailable".to_string())
}

/// The machine's UTC offset (ms) at `utc_ms` — per instant, so a match
/// on the other side of a DST change lands in the right hour.
pub(crate) fn local_offset_ms(utc_ms: i64) -> i64 {
    use chrono::{Local, Offset, TimeZone};
    Local
        .timestamp_millis_opt(utc_ms)
        .single()
        .map_or(0, |t| i64::from(t.offset().fix().local_minus_utc()) * 1000)
}

/// The user's own data changed (or My Stats may just have become
/// computable): recompute in the background and emit `"mystats"` if the
/// result differs from the last emission. Cheap to call, from any task.
pub fn notify(st: &Arc<AppState>) {
    {
        let mut push = lock(&st.my_stats_push);
        push.dirty = true;
        if push.running {
            return;
        }
        push.running = true;
    }
    let st = st.clone();
    tauri::async_runtime::spawn(async move { push_until_clean(&st).await });
}

/// [`notify`] when `puuid` is the user (the session's account): the other
/// players of a roster change nothing My Stats shows.
pub fn notify_if_own(st: &Arc<AppState>, own: &str, puuid: &str) {
    if own == puuid {
        notify(st);
    }
}

async fn push_until_clean(st: &Arc<AppState>) {
    let _running = RunningGuard(st);
    loop {
        {
            let mut push = lock(&st.my_stats_push);
            if !push.dirty {
                push.running = false;
                return;
            }
            push.dirty = false;
            #[cfg(test)]
            {
                push.computed += 1;
            }
        }
        let stats = match compute(st).await {
            Ok(Some(stats)) => stats,
            // Not computable yet: whatever makes it so notifies again.
            Ok(None) => continue,
            Err(e) => {
                log::warn!("my stats push: {e}");
                continue;
            }
        };
        let bytes = serde_json::to_vec(&stats).unwrap_or_default();
        let mut push = lock(&st.my_stats_push);
        if bytes == push.last {
            continue;
        }
        push.last = bytes;
        log::debug!("emit mystats");
        st.emit_my_stats(&stats);
    }
}

/// Waits until no push is running or wanted (tests: a notification has been
/// fully handled, its emission included).
#[cfg(test)]
pub async fn idle(st: &AppState) {
    crate::pipeline_tests::wait_until("the my stats push is idle", || {
        let push = lock(&st.my_stats_push);
        !push.running && !push.dirty
    })
    .await;
}
