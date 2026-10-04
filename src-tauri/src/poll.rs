//! The live poll loop — one Tokio task started in `setup`, restarted by a
//! supervisor ([`supervise`]) if it ever panics.
//!
//! ```text
//!            lockfile missing (check every 5s — the ONLY idle work)
//!   ┌──────── Disconnected ◄───────────────────────────────┐
//!   │  lockfile present → RiotSession::connect             │ ClientGone /
//!   │  (fail → back off 5s,10s,20s,40s,60s; a rewritten    │ 5 presence errors /
//!   │   lockfile retries at once — R47)                    │ VALORANT (re)started
//!   ▼                                                      │ under a game-less
//! Connected: own Riot ID from /chat/v1/session (R43)       │ session (R46)
//!   own loadout once, at the first VALORANT presence (R90) │
//!   presence (local) every 2s (5s while closed) ───────────┘
//!   │  own VALORANT presence absent twice → stage Closed (own card from DB)
//!   │  view_stage(sessionLoopState, queueId) change → plan_transition():
//!   │     MENUS   → party roster   (+ re-fetch on partyId/partySize change)
//!   │     PREGAME → ally team      (+ re-poll every 3s for agent picks, R39;
//!   │                               ally skins until a valid answer, R83)
//!   │     INGAME  → both teams + loadouts
//!   │     PREGAME/INGAME in a free-for-all queue → stays MENUS (R41)
//!   │     INGAME→MENUS = match end: back to Lobby + post-match follow-up
//!   │        (kept through a quick requeue, R48), and the own-history
//!   │        backfill starts (R101; also on connect)
//!   └─ roster → cached-first cards (DB) → emit → worker syncs
//!      self → party → allies → enemies, re-emitting after each player
//! ```
//!
//! A failed roster fetch never leaves the previous stage's players on
//! screen: the new stage is emitted immediately with just the user's own
//! card, and the fetch is retried with backoff.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use tauri::async_runtime::JoinHandle;
use tokio::time::{sleep, sleep_until, Instant};

use opd1_core::presence::{decode_presence, Stage};
use opd1_riot::{RiotError, RiotSession};

use crate::logbuf::short;
use crate::roster::{
    connect_retry_delay, extract_loadouts, extract_pregame_loadouts, fill_tiers, from_coregame,
    from_party, from_pregame, is_ffa_roster, order_players, own_valorant_private, party_key,
    plan_transition, pregame_tiers, view_stage, want_pregame_loadouts, GameVerdict, GameWatch,
    PartyKey, RosterFetch, RosterPlayer,
};
use crate::state::{closed_self_slot, lock, now_ms, AppState, Slot};
use crate::store::{
    load_player_db, match_landed, save_player, PlayerDb, SELF_LOADOUT_KEY, SELF_PUUID_KEY,
};

/// Closed: how often the lockfile's existence is checked.
const CLOSED_CHECK: Duration = Duration::from_secs(5);
/// Presence cadence while VALORANT is running / while it isn't.
const PRESENCE_EVERY: Duration = Duration::from_secs(2);
const PRESENCE_EVERY_IDLE: Duration = Duration::from_secs(5);
/// R39: Agent Select re-poll, only to pick up agent selections.
const PREGAME_REPOLL: Duration = Duration::from_secs(3);
/// Consecutive presence failures (other than ClientGone) before the
/// session is dropped and rebuilt.
const PRESENCE_ERRORS_TO_RECONNECT: u32 = 5;
/// R164: presence can say PREGAME / INGAME a moment before the player
/// endpoint answers, so the first [`ROSTER_RETRY_FLAT_TRIES`] retries come
/// every [`ROSTER_RETRY_FLAT`]; after that the backoff is 3s, 6s, 12s, … capped.
const ROSTER_RETRY_FLAT: Duration = Duration::from_secs(2);
const ROSTER_RETRY_FLAT_TRIES: u32 = 5;
const ROSTER_RETRY_BASE: Duration = Duration::from_secs(3);
const ROSTER_RETRY_MAX: Duration = Duration::from_secs(30);
/// After a match ends, Riot needs a moment to publish it: re-sync the party
/// at about +10, +20, +40, +80 and +160 s (each gap is the delay since the
/// previous check) until the ended match has landed (R48: for competitive,
/// its `comp_updates` row — RR/rank — not just its details; R164). Each check
/// is one DB lookup plus the existing re-sync, and the first success stops it.
const POST_MATCH_DELAYS: [Duration; 5] = [
    Duration::from_secs(10),
    Duration::from_secs(10),
    Duration::from_secs(20),
    Duration::from_secs(40),
    Duration::from_secs(80),
];

/// R164: the In Game loadouts only feed Player Details, and the poll loop
/// awaits them inline after the roster is on screen, so a call stuck in a
/// throttle hold or a slow answer is cut off after this long (log and skip).
const INGAME_LOADOUTS_TIMEOUT: Duration = Duration::from_secs(3);

/// The wait before the next roster retry after `failures` failed fetches
/// (the first one counts; `failures >= 1`).
fn roster_retry_delay(failures: u32) -> Duration {
    if failures <= ROSTER_RETRY_FLAT_TRIES {
        return ROSTER_RETRY_FLAT;
    }
    ROSTER_RETRY_BASE
        .saturating_mul(1 << (failures - ROSTER_RETRY_FLAT_TRIES - 1).min(4))
        .min(ROSTER_RETRY_MAX)
}

/// Why the connected tick stopped.
#[derive(Debug)]
pub(crate) enum Stop {
    /// Drop the session and go back to Disconnected.
    Reconnect(&'static str),
}

struct PostMatch {
    match_id: String,
    /// Wait for the RR row (`comp_updates`), not just the match (R48).
    competitive: bool,
    due: Instant,
    attempt: usize,
}

/// How the supervisor ([`supervise`]) paces the restarts of a poll loop that
/// panicked: the first restart waits `delays[0]`, the second `delays[1]`, every
/// later one `delays[2]`; a poller that ran at least `healthy_after` before it
/// panicked starts the count over (it was a one-off, not a crash loop).
#[derive(Debug, Clone, Copy)]
struct RestartPacing {
    delays: [Duration; 3],
    healthy_after: Duration,
}

/// 1 s, then 5 s, then 30 s; a run of a minute resets it.
const POLLER_RESTARTS: RestartPacing = RestartPacing {
    delays: [
        Duration::from_secs(1),
        Duration::from_secs(5),
        Duration::from_secs(30),
    ],
    healthy_after: Duration::from_secs(60),
};

impl RestartPacing {
    /// The wait after the `failures`-th panic in a row (counted from 1).
    fn delay(&self, failures: u32) -> Duration {
        let last = self.delays.len() - 1;
        self.delays[(failures.saturating_sub(1) as usize).min(last)]
    }
}

/// The text of a task's panic payload (`panic!("…")` gives a `&str` or a
/// `String`; anything else has no text to show).
fn panic_text(payload: &(dyn std::any::Any + Send)) -> &str {
    payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("no message")
}

/// Runs `make()` as its own task and starts a fresh one whenever that task
/// panics, so one bug in the poll loop cannot freeze every card for the rest
/// of the session (release builds unwind, R138: a panic ends the task, not the
/// app). Each restart waits as `pacing` says; `on_panic(message, wait)` runs
/// right at the panic, before the wait (log it, reset what the dead loop left
/// behind), and the future it returns is awaited to its end before the wait
/// starts, so whatever it does is ordered before the next loop (review round 2:
/// the Closed card's DB read must not land after the new poller's first
/// publish). A task that returns normally ends the supervisor; so does a
/// cancelled one (the runtime is shutting down).
async fn supervise<F, Fut, P, Pf>(make: F, pacing: RestartPacing, mut on_panic: P)
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = ()> + Send + 'static,
    P: FnMut(&str, Duration) -> Pf,
    Pf: std::future::Future<Output = ()>,
{
    let mut failures = 0u32;
    loop {
        let started = Instant::now();
        let error = match tokio::spawn(make()).await {
            Ok(()) => return,
            Err(e) if e.is_panic() => e,
            Err(_) => return,
        };
        if started.elapsed() >= pacing.healthy_after {
            failures = 0;
        }
        failures = failures.saturating_add(1);
        let wait = pacing.delay(failures);
        let payload = error.into_panic();
        on_panic(panic_text(&*payload), wait).await;
        sleep(wait).await;
    }
}

/// What the dead poll loop left in the shared model. Its worker task is
/// aborted by the poller's own drop ([`Poller`]'s `Drop`); the claim is
/// revoked here too, so nothing waits on it. Its backfill run and its session
/// are the dead loop's: the new one connects again from the lockfile like at
/// startup, and the cursor of an interrupted run is never trusted (R101).
fn reset_after_panic(st: &AppState) {
    {
        let mut live = lock(&st.live);
        live.revoke_worker();
        live.backfill = None;
        live.comp_backfill = None;
    }
    st.set_session(None);
}

pub fn start(st: Arc<AppState>) {
    spawn_supervised(
        st.clone(),
        // The poller is built inside the task, so even `new` is covered.
        move || {
            let st = st.clone();
            async move { Poller::new(st).run().await }
        },
        POLLER_RESTARTS,
    );
}

/// [`start`]'s wiring with the loop a parameter (the test seam): `make` under
/// the supervisor, and at each panic the error line, [`reset_after_panic`],
/// and the Closed view — the user's own card from the DB, published at once —
/// so the cards of the dead loop are not what the UI shows during the wait.
/// The Closed view is finished before the restart wait begins (review round
/// 2): it was a detached task, so a slow DB read could land after the new
/// poller's first publish and put Closed over its roster.
fn spawn_supervised<F, Fut>(st: Arc<AppState>, make: F, pacing: RestartPacing) -> JoinHandle<()>
where
    F: Fn() -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    tauri::async_runtime::spawn(supervise(make, pacing, move |message, wait| {
        log::error!(
            "the poll loop panicked ({message}); restarting in {} s",
            wait.as_secs()
        );
        reset_after_panic(&st);
        let st = st.clone();
        async move {
            // Its own task, awaited: a panic inside it ends that task, not
            // the supervisor (which must start the loop again).
            let _ = tauri::async_runtime::spawn(async move { show_closed(&st).await }).await;
        }
    }))
}

pub(crate) struct Poller {
    st: Arc<AppState>,
    local_app_data: Option<PathBuf>,
    session: Option<Arc<RiotSession>>,
    /// The stage being shown (`roster::view_stage` of the presence).
    stage: Stage,
    /// The raw presence stage last seen (differs from `stage` in FFA).
    presence_stage: Stage,
    /// VALORANT's own presence under this session (R46).
    game: GameWatch,
    /// The user's own loadout was asked for under this session (R90: once
    /// per connect, at the first presence that shows VALORANT running).
    self_loadout_asked: bool,
    /// R162: the content-service was asked for this session.
    content_asked: bool,
    /// The client version came from the user's VALORANT presence under this
    /// session (R159: re-resolved at the first presence that has one, once per
    /// connect — no extra request).
    version_from_presence_done: bool,
    /// The presence `queueId` while in a match — decides the post-match
    /// check for the match that just ended (R48).
    match_queue: Option<String>,
    presence_errors: u32,
    party_key: Option<PartyKey>,
    self_party: HashSet<String>,
    pregame_match_id: Option<String>,
    /// Riot answered `LoadoutsValid` for the current Agent Select: the
    /// pre-game loadouts are not asked for again (R83).
    pregame_loadouts_valid: bool,
    /// How long the In Game loadouts call may take (a field so a test can
    /// shorten it).
    pub(crate) ingame_loadouts_timeout: Duration,
    next_presence: Instant,
    next_pregame: Option<Instant>,
    /// When the next roster retry is due and how many fetches failed so far.
    roster_retry: Option<(Instant, u32)>,
    post_match: Option<PostMatch>,
    /// The sync worker's task and the id its claim in `LiveModel` has.
    worker: Option<(JoinHandle<()>, u64)>,
    /// Tests only: `ensure_worker` claims and starts nothing, so a test that
    /// looks at what a roster did on its own does not race a worker syncing on
    /// the app's runtime (aborting one later cannot undo a write it made).
    #[cfg(test)]
    pub(crate) hold_worker: bool,
    last_connect_error: Option<String>,
    /// R47: consecutive connect failures and when the next attempt is due.
    connect_failures: u32,
    next_connect: Instant,
    /// The lockfile's mtime at the last check — a rewritten lockfile (the
    /// Riot Client restarted) retries at once, whatever the backoff.
    lockfile_mtime: Option<SystemTime>,
}

impl Drop for Poller {
    /// A poller that dies — a panic unwinding its task — takes its worker with
    /// it: the handle is aborted, not left running detached.
    fn drop(&mut self) {
        if let Some((handle, _)) = self.worker.take() {
            handle.abort();
        }
    }
}

impl Poller {
    pub(crate) fn new(st: Arc<AppState>) -> Self {
        let local_app_data = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
        if local_app_data.is_none() {
            log::warn!("LOCALAPPDATA is not set; the Riot client can't be found");
        }
        Self {
            st,
            local_app_data,
            session: None,
            stage: Stage::Closed,
            presence_stage: Stage::Closed,
            game: GameWatch::default(),
            self_loadout_asked: false,
            content_asked: false,
            version_from_presence_done: false,
            match_queue: None,
            presence_errors: 0,
            party_key: None,
            self_party: HashSet::new(),
            pregame_match_id: None,
            pregame_loadouts_valid: false,
            ingame_loadouts_timeout: INGAME_LOADOUTS_TIMEOUT,
            next_presence: Instant::now(),
            next_pregame: None,
            roster_retry: None,
            post_match: None,
            worker: None,
            #[cfg(test)]
            hold_worker: false,
            last_connect_error: None,
            connect_failures: 0,
            next_connect: Instant::now(),
            lockfile_mtime: None,
        }
    }

    /// Tests only: a poller that starts no sync worker (`hold_worker`), for a
    /// test that looks at what a roster did on its own.
    #[cfg(test)]
    pub(crate) fn held(st: Arc<AppState>) -> Self {
        let mut poller = Self::new(st);
        poller.hold_worker = true;
        poller
    }

    async fn run(mut self) {
        log::info!("poll loop started");
        self.enter_closed().await;
        loop {
            match self.session.clone() {
                None => self.disconnected_tick().await,
                Some(session) => match self.connected_tick(&session).await {
                    Ok(()) => self.sleep_until_next().await,
                    Err(Stop::Reconnect(why)) => {
                        log::info!("dropping Riot session: {why}");
                        self.drop_session().await;
                    }
                },
            }
        }
    }

    // -- disconnected ----------------------------------------------------

    async fn disconnected_tick(&mut self) {
        let Some(lad) = self.local_app_data.clone() else {
            // No Riot Client can be found: the first verdict is "closed".
            self.settle();
            sleep(CLOSED_CHECK).await;
            return;
        };
        let lockfile = lockfile_path(&lad);
        let Ok(meta) = std::fs::metadata(&lockfile) else {
            // Riot Client closed: back to the plain 5s existence check. That
            // is the first verdict (VALORANT cannot be running without it).
            self.settle();
            self.last_connect_error = None;
            self.connect_failures = 0;
            self.lockfile_mtime = None;
            sleep(CLOSED_CHECK).await;
            return;
        };
        let mtime = meta.modified().ok();
        if mtime != self.lockfile_mtime {
            // New or rewritten lockfile (a client (re)start): try now.
            self.lockfile_mtime = mtime;
            self.connect_failures = 0;
            self.next_connect = Instant::now();
        }
        if Instant::now() < self.next_connect {
            sleep(CLOSED_CHECK).await;
            return;
        }
        match RiotSession::connect(&lad).await {
            Ok(session) => {
                self.connect_failures = 0;
                self.on_connected(Arc::new(session)).await;
            }
            Err(e) => {
                // A client that cannot be reached tells nothing about the game:
                // the first verdict is "closed" (a later success still moves on).
                self.settle();
                self.connect_failures = self.connect_failures.saturating_add(1);
                let delay = connect_retry_delay(self.connect_failures);
                self.next_connect = Instant::now() + delay;
                let msg = e.to_string();
                if self.last_connect_error.as_deref() != Some(msg.as_str()) {
                    log::info!(
                        "Riot client not ready: {msg} (retrying in {}s)",
                        delay.as_secs()
                    );
                    self.last_connect_error = Some(msg);
                }
                sleep(CLOSED_CHECK).await;
            }
        }
    }

    pub(crate) async fn on_connected(&mut self, session: Arc<RiotSession>) {
        let me = session.self_puuid().to_string();
        log::info!("connected to the Riot client as {}", short(&me));
        self.last_connect_error = None;
        let switched = {
            let mut live = lock(&self.st.live);
            let switched = live.self_puuid.as_deref() != Some(me.as_str());
            if switched {
                // A different account: nothing learned about the previous
                // one's session applies.
                live.synced.clear();
                live.party_ready.clear();
                live.names_attempted.clear();
                live.self_roster = None;
                live.self_name = None;
                live.self_loadout = None;
                live.seen_hidden.clear();
                // R101: the previous account's run (and its cursor) is not
                // this account's.
                live.backfill = None;
                // B1: nor is its competitive-updates run.
                live.comp_backfill = None;
                // R150: nor are its queued name asks and its last match end
                // (their hidden players were just forgotten above).
                live.clear_name_fill();
                live.match_end_ms = None;
                // R155: nor are its pending rank retries.
                live.rank_retries.clear();
                // R161: nor are the tiers of its last Agent Select.
                live.match_tiers.clear();
            }
            // R160: a new session has its own, empty request holds.
            live.reset_names_hold();
            live.self_puuid = Some(me.clone());
            live.self_name_attempted = false;
            switched
        };
        if switched {
            // B2: another account — its My Stats replace the old account's at
            // once (DB only), not after the asset tables are re-checked below
            // and not after the push's trailing wait either.
            crate::my_stats::notify_now(&self.st);
        }
        let key_value = me.clone();
        self.st
            .db(move |repo| {
                repo.set_state(SELF_PUUID_KEY, &key_value)?;
                if switched {
                    // The previous account's loadout copy must not show for
                    // this one: no key is no stored loadout. (The backfill
                    // anchors are per account, R137: nothing to delete.)
                    repo.delete_state(SELF_LOADOUT_KEY)?;
                }
                Ok(())
            })
            .await;
        self.st.set_session(Some(session.clone()));
        self.session = Some(session.clone());
        self.game = GameWatch::default();
        // R90: asked at this session's first presence showing VALORANT.
        self.self_loadout_asked = false;
        // R162: and the current act.
        self.content_asked = false;
        // R159: so is the client version, from that same presence.
        self.version_from_presence_done = false;
        self.presence_errors = 0;
        self.next_presence = Instant::now();
        // R43: the user's own Riot ID, from the local chat session.
        let _ = crate::sync::fetch_self_name(&self.st, &session).await;

        // One cheap /v1/version check per client start (new patch → new
        // tables); the tables themselves only download when it changed.
        let st = self.st.clone();
        tauri::async_runtime::spawn(async move {
            if st.ensure_tables(true).await.is_ok() {
                st.publish();
                // B2: with the user's puuid known (just above), the tables
                // may be what makes My Stats computable.
                crate::my_stats::notify(&st);
            }
        });
        // The user's own card, from the DB, while the first presence
        // arrives (closed-stage card if the game isn't up).
        self.enter_closed().await;
        // R101: catch up on the matches played while the app was closed.
        // Only the run is queued here; R137: no worker until VALORANT is
        // running. A session built while the game is closed is stale (R46:
        // it is rebuilt, and the run dropped, when the game starts), so a
        // worker now would sync the Closed slot and page history for
        // nothing. The first presence that settles a stage installs the
        // roster, whose `ensure_worker` runs the queued run behind the live
        // cards (`LiveModel::next_job`).
        crate::sync::start_own_backfills(&self.st).await;
    }

    /// R101: a match ended (VALORANT is running): starts the own-history
    /// backfill from the stored anchor (a run already in progress keeps
    /// going) and makes sure a worker will run it — behind any live card
    /// work, which `LiveModel::next_job` puts first.
    async fn begin_backfill(&mut self, session: &Arc<RiotSession>) {
        crate::sync::start_own_backfills(&self.st).await;
        self.ensure_worker(session);
    }

    async fn drop_session(&mut self) {
        self.stop_worker();
        // R101: an interrupted run is dropped; the next connect starts over
        // from the stored anchor, which only a finished run moves. B1: the
        // competitive-updates run likewise.
        {
            let mut live = lock(&self.st.live);
            live.backfill = None;
            live.comp_backfill = None;
        }
        self.session = None;
        self.st.set_session(None);
        self.stage = Stage::Closed;
        self.pregame_match_id = None;
        self.next_pregame = None;
        self.roster_retry = None;
        self.post_match = None;
        self.party_key = None;
        self.match_queue = None;
        self.presence_stage = Stage::Closed;
        // A deliberate reconnect (e.g. VALORANT started) goes straight to
        // `connect`, not through the failure backoff.
        self.connect_failures = 0;
        self.next_connect = Instant::now();
        self.enter_closed().await;
    }

    // -- connected -------------------------------------------------------

    async fn connected_tick(&mut self, session: &Arc<RiotSession>) -> Result<(), Stop> {
        // R162: when the act Riot named has ended (or it never answered),
        // ask again; a no-op otherwise. Never while VALORANT is closed: the
        // session is stale then (R46/R137), like every other remote call.
        if self.stage != Stage::Closed {
            self.st.spawn_content_refresh(false);
        }
        let now = Instant::now();
        if now >= self.next_presence {
            self.presence_tick(session).await?;
            let every = if self.stage == Stage::Closed {
                PRESENCE_EVERY_IDLE
            } else {
                PRESENCE_EVERY
            };
            self.next_presence = Instant::now() + every;
        }
        if self.stage == Stage::Pregame && self.next_pregame.is_some_and(|t| Instant::now() >= t) {
            self.next_pregame = Some(Instant::now() + PREGAME_REPOLL);
            self.pregame_repoll(session).await?;
        }
        if let Some((due, attempt)) = self.roster_retry {
            if Instant::now() >= due {
                self.roster_retry = None;
                if !self.fetch_roster(session).await? {
                    let failures = attempt + 1;
                    self.roster_retry =
                        Some((Instant::now() + roster_retry_delay(failures), failures));
                }
            }
        }
        if self
            .post_match
            .as_ref()
            .is_some_and(|p| Instant::now() >= p.due)
        {
            self.post_match_check(session).await;
        }
        // R150: names-fill asks queued by a command (Player Details, Match
        // Details) need a worker, and only this loop owns the worker's
        // handle. Not while VALORANT is closed (R137: no worker on a stale
        // session); the next roster install starts one then.
        if self.st.take_worker_kick() && self.stage != Stage::Closed {
            self.ensure_worker(session);
        }
        // R155: a rank retry came due. The worker exits when nothing else is
        // queued and never waits for one, so this tick (every 2 s while
        // VALORANT runs) is what wakes it; a running worker takes it itself.
        if self.stage != Stage::Closed && lock(&self.st.live).due_rank_retry().is_some() {
            self.ensure_worker(session);
        }
        // R160: a names deferral (name-service was refused by a hold) ran out
        // and names or fill asks are waiting: the same wake-up, once.
        if self.stage != Stage::Closed && lock(&self.st.live).names_wakeup_due() {
            self.ensure_worker(session);
        }
        // B1: the same for the competitive-updates run, paused by the mmr
        // hold: its pause ran out.
        if self.stage != Stage::Closed && lock(&self.st.live).comp_backfill_resume_due() {
            self.ensure_worker(session);
        }
        Ok(())
    }

    async fn sleep_until_next(&self) {
        let mut next = self.next_presence;
        if self.stage == Stage::Pregame {
            if let Some(t) = self.next_pregame {
                next = next.min(t);
            }
        }
        if let Some((t, _)) = self.roster_retry {
            next = next.min(t);
        }
        if let Some(p) = &self.post_match {
            next = next.min(p.due);
        }
        sleep_until(next).await;
    }

    pub(crate) async fn presence_tick(&mut self, session: &Arc<RiotSession>) -> Result<(), Stop> {
        let presences = match session.presences().await {
            Ok(p) => {
                self.presence_errors = 0;
                p
            }
            Err(RiotError::ClientGone) => return Err(Stop::Reconnect("Riot client closed")),
            Err(e) => {
                self.presence_errors += 1;
                log::warn!("presence failed ({}x): {e}", self.presence_errors);
                if self.presence_errors >= PRESENCE_ERRORS_TO_RECONNECT {
                    return Err(Stop::Reconnect("presence keeps failing"));
                }
                return Ok(());
            }
        };
        let me = session.self_puuid();
        let private = own_valorant_private(&presences, me);
        let decoded = private.as_deref().and_then(decode_presence);

        match self.game.on_presence(decoded.is_some()) {
            GameVerdict::Present => {
                self.read_self_loadout_once(session);
                self.content_once();
                self.client_version_from_presence_once(session, private.as_deref());
            }
            // The session was built (or has lived) while VALORANT wasn't
            // running: stale shards / fallback client version. Rebuild it
            // now that the game is up — first launch or relaunch (R46).
            GameVerdict::Reconnect => return Err(Stop::Reconnect("VALORANT started")),
            GameVerdict::Absent => return Ok(()),
            GameVerdict::Closed => {
                if self.stage != Stage::Closed {
                    log::info!("VALORANT is not running");
                    self.change_stage(session, Stage::Closed, None).await?;
                }
                // Absent twice: the first verdict (or a later one, no change).
                self.settle();
                return Ok(());
            }
        }
        let Some(presence) = decoded else {
            return Ok(());
        };
        // R137: the raw presence, not the view — a free-for-all match keeps
        // the Lobby view (R41) but is still a match. Set before any stage
        // change below starts a worker.
        {
            let mut live = lock(&self.st.live);
            live.in_match = matches!(presence.stage, Stage::Pregame | Stage::Ingame);
            if live.in_match {
                // R150: a match is running, so no match end is on record: the
                // one that ended before it is history.
                live.match_end_ms = None;
            }
        }

        if presence.stage == Stage::Ingame {
            self.match_queue = presence.queue_id.clone();
        }
        // R101: a match just ended. The raw presence, not the shown stage:
        // a free-for-all keeps the Lobby view through the whole match, but
        // its history entry is just as new.
        let match_ended = self.presence_stage == Stage::Ingame && presence.stage == Stage::Menus;
        if match_ended {
            // R150: the main trigger of the names fill. On record before the
            // stage change below starts a worker, so the match's details —
            // stored by that worker's sync or by the backfill, whichever gets
            // them first (`sync::store_details`) — queue its players.
            lock(&self.st.live).match_end_ms = Some(now_ms());
        }
        let view = view_stage(presence.stage, presence.queue_id.as_deref());
        if presence.stage != self.presence_stage {
            if view != presence.stage {
                log::info!("free-for-all {:?}: keeping the lobby view", presence.stage);
            }
            self.presence_stage = presence.stage;
        }
        let key = private.as_deref().and_then(party_key);
        if view != self.stage {
            self.change_stage(session, view, key).await?;
        } else if key != self.party_key {
            self.party_key = key;
            if self.stage == Stage::Menus {
                log::info!("party changed");
                self.fetch_or_retry(session).await?;
            } else {
                // Keep the own-party set current for the in-match stripe
                // flags; the roster itself doesn't change mid-match.
                self.refresh_self_party(session).await?;
            }
        }
        if match_ended {
            // After the stage change above: the Lobby's roster is installed
            // (and its worker running) before the backfill is queued.
            self.begin_backfill(session).await;
        }
        // VALORANT is running and its stage is on screen: the first verdict.
        self.settle();
        Ok(())
    }

    /// R90: the user's own equipped skins, once per connect (one read-only
    /// pd call; a failure waits for the next connect) — at the first
    /// presence that shows VALORANT running, like the backfill (R137): a
    /// session built while the game is closed is stale and rebuilt when it
    /// starts (R46), and a connect that never sees VALORANT (the Riot Client
    /// alone, or a presence that keeps failing and reconnecting) asks
    /// nothing.
    fn read_self_loadout_once(&mut self, session: &Arc<RiotSession>) {
        if self.self_loadout_asked {
            return;
        }
        self.self_loadout_asked = true;
        let (st, s) = (self.st.clone(), session.clone());
        tauri::async_runtime::spawn(async move {
            let _ = crate::sync::fetch_self_loadout(&st, &s).await;
        });
    }

    /// R162: Riot's own current act, asked once per session at the first
    /// presence showing VALORANT (valorant-api's dates stand in until it
    /// answers, or if it never does). Not at connect: the session is stale
    /// while the game is closed (R46, R137).
    fn content_once(&mut self) {
        if self.content_asked {
            return;
        }
        self.content_asked = true;
        self.st.spawn_content_refresh(true);
    }

    /// R159: the first time this session sees the user's VALORANT presence
    /// *with a usable version*, its `partyClientVersion` (already in hand: the
    /// tick read the presences) replaces a client version the session took
    /// from a lower source when it was built before the game's presence
    /// existed. No request; the header value of the next remote call changes
    /// if the version did. A presence without one changes nothing and does not
    /// use up the turn (a tiny base64 + JSON read per tick until one has it),
    /// or a session would keep the lower value for its whole life.
    fn client_version_from_presence_once(
        &mut self,
        session: &Arc<RiotSession>,
        private: Option<&str>,
    ) {
        if self.version_from_presence_done {
            return;
        }
        if let Some(private) = private {
            session.note_presence_version(private);
            // The presence has spoken (now or at connect) exactly when it is
            // the source in use.
            self.version_from_presence_done =
                session.client_version_source() == opd1_riot::session::VersionSource::Presence;
        }
    }

    pub(crate) fn stop_worker(&mut self) {
        if let Some((handle, _)) = self.worker.take() {
            handle.abort();
        }
        lock(&self.st.live).revoke_worker();
    }

    /// Starts the sync worker when there is work and no worker holds the
    /// claim (R45: claim + work check happen under one lock in
    /// `LiveModel::try_start_worker`; a running worker picks up roster
    /// changes itself).
    fn ensure_worker(&mut self, session: &Arc<RiotSession>) {
        #[cfg(test)]
        if self.hold_worker {
            return;
        }
        // A worker task that ended without releasing its claim (it
        // panicked; release builds unwind, R138, so the task ends and the
        // app keeps running) must not block all future work.
        if let Some((handle, id)) = &self.worker {
            if handle.inner().is_finished() {
                lock(&self.st.live).release_worker(*id);
                self.worker = None;
            }
        }
        let Some(id) = lock(&self.st.live).try_start_worker() else {
            return;
        };
        let (st, s) = (self.st.clone(), session.clone());
        let handle = tauri::async_runtime::spawn(crate::sync::run_worker(st, s, id));
        self.worker = Some((handle, id));
    }

    async fn change_stage(
        &mut self,
        session: &Arc<RiotSession>,
        next: Stage,
        key: Option<PartyKey>,
    ) -> Result<(), Stop> {
        let Some(t) = plan_transition(self.stage, next) else {
            return Ok(());
        };
        log::info!("stage {:?} -> {:?}", self.stage, next);
        self.stop_worker();
        // R43: retry the own Riot ID until the chat session has it (the
        // guard is dropped before the await).
        let need_self_name = lock(&self.st.live).self_name.is_none();
        if need_self_name
            && crate::sync::fetch_self_name(&self.st, session)
                .await
                .is_err()
        {
            return Err(Stop::Reconnect("Riot client closed"));
        }

        let ended_match = {
            let mut live = lock(&self.st.live);
            let ended = live.coregame_match_id.take();
            live.loadouts.clear();
            if t.reset_synced {
                live.synced.clear();
                live.party_ready.clear();
                live.names_attempted.clear();
                // R165: the failure counts went with it; so does the wake-up.
                live.clear_names_retry_wake();
                // R161: a match is over or VALORANT closed.
                live.match_tiers.clear();
            }
            ended
        };
        self.post_match = match (t.match_ended, ended_match) {
            (true, Some(match_id)) => Some(PostMatch {
                match_id,
                // An unknown queue gets the stricter (competitive) check: at
                // worst two extra cheap re-syncs.
                competitive: self
                    .match_queue
                    .as_deref()
                    .is_none_or(|q| q == "competitive"),
                due: Instant::now() + POST_MATCH_DELAYS[0],
                attempt: 0,
            }),
            // The next match end replaces a pending follow-up (or ends it,
            // when that match's id is unknown).
            (true, None) => None,
            // VALORANT closed: nothing to re-sync.
            _ if next == Stage::Closed => None,
            // R48: a quick requeue (Lobby -> Agent Select -> In Game, or a
            // dodge back) keeps a pending follow-up: the user and party are
            // still `synced` from a Lobby sync that may predate Riot
            // publishing the match, and nothing else re-syncs them before
            // the next match ends.
            _ => self.post_match.take(),
        };
        self.stage = next;
        self.party_key = key;
        self.pregame_match_id = None;
        self.pregame_loadouts_valid = false;
        self.next_pregame = None;
        self.roster_retry = None;

        match t.fetch {
            RosterFetch::None => {
                self.enter_closed().await;
                Ok(())
            }
            _ => self.fetch_or_retry(session).await,
        }
    }

    /// Fetches the current stage's roster; on failure shows the stage with
    /// only the user's own card and schedules a retry.
    async fn fetch_or_retry(&mut self, session: &Arc<RiotSession>) -> Result<(), Stop> {
        if !self.fetch_roster(session).await? {
            self.show_self_only(session).await;
            self.roster_retry = Some((Instant::now() + roster_retry_delay(1), 1));
        }
        Ok(())
    }

    /// `Ok(true)` = roster applied; `Ok(false)` = fetch failed (retry).
    async fn fetch_roster(&mut self, session: &Arc<RiotSession>) -> Result<bool, Stop> {
        let me = session.self_puuid().to_string();
        let res = match self.stage {
            Stage::Closed => return Ok(true),
            Stage::Menus => self.fetch_party(session, &me).await,
            Stage::Pregame => self.fetch_pregame(session, &me).await,
            Stage::Ingame => self.fetch_ingame(session, &me).await,
        };
        match res {
            Ok(()) => Ok(true),
            Err(RiotError::ClientGone) => Err(Stop::Reconnect("Riot client closed")),
            Err(e) => {
                log::warn!("{:?} roster fetch failed: {e}", self.stage);
                Ok(false)
            }
        }
    }

    async fn party_response(
        &self,
        session: &RiotSession,
        me: &str,
    ) -> Result<opd1_riot::endpoints::PartyResponse, RiotError> {
        let party_id = match self.party_key.as_ref().and_then(|k| k.party_id.clone()) {
            Some(id) => id,
            None => session.party_of(me).await?.current_party_id,
        };
        session.party(&party_id).await
    }

    pub(crate) async fn fetch_party(
        &mut self,
        session: &Arc<RiotSession>,
        me: &str,
    ) -> Result<(), RiotError> {
        let party = self.party_response(session, me).await?;
        let roster = from_party(&party);
        self.self_party = roster.iter().map(|p| p.puuid.clone()).collect();
        // R161: a Lobby answer says nothing about a match to come.
        lock(&self.st.live).match_tiers.clear();
        log::info!("lobby: party of {}", roster.len());
        self.apply_roster(session, Stage::Menus, None, roster).await;
        Ok(())
    }

    /// Best-effort own-party refresh outside the lobby (e.g. the app was
    /// started mid-match and never saw the lobby). The roster isn't
    /// re-fetched mid-match, so the cards on screen get the new own-party
    /// flags here (no stripe and no ⟲ badge for the user's party) and are
    /// republished when one changed.
    pub(crate) async fn refresh_self_party(
        &mut self,
        session: &Arc<RiotSession>,
    ) -> Result<(), Stop> {
        let me = session.self_puuid().to_string();
        match self.party_response(session, &me).await {
            Ok(party) => {
                self.self_party = party.members.iter().map(|m| m.subject.clone()).collect();
                let changed = {
                    let mut live = lock(&self.st.live);
                    let mut changed = false;
                    for slot in &mut live.slots {
                        let p = &slot.roster.puuid;
                        let in_party = *p == me || self.self_party.contains(p);
                        if slot.roster.is_self_party != in_party {
                            slot.roster.is_self_party = in_party;
                            changed = true;
                        }
                    }
                    changed
                };
                if changed {
                    self.st.publish();
                }
                Ok(())
            }
            Err(RiotError::ClientGone) => Err(Stop::Reconnect("Riot client closed")),
            Err(e) => {
                log::warn!("party refresh failed: {e}");
                Ok(())
            }
        }
    }

    pub(crate) async fn fetch_pregame(
        &mut self,
        session: &Arc<RiotSession>,
        me: &str,
    ) -> Result<(), RiotError> {
        if self.self_party.is_empty() {
            if let Ok(party) = self.party_response(session, me).await {
                self.self_party = party.members.iter().map(|m| m.subject.clone()).collect();
            }
        }
        let match_id = session.pregame_player(me).await?.match_id;
        let m = session.pregame_match(&match_id).await?;
        let roster = from_pregame(&m, me, &self.self_party);
        lock(&self.st.live)
            .match_tiers
            .set_pregame(&match_id, pregame_tiers(&m));
        log::info!("agent select: {} allies", roster.len());
        if self.pregame_match_id.as_deref() != Some(match_id.as_str()) {
            // A new Agent Select: its loadouts start unknown.
            self.pregame_loadouts_valid = false;
        }
        self.pregame_match_id = Some(match_id);
        self.next_pregame = Some(Instant::now() + PREGAME_REPOLL);
        self.apply_roster(session, Stage::Pregame, Some(m.map_id), roster)
            .await;
        Ok(())
    }

    /// R39: re-read the pregame match only to pick up agent selections
    /// (applied in place; the worker isn't restarted). R83: on the same
    /// re-poll the allies' skins are asked for, until Riot answers valid.
    pub(crate) async fn pregame_repoll(&mut self, session: &Arc<RiotSession>) -> Result<(), Stop> {
        let Some(match_id) = self.pregame_match_id.clone() else {
            return Ok(());
        };
        match session.pregame_match(&match_id).await {
            Ok(m) => {
                let me = session.self_puuid().to_string();
                let roster = from_pregame(&m, &me, &self.self_party);
                lock(&self.st.live)
                    .match_tiers
                    .set_pregame(&match_id, pregame_tiers(&m));
                let allies: Vec<String> = roster.iter().map(|p| p.puuid.clone()).collect();
                self.apply_roster(session, Stage::Pregame, Some(m.map_id), roster)
                    .await;
                self.pregame_loadouts_step(session, &match_id, &allies)
                    .await
            }
            Err(RiotError::ClientGone) => Err(Stop::Reconnect("Riot client closed")),
            // Typically 404 right as Agent Select ends — presence moves on.
            Err(e) => {
                log::debug!("pregame re-poll: {e}");
                Ok(())
            }
        }
    }

    /// R83: at most one `pregame/.../loadouts` call per re-poll, and only
    /// while [`want_pregame_loadouts`]: not once Riot answered
    /// `LoadoutsValid`, nor when every ally's skins are already known. Riot
    /// nulls the payload until the picks are in, so the first answers can
    /// be invalid; a valid one is merged into `live.loadouts` (Details).
    async fn pregame_loadouts_step(
        &mut self,
        session: &Arc<RiotSession>,
        match_id: &str,
        allies: &[String],
    ) -> Result<(), Stop> {
        let want = {
            let live = lock(&self.st.live);
            want_pregame_loadouts(allies, &live.loadouts, self.pregame_loadouts_valid)
        };
        if !want {
            return Ok(());
        }
        match session.pregame_loadouts(match_id).await {
            Ok(resp) => {
                if resp.loadouts_valid {
                    self.pregame_loadouts_valid = true;
                }
                let found = extract_pregame_loadouts(&resp);
                if !found.is_empty() {
                    lock(&self.st.live).loadouts.extend(found);
                    // Nothing on a card shows skins, so no publish: an open
                    // Details of an ally is rebuilt (and its skins named).
                    crate::details::refresh_open_after_live_change(&self.st).await;
                }
                Ok(())
            }
            Err(RiotError::ClientGone) => Err(Stop::Reconnect("Riot client closed")),
            // Typically 404 right as Agent Select ends — presence moves on.
            Err(e) => {
                log::debug!("pregame loadouts: {e}");
                Ok(())
            }
        }
    }

    pub(crate) async fn fetch_ingame(
        &mut self,
        session: &Arc<RiotSession>,
        me: &str,
    ) -> Result<(), RiotError> {
        if self.self_party.is_empty() {
            if let Ok(party) = self.party_response(session, me).await {
                self.self_party = party.members.iter().map(|m| m.subject.clone()).collect();
            }
        }
        let match_id = session.coregame_player(me).await?.match_id;
        let m = session.coregame_match(&match_id).await?;
        if is_ffa_roster(&m) {
            // Presence had no usable queueId, but everyone is on their own
            // team: free-for-all keeps the lobby view (R41).
            log::info!("free-for-all roster: keeping the lobby view");
            // The Lobby roster goes on screen as Menus, but this is still a
            // match (R137): no backfill until it ends.
            lock(&self.st.live).in_match = true;
            return self.fetch_party(session, me).await;
        }
        let mut roster = from_coregame(&m, me, &self.self_party);
        // R161: core-game answers carry no tier; the Agent Select one of
        // this very match did.
        let known = lock(&self.st.live).match_tiers.for_match(&match_id);
        fill_tiers(&mut roster, &known);
        log::info!(
            "in game: {} players, match {}",
            roster.len(),
            short(&match_id)
        );
        lock(&self.st.live).coregame_match_id = Some(match_id.clone());
        // R164: the roster goes on screen (and the worker starts) first; the
        // loadouts only feed Player Details, so a slow or failed answer never
        // holds the ten cards back (as in Agent Select, R83).
        self.apply_roster(session, Stage::Ingame, Some(m.map_id), roster)
            .await;
        let asked = tokio::time::timeout(
            self.ingame_loadouts_timeout,
            session.coregame_loadouts(&match_id),
        )
        .await;
        match asked {
            Err(_) => log::warn!("loadouts skipped: no answer in time"),
            Ok(Ok(resp)) => {
                let found = extract_loadouts(&resp);
                if !found.is_empty() {
                    lock(&self.st.live).loadouts.extend(found);
                    // No card shows skins: an open Details is rebuilt.
                    crate::details::refresh_open_after_live_change(&self.st).await;
                }
            }
            Ok(Err(RiotError::ClientGone)) => return Err(RiotError::ClientGone),
            Ok(Err(e)) => log::warn!("loadouts failed: {e}"),
        }
        Ok(())
    }

    /// Installs a roster: cached-first slots (DB) → publish → worker.
    /// Players already on screen keep their DB bundle; only new ones are
    /// read (and get a `players` row upsert), plus the user's own row when
    /// their level is new or changed (R100).
    pub(crate) async fn apply_roster(
        &mut self,
        session: &Arc<RiotSession>,
        stage: Stage,
        map_path: Option<String>,
        roster: Vec<RosterPlayer>,
    ) {
        let me = session.self_puuid().to_string();
        let roster = order_players(roster, &me);

        let new_players: Vec<RosterPlayer> = {
            let live = lock(&self.st.live);
            roster
                .iter()
                .filter(
                    |p| match live.slots.iter().find(|s| s.roster.puuid == p.puuid) {
                        None => true,
                        // R100: the user's slot is already on screen (the
                        // Closed one from startup); their level is still
                        // saved when it is new or changed.
                        Some(s) => {
                            let stored = s.db.row.as_ref().map(|r| r.account_level);
                            p.own_level_stale(&me, stored)
                        }
                    },
                )
                .cloned()
                .collect()
        };
        let mut loaded: HashMap<String, PlayerDb> = HashMap::new();
        if !new_players.is_empty() {
            let now = now_ms();
            let me = me.clone();
            loaded = self
                .st
                .db(move |repo| {
                    let mut out = HashMap::new();
                    for p in &new_players {
                        save_player(repo.conn(), &p.player_update(&me), now)?;
                        out.insert(p.puuid.clone(), load_player_db(repo, Some(&me), &p.puuid)?);
                    }
                    Ok(out)
                })
                .await
                .unwrap_or_default();
        }

        {
            let mut live = lock(&self.st.live);
            // Built under the lock from the *current* slots: the worker may
            // have stored a fresh DB bundle while the reads above ran.
            let slots: Vec<Slot> = roster
                .into_iter()
                .map(|r| {
                    let db = match live.slots.iter().find(|s| s.roster.puuid == r.puuid) {
                        Some(s) => {
                            let mut db = s.db.clone();
                            // The user's freshly saved `players` row (own
                            // level, R100) replaces the stale one; the rest
                            // of the bundle stays the worker's.
                            if let Some(fresh) = loaded.remove(&r.puuid) {
                                db.row = fresh.row;
                            }
                            db
                        }
                        None => loaded.remove(&r.puuid).unwrap_or_default(),
                    };
                    Slot { roster: r, db }
                })
                .collect();
            if let Some(own) = slots.iter().find(|s| s.roster.puuid == me) {
                live.self_roster = Some(own.roster.clone());
            }
            if live.map_path != map_path {
                live.map_id = None;
            }
            live.stage = stage;
            live.map_path = map_path;
            live.slots = slots;
            live.note_hidden();
            // R155: a player the new roster no longer shows is not retried.
            live.prune_rank_retries();
        }
        self.st.publish();
        // An open Details of someone on this roster follows it (a lock-in,
        // a team change): it is a snapshot otherwise. Before the worker
        // starts, so a caller that stops the worker right after this returns
        // is not racing a sync this rebuild gave time to run.
        crate::details::refresh_open_after_live_change(&self.st).await;
        self.ensure_worker(session);
    }

    /// The new stage with only the user's own card (roster fetch failed);
    /// the user's own data still syncs meanwhile (never a stuck skeleton).
    async fn show_self_only(&mut self, session: &Arc<RiotSession>) {
        let (me, own) = {
            let live = lock(&self.st.live);
            (live.self_puuid.clone(), live.self_roster.clone())
        };
        let Some(me) = me else { return };
        let p = me.clone();
        let db = self
            .st
            .db(move |repo| load_player_db(repo, Some(&p), &p))
            .await
            .unwrap_or_default();
        let slot = match own {
            Some(roster) => Some(Slot { roster, db }),
            None => closed_self_slot(&me, db),
        };
        {
            let mut live = lock(&self.st.live);
            live.stage = self.stage;
            live.map_path = None;
            live.map_id = None;
            live.slots = slot.into_iter().collect();
            live.prune_rank_retries();
        }
        self.st.publish();
        self.ensure_worker(session);
    }

    /// The first verdict on whether VALORANT is running is in: from now on the
    /// published `LiveState` says `settled` (the UI picks its start page from
    /// it, once). Idempotent; publishes only when it changed.
    fn settle(&self) {
        if lock(&self.st.live).settle() {
            self.st.publish();
        }
    }

    /// Closed: the user's own card from the DB (nothing if unknown).
    pub(crate) async fn enter_closed(&mut self) {
        self.stop_worker();
        self.stage = Stage::Closed;
        show_closed(&self.st).await;
    }

    /// A match ended: once Riot has published it, the user's party needs a
    /// re-sync (their last-5/RR changed). Checks whether the ended match
    /// is in the DB yet; if not, re-syncs the user and their party (in
    /// whatever stage is shown: a quick requeue carries the check into Agent
    /// Select and In Game, R48) and tries again later.
    async fn post_match_check(&mut self, session: &Arc<RiotSession>) {
        let Some(pm) = self.post_match.take() else {
            return;
        };
        let Some(me) = lock(&self.st.live).self_puuid.clone() else {
            return;
        };
        let me_now = me.clone();
        let (id, competitive) = (pm.match_id.clone(), pm.competitive);
        let landed = self
            .st
            .db(move |repo| match_landed(repo.conn(), &me, &id, competitive))
            .await
            .unwrap_or(true);
        if landed {
            return;
        }
        log::info!(
            "match {} not published yet; re-syncing party",
            short(&pm.match_id)
        );
        {
            let mut live = lock(&self.st.live);
            let party: Vec<String> = live
                .slots
                .iter()
                .filter(|s| s.roster.is_self_party || s.roster.puuid == me_now)
                .map(|s| s.roster.puuid.clone())
                .collect();
            for p in party {
                live.synced.remove(&p);
                live.party_ready.remove(&p);
            }
        }
        self.ensure_worker(session);
        let next = pm.attempt + 1;
        if let Some(delay) = POST_MATCH_DELAYS.get(next) {
            self.post_match = Some(PostMatch {
                match_id: pm.match_id,
                competitive: pm.competitive,
                due: Instant::now() + *delay,
                attempt: next,
            });
        }
    }
}

/// The shared model in the Closed stage, published: the user's own card from
/// the DB (when it knows them), nothing else.
async fn show_closed(st: &Arc<AppState>) {
    let me = lock(&st.live).self_puuid.clone();
    let slot = match me {
        Some(me) => {
            let p = me.clone();
            let db = st
                .db(move |repo| load_player_db(repo, Some(&p), &p))
                .await
                .unwrap_or_default();
            closed_self_slot(&me, db)
        }
        None => None,
    };
    {
        let mut live = lock(&st.live);
        live.stage = Stage::Closed;
        live.in_match = false;
        live.map_path = None;
        live.map_id = None;
        live.coregame_match_id = None;
        live.loadouts.clear();
        live.match_tiers.clear();
        live.slots = slot.into_iter().collect();
        // R155: nothing is retried while VALORANT is closed.
        live.prune_rank_retries();
    }
    st.publish();
}

fn lockfile_path(local_app_data: &Path) -> PathBuf {
    local_app_data
        .join("Riot Games")
        .join("Riot Client")
        .join("Config")
        .join("lockfile")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use base64::engine::general_purpose::STANDARD as BASE64;
    use base64::Engine as _;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, ResponseTemplate};

    use super::*;
    use crate::pipeline_tests::backfill::{
        history_path, mount_feed, requests, run_active, store_anchor, store_anchor_for,
        stored_anchor, stored_anchor_of, Feed,
    };
    use crate::pipeline_tests::comp_backfill::{
        mount_comp_feed, row_id, store_comp_anchor_for, stored_comp_anchor, stored_comp_anchor_of,
    };
    use crate::pipeline_tests::{card_json, harness, roster, session_with_other_wait, Harness, ME};
    use crate::store::{save_player, PlayerUpdate};

    // -- the supervisor (A1) -----------------------------------------

    /// Short waits so the tests need no mock clock.
    const FAST: RestartPacing = RestartPacing {
        delays: [
            Duration::from_millis(5),
            Duration::from_millis(10),
            Duration::from_millis(20),
        ],
        healthy_after: Duration::from_millis(150),
    };

    /// What one run of a scripted poll loop does.
    #[derive(Clone, Copy)]
    enum Run {
        /// Panics at once.
        Panic,
        /// Runs this many ms, then panics.
        PanicAfter(u64),
        /// Never ends.
        Pend,
        /// Ends normally.
        Return,
    }

    /// A supervised scripted loop (run `n` does `script[n]`, the last entry
    /// repeats): how often it was started, and what `on_panic` was told.
    struct Supervised {
        runs: Arc<std::sync::atomic::AtomicUsize>,
        panics: Arc<std::sync::Mutex<Vec<(String, Duration)>>>,
        task: tokio::task::JoinHandle<()>,
    }

    impl Supervised {
        fn start(script: Vec<Run>, pacing: RestartPacing) -> Self {
            use std::sync::atomic::Ordering::SeqCst;
            let runs = Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let panics = Arc::new(std::sync::Mutex::new(Vec::new()));
            let make = {
                let runs = runs.clone();
                move || {
                    let n = runs.fetch_add(1, SeqCst);
                    let run = script[n.min(script.len() - 1)];
                    async move {
                        match run {
                            Run::Panic => panic!("boom"),
                            Run::PanicAfter(ms) => {
                                sleep(Duration::from_millis(ms)).await;
                                panic!("boom after {ms} ms");
                            }
                            Run::Pend => std::future::pending::<()>().await,
                            Run::Return => {}
                        }
                    }
                }
            };
            let seen = panics.clone();
            let task = tokio::spawn(supervise(make, pacing, move |text, wait| {
                seen.lock().unwrap().push((text.to_string(), wait));
                async {}
            }));
            Self { runs, panics, task }
        }

        /// Waits (up to ~5 s) until `n` runs have started.
        async fn until_started(&self, n: usize) {
            for _ in 0..500 {
                if self.runs.load(std::sync::atomic::Ordering::SeqCst) >= n {
                    return;
                }
                sleep(Duration::from_millis(10)).await;
            }
            panic!(
                "only {} runs started",
                self.runs.load(std::sync::atomic::Ordering::SeqCst)
            );
        }

        fn waits(&self) -> Vec<Duration> {
            self.panics
                .lock()
                .unwrap()
                .iter()
                .map(|(_, w)| *w)
                .collect()
        }
    }

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[tokio::test]
    async fn a_poll_loop_that_panics_is_started_again_and_the_panic_is_reported() {
        let sup = Supervised::start(vec![Run::Panic, Run::Pend], FAST);
        sup.until_started(2).await;
        assert!(!sup.task.is_finished(), "the supervisor keeps watching");
        let panics = sup.panics.lock().unwrap().clone();
        assert_eq!(panics, vec![("boom".to_string(), ms(5))]);
        sup.task.abort();
    }

    #[tokio::test]
    async fn a_panic_with_a_formatted_message_is_reported_with_it() {
        let sup = Supervised::start(vec![Run::PanicAfter(1), Run::Pend], FAST);
        sup.until_started(2).await;
        let panics = sup.panics.lock().unwrap().clone();
        assert_eq!(panics, vec![("boom after 1 ms".to_string(), ms(5))]);
        sup.task.abort();
    }

    #[test]
    fn the_restart_waits_are_1_then_5_then_30_seconds_and_stay_there() {
        let waits: Vec<u64> = (1..=6)
            .map(|n| POLLER_RESTARTS.delay(n).as_secs())
            .collect();
        assert_eq!(waits, [1, 5, 30, 30, 30, 30]);
        assert_eq!(POLLER_RESTARTS.healthy_after, Duration::from_secs(60));
    }

    #[tokio::test]
    async fn restarts_back_off_and_hold_at_the_cap_while_it_keeps_panicking() {
        let sup = Supervised::start(
            vec![Run::Panic, Run::Panic, Run::Panic, Run::Panic, Run::Pend],
            FAST,
        );
        sup.until_started(5).await;
        assert_eq!(sup.waits(), [ms(5), ms(10), ms(20), ms(20)]);
        sup.task.abort();
    }

    #[tokio::test]
    async fn a_poller_that_ran_long_enough_starts_the_backoff_over() {
        // Two quick panics (5 ms, 10 ms), a run of 200 ms (>= the 150 ms that
        // counts as healthy) that panics, then a quick one: back to the
        // first wait, then the second.
        let sup = Supervised::start(
            vec![
                Run::Panic,
                Run::Panic,
                Run::PanicAfter(200),
                Run::Panic,
                Run::Pend,
            ],
            FAST,
        );
        sup.until_started(5).await;
        assert_eq!(sup.waits(), [ms(5), ms(10), ms(5), ms(10)]);
        sup.task.abort();
    }

    #[tokio::test]
    async fn a_poll_loop_that_returns_is_not_started_again() {
        let sup = Supervised::start(vec![Run::Return], FAST);
        tokio::time::timeout(Duration::from_secs(5), sup.task)
            .await
            .expect("the supervisor ends with the loop")
            .unwrap();
        assert_eq!(sup.runs.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert!(sup.panics.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn the_dead_loops_claims_are_dropped_so_the_new_one_starts_clean() {
        let h = harness().await;
        {
            let mut live = lock(&h.st.live);
            live.stage = Stage::Menus;
            live.start_backfill(None, None);
            live.start_comp_backfill(None, None);
            assert!(
                live.try_start_worker().is_some(),
                "a worker holds the claim"
            );
        }
        h.st.set_session(Some(h.session.clone()));

        reset_after_panic(&h.st);

        let live = lock(&h.st.live);
        assert!(live.backfill.is_none(), "an interrupted run is dropped");
        assert!(
            live.comp_backfill.is_none(),
            "so is the competitive-updates run"
        );
        assert!(!live.worker_running(), "the detached worker is revoked");
        assert!(h.st.session().is_none());
    }

    #[tokio::test]
    async fn a_dropped_poller_aborts_its_worker_task() {
        // The poller that dies in a panic takes its worker with it: the task is
        // aborted, not left detached.
        use std::sync::atomic::{AtomicBool, Ordering};
        struct SetOnDrop(Arc<AtomicBool>);
        impl Drop for SetOnDrop {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }
        let h = harness().await;
        let dropped = Arc::new(AtomicBool::new(false));
        let guard = SetOnDrop(dropped.clone());
        let mut poller = Poller::new(h.st.clone());
        poller.worker = Some((
            tauri::async_runtime::spawn(async move {
                let _guard = guard;
                std::future::pending::<()>().await
            }),
            1,
        ));
        sleep(Duration::from_millis(20)).await;
        assert!(!dropped.load(Ordering::SeqCst), "it is running");

        drop(poller);

        crate::pipeline_tests::wait_until("the worker task was aborted", || {
            dropped.load(Ordering::SeqCst)
        })
        .await;
    }

    #[tokio::test]
    async fn a_panic_under_the_supervisor_resets_the_model_and_shows_the_closed_card() {
        // `start`'s own wiring (`spawn_supervised`): at a panic the dead loop's
        // claims are dropped and the UI gets the Closed view (the user's own
        // card) at once, not the dead loop's cards for the whole wait.
        let h = harness().await;
        let now = now_ms();
        h.st.db(move |repo| {
            save_player(
                repo.conn(),
                &PlayerUpdate {
                    puuid: ME.into(),
                    name: Some(("Me".into(), "OPD1".into())),
                    account_level: 87,
                    card_id: "card".into(),
                },
                now,
            )
        })
        .await
        .unwrap();
        {
            let mut live = lock(&h.st.live);
            live.stage = Stage::Ingame;
            live.in_match = true;
            live.slots = ["cccccccc-0000-0000-0000-000000000003", ME]
                .map(|p| Slot {
                    roster: roster(p, false),
                    db: PlayerDb::default(),
                })
                .into();
            live.start_backfill(None, None);
            live.start_comp_backfill(None, None);
            assert!(
                live.try_start_worker().is_some(),
                "a worker holds the claim"
            );
        }
        h.st.set_session(Some(h.session.clone()));
        h.st.publish();
        assert_eq!(h.st.live_state().stage, Stage::Ingame);
        assert_eq!(h.st.live_state().players.len(), 2);

        let runs = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let task = spawn_supervised(
            h.st.clone(),
            {
                let runs = runs.clone();
                move || {
                    let first = runs.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0;
                    async move {
                        if first {
                            panic!("boom");
                        }
                        std::future::pending::<()>().await
                    }
                }
            },
            FAST,
        );

        crate::pipeline_tests::wait_until("the Closed view was published", || {
            h.st.live_state().stage == Stage::Closed
        })
        .await;
        let live_state = h.st.live_state();
        assert_eq!(live_state.players.len(), 1, "only the user's own card");
        assert_eq!(live_state.players[0].puuid, ME);
        {
            let live = lock(&h.st.live);
            assert!(live.backfill.is_none() && live.comp_backfill.is_none());
            assert!(!live.worker_running(), "the claim is revoked");
            assert!(!live.in_match);
        }
        assert!(h.st.session().is_none());
        crate::pipeline_tests::wait_until("the loop was started again", || {
            runs.load(std::sync::atomic::Ordering::SeqCst) == 2
        })
        .await;
        task.abort();
    }

    #[tokio::test]
    async fn the_restart_waits_for_the_closed_card_so_it_cannot_land_over_the_new_roster() {
        // Review round 2: the Closed card's read of the user's own row was a
        // detached task. Slow (the DB busy), it finished after the restarted
        // poller had published its roster, and put Closed over it. The
        // supervisor now awaits it before the backoff, so the new loop starts
        // only after the Closed card is out.
        use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
        let h = harness().await;
        {
            let mut live = lock(&h.st.live);
            live.stage = Stage::Ingame;
            live.in_match = true;
        }
        h.st.publish();
        assert_eq!(h.st.live_state().stage, Stage::Ingame);

        // The DB is busy: whatever reads it next waits.
        let (held_tx, held_rx) = std::sync::mpsc::channel::<()>();
        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        let holder = {
            let st = h.st.clone();
            std::thread::spawn(move || {
                st.db_blocking(|_| {
                    held_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                    Ok(())
                });
            })
        };
        held_rx.recv_timeout(Duration::from_secs(5)).unwrap();

        let runs = Arc::new(AtomicUsize::new(0));
        let task = spawn_supervised(
            h.st.clone(),
            {
                let runs = runs.clone();
                move || {
                    let first = runs.fetch_add(1, SeqCst) == 0;
                    async move {
                        if first {
                            panic!("boom");
                        }
                        std::future::pending::<()>().await
                    }
                }
            },
            FAST,
        );
        crate::pipeline_tests::wait_until("the first loop ran", || runs.load(SeqCst) == 1).await;
        // Far longer than FAST's first restart wait (5 ms): without the await
        // the second loop would be running by now.
        sleep(Duration::from_millis(300)).await;
        assert_eq!(
            runs.load(SeqCst),
            1,
            "the restart waits for the Closed card"
        );
        assert_eq!(
            h.st.live_state().stage,
            Stage::Ingame,
            "still the dead loop's"
        );

        release_tx.send(()).unwrap();
        holder.join().unwrap();
        crate::pipeline_tests::wait_until("the loop was started again", || runs.load(SeqCst) == 2)
            .await;
        assert_eq!(
            h.st.live_state().stage,
            Stage::Closed,
            "the Closed card was out before the second loop began"
        );
        task.abort();
    }

    /// Waits (up to ~5s) for a finished backfill run's anchor.
    async fn wait_for_anchor(st: &AppState) -> Option<(String, i64)> {
        for _ in 0..250 {
            if let Some(a) = stored_anchor(st).await {
                return Some(a);
            }
            sleep(Duration::from_millis(20)).await;
        }
        None
    }

    /// Answers `/chat/v4/presences` with the user's VALORANT presence in
    /// `state` (`MENUS` / `PREGAME` / `INGAME`) and `queue`.
    async fn mock_presence(h: &Harness, state: &str, queue: &str) {
        let private = BASE64.encode(
            json!({"sessionLoopState": state, "queueId": queue, "partyId": "party-1"}).to_string(),
        );
        Mock::given(method("GET"))
            .and(path("/chat/v4/presences"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "presences": [{"puuid": ME, "product": "valorant", "private": private}]
            })))
            .with_priority(1)
            .mount(&h.server)
            .await;
    }

    /// Like [`mock_presence`], but the presence can be switched mid-test:
    /// the returned cell holds the `(sessionLoopState, queueId)` served.
    async fn switchable_presence(
        h: &Harness,
        state: &str,
        queue: &str,
    ) -> Arc<std::sync::Mutex<(String, String)>> {
        let cell = Arc::new(std::sync::Mutex::new((
            state.to_string(),
            queue.to_string(),
        )));
        let served = cell.clone();
        Mock::given(method("GET"))
            .and(path("/chat/v4/presences"))
            .respond_with(move |_: &wiremock::Request| {
                let (state, queue) = served.lock().unwrap().clone();
                let private = BASE64.encode(
                    json!({"sessionLoopState": state, "queueId": queue, "partyId": "party-1"})
                        .to_string(),
                );
                ResponseTemplate::new(200).set_body_json(json!({
                    "presences": [{"puuid": ME, "product": "valorant", "private": private}]
                }))
            })
            .with_priority(1)
            .mount(&h.server)
            .await;
        cell
    }

    /// The party key the mocked presence carries (so a tick sees no party
    /// change).
    fn mocked_party_key() -> Option<PartyKey> {
        Some(PartyKey {
            party_id: Some("party-1".into()),
            size: None,
        })
    }

    /// `/match-details/` requests so far.
    async fn details_requests(h: &Harness) -> usize {
        requests(&h.server)
            .await
            .iter()
            .filter(|r| r.url.path().starts_with("/match-details/"))
            .count()
    }

    /// The user's own card on screen in the Lobby, already synced: only
    /// the backfill is left to do.
    fn synced_lobby(h: &Harness) {
        let mut live = lock(&h.st.live);
        live.stage = Stage::Menus;
        live.slots = vec![Slot {
            roster: roster(ME, false),
            db: PlayerDb::default(),
        }];
        live.self_roster = Some(roster(ME, false));
        live.synced.insert(ME.into());
    }

    // R164 (review finding 7): the roster retry is a flat ~2 s for the first
    // five retries, then the old 3, 6, 12, 24, 30 s backoff.
    #[test]
    fn the_roster_retry_is_flat_at_first_then_backs_off() {
        let secs: Vec<u64> = (1..=11)
            .map(|failures| roster_retry_delay(failures).as_secs())
            .collect();
        assert_eq!(secs, [2, 2, 2, 2, 2, 3, 6, 12, 24, 30, 30]);
        assert_eq!(roster_retry_delay(u32::MAX).as_secs(), 30, "never panics");
    }

    #[tokio::test]
    async fn a_failed_first_roster_fetch_retries_in_about_two_seconds() {
        // Presence says PREGAME, but the pregame player endpoint is not ready
        // (this harness mounts none: 404).
        let h = harness().await;
        let mut poller = Poller::new(h.st.clone());
        poller.stage = Stage::Pregame;
        poller.fetch_or_retry(&h.session).await.unwrap();
        let (due, failures) = poller.roster_retry.expect("a retry is scheduled");
        assert_eq!(failures, 1);
        let wait = due.saturating_duration_since(Instant::now());
        assert!(
            wait > Duration::from_millis(1_000) && wait <= Duration::from_secs(2),
            "first retry in ~2 s, not {wait:?}"
        );
        poller.stop_worker();
    }

    #[tokio::test]
    async fn a_failing_retry_reschedules_two_seconds_out_five_times() {
        let h = harness().await;
        let mut poller = Poller::new(h.st.clone());
        poller.stage = Stage::Pregame;
        poller.next_presence = Instant::now() + Duration::from_secs(3600);
        poller.fetch_or_retry(&h.session).await.unwrap();
        for expected in 2..=5 {
            // Make the retry due and let the tick run it (still 404).
            let (_, failures) = poller.roster_retry.unwrap();
            poller.roster_retry = Some((Instant::now(), failures));
            poller.connected_tick(&h.session).await.unwrap();
            let (due, failures) = poller.roster_retry.expect("still retrying");
            assert_eq!(failures, expected);
            let wait = due.saturating_duration_since(Instant::now());
            assert!(
                wait > Duration::from_millis(1_000) && wait <= Duration::from_secs(2),
                "retry {expected}: ~2 s, not {wait:?}"
            );
        }
        // The sixth failure backs off to 3 s.
        poller.roster_retry = Some((Instant::now(), 5));
        poller.connected_tick(&h.session).await.unwrap();
        let (due, failures) = poller.roster_retry.unwrap();
        assert_eq!(failures, 6);
        assert!(due.saturating_duration_since(Instant::now()) > Duration::from_millis(2_500));
        poller.stop_worker();
    }

    // R164 (finding 6): the post-match rank check at about +10, +20, +40,
    // +80 and +160 s after the match.
    #[test]
    fn the_post_match_checks_land_at_10_20_40_80_160_seconds() {
        let mut at = 0;
        let mut seen = Vec::new();
        for gap in POST_MATCH_DELAYS {
            at += gap.as_secs();
            seen.push(at);
        }
        assert_eq!(seen, [10, 20, 40, 80, 160]);
    }

    #[tokio::test]
    async fn the_post_match_check_reschedules_until_the_schedule_is_used_up() {
        // The ended match never lands in this DB: each check re-syncs and
        // schedules the next, five in all, then stops.
        let h = harness().await;
        let mut poller = Poller::held(h.st.clone());
        poller.stage = Stage::Menus;
        synced_lobby(&h);
        poller.post_match = Some(PostMatch {
            match_id: "m-ended".into(),
            competitive: true,
            due: Instant::now(),
            attempt: 0,
        });
        let mut gaps = Vec::new();
        for _ in 0..POST_MATCH_DELAYS.len() {
            poller.post_match_check(&h.session).await;
            if let Some(p) = &poller.post_match {
                gaps.push(p.due.saturating_duration_since(Instant::now()).as_secs());
            }
        }
        // The delay to each later check, rounded down: 10, 20, 40, 80.
        assert_eq!(gaps.len(), 4);
        for (gap, want) in gaps.iter().zip([9, 19, 39, 79]) {
            assert!(*gap == want || *gap == want + 1, "gap {gap} vs {want}");
        }
        assert!(poller.post_match.is_none(), "the fifth check is the last");
    }

    #[tokio::test]
    async fn a_deathmatch_pauses_the_backfill_run_until_it_ends() {
        // R137 with R41: a free-for-all keeps the Lobby view for the whole
        // match, but the raw presence is in a match — a run under way stops
        // at the next id, keeps its cursor, and resumes when the match ends.
        let h = harness().await;
        let mut feed = Feed::new(60);
        feed.details_delay = Some(Duration::from_millis(30));
        mount_feed(&h.server, feed).await;
        let presence = switchable_presence(&h, "MENUS", "deathmatch").await;
        let mut poller = Poller::new(h.st.clone());
        poller.stage = Stage::Menus;
        poller.presence_stage = Stage::Menus;
        poller.party_key = mocked_party_key();
        synced_lobby(&h);
        crate::sync::start_backfill_run(&h.st).await;
        poller.ensure_worker(&h.session);
        for _ in 0..250 {
            if details_requests(&h).await >= 3 {
                break;
            }
            sleep(Duration::from_millis(10)).await;
        }
        assert!(details_requests(&h).await >= 3, "the run is under way");

        // The Deathmatch starts: the view stays the Lobby.
        *presence.lock().unwrap() = ("INGAME".into(), "deathmatch".into());
        poller.presence_tick(&h.session).await.unwrap();
        assert_eq!(poller.stage, Stage::Menus, "R41: the Lobby view stays");
        let paused_at = details_requests(&h).await;
        sleep(Duration::from_millis(400)).await;
        assert!(
            details_requests(&h).await <= paused_at + 1,
            "at most the one in-flight request after the match started: {} then {}",
            paused_at,
            details_requests(&h).await
        );
        assert!(run_active(&h.st), "the cursor waits for the match to end");
        assert_eq!(stored_anchor(&h.st).await, None);

        // The Deathmatch ends: the same run resumes and finishes.
        *presence.lock().unwrap() = ("MENUS".into(), "deathmatch".into());
        poller.presence_tick(&h.session).await.unwrap();
        let anchor = wait_for_anchor(&h.st).await.expect("the run finished");
        assert_eq!(anchor.0, crate::pipeline_tests::backfill::entry_id(0));
        assert_eq!(details_requests(&h).await, 60, "every id once");
        poller.stop_worker();
    }

    #[tokio::test]
    async fn an_ffa_roster_fallback_never_starts_the_backfill_mid_match() {
        // Presence has no queueId, so the view goes In Game; the core-game
        // roster is free-for-all, so the Lobby roster is applied (R41) — as
        // Menus, in the middle of a match. The queued run must still wait.
        let h = harness().await;
        mount_feed(&h.server, Feed::new(5)).await;
        let identity = json!({"Incognito": false, "HideAccountLevel": false,
                              "AccountLevel": 10, "PlayerCardID": "card"});
        let get = |p: String, body: serde_json::Value| {
            Mock::given(method("GET"))
                .and(path(p))
                .respond_with(ResponseTemplate::new(200).set_body_json(body))
        };
        get(
            "/parties/v1/parties/party-1".into(),
            json!({"ID": "party-1", "Members": [{"Subject": ME, "PlayerIdentity": identity}]}),
        )
        .mount(&h.server)
        .await;
        get(
            format!("/core-game/v1/players/{ME}"),
            json!({"Subject": ME, "MatchID": "cg-ffa"}),
        )
        .mount(&h.server)
        .await;
        let player = |puuid: &str| {
            json!({"Subject": puuid, "TeamID": puuid, "CharacterID": "agent",
                   "PlayerIdentity": identity})
        };
        get(
            "/core-game/v1/matches/cg-ffa".into(),
            json!({"MapID": "/Game/Maps/Ascent/Ascent",
                   "Players": [player(ME), player("p2"), player("p3")]}),
        )
        .mount(&h.server)
        .await;
        mock_presence(&h, "INGAME", "").await;
        let mut poller = Poller::new(h.st.clone());
        poller.stage = Stage::Menus;
        poller.presence_stage = Stage::Menus;
        poller.party_key = mocked_party_key();
        synced_lobby(&h);
        crate::sync::start_backfill_run(&h.st).await;

        poller.presence_tick(&h.session).await.unwrap();
        sleep(Duration::from_millis(400)).await;

        assert_eq!(poller.stage, Stage::Ingame);
        assert_eq!(
            lock(&h.st.live).stage,
            Stage::Menus,
            "the FFA fallback applied the Lobby roster"
        );
        assert_eq!(backfill_pages(&h).await, 0, "no history page mid-match");
        assert_eq!(details_requests(&h).await, 0, "no match-details mid-match");
        assert!(run_active(&h.st), "the run waits for the match to end");
        poller.stop_worker();
    }

    #[tokio::test]
    async fn a_quick_requeue_keeps_the_post_match_follow_up() {
        // R48: a competitive match ended and the Lobby sync ran before Riot
        // published it. The user requeues before the first check: the
        // follow-up must still re-sync the user's card — in Agent Select, and
        // still pending In Game — instead of being dropped.
        let h = harness().await;
        let comp = format!("/mmr/v1/players/{ME}/competitiveupdates");
        let comp_calls =
            |reqs: Vec<wiremock::Request>| reqs.iter().filter(|r| r.url.path() == comp).count();
        let mut poller = Poller::new(h.st.clone());
        poller.stage = Stage::Menus;
        synced_lobby(&h);
        poller.post_match = Some(PostMatch {
            match_id: "m-ended".into(),
            competitive: true,
            due: Instant::now(),
            attempt: 0,
        });

        // Agent Select starts (this mock has no pregame roster: the user's
        // own card only).
        poller
            .change_stage(&h.session, Stage::Pregame, None)
            .await
            .unwrap();
        assert!(
            poller.post_match.is_some(),
            "the requeue keeps the follow-up"
        );
        assert!(lock(&h.st.live).synced.contains(ME), "still synced");

        // The check comes due in Agent Select: the match is still not in the
        // DB, so the user is re-synced there.
        let before = comp_calls(requests(&h.server).await);
        poller.post_match_check(&h.session).await;
        for _ in 0..250 {
            if lock(&h.st.live).synced.contains(ME) {
                break;
            }
            sleep(Duration::from_millis(20)).await;
        }
        assert!(lock(&h.st.live).synced.contains(ME), "re-synced");
        assert_eq!(comp_calls(requests(&h.server).await), before + 1);
        assert_eq!(
            poller.post_match.as_ref().map(|p| p.attempt),
            Some(1),
            "the next check is scheduled"
        );

        // Agent Select -> In Game keeps it too; only Closed drops it.
        poller
            .change_stage(&h.session, Stage::Ingame, None)
            .await
            .unwrap();
        assert!(poller.post_match.is_some(), "In Game keeps the follow-up");
        poller
            .change_stage(&h.session, Stage::Closed, None)
            .await
            .unwrap();
        assert!(poller.post_match.is_none(), "Closed drops it");
        poller.stop_worker();
    }

    #[tokio::test]
    async fn a_mid_match_party_refresh_reaches_the_cards() {
        // The app started mid-match and its one party fetch failed, so the
        // duo was built as a stranger: a party-stripe candidate with a ⟲
        // badge. A later party change refreshes the own-party set; the cards
        // must follow it (the roster is not re-fetched In Game).
        const DUO: &str = "dddddddd-0000-0000-0000-00000000000d";
        let h = harness().await;
        let identity = json!({"Incognito": false, "HideAccountLevel": false,
                              "AccountLevel": 10, "PlayerCardID": "card"});
        Mock::given(method("GET"))
            .and(path("/parties/v1/parties/party-1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "ID": "party-1",
                "Members": [{"Subject": ME, "PlayerIdentity": identity},
                            {"Subject": DUO, "PlayerIdentity": identity}]
            })))
            .mount(&h.server)
            .await;
        let mut duo = roster(DUO, false);
        duo.team = opd1_core::model::Team::Ally;
        duo.team_id = "Blue".into();
        {
            let mut live = lock(&h.st.live);
            live.stage = Stage::Ingame;
            live.slots = vec![
                Slot {
                    roster: roster(ME, false),
                    db: PlayerDb::default(),
                },
                Slot {
                    roster: duo,
                    db: PlayerDb {
                        encounters: Some(opd1_core::model::EncounterCount {
                            with: 3,
                            against: 0,
                        }),
                        ..PlayerDb::default()
                    },
                },
            ];
        }
        h.st.publish();
        assert_eq!(card_json(&h.st, DUO)["encounters"]["with"], json!(3));

        let mut poller = Poller::new(h.st.clone());
        poller.stage = Stage::Ingame;
        poller.party_key = mocked_party_key();
        poller.refresh_self_party(&h.session).await.unwrap();

        assert!(poller.self_party.contains(DUO));
        let duo_slot = lock(&h.st.live)
            .slots
            .iter()
            .find(|s| s.roster.puuid == DUO)
            .map(|s| s.roster.is_self_party);
        assert_eq!(duo_slot, Some(true), "the slot follows the refresh");
        assert_eq!(
            card_json(&h.st, DUO)["encounters"],
            serde_json::Value::Null,
            "the user's party gets no badge, and the card was republished"
        );
    }

    /// The `page` requests the backfill made (`endIndex=20`; a per-player
    /// sync asks for `0..2`).
    async fn backfill_pages(h: &Harness) -> usize {
        requests(&h.server)
            .await
            .iter()
            .filter(|r| r.url.path() == history_path())
            .filter(|r| {
                r.url
                    .query_pairs()
                    .any(|(k, v)| k == "endIndex" && v == "20")
            })
            .count()
    }

    async fn content_requests(h: &Harness) -> usize {
        requests(&h.server)
            .await
            .iter()
            .filter(|r| r.url.path().starts_with("/content-service/"))
            .count()
    }

    #[tokio::test]
    async fn the_current_act_is_asked_once_when_the_game_shows_up_never_while_closed() {
        // R162, like R137: the session is stale while VALORANT is closed.
        let h = harness().await;
        let mut poller = Poller::new(h.st.clone());
        poller.on_connected(h.session.clone()).await;
        sleep(Duration::from_millis(300)).await;
        assert_eq!(content_requests(&h).await, 0, "no ask on a Closed connect");
        // Closed ticks (the due re-ask included) ask nothing either.
        poller.connected_tick(&h.session).await.unwrap();
        sleep(Duration::from_millis(100)).await;
        assert_eq!(content_requests(&h).await, 0);

        // VALORANT is up: one ask, however many presences follow.
        mock_presence(&h, "MENUS", "unrated").await;
        poller.presence_tick(&h.session).await.unwrap();
        poller.presence_tick(&h.session).await.unwrap();
        sleep(Duration::from_millis(300)).await;
        assert_eq!(content_requests(&h).await, 1);
    }

    #[tokio::test]
    async fn every_new_session_asks_for_the_current_act_once_more() {
        // The flag resets at each connect (the game starting, an account
        // switch), so the new session asks again, with its fresh headers.
        let h = harness().await;
        let mut poller = Poller::new(h.st.clone());
        mock_presence(&h, "MENUS", "unrated").await;
        poller.on_connected(h.session.clone()).await;
        poller.presence_tick(&h.session).await.unwrap();
        poller.presence_tick(&h.session).await.unwrap();
        sleep(Duration::from_millis(300)).await;
        assert_eq!(content_requests(&h).await, 1);

        poller.on_connected(h.session.clone()).await;
        poller.presence_tick(&h.session).await.unwrap();
        sleep(Duration::from_millis(300)).await;
        assert_eq!(content_requests(&h).await, 2, "the new session asked once");
    }

    #[tokio::test]
    async fn the_tick_asks_again_when_due_and_the_game_is_not_closed() {
        let h = harness().await;
        let mut poller = Poller::new(h.st.clone());
        poller.on_connected(h.session.clone()).await;
        // Only the re-ask is under test: no presence tick, a stage showing
        // the game is up.
        poller.next_presence = Instant::now() + Duration::from_secs(3600);
        poller.stage = Stage::Menus;
        poller.connected_tick(&h.session).await.unwrap();
        sleep(Duration::from_millis(300)).await;
        assert_eq!(content_requests(&h).await, 1, "never answered, so due");
    }

    // -- the start page's signal: LiveState.settled ---------------------

    /// The disconnected tick runs in a task (it sleeps 5 s after deciding, and
    /// the first verdict is made before that sleep): wait for `settled`, then
    /// stop it. Returns whether the published state said settled in time.
    async fn settled_after_a_disconnected_tick(h: &Harness, poller: Poller) -> bool {
        let task = tokio::spawn(async move {
            let mut poller = poller;
            poller.disconnected_tick().await;
        });
        let mut settled = false;
        for _ in 0..100 {
            if h.st.live_state().settled {
                settled = true;
                break;
            }
            sleep(Duration::from_millis(20)).await;
        }
        task.abort();
        settled
    }

    #[tokio::test]
    async fn the_published_state_is_unsettled_until_the_first_verdict() {
        // The model's starting Closed is not a verdict: nothing has looked yet.
        let h = harness().await;
        assert!(!h.st.live_state().settled);
        h.st.publish();
        assert!(!h.st.live_state().settled);
        assert!(!lock(&h.st.live).is_settled());
        // The Closed card of a connect is not one either.
        let mut poller = Poller::new(h.st.clone());
        poller.enter_closed().await;
        assert_eq!(h.st.live_state().stage, Stage::Closed);
        assert!(!h.st.live_state().settled);
    }

    #[tokio::test]
    async fn no_lockfile_at_the_first_check_is_the_first_verdict_closed() {
        let h = harness().await;
        let empty = tempfile::tempdir().unwrap();
        let mut poller = Poller::new(h.st.clone());
        poller.local_app_data = Some(empty.path().to_path_buf());
        assert!(settled_after_a_disconnected_tick(&h, poller).await);
        assert_eq!(h.st.live_state().stage, Stage::Closed);
    }

    #[tokio::test]
    async fn a_machine_with_no_riot_client_location_is_settled_closed() {
        let h = harness().await;
        let mut poller = Poller::new(h.st.clone());
        poller.local_app_data = None;
        assert!(settled_after_a_disconnected_tick(&h, poller).await);
    }

    #[tokio::test]
    async fn a_failed_connect_attempt_is_the_first_verdict_closed() {
        // A lockfile that names nothing usable: `connect` fails.
        let h = harness().await;
        let dir = tempfile::tempdir().unwrap();
        let cfg = dir
            .path()
            .join("Riot Games")
            .join("Riot Client")
            .join("Config");
        std::fs::create_dir_all(&cfg).unwrap();
        std::fs::write(cfg.join("lockfile"), "not a lockfile").unwrap();
        let mut poller = Poller::new(h.st.clone());
        poller.local_app_data = Some(dir.path().to_path_buf());
        assert!(settled_after_a_disconnected_tick(&h, poller).await);
        assert_eq!(h.st.live_state().stage, Stage::Closed);
    }

    #[tokio::test]
    async fn connected_but_no_presence_verdict_yet_stays_unsettled() {
        // The connect itself (the Closed card), then a presence call that
        // fails: no verdict, the app has not looked at the game yet.
        let h = harness().await;
        let mut poller = Poller::new(h.st.clone());
        poller.on_connected(h.session.clone()).await;
        assert!(!h.st.live_state().settled);
        poller.presence_tick(&h.session).await.unwrap(); // no presence mock: 404
        assert!(!h.st.live_state().settled);
        assert_eq!(poller.presence_errors, 1);
        poller.stop_worker();
    }

    #[tokio::test]
    async fn the_first_absent_presence_is_not_a_verdict_the_second_is() {
        // Two strikes (GameWatch::ABSENT_TICKS_TO_CLOSE): one odd payload must
        // not settle the start page on Closed.
        let h = harness().await;
        Mock::given(method("GET"))
            .and(path("/chat/v4/presences"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"presences": []})))
            .with_priority(1)
            .mount(&h.server)
            .await;
        let mut poller = Poller::new(h.st.clone());
        poller.on_connected(h.session.clone()).await;
        poller.presence_tick(&h.session).await.unwrap();
        assert!(!h.st.live_state().settled, "one absent tick");
        poller.presence_tick(&h.session).await.unwrap();
        let state = h.st.live_state();
        assert!(state.settled, "two absent ticks: VALORANT is closed");
        assert_eq!(state.stage, Stage::Closed);
        poller.stop_worker();
    }

    #[tokio::test]
    async fn the_first_presence_with_a_stage_settles_the_state_with_that_stage() {
        // VALORANT was already running when the app started: the first state
        // with `settled: true` is the Lobby, never a settled Closed.
        let h = harness().await;
        mock_presence(&h, "MENUS", "unrated").await;
        let mut poller = Poller::held(h.st.clone());
        poller.on_connected(h.session.clone()).await;
        assert!(!h.st.live_state().settled);
        poller.presence_tick(&h.session).await.unwrap();
        let state = h.st.live_state();
        assert!(state.settled);
        assert_eq!(state.stage, Stage::Menus);
        // Every state a UI could have received with `settled: true` was a stage.
        let settled: Vec<_> = h.live_events().into_iter().filter(|s| s.settled).collect();
        assert!(!settled.is_empty());
        for s in settled {
            assert_eq!(s.stage, Stage::Menus);
        }
    }

    #[tokio::test]
    async fn once_settled_it_stays_settled_through_a_later_presence_and_a_model_reset() {
        let h = harness().await;
        let cell = switchable_presence(&h, "MENUS", "unrated").await;
        let mut poller = Poller::held(h.st.clone());
        poller.on_connected(h.session.clone()).await;
        poller.presence_tick(&h.session).await.unwrap();
        assert!(h.st.live_state().settled);
        *cell.lock().unwrap() = ("PREGAME".into(), "unrated".into());
        poller.presence_tick(&h.session).await.unwrap();
        assert!(h.st.live_state().settled);
        // The supervisor's reset after a panic (R169) and the Closed card it
        // publishes do not unsettle it, and neither does a reconnect's.
        reset_after_panic(&h.st);
        show_closed(&h.st).await;
        let state = h.st.live_state();
        assert_eq!(state.stage, Stage::Closed);
        assert!(
            state.settled,
            "the restarted poller does not start the verdict over"
        );
        poller.drop_session().await;
        assert!(h.st.live_state().settled);
    }

    #[tokio::test]
    async fn on_connected_queues_the_run_and_the_first_presence_starts_the_worker() {
        // R137: connecting while VALORANT is closed makes no Sync/backfill
        // call: the session is stale (R46) and would be rebuilt when the
        // game starts. The run waits in the model until the first presence
        // shows VALORANT is up and the roster's worker picks it up.
        let h = harness().await;
        mount_feed(&h.server, Feed::new(5)).await;
        let mut poller = Poller::new(h.st.clone());
        poller.on_connected(h.session.clone()).await;
        sleep(Duration::from_millis(400)).await;

        assert!(run_active(&h.st), "the run is queued");
        assert!(!lock(&h.st.live).worker_running(), "no worker yet");
        let calls = requests(&h.server)
            .await
            .iter()
            .filter(|r| {
                ["/match-history/", "/mmr/", "/match-details/"]
                    .iter()
                    .any(|p| r.url.path().starts_with(p))
            })
            .count();
        assert_eq!(calls, 0, "no sync or backfill call on a Closed start");
        assert_eq!(stored_anchor(&h.st).await, None);

        // The first presence: VALORANT is up (Lobby). A first run (no anchor
        // yet) ends at the oldest entry and the anchor is the newest.
        mock_presence(&h, "MENUS", "unrated").await;
        poller.presence_tick(&h.session).await.unwrap();
        let anchor = wait_for_anchor(&h.st).await.expect("the run finished");
        assert_eq!(anchor.0, crate::pipeline_tests::backfill::entry_id(0));
        assert_eq!(backfill_pages(&h).await, 1);
        poller.stop_worker();
    }

    #[tokio::test]
    async fn the_transition_to_menus_forgets_the_match_tiers_even_if_the_lobby_fetch_fails() {
        // R161: the Agent Select tiers belong to the match that just ended; a
        // failed party fetch (no mock answers it here) must not leave them
        // for the next In Game roster.
        const OTHER: &str = "cccccccc-0000-0000-0000-000000000003";
        let h = harness().await;
        mock_presence(&h, "MENUS", "unrated").await;
        let mut poller = Poller::held(h.st.clone());
        poller.stage = Stage::Ingame;
        poller.presence_stage = Stage::Ingame;
        {
            let mut live = lock(&h.st.live);
            live.stage = Stage::Ingame;
            live.match_tiers
                .set_pregame("pg-1", [(ME.to_string(), 22)].into());
            assert!(!live.match_tiers.is_empty());
            // R165: a names retry is waiting with its wake-up armed.
            live.slots = vec![crate::state::Slot {
                roster: roster(OTHER, false),
                db: Default::default(),
            }];
            live.note_names_failed(&[OTHER.to_string()]);
            live.arm_names_retry_wake();
        }

        poller.presence_tick(&h.session).await.unwrap();

        assert_eq!(lock(&h.st.live).stage, Stage::Menus);
        assert!(lock(&h.st.live).match_tiers.is_empty());
        {
            // The stage reset forgot the failure counts and the wake-up: the
            // player needs a name again, and a stale wake-up must not fire.
            let mut live = lock(&h.st.live);
            if !live.slots.iter().any(|s| s.roster.puuid == OTHER) {
                live.slots.push(crate::state::Slot {
                    roster: roster(OTHER, false),
                    db: Default::default(),
                });
            }
            live.advance_clock(std::time::Duration::from_secs(100));
            assert!(!live.names_needed().is_empty());
            assert!(!live.names_wakeup_due(), "no stale names wake-up");
        }
    }

    #[tokio::test]
    async fn a_match_end_starts_a_backfill_run() {
        // INGAME -> MENUS: the match ended; the backfill runs from the stored
        // anchor (entry 3 of 5): entries 0..3 are fetched, then it moves.
        let h = harness().await;
        let base = crate::pipeline_tests::backfill::base_ms();
        mount_feed(&h.server, Feed::new(5)).await;
        store_anchor(&h.st, 3, base).await;
        mock_presence(&h, "MENUS", "unrated").await;
        let mut poller = Poller::new(h.st.clone());
        poller.stage = Stage::Ingame;
        poller.presence_stage = Stage::Ingame;
        lock(&h.st.live).stage = Stage::Ingame;

        poller.presence_tick(&h.session).await.unwrap();

        for _ in 0..250 {
            if stored_anchor(&h.st).await.map(|a| a.0)
                == Some(crate::pipeline_tests::backfill::entry_id(0))
            {
                break;
            }
            sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(
            stored_anchor(&h.st).await.map(|a| a.0),
            Some(crate::pipeline_tests::backfill::entry_id(0)),
            "the anchor moved to the newest entry"
        );
        assert_eq!(poller.stage, Stage::Menus);
        poller.stop_worker();
    }

    #[tokio::test]
    async fn a_deathmatch_end_starts_a_backfill_run_too() {
        // A free-for-all keeps the Lobby view (`stage` stays Menus through
        // the whole match), but the raw presence still goes INGAME -> MENUS.
        let h = harness().await;
        mount_feed(&h.server, Feed::new(5)).await;
        mock_presence(&h, "MENUS", "deathmatch").await;
        let mut poller = Poller::new(h.st.clone());
        poller.stage = Stage::Menus;
        poller.presence_stage = Stage::Ingame;
        lock(&h.st.live).stage = Stage::Menus;

        poller.presence_tick(&h.session).await.unwrap();

        assert!(wait_for_anchor(&h.st).await.is_some(), "the run finished");
        poller.stop_worker();
    }

    #[tokio::test]
    async fn other_stage_changes_do_not_start_a_backfill_run() {
        let h = harness().await;
        mount_feed(&h.server, Feed::new(5)).await;
        mock_presence(&h, "PREGAME", "unrated").await;
        let mut poller = Poller::new(h.st.clone());
        poller.stage = Stage::Menus;
        poller.presence_stage = Stage::Menus;

        poller.presence_tick(&h.session).await.unwrap();
        sleep(Duration::from_millis(400)).await;

        assert_eq!(backfill_pages(&h).await, 0);
        assert!(!run_active(&h.st));
        assert_eq!(stored_anchor(&h.st).await, None);
        poller.stop_worker();
    }

    #[tokio::test]
    async fn an_account_switch_clears_the_run_but_keeps_the_previous_anchor() {
        let h = harness().await;
        // Slow answers: the new account's run cannot finish before we look.
        let mut feed = Feed::new(50);
        feed.details_delay = Some(Duration::from_millis(100));
        mount_feed(&h.server, feed).await;
        // The previous account's stale run and its anchor.
        let base = crate::pipeline_tests::backfill::base_ms();
        lock(&h.st.live).self_puuid = Some("previous-account".into());
        lock(&h.st.live).start_backfill(Some(("stale".into(), 1)), None);
        store_anchor_for(&h.st, "previous-account", 7, base).await;

        let mut poller = Poller::new(h.st.clone());
        poller.on_connected(h.session.clone()).await;

        assert_eq!(
            stored_anchor_of(&h.st, "previous-account")
                .await
                .map(|a| a.0),
            Some(crate::pipeline_tests::backfill::entry_id(7)),
            "the switch deletes nothing: switching back finds it again"
        );
        assert_eq!(stored_anchor(&h.st).await, None, "the new account has none");
        {
            let live = lock(&h.st.live);
            let run = live.backfill.as_ref().expect("the new account's own run");
            assert_eq!(run.anchor, None, "a first run, not the stale one");
        }
        poller.stop_worker();
    }

    #[tokio::test]
    async fn connecting_queues_the_competitive_updates_run_from_the_accounts_own_anchor() {
        // B1: the same trigger as the match backfill's, with its own anchor
        // per account (the other account's is neither read nor deleted).
        let h = harness().await;
        mount_comp_feed(&h.server, 100, None).await;
        store_comp_anchor_for(&h.st, ME, 40).await;
        store_comp_anchor_for(&h.st, "previous-account", 7).await;
        // The app was last connected as another account, with a run under way.
        lock(&h.st.live).self_puuid = Some("previous-account".into());
        lock(&h.st.live).start_comp_backfill(Some(("stale".into(), 1)), None);
        lock(&h.st.live).comp_backfill.as_mut().unwrap().next_index = 60;

        let mut poller = Poller::new(h.st.clone());
        poller.on_connected(h.session.clone()).await;

        {
            let live = lock(&h.st.live);
            let run = live.comp_backfill.as_ref().expect("this account's own run");
            assert_eq!(
                run.anchor.as_ref().map(|a| a.0.clone()),
                Some(row_id(40)),
                "ME's anchor, not the stale run's"
            );
            assert_eq!(run.next_index, 0, "from the start, not the stale cursor");
            assert!(
                !live.worker_running(),
                "queued only: no worker on a Closed start"
            );
        }
        assert_eq!(
            stored_comp_anchor_of(&h.st, "previous-account")
                .await
                .map(|a| a.0),
            Some(row_id(7)),
            "the switch deletes nothing"
        );
        poller.stop_worker();
    }

    #[tokio::test]
    async fn a_new_account_with_no_anchor_starts_a_first_run_and_dropping_the_session_ends_it() {
        let h = harness().await;
        lock(&h.st.live).self_puuid = Some("previous-account".into());
        let mut poller = Poller::new(h.st.clone());
        poller.on_connected(h.session.clone()).await;
        {
            let live = lock(&h.st.live);
            let run = live.comp_backfill.as_ref().expect("a run");
            assert_eq!(run.anchor, None, "a first run");
            assert!(run.act_start_ms.is_some(), "bounded by the act start");
        }

        poller.drop_session().await;
        assert!(lock(&h.st.live).comp_backfill.is_none());
        poller.stop_worker();
    }

    #[tokio::test]
    async fn a_match_end_starts_the_competitive_updates_run_too() {
        // B1: "after each match end like the backfill": In Game -> Menus.
        let h = harness().await;
        mount_feed(&h.server, Feed::new(1)).await;
        mount_comp_feed(&h.server, 7, None).await;
        let presence = switchable_presence(&h, "INGAME", "unrated").await;
        let mut poller = Poller::new(h.st.clone());
        poller.stage = Stage::Ingame;
        poller.presence_stage = Stage::Ingame;
        poller.party_key = mocked_party_key();
        {
            let mut live = lock(&h.st.live);
            live.stage = Stage::Ingame;
            live.in_match = true;
        }
        poller.presence_tick(&h.session).await.unwrap();
        assert!(
            lock(&h.st.live).comp_backfill.is_none(),
            "no run during the match"
        );

        *presence.lock().unwrap() = ("MENUS".into(), "unrated".into());
        poller.presence_tick(&h.session).await.unwrap();

        // The run was queued and the worker pages it behind the live jobs.
        for _ in 0..250 {
            if stored_comp_anchor(&h.st).await.is_some() {
                break;
            }
            sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(
            stored_comp_anchor(&h.st).await.map(|a| a.0),
            Some(row_id(0)),
            "the run finished after the match"
        );
        poller.stop_worker();
    }

    #[tokio::test]
    async fn a_paused_competitive_updates_run_gets_a_worker_at_the_tick_once_its_pause_is_over() {
        let h = harness().await;
        mount_comp_feed(&h.server, 30, None).await;
        let mut poller = Poller::new(h.st.clone());
        poller.stage = Stage::Menus;
        poller.next_presence = Instant::now() + Duration::from_secs(3600);
        synced_lobby(&h);
        crate::sync::start_comp_backfill_run(&h.st).await;
        lock(&h.st.live).pause_comp_backfill(Duration::from_secs(10));
        let comp_pages = || async {
            requests(&h.server)
                .await
                .iter()
                .filter(|r| r.url.path() == format!("/mmr/v1/players/{ME}/competitiveupdates"))
                .count()
        };

        // Inside the pause: the tick starts nothing.
        poller.connected_tick(&h.session).await.unwrap();
        assert!(!lock(&h.st.live).worker_running());
        assert_eq!(comp_pages().await, 0);

        // The pause is over: the tick wakes a worker, which asks for one page.
        lock(&h.st.live).advance_clock(Duration::from_secs(11));
        poller.connected_tick(&h.session).await.unwrap();
        for _ in 0..250 {
            if comp_pages().await >= 1 && !lock(&h.st.live).worker_running() {
                break;
            }
            sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(comp_pages().await, 1, "one page a job");
        assert_eq!(stored_comp_anchor(&h.st).await, None, "the run is not over");

        // The gap between pages: a tick inside it starts nothing, the first
        // one after it wakes the worker for the last page.
        lock(&h.st.live).advance_clock(Duration::from_millis(500));
        poller.connected_tick(&h.session).await.unwrap();
        assert!(!lock(&h.st.live).worker_running());
        assert_eq!(comp_pages().await, 1);
        lock(&h.st.live).advance_clock(Duration::from_secs(3));
        poller.connected_tick(&h.session).await.unwrap();
        for _ in 0..250 {
            if stored_comp_anchor(&h.st).await.is_some() {
                break;
            }
            sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(comp_pages().await, 2);
        assert_eq!(
            stored_comp_anchor(&h.st).await.map(|a| a.0),
            Some(row_id(0))
        );
        poller.stop_worker();
    }

    #[tokio::test]
    async fn an_account_switch_clears_the_names_fill_queue_and_the_match_end() {
        // R150: the previous account's queued asks and match end are not
        // this account's (their hidden players are forgotten too).
        let h = harness().await;
        {
            let mut live = lock(&h.st.live);
            live.self_puuid = Some("previous-account".into());
            live.match_end_ms = Some(now_ms());
            live.queue_name_fill([opd1_core::db::repo::NameAsk {
                puuid: "someone".into(),
                match_start_ms: None,
            }]);
            assert_eq!(live.name_fill.len(), 1);
        }

        let mut poller = Poller::new(h.st.clone());
        poller.on_connected(h.session.clone()).await;

        let live = lock(&h.st.live);
        assert!(live.name_fill.is_empty());
        assert_eq!(live.match_end_ms, None);
        drop(live);
        poller.stop_worker();
    }

    #[tokio::test]
    async fn a_match_end_arms_the_names_fill_and_a_new_match_disarms_it() {
        // R150: In Game -> Menus (a Deathmatch included) records the match
        // end; the next match starting drops it, so a later match is never
        // taken for the one that ended.
        for queue in ["unrated", "deathmatch"] {
            let h = harness().await;
            let presence = switchable_presence(&h, "INGAME", queue).await;
            let mut poller = Poller::held(h.st.clone());
            poller.stage = if queue == "deathmatch" {
                Stage::Menus
            } else {
                Stage::Ingame
            };
            poller.presence_stage = Stage::Ingame;
            poller.party_key = mocked_party_key();
            lock(&h.st.live).stage = poller.stage;
            lock(&h.st.live).in_match = true;
            lock(&h.st.live).match_end_ms = Some(1);
            poller.presence_tick(&h.session).await.unwrap();
            assert_eq!(
                lock(&h.st.live).match_end_ms,
                None,
                "{queue}: a match is running: no match end on record"
            );

            let before = now_ms();
            *presence.lock().unwrap() = ("MENUS".into(), queue.into());
            poller.presence_tick(&h.session).await.unwrap();
            let end = lock(&h.st.live).match_end_ms.expect("armed at the end");
            assert!(end >= before && end <= now_ms(), "{queue}: ends now");

            // Still in the Lobby a tick later: unchanged.
            poller.presence_tick(&h.session).await.unwrap();
            assert_eq!(lock(&h.st.live).match_end_ms, Some(end));

            *presence.lock().unwrap() = ("PREGAME".into(), queue.into());
            poller.presence_tick(&h.session).await.unwrap();
            assert_eq!(lock(&h.st.live).match_end_ms, None, "{queue}: new match");
        }
    }

    #[tokio::test]
    async fn a_queued_fill_gets_a_worker_at_the_next_tick() {
        // A command can't start the worker (the poll loop owns its handle):
        // it queues and the next tick starts one — in the Lobby, not
        // in a match.
        let h = harness().await;
        Mock::given(method("PUT"))
            .and(path("/name-service/v2/players"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
            .with_priority(1)
            .mount(&h.server)
            .await;
        let mut poller = Poller::new(h.st.clone());
        poller.stage = Stage::Menus;
        poller.next_presence = Instant::now() + Duration::from_secs(3600);
        synced_lobby(&h);
        let asks = || {
            vec![opd1_core::db::repo::NameAsk {
                puuid: "cccccccc-0000-0000-0000-000000000009".into(),
                match_start_ms: None,
            }]
        };

        // In a match: the tick starts nothing, the ask waits.
        lock(&h.st.live).in_match = true;
        h.st.queue_name_fill(asks());
        poller.connected_tick(&h.session).await.unwrap();
        assert!(!lock(&h.st.live).worker_running());
        assert_eq!(lock(&h.st.live).name_fill.len(), 1);

        // The match ended and a roster install starts the worker (the kick
        // was spent in the match): it asks.
        lock(&h.st.live).in_match = false;
        h.st.queue_name_fill(asks());
        poller.connected_tick(&h.session).await.unwrap();
        for _ in 0..250 {
            if !requests(&h.server)
                .await
                .iter()
                .all(|r| r.method.as_str() != "PUT")
            {
                break;
            }
            sleep(Duration::from_millis(20)).await;
        }
        assert!(
            requests(&h.server)
                .await
                .iter()
                .any(|r| r.method.as_str() == "PUT"),
            "the tick started a worker that asked name-service"
        );
        poller.stop_worker();
    }

    /// Like [`mock_presence`], the private blob also carrying
    /// `partyClientVersion` (nested, as the game sends it).
    async fn mock_presence_with_version(h: &Harness, state: &str, version: &str, times: u64) {
        let private = BASE64.encode(
            json!({
                "sessionLoopState": state, "queueId": "competitive", "partyId": "party-1",
                "partyPresenceData": {"partyClientVersion": version},
            })
            .to_string(),
        );
        Mock::given(method("GET"))
            .and(path("/chat/v4/presences"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "presences": [{"puuid": ME, "product": "valorant", "private": private}]
            })))
            .up_to_n_times(times)
            .with_priority(1)
            .mount(&h.server)
            .await;
    }

    /// The `X-Riot-ClientVersion` of the newest request to `p`.
    async fn last_version_header(h: &Harness, p: &str) -> String {
        let reqs = requests(&h.server).await;
        let r = reqs
            .iter()
            .rev()
            .find(|r| r.url.path() == p)
            .expect("asked");
        r.headers
            .get("x-riot-clientversion")
            .expect("header")
            .to_str()
            .unwrap()
            .to_string()
    }

    #[tokio::test]
    async fn the_first_valorant_presence_re_resolves_the_client_version_once() {
        // R159: the session was built before the game's presence existed (the
        // harness's own sources are all empty: the literal fallback). The
        // first presence with a `partyClientVersion` — read by the tick, no
        // request of ours — puts the game's own value on the next request.
        let h = harness().await;
        assert_eq!(
            h.session.client_version_source(),
            opd1_riot::session::VersionSource::Fallback
        );
        let mut poller = Poller::new(h.st.clone());
        poller.stage = Stage::Menus;
        poller.presence_stage = Stage::Menus;
        poller.party_key = mocked_party_key();
        poller.next_presence = Instant::now() + Duration::from_secs(3600);
        synced_lobby(&h);
        mock_presence_with_version(&h, "MENUS", "release-13.06-shipping-18-5590001", 1).await;

        let asked_before = requests(&h.server).await.len();
        poller.presence_tick(&h.session).await.unwrap();
        assert_eq!(
            requests(&h.server)
                .await
                .iter()
                .skip(asked_before)
                .filter(|r| r.url.path() != "/chat/v4/presences")
                .count(),
            0,
            "the tick's own presence read is the only request it makes"
        );
        assert_eq!(
            h.session.client_version(),
            "release-13.06-shipping-18-5590001"
        );
        assert_eq!(
            h.session.client_version_source(),
            opd1_riot::session::VersionSource::Presence
        );
        h.session.mmr(ME).await.ok();
        assert_eq!(
            last_version_header(&h, &mmr_of(ME)).await,
            "release-13.06-shipping-18-5590001",
            "the next remote request carries it"
        );

        // Once per session: a later presence with another value changes nothing.
        mock_presence_with_version(&h, "MENUS", "release-other-0", 10).await;
        poller.presence_tick(&h.session).await.unwrap();
        assert_eq!(
            h.session.client_version(),
            "release-13.06-shipping-18-5590001"
        );
        poller.stop_worker();
    }

    #[tokio::test]
    async fn a_first_presence_without_a_version_does_not_use_up_the_re_resolve() {
        // R159 review: the turn is only spent by a presence that had a usable
        // `partyClientVersion`; otherwise the session would keep the lower
        // source's value for its whole life.
        let h = harness().await;
        let mut poller = Poller::new(h.st.clone());
        poller.stage = Stage::Menus;
        poller.presence_stage = Stage::Menus;
        poller.party_key = mocked_party_key();
        poller.next_presence = Instant::now() + Duration::from_secs(3600);
        synced_lobby(&h);
        // The first presences carry a blank version; the third has the real one.
        mock_presence_with_version(&h, "MENUS", "  ", 2).await;
        mock_presence_with_version(&h, "MENUS", "release-13.06-shipping-18-5590001", 1).await;
        mock_presence_with_version(&h, "MENUS", "release-other-0", 10).await;
        let before = h.session.client_version();

        for _ in 0..2 {
            poller.presence_tick(&h.session).await.unwrap();
            assert_eq!(h.session.client_version(), before, "nothing usable yet");
            assert!(!poller.version_from_presence_done);
        }
        poller.presence_tick(&h.session).await.unwrap();
        assert_eq!(
            h.session.client_version(),
            "release-13.06-shipping-18-5590001"
        );
        assert!(poller.version_from_presence_done);
        h.session.mmr(ME).await.ok();
        assert_eq!(
            last_version_header(&h, &mmr_of(ME)).await,
            "release-13.06-shipping-18-5590001",
            "the request after the second presence carries it"
        );
        // And now it is done: a later value changes nothing.
        poller.presence_tick(&h.session).await.unwrap();
        assert_eq!(
            h.session.client_version(),
            "release-13.06-shipping-18-5590001"
        );
        poller.stop_worker();
    }

    /// The mmr path of `puuid`.
    fn mmr_of(puuid: &str) -> String {
        format!("/mmr/v1/players/{puuid}")
    }

    /// `ME`'s `mmr` request always fails (a 500).
    async fn mmr_fails(h: &Harness) {
        Mock::given(method("GET"))
            .and(path(mmr_of(ME).as_str()))
            .respond_with(ResponseTemplate::new(500))
            .with_priority(1)
            .mount(&h.server)
            .await;
    }

    /// How often `ME`'s mmr has been asked so far.
    async fn mmr_requests(h: &Harness) -> usize {
        requests(&h.server)
            .await
            .iter()
            .filter(|r| r.url.path() == mmr_of(ME))
            .count()
    }

    /// Waits (up to ~5s) until `ME`'s mmr has been asked `n` times.
    async fn wait_for_mmr_requests(h: &Harness, n: usize) -> usize {
        let mut seen = 0;
        for _ in 0..250 {
            seen = mmr_requests(h).await;
            if seen >= n {
                break;
            }
            sleep(Duration::from_millis(20)).await;
        }
        seen
    }

    #[tokio::test]
    async fn a_due_rank_retry_gets_a_worker_at_the_next_tick() {
        // R155: the worker exits when nothing else is queued and never waits
        // for a retry: the poll loop's tick is what starts one when it is due.
        let h = harness().await;
        mmr_fails(&h).await;
        let mut poller = Poller::new(h.st.clone());
        poller.stage = Stage::Menus;
        poller.next_presence = Instant::now() + Duration::from_secs(3600);
        {
            let mut live = lock(&h.st.live);
            live.stage = Stage::Menus;
            live.slots = vec![Slot {
                roster: roster(ME, false),
                db: PlayerDb::default(),
            }];
        }
        // The first pass (the tick's roster-install path is not under test).
        let id = lock(&h.st.live).try_start_worker().expect("first pass");
        crate::sync::run_worker(h.st.clone(), h.session.clone(), id).await;
        assert_eq!(mmr_requests(&h).await, 1);

        // Not due: the tick starts nothing.
        poller.connected_tick(&h.session).await.unwrap();
        assert!(!lock(&h.st.live).worker_running());
        lock(&h.st.live).advance_clock(Duration::from_secs(19));
        poller.connected_tick(&h.session).await.unwrap();
        assert!(!lock(&h.st.live).worker_running(), "19 s: not yet");

        // Due, with nothing else queued: the next tick starts a worker.
        lock(&h.st.live).advance_clock(Duration::from_secs(1));
        poller.connected_tick(&h.session).await.unwrap();
        assert_eq!(wait_for_mmr_requests(&h, 2).await, 2, "the retry ran");
        poller.stop_worker();
    }

    #[tokio::test]
    async fn a_names_fill_held_by_the_hold_runs_again_after_it_without_any_other_event() {
        // R160: name-service is refused while the `other` family is on hold.
        // The asks stay queued, the worker exits (the deferral empties its job
        // list) and nothing else is going to happen: the tick must start a
        // worker once the deferral has run out, exactly once.
        const OTHER: &str = "cccccccc-0000-0000-0000-000000000003";
        let h = harness().await;
        let session = session_with_other_wait(&h, Duration::from_millis(100)).await;
        Mock::given(method("PUT"))
            .and(path("/name-service/v2/players"))
            .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "1.5"))
            .up_to_n_times(1)
            .with_priority(1)
            .mount(&h.server)
            .await;
        let puts = || async {
            requests(&h.server)
                .await
                .iter()
                .filter(|r| {
                    r.method.as_str() == "PUT" && r.url.path() == "/name-service/v2/players"
                })
                .count()
        };
        let visible = [opd1_riot::VisiblePuuid::new(OTHER.to_string(), false).unwrap()];
        let hold_set = Instant::now();
        assert!(
            session.names(&visible).await.is_err(),
            "the 429 sets the hold"
        );
        assert_eq!(puts().await, 1);

        let mut poller = Poller::new(h.st.clone());
        poller.stage = Stage::Menus;
        poller.next_presence = Instant::now() + Duration::from_secs(3600);
        {
            let mut live = lock(&h.st.live);
            live.stage = Stage::Menus;
            live.slots = vec![Slot {
                roster: roster(ME, false),
                db: PlayerDb::default(),
            }];
            live.synced.insert(ME.to_string());
            live.names_attempted.insert(ME.to_string());
        }
        h.st.queue_name_fill(vec![opd1_core::db::repo::NameAsk {
            puuid: OTHER.to_string(),
            match_start_ms: None,
        }]);

        // The kick starts a worker; the fill is refused locally and deferred.
        poller.connected_tick(&session).await.unwrap();
        for _ in 0..250 {
            if !lock(&h.st.live).worker_running() {
                break;
            }
            sleep(Duration::from_millis(20)).await;
        }
        assert!(!lock(&h.st.live).worker_running(), "the worker exited");
        assert_eq!(puts().await, 1, "the held fill sent nothing");
        assert_eq!(
            lock(&h.st.live).name_fill.len(),
            1,
            "the ask is still queued"
        );

        // Deferred: the tick starts nothing.
        poller.connected_tick(&session).await.unwrap();
        assert!(!lock(&h.st.live).worker_running());
        assert_eq!(puts().await, 1);

        // The deferral and the real hold run out, with no other event.
        // (The real hold is 1.5 s against a 100 ms bound: the refusals above
        // have a 1.4 s margin on a slow runner.)
        sleep(Duration::from_millis(1_600).saturating_sub(hold_set.elapsed())).await;
        lock(&h.st.live).advance_clock(Duration::from_secs(3));
        poller.connected_tick(&session).await.unwrap();
        for _ in 0..250 {
            if puts().await >= 2 {
                break;
            }
            sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(puts().await, 2, "the fill ran again");
        poller.stop_worker();
        assert!(
            !lock(&h.st.live).names_wakeup_due(),
            "one-shot: nothing more to wake"
        );
    }

    #[tokio::test]
    async fn no_tick_starts_a_retry_while_closed_and_closing_drops_it() {
        let h = harness().await;
        mmr_fails(&h).await;
        let mut poller = Poller::new(h.st.clone());
        poller.stage = Stage::Menus;
        poller.next_presence = Instant::now() + Duration::from_secs(3600);
        {
            let mut live = lock(&h.st.live);
            live.stage = Stage::Menus;
            live.slots = vec![Slot {
                roster: roster(ME, false),
                db: PlayerDb::default(),
            }];
        }
        let id = lock(&h.st.live).try_start_worker().expect("first pass");
        crate::sync::run_worker(h.st.clone(), h.session.clone(), id).await;
        assert_eq!(mmr_requests(&h).await, 1);
        assert!(!lock(&h.st.live).rank_retries.is_empty());

        // VALORANT closes: the retry is dropped with the session's roster.
        poller.enter_closed().await;
        assert!(lock(&h.st.live).rank_retries.is_empty());
        lock(&h.st.live).advance_clock(Duration::from_secs(1_000));
        poller.stage = Stage::Closed;
        poller.connected_tick(&h.session).await.unwrap();
        assert!(!lock(&h.st.live).worker_running());
        sleep(Duration::from_millis(200)).await;
        assert_eq!(mmr_requests(&h).await, 1, "no retry ran");
    }

    #[tokio::test]
    async fn an_account_switch_drops_the_pending_retries() {
        let h = harness().await;
        {
            let mut live = lock(&h.st.live);
            live.self_puuid = Some("previous-account".into());
            live.rank_retries
                .first_pass("someone", true, std::time::Instant::now());
        }
        let mut poller = Poller::new(h.st.clone());
        poller.on_connected(h.session.clone()).await;
        assert!(lock(&h.st.live).rank_retries.is_empty());
        poller.stop_worker();
    }

    #[tokio::test]
    async fn switching_back_to_an_account_finds_its_anchor() {
        let h = harness().await;
        let mut feed = Feed::new(50);
        feed.details_delay = Some(Duration::from_millis(100));
        mount_feed(&h.server, feed).await;
        let base = crate::pipeline_tests::backfill::base_ms();
        // The app was last connected as another account; ME's anchor is from
        // an earlier session.
        lock(&h.st.live).self_puuid = Some("previous-account".into());
        store_anchor(&h.st, 40, base).await;

        let mut poller = Poller::new(h.st.clone());
        poller.on_connected(h.session.clone()).await;

        let run_anchor = lock(&h.st.live)
            .backfill
            .as_ref()
            .and_then(|r| r.anchor.clone());
        assert_eq!(
            run_anchor.map(|a| a.0),
            Some(crate::pipeline_tests::backfill::entry_id(40)),
            "the run resumes from ME's own anchor"
        );
        assert_eq!(
            stored_anchor(&h.st).await.map(|a| a.0),
            Some(crate::pipeline_tests::backfill::entry_id(40))
        );
        poller.stop_worker();
    }

    #[tokio::test]
    async fn reconnecting_the_same_account_keeps_its_anchor() {
        let h = harness().await;
        let mut feed = Feed::new(50);
        feed.details_delay = Some(Duration::from_millis(100));
        mount_feed(&h.server, feed).await;
        let base = crate::pipeline_tests::backfill::base_ms();
        lock(&h.st.live).self_puuid = Some(ME.into());
        store_anchor(&h.st, 40, base).await;

        let mut poller = Poller::new(h.st.clone());
        poller.on_connected(h.session.clone()).await;

        assert_eq!(
            stored_anchor(&h.st).await.map(|a| a.0),
            Some(crate::pipeline_tests::backfill::entry_id(40))
        );
        let run_anchor = lock(&h.st.live)
            .backfill
            .as_ref()
            .and_then(|r| r.anchor.clone());
        assert_eq!(
            run_anchor.map(|a| a.0),
            Some(crate::pipeline_tests::backfill::entry_id(40))
        );
        poller.stop_worker();
    }

    #[tokio::test]
    async fn dropping_the_session_ends_the_run() {
        let h = harness().await;
        lock(&h.st.live).start_backfill(None, None);
        let mut poller = Poller::new(h.st.clone());
        poller.session = Some(h.session.clone());
        poller.drop_session().await;
        assert!(!run_active(&h.st), "the next connect starts over");
    }

    /// R100 through the real `apply_roster`: the user already has a `players`
    /// row (level 0: it was created before their level was known) and the
    /// app started with the Closed-stage slot on screen, so the user is not a
    /// "new" player when the roster arrives. Their level (87, with
    /// `HideAccountLevel` on) must still reach the DB, or the Closed card
    /// the app opens on shows `Lv —`.
    #[tokio::test]
    async fn own_level_is_stored_when_the_closed_slot_is_already_on_screen() {
        let h = harness().await;
        let now = now_ms();
        h.st.db(move |repo| {
            save_player(
                repo.conn(),
                &PlayerUpdate {
                    puuid: ME.into(),
                    name: Some(("Me".into(), "OPD1".into())),
                    account_level: 0,
                    card_id: "card".into(),
                },
                now,
            )
        })
        .await
        .unwrap();
        // What `lib.rs` does at startup.
        let db =
            h.st.db(|repo| load_player_db(repo, Some(ME), ME))
                .await
                .unwrap();
        lock(&h.st.live).slots = vec![closed_self_slot(ME, db).unwrap()];

        let mut me = roster(ME, false);
        me.hide_account_level = true;
        me.account_level = 87;
        let mut poller = Poller::held(h.st.clone());
        poller
            .apply_roster(&h.session, Stage::Menus, None, vec![me])
            .await;

        let stored =
            h.st.db(|repo| load_player_db(repo, Some(ME), ME))
                .await
                .unwrap();
        assert_eq!(stored.row.unwrap().account_level, 87, "stored in the DB");
        let on_screen = lock(&h.st.live)
            .slots
            .iter()
            .find(|s| s.roster.puuid == ME)
            .and_then(|s| s.db.row.clone())
            .unwrap();
        assert_eq!(
            on_screen.account_level, 87,
            "the on-screen row follows, so the next roster does not re-save"
        );

        // The app goes back to Closed and re-reads the row.
        poller.enter_closed().await;
        let card = card_json(&h.st, ME);
        assert_eq!(card["isSelf"], json!(true));
        assert_eq!(card["level"], json!(87));
    }
}
