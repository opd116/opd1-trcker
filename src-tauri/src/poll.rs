//! The live poll loop — one Tokio task started in `setup`.
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
    connect_retry_delay, extract_loadouts, extract_pregame_loadouts, from_coregame, from_party,
    from_pregame, is_ffa_roster, order_players, party_key, plan_transition, view_stage,
    want_pregame_loadouts, GameVerdict, GameWatch, PartyKey, RosterFetch, RosterPlayer,
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

pub fn start(st: Arc<AppState>) {
    tauri::async_runtime::spawn(async move {
        Poller::new(st).run().await;
    });
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
    next_presence: Instant,
    next_pregame: Option<Instant>,
    /// When the next roster retry is due and how many fetches failed so far.
    roster_retry: Option<(Instant, u32)>,
    post_match: Option<PostMatch>,
    /// The sync worker's task and the id its claim in `LiveModel` has.
    worker: Option<(JoinHandle<()>, u64)>,
    last_connect_error: Option<String>,
    /// R47: consecutive connect failures and when the next attempt is due.
    connect_failures: u32,
    next_connect: Instant,
    /// The lockfile's mtime at the last check — a rewritten lockfile (the
    /// Riot Client restarted) retries at once, whatever the backoff.
    lockfile_mtime: Option<SystemTime>,
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
            version_from_presence_done: false,
            match_queue: None,
            presence_errors: 0,
            party_key: None,
            self_party: HashSet::new(),
            pregame_match_id: None,
            pregame_loadouts_valid: false,
            next_presence: Instant::now(),
            next_pregame: None,
            roster_retry: None,
            post_match: None,
            worker: None,
            last_connect_error: None,
            connect_failures: 0,
            next_connect: Instant::now(),
            lockfile_mtime: None,
        }
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
            sleep(CLOSED_CHECK).await;
            return;
        };
        let lockfile = lockfile_path(&lad);
        let Ok(meta) = std::fs::metadata(&lockfile) else {
            // Riot Client closed: back to the plain 5s existence check.
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
                // R150: nor are its queued name asks and its last match end
                // (their hidden players were just forgotten above).
                live.name_fill.clear();
                live.match_end_ms = None;
                // R155: nor are its pending rank retries.
                live.rank_retries.clear();
            }
            live.self_puuid = Some(me.clone());
            live.self_name_attempted = false;
            switched
        };
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
        crate::sync::start_backfill_run(&self.st).await;
    }

    /// R101: a match ended (VALORANT is running): starts the own-history
    /// backfill from the stored anchor (a run already in progress keeps
    /// going) and makes sure a worker will run it — behind any live card
    /// work, which `LiveModel::next_job` puts first.
    async fn begin_backfill(&mut self, session: &Arc<RiotSession>) {
        crate::sync::start_backfill_run(&self.st).await;
        self.ensure_worker(session);
    }

    async fn drop_session(&mut self) {
        self.stop_worker();
        // R101: an interrupted run is dropped; the next connect starts over
        // from the stored anchor, which only a finished run moves.
        lock(&self.st.live).backfill = None;
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
        let private = presences
            .presences
            .iter()
            .find(|p| p.puuid == me && p.product == "valorant")
            .and_then(|p| p.private.clone());
        let decoded = private.as_deref().and_then(decode_presence);

        match self.game.on_presence(decoded.is_some()) {
            GameVerdict::Present => {
                self.read_self_loadout_once(session);
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

    async fn fetch_party(&mut self, session: &Arc<RiotSession>, me: &str) -> Result<(), RiotError> {
        let party = self.party_response(session, me).await?;
        let roster = from_party(&party);
        self.self_party = roster.iter().map(|p| p.puuid.clone()).collect();
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
        let roster = from_coregame(&m, me, &self.self_party);
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
        match session.coregame_loadouts(&match_id).await {
            Ok(resp) => {
                let found = extract_loadouts(&resp);
                if !found.is_empty() {
                    lock(&self.st.live).loadouts.extend(found);
                    // No card shows skins: an open Details is rebuilt.
                    crate::details::refresh_open_after_live_change(&self.st).await;
                }
            }
            Err(RiotError::ClientGone) => return Err(RiotError::ClientGone),
            Err(e) => log::warn!("loadouts failed: {e}"),
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

    /// Closed: the user's own card from the DB (nothing if unknown).
    async fn enter_closed(&mut self) {
        self.stop_worker();
        self.stage = Stage::Closed;
        let me = lock(&self.st.live).self_puuid.clone();
        let slot = match me {
            Some(me) => {
                let p = me.clone();
                let db = self
                    .st
                    .db(move |repo| load_player_db(repo, Some(&p), &p))
                    .await
                    .unwrap_or_default();
                closed_self_slot(&me, db)
            }
            None => None,
        };
        {
            let mut live = lock(&self.st.live);
            live.stage = Stage::Closed;
            live.in_match = false;
            live.map_path = None;
            live.map_id = None;
            live.coregame_match_id = None;
            live.loadouts.clear();
            live.slots = slot.into_iter().collect();
            // R155: nothing is retried while VALORANT is closed.
            live.prune_rank_retries();
        }
        self.st.publish();
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
    use crate::pipeline_tests::{card_json, harness, roster, Harness, ME};
    use crate::store::{save_player, PlayerUpdate};

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
        let mut poller = Poller::new(h.st.clone());
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
            poller.stop_worker();
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
            mount_feed(&h.server, Feed::new(1)).await;
            let presence = switchable_presence(&h, "INGAME", queue).await;
            let mut poller = Poller::new(h.st.clone());
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
            poller.stop_worker();
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
        let mut poller = Poller::new(h.st.clone());
        poller
            .apply_roster(&h.session, Stage::Menus, None, vec![me])
            .await;
        poller.stop_worker();

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
