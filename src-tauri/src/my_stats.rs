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
//! loop's presence tick. A slow tick ([`start_periodic`]) asks again every
//! [`PERIOD`] so what depends on the clock alone (a session that expired, a
//! last-7-days window) catches up; the byte comparison makes it free when
//! nothing changed.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use opd1_core::model::MyStats;
use tokio::time::{interval, MissedTickBehavior};

use crate::state::{lock, now_ms, AppState};

/// How often the slow tick asks for a recompute (15 minutes).
pub const PERIOD: Duration = Duration::from_secs(15 * 60);

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

/// Clears `running` when the task dies — a panic — so a later [`notify`] can
/// start a new one. A clean exit clears it itself, under the same lock as its
/// "nothing is dirty" check, and disarms the guard: clearing it again after
/// that lock was released could land on a task a `notify` had started in
/// between and let a third run beside it.
struct RunningGuard<'a> {
    st: &'a AppState,
    armed: bool,
}

impl Drop for RunningGuard<'_> {
    fn drop(&mut self) {
        if self.armed {
            lock(&self.st.my_stats_push).running = false;
        }
    }
}

/// My Stats for the current act, from the DB only (no Riot call, no table
/// download): `None` until the user's puuid is known (first run) or while no
/// act is current (the asset tables are not loaded yet).
pub async fn compute(st: &Arc<AppState>) -> Result<Option<MyStats>, String> {
    Ok(compute_for_user(st).await?.map(|(_, stats)| stats))
}

/// [`compute`], with the puuid it was computed for: the push drops a result
/// whose account is no longer the current one.
async fn compute_for_user(st: &Arc<AppState>) -> Result<Option<(String, MyStats)>, String> {
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
    let owner = me.clone();
    st.db(move |repo| {
        let lookup = |path: &str| {
            crate::roster::map_uuid(path, |p| assets.map_uuid_for_path(p).ok().flatten())
        };
        crate::store::load_my_stats(repo, &me, &act, now, &lookup, &map_names, &local_offset_ms)
    })
    .await
    .map(|stats| Some((owner, stats)))
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

/// The slow tick: once, at startup, from `setup`. Every [`PERIOD`] it asks
/// for a recompute, which costs one DB read and no event when nothing
/// changed. Never driven by the presence tick.
pub fn start_periodic(st: &Arc<AppState>) {
    tauri::async_runtime::spawn(periodic(st.clone(), PERIOD));
}

pub(crate) async fn periodic(st: Arc<AppState>, every: Duration) {
    let mut tick = interval(every);
    // A machine that slept does not owe the missed ticks: one is enough.
    tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
    // The first tick is immediate; the startup's own notifications cover it.
    tick.tick().await;
    loop {
        tick.tick().await;
        notify(&st);
    }
}

async fn push_until_clean(st: &Arc<AppState>) {
    let mut running = RunningGuard { st, armed: true };
    loop {
        {
            let mut push = lock(&st.my_stats_push);
            if !push.dirty {
                push.running = false;
                running.armed = false;
                return;
            }
            push.dirty = false;
            #[cfg(test)]
            {
                push.computed += 1;
            }
        }
        let (owner, stats) = match compute_for_user(st).await {
            Ok(Some(computed)) => computed,
            // Not computable yet: whatever makes it so notifies again.
            Ok(None) => continue,
            Err(e) => {
                log::warn!("my stats push: {e}");
                continue;
            }
        };
        // An account switch while the read ran: these are the old account's
        // stats. Dropped, and read again for the new one (checked before the
        // push lock is taken: the model's lock is never nested inside it).
        let current = lock(&st.live).self_puuid.clone();
        let bytes = serde_json::to_vec(&stats).unwrap_or_default();
        let mut push = lock(&st.my_stats_push);
        if current.as_deref() != Some(owner.as_str()) {
            push.dirty = true;
            continue;
        }
        if bytes == push.last {
            continue;
        }
        log::debug!("emit mystats");
        st.emit_my_stats(&stats);
        // After the emit: a sink that panics leaves the bytes unrecorded, so
        // the next notification emits them.
        push.last = bytes;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline_tests::harness;

    #[tokio::test]
    async fn a_clean_exit_never_clears_a_newer_tasks_running_flag() {
        // The clean exit clears `running` itself, under the lock. A task that
        // a `notify` started right after that must keep its flag when the
        // first one's guard is finally dropped.
        let h = harness().await;
        lock(&h.st.my_stats_push).running = true; // "task B"
        drop(RunningGuard {
            st: &h.st,
            armed: false,
        });
        assert!(lock(&h.st.my_stats_push).running);

        // A task that dies (a panic) clears it, so the next notify can start.
        drop(RunningGuard {
            st: &h.st,
            armed: true,
        });
        assert!(!lock(&h.st.my_stats_push).running);
    }
}
