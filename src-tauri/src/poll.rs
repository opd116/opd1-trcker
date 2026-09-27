//! The live poll loop — one Tokio task started in `setup`.
//!
//! ```text
//!            lockfile missing (check every 5s — the ONLY idle work)
//!   ┌──────── Disconnected ◄───────────────────────────────┐
//!   │  lockfile present → RiotSession::connect             │ ClientGone /
//!   │  (fail → retry in 5s)                                │ 5 presence errors /
//!   ▼                                                      │ game started after
//! Connected: presence (local) every 2s (5s while closed) ──┘ a game-less connect
//!   │  own VALORANT presence absent twice → stage Closed (own card from DB)
//!   │  sessionLoopState change → plan_transition():
//!   │     MENUS   → party roster   (+ re-fetch on partyId/partySize change)
//!   │     PREGAME → ally team      (+ re-poll every 3s for agent picks, R39)
//!   │     INGAME  → both teams + loadouts
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
use std::time::Duration;

use tauri::async_runtime::JoinHandle;
use tokio::time::{sleep, sleep_until, Instant};

use opd1_core::presence::{decode_presence, Stage};
use opd1_riot::{RiotError, RiotSession};

use crate::logbuf::short;
use crate::roster::{
    extract_loadouts, from_coregame, from_party, from_pregame, order_players, party_key,
    plan_transition, PartyKey, RosterFetch, RosterPlayer,
};
use crate::state::{closed_self_slot, lock, now_ms, AppState, Slot};
use crate::store::{load_player_db, save_player, PlayerDb, PlayerUpdate, SELF_PUUID_KEY};

/// Closed: how often the lockfile's existence is checked.
const CLOSED_CHECK: Duration = Duration::from_secs(5);
/// Presence cadence while VALORANT is running / while it isn't.
const PRESENCE_EVERY: Duration = Duration::from_secs(2);
const PRESENCE_EVERY_IDLE: Duration = Duration::from_secs(5);
/// R39: Agent Select re-poll, only to pick up agent selections.
const PREGAME_REPOLL: Duration = Duration::from_secs(3);
/// Consecutive absent presences before declaring VALORANT closed (avoids
/// flashing the Closed stage on one odd presence payload).
const ABSENT_TICKS_TO_CLOSE: u32 = 2;
/// Consecutive presence failures (other than ClientGone) before the
/// session is dropped and rebuilt.
const PRESENCE_ERRORS_TO_RECONNECT: u32 = 5;
/// Roster retry backoff: 3s, 6s, 12s, … capped.
const ROSTER_RETRY_BASE: Duration = Duration::from_secs(3);
const ROSTER_RETRY_MAX: Duration = Duration::from_secs(30);
/// After a match ends, Riot needs a moment to publish its details: re-sync
/// the party at these delays until the ended match is in the DB.
const POST_MATCH_DELAYS: [Duration; 2] = [Duration::from_secs(30), Duration::from_secs(90)];

/// Why the connected tick stopped.
enum Stop {
    /// Drop the session and go back to Disconnected.
    Reconnect(&'static str),
}

struct PostMatch {
    match_id: String,
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
    stage: Stage,
    /// `Some(true)` when this session was connected while VALORANT itself
    /// wasn't running (its shards/version may be stale → reconnect once
    /// the game shows up).
    connected_without_game: Option<bool>,
    absent_ticks: u32,
    presence_errors: u32,
    party_key: Option<PartyKey>,
    self_party: HashSet<String>,
    pregame_match_id: Option<String>,
    next_presence: Instant,
    next_pregame: Option<Instant>,
    roster_retry: Option<(Instant, u32)>,
    post_match: Option<PostMatch>,
    worker: Option<JoinHandle<()>>,
    last_connect_error: Option<String>,
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
            connected_without_game: None,
            absent_ticks: 0,
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
        if !lockfile_path(&lad).exists() {
            self.last_connect_error = None;
            sleep(CLOSED_CHECK).await;
            return;
        }
        match RiotSession::connect(&lad).await {
            Ok(session) => self.on_connected(Arc::new(session)).await,
            Err(e) => {
                let msg = e.to_string();
                if self.last_connect_error.as_deref() != Some(msg.as_str()) {
                    log::info!("Riot client not ready: {msg}");
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
                live.names_attempted.clear();
                live.self_roster = None;
            }
            live.self_puuid = Some(me.clone());
        }
        let key_value = me.clone();
        self.st
            .db(move |repo| repo.set_state(SELF_PUUID_KEY, &key_value))
            .await;
        self.st.set_session(Some(session.clone()));
        self.session = Some(session);
        self.connected_without_game = None;
        self.absent_ticks = 0;
        self.presence_errors = 0;
        self.next_presence = Instant::now();

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

        let Some(presence) = decoded else {
            if self.connected_without_game.is_none() {
                self.connected_without_game = Some(true);
            }
            self.absent_ticks += 1;
            if self.absent_ticks >= ABSENT_TICKS_TO_CLOSE && self.stage != Stage::Closed {
                log::info!("VALORANT is not running");
                self.change_stage(session, Stage::Closed, None).await?;
            }
            return Ok(());
        };

        self.absent_ticks = 0;
        if self.connected_without_game == Some(true) {
            // The session was built from a stale ShooterGame.log / fallback
            // client version; rebuild it now that VALORANT is up.
            return Err(Stop::Reconnect("VALORANT started"));
        }
        self.connected_without_game = Some(false);

        let key = private.as_deref().and_then(party_key);
        if presence.stage != self.stage {
            self.change_stage(session, presence.stage, key).await?;
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
        if let Some(handle) = self.worker.take() {
            handle.abort();
        }
    }

    /// Starts the sync worker unless one is already running (it picks up
    /// roster changes on its own).
    fn ensure_worker(&mut self, session: &Arc<RiotSession>) {
        if self
            .worker
            .as_ref()
            .is_some_and(|h| !h.inner().is_finished())
        {
            return;
        }
        if !lock(&self.st.live).has_pending_work() {
            return;
        }
        let (st, s) = (self.st.clone(), session.clone());
        self.worker = Some(tauri::async_runtime::spawn(crate::sync::run_worker(st, s)));
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

        let ended_match = {
            let mut live = lock(&self.st.live);
            let ended = live.coregame_match_id.take();
            live.loadouts.clear();
            if t.reset_synced {
                live.synced.clear();
                live.names_attempted.clear();
            }
            ended
        };
        self.post_match = match (t.match_ended, ended_match) {
            (true, Some(match_id)) => Some(PostMatch {
                match_id,
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
    /// read (and get a `players` row upsert).
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
                .filter(|p| !live.slots.iter().any(|s| s.roster.puuid == p.puuid))
                .cloned()
                .collect()
        };
        let mut loaded: HashMap<String, PlayerDb> = HashMap::new();
        if !new_players.is_empty() {
            let now = now_ms();
            loaded = self
                .st
                .db(move |repo| {
                    let mut out = HashMap::new();
                    for p in &new_players {
                        save_player(
                            repo.conn(),
                            &PlayerUpdate {
                                puuid: p.puuid.clone(),
                                name: None,
                                account_level: if p.hide_account_level {
                                    0
                                } else {
                                    p.account_level
                                },
                                card_id: p.card_id.clone(),
                            },
                            now,
                        )?;
                        out.insert(p.puuid.clone(), load_player_db(repo, &p.puuid)?);
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
                    let db = live
                        .slots
                        .iter()
                        .find(|s| s.roster.puuid == r.puuid)
                        .map(|s| s.db.clone())
                        .or_else(|| loaded.remove(&r.puuid))
                        .unwrap_or_default();
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
            .db(move |repo| load_player_db(repo, &p))
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
                    .db(move |repo| load_player_db(repo, &p))
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
        let id = pm.match_id.clone();
        let missing = self
            .st
            .db(move |repo| repo.new_match_ids(&[id]))
            .await
            .map(|m| !m.is_empty())
            .unwrap_or(false);
        if !missing {
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
            }
        }
        self.ensure_worker(session);
        let next = pm.attempt + 1;
        if let Some(delay) = POST_MATCH_DELAYS.get(next) {
            self.post_match = Some(PostMatch {
                match_id: pm.match_id,
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
