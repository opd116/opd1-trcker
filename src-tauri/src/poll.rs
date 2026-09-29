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
//!   presence (local) every 2s (5s while closed) ───────────┘
//!   │  own VALORANT presence absent twice → stage Closed (own card from DB)
//!   │  view_stage(sessionLoopState, queueId) change → plan_transition():
//!   │     MENUS   → party roster   (+ re-fetch on partyId/partySize change)
//!   │     PREGAME → ally team      (+ re-poll every 3s for agent picks, R39)
//!   │     INGAME  → both teams + loadouts
//!   │     PREGAME/INGAME in a free-for-all queue → stays MENUS (R41)
//!   │     INGAME→MENUS = match end: back to Lobby + post-match follow-up
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
    connect_retry_delay, extract_loadouts, from_coregame, from_party, from_pregame, is_ffa_roster,
    order_players, party_key, plan_transition, view_stage, GameVerdict, GameWatch, PartyKey,
    RosterFetch, RosterPlayer,
};
use crate::state::{closed_self_slot, lock, now_ms, AppState, Slot};
use crate::store::{load_player_db, match_landed, save_player, PlayerDb, SELF_PUUID_KEY};

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
/// Roster retry backoff: 3s, 6s, 12s, … capped.
const ROSTER_RETRY_BASE: Duration = Duration::from_secs(3);
const ROSTER_RETRY_MAX: Duration = Duration::from_secs(30);
/// After a match ends, Riot needs a moment to publish it: re-sync the party
/// at these delays until the ended match has landed (R48: for competitive,
/// its `comp_updates` row — RR/rank — not just its details).
const POST_MATCH_DELAYS: [Duration; 2] = [Duration::from_secs(30), Duration::from_secs(90)];

/// Why the connected tick stopped.
enum Stop {
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

struct Poller {
    st: Arc<AppState>,
    local_app_data: Option<PathBuf>,
    session: Option<Arc<RiotSession>>,
    /// The stage being shown (`roster::view_stage` of the presence).
    stage: Stage,
    /// The raw presence stage last seen (differs from `stage` in FFA).
    presence_stage: Stage,
    /// VALORANT's own presence under this session (R46).
    game: GameWatch,
    /// The presence `queueId` while in a match — decides the post-match
    /// check for the match that just ended (R48).
    match_queue: Option<String>,
    presence_errors: u32,
    party_key: Option<PartyKey>,
    self_party: HashSet<String>,
    pregame_match_id: Option<String>,
    next_presence: Instant,
    next_pregame: Option<Instant>,
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
    fn new(st: Arc<AppState>) -> Self {
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
            match_queue: None,
            presence_errors: 0,
            party_key: None,
            self_party: HashSet::new(),
            pregame_match_id: None,
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

    async fn on_connected(&mut self, session: Arc<RiotSession>) {
        let me = session.self_puuid().to_string();
        log::info!("connected to the Riot client as {}", short(&me));
        self.last_connect_error = None;
        {
            let mut live = lock(&self.st.live);
            if live.self_puuid.as_deref() != Some(me.as_str()) {
                // A different account: nothing learned about the previous
                // one's session applies.
                live.synced.clear();
                live.party_ready.clear();
                live.names_attempted.clear();
                live.self_roster = None;
                live.self_name = None;
            }
            live.self_puuid = Some(me.clone());
            live.self_name_attempted = false;
        }
        let key_value = me.clone();
        self.st
            .db(move |repo| repo.set_state(SELF_PUUID_KEY, &key_value))
            .await;
        self.st.set_session(Some(session.clone()));
        self.session = Some(session.clone());
        self.game = GameWatch::default();
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
    }

    async fn drop_session(&mut self) {
        self.stop_worker();
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
                    let delay = ROSTER_RETRY_BASE
                        .saturating_mul(1 << attempt.min(4))
                        .min(ROSTER_RETRY_MAX);
                    self.roster_retry = Some((Instant::now() + delay, attempt + 1));
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

    async fn presence_tick(&mut self, session: &Arc<RiotSession>) -> Result<(), Stop> {
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
            GameVerdict::Present => {}
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

        if presence.stage == Stage::Ingame {
            self.match_queue = presence.queue_id.clone();
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
        Ok(())
    }

    fn stop_worker(&mut self) {
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
        // A worker task that ended without releasing its claim (it can
        // only panic in a debug build) must not block all future work.
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
            _ => None,
        };
        self.stage = next;
        self.party_key = key;
        self.pregame_match_id = None;
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
            self.roster_retry = Some((Instant::now() + ROSTER_RETRY_BASE, 1));
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
    /// started mid-match and never saw the lobby).
    async fn refresh_self_party(&mut self, session: &Arc<RiotSession>) -> Result<(), Stop> {
        let me = session.self_puuid().to_string();
        match self.party_response(session, &me).await {
            Ok(party) => {
                self.self_party = party.members.iter().map(|m| m.subject.clone()).collect();
                Ok(())
            }
            Err(RiotError::ClientGone) => Err(Stop::Reconnect("Riot client closed")),
            Err(e) => {
                log::warn!("party refresh failed: {e}");
                Ok(())
            }
        }
    }

    async fn fetch_pregame(
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
        self.pregame_match_id = Some(match_id);
        self.next_pregame = Some(Instant::now() + PREGAME_REPOLL);
        self.apply_roster(session, Stage::Pregame, Some(m.map_id), roster)
            .await;
        Ok(())
    }

    /// R39: re-read the pregame match only to pick up agent selections
    /// (applied in place; the worker isn't restarted).
    async fn pregame_repoll(&mut self, session: &Arc<RiotSession>) -> Result<(), Stop> {
        let Some(match_id) = self.pregame_match_id.clone() else {
            return Ok(());
        };
        match session.pregame_match(&match_id).await {
            Ok(m) => {
                let me = session.self_puuid().to_string();
                let roster = from_pregame(&m, &me, &self.self_party);
                self.apply_roster(session, Stage::Pregame, Some(m.map_id), roster)
                    .await;
                Ok(())
            }
            Err(RiotError::ClientGone) => Err(Stop::Reconnect("Riot client closed")),
            // Typically 404 right as Agent Select ends — presence moves on.
            Err(e) => {
                log::debug!("pregame re-poll: {e}");
                Ok(())
            }
        }
    }

    async fn fetch_ingame(
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
            return self.fetch_party(session, me).await;
        }
        let roster = from_coregame(&m, me, &self.self_party);
        let loadouts = match session.coregame_loadouts(&match_id).await {
            Ok(resp) => Some(extract_loadouts(&resp)),
            Err(RiotError::ClientGone) => return Err(RiotError::ClientGone),
            Err(e) => {
                log::warn!("loadouts failed: {e}");
                None
            }
        };
        log::info!(
            "in game: {} players, match {}",
            roster.len(),
            short(&match_id)
        );
        {
            let mut live = lock(&self.st.live);
            live.coregame_match_id = Some(match_id);
            live.loadouts = loadouts.unwrap_or_default();
        }
        self.apply_roster(session, Stage::Ingame, Some(m.map_id), roster)
            .await;
        Ok(())
    }

    /// Installs a roster: cached-first slots (DB) → publish → worker.
    /// Players already on screen keep their DB bundle; only new ones are
    /// read (and get a `players` row upsert), plus the user's own row when
    /// their level is new or changed (R100).
    async fn apply_roster(
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
        }
        self.st.publish();
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
            live.map_path = None;
            live.map_id = None;
            live.coregame_match_id = None;
            live.loadouts.clear();
            live.slots = slot.into_iter().collect();
        }
        self.st.publish();
    }

    /// A match ended: once Riot has published it, the user's party needs a
    /// re-sync (their last-5/RR changed). Checks whether the ended match
    /// is in the DB yet; if not, re-syncs the party and tries again later.
    async fn post_match_check(&mut self, session: &Arc<RiotSession>) {
        let Some(pm) = self.post_match.take() else {
            return;
        };
        if self.stage != Stage::Menus {
            return;
        }
        let Some(me) = lock(&self.st.live).self_puuid.clone() else {
            return;
        };
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
                .filter(|s| s.roster.is_self_party)
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

    use super::*;
    use crate::pipeline_tests::{card_json, harness, roster, ME};
    use crate::store::{save_player, PlayerUpdate};

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
