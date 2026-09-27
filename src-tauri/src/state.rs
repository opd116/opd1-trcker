//! Shared app state (managed by Tauri, shared with the poll loop) and the
//! live model the `"live"` event is built from.
//!
//! Locking rules: every `std::sync` lock here is held only for short,
//! synchronous work (never across an `.await`); SQLite work always goes
//! through [`AppState::db`] (`spawn_blocking`).

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Emitter};

use opd1_core::db::repo::Repo;
use opd1_core::model::{
    build_card, BuildCardInput, Field, Identity, LiveState, Loadout, MmrInput, PlayerCard,
    PlayerDetails, Team,
};
use opd1_core::presence::Stage;
use opd1_core::stats::Act;
use opd1_riot::assets::AssetCache;
use opd1_riot::to_core::{current_rank_from_mmr, seasonal_map_from_mmr};
use opd1_riot::RiotSession;

use crate::logbuf::short;
use crate::roster::{current_act, RosterPlayer};
use crate::store::{PlayerDb, StoredPlayer};

/// A failed asset-table load isn't retried more often than this (offline
/// first run: `meta()` and every `img` request would otherwise each hit
/// valorant-api again).
const TABLES_RETRY_AFTER: Duration = Duration::from_secs(30);

/// Player Details re-syncs the same player at most this often.
const DETAILS_RESYNC_AFTER: Duration = Duration::from_secs(120);

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// The clock cards are built with, floored to the hour. `peak.sinceActEndMs`
/// is `now - actEnd`: with the raw clock every rebuild would serialize
/// differently and the "emit only when the bytes change" rule would never
/// dedupe anything. The UI shows it as "1y"/"3mo", so an hour is plenty.
pub fn card_clock(now_ms: i64) -> i64 {
    now_ms - now_ms.rem_euclid(3_600_000)
}

pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

// ---------------------------------------------------------------------
// live model
// ---------------------------------------------------------------------

/// One card's inputs: the roster entry plus what the DB knows.
#[derive(Debug, Clone)]
pub struct Slot {
    pub roster: RosterPlayer,
    pub db: PlayerDb,
}

/// One unit of work for the roster worker (see [`LiveModel::next_job`]).
#[derive(Debug, Clone, PartialEq)]
pub enum Job {
    /// One `names()` call: `(puuid, incognito)` pairs — `incognito` is
    /// always `false` (filtered), carried so the worker builds each
    /// `VisiblePuuid` from the real flag.
    Names(Vec<(String, bool)>),
    /// The per-player sync of one puuid.
    Sync(String),
}

/// The live stage/roster plus the session-scoped bookkeeping the sync
/// worker and the card builder share.
pub struct LiveModel {
    pub stage: Stage,
    /// Riot's `MapID` path for the current match (Agent Select / In Game).
    pub map_path: Option<String>,
    /// `map_path` as a valorant-api uuid — resolved at publish time (the
    /// asset tables may load after the roster arrived); never the path.
    pub map_id: Option<String>,
    /// Display/sync order (`roster::order_players`).
    pub slots: Vec<Slot>,
    /// Equipped chromas from the current core-game match (Player Details).
    pub loadouts: HashMap<String, Loadout>,
    pub coregame_match_id: Option<String>,
    pub self_puuid: Option<String>,
    /// The user's own Riot ID from the local chat session
    /// (`/chat/v1/session`, R43) — the only source of the user's name; the
    /// user is never sent to name-service.
    pub self_name: Option<(String, String)>,
    /// The chat-session lookup ran for this Riot Client session (so an
    /// unknown own name is Missing, not Loading). Not cleared by stage
    /// resets — only a new connection re-asks.
    pub self_name_attempted: bool,
    /// The user's own last-seen roster entry (placeholder while a roster
    /// fetch is retried).
    pub self_roster: Option<RosterPlayer>,
    /// Names from `names()` this run (visible, non-self players only).
    pub known_names: HashMap<String, (String, String)>,
    /// Players a `names()` call was attempted for (since the last reset):
    /// a visible name still unknown after that is Missing, not Loading.
    pub names_attempted: HashSet<String>,
    /// Players whose per-player sync ran (since the last reset): a field
    /// still unknown after that is Missing, not Loading.
    pub synced: HashSet<String>,
    /// The worker currently entitled to run (R45). Set and cleared only
    /// under this model's lock, together with the "is there work?" check,
    /// so no roster change can fall between a worker deciding to exit and
    /// a new one being refused.
    worker_id: Option<u64>,
    next_worker_id: u64,
    /// The last built/emitted state (`live_state()` returns it).
    pub current: LiveState,
    last_bytes: Vec<u8>,
}

impl LiveModel {
    pub fn new(self_puuid: Option<String>) -> Self {
        Self {
            stage: Stage::Closed,
            map_path: None,
            map_id: None,
            slots: Vec::new(),
            loadouts: HashMap::new(),
            coregame_match_id: None,
            self_puuid,
            self_name: None,
            self_name_attempted: false,
            self_roster: None,
            known_names: HashMap::new(),
            names_attempted: HashSet::new(),
            synced: HashSet::new(),
            worker_id: None,
            next_worker_id: 0,
            current: LiveState {
                stage: Stage::Closed,
                map_id: None,
                players: Vec::new(),
            },
            last_bytes: Vec::new(),
        }
    }

    fn self_id(&self) -> &str {
        self.self_puuid.as_deref().unwrap_or("")
    }

    /// Builds the full `LiveState` (pure; `now_ms` should be [`card_clock`]).
    pub fn build(&self, acts: &[Act], now_ms: i64) -> LiveState {
        let current_act_id = current_act(acts, now_ms)
            .map(|a| a.id.as_str())
            .unwrap_or("");
        LiveState {
            stage: self.stage,
            map_id: self.map_id.clone(),
            players: self
                .slots
                .iter()
                .map(|s| self.card_for(s, acts, current_act_id, now_ms))
                .collect(),
        }
    }

    pub fn card_for(
        &self,
        slot: &Slot,
        acts: &[Act],
        current_act_id: &str,
        now_ms: i64,
    ) -> PlayerCard {
        let r = &slot.roster;
        let is_self = r.puuid == self.self_id();
        // While VALORANT is closed nothing can be fetched: whatever the DB
        // doesn't have is Missing, not a skeleton that never resolves.
        let closed = self.stage == Stage::Closed;
        let data_done = closed || self.synced.contains(&r.puuid);

        // Names (CLAUDE.md "Hidden names", R43):
        // - the user: their own Riot ID from the local chat session (or its
        //   stored copy) — shown to the user, never looked up remotely;
        // - an incognito player: no name is fed into the card from any
        //   source (build_card also ignores it, belt and braces);
        // - a visible player: this run's name-service answer first, then
        //   the name stored the last time they were visible (cached-first).
        let stored = || slot.db.row.as_ref().and_then(StoredPlayer::name);
        let (incognito, (game_name, tag_line), names_done) = if is_self {
            let name = self.self_name.clone().or_else(stored).unwrap_or_default();
            (false, name, closed || data_done || self.self_name_attempted)
        } else if r.incognito {
            (true, Default::default(), true)
        } else {
            let name = self
                .known_names
                .get(&r.puuid)
                .cloned()
                .or_else(stored)
                .unwrap_or_default();
            let done = closed || data_done || self.names_attempted.contains(&r.puuid);
            (false, name, done)
        };

        let mmr = match &slot.db.mmr {
            Some(resp) => Field::Value(MmrInput {
                seasons: seasonal_map_from_mmr(resp),
                current_rank: current_rank_from_mmr(resp, current_act_id),
            }),
            None if data_done => Field::Missing,
            None => Field::Loading,
        };
        let last5 = if !slot.db.last5.is_empty() {
            Field::Value(slot.db.last5.clone())
        } else if data_done {
            Field::Missing
        } else {
            Field::Loading
        };

        let mut card = build_card(BuildCardInput {
            identity: Identity {
                puuid: r.puuid.clone(),
                incognito,
                hide_account_level: r.hide_account_level || r.account_level == 0,
                account_level: r.account_level,
                card_id: r.card_id.clone(),
                character_id: r.character_id.clone(),
                character_selection_state: r.selection_state.clone(),
                game_name,
                tag_line,
            },
            mmr,
            comp_updates: Field::Value(slot.db.comp.clone()),
            last5,
            ordered_acts: acts,
            current_act_id,
            now_ms,
            team: r.team,
            is_self_party: r.is_self_party,
        });
        settle_card(&mut card, names_done, data_done);
        card
    }

    /// Players shown hidden right now (R18: their names must not leak
    /// through Player Details' Top Teammates either). The user's own name
    /// is known locally (chat session) and is not hidden from the user.
    pub fn hidden_puuids(&self) -> HashSet<String> {
        let self_id = self.self_id();
        self.slots
            .iter()
            .filter(|s| s.roster.incognito && s.roster.puuid != self_id)
            .map(|s| s.roster.puuid.clone())
            .collect()
    }

    /// Players to send to name-service: never the user (R43 — their name
    /// comes from the chat session), never anyone whose `Incognito` is
    /// true, only names not known and not yet asked for.
    pub fn names_needed(&self) -> Vec<(String, bool)> {
        let self_id = self.self_id();
        self.slots
            .iter()
            .filter(|s| s.roster.puuid != self_id)
            .filter(|s| !s.roster.incognito)
            .filter(|s| !self.known_names.contains_key(&s.roster.puuid))
            .filter(|s| !self.names_attempted.contains(&s.roster.puuid))
            .map(|s| (s.roster.puuid.clone(), s.roster.incognito))
            .collect()
    }

    /// The next player to sync, in display order (self -> party ->
    /// allies -> enemies).
    pub fn next_unsynced(&self) -> Option<String> {
        self.slots
            .iter()
            .map(|s| &s.roster.puuid)
            .find(|p| !self.synced.contains(*p))
            .cloned()
    }

    pub fn has_pending_work(&self) -> bool {
        self.next_unsynced().is_some() || !self.names_needed().is_empty()
    }

    /// R45: claims the worker slot if there is work and no worker owns it
    /// — returns the new worker's id (the caller spawns it), else `None`.
    pub fn try_start_worker(&mut self) -> Option<u64> {
        if self.worker_id.is_some() || !self.has_pending_work() {
            return None;
        }
        self.next_worker_id += 1;
        self.worker_id = Some(self.next_worker_id);
        self.worker_id
    }

    /// R45: the worker's next job, decided under the lock. When there is
    /// nothing left, the worker's claim is released *in the same critical
    /// section*, so a player added a moment later finds no worker and
    /// `try_start_worker` starts a fresh one. A superseded worker (`id`
    /// no longer the owner) just gets `None`.
    pub fn next_job(&mut self, id: u64) -> Option<Job> {
        if self.worker_id != Some(id) {
            return None;
        }
        let names = self.names_needed();
        if !names.is_empty() {
            return Some(Job::Names(names));
        }
        if let Some(puuid) = self.next_unsynced() {
            return Some(Job::Sync(puuid));
        }
        self.worker_id = None;
        None
    }

    /// Releases worker `id`'s claim (early exit, e.g. the client went away).
    pub fn release_worker(&mut self, id: u64) {
        if self.worker_id == Some(id) {
            self.worker_id = None;
        }
    }

    /// Revokes any worker's claim (the poll loop aborted it).
    pub fn revoke_worker(&mut self) {
        self.worker_id = None;
    }

    #[cfg(test)]
    pub fn worker_running(&self) -> bool {
        self.worker_id.is_some()
    }
}

/// R23 / Review Focus 2: once a source has been *attempted* (successfully
/// or not), none of its fields may stay `Loading` — the UI would show a
/// skeleton forever. `names_done` covers `name`; `data_done` covers every
/// MMR/last-5 field. Values and Missing are left alone.
pub fn settle_card(card: &mut PlayerCard, names_done: bool, data_done: bool) {
    fn settle<T>(f: &mut Field<T>) {
        if f.is_loading() {
            *f = Field::Missing;
        }
    }
    if names_done {
        settle(&mut card.name);
    }
    if data_done {
        settle(&mut card.level);
        settle(&mut card.rank);
        settle(&mut card.prev_act_tier);
        settle(&mut card.peak);
        settle(&mut card.kd);
        settle(&mut card.hs_pct);
        settle(&mut card.act_winrate);
        settle(&mut card.last5);
    }
}

/// The user's own card inputs while VALORANT is closed: identity from the
/// `players` row (nothing if the DB has never seen the user).
pub fn closed_self_slot(self_puuid: &str, db: PlayerDb) -> Option<Slot> {
    let row = db.row.clone()?;
    let level = u32::try_from(row.account_level).unwrap_or(0);
    Some(Slot {
        roster: RosterPlayer {
            puuid: self_puuid.to_string(),
            team_id: String::new(),
            team: Team::Ally,
            is_self_party: true,
            incognito: false,
            hide_account_level: level == 0,
            account_level: level,
            card_id: row.card_id,
            character_id: String::new(),
            selection_state: String::new(),
        },
        db,
    })
}

// ---------------------------------------------------------------------
// AppState
// ---------------------------------------------------------------------

/// Where the `"live"` / `"details"` events go: the Tauri app in
/// production, a recorder in tests.
pub trait EventSink: Send + Sync {
    fn live(&self, state: &LiveState);
    fn details(&self, details: &PlayerDetails);
}

impl EventSink for AppHandle {
    fn live(&self, state: &LiveState) {
        if let Err(e) = self.emit("live", state) {
            log::warn!("emit live failed: {e}");
        }
    }

    fn details(&self, details: &PlayerDetails) {
        if let Err(e) = self.emit("details", details) {
            log::warn!("emit details failed: {e}");
        }
    }
}

#[derive(Default)]
struct TablesStatus {
    loaded: bool,
    last_failure: Option<Instant>,
}

pub struct AppState {
    sink: Box<dyn EventSink>,
    repo: Arc<Mutex<Repo>>,
    pub assets: Arc<AssetCache>,
    session: RwLock<Option<Arc<RiotSession>>>,
    pub live: Mutex<LiveModel>,
    acts: RwLock<Arc<Vec<Act>>>,
    tables: tokio::sync::Mutex<TablesStatus>,
    details_synced: Mutex<HashMap<String, Instant>>,
    /// `app_data_dir()/debug` — where `dump_raw` writes.
    pub debug_dir: PathBuf,
}

impl AppState {
    pub fn new(
        sink: Box<dyn EventSink>,
        repo: Repo,
        assets: AssetCache,
        debug_dir: PathBuf,
        self_puuid: Option<String>,
    ) -> Self {
        Self {
            sink,
            repo: Arc::new(Mutex::new(repo)),
            assets: Arc::new(assets),
            session: RwLock::new(None),
            live: Mutex::new(LiveModel::new(self_puuid)),
            acts: RwLock::new(Arc::new(Vec::new())),
            tables: tokio::sync::Mutex::new(TablesStatus::default()),
            details_synced: Mutex::new(HashMap::new()),
            debug_dir,
        }
    }

    pub fn session(&self) -> Option<Arc<RiotSession>> {
        self.session
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub fn set_session(&self, session: Option<Arc<RiotSession>>) {
        *self.session.write().unwrap_or_else(PoisonError::into_inner) = session;
    }

    pub fn acts(&self) -> Arc<Vec<Act>> {
        self.acts
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    #[cfg(test)]
    pub fn set_acts_for_test(&self, acts: Vec<Act>) {
        *self.acts.write().unwrap_or_else(PoisonError::into_inner) = Arc::new(acts);
    }

    /// The current act (by wall clock), if the tables are loaded.
    pub fn current_act(&self) -> Option<Act> {
        let acts = self.acts();
        current_act(&acts, now_ms()).cloned()
    }

    /// Riot map path -> valorant-api uuid (`None` if unknown or the tables
    /// aren't loaded).
    pub fn map_uuid(&self, path: &str) -> Option<String> {
        crate::roster::map_uuid(path, |p| self.assets.map_uuid_for_path(p).ok().flatten())
    }

    /// Runs `f` against the repo on the blocking pool. `None` (logged) on
    /// a SQLite error — callers treat that as "the DB doesn't know".
    pub async fn db<T, F>(&self, f: F) -> Option<T>
    where
        F: FnOnce(&Repo) -> rusqlite::Result<T> + Send + 'static,
        T: Send + 'static,
    {
        let repo = self.repo.clone();
        match tauri::async_runtime::spawn_blocking(move || f(&lock(&repo))).await {
            Ok(Ok(v)) => Some(v),
            Ok(Err(e)) => {
                log::warn!("db: {e}");
                None
            }
            Err(e) => {
                log::warn!("db task failed: {e}");
                None
            }
        }
    }

    /// Synchronous DB access for setup (before the async runtime is doing
    /// anything else) — never call from an async task.
    pub fn db_blocking<T>(&self, f: impl FnOnce(&Repo) -> rusqlite::Result<T>) -> Option<T> {
        f(&lock(&self.repo)).map_err(|e| log::warn!("db: {e}")).ok()
    }

    /// Loads the valorant-api tables (agents/maps/tiers/seasons) if they
    /// aren't yet — or, with `recheck`, checks `/v1/version` again (one
    /// small request; the tables are only re-downloaded when it changed).
    /// Offline, `AssetCache` falls back to the disk copy.
    pub async fn ensure_tables(&self, recheck: bool) -> Result<(), String> {
        let mut status = self.tables.lock().await;
        if status.loaded && !recheck {
            return Ok(());
        }
        if !status.loaded {
            if let Some(t) = status.last_failure {
                if t.elapsed() < TABLES_RETRY_AFTER {
                    return Err("asset tables unavailable".into());
                }
            }
        }
        match self.assets.refresh_tables().await {
            Ok(refetched) => {
                status.loaded = true;
                status.last_failure = None;
                let acts = self.assets.acts().unwrap_or_default();
                log::info!(
                    "asset tables {} ({} acts)",
                    if refetched {
                        "downloaded"
                    } else {
                        "loaded from cache"
                    },
                    acts.len()
                );
                *self.acts.write().unwrap_or_else(PoisonError::into_inner) = Arc::new(acts);
                Ok(())
            }
            Err(e) => {
                status.last_failure = Some(Instant::now());
                log::warn!("asset tables: {e}");
                if status.loaded {
                    Ok(())
                } else {
                    Err(e.to_string())
                }
            }
        }
    }

    /// Rebuilds `LiveState` and emits `"live"` — only when its serialized
    /// bytes differ from the last emission.
    ///
    /// Build, compare, record and emit all happen under the `live` lock, so
    /// concurrent publishers (poll loop, worker, details sync, tables task)
    /// are serialized: events leave in the order the states were built and
    /// the last one emitted is always `current` — an older snapshot can
    /// never land after a newer one and then be "deduped" into sticking.
    /// Holding the lock across the emit is fine: Tauri's `emit` only
    /// serializes and queues the event for the webview.
    pub fn publish(&self) {
        let acts = self.acts();
        let mut live = lock(&self.live);
        if live.map_id.is_none() {
            if let Some(path) = live.map_path.clone() {
                live.map_id = self.map_uuid(&path);
            }
        }
        let state = live.build(&acts, card_clock(now_ms()));
        let bytes = serde_json::to_vec(&state).unwrap_or_default();
        if bytes == live.last_bytes {
            return;
        }
        live.last_bytes = bytes;
        log::debug!(
            "emit live: {:?}, {} players",
            state.stage,
            state.players.len()
        );
        self.sink.live(&state);
        live.current = state;
    }

    pub fn emit_details(&self, details: &PlayerDetails) {
        self.sink.details(details);
    }

    pub fn live_state(&self) -> LiveState {
        lock(&self.live).current.clone()
    }

    /// Replaces the DB bundle of `puuid`'s live slot (if it's on screen)
    /// with a fresh read, then republishes.
    pub async fn reload_slot(&self, puuid: &str) {
        let p = puuid.to_string();
        let Some(db) = self
            .db(move |repo| crate::store::load_player_db(repo, &p))
            .await
        else {
            return;
        };
        {
            let mut live = lock(&self.live);
            if let Some(slot) = live.slots.iter_mut().find(|s| s.roster.puuid == puuid) {
                slot.db = db;
            }
        }
        self.publish();
    }

    /// `true` (and records now) when `puuid`'s Player Details data hasn't
    /// been synced in the last [`DETAILS_RESYNC_AFTER`].
    pub fn claim_details_sync(&self, puuid: &str) -> bool {
        let mut map = lock(&self.details_synced);
        let now = Instant::now();
        map.retain(|_, t| now.duration_since(*t) < DETAILS_RESYNC_AFTER);
        if map.contains_key(puuid) {
            return false;
        }
        map.insert(puuid.to_string(), now);
        log::debug!("details sync claimed for {}", short(puuid));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opd1_core::db::repo::MatchPlayerRow;
    use opd1_core::model::DisplayName;

    fn roster(puuid: &str, incognito: bool) -> RosterPlayer {
        RosterPlayer {
            puuid: puuid.into(),
            team_id: "Blue".into(),
            team: Team::Ally,
            is_self_party: false,
            incognito,
            hide_account_level: false,
            account_level: 50,
            card_id: "card".into(),
            character_id: "agent".into(),
            selection_state: "locked".into(),
        }
    }

    fn slot(puuid: &str, incognito: bool) -> Slot {
        Slot {
            roster: roster(puuid, incognito),
            db: PlayerDb::default(),
        }
    }

    fn model(slots: Vec<Slot>) -> LiveModel {
        let mut m = LiveModel::new(Some("me".into()));
        m.stage = Stage::Ingame;
        m.slots = slots;
        m
    }

    fn row(match_id: &str, start_ms: i64) -> MatchPlayerRow {
        MatchPlayerRow {
            match_id: match_id.into(),
            map_id: "/Game/Maps/Ascent/Ascent".into(),
            start_ms,
            winning_team: Some("Blue".into()),
            team_id: "Blue".into(),
            party_id: None,
            agent_id: None,
            kills: 10,
            deaths: 5,
            assists: 1,
            score: 1,
            rounds: None,
            hs: 1,
            bs: 1,
            ls: 0,
            game_name: None,
            tag_line: None,
        }
    }

    #[test]
    fn unsynced_player_is_loading_then_missing_once_attempted() {
        let mut m = model(vec![slot("p1", false)]);
        let card = &m.build(&[], 0).players[0];
        assert!(card.rank.is_loading());
        assert!(card.last5.is_loading());
        assert!(card.name.is_loading());

        // The sync ran but every call failed / returned nothing.
        m.synced.insert("p1".into());
        m.names_attempted.insert("p1".into());
        let card = &m.build(&[], 0).players[0];
        assert_eq!(card.rank, Field::Missing);
        assert_eq!(card.last5, Field::Missing);
        assert_eq!(card.kd, Field::Missing);
        assert_eq!(card.name, Field::Missing);
    }

    #[test]
    fn cached_db_data_shows_before_the_sync() {
        let mut s = slot("p1", false);
        s.db.last5 = vec![row("m1", 10)];
        let m = model(vec![s]);
        let card = &m.build(&[], 0).players[0];
        assert!(matches!(card.last5, Field::Value(ref v) if v.len() == 1));
        assert!(matches!(card.kd, Field::Value(_)));
        assert!(card.rank.is_loading(), "no MMR snapshot yet and not synced");
    }

    #[test]
    fn closed_stage_never_shows_skeletons() {
        let mut m = model(vec![slot("me", false)]);
        m.stage = Stage::Closed;
        let json = serde_json::to_value(&m.build(&[], 0).players[0]).unwrap();
        for key in [
            "name",
            "rank",
            "prevActTier",
            "peak",
            "kd",
            "hsPct",
            "actWinrate",
            "last5",
        ] {
            assert!(
                json.get(key).is_some(),
                "{key} must be present (null), not absent"
            );
        }
    }

    #[test]
    fn hidden_players_never_get_a_name_even_if_known() {
        let mut s = slot("h1", true);
        s.db.row = Some(StoredPlayer {
            game_name: "Real".into(),
            tag_line: "Name".into(),
            account_level: 1,
            card_id: "c".into(),
        });
        let mut m = model(vec![s]);
        m.known_names
            .insert("h1".into(), ("Real".into(), "Name".into()));
        let card = &m.build(&[], 0).players[0];
        assert_eq!(
            card.name,
            Field::Value(DisplayName::HiddenAgent {
                agent_id: "agent".into()
            })
        );
        assert!(!serde_json::to_string(card).unwrap().contains("Real"));
        assert!(
            m.names_needed().is_empty(),
            "never ask name-service for a hidden player"
        );
        assert_eq!(m.hidden_puuids(), HashSet::from(["h1".to_string()]));
    }

    #[test]
    fn the_user_is_never_sent_to_name_service() {
        // Visible or incognito, the user's own puuid never needs names()
        // (R43: their Riot ID comes from the local chat session).
        for incognito in [false, true] {
            let m = model(vec![slot("me", incognito), slot("p1", false)]);
            assert_eq!(m.names_needed(), vec![("p1".to_string(), false)]);
        }
    }

    #[test]
    fn the_users_own_name_comes_from_the_chat_session() {
        let mut m = model(vec![slot("me", true)]);
        // Unknown until the chat session was asked…
        assert!(m.build(&[], 0).players[0].name.is_loading());
        m.self_name_attempted = true;
        assert_eq!(m.build(&[], 0).players[0].name, Field::Missing);
        // …then the local chat session's Riot ID, shown to the user even
        // with their own streamer-mode flag on.
        m.self_name = Some(("Shanky".into(), "OPD1".into()));
        assert_eq!(
            m.build(&[], 0).players[0].name,
            Field::Value(DisplayName::Visible {
                name: "Shanky".into(),
                tag: "OPD1".into()
            })
        );
        assert!(m.hidden_puuids().is_empty());
        // A stage reset (which clears names_attempted) keeps it settled.
        m.names_attempted.clear();
        m.self_name = None;
        assert_eq!(m.build(&[], 0).players[0].name, Field::Missing);
    }

    #[test]
    fn cached_name_is_used_until_name_service_answers() {
        let mut s = slot("p1", false);
        s.db.row = Some(StoredPlayer {
            game_name: "Old".into(),
            tag_line: "TAG".into(),
            account_level: 1,
            card_id: "c".into(),
        });
        let mut m = model(vec![s]);
        let name = |m: &LiveModel| m.build(&[], 0).players[0].name.clone();
        assert_eq!(
            name(&m),
            Field::Value(DisplayName::Visible {
                name: "Old".into(),
                tag: "TAG".into()
            })
        );
        m.known_names
            .insert("p1".into(), ("New".into(), "TAG".into()));
        assert_eq!(
            name(&m),
            Field::Value(DisplayName::Visible {
                name: "New".into(),
                tag: "TAG".into()
            })
        );
    }

    #[test]
    fn next_unsynced_walks_display_order() {
        let mut m = model(vec![slot("me", false), slot("a", false), slot("e", false)]);
        assert_eq!(m.next_unsynced().as_deref(), Some("me"));
        m.synced.insert("me".into());
        assert_eq!(m.next_unsynced().as_deref(), Some("a"));
        m.synced.extend(["a".to_string(), "e".to_string()]);
        m.names_attempted.extend(["a".to_string(), "e".to_string()]);
        assert!(!m.has_pending_work());
    }

    #[test]
    fn worker_exit_and_a_new_player_cannot_race() {
        let mut m = model(vec![slot("me", false)]);
        m.names_attempted.insert("me".into());
        let id = m
            .try_start_worker()
            .expect("work pending -> worker claimed");
        assert!(m.try_start_worker().is_none(), "one worker at a time");
        assert_eq!(m.next_job(id), Some(Job::Sync("me".into())));
        m.synced.insert("me".into());

        // Nothing left: the claim is released in the same step…
        assert_eq!(m.next_job(id), None);
        assert!(!m.worker_running());
        // …so a player added right after the worker decided to exit gets a
        // fresh worker instead of staying Loading forever (R45).
        m.slots.push(slot("late", false));
        let id2 = m
            .try_start_worker()
            .expect("new work after exit -> new worker");
        assert_eq!(
            m.next_job(id2),
            Some(Job::Names(vec![("late".into(), false)]))
        );

        // A player added while a worker still runs is picked up by it.
        m.names_attempted.insert("late".into());
        m.slots.push(slot("later", false));
        m.names_attempted.insert("later".into());
        assert_eq!(m.next_job(id2), Some(Job::Sync("late".into())));
        m.synced.insert("late".into());
        assert_eq!(m.next_job(id2), Some(Job::Sync("later".into())));
    }

    #[test]
    fn a_revoked_worker_gets_no_more_jobs_and_cannot_release_its_successor() {
        let mut m = model(vec![slot("me", false), slot("p", false)]);
        let old = m.try_start_worker().unwrap();
        m.revoke_worker(); // the poll loop aborted it (stage change)
        let new = m.try_start_worker().unwrap();
        assert_eq!(m.next_job(old), None, "superseded");
        m.release_worker(old);
        assert!(
            m.worker_running(),
            "the old worker can't free the new one's claim"
        );
        assert!(m.next_job(new).is_some());
    }

    #[test]
    fn settle_only_touches_attempted_sources() {
        let mut m = model(vec![slot("p1", false)]);
        m.names_attempted.insert("p1".into());
        let card = &m.build(&[], 0).players[0];
        assert_eq!(card.name, Field::Missing);
        assert!(card.rank.is_loading(), "data not attempted yet");
    }

    #[test]
    fn hidden_level_is_missing() {
        let mut s = slot("p1", false);
        s.roster.hide_account_level = true;
        let m = model(vec![s]);
        assert_eq!(m.build(&[], 0).players[0].level, Field::Missing);
    }

    #[test]
    fn card_clock_floors_to_the_hour() {
        assert_eq!(card_clock(3_600_000 * 5 + 1234), 3_600_000 * 5);
        assert_eq!(card_clock(3_600_000 * 5), 3_600_000 * 5);
    }

    #[test]
    fn closed_self_slot_needs_a_players_row() {
        assert!(closed_self_slot("me", PlayerDb::default()).is_none());
        let db = PlayerDb {
            row: Some(StoredPlayer {
                game_name: "Me".into(),
                tag_line: "1".into(),
                account_level: 0,
                card_id: "card".into(),
            }),
            ..PlayerDb::default()
        };
        let s = closed_self_slot("me", db).unwrap();
        assert!(s.roster.is_self_party);
        assert!(s.roster.hide_account_level, "unknown level shows as —");
    }

    // -- publish() ------------------------------------------------------

    #[derive(Default)]
    struct Recorder(Mutex<Vec<LiveState>>);

    struct Sink(Arc<Recorder>);

    impl EventSink for Sink {
        fn live(&self, state: &LiveState) {
            lock(&self.0 .0).push(state.clone());
        }
        fn details(&self, _details: &PlayerDetails) {}
    }

    fn test_state() -> (Arc<AppState>, Arc<Recorder>, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let rec = Arc::new(Recorder::default());
        let repo = Repo::new(opd1_core::db::open_db(std::path::Path::new(":memory:")).unwrap());
        let assets = AssetCache::with_base(dir.path().join("a"), "http://127.0.0.1:1".into());
        let st = AppState::new(
            Box::new(Sink(rec.clone())),
            repo,
            assets,
            dir.path().join("debug"),
            Some("me".into()),
        );
        (Arc::new(st), rec, dir)
    }

    #[test]
    fn publish_emits_only_when_the_state_changes() {
        let (st, rec, _dir) = test_state();
        st.publish();
        st.publish();
        assert_eq!(lock(&rec.0).len(), 1, "same bytes -> no second emit");
        lock(&st.live).slots.push(slot("p1", false));
        st.publish();
        st.publish();
        let emitted = lock(&rec.0).clone();
        assert_eq!(emitted.len(), 2);
        assert_eq!(emitted.last(), Some(&st.live_state()));
    }

    #[test]
    fn concurrent_publishers_always_end_on_the_current_state() {
        for _ in 0..10 {
            let (st, rec, _dir) = test_state();
            let threads: Vec<_> = (0..6)
                .map(|t| {
                    let st = st.clone();
                    std::thread::spawn(move || {
                        for i in 0..20 {
                            {
                                let mut live = lock(&st.live);
                                let p = format!("p{t}-{i}");
                                live.slots.push(slot(&p, false));
                                if i % 3 == 0 {
                                    live.synced.insert(p);
                                }
                            }
                            st.publish();
                        }
                    })
                })
                .collect();
            for t in threads {
                t.join().unwrap();
            }
            let emitted = lock(&rec.0).clone();
            let current = st.live_state();
            // The last event the UI received is the model's current state…
            assert_eq!(emitted.last(), Some(&current));
            // …and it matches a fresh build (nothing newer was suppressed).
            let fresh = lock(&st.live).build(&[], card_clock(now_ms()));
            assert_eq!(current, fresh);
            // Events never go backwards: each has at least as many players
            // as the one before (slots only grow in this test).
            for w in emitted.windows(2) {
                assert!(
                    w[1].players.len() >= w[0].players.len(),
                    "out-of-order emit"
                );
            }
        }
    }
}
