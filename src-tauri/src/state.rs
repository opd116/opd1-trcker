//! Shared app state (managed by Tauri, shared with the poll loop) and the
//! live model the `"live"` event is built from.
//!
//! Locking rules: every `std::sync` lock here is held only for short,
//! synchronous work (never across an `.await`); SQLite work always goes
//! through [`AppState::db`] (`spawn_blocking`). Order: a DB closure may
//! take the `live` lock (`sync::fetch_self_loadout`'s account check), so
//! nothing takes the repo while holding `live`.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Emitter};

use opd1_core::db::repo::{Match, MatchPlayer, NameAsk, Repo};
use opd1_core::model::{
    build_card, shown_agent, BuildCardInput, Field, Identity, LiveState, MatchDetailsView,
    MmrInput, MyStats, PartyMark, PlayerCard, PlayerDetails, Team,
};
use opd1_core::party::detect_parties;
use opd1_core::presence::Stage;
use opd1_core::stats::Act;
use opd1_riot::assets::{AssetCache, Meta};
use opd1_riot::endpoints::ContentActs;
use opd1_riot::to_core::{current_rank_from_mmr, seasonal_map_from_mmr};
use opd1_riot::RiotSession;
use opd1_riot::VisiblePuuid;

use crate::logbuf::short;
use crate::my_stats::MyStatsPush;
use crate::rank_retry::{RankRetries, RANK_RETRY_DELAYS};
use crate::roster::{current_act, resolve_acts, RawLoadout, RosterPlayer};
use crate::store::{PlayerDb, StoredPlayer, SELF_LOADOUT_KEY};

/// A failed asset-table load isn't retried more often than this (offline
/// first run: `meta()` and every `img` request would otherwise each hit
/// valorant-api again).
const TABLES_RETRY_AFTER: Duration = Duration::from_secs(30);

/// Riot's content-service is asked again at most this often: after a failure
/// (every connect also asks, regardless).
const CONTENT_RETRY_AFTER: Duration = Duration::from_secs(5 * 60);
/// … and after the act it named has ended (Riot late at a rollover).
const CONTENT_RECHECK_AFTER: Duration = Duration::from_secs(60);
/// An act Riot still calls active past its end is believed for this long.
const CONTENT_LATE_TRUST_MS: i64 = 10 * 60_000;

/// Player Details re-syncs the same player at most this often.
const DETAILS_RESYNC_AFTER: Duration = Duration::from_secs(120);

/// How long a task waits for a match-details another task is downloading
/// ([`AppState::claim_details_fetch`]) before it goes on without it. The
/// download itself is bounded (the session's timeout and 429 retries).
const DETAILS_WAIT_MAX: Duration = Duration::from_secs(30);

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

/// R150: the most asks the names-fill queue holds. A match end adds at most
/// the match's players, the own-history backfill at most
/// `sync::BACKFILL_FILL_MAX` per run, and Details a handful: a full queue is
/// a burst nobody needs, and it is dropped rather than grown.
pub const NAME_FILL_QUEUE_MAX: usize = 300;

/// R167: the soft cap on low-priority asks (a roster player's matches phase,
/// `LiveModel::queue_name_fill_low`): they never take more than this many of
/// the [`NAME_FILL_QUEUE_MAX`] places, so a mid-match burst of strangers'
/// histories leaves room for the match-end and Details asks.
pub const NAME_FILL_LOW_MAX: usize = 100;

/// R150: how long before a match end a match can have started and still be
/// the one that just ended. A game runs well under an hour; the slack covers
/// overtime and an app that was started mid-match.
pub const MATCH_END_WINDOW_MS: i64 = 2 * 3_600_000;

/// R150: the puuids of a stored match's players other than `me` whose name
/// match-details left blank — the only ones that can need the names fill (a
/// player named in this very row has a stored name).
pub fn unnamed_others<'a>(
    players: &'a [MatchPlayer],
    me: &'a str,
) -> impl Iterator<Item = String> + 'a {
    let full = |s: &Option<String>| s.as_deref().is_some_and(|s| !s.is_empty());
    players
        .iter()
        .filter(move |p| p.puuid != me && !(full(&p.game_name) && full(&p.tag_line)))
        .map(|p| p.puuid.clone())
}

/// R165: how many players' rank phases (`comp_updates` + `mmr`) the worker
/// runs at once. Inside the session's own buckets (the mmr burst covers ten
/// players' two calls); the worker stays a single job loop, the batch is one
/// job.
pub const RANK_CONCURRENCY: usize = 3;

/// R165: the waits before the retries of a failed live names lookup: a failed
/// lookup is asked again after the first, and once more after the second, so a
/// puuid gets at most `NAMES_RETRY_DELAYS.len() + 1` tries per stage.
pub const NAMES_RETRY_DELAYS: [Duration; 2] = [Duration::from_secs(2), Duration::from_secs(6)];

/// The players whose per-player sync ran (`LiveModel::synced`), in the two
/// phases of R165: the *rank* phase (`comp_updates` + `mmr`, run for the
/// whole roster first) and the *matches* phase (history + match-details). A
/// player counts as synced (`contains`) when the matches phase ended; a
/// player counts as ranked ([`Self::rank_done`]) when the rank phase did, or
/// the full sync did. Cleared, and a puuid removed, together: the poll loop's
/// resets (`synced.clear()`, `synced.remove(..)`) start both phases over.
#[derive(Debug, Default, Clone)]
pub struct SyncedSet {
    synced: HashSet<String>,
    ranked: HashSet<String>,
}

impl SyncedSet {
    /// The matches phase (so the whole sync) ran.
    pub fn contains(&self, puuid: &str) -> bool {
        self.synced.contains(puuid)
    }

    /// The whole sync ran (the rank phase too).
    pub fn insert(&mut self, puuid: String) -> bool {
        self.synced.insert(puuid)
    }

    pub fn remove(&mut self, puuid: &str) -> bool {
        self.ranked.remove(puuid);
        self.synced.remove(puuid)
    }

    pub fn clear(&mut self) {
        self.ranked.clear();
        self.synced.clear();
    }

    #[cfg(test)]
    pub fn extend(&mut self, puuids: impl IntoIterator<Item = String>) {
        self.synced.extend(puuids);
    }

    /// The rank phase ran (R165).
    pub fn mark_ranked(&mut self, puuid: &str) {
        self.ranked.insert(puuid.to_string());
    }

    /// The rank phase ran, or the whole sync did.
    pub fn rank_done(&self, puuid: &str) -> bool {
        self.ranked.contains(puuid) || self.synced.contains(puuid)
    }
}

/// One puuid's failed live names lookups (see [`NamesAttempted`]).
#[derive(Debug, Clone, Copy)]
struct NameFails {
    fails: usize,
    /// Asked again from this instant on.
    due: Instant,
}

/// The players a `names()` call was *answered* for (`LiveModel::names_attempted`):
/// a visible name still unknown after that is Missing, not Loading. R165: a
/// failed request is not an answer; it only counts a try
/// ([`Self::fail`]) and the player is asked again after
/// [`NAMES_RETRY_DELAYS`]. The tries are dropped with the set (`clear`, the
/// poll loop's stage reset) and when the player is answered.
#[derive(Debug, Default, Clone)]
pub struct NamesAttempted {
    set: HashSet<String>,
    failed: HashMap<String, NameFails>,
}

impl NamesAttempted {
    pub fn contains(&self, puuid: &str) -> bool {
        self.set.contains(puuid)
    }

    pub fn insert(&mut self, puuid: String) -> bool {
        self.failed.remove(&puuid);
        self.set.insert(puuid)
    }

    pub fn extend(&mut self, puuids: impl IntoIterator<Item = String>) {
        for p in puuids {
            self.insert(p);
        }
    }

    pub fn clear(&mut self) {
        self.set.clear();
        self.failed.clear();
    }

    /// When `puuid`'s next try is due, while it is still waiting after `now`.
    fn due_after(&self, puuid: &str, now: Instant) -> Option<Instant> {
        self.failed.get(puuid).map(|f| f.due).filter(|d| *d > now)
    }

    /// The player's next try is not due yet.
    fn waiting(&self, puuid: &str, now: Instant) -> bool {
        self.failed.get(puuid).is_some_and(|f| f.due > now)
    }

    /// A lookup for `puuid` failed at `now`: `Some(wait)` before the next try,
    /// or `None` after the last try — the player is attempted from then on.
    fn fail(&mut self, puuid: &str, now: Instant) -> Option<Duration> {
        let fails = self.failed.get(puuid).map_or(0, |f| f.fails) + 1;
        match NAMES_RETRY_DELAYS.get(fails - 1) {
            Some(delay) => {
                self.failed.insert(
                    puuid.to_string(),
                    NameFails {
                        fails,
                        due: now + *delay,
                    },
                );
                Some(*delay)
            }
            None => {
                self.insert(puuid.to_string());
                None
            }
        }
    }
}

/// R161: the ranked tiers (by puuid) an Agent Select answer carried, bound to
/// the match they came from. Pregame's `ID` is the core-game `MatchID` of the
/// same match, so the In Game roster can tell a tier of its own match from a
/// leftover of an earlier one.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MatchTiers {
    match_id: Option<String>,
    tiers: HashMap<String, u8>,
}

impl MatchTiers {
    /// An Agent Select answer for `match_id`. A different match replaces
    /// what is stored; the same one (a re-poll) merges by presence, so a
    /// player an answer leaves the tier out for keeps the earlier tier.
    pub fn set_pregame(&mut self, match_id: &str, tiers: HashMap<String, u8>) {
        if self.match_id.as_deref() == Some(match_id) {
            self.tiers.extend(tiers);
        } else {
            self.match_id = Some(match_id.to_string());
            self.tiers = tiers;
        }
    }

    /// The tiers stored for `match_id`; none for any other match.
    pub fn for_match(&self, match_id: &str) -> HashMap<String, u8> {
        if self.match_id.as_deref() == Some(match_id) {
            self.tiers.clone()
        } else {
            HashMap::new()
        }
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.match_id.is_none() && self.tiers.is_empty()
    }
}

/// One unit of work for the roster worker (see [`LiveModel::next_job`]).
#[derive(Debug, Clone, PartialEq)]
pub enum Job {
    /// One `names()` call: `(puuid, incognito)` pairs — `incognito` is
    /// always `false` (filtered), carried so the worker builds each
    /// `VisiblePuuid` from the real flag.
    Names(Vec<(String, bool)>),
    /// R165: the rank phase (`comp_updates` + `mmr`, then the card is
    /// published) of up to [`RANK_CONCURRENCY`] players, run at once. Handed
    /// out for the whole roster before any `Sync`.
    Rank(Vec<String>),
    /// The matches phase of one puuid (R165: history + match-details, then the
    /// card is published) — only for players whose rank phase ran.
    Sync(String),
    /// R155: the sync of one puuid again (both phases), because its rank part
    /// failed or its matches were only partly fetched
    /// (`LiveModel::due_rank_retry`). Handed out after the first-pass syncs
    /// and before the names fill and the backfill.
    RankRetry(String),
    /// R150: one names-fill request (`sync::fill_names_step`): the queued
    /// asks worth sending, in one name-service `PUT`. Handed out when no
    /// `Names`, `Sync` or due `RankRetry` job is left and before the
    /// backfill, and only between matches, like the backfill
    /// (`LiveModel::between_matches`).
    FillNames,
    /// One step of the own-history backfill (R101): only handed out when no
    /// `Names`, `Sync`, due `RankRetry` or `FillNames` job is left, so it is
    /// always behind live cards, and only in Menus or Closed (R137), never
    /// during a match
    /// — a free-for-all one included (`LiveModel::in_match`).
    Backfill,
    /// One page of the user's own competitive updates (B1,
    /// `sync::comp_backfill_step`): the lowest priority of all, behind the
    /// match-history backfill, between matches only, like it.
    CompBackfill,
}

/// One own-history backfill run (R101; `sync::backfill_step` advances it).
/// It lives in the [`LiveModel`], so a step that yields to a live job — or a
/// worker aborted by a stage change — resumes from the same cursor. The
/// stored anchor moves only when a run *finishes*: a run that ends early
/// (disconnect, account switch, a Riot error) is dropped, and the next one
/// starts over from the old anchor (ids already stored are skipped).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackfillRun {
    /// The last finished run's newest `(match_id, start_ms)`; `None` on the
    /// first run ever.
    pub anchor: Option<(String, i64)>,
    /// The current act's start, first run only (`None` when the act tables
    /// aren't loaded; each page looks it up again until it is known).
    pub act_start_ms: Option<i64>,
    /// The next match-history `startIndex`: the last page's start plus the
    /// entries it actually held (a short page mid-history leaves no gap).
    pub next_index: u32,
    /// History entries looked at so far (the first run's cap).
    pub scanned: u32,
    /// Page 0's first entry; becomes the anchor when the run finishes.
    pub newest: Option<(String, i64)>,
    /// Ids from the current page still to fetch, kept when a step yields.
    pub pending: VecDeque<String>,
    /// The last page reached a stop (the anchor, the act start, the cap, or
    /// the end of the history); the run finishes once `pending` is empty.
    pub done: bool,
    /// The history's `Total` from the latest page (`None` until a page
    /// answered, or when Riot omits it). It tells the last page from a short
    /// one, and the finish log line shows it.
    pub total: Option<u32>,
    /// R150: the players of the matches this run stored, other than the
    /// user — newest match first, each puuid once with the start of its
    /// newest match, at most `sync::BACKFILL_FILL_MAX`. Queued for the names
    /// fill when the run ends (`sync::end_run`), so one run asks in a few
    /// batched requests, not one per match.
    pub participants: Vec<NameAsk>,
}

impl BackfillRun {
    /// Adds the players of a stored match that started at `start_ms` (see
    /// [`Self::participants`]). Matches arrive newest first, so a puuid
    /// already there keeps its newer match; the list stops at the cap.
    pub fn add_participants(&mut self, start_ms: i64, puuids: Vec<String>) {
        for puuid in puuids {
            if self.participants.len() >= crate::sync::BACKFILL_FILL_MAX {
                break;
            }
            if !self.participants.iter().any(|a| a.puuid == puuid) {
                self.participants.push(NameAsk {
                    puuid,
                    match_start_ms: Some(start_ms),
                });
            }
        }
    }
}

/// One own competitive-updates backfill run (B1; `sync::comp_backfill_step`
/// advances it, one page per job). My Stats' RR chart, net RR and rank changes
/// read every competitive update of the act, and the live sync only fetches
/// the newest 5 or 20: this pages the rest in. Like [`BackfillRun`] it lives
/// in the [`LiveModel`] so a worker aborted by a stage change resumes from the
/// same cursor, and the stored anchor moves only when a run *finishes*.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompBackfillRun {
    /// The last finished run's newest `(match_id, start_ms)`; `None` on the
    /// first run ever.
    pub anchor: Option<(String, i64)>,
    /// The current act's start (`None` when the act tables aren't loaded; each
    /// page looks it up again until it is known).
    pub act_start_ms: Option<i64>,
    /// The next `startIndex`: the last page's start plus the rows it held.
    pub next_index: u32,
    /// Pages fetched so far (`sync::COMP_BACKFILL_MAX_PAGES` bounds it).
    pub pages: u32,
    /// Page 0's first row; becomes the anchor when the run finishes.
    pub newest: Option<(String, i64)>,
    /// R160: the mmr request family was on hold when the last page was asked
    /// for (`RiotError::Held`, nothing sent): no page goes out before this
    /// instant (the hold's end plus a little slack).
    pub resume_at: Option<Instant>,
}

/// The live stage/roster plus the session-scoped bookkeeping the sync
/// worker and the card builder share.
pub struct LiveModel {
    /// The stage being *shown* (`roster::view_stage`): a free-for-all match
    /// keeps the Lobby view, `Menus` (R41).
    pub stage: Stage,
    /// The raw presence is in a match (Agent Select or In Game), whatever
    /// the view shows: a free-for-all match keeps `stage` at `Menus` (R41),
    /// yet it is a match — R137 keeps the own-history backfill out of it.
    pub in_match: bool,
    /// Riot's `MapID` path for the current match (Agent Select / In Game).
    pub map_path: Option<String>,
    /// `map_path` as a valorant-api uuid — resolved at publish time (the
    /// asset tables may load after the roster arrived); never the path.
    pub map_id: Option<String>,
    /// Display/sync order (`roster::order_players`).
    pub slots: Vec<Slot>,
    /// Equipped skins of the current match's players (Player Details):
    /// core-game loadouts In Game, pre-game loadouts (allies) in Agent
    /// Select (R83). Skin names resolve later, from valorant-api.
    pub loadouts: HashMap<String, RawLoadout>,
    /// The user's own equipped skins, read once per connect from the pd
    /// personalization endpoint (R90) — so their Details shows skins
    /// outside a match too. Cleared when the account changes; also restored
    /// at startup from the copy kept in `app_state` (Closed stage), so this
    /// field *is* the stored copy's in-memory form: `details` never reads
    /// `app_state` itself (see `AppState::new`).
    pub self_loadout: Option<RawLoadout>,
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
    /// R161: the ranked tiers an Agent Select answer carried (both teams),
    /// bound to that match. Core-game answers carry no tier, so an In Game
    /// roster takes its players' tiers from here, only when its own match id
    /// is the stored one (`MatchTiers::for_match`). Replaced by a new Agent
    /// Select, emptied on every Lobby fetch, on the transition to Menus or
    /// Closed, and when VALORANT closes or the account changes.
    pub match_tiers: MatchTiers,
    /// Names from `names()` this run (visible, non-self players only).
    pub known_names: HashMap<String, (String, String)>,
    /// Players whose card showed them hidden the last time they were on
    /// screen this run, with the agent it showed (once locked, R99) — see
    /// [`Self::note_hidden`]. Player Details builds such a player's card
    /// off screen by the same hidden-name rule (after a dodge or the match
    /// end), never with a name-service name from an earlier visible
    /// sighting. Cleared when the account changes. It does not limit the
    /// names fill: a player hidden in a finished match is asked about after
    /// it (R153).
    pub seen_hidden: HashMap<String, Option<String>>,
    /// Players a `names()` call was attempted for (since the last reset):
    /// a visible name still unknown after that is Missing, not Loading.
    pub names_attempted: NamesAttempted,
    /// Players whose per-player sync ran (since the last reset): a field
    /// still unknown after that is Missing, not Loading. R165: in two phases,
    /// see [`SyncedSet`].
    pub synced: SyncedSet,
    /// The subset of `synced` whose match-history call answered this cycle:
    /// only these take part in party detection (a failed history leaves the
    /// stored one from an earlier session, which would draw a false
    /// stripe). Cleared and pruned together with `synced`.
    pub party_ready: HashSet<String>,
    /// The worker currently entitled to run (R45). Set and cleared only
    /// under this model's lock, together with the "is there work?" check,
    /// so no roster change can fall between a worker deciding to exit and
    /// a new one being refused.
    worker_id: Option<u64>,
    next_worker_id: u64,
    /// The own-history backfill run in progress, if any (R101). Cleared when
    /// the account changes or the session drops.
    pub backfill: Option<BackfillRun>,
    /// B1: the own competitive-updates backfill run in progress, if any.
    /// Cleared like [`Self::backfill`] (account switch, session drop).
    pub comp_backfill: Option<CompBackfillRun>,
    /// Review round 1: the user's newest competitive updates as the last
    /// `comp_updates(0..N)` of their own sync stored them (`(puuid, [(match
    /// id, start ms)])`, newest first): the competitive-updates backfill skips
    /// its first page when they hold its anchor. Read only for the account it
    /// names.
    pub own_comp_head: Option<(String, Vec<(String, i64)>)>,
    /// R150: the asks waiting for the names fill (`Job::FillNames`), oldest
    /// first, one per puuid. In memory only: a restart loses them, and the
    /// next match end, Details open or backfill asks again.
    ///
    /// R167: two priorities in one deque. High asks (match end, Player and
    /// Match Details, a backfill, asks put back by a dropped step) sit in
    /// front; low asks ([`Self::name_fill_low`]: a roster player's matches
    /// phase) sit behind them, so `take_fill` serves the high ones first.
    pub name_fill: VecDeque<NameAsk>,
    /// R167: the puuids of `name_fill` that are low priority. Always at the
    /// back of the deque; a stale entry (the deque was cleared) is harmless:
    /// only puuids found in the deque count.
    name_fill_low: HashSet<String>,
    /// R150: when the last match ended (epoch ms) — In Game to Menus, a
    /// free-for-all one included — until the next one starts. A match stored
    /// meanwhile that the user played in, that started shortly before, is the
    /// match that just ended: its players are asked about
    /// (`AppState::match_end_asks`).
    pub match_end_ms: Option<i64>,
    /// R165: the last match end seen by [`Self::observe_match_end`], with the
    /// account it belongs to. Unlike `match_end_ms` a new match does not clear
    /// it, so a match published after the next one has started still raises
    /// its asks (`AppState::match_end_asks`).
    last_match_end: Option<(String, i64)>,
    /// R155: the players whose rank sync failed and are synced again later
    /// (`Self::note_rank_sync`, `Self::due_rank_retry`). Only players on the
    /// live roster; dropped with a roster change, an account switch, and when
    /// VALORANT closes.
    pub rank_retries: RankRetries,
    /// R160: name-service answered "on hold" (`RiotError::Held`, nothing
    /// sent): until this instant no names job is handed out
    /// ([`Self::names_needed`], [`Self::fill_ready`]), so the worker neither
    /// spins on a refused call nor marks anyone as asked.
    names_hold_until: Option<Instant>,
    /// Review round 1: the names *fill* stands down until this instant (the
    /// user's own presence already said a match began, before the poll loop
    /// saw it, `sync::fill_names_step`). Fill only: the live roster's names
    /// step is not held by it ([`Self::names_needed`] ignores it), unlike
    /// the global hold above.
    fill_stand_down_until: Option<Instant>,
    /// R165: when the earliest waiting names retry is due ([`Self::arm_names_retry_wake`]);
    /// only [`Self::names_wakeup_due`] reads it, nothing is blocked by it.
    names_retry_wake: Option<Instant>,
    /// Test clock: added to "now" wherever the retries are timed, so a test
    /// moves time forward instead of sleeping (`Self::advance_clock`).
    #[cfg(test)]
    clock_offset: Duration,
    /// The poll loop has reached its first verdict on whether VALORANT is
    /// running (`LiveState::settled`). Starts `false`, set once by
    /// [`Self::settle`] and never cleared: a supervisor restart (R169) resets
    /// the rest of the model, not this, so the UI's start page is decided once.
    settled: bool,
    /// The last built/emitted state (`live_state()` returns it).
    pub current: LiveState,
    last_bytes: Vec<u8>,
}

impl LiveModel {
    pub fn new(self_puuid: Option<String>) -> Self {
        Self {
            stage: Stage::Closed,
            in_match: false,
            map_path: None,
            map_id: None,
            slots: Vec::new(),
            loadouts: HashMap::new(),
            self_loadout: None,
            coregame_match_id: None,
            self_puuid,
            self_name: None,
            self_name_attempted: false,
            self_roster: None,
            match_tiers: MatchTiers::default(),
            known_names: HashMap::new(),
            seen_hidden: HashMap::new(),
            names_attempted: NamesAttempted::default(),
            synced: SyncedSet::default(),
            party_ready: HashSet::new(),
            worker_id: None,
            next_worker_id: 0,
            backfill: None,
            comp_backfill: None,
            own_comp_head: None,
            name_fill: VecDeque::new(),
            name_fill_low: HashSet::new(),
            match_end_ms: None,
            last_match_end: None,
            rank_retries: RankRetries::default(),
            names_hold_until: None,
            fill_stand_down_until: None,
            names_retry_wake: None,
            #[cfg(test)]
            clock_offset: Duration::ZERO,
            settled: false,
            current: LiveState {
                stage: Stage::Closed,
                map_id: None,
                players: Vec::new(),
                settled: false,
            },
            last_bytes: Vec::new(),
        }
    }

    /// The poll loop's first verdict is in (VALORANT running in some stage,
    /// absent for good, the Riot Client not there, or not reachable): from now
    /// on the published state says `settled`. `true` when this call changed it
    /// (the caller then publishes).
    pub fn settle(&mut self) -> bool {
        !std::mem::replace(&mut self.settled, true)
    }

    /// Whether the first verdict is in (see [`Self::settle`]).
    #[cfg(test)]
    pub fn is_settled(&self) -> bool {
        self.settled
    }

    fn self_id(&self) -> &str {
        self.self_puuid.as_deref().unwrap_or("")
    }

    /// Builds the full `LiveState` (pure; `now_ms` should be [`card_clock`]).
    /// `map_uuid` turns a stored Riot map path into a valorant-api uuid (for
    /// a remembered name's source map).
    pub fn build(
        &self,
        acts: &[Act],
        now_ms: i64,
        map_uuid: &dyn Fn(&str) -> Option<String>,
    ) -> LiveState {
        let current_act_id = current_act(acts, now_ms)
            .map(|a| a.id.as_str())
            .unwrap_or("");
        let marks = self.party_marks();
        LiveState {
            stage: self.stage,
            map_id: self.map_id.clone(),
            settled: self.settled,
            players: self
                .slots
                .iter()
                .map(|s| {
                    let party = marks.get(&s.roster.puuid).copied();
                    self.card_for(s, acts, current_act_id, now_ms, party, map_uuid)
                })
                .collect(),
        }
    }

    /// Party stripes for the players on screen (CLAUDE.md "Party
    /// detection": both teams, Agent Select + In Game — the Lobby shows
    /// only the user's own party, which is never marked). Recomputed on
    /// every build: it's a few comparisons over at most 10 players, all
    /// from the slots' DB bundles (no request, no query).
    ///
    /// Only players whose history was synced this cycle take part (R53):
    /// a cached history from an earlier session could link players who no
    /// longer play together — and so could the stored one of a player whose
    /// history call failed now (`party_ready`, not just `synced`). Their
    /// mark appears on the publish after their sync.
    pub fn party_marks(&self) -> HashMap<String, PartyMark> {
        if !matches!(self.stage, Stage::Pregame | Stage::Ingame) {
            return HashMap::new();
        }
        let self_id = self.self_id();
        let synced: Vec<&Slot> = self
            .slots
            .iter()
            .filter(|s| self.party_ready.contains(&s.roster.puuid))
            .collect();
        let players: Vec<(String, Team)> = synced
            .iter()
            .map(|s| (s.roster.puuid.clone(), s.roster.team))
            .collect();
        let recent = synced
            .iter()
            .map(|s| (s.roster.puuid.clone(), s.db.recent.clone()))
            .collect();
        let self_party: HashSet<String> = self
            .slots
            .iter()
            .filter(|s| s.roster.is_self_party || s.roster.puuid == self_id)
            .map(|s| s.roster.puuid.clone())
            .collect();
        detect_parties(&players, &recent, &self_party)
    }

    /// One card. `party` is this player's mark from [`Self::party_marks`]
    /// (`None` off screen, e.g. Player Details for someone not in the
    /// roster). `map_uuid` converts a remembered name's stored Riot map path
    /// (`KnownName::with_map_uuid`, R95).
    pub fn card_for(
        &self,
        slot: &Slot,
        acts: &[Act],
        current_act_id: &str,
        now_ms: i64,
        party: Option<PartyMark>,
        map_uuid: &dyn Fn(&str) -> Option<String>,
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
        // - an incognito player: no visible name is fed into the card from
        //   any source (build_card also ignores it, belt and braces) — only
        //   the best name the DB knows from any source, in the remembered
        //   style (`PlayerDb::remembered`, `Repo::known_name`, R145);
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
            // R165: the rank phase settles the MMR fields; the matches phase
            // (`data_done`) settles the rest.
            None if data_done || self.synced.rank_done(&r.puuid) => Field::Missing,
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
                // R100: a hidden level shows for the user's own card only;
                // an unknown level (0) is `—` for everyone.
                hide_account_level: (r.hide_account_level && !is_self) || r.account_level == 0,
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
            is_self,
            // CLAUDE.md: the user and their party get no stripe and no
            // encounter badge (build_card drops both for them).
            is_self_party: r.is_self_party || is_self,
            remembered: if is_self {
                None
            } else {
                slot.db
                    .remembered
                    .clone()
                    .map(|known| known.with_map_uuid(map_uuid))
            },
            party,
            encounters: slot.db.encounters,
            // R154: read only for a player with no snapshot, used only when
            // MMR is not a value.
            stored_tier: slot.db.stored_tier,
            live_tier: r.live_tier,
        });
        settle_card(&mut card, names_done, data_done);
        card
    }

    /// Players shown hidden right now, with the agent their card shows (once
    /// locked, R99) — R18: their names must not leak through Player Details'
    /// Top Teammates either; there they're shown by the same rules as on
    /// their card. The user's own name is known locally (chat session) and
    /// is not hidden from the user.
    pub fn hidden_players(&self) -> HashMap<String, Option<String>> {
        let self_id = self.self_id();
        self.slots
            .iter()
            .filter(|s| s.roster.incognito && s.roster.puuid != self_id)
            .map(|s| {
                let agent = shown_agent(&s.roster.character_id, &s.roster.selection_state);
                (s.roster.puuid.clone(), agent.map(str::to_string))
            })
            .collect()
    }

    /// Records who is shown hidden on screen right now (see
    /// [`Self::seen_hidden`]): every incognito player but the user, with
    /// the agent their card shows; a player shown visible now is dropped.
    /// Called whenever a roster is installed.
    pub fn note_hidden(&mut self) {
        let hidden = self.hidden_players();
        let self_id = self.self_id().to_string();
        for s in &self.slots {
            let p = &s.roster.puuid;
            match hidden.get(p) {
                Some(agent) => {
                    self.seen_hidden.insert(p.clone(), agent.clone());
                }
                None if *p != self_id => {
                    self.seen_hidden.remove(p);
                }
                None => {}
            }
        }
    }

    /// Players to send to name-service: never the user (R43 — their name
    /// comes from the chat session), never anyone whose `Incognito` is
    /// true, only names not known and not yet asked for.
    ///
    /// R165: a player whose lookup failed is asked again only once the retry
    /// delay has passed ([`Self::note_names_failed`]).
    pub fn names_needed(&self) -> Vec<(String, bool)> {
        if self.names_deferred() {
            return Vec::new();
        }
        let self_id = self.self_id();
        let now = self.now();
        self.slots
            .iter()
            .filter(|s| s.roster.puuid != self_id)
            .filter(|s| !s.roster.incognito)
            .filter(|s| !self.known_names.contains_key(&s.roster.puuid))
            .filter(|s| !self.names_attempted.contains(&s.roster.puuid))
            .filter(|s| !self.names_attempted.waiting(&s.roster.puuid, now))
            .map(|s| (s.roster.puuid.clone(), s.roster.incognito))
            .collect()
    }

    /// R165: a `names()` request for `asked` failed (an error is not an
    /// answer): each puuid is asked again after [`NAMES_RETRY_DELAYS`], at most
    /// three tries in all; after the last they count as attempted (the card
    /// shows `—`). Returns the shortest wait before a retry is due, `None`
    /// when no puuid has a retry left. Privacy is unchanged: only
    /// [`Self::names_needed`] (never an incognito player) hands puuids out.
    pub fn note_names_failed(&mut self, asked: &[String]) -> Option<Duration> {
        let now = self.now();
        asked
            .iter()
            .filter_map(|p| self.names_attempted.fail(p, now))
            .min()
    }

    /// R165: arms the wake-up of the earliest still-waiting names retry: the
    /// earliest due time among players who are on the roster and still need a
    /// name (not the user, not hidden, not known, not attempted). Read only by
    /// [`Self::names_wakeup_due`]: unlike [`Self::defer_names`] (R160's hold)
    /// it blocks nothing, so a new player's first ask is never delayed. Called
    /// after every names step, so each staggered retry is woken in turn.
    pub fn arm_names_retry_wake(&mut self) {
        let now = self.now();
        let self_id = self.self_id();
        self.names_retry_wake = self
            .slots
            .iter()
            .filter(|s| s.roster.puuid != self_id && !s.roster.incognito)
            .filter(|s| !self.known_names.contains_key(&s.roster.puuid))
            .filter(|s| !self.names_attempted.contains(&s.roster.puuid))
            .filter_map(|s| self.names_attempted.due_after(&s.roster.puuid, now))
            .min();
    }

    /// The next player whose matches phase is due, in display order (self ->
    /// party -> allies -> enemies).
    pub fn next_unsynced(&self) -> Option<String> {
        self.slots
            .iter()
            .map(|s| &s.roster.puuid)
            .find(|p| !self.synced.contains(p))
            .cloned()
    }

    /// R165: the players whose rank phase is due, in display order, at most
    /// [`RANK_CONCURRENCY`] of them: one `Job::Rank`. Handed out ahead of any
    /// matches phase, so the whole roster's rank comes before the first
    /// match-details download.
    pub fn next_rank_batch(&self) -> Vec<String> {
        self.slots
            .iter()
            .map(|s| &s.roster.puuid)
            .filter(|p| !self.synced.rank_done(p))
            .take(RANK_CONCURRENCY)
            .cloned()
            .collect()
    }

    /// A `Names`, `Sync` or (R155) due `RankRetry` job is waiting: what the
    /// backfill and the names fill must yield to.
    pub fn live_job_pending(&self) -> bool {
        !self.names_needed().is_empty()
            || self.next_unsynced().is_some()
            || self.due_rank_retry().is_some()
    }

    /// R165: records the match end on record (`match_end_ms`) as the last one.
    /// The poll loop sets `match_end_ms` at the end and clears it when the next
    /// match starts; this copy is kept for `AppState::match_end_asks`, so a
    /// match Riot publishes after the next one has begun still raises its
    /// asks. Called wherever the model is locked for the worker or a publish,
    /// which is always between the end and the next match.
    pub fn observe_match_end(&mut self) {
        if let (Some(end), Some(me)) = (self.match_end_ms, self.self_puuid.clone()) {
            self.last_match_end = Some((me, end));
        }
    }

    /// R165: the last match end of the current account, whether or not a new
    /// match has cleared `match_end_ms` since.
    pub fn last_match_end_ms(&self) -> Option<i64> {
        let (owner, end) = self.last_match_end.as_ref()?;
        (self.self_puuid.as_deref() == Some(owner.as_str())).then_some(*end)
    }

    /// Between matches: Lobby or Closed, and the raw presence is not in a
    /// match (R137: a free-for-all match keeps the Lobby view, yet it is a
    /// match). The only time the own-history backfill and the names fill (R150)
    /// run.
    pub fn between_matches(&self) -> bool {
        !self.in_match && matches!(self.stage, Stage::Menus | Stage::Closed)
    }

    /// R137: the own-history backfill runs only between matches (Lobby, or
    /// Closed), never in Agent Select or In Game: a first run is tens of MB
    /// of match-details and a 429 it triggers would hold the rate bucket the
    /// live cards need. A free-for-all match counts too, although its view
    /// stays the Lobby (R41): the raw presence (`in_match`) decides. The
    /// run's cursor survives the pause.
    pub fn backfill_ready(&self) -> bool {
        self.backfill.is_some() && self.between_matches()
    }

    /// B1: the own competitive-updates backfill has a page to fetch: a run is
    /// active, it is between matches (same gate as [`Self::backfill_ready`]),
    /// and the mmr hold the last page met is over.
    pub fn comp_backfill_ready(&self) -> bool {
        self.between_matches()
            && self
                .comp_backfill
                .as_ref()
                .is_some_and(|run| run.resume_at.is_none_or(|at| self.now() >= at))
    }

    /// B1: a paused run's hold has run out: the poll tick starts a worker (it
    /// exited when the pause left nothing to do, and the passing of time is no
    /// other wake-up). Not one-shot: it stays true until the run's next page
    /// clears the pause, and a worker that is already running ignores it.
    pub fn comp_backfill_resume_due(&self) -> bool {
        self.comp_backfill
            .as_ref()
            .and_then(|run| run.resume_at)
            .is_some_and(|at| self.now() >= at)
            && self.between_matches()
    }

    /// B1: the mmr family is on hold for `wait` more: the run's next page
    /// waits for it, uses no failure count (R160).
    pub fn pause_comp_backfill(&mut self, wait: Duration) {
        let at = self.now() + wait;
        if let Some(run) = self.comp_backfill.as_mut() {
            run.resume_at = Some(run.resume_at.map_or(at, |t| t.max(at)));
        }
    }

    /// R150: a names fill is queued and it is between matches. The asks wait
    /// in the queue through a match and run after it.
    pub fn fill_ready(&self) -> bool {
        !self.name_fill.is_empty()
            && self.between_matches()
            && !self.names_deferred()
            && !self.fill_standing_down()
    }

    /// Review round 1: the names fill (and only it) is not handed out for
    /// `wait`: the poll loop has not yet seen the match the presence reports.
    /// Only ever extended.
    pub fn stand_down_fill(&mut self, wait: Duration) {
        let until = self.now() + wait;
        self.fill_stand_down_until =
            Some(self.fill_stand_down_until.map_or(until, |u| u.max(until)));
    }

    fn fill_standing_down(&self) -> bool {
        self.fill_stand_down_until.is_some_and(|u| self.now() < u)
    }

    /// R160: no names job is handed out for `wait` (a name-service call was
    /// refused because the `other` request family is on hold). Only ever
    /// extended.
    pub fn defer_names(&mut self, wait: Duration) {
        let until = self.now() + wait;
        self.names_hold_until = Some(self.names_hold_until.map_or(until, |u| u.max(until)));
    }

    fn names_deferred(&self) -> bool {
        self.names_hold_until.is_some_and(|u| self.now() < u)
    }

    /// R160: `true` once, when a names deferral has just run out and names or
    /// names-fill work is waiting: the poll tick then starts a worker (the
    /// worker exited when the deferral emptied its job list, and a pure
    /// passing of time is no other wake-up). One-shot: the deferral is
    /// cleared here, so a job that cannot run cannot make the tick start a
    /// worker again and again.
    pub fn names_wakeup_due(&mut self) -> bool {
        let now = self.now();
        let mut wake = false;
        if self.names_hold_until.is_some_and(|until| now >= until) {
            self.names_hold_until = None;
            wake |= !self.names_needed().is_empty() || self.fill_ready();
        }
        // The fill's stand-down ran out: the same wake-up, once.
        if self.fill_stand_down_until.is_some_and(|until| now >= until) {
            self.fill_stand_down_until = None;
            wake |= self.fill_ready();
        }
        // R165: a waiting names retry came due (one-shot like the hold; the
        // names step that follows arms the next one).
        if self.names_retry_wake.is_some_and(|at| now >= at) {
            self.names_retry_wake = None;
            wake |= !self.names_needed().is_empty();
        }
        wake
    }

    /// R165: drops the armed names-retry wake-up (the stage reset forgets the
    /// failure counts with `names_attempted.clear()`, so no retry is waiting).
    pub fn clear_names_retry_wake(&mut self) {
        self.names_retry_wake = None;
    }

    /// R160: forgets a names deferral. A new session (a reconnect, another
    /// account) has its own, empty hold: what the old one refused is not
    /// refused now.
    pub fn reset_names_hold(&mut self) {
        self.names_hold_until = None;
        self.fill_stand_down_until = None;
        self.names_retry_wake = None;
    }

    /// The clock the rank retries are timed by.
    fn now(&self) -> Instant {
        let now = Instant::now();
        #[cfg(test)]
        let now = now + self.clock_offset;
        now
    }

    /// Moves the retry clock forward (tests only: the 20 s and 60 s waits).
    #[cfg(test)]
    pub fn advance_clock(&mut self, by: Duration) {
        self.clock_offset += by;
    }

    /// R155: a per-player sync of `puuid` ended (`retry`: it was a retry, not
    /// the first pass) and `failed` says its rank part did. A failed first
    /// pass schedules the first retry, a failed retry the next one (or, after
    /// the last, nothing: [`crate::rank_retry::RANK_RETRY_DELAYS`]); a success
    /// ends the round. Nothing is scheduled for a player who is not on the
    /// live roster, nor while VALORANT is closed. A retry was counted when it
    /// was handed out ([`Self::next_job`]).
    pub fn note_rank_sync(&mut self, puuid: &str, failed: bool, retry: bool) {
        let shown = self.stage != Stage::Closed && self.shows(puuid);
        if !shown {
            self.rank_retries.forget(puuid);
            return;
        }
        let now = self.now();
        if retry {
            self.rank_retries.finished(puuid, failed, now);
        } else {
            self.rank_retries.first_pass(puuid, failed, now);
        }
        if !failed {
            return;
        }
        match self.rank_retries.retries_done(puuid) {
            Some(done) => log::info!(
                "rank sync for {} failed; retry {} of {} in {}s",
                short(puuid),
                done + 1,
                RANK_RETRY_DELAYS.len(),
                RANK_RETRY_DELAYS[done].as_secs()
            ),
            None if retry => log::info!("rank sync for {} failed again; giving up", short(puuid)),
            None => {}
        }
    }

    /// R160: a per-player sync of `puuid` ended (`retry`: it was a retry) and
    /// its rank part did not run only because the mmr request family was on
    /// hold, `left` of it still to go: nothing was sent, so no R155 retry is
    /// used up. The player is synced again once the hold is over (+ a moment).
    /// Same guards as [`Self::note_rank_sync`]: nothing for a player who is not
    /// on the live roster, nor while VALORANT is closed.
    pub fn note_rank_held(&mut self, puuid: &str, left: Duration, retry: bool) {
        let shown = self.stage != Stage::Closed && self.shows(puuid);
        if !shown {
            self.rank_retries.forget(puuid);
            return;
        }
        let now = self.now();
        if retry {
            self.rank_retries.retry_held(puuid, left, now);
        } else {
            self.rank_retries.first_pass_held(puuid, left, now);
        }
        log::info!(
            "rank sync for {} waits for the mmr hold ({}s more); no retry used",
            short(puuid),
            left.as_secs_f64().ceil() as u64
        );
    }

    /// R165: the matches phase of a first-pass sync of `puuid` was only partly
    /// done — its history call or a match-details download failed (anything
    /// but a gone client). It joins the R155 schedule like a failed rank part:
    /// the first retry is due 20 s from now and syncs the player again (only
    /// the missing ids are fetched). A round already pending for the player
    /// (their rank part failed or was held in the same pass) is left as it is.
    /// Same guards as [`Self::note_rank_sync`].
    pub fn note_partial_sync(&mut self, puuid: &str) {
        if self.stage == Stage::Closed || !self.shows(puuid) {
            self.rank_retries.forget(puuid);
            return;
        }
        if self.rank_retries.retries_done(puuid).is_some() {
            return;
        }
        let now = self.now();
        self.rank_retries.first_pass(puuid, true, now);
        log::info!(
            "sync for {} was partial; retry 1 of {} in {}s",
            short(puuid),
            RANK_RETRY_DELAYS.len(),
            RANK_RETRY_DELAYS[0].as_secs()
        );
    }

    /// R160/R165: the matches phase of a first-pass sync of `puuid` found the
    /// `other` request family on hold (`left` of it to go) and sent nothing:
    /// not a failure, so no retry is used. The player is synced again once the
    /// hold is over, unless a round is already pending for them (a failed or
    /// held rank part of the same pass): that round's sync covers it. Same
    /// guards as [`Self::note_rank_sync`].
    pub fn note_matches_held(&mut self, puuid: &str, left: Duration) {
        if self.stage == Stage::Closed || !self.shows(puuid) {
            self.rank_retries.forget(puuid);
            return;
        }
        if self.rank_retries.retries_done(puuid).is_some() {
            return;
        }
        let now = self.now();
        self.rank_retries.first_pass_held(puuid, left, now);
        log::info!(
            "sync for {} waits for the request hold ({}s more); no retry used",
            short(puuid),
            left.as_secs_f64().ceil() as u64
        );
    }

    /// R155: the first player on the live roster (display order) whose rank
    /// retry is due — `None` while VALORANT is closed, and for a player who
    /// is no longer shown. Like the first-pass syncs it is work for the
    /// roster worker ([`Self::next_job`]); waiting for it is not.
    pub fn due_rank_retry(&self) -> Option<String> {
        if self.stage == Stage::Closed || self.rank_retries.is_empty() {
            return None;
        }
        let now = self.now();
        self.slots
            .iter()
            .map(|s| &s.roster.puuid)
            .find(|p| self.rank_retries.is_due(p, now))
            .cloned()
    }

    /// R155: drops the pending retries of players the roster no longer shows
    /// (all of them while VALORANT is closed). Called whenever a roster is
    /// installed; [`Self::due_rank_retry`] never hands out such a player
    /// anyway, but a stale round must not come back with a later roster.
    pub fn prune_rank_retries(&mut self) {
        let Self {
            stage,
            slots,
            rank_retries,
            ..
        } = self;
        if *stage == Stage::Closed {
            rank_retries.clear();
        } else {
            rank_retries.retain(|p| slots.iter().any(|s| s.roster.puuid == p));
        }
    }

    fn shows(&self, puuid: &str) -> bool {
        self.slots.iter().any(|s| s.roster.puuid == puuid)
    }

    pub fn has_pending_work(&self) -> bool {
        self.live_job_pending()
            || self.fill_ready()
            || self.backfill_ready()
            || self.comp_backfill_ready()
    }

    /// R150: queues asks for the names fill, one per puuid. Asks
    /// [`Self::fill_allowed`] refuses (the user, an empty id) are not queued
    /// at all — and are checked again when taken. Asks are accepted at any
    /// stage, a match included: they wait in the queue until it is over
    /// ([`Self::take_fill`]). Asked twice, the newer match's ask stays (an ask
    /// raised by a finished match covers a puuid asked before; an on-demand
    /// one never overrides it). A full queue ([`NAME_FILL_QUEUE_MAX`]) takes
    /// no new puuid.
    pub fn queue_name_fill(&mut self, asks: impl IntoIterator<Item = NameAsk>) {
        for ask in asks {
            if !self.fill_allowed(&ask.puuid) {
                continue;
            }
            // R167: a high ask for a puuid queued low promotes it: it leaves
            // the low part (it is re-inserted below, in front of it).
            let promoted = self.name_fill_low.remove(&ask.puuid);
            if let Some(i) = self.name_fill.iter().position(|q| q.puuid == ask.puuid) {
                let mut queued = self.name_fill.remove(i).expect("position is in range");
                // `None` (on demand) is older than any match.
                queued.match_start_ms = queued.match_start_ms.max(ask.match_start_ms);
                if promoted {
                    let at = self.name_fill.len() - self.low_count();
                    self.name_fill.insert(at, queued);
                } else {
                    self.name_fill.insert(i, queued);
                }
                continue;
            }
            // R167: a full queue makes room for a high ask by dropping its
            // oldest low ones; with none left it takes no new puuid.
            while self.name_fill.len() >= NAME_FILL_QUEUE_MAX {
                let Some(i) = self
                    .name_fill
                    .iter()
                    .position(|q| self.name_fill_low.contains(&q.puuid))
                else {
                    break;
                };
                if let Some(evicted) = self.name_fill.remove(i) {
                    self.name_fill_low.remove(&evicted.puuid);
                }
            }
            if self.name_fill.len() < NAME_FILL_QUEUE_MAX {
                let at = self.name_fill.len() - self.low_count();
                self.name_fill.insert(at, ask);
            }
        }
    }

    /// R167: queues low-priority asks (a roster player's matches phase stored
    /// matches whose players have no name). Like [`Self::queue_name_fill`],
    /// but they go behind every high ask, never evict anything, and take at
    /// most [`NAME_FILL_LOW_MAX`] places. A puuid already queued (either
    /// priority) keeps its place and takes the newer match.
    pub fn queue_name_fill_low(&mut self, asks: impl IntoIterator<Item = NameAsk>) {
        for ask in asks {
            if !self.fill_allowed(&ask.puuid) {
                continue;
            }
            if let Some(queued) = self.name_fill.iter_mut().find(|q| q.puuid == ask.puuid) {
                queued.match_start_ms = queued.match_start_ms.max(ask.match_start_ms);
            } else if self.name_fill.len() < NAME_FILL_QUEUE_MAX
                && self.low_count() < NAME_FILL_LOW_MAX
            {
                self.name_fill_low.insert(ask.puuid.clone());
                self.name_fill.push_back(ask);
            }
        }
    }

    /// Empties the names-fill queue and its low-priority set together (an
    /// account switch, R150).
    pub fn clear_name_fill(&mut self) {
        self.name_fill.clear();
        self.name_fill_low.clear();
    }

    /// How many of the queued asks are low priority.
    fn low_count(&self) -> usize {
        self.name_fill
            .iter()
            .filter(|q| self.name_fill_low.contains(&q.puuid))
            .count()
    }

    /// R150, R153 (user, 2026-09-30) with R16: whether the names fill may ask
    /// about `puuid` at all: not the user (their name comes from the chat
    /// session, R43) and not an empty id. Whether *someone was hidden* is not
    /// a reason to refuse: after a match, the game's own Match History shows
    /// the real names of the players who hid theirs during it, and the fill
    /// exists to capture those names (stored by puuid, shown later in the
    /// remembered style, R145) — so a player shown hidden in the match that
    /// just ended, or earlier this run ([`Self::seen_hidden`]), is asked
    /// about like anyone else.
    ///
    /// The one hard guard is the stage, not the player: nothing is ever sent
    /// while a match is running — Agent Select, In Game, a free-for-all
    /// match ([`Self::between_matches`]). It is enforced where asks leave
    /// the queue ([`Self::take_fill`]) and again right before the request
    /// (`sync::fill_names_step`), and the live-stage lookup
    /// (`names_needed` -> `Job::Names`) still never asks for an incognito
    /// player (CLAUDE.md "Never request a hidden player's name").
    pub fn fill_allowed(&self, puuid: &str) -> bool {
        !puuid.is_empty() && puuid != self.self_id()
    }

    /// R167: the names fill's `VisiblePuuid`s for `asks` — the only place
    /// that builds them with [`VisiblePuuid::between_matches`]. **Empty while
    /// a match is running** ([`Self::between_matches`]: Agent Select, In
    /// Game, a free-for-all match), so the claim "no live stage" is checked at
    /// the door itself and not only where the asks leave the queue
    /// ([`Self::take_fill`]). Asks [`Self::fill_allowed`] refuses (the user, an
    /// empty id) are left out. Whether a player was hidden does not matter
    /// between matches (R153).
    pub fn fill_visible(&self, asks: &[NameAsk]) -> Vec<VisiblePuuid> {
        if !self.between_matches() {
            return Vec::new();
        }
        asks.iter()
            .filter(|a| self.fill_allowed(&a.puuid))
            .filter_map(|a| VisiblePuuid::between_matches(a.puuid.clone()))
            .collect()
    }

    /// R150: takes up to `n` asks off the queue, oldest first — **none while a
    /// match is running** ([`Self::between_matches`]): the asks stay queued,
    /// the fill is paused, and they go out after the match. Asks that
    /// [`Self::fill_allowed`] refuses are dropped on the way (and don't count
    /// towards `n`); they are not kept for later.
    pub fn take_fill(&mut self, n: usize) -> Vec<NameAsk> {
        let mut taken = Vec::new();
        if !self.between_matches() {
            return taken;
        }
        while taken.len() < n {
            let Some(ask) = self.name_fill.pop_front() else {
                break;
            };
            self.name_fill_low.remove(&ask.puuid);
            if self.fill_allowed(&ask.puuid) {
                taken.push(ask);
            }
        }
        taken
    }

    /// R101: starts an own-history backfill run. A no-op while one is
    /// active, so a second trigger (a match ended mid-run) keeps its cursor.
    pub fn start_backfill(&mut self, anchor: Option<(String, i64)>, act_start_ms: Option<i64>) {
        if self.backfill.is_some() {
            return;
        }
        self.backfill = Some(BackfillRun {
            anchor,
            act_start_ms,
            next_index: 0,
            scanned: 0,
            newest: None,
            pending: VecDeque::new(),
            done: false,
            total: None,
            participants: Vec::new(),
        });
    }

    /// B1: starts an own competitive-updates run. A no-op while one is active,
    /// so a second trigger (a match ended mid-run) keeps its cursor.
    pub fn start_comp_backfill(
        &mut self,
        anchor: Option<(String, i64)>,
        act_start_ms: Option<i64>,
    ) {
        if self.comp_backfill.is_some() {
            return;
        }
        self.comp_backfill = Some(CompBackfillRun {
            anchor,
            act_start_ms,
            next_index: 0,
            pages: 0,
            newest: None,
            resume_at: None,
        });
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
        self.observe_match_end();
        let names = self.names_needed();
        if !names.is_empty() {
            return Some(Job::Names(names));
        }
        // R165: the rank of the whole roster first (rank, then publish), the
        // matches phases after.
        let batch = self.next_rank_batch();
        if !batch.is_empty() {
            return Some(Job::Rank(batch));
        }
        if let Some(puuid) = self.next_unsynced() {
            return Some(Job::Sync(puuid));
        }
        // R155: after every first-pass sync, before the fill and the
        // backfill. Not due: not work; the worker never waits for it.
        if let Some(puuid) = self.due_rank_retry() {
            // Counted now, not when it ends: a retry cut short (the worker
            // is aborted by a stage change) has still used up its turn.
            let now = self.now();
            self.rank_retries.started(&puuid, now);
            return Some(Job::RankRetry(puuid));
        }
        // After the live cards, before the backfill: one request a screen
        // may be waiting on. Only between matches.
        if self.fill_ready() {
            return Some(Job::FillNames);
        }
        // Lowest priority: only with no live job left, and only between
        // matches.
        if self.backfill_ready() {
            return Some(Job::Backfill);
        }
        // B1: behind the match-history backfill, one page per job.
        if self.comp_backfill_ready() {
            return Some(Job::CompBackfill);
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
        settle(&mut card.act_wins);
        settle(&mut card.act_games);
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
            live_tier: None,
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
    /// R150: the open Match Details, rebuilt because names arrived.
    fn match_view(&self, view: &MatchDetailsView);
    /// R163: the asset tables (names, colours) loaded or were refreshed.
    /// Default: nothing (test sinks that do not record it).
    fn meta(&self, _meta: &Meta) {}
    /// B2: the user's My Stats changed (`"mystats"`; the same JSON the
    /// `my_stats` command returns). Default: nothing.
    fn my_stats(&self, _stats: &MyStats) {}
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

    fn match_view(&self, view: &MatchDetailsView) {
        if let Err(e) = self.emit("match", view) {
            log::warn!("emit match failed: {e}");
        }
    }

    fn meta(&self, meta: &Meta) {
        if let Err(e) = self.emit("meta", meta) {
            log::warn!("emit meta failed: {e}");
        }
    }

    fn my_stats(&self, stats: &MyStats) {
        if let Err(e) = self.emit("mystats", stats) {
            log::warn!("emit mystats failed: {e}");
        }
    }
}

/// The act table every "current act" / "previous act" reads (R162): valorant-
/// api's list, with Riot's own answer folded in while it is fresh. The one
/// place that decides; nothing outside this struct applies the date rule to
/// the raw table.
struct ActsState {
    /// valorant-api's acts, ascending by start.
    base: Arc<Vec<Act>>,
    /// Riot's content-service answer, with the wall-clock time after which
    /// it is stale (the act's end; a late Riot gets a short trust window).
    riot: Option<(ContentActs, i64)>,
    /// `base` with `riot` folded in, valid until `valid_until_ms`.
    resolved: Arc<Vec<Act>>,
    valid_until_ms: i64,
    /// Whether `resolved` has Riot's act folded in (else it is `base`).
    from_riot: bool,
    /// With `from_riot`: the current act's id and true start, which the
    /// resolved entry may have clamped to the card clock.
    true_start: Option<(String, i64)>,
}

impl ActsState {
    fn new() -> Self {
        let empty = Arc::new(Vec::new());
        Self {
            base: empty.clone(),
            riot: None,
            resolved: empty,
            valid_until_ms: i64::MAX,
            from_riot: false,
            true_start: None,
        }
    }

    fn rebuild(&mut self, now_ms: i64) {
        let folded = self
            .riot
            .as_ref()
            .filter(|(_, usable_until)| now_ms < *usable_until)
            .and_then(|(riot, usable_until)| {
                resolve_acts(&self.base, riot, now_ms).map(|r| (r, *usable_until))
            });
        match folded {
            Some((r, usable_until)) => {
                self.valid_until_ms = r.valid_until_ms.min(usable_until);
                self.resolved = Arc::new(r.acts);
                self.from_riot = true;
                self.true_start = self
                    .resolved
                    .last()
                    .map(|a| (a.id.clone(), r.current_start_ms));
            }
            None => {
                self.valid_until_ms = i64::MAX;
                self.resolved = self.base.clone();
                self.from_riot = false;
                self.true_start = None;
            }
        }
    }
}

/// Bookkeeping for the content-service fetches.
#[derive(Default)]
struct ContentFetch {
    in_flight: bool,
    last_attempt: Option<Instant>,
    /// The act and source last logged, so each is logged once.
    logged: Option<(String, &'static str)>,
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
    acts: RwLock<ActsState>,
    content: Mutex<ContentFetch>,
    tables: tokio::sync::Mutex<TablesStatus>,
    details_synced: Mutex<HashMap<String, Instant>>,
    /// R160: players whose Details sync is waiting out a hold to run again
    /// (`commands::details_sync_job`): one waiting job per player.
    details_rerun: Mutex<HashSet<String>>,
    /// Players whose skin names are being fetched right now, with how many
    /// fetches run (R97): while a puuid is in here its Details loadout
    /// weapons are still "resolving" (absent keys); once it leaves, an
    /// unknown name is `null`.
    skins_inflight: Mutex<HashMap<String, usize>>,
    /// The Player Details the UI asked for last (`player_details`), as it
    /// was last given to the UI (the answer, or the last `details` event).
    /// A rebuild is emitted only for this player and only when it differs:
    /// the UI shows one Details at a time and drops events for any other.
    /// The worker refreshes it after syncing that player, so a hero card
    /// opened while still loading resolves (R23) without a reopen.
    open_details: Mutex<Option<PlayerDetails>>,
    /// Match ids whose match-details a task is downloading right now, each
    /// with the sender its waiters watch ([`Self::claim_details_fetch`]):
    /// the live sync, a Details sync, Match Details and the backfill can all
    /// want the same match at once, and it is downloaded once.
    details_inflight: Mutex<HashMap<String, tokio::sync::watch::Sender<()>>>,
    /// R165: match ids Riot answered 404 for (`sync::fetch_missing_details_full`
    /// records them). Read only by the waiter recheck there: a task that waited
    /// on another's download of such an id does not count it as missing.
    /// Bounded; it does not stop a later sync from asking for the id again.
    details_404: Mutex<HashSet<String>>,
    /// The Match Details the UI has open, if any (R150): replaced by the next
    /// `match_details` call, dropped when the Player Details is opened again
    /// (Back from the match). The UI has no signal for leaving the match
    /// page for Live, so it can outlive the page: a rebuild is one DB read,
    /// and it emits only on a change.
    open_match: Mutex<Option<OpenMatch>>,
    /// R163: which `player_details` / `match_details` call may open its page
    /// ([`OpenGate`]). Lock order: a gate, then its open slot.
    details_gate: Mutex<OpenGate>,
    match_gate: Mutex<OpenGate>,
    /// B2: the `"mystats"` push's state (`my_stats::notify`): the last emitted
    /// bytes and the one-task-at-a-time flags.
    pub(crate) my_stats_push: Mutex<MyStatsPush>,
    /// R150: set when names-fill asks are queued (`queue_name_fill`), taken
    /// by the poll loop (`take_worker_kick`), which owns the worker's handle:
    /// a command can't start a worker itself.
    worker_kick: AtomicBool,
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
        // R90: the user's own last-known loadout, kept in `app_state` by
        // `sync::fetch_self_loadout`, so their Details shows their skins
        // while VALORANT is closed. A copy that doesn't parse is ignored.
        //
        // It is restored once, here, into `live.self_loadout` rather than
        // read from `app_state` inside every `details::build` (Task 15
        // review F1/F4): `details::skins_pending` and `resolve_skins` are
        // synchronous over `&AppState` and need the `RawLoadout` *before*
        // the build, to mark its skins "resolving" (R97) and fetch their
        // names in the Closed stage, where nothing else supplies it. Memory
        // and `app_state` never drift: a read writes both (the stored copy
        // only on change) and an account switch clears both, so the source
        // order stays match loadout, then the user's own read, then null.
        let stored_loadout: Option<RawLoadout> = match repo.get_state(SELF_LOADOUT_KEY) {
            Ok(json) => json.and_then(|json| serde_json::from_str(&json).ok()),
            Err(e) => {
                // A DB read failure would otherwise silently hide the
                // user's Closed-stage skins; copy-log shows it.
                log::warn!("stored self loadout unreadable: {e}");
                None
            }
        };
        let mut live = LiveModel::new(self_puuid);
        live.self_loadout = stored_loadout;
        Self {
            sink,
            repo: Arc::new(Mutex::new(repo)),
            assets: Arc::new(assets),
            session: RwLock::new(None),
            live: Mutex::new(live),
            acts: RwLock::new(ActsState::new()),
            content: Mutex::new(ContentFetch::default()),
            tables: tokio::sync::Mutex::new(TablesStatus::default()),
            details_synced: Mutex::new(HashMap::new()),
            details_rerun: Mutex::new(HashSet::new()),
            skins_inflight: Mutex::new(HashMap::new()),
            open_details: Mutex::new(None),
            details_inflight: Mutex::new(HashMap::new()),
            details_404: Mutex::new(HashSet::new()),
            open_match: Mutex::new(None),
            details_gate: Mutex::new(OpenGate::default()),
            match_gate: Mutex::new(OpenGate::default()),
            my_stats_push: Mutex::new(MyStatsPush::default()),
            worker_kick: AtomicBool::new(false),
            debug_dir,
        }
    }

    /// B2: hands a freshly computed My Stats to the UI (`my_stats::notify`
    /// decides whether it changed).
    pub fn emit_my_stats(&self, stats: &MyStats) {
        self.sink.my_stats(stats);
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

    /// The acts to read "current" and "previous" from: valorant-api's table
    /// with Riot's own current act folded in (R162). The last started act is
    /// the current one, the one before it the previous one — and with
    /// nothing from Riot that is valorant-api's date rule, as before.
    pub fn acts(&self) -> Arc<Vec<Act>> {
        self.acts_at(now_ms())
    }

    pub fn acts_at(&self, now_ms: i64) -> Arc<Vec<Act>> {
        {
            let g = self.acts.read().unwrap_or_else(PoisonError::into_inner);
            if now_ms < g.valid_until_ms {
                return g.resolved.clone();
            }
        }
        let mut g = self.acts.write().unwrap_or_else(PoisonError::into_inner);
        g.rebuild(now_ms);
        g.resolved.clone()
    }

    #[cfg(test)]
    pub fn set_acts_for_test(&self, acts: Vec<Act>) {
        self.set_base_acts(acts);
    }

    /// valorant-api's act table (after the tables load).
    fn set_base_acts(&self, acts: Vec<Act>) {
        let mut g = self.acts.write().unwrap_or_else(PoisonError::into_inner);
        g.base = Arc::new(acts);
        g.rebuild(now_ms());
    }

    /// Riot's answer, fetched at `fetched_ms`.
    fn set_riot_acts(&self, riot: Option<ContentActs>, fetched_ms: i64) {
        let mut g = self.acts.write().unwrap_or_else(PoisonError::into_inner);
        g.riot = riot.map(|r| {
            let usable_until = match r.current.end_ms {
                Some(end) if end > fetched_ms => end,
                // No end known, or already past while Riot still calls the
                // act active: trust it for a short while, then ask again.
                _ => fetched_ms + CONTENT_LATE_TRUST_MS,
            };
            (r, usable_until)
        });
        g.rebuild(now_ms());
    }

    #[cfg(test)]
    pub fn set_riot_acts_for_test(&self, riot: Option<ContentActs>, fetched_ms: i64) {
        self.set_riot_acts(riot, fetched_ms);
    }

    /// The current act, if the tables are loaded.
    pub fn current_act(&self) -> Option<Act> {
        let now = now_ms();
        let acts = self.acts_at(now);
        let mut act = current_act(&acts, now).cloned()?;
        // The list may carry the hour-floored card clock as the start for the
        // first hour of an act (so cards switch at once); everything that
        // reads dates (backfill bound, My Stats, hero stats) wants the true
        // start.
        let g = self.acts.read().unwrap_or_else(PoisonError::into_inner);
        if let Some((id, start)) = &g.true_start {
            if *id == act.id && g.from_riot {
                // Never after now: Riot's clock a few seconds ahead of ours
                // must not leave the act empty for those seconds.
                act.start_ms = (*start).min(now);
            }
        }
        Some(act)
    }

    /// A failed or empty ask: an earlier answer whose act has not ended
    /// stays (a blip must not swap the act for valorant-api's dates);
    /// otherwise the date rule answers.
    fn drop_riot_answer_unless_valid(&self, now_ms: i64) {
        if !self.riot_answer_still_valid(now_ms) {
            self.set_riot_acts(None, now_ms);
        }
    }

    /// Riot's earlier answer is still good: its act has not ended.
    fn riot_answer_still_valid(&self, now_ms: i64) -> bool {
        let g = self.acts.read().unwrap_or_else(PoisonError::into_inner);
        g.riot.as_ref().is_some_and(|(r, usable_until)| {
            now_ms < *usable_until && r.current.end_ms.is_some_and(|end| end > now_ms)
        })
    }

    /// Whether Riot's content-service should be asked now: never while
    /// another ask runs; after a failure (or before any answer) every
    /// [`CONTENT_RETRY_AFTER`]; once the act it named has ended, every
    /// [`CONTENT_RECHECK_AFTER`] until it names the next one.
    pub(crate) fn content_due(&self, now_ms: i64) -> bool {
        let c = lock(&self.content);
        if c.in_flight {
            return false;
        }
        let since = c.last_attempt.map(|t| t.elapsed());
        let g = self.acts.read().unwrap_or_else(PoisonError::into_inner);
        match &g.riot {
            Some((_, usable_until)) if now_ms < *usable_until => false,
            Some(_) => since.is_none_or(|s| s >= CONTENT_RECHECK_AFTER),
            None => since.is_none_or(|s| s >= CONTENT_RETRY_AFTER),
        }
    }

    /// Asks Riot for the current act in the background (R162): `force` on a
    /// connect, otherwise only when [`content_due`](Self::content_due).
    pub fn spawn_content_refresh(self: &Arc<Self>, force: bool) {
        if self.session().is_none() {
            return;
        }
        if !force && !self.content_due(now_ms()) {
            return;
        }
        let st = self.clone();
        tauri::async_runtime::spawn(async move { st.refresh_content().await });
    }

    /// One content-service ask and what follows: on an answer, Riot's act
    /// becomes the current one; on any failure valorant-api's date rule
    /// stays (one log line says why). Republishes when the current act
    /// changed.
    pub async fn refresh_content(self: &Arc<Self>) {
        let Some(session) = self.session() else {
            return;
        };
        {
            let mut c = lock(&self.content);
            if c.in_flight {
                return;
            }
            c.in_flight = true;
            c.last_attempt = Some(Instant::now());
        }
        let result = session.content().await;
        let now = now_ms();
        let before = self.current_act().map(|a| a.id);
        let why = match result {
            Ok(seasons) => match seasons.active_acts() {
                Some(acts) => {
                    self.set_riot_acts(Some(acts), now);
                    None
                }
                None => {
                    self.drop_riot_answer_unless_valid(now);
                    Some("no active act in the answer".to_string())
                }
            },
            Err(e) => {
                self.drop_riot_answer_unless_valid(now);
                Some(e.to_string())
            }
        };
        let current = self.current_act();
        // Riot's act may have no usable times; then valorant-api's rule
        // stands in even after an answer, and the log says so.
        let source = if self
            .acts
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .from_riot
        {
            "riot content-service"
        } else {
            "valorant-api"
        };
        let label = current.as_ref().map_or_else(
            || "none".to_string(),
            |a| format!("{} ({})", opd1_core::stats::act_full_name(a), a.id),
        );
        {
            let mut c = lock(&self.content);
            c.in_flight = false;
            let key = (
                current.as_ref().map(|a| a.id.clone()).unwrap_or_default(),
                source,
            );
            match (&why, c.logged.as_ref() != Some(&key)) {
                (Some(why), _) => {
                    log::warn!("content-service: {why}; current act {label} from {source}");
                }
                (None, true) => log::info!("current act {label} from {source}"),
                (None, false) => {}
            }
            c.logged = Some(key);
        }
        if current.map(|a| a.id) != before {
            // The live cards, an open Player Details and My Stats (B2) read
            // the act.
            self.publish();
            crate::my_stats::notify(self);
            crate::details::refresh_open(self).await;
        }
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
        // A copy of the previous version's tables stands in (the new
        // download failed): ask again, at most every `TABLES_RETRY_AFTER`.
        let stale = self.assets.tables_stale();
        if status.loaded && !recheck && !stale {
            return Ok(());
        }
        if !status.loaded || (stale && !recheck) {
            if let Some(t) = status.last_failure {
                if t.elapsed() < TABLES_RETRY_AFTER {
                    return if status.loaded {
                        Ok(())
                    } else {
                        Err("asset tables unavailable".into())
                    };
                }
            }
        }
        let first = !status.loaded;
        match self.assets.refresh_tables().await {
            Ok(refetched) => {
                status.loaded = true;
                status.last_failure = self.assets.tables_stale().then(Instant::now);
                let acts = self.assets.acts().unwrap_or_default();
                let still_stale = self.assets.tables_stale();
                // Once per stale period: a retry that fails the same way
                // every 30 s says nothing new.
                if !(stale && still_stale) {
                    log::info!(
                        "asset tables {} ({} acts)",
                        if refetched {
                            "downloaded"
                        } else {
                            "loaded from cache"
                        },
                        acts.len()
                    );
                }
                self.set_base_acts(acts);
                // R163: the UI's names and colours follow the tables: sent
                // when they first load and whenever they were downloaded
                // again (a new patch), not on a plain version re-check.
                if first || refetched {
                    match self.assets.meta() {
                        Ok(meta) => self.sink.meta(&meta),
                        Err(e) => log::warn!("meta event: {e}"),
                    }
                }
                if stale && !still_stale {
                    // Healed: the cards were built from the old tables.
                    self.publish();
                }
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
        live.observe_match_end();
        if live.map_id.is_none() {
            if let Some(path) = live.map_path.clone() {
                live.map_id = self.map_uuid(&path);
            }
        }
        let state = live.build(&acts, card_clock(now_ms()), &|p| self.map_uuid(p));
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

    /// A `player_details` call starts opening `puuid`'s page: the ticket it
    /// must hand to [`Self::set_open_details`] once its build is done. Any
    /// earlier ticket is void from now on (R163).
    pub fn begin_open_details(&self, puuid: &str) -> u64 {
        lock(&self.details_gate).begin(puuid)
    }

    /// Records `details` as the open Player Details, as the UI was just
    /// given them (the `player_details` answer), when `ticket` is still the
    /// latest open and its page was not left meanwhile (R163: a build that
    /// finishes after Back, or after a newer open, must not set the open
    /// page); `false` when it lost. Opening a Player Details also closes any
    /// Match Details: the UI only gets here from the match page by going
    /// Back.
    pub fn set_open_details(&self, ticket: u64, details: &PlayerDetails) -> bool {
        let gate = lock(&self.details_gate);
        if gate.seq != ticket {
            return false;
        }
        *lock(&self.open_details) = Some(details.clone());
        *lock(&self.open_match) = None;
        true
    }

    /// A `match_details` call starts opening `match_id`'s page (like
    /// [`Self::begin_open_details`]).
    pub fn begin_open_match(&self, match_id: &str) -> u64 {
        lock(&self.match_gate).begin(match_id)
    }

    /// Records `view` as the open Match Details, as the UI was just given it
    /// (the `match_details` answer), when `ticket` is still the latest open
    /// of its page; `false` when it lost (R163).
    pub fn set_open_match(
        &self,
        ticket: u64,
        match_id: &str,
        focus_puuid: &str,
        view: &MatchDetailsView,
    ) -> bool {
        let gate = lock(&self.match_gate);
        if gate.seq != ticket {
            return false;
        }
        *lock(&self.open_match) = Some(OpenMatch {
            match_id: match_id.to_string(),
            focus_puuid: focus_puuid.to_string(),
            view: view.clone(),
        });
        true
    }

    /// The UI left the Match Details page (`close_match`): nothing is open,
    /// so no `match` event is built or emitted until the next
    /// `match_details` call, and an open still being built is void. With a
    /// `match_id`, a close for any other match than the one open or being
    /// opened is ignored (R163: a late close of a page already replaced).
    /// A no-op with nothing open.
    pub fn close_match(&self, match_id: Option<&str>) {
        let mut gate = lock(&self.match_gate);
        if !gate.close(match_id) {
            return;
        }
        *lock(&self.open_match) = None;
    }

    /// `(match id, focus puuid)` of the open Match Details, if any.
    pub fn open_match_args(&self) -> Option<(String, String)> {
        lock(&self.open_match)
            .as_ref()
            .map(|o| (o.match_id.clone(), o.focus_puuid.clone()))
    }

    /// Emits `fresh` as a `"match"` event when it is the open Match Details
    /// (same match and focus) and differs from what the UI was last given;
    /// `true` when it was emitted. Compare, emit and record happen under one
    /// lock, like [`Self::emit_details_if_changed`].
    pub fn emit_match_if_changed(
        &self,
        match_id: &str,
        focus_puuid: &str,
        fresh: MatchDetailsView,
    ) -> bool {
        let mut open = lock(&self.open_match);
        match open.as_mut() {
            Some(o)
                if o.match_id == match_id && o.focus_puuid == focus_puuid && o.view != fresh =>
            {
                self.sink.match_view(&fresh);
                o.view = fresh;
                true
            }
            _ => false,
        }
    }

    /// R150: queues names-fill asks (`LiveModel::queue_name_fill`) and wakes
    /// the poll loop, which starts a worker for them (`take_worker_kick`).
    /// The asks run between matches, behind the live cards.
    pub fn queue_name_fill(&self, asks: Vec<NameAsk>) {
        if asks.is_empty() {
            return;
        }
        lock(&self.live).queue_name_fill(asks);
        self.worker_kick.store(true, Ordering::Release);
    }

    /// R167: [`Self::queue_name_fill`] at low priority
    /// (`LiveModel::queue_name_fill_low`).
    pub fn queue_name_fill_low(&self, asks: Vec<NameAsk>) {
        if asks.is_empty() {
            return;
        }
        lock(&self.live).queue_name_fill_low(asks);
        self.worker_kick.store(true, Ordering::Release);
    }

    /// R165: Riot no longer has this match (404): remembered for the session.
    pub fn note_details_404(&self, match_id: &str) {
        let mut set = lock(&self.details_404);
        if set.len() >= 512 {
            set.clear();
        }
        set.insert(match_id.to_string());
    }

    /// R165: this match answered 404 earlier ([`Self::note_details_404`]).
    pub fn details_404(&self, match_id: &str) -> bool {
        lock(&self.details_404).contains(match_id)
    }

    /// `true` once after each [`Self::queue_name_fill`]: the poll loop calls
    /// it every tick and, when it is set, makes sure a worker runs.
    pub fn take_worker_kick(&self) -> bool {
        self.worker_kick.swap(false, Ordering::AcqRel)
    }

    /// R150: the asks a just-stored match raises — every other player of the
    /// match that just ended whose name match-details left blank, each tagged
    /// with the match's start — or nothing.
    /// A match is "the one that just ended" when a match end is on record
    /// (`LiveModel::match_end_ms`), the user played in it, and it started
    /// within [`MATCH_END_WINDOW_MS`] before that end (not after it: that is
    /// the next match). Who may actually be asked is decided later, when the
    /// fill runs (`LiveModel::take_fill`).
    pub fn match_end_asks(&self, m: &Match, players: &[MatchPlayer]) -> Vec<NameAsk> {
        let mut live = lock(&self.live);
        live.observe_match_end();
        // R165: the last end, which a new match does not clear.
        let (Some(me), Some(end)) = (live.self_puuid.as_deref(), live.last_match_end_ms()) else {
            return Vec::new();
        };
        if m.start_ms > end
            || m.start_ms < end - MATCH_END_WINDOW_MS
            || !players.iter().any(|p| p.puuid == me)
        {
            return Vec::new();
        }
        unnamed_others(players, me)
            .map(|puuid| NameAsk {
                puuid,
                match_start_ms: Some(m.start_ms),
            })
            .collect()
    }

    /// `puuid`'s Player Details is the one the UI asked for last.
    pub fn details_open_for(&self, puuid: &str) -> bool {
        lock(&self.open_details)
            .as_ref()
            .is_some_and(|d| d.card.puuid == puuid)
    }

    /// `puuid`'s Player Details as the UI was last given them, when that is
    /// the page open now.
    pub fn open_details_of(&self, puuid: &str) -> Option<PlayerDetails> {
        lock(&self.open_details)
            .as_ref()
            .filter(|d| d.card.puuid == puuid)
            .cloned()
    }

    /// The player whose Details the UI has open, if any.
    pub fn open_details_puuid(&self) -> Option<String> {
        lock(&self.open_details)
            .as_ref()
            .map(|d| d.card.puuid.clone())
    }

    /// The UI left the Details page (`close_details`): nothing is open, so
    /// no rebuild is done or emitted for it until the next open, and an open
    /// still being built is void. With a `puuid`, a close for any other
    /// player than the one open or being opened is ignored (R163).
    pub fn close_details(&self, puuid: Option<&str>) {
        let mut gate = lock(&self.details_gate);
        if !gate.close(puuid) {
            return;
        }
        *lock(&self.open_details) = None;
    }

    /// Emits `fresh` as a `"details"` event when it is the open Player
    /// Details and differs from what the UI was last given; `true` when it
    /// was emitted. Compare, emit and record happen under one lock, so two
    /// rebuilds can't both emit the same change.
    pub fn emit_details_if_changed(&self, fresh: PlayerDetails) -> bool {
        let mut open = lock(&self.open_details);
        match open.as_ref() {
            Some(last) if last.card.puuid == fresh.card.puuid && *last != fresh => {
                self.sink.details(&fresh);
                *open = Some(fresh);
                true
            }
            _ => false,
        }
    }

    pub fn live_state(&self) -> LiveState {
        lock(&self.live).current.clone()
    }

    /// Replaces the DB bundle of `puuid`'s live slot (if it's on screen)
    /// with a fresh read, then republishes.
    pub async fn reload_slot(&self, puuid: &str) {
        let p = puuid.to_string();
        let me = lock(&self.live).self_puuid.clone();
        let Some(db) = self
            .db(move |repo| crate::store::load_player_db(repo, me.as_deref(), &p))
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

    /// R167: [`Self::reload_slot`] for several players at once: the DB bundle
    /// of each of `puuids` that is on screen is read afresh (one DB call),
    /// and the cards are published **once**. Nothing is read or published
    /// when none of them is on screen.
    pub async fn reload_slots(&self, puuids: &[String]) {
        let (on_screen, me) = {
            let live = lock(&self.live);
            let on_screen: Vec<String> = puuids
                .iter()
                .filter(|p| live.slots.iter().any(|s| &s.roster.puuid == *p))
                .cloned()
                .collect();
            (on_screen, live.self_puuid.clone())
        };
        if on_screen.is_empty() {
            return;
        }
        let ids = on_screen.clone();
        let Some(bundles) = self
            .db(move |repo| {
                ids.iter()
                    .map(|p| {
                        Ok((
                            p.clone(),
                            crate::store::load_player_db(repo, me.as_deref(), p)?,
                        ))
                    })
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .await
        else {
            return;
        };
        {
            let mut live = lock(&self.live);
            for (puuid, db) in bundles {
                if let Some(slot) = live.slots.iter_mut().find(|s| s.roster.puuid == puuid) {
                    slot.db = db;
                }
            }
        }
        self.publish();
    }

    /// R97: `true` only while a skin-name fetch for `puuid` runs (between
    /// [`Self::begin_skin_resolve`] and [`Self::end_skin_resolve`]). A
    /// Details loadout weapon whose name isn't known is "resolving" (its
    /// key absent) exactly then, so a skeleton never outlives the attempt.
    pub fn skins_resolving(&self, puuid: &str) -> bool {
        lock(&self.skins_inflight).contains_key(puuid)
    }

    /// Marks a skin-name fetch for `puuid` as running (see
    /// [`Self::skins_resolving`]) until the returned guard is dropped. The
    /// guard's `Drop` ends the mark, so a panic in the fetch can never leave
    /// a puuid resolving for the rest of the session (R97). Drop it *before*
    /// rebuilding the details that should show the result.
    #[must_use = "the mark ends when the guard is dropped"]
    pub fn begin_skin_resolve(self: &Arc<Self>, puuid: &str) -> SkinResolveGuard {
        *lock(&self.skins_inflight)
            .entry(puuid.to_string())
            .or_default() += 1;
        SkinResolveGuard {
            st: self.clone(),
            puuid: puuid.to_string(),
        }
    }

    fn end_skin_resolve(&self, puuid: &str) {
        let mut inflight = lock(&self.skins_inflight);
        if let Some(n) = inflight.get_mut(puuid) {
            *n -= 1;
            if *n == 0 {
                inflight.remove(puuid);
            }
        }
    }

    /// Claims the download of `match_id`'s match-details: `Mine` when no
    /// other task is downloading it (the guard releases the claim when
    /// dropped — after the rows are stored, on a failure, or when the task
    /// is aborted), else `Busy` with a handle to [`wait_for_details_fetch`]
    /// on. Keeps one download per match however many tasks want it.
    pub fn claim_details_fetch(self: &Arc<Self>, match_id: &str) -> DetailsClaim {
        let mut inflight = lock(&self.details_inflight);
        if let Some(tx) = inflight.get(match_id) {
            return DetailsClaim::Busy(tx.subscribe());
        }
        inflight.insert(match_id.to_string(), tokio::sync::watch::channel(()).0);
        DetailsClaim::Mine(DetailsFetchGuard {
            st: self.clone(),
            match_id: match_id.to_string(),
        })
    }

    /// R160: marks `puuid`'s Details sync as waiting out a hold to run again,
    /// unless another job of that player already waits (`None`): a page
    /// reopened during the wait does not leave two reruns. The mark ends when
    /// the guard is dropped, on every path.
    pub fn begin_details_rerun(self: &Arc<Self>, puuid: &str) -> Option<DetailsRerunGuard> {
        lock(&self.details_rerun)
            .insert(puuid.to_string())
            .then(|| DetailsRerunGuard {
                st: self.clone(),
                puuid: puuid.to_string(),
            })
    }

    /// Whether `puuid`'s Details sync is waiting out a hold to run again
    /// ([`Self::begin_details_rerun`]); tests wait for it instead of a clock.
    #[cfg(test)]
    pub fn details_rerun_waiting(&self, puuid: &str) -> bool {
        lock(&self.details_rerun).contains(puuid)
    }

    /// Gives back a [`Self::claim_details_sync`]: the sync did not get its data
    /// (R160: refused by a hold, or the competitive-updates call failed), so
    /// the next open or refresh may run it again instead of waiting out the
    /// 120 s.
    pub fn release_details_sync(&self, puuid: &str) {
        lock(&self.details_synced).remove(puuid);
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

/// Which open of one page (Player Details, or Match Details) is the latest
/// (R163). Every `player_details` / `match_details` call takes a ticket when
/// it starts ([`OpenGate::begin`]) and may set the open page only while its
/// ticket is still `seq`; a close for the page voids every ticket taken so
/// far. Held across the check and the write, so a close can never fall in
/// between.
#[derive(Default)]
struct OpenGate {
    /// The latest ticket; bumped by every `begin` and every effective close.
    seq: u64,
    /// What the latest `begin` is opening (a puuid, a match id); `None` once
    /// closed.
    key: Option<String>,
}

impl OpenGate {
    fn begin(&mut self, key: &str) -> u64 {
        self.seq += 1;
        self.key = Some(key.to_string());
        self.seq
    }

    /// `false` when `key` is given and is not what is open or being opened
    /// (nothing changes); otherwise voids every ticket and returns `true`.
    fn close(&mut self, key: Option<&str>) -> bool {
        if key.is_some_and(|k| self.key.as_deref() != Some(k)) {
            return false;
        }
        self.seq += 1;
        self.key = None;
        true
    }
}

/// The Match Details the UI asked for last (`match_details`), as it was last
/// given to the UI (R150): rebuilt and re-emitted when names arrive.
struct OpenMatch {
    match_id: String,
    focus_puuid: String,
    view: MatchDetailsView,
}

/// A player's waiting Details rerun ([`AppState::begin_details_rerun`]);
/// dropping it ends the mark.
pub struct DetailsRerunGuard {
    st: Arc<AppState>,
    puuid: String,
}

impl Drop for DetailsRerunGuard {
    fn drop(&mut self) {
        lock(&self.st.details_rerun).remove(&self.puuid);
    }
}

/// See [`AppState::claim_details_fetch`].
pub enum DetailsClaim {
    /// This task downloads it; drop the guard once the rows are stored.
    Mine(DetailsFetchGuard),
    /// Another task is downloading it.
    Busy(tokio::sync::watch::Receiver<()>),
}

/// A claimed match-details download ([`AppState::claim_details_fetch`]);
/// dropping it releases the claim and wakes every task waiting on it.
pub struct DetailsFetchGuard {
    st: Arc<AppState>,
    match_id: String,
}

impl Drop for DetailsFetchGuard {
    fn drop(&mut self) {
        // Dropping the sender closes the channel: the waiters wake.
        lock(&self.st.details_inflight).remove(&self.match_id);
    }
}

/// Waits (at most [`DETAILS_WAIT_MAX`]) until the task that claimed a
/// match-details download has finished it, stored or not.
pub async fn wait_for_details_fetch(mut busy: tokio::sync::watch::Receiver<()>) {
    // Nothing is ever sent: `changed` returns once the sender is dropped.
    let _ = tokio::time::timeout(DETAILS_WAIT_MAX, busy.changed()).await;
}

/// A running skin-name fetch's mark on its puuid
/// ([`AppState::begin_skin_resolve`]); dropping it ends the mark.
pub struct SkinResolveGuard {
    st: Arc<AppState>,
    puuid: String,
}

impl Drop for SkinResolveGuard {
    fn drop(&mut self) {
        self.st.end_skin_resolve(&self.puuid);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opd1_core::db::repo::{CompUpdateRow, MatchPlayerRow, NameAsk, StoredTier};
    use opd1_core::model::{DisplayName, EncounterCount, PartyMark, RememberedSource};
    use opd1_core::names::KnownName;
    use opd1_core::party::SharedRef;
    use opd1_riot::endpoints::PlayerMmrResponse;
    use serde_json::{json, Value};

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
            live_tier: None,
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
        let card = &m.build(&[], 0, &|_| None).players[0];
        assert!(card.rank.is_loading());
        assert!(card.last5.is_loading());
        assert!(card.name.is_loading());

        // The sync ran but every call failed / returned nothing.
        m.synced.insert("p1".into());
        m.names_attempted.insert("p1".into());
        let card = &m.build(&[], 0, &|_| None).players[0];
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
        let card = &m.build(&[], 0, &|_| None).players[0];
        assert!(matches!(card.last5, Field::Value(ref v) if v.len() == 1));
        assert!(matches!(card.kd, Field::Value(_)));
        assert!(card.rank.is_loading(), "no MMR snapshot yet and not synced");
    }

    #[test]
    fn closed_stage_never_shows_skeletons() {
        let mut m = model(vec![slot("me", false)]);
        m.stage = Stage::Closed;
        let json = serde_json::to_value(&m.build(&[], 0, &|_| None).players[0]).unwrap();
        for key in [
            "name",
            "rank",
            "prevActTier",
            "peak",
            "kd",
            "hsPct",
            "actWinrate",
            "actWins",
            "actGames",
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
        let card = &m.build(&[], 0, &|_| None).players[0];
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
        assert_eq!(
            m.hidden_players(),
            HashMap::from([("h1".to_string(), Some("agent".to_string()))])
        );
    }

    /// The src-tauri half of CLAUDE.md "Hidden names": an incognito
    /// player's real name may be sitting in the `players` row (seen while
    /// visible) or in this run's name-service answers, yet their card only
    /// ever shows HiddenPlayer / HiddenAgent — or, when the DB knows the
    /// name from some source (R145, `Repo::known_name` fills
    /// `db.remembered`), the remembered name; the slot's own row and this
    /// run's answers are never read for it by the card.
    #[test]
    fn incognito_card_never_carries_a_visible_name_unless_remembered() {
        let mut s = slot("h1", true);
        s.roster.selection_state = String::new();
        s.db.row = Some(StoredPlayer {
            game_name: "Real".into(),
            tag_line: "Name".into(),
            account_level: 1,
            card_id: "c".into(),
        });
        let mut m = model(vec![s]);
        m.known_names
            .insert("h1".into(), ("Real".into(), "Name".into()));
        m.synced.insert("h1".into());
        let card = |m: &LiveModel| m.build(&[], 0, &|_| None).players[0].clone();
        let leaks = |m: &LiveModel| serde_json::to_string(&card(m)).unwrap().contains("Real");

        // Before agent select: Hidden Player.
        assert_eq!(card(&m).name, Field::Value(DisplayName::HiddenPlayer));
        assert!(!leaks(&m));
        // After: the agent, no tag.
        m.slots[0].roster.selection_state = "locked".into();
        assert_eq!(
            card(&m).name,
            Field::Value(DisplayName::HiddenAgent {
                agent_id: "agent".into()
            })
        );
        assert!(!leaks(&m));
        // Remembered (R145: the DB knows the name from some source): that
        // name, in its own style. The card never reads the slot's `players`
        // row or this run's name-service answers by itself for an incognito
        // player — only what `Repo::known_name` handed over.
        m.slots[0].db.remembered = Some(KnownName {
            name: "Seen".into(),
            tag: "AFTER".into(),
            map_id: None,
            start_ms: None,
            source: RememberedSource::OwnMatch,
        });
        assert_eq!(
            card(&m).name,
            Field::Value(DisplayName::Remembered {
                name: "Seen".into(),
                tag: "AFTER".into(),
                source: RememberedSource::OwnMatch,
                from_map_id: None,
                from_start_ms: None,
            })
        );
        assert!(!leaks(&m));
        assert!(m.names_needed().is_empty(), "never sent to name-service");

        // A visible player never shows the remembered style.
        m.slots[0].roster.incognito = false;
        assert_eq!(
            card(&m).name,
            Field::Value(DisplayName::Visible {
                name: "Real".into(),
                tag: "Name".into()
            })
        );
    }

    #[test]
    fn card_for_shows_a_seen_name_of_a_hidden_player_in_the_remembered_style() {
        // R145: a name-service answer from a time the player was visible
        // (`Repo::known_name`'s `Seen` source) is shown while they are
        // hidden now: remembered style, no map, never the visible style.
        let mut s = slot("h1", true);
        s.db.remembered = Some(KnownName {
            name: "Vis".into(),
            tag: "IBLE".into(),
            map_id: None,
            start_ms: Some(700),
            source: RememberedSource::Seen,
        });
        let m = model(vec![s]);
        let json = serde_json::to_value(&m.build(&[], 0, &|_| None).players[0]).unwrap();
        assert_eq!(
            json["name"],
            json!({
                "kind": "remembered", "name": "Vis", "tag": "IBLE", "source": "seen",
                "fromMapId": null, "fromStartMs": 700
            })
        );
    }

    #[test]
    fn card_for_maps_remembered_source() {
        let mut s = slot("h1", true);
        s.db.remembered = Some(KnownName {
            name: "Seen".into(),
            tag: "AFTER".into(),
            // The stored Riot path (R84): converted only when the card is built.
            map_id: Some("/Game/Maps/Bonsai/Bonsai".into()),
            start_ms: Some(1_234),
            source: RememberedSource::OwnMatch,
        });
        let m = model(vec![s]);
        let card_json = |lookup: &dyn Fn(&str) -> Option<String>| {
            serde_json::to_value(&m.build(&[], 0, lookup).players[0]).unwrap()
        };

        let known =
            card_json(&|p| (p == "/Game/Maps/Bonsai/Bonsai").then(|| "uuid-split".to_string()));
        assert_eq!(known["name"]["kind"], "remembered");
        assert_eq!(known["name"]["fromMapId"], "uuid-split");
        assert_eq!(known["name"]["fromStartMs"], 1_234);
        assert!(!known.to_string().contains("/Game/"));

        // A map the tables don't know: no source map, and never the path.
        let unknown = card_json(&|_| None);
        assert_eq!(unknown["name"]["fromMapId"], Value::Null);
        assert_eq!(unknown["name"]["fromStartMs"], 1_234);
        assert!(!unknown.to_string().contains("/Game/"));
    }

    #[test]
    fn hidden_players_ignore_hover() {
        // R99: an incognito player who only hovers an agent has no agent
        // yet, so Top Teammates shows them as Hidden Player, like the card.
        let mut hover = slot("h1", true);
        hover.roster.selection_state = "selected".into();
        let mut locked = slot("h2", true);
        locked.roster.selection_state = "locked".into();
        let m = model(vec![hover, locked]);
        assert_eq!(
            m.hidden_players(),
            HashMap::from([
                ("h1".to_string(), None),
                ("h2".to_string(), Some("agent".to_string())),
            ])
        );
        let cards = m.build(&[], 0, &|_| None).players;
        assert_eq!(cards[0].agent_id, None);
        assert_eq!(cards[0].name, Field::Value(DisplayName::HiddenPlayer));
        assert_eq!(cards[1].agent_id.as_deref(), Some("agent"));
    }

    #[test]
    fn own_card_shows_level_even_when_hidden() {
        // R100: `HideAccountLevel` hides the level for everyone but the user.
        let hidden_level = |puuid: &str, level: u32| {
            let mut s = slot(puuid, false);
            s.roster.hide_account_level = true;
            s.roster.account_level = level;
            s
        };
        let m = model(vec![
            hidden_level("me", 87),
            hidden_level("p1", 87),
            hidden_level("me", 0),
        ]);
        let cards = m.build(&[], 0, &|_| None).players;
        assert_eq!(cards[0].level, Field::Value(87));
        assert!(cards[0].is_self);
        assert_eq!(cards[1].level, Field::Missing);
        assert!(!cards[1].is_self);
        // An unknown level (0) is `—` for everyone, the user included.
        assert_eq!(cards[2].level, Field::Missing);
    }

    #[test]
    fn encounters_show_for_others_but_never_for_the_user_or_their_party() {
        let enc = Some(EncounterCount {
            with: 1,
            against: 2,
        });
        let mut me = slot("me", false);
        me.roster.is_self_party = true;
        me.db.encounters = enc;
        let mut duo = slot("duo", false);
        duo.roster.is_self_party = true;
        duo.db.encounters = enc;
        let mut known = slot("known", false);
        known.db.encounters = enc;
        let stranger = slot("stranger", false);
        let cards = model(vec![me, duo, known, stranger])
            .build(&[], 0, &|_| None)
            .players;
        let got: Vec<Option<EncounterCount>> = cards.iter().map(|c| c.encounters).collect();
        assert_eq!(got, vec![None, None, enc, None]);
    }

    fn last_match(match_id: &str, team: &str) -> [Option<SharedRef>; 2] {
        [
            Some(SharedRef {
                match_id: match_id.into(),
                team_id: Some(team.into()),
                party_id: None,
            }),
            None,
        ]
    }

    #[test]
    fn party_marks_come_from_players_with_a_synced_history_only_in_agent_select_and_in_game() {
        let mut slots = Vec::new();
        for (p, self_party, team) in [
            ("me", true, Team::Ally),
            ("duo", true, Team::Ally),
            ("e1", false, Team::Enemy),
            ("e2", false, Team::Enemy),
            ("e3", false, Team::Enemy),
        ] {
            let mut s = slot(p, false);
            s.roster.is_self_party = self_party;
            s.roster.team = team;
            // Everyone's last match was the same one, on the same team.
            s.db.recent = last_match("m1", "Red");
            slots.push(s);
        }
        let mut m = model(slots);
        let marks = |m: &LiveModel| -> Vec<Option<PartyMark>> {
            m.build(&[], 0, &|_| None)
                .players
                .iter()
                .map(|c| c.party)
                .collect()
        };
        let likely = Some(PartyMark {
            group: 0,
            confirmed: false,
        });

        // Cached history isn't trusted before this cycle's sync (R53).
        assert_eq!(marks(&m), vec![None; 5]);
        for p in ["me", "duo", "e1", "e2"] {
            m.synced.insert(p.into());
            m.party_ready.insert(p.into());
        }
        // e3 not synced yet: no link yet. The user's party: never.
        assert_eq!(marks(&m), vec![None, None, likely, likely, None]);
        // Synced, but its history call failed (not `party_ready`): still no
        // link, whatever the stored history says.
        m.synced.insert("e3".into());
        assert_eq!(marks(&m), vec![None, None, likely, likely, None]);
        m.party_ready.insert("e3".into());
        assert_eq!(marks(&m), vec![None, None, likely, likely, likely]);

        m.stage = Stage::Pregame;
        assert_eq!(marks(&m), vec![None, None, likely, likely, likely]);
        // Lobby / Closed show only the user's party: no stripes at all.
        for stage in [Stage::Menus, Stage::Closed] {
            m.stage = stage;
            assert_eq!(marks(&m), vec![None; 5], "{stage:?}");
        }
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
        assert!(m.build(&[], 0, &|_| None).players[0].name.is_loading());
        m.self_name_attempted = true;
        assert_eq!(m.build(&[], 0, &|_| None).players[0].name, Field::Missing);
        // …then the local chat session's Riot ID, shown to the user even
        // with their own streamer-mode flag on.
        m.self_name = Some(("Shanky".into(), "OPD1".into()));
        assert_eq!(
            m.build(&[], 0, &|_| None).players[0].name,
            Field::Value(DisplayName::Visible {
                name: "Shanky".into(),
                tag: "OPD1".into()
            })
        );
        assert!(m.hidden_players().is_empty());
        // A stage reset (which clears names_attempted) keeps it settled.
        m.names_attempted.clear();
        m.self_name = None;
        assert_eq!(m.build(&[], 0, &|_| None).players[0].name, Field::Missing);
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
        let name = |m: &LiveModel| m.build(&[], 0, &|_| None).players[0].name.clone();
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
        assert_eq!(m.next_job(id), Some(Job::Rank(vec!["me".into()])));
        m.synced.mark_ranked("me");
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
        assert_eq!(
            m.next_job(id2),
            Some(Job::Rank(vec!["late".into(), "later".into()]))
        );
        m.synced.mark_ranked("late");
        m.synced.mark_ranked("later");
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
    fn backfill_job_is_lowest_priority() {
        let mut m = model(vec![slot("me", false), slot("p1", false)]);
        m.stage = Stage::Menus;
        m.start_backfill(None, None);
        let id = m.try_start_worker().expect("work pending");

        // A name is needed and slots are unsynced: both come before the run.
        assert!(m.live_job_pending());
        assert!(matches!(m.next_job(id), Some(Job::Names(_))));
        m.names_attempted.extend(["p1".to_string()]);
        assert_eq!(
            m.next_job(id),
            Some(Job::Rank(vec!["me".into(), "p1".into()]))
        );
        m.synced.mark_ranked("me");
        m.synced.mark_ranked("p1");
        assert_eq!(m.next_job(id), Some(Job::Sync("me".into())));
        m.synced.insert("me".into());
        assert_eq!(m.next_job(id), Some(Job::Sync("p1".into())));
        m.synced.insert("p1".into());

        // Both empty: only now the backfill.
        assert!(!m.live_job_pending());
        assert_eq!(m.next_job(id), Some(Job::Backfill));
        assert_eq!(m.next_job(id), Some(Job::Backfill), "until the run ends");
        // A roster player added mid-run goes first again.
        m.slots.push(slot("late", false));
        m.names_attempted.insert("late".into());
        assert!(m.live_job_pending());
        assert_eq!(m.next_job(id), Some(Job::Rank(vec!["late".into()])));
        m.synced.mark_ranked("late");
        assert_eq!(m.next_job(id), Some(Job::Sync("late".into())));
        m.synced.insert("late".into());
        assert_eq!(m.next_job(id), Some(Job::Backfill));

        // The run ended: nothing left, and the claim is released with it.
        m.backfill = None;
        assert_eq!(m.next_job(id), None);
        assert!(!m.worker_running(), "claim released");
    }

    // -- R155: bounded rank retries --------------------------------------

    const S: Duration = Duration::from_secs(1);

    /// A Lobby whose players are all named and synced: nothing to do.
    fn settled_lobby(puuids: &[&str]) -> LiveModel {
        let mut m = model(puuids.iter().map(|p| slot(p, false)).collect());
        m.stage = Stage::Menus;
        for p in puuids {
            m.synced.insert(p.to_string());
            m.names_attempted.insert(p.to_string());
        }
        m
    }

    #[test]
    fn a_failed_rank_sync_is_retried_20s_then_60s_later_and_no_more() {
        let mut m = settled_lobby(&["me"]);
        m.note_rank_sync("me", true, false);
        // Waiting is not work: no worker, no due retry.
        assert!(!m.has_pending_work());
        assert!(m.try_start_worker().is_none());
        m.advance_clock(19 * S);
        assert_eq!(m.due_rank_retry(), None, "not before 20 s");
        assert!(m.try_start_worker().is_none());
        m.advance_clock(S);
        assert_eq!(m.due_rank_retry().as_deref(), Some("me"));
        let id = m.try_start_worker().expect("a due retry is work");
        assert_eq!(m.next_job(id), Some(Job::RankRetry("me".into())));

        // The first retry fails too: the second is 60 s after it.
        m.note_rank_sync("me", true, true);
        assert_eq!(m.next_job(id), None, "the worker does not wait for it");
        assert!(!m.worker_running());
        m.advance_clock(59 * S);
        assert!(m.try_start_worker().is_none());
        m.advance_clock(S);
        let id = m.try_start_worker().expect("the second retry is due");
        assert_eq!(m.next_job(id), Some(Job::RankRetry("me".into())));

        // The second fails as well: that was the last.
        m.note_rank_sync("me", true, true);
        assert_eq!(m.next_job(id), None);
        m.advance_clock(100_000 * S);
        assert!(m.try_start_worker().is_none());
        assert_eq!(m.due_rank_retry(), None);
    }

    #[test]
    fn a_rank_sync_held_by_the_mmr_hold_waits_for_it_and_uses_no_retry() {
        // R160: nothing was sent, so nothing is spent: the sync is due 1.5 s
        // after the hold ends, and both R155 retries are still there.
        let mut m = settled_lobby(&["me"]);
        m.note_rank_held("me", 40 * S, false);
        assert!(!m.has_pending_work(), "waiting is not work");
        m.advance_clock(41 * S);
        assert_eq!(m.due_rank_retry(), None, "1.5 s of slack after the hold");
        m.advance_clock(S);
        let id = m.try_start_worker().expect("due");
        assert_eq!(m.next_job(id), Some(Job::RankRetry("me".into())));
        assert_eq!(m.rank_retries.retries_done("me"), Some(1));

        // It met the hold again (somebody's 429 extended it): given back.
        m.note_rank_held("me", 10 * S, true);
        assert_eq!(m.rank_retries.retries_done("me"), Some(0));
        assert_eq!(m.next_job(id), None, "the worker does not wait");
        m.advance_clock(12 * S);
        let id = m.try_start_worker().expect("due again");
        assert_eq!(m.next_job(id), Some(Job::RankRetry("me".into())));

        // A real failure now moves the usual schedule on: 60 s, then the last.
        m.note_rank_sync("me", true, true);
        assert_eq!(m.rank_retries.retries_done("me"), Some(1));
        assert_eq!(m.next_job(id), None);
        m.advance_clock(60 * S);
        let id = m.try_start_worker().expect("the second retry");
        assert_eq!(m.next_job(id), Some(Job::RankRetry("me".into())));
        m.note_rank_sync("me", true, true);
        m.advance_clock(100_000 * S);
        assert!(m.try_start_worker().is_none(), "no third retry");
    }

    #[test]
    fn a_names_deferral_wakes_the_worker_once_when_it_ends_and_work_is_waiting() {
        // R160: the tick asks `names_wakeup_due`; it is true once.
        let mut m = settled_lobby(&["me"]);
        m.slots.push(slot("p2", false));
        m.synced.insert("p2".into());
        assert!(!m.names_needed().is_empty(), "p2 needs a name");
        m.defer_names(10 * S);
        assert!(m.names_needed().is_empty(), "deferred");
        assert!(!m.names_wakeup_due(), "not before the end");
        m.advance_clock(11 * S);
        assert!(m.names_wakeup_due(), "ended, names waiting");
        assert!(!m.names_wakeup_due(), "once");
        assert!(!m.names_needed().is_empty());

        // Nothing waiting: the deferral is cleared without a wake-up.
        let mut m = settled_lobby(&["me"]);
        m.defer_names(S);
        m.advance_clock(2 * S);
        assert!(!m.names_wakeup_due());

        // A new session forgets it.
        let mut m = settled_lobby(&["me"]);
        m.slots.push(slot("p2", false));
        m.defer_names(100 * S);
        m.reset_names_hold();
        assert!(!m.names_needed().is_empty());
        assert!(!m.names_wakeup_due());
    }

    #[tokio::test]
    async fn a_waiting_details_rerun_is_one_per_player_and_ends_with_its_guard() {
        // R160: a reopen during the wait finds the mark taken; the mark ends
        // when the guard drops, whichever way the job leaves.
        let h = crate::pipeline_tests::harness().await;
        let first = h.st.begin_details_rerun("p1").expect("free");
        assert!(h.st.begin_details_rerun("p1").is_none(), "one per player");
        let other =
            h.st.begin_details_rerun("p2")
                .expect("another player is free");
        drop(first);
        assert!(
            h.st.begin_details_rerun("p1").is_some(),
            "ended with the guard"
        );
        drop(other);
    }

    #[test]
    fn a_held_rank_sync_for_a_player_off_the_roster_or_while_closed_schedules_nothing() {
        let mut m = settled_lobby(&["me"]);
        m.note_rank_held("ghost", 5 * S, false);
        m.stage = Stage::Closed;
        m.note_rank_held("me", 5 * S, false);
        m.stage = Stage::Menus;
        m.advance_clock(100 * S);
        assert_eq!(m.due_rank_retry(), None);
        assert!(m.rank_retries.is_empty());
    }

    #[test]
    fn a_retry_cut_short_by_an_aborted_worker_still_uses_up_its_turn() {
        // A stage change aborts the worker mid-retry: no outcome is ever
        // noted. The turn is spent all the same, so aborts cannot add
        // attempts: the next retry is 60 s after this one was handed out.
        let mut m = settled_lobby(&["me"]);
        m.note_rank_sync("me", true, false);
        m.advance_clock(20 * S);
        let id = m.try_start_worker().expect("due");
        assert_eq!(m.next_job(id), Some(Job::RankRetry("me".into())));
        m.revoke_worker(); // aborted; nothing noted
        assert_eq!(m.due_rank_retry(), None, "not straight away again");
        m.advance_clock(59 * S);
        assert!(m.try_start_worker().is_none());
        m.advance_clock(S);
        let id = m.try_start_worker().expect("the second and last turn");
        assert_eq!(m.next_job(id), Some(Job::RankRetry("me".into())));
        m.revoke_worker(); // aborted again
        m.advance_clock(100_000 * S);
        assert!(m.try_start_worker().is_none(), "no third turn, ever");
    }

    #[test]
    fn a_retry_that_works_ends_the_round() {
        let mut m = settled_lobby(&["me"]);
        m.note_rank_sync("me", true, false);
        m.advance_clock(20 * S);
        m.note_rank_sync("me", false, true);
        m.advance_clock(1_000 * S);
        assert!(m.try_start_worker().is_none());
    }

    #[test]
    fn a_due_retry_comes_after_first_pass_syncs_and_before_the_fill_and_the_backfill() {
        let mut m = model(vec![
            slot("me", false),
            slot("p1", false),
            slot("p2", false),
        ]);
        m.stage = Stage::Menus;
        m.names_attempted
            .extend(["p1".to_string(), "p2".to_string()]);
        m.synced.extend(["me".to_string(), "p1".to_string()]);
        m.note_rank_sync("p1", true, false);
        m.advance_clock(20 * S);
        m.queue_name_fill([NameAsk {
            puuid: "stranger".into(),
            match_start_ms: Some(1),
        }]);
        m.start_backfill(None, None);
        let id = m.try_start_worker().expect("work pending");

        // p2 was never synced: a first pass goes before any retry.
        assert_eq!(m.next_job(id), Some(Job::Rank(vec!["p2".into()])));
        m.synced.mark_ranked("p2");
        assert_eq!(m.next_job(id), Some(Job::Sync("p2".into())));
        m.synced.insert("p2".into());
        // The due retry goes before the names fill and the backfill.
        assert_eq!(m.next_job(id), Some(Job::RankRetry("p1".into())));
        m.note_rank_sync("p1", false, true);
        assert_eq!(m.next_job(id), Some(Job::FillNames));
        m.clear_name_fill();
        assert_eq!(m.next_job(id), Some(Job::Backfill));
        // A retry that comes due while the backfill runs is taken before its
        // next step: the run's own yield check sees it.
        m.note_rank_sync("p2", true, false);
        assert!(!m.live_job_pending(), "not due yet");
        m.advance_clock(20 * S);
        assert!(m.live_job_pending(), "due: the backfill yields to it");
        assert_eq!(m.next_job(id), Some(Job::RankRetry("p2".into())));
    }

    #[test]
    fn a_retry_that_is_not_due_yet_blocks_no_other_work() {
        let mut m = settled_lobby(&["me"]);
        m.note_rank_sync("me", true, false);
        m.start_backfill(None, None);
        let id = m.try_start_worker().expect("the run is work");
        assert_eq!(m.next_job(id), Some(Job::Backfill), "no waiting for it");
        m.backfill = None;
        assert_eq!(m.next_job(id), None, "the worker exits, it does not wait");
        assert!(!m.worker_running());
        // And it wakes for the retry once due, with nothing else queued.
        m.advance_clock(20 * S);
        let id = m.try_start_worker().expect("the retry is work");
        assert_eq!(m.next_job(id), Some(Job::RankRetry("me".into())));
    }

    #[test]
    fn a_player_who_left_the_roster_is_not_retried() {
        let mut m = settled_lobby(&["me", "p1"]);
        m.note_rank_sync("p1", true, false);
        m.slots.retain(|s| s.roster.puuid != "p1");
        m.prune_rank_retries();
        m.advance_clock(100 * S);
        assert_eq!(m.due_rank_retry(), None);
        assert!(m.try_start_worker().is_none());
        // Back on a later roster: the old round is gone (not resumed).
        m.slots.push(slot("p1", false));
        m.synced.insert("p1".into());
        assert_eq!(m.due_rank_retry(), None);

        // Never scheduled for someone who is not on the roster.
        m.note_rank_sync("ghost", true, false);
        m.advance_clock(100 * S);
        m.slots.push(slot("ghost", false));
        assert_eq!(m.due_rank_retry(), None);
    }

    #[test]
    fn a_retry_waits_out_a_roster_change_without_the_player_leaving() {
        // The user's own card counts, and a roster that still shows the
        // player keeps their pending retry.
        let mut m = settled_lobby(&["me"]);
        m.note_rank_sync("me", true, false);
        m.slots.push(slot("newcomer", false));
        m.prune_rank_retries();
        m.advance_clock(20 * S);
        assert_eq!(m.due_rank_retry().as_deref(), Some("me"));
    }

    #[test]
    fn no_retry_runs_or_is_scheduled_while_valorant_is_closed() {
        let mut m = settled_lobby(&["me"]);
        m.note_rank_sync("me", true, false);
        m.stage = Stage::Closed;
        m.advance_clock(1_000 * S);
        assert_eq!(m.due_rank_retry(), None);
        assert!(m.try_start_worker().is_none());
        // A sync finishing while closed schedules nothing.
        let mut m = settled_lobby(&["me"]);
        m.stage = Stage::Closed;
        m.note_rank_sync("me", true, false);
        m.stage = Stage::Menus;
        m.advance_clock(1_000 * S);
        assert_eq!(m.due_rank_retry(), None);
    }

    #[test]
    fn backfill_waits_for_the_match_to_end() {
        // R137: a first run is tens of MB of match-details; never during a
        // match (Agent Select / In Game), where it would compete with the
        // live cards for the shared rate bucket.
        for stage in [Stage::Pregame, Stage::Ingame] {
            let mut m = model(vec![slot("me", false)]);
            m.synced.insert("me".into());
            m.stage = stage;
            m.start_backfill(None, None);
            assert!(
                m.try_start_worker().is_none(),
                "{stage:?}: a run alone is no work during a match"
            );

            // A worker that claimed in Menus when the stage changed under it.
            m.stage = Stage::Menus;
            let id = m.try_start_worker().expect("Menus: the run is work");
            m.stage = stage;
            assert_eq!(m.next_job(id), None, "{stage:?}: never Backfill");
            assert!(!m.worker_running(), "{stage:?}: the claim is released");
            assert!(m.backfill.is_some(), "{stage:?}: the cursor waits");
        }

        // Back between matches: the same run resumes (Menus and Closed).
        for stage in [Stage::Menus, Stage::Closed] {
            let mut m = model(vec![slot("me", false)]);
            m.synced.insert("me".into());
            m.stage = stage;
            m.start_backfill(None, None);
            let id = m.try_start_worker().expect("the run is work");
            assert_eq!(m.next_job(id), Some(Job::Backfill), "{stage:?}");
        }
    }

    #[test]
    fn backfill_waits_for_a_free_for_all_match_too() {
        // R137 with R41: a Deathmatch keeps the Lobby view (`stage` stays
        // Menus) for the whole match; the raw presence (`in_match`) still
        // keeps the backfill out of it.
        let mut m = model(vec![slot("me", false)]);
        m.synced.insert("me".into());
        m.stage = Stage::Menus;
        m.in_match = true;
        m.start_backfill(None, None);
        assert!(
            m.try_start_worker().is_none(),
            "a run alone is no work during a free-for-all match"
        );

        // A worker that claimed before the match started.
        m.in_match = false;
        let id = m
            .try_start_worker()
            .expect("between matches: the run is work");
        m.in_match = true;
        assert_eq!(m.next_job(id), None, "never Backfill mid-match");
        assert!(!m.worker_running(), "the claim is released");
        assert!(m.backfill.is_some(), "the cursor waits");

        // The match ended: the same run resumes.
        m.in_match = false;
        let id = m.try_start_worker().expect("the run is work again");
        assert_eq!(m.next_job(id), Some(Job::Backfill));
    }

    #[test]
    fn a_backfill_run_alone_starts_a_worker() {
        // The roster is fully synced; only the run is left. `try_start_worker`
        // must still claim (R45), or a run started after the sync would
        // never get a worker.
        let mut m = model(vec![slot("me", false)]);
        m.stage = Stage::Menus;
        m.synced.insert("me".into());
        assert!(m.try_start_worker().is_none(), "no run, no work");
        m.start_backfill(None, None);
        let id = m.try_start_worker().expect("the run is work");
        assert_eq!(m.next_job(id), Some(Job::Backfill));
    }

    #[test]
    fn start_backfill_is_noop_while_running() {
        let mut m = model(vec![slot("me", false)]);
        let anchor = Some(("m9".to_string(), 900));
        m.start_backfill(anchor.clone(), Some(500));
        {
            let run = m.backfill.as_mut().unwrap();
            assert_eq!(run.anchor, anchor);
            assert_eq!(run.act_start_ms, Some(500));
            assert_eq!((run.next_index, run.scanned), (0, 0));
            assert!(run.pending.is_empty() && run.newest.is_none() && !run.done);
            assert_eq!(run.total, None);
            // The run made progress…
            run.next_index = 40;
            run.scanned = 37;
            run.pending.push_back("m3".into());
        }
        // …and a second trigger (a match ended meanwhile) must not reset it.
        m.start_backfill(None, None);
        let run = m.backfill.as_ref().unwrap();
        assert_eq!(run.anchor, anchor, "same run");
        assert_eq!((run.next_index, run.scanned), (40, 37));
        assert_eq!(run.pending.len(), 1);
    }

    // -- B1: the own competitive-updates backfill --------------------------

    #[test]
    fn the_comp_backfill_job_comes_after_live_cards_the_fill_and_the_match_backfill() {
        let mut m = model(vec![slot("me", false)]);
        m.stage = Stage::Menus;
        m.synced.insert("me".into());
        m.start_backfill(None, None);
        m.start_comp_backfill(None, None);
        let id = m.try_start_worker().expect("work");

        // The names fill and the match backfill both go first.
        m.queue_name_fill([ask("someone", None)]);
        assert_eq!(m.next_job(id), Some(Job::FillNames));
        m.name_fill.clear();
        assert_eq!(m.next_job(id), Some(Job::Backfill));
        m.backfill = None;

        // Only now the competitive updates, a page per job until the run ends.
        assert_eq!(m.next_job(id), Some(Job::CompBackfill));
        assert_eq!(
            m.next_job(id),
            Some(Job::CompBackfill),
            "until the run ends"
        );
        // A roster player added mid-run goes first again.
        m.slots.push(slot("late", false));
        m.names_attempted.insert("late".into());
        assert_eq!(m.next_job(id), Some(Job::Rank(vec!["late".into()])));
        m.synced.mark_ranked("late");
        assert_eq!(m.next_job(id), Some(Job::Sync("late".into())));
        m.synced.insert("late".into());
        assert_eq!(m.next_job(id), Some(Job::CompBackfill));

        m.comp_backfill = None;
        assert_eq!(m.next_job(id), None);
        assert!(!m.worker_running(), "claim released");
    }

    #[test]
    fn comp_backfill_waits_for_the_match_to_end() {
        for (stage, in_match) in [
            (Stage::Pregame, true),
            (Stage::Ingame, true),
            // A free-for-all match keeps the Lobby view.
            (Stage::Menus, true),
        ] {
            let mut m = model(vec![slot("me", false)]);
            m.synced.insert("me".into());
            m.stage = stage;
            m.in_match = in_match;
            m.start_comp_backfill(None, None);
            assert!(
                m.try_start_worker().is_none(),
                "{stage:?}: a run alone is no work in a match"
            );

            // A worker that claimed just before the match began.
            m.stage = Stage::Menus;
            m.in_match = false;
            let id = m
                .try_start_worker()
                .expect("between matches: the run is work");
            m.stage = stage;
            m.in_match = in_match;
            assert_eq!(m.next_job(id), None, "{stage:?}: never CompBackfill");
            assert!(m.comp_backfill.is_some(), "{stage:?}: the cursor waits");
        }
        // Closed counts as between matches, like the match backfill.
        let mut m = model(vec![slot("me", false)]);
        m.synced.insert("me".into());
        m.stage = Stage::Closed;
        m.start_comp_backfill(None, None);
        let id = m.try_start_worker().expect("the run is work");
        assert_eq!(m.next_job(id), Some(Job::CompBackfill));
    }

    #[test]
    fn a_paused_comp_backfill_is_no_work_until_its_hold_ends_and_then_wakes_the_tick() {
        let mut m = model(vec![slot("me", false)]);
        m.stage = Stage::Menus;
        m.synced.insert("me".into());
        m.start_comp_backfill(None, None);
        m.pause_comp_backfill(10 * S);
        assert!(!m.comp_backfill_ready());
        assert!(!m.comp_backfill_resume_due());
        assert!(m.try_start_worker().is_none(), "a pause alone is no work");
        assert!(m.comp_backfill.is_some(), "the cursor waits");

        // A shorter pause never shortens it.
        m.pause_comp_backfill(2 * S);
        m.advance_clock(5 * S);
        assert!(!m.comp_backfill_ready());

        m.advance_clock(6 * S);
        assert!(m.comp_backfill_ready());
        assert!(m.comp_backfill_resume_due(), "the tick starts a worker");
        let id = m.try_start_worker().expect("work again");
        assert_eq!(m.next_job(id), Some(Job::CompBackfill));

        // The pause ends with the run's next page, and in a match it is no
        // wake-up at all.
        m.comp_backfill.as_mut().unwrap().resume_at = None;
        assert!(!m.comp_backfill_resume_due());
        m.pause_comp_backfill(S);
        m.advance_clock(2 * S);
        m.in_match = true;
        assert!(!m.comp_backfill_resume_due());
    }

    #[test]
    fn start_comp_backfill_is_noop_while_running() {
        let mut m = model(vec![slot("me", false)]);
        let anchor = Some(("m9".to_string(), 900));
        m.start_comp_backfill(anchor.clone(), Some(500));
        {
            let run = m.comp_backfill.as_mut().unwrap();
            assert_eq!(run.anchor, anchor);
            assert_eq!(run.act_start_ms, Some(500));
            assert_eq!((run.next_index, run.pages), (0, 0));
            assert!(run.newest.is_none() && run.resume_at.is_none());
            run.next_index = 40;
            run.pages = 2;
        }
        // A second trigger (a match ended meanwhile) must not reset it.
        m.start_comp_backfill(None, None);
        let run = m.comp_backfill.as_ref().unwrap();
        assert_eq!(run.anchor, anchor, "same run");
        assert_eq!((run.next_index, run.pages), (40, 2));
    }

    // -- R150: the names fill ---------------------------------------------

    fn ask(puuid: &str, match_start_ms: Option<i64>) -> NameAsk {
        NameAsk {
            puuid: puuid.into(),
            match_start_ms,
        }
    }

    fn queued(m: &LiveModel) -> Vec<(&str, Option<i64>)> {
        m.name_fill
            .iter()
            .map(|a| (a.puuid.as_str(), a.match_start_ms))
            .collect()
    }

    #[test]
    fn the_fill_queue_keeps_one_ask_per_puuid_and_the_newest_match() {
        let mut m = model(vec![]);
        m.queue_name_fill(vec![ask("a", None), ask("b", Some(10))]);
        // Asked again: an on-demand ask never overrides a match's; a match's
        // ask upgrades an on-demand one (it covers a puuid asked before);
        // of two matches the newer stays.
        m.queue_name_fill(vec![
            ask("a", Some(20)),
            ask("b", None),
            ask("b", Some(5)),
            ask("c", None),
        ]);
        assert_eq!(queued(&m), [("a", Some(20)), ("b", Some(10)), ("c", None)]);
        m.queue_name_fill(vec![ask("b", Some(30))]);
        assert_eq!(queued(&m)[1], ("b", Some(30)));
    }

    #[test]
    fn the_fill_queue_is_bounded() {
        let mut m = model(vec![]);
        let many: Vec<NameAsk> = (0..NAME_FILL_QUEUE_MAX + 50)
            .map(|i| ask(&format!("p{i}"), None))
            .collect();
        m.queue_name_fill(many);
        assert_eq!(m.name_fill.len(), NAME_FILL_QUEUE_MAX);
        assert_eq!(m.name_fill[0].puuid, "p0", "the oldest asks are kept");
        // Room is not made by dropping: a full queue takes nothing more.
        m.queue_name_fill(vec![ask("late", None)]);
        assert!(!m.name_fill.iter().any(|a| a.puuid == "late"));
    }

    #[test]
    fn low_priority_asks_have_a_soft_cap_and_never_evict_anything() {
        // R167: the roster matches phase's asks are low priority.
        let mut m = model(vec![]);
        m.queue_name_fill_low((0..NAME_FILL_LOW_MAX + 40).map(|i| ask(&format!("l{i}"), Some(1))));
        assert_eq!(m.name_fill.len(), NAME_FILL_LOW_MAX, "soft cap");
        // Room in the hard cap does not lift the soft one…
        m.queue_name_fill_low([ask("late", Some(1))]);
        assert_eq!(m.name_fill.len(), NAME_FILL_LOW_MAX);
        // …and a queue full of high asks takes no low one.
        let mut m = model(vec![]);
        m.queue_name_fill((0..NAME_FILL_QUEUE_MAX).map(|i| ask(&format!("h{i}"), None)));
        m.queue_name_fill_low([ask("l", Some(1))]);
        assert_eq!(m.name_fill.len(), NAME_FILL_QUEUE_MAX);
        assert!(!m.name_fill.iter().any(|a| a.puuid == "l"));
    }

    #[test]
    fn high_priority_asks_go_first_and_evict_the_oldest_low_ones_when_full() {
        let mut m = model(vec![]);
        m.queue_name_fill_low((0..NAME_FILL_LOW_MAX).map(|i| ask(&format!("l{i}"), Some(1))));
        m.queue_name_fill((0..NAME_FILL_QUEUE_MAX - 50).map(|i| ask(&format!("h{i}"), Some(9))));
        // 100 low + 250 high would be 350: the 50 oldest low ones made room.
        assert_eq!(m.name_fill.len(), NAME_FILL_QUEUE_MAX);
        assert!(m
            .name_fill
            .iter()
            .take(NAME_FILL_QUEUE_MAX - 50)
            .all(|a| a.puuid.starts_with('h')));
        assert!(!m
            .name_fill
            .iter()
            .any(|a| a.puuid == "l0" || a.puuid == "l49"));
        assert!(m.name_fill.iter().any(|a| a.puuid == "l50"));
        // No high ask was lost: all 250 are queued, in arrival order.
        let high: Vec<&str> = m
            .name_fill
            .iter()
            .map(|a| a.puuid.as_str())
            .filter(|p| p.starts_with('h'))
            .collect();
        let expected: Vec<String> = (0..NAME_FILL_QUEUE_MAX - 50)
            .map(|i| format!("h{i}"))
            .collect();
        assert_eq!(high, expected);
        // A full queue of high asks takes no more (as before).
        m.queue_name_fill((0..100).map(|i| ask(&format!("x{i}"), Some(9))));
        m.queue_name_fill((0..100).map(|i| ask(&format!("y{i}"), Some(9))));
        assert!(m.name_fill.len() <= NAME_FILL_QUEUE_MAX);
        // Taken in that order: high before low.
        m.stage = Stage::Menus;
        let first = m.take_fill(1).pop().unwrap();
        assert_eq!(first.puuid, "h0");
    }

    #[test]
    fn a_high_ask_promotes_a_queued_low_one_and_a_low_ask_never_demotes() {
        let mut m = model(vec![]);
        m.queue_name_fill_low([ask("a", Some(1)), ask("b", Some(1))]);
        m.queue_name_fill([ask("c", Some(5))]);
        m.queue_name_fill([ask("b", Some(7))]);
        // c, then the promoted b, then the low a.
        assert_eq!(queued(&m), [("c", Some(5)), ("b", Some(7)), ("a", Some(1))]);
        m.queue_name_fill_low([ask("c", Some(9)), ask("a", Some(3))]);
        // c keeps its place and takes the newer match; a is still low.
        assert_eq!(queued(&m), [("c", Some(9)), ("b", Some(7)), ("a", Some(3))]);
        m.queue_name_fill([ask("d", None)]);
        assert_eq!(
            queued(&m)[2],
            ("d", None),
            "a high ask still lands before the low a"
        );
        assert_eq!(queued(&m)[3].0, "a");
    }

    #[test]
    fn the_fill_job_comes_after_live_cards_and_before_the_backfill() {
        let mut m = model(vec![slot("me", false), slot("p1", false)]);
        m.stage = Stage::Menus;
        m.start_backfill(None, None);
        m.queue_name_fill(vec![ask("x", Some(1))]);
        let id = m.try_start_worker().expect("work pending");

        assert!(matches!(m.next_job(id), Some(Job::Names(_))));
        m.names_attempted.insert("p1".into());
        assert_eq!(
            m.next_job(id),
            Some(Job::Rank(vec!["me".into(), "p1".into()]))
        );
        m.synced.mark_ranked("me");
        m.synced.mark_ranked("p1");
        assert_eq!(m.next_job(id), Some(Job::Sync("me".into())));
        m.synced.insert("me".into());
        assert_eq!(m.next_job(id), Some(Job::Sync("p1".into())));
        m.synced.insert("p1".into());
        assert!(!m.live_job_pending());

        // Live cards done: the fill, then (its queue drained) the run.
        assert_eq!(m.next_job(id), Some(Job::FillNames));
        assert_eq!(m.take_fill(10).len(), 1);
        assert_eq!(m.next_job(id), Some(Job::Backfill));

        // A fill queued mid-run goes ahead of the run's next step, and a
        // roster player added later goes ahead of both.
        m.queue_name_fill(vec![ask("y", None)]);
        assert_eq!(m.next_job(id), Some(Job::FillNames));
        m.slots.push(slot("late", false));
        m.names_attempted.insert("late".into());
        assert_eq!(m.next_job(id), Some(Job::Rank(vec!["late".into()])));
    }

    #[test]
    fn the_fill_waits_for_the_match_to_end() {
        // Never in Agent Select or In Game, and never in a free-for-all
        // match either (its view stays the Lobby, `in_match` is the truth).
        for (stage, in_match) in [
            (Stage::Pregame, true),
            (Stage::Ingame, true),
            (Stage::Menus, true),
        ] {
            let mut m = model(vec![slot("me", false)]);
            m.synced.insert("me".into());
            m.stage = stage;
            m.in_match = in_match;
            m.queue_name_fill(vec![ask("x", None)]);
            assert!(
                m.try_start_worker().is_none(),
                "{stage:?}/{in_match}: a fill alone is no work during a match"
            );

            // A worker that claimed between matches, then the match began.
            m.stage = Stage::Menus;
            m.in_match = false;
            let id = m.try_start_worker().expect("between matches: work");
            m.stage = stage;
            m.in_match = in_match;
            assert_eq!(m.next_job(id), None, "{stage:?}/{in_match}: never a fill");
            assert!(!m.worker_running(), "the claim is released");
            assert_eq!(m.name_fill.len(), 1, "the ask waits in the queue");
        }
        // Back between matches (Lobby, or Closed): the same ask runs.
        for stage in [Stage::Menus, Stage::Closed] {
            let mut m = model(vec![slot("me", false)]);
            m.synced.insert("me".into());
            m.stage = stage;
            m.queue_name_fill(vec![ask("x", None)]);
            let id = m.try_start_worker().expect("the fill is work");
            assert_eq!(m.next_job(id), Some(Job::FillNames), "{stage:?}");
        }
    }

    #[test]
    fn take_fill_hands_out_hidden_players_but_never_the_user_or_an_empty_id() {
        // R153: a puuid shown hidden on the live screen now
        // (`hidden_players`) or earlier this run (`seen_hidden`) is asked
        // about between matches like anyone else — the game's Match History
        // shows their real names after the match. The user and an empty id
        // are dropped from the queue, and not kept for later.
        let mut m = model(vec![
            slot("me", false),
            slot("live-hidden", true),
            slot("visible", false),
        ]);
        m.stage = Stage::Menus;
        m.seen_hidden.insert("was-hidden".into(), None);
        // Straight into the queue: it is `take_fill` that is under test.
        m.name_fill.extend([
            ask("live-hidden", Some(1)),
            ask("was-hidden", Some(1)),
            ask("me", Some(1)),
            ask("", Some(1)),
            ask("visible", Some(1)),
            ask("stranger", None),
        ]);
        let taken: Vec<String> = m.take_fill(10).into_iter().map(|a| a.puuid).collect();
        assert_eq!(taken, ["live-hidden", "was-hidden", "visible", "stranger"]);
        assert!(
            m.name_fill.is_empty(),
            "the user's and empty asks are dropped"
        );
        // The same rule as a check, for the last look before a request.
        assert!(m.fill_allowed("live-hidden"));
        assert!(m.fill_allowed("was-hidden"));
        assert!(!m.fill_allowed("me"));
        assert!(!m.fill_allowed(""));
        assert!(m.fill_allowed("stranger"));
    }

    #[test]
    fn take_fill_hands_out_nothing_while_a_match_runs() {
        // R153: the stage is the guard. Agent Select, In Game and a
        // free-for-all match (Lobby view, raw presence in a match): no ask
        // leaves the queue — not even one for a player shown hidden on the
        // roster — and none is lost; they go out once the match is over.
        for (stage, in_match) in [
            (Stage::Pregame, true),
            (Stage::Ingame, true),
            (Stage::Menus, true),
        ] {
            let mut m = model(vec![slot("me", false), slot("live-hidden", true)]);
            m.stage = stage;
            m.in_match = in_match;
            m.seen_hidden.insert("was-hidden".into(), None);
            m.queue_name_fill([
                ask("live-hidden", Some(1)),
                ask("was-hidden", Some(1)),
                ask("stranger", None),
            ]);
            assert!(
                m.take_fill(10).is_empty(),
                "{stage:?}/{in_match}: nothing is taken"
            );
            assert_eq!(m.name_fill.len(), 3, "{stage:?}/{in_match}: all kept");

            m.stage = Stage::Menus;
            m.in_match = false;
            let taken: Vec<String> = m.take_fill(10).into_iter().map(|a| a.puuid).collect();
            assert_eq!(taken, ["live-hidden", "was-hidden", "stranger"]);
        }
    }

    #[test]
    fn fill_visible_builds_nothing_while_a_match_runs() {
        // R167: the door checks the stage itself. A live-hidden player, a
        // player hidden earlier and a stranger are all built between matches
        // (R153); the user and an empty id never; nothing at all during a
        // match, whoever is asked.
        let asks = [
            ask("live-hidden", Some(1)),
            ask("was-hidden", Some(1)),
            ask("stranger", None),
            ask("me", None),
            ask("", None),
        ];
        let ids = |v: Vec<VisiblePuuid>| -> Vec<String> {
            v.iter().map(|p| p.as_str().to_string()).collect()
        };
        for (stage, in_match) in [
            (Stage::Pregame, true),
            (Stage::Ingame, true),
            (Stage::Menus, true),
        ] {
            let mut m = model(vec![slot("me", false), slot("live-hidden", true)]);
            m.stage = stage;
            m.in_match = in_match;
            m.seen_hidden.insert("was-hidden".into(), None);
            assert!(
                m.fill_visible(&asks).is_empty(),
                "{stage:?}/{in_match}: nothing is built"
            );
        }
        for stage in [Stage::Menus, Stage::Closed] {
            let mut m = model(vec![slot("me", false), slot("live-hidden", true)]);
            m.stage = stage;
            m.in_match = false;
            m.seen_hidden.insert("was-hidden".into(), None);
            assert_eq!(
                ids(m.fill_visible(&asks)),
                ["live-hidden", "was-hidden", "stranger"],
                "{stage:?}"
            );
        }
    }

    #[test]
    fn hidden_players_are_queued_like_anyone_else_but_not_the_user() {
        let mut m = model(vec![slot("me", false), slot("live-hidden", true)]);
        m.seen_hidden.insert("was-hidden".into(), None);
        m.queue_name_fill(vec![
            ask("live-hidden", None),
            ask("was-hidden", Some(1)),
            ask("me", Some(1)),
            ask("", None),
            ask("visible", None),
        ]);
        assert_eq!(
            queued(&m),
            [
                ("live-hidden", None),
                ("was-hidden", Some(1)),
                ("visible", None)
            ]
        );
    }

    #[test]
    fn take_fill_takes_at_most_n_in_arrival_order() {
        let mut m = model(vec![]);
        m.stage = Stage::Menus;
        m.queue_name_fill((0..5).map(|i| ask(&format!("p{i}"), None)));
        let first: Vec<String> = m.take_fill(2).into_iter().map(|a| a.puuid).collect();
        assert_eq!(first, ["p0", "p1"]);
        assert_eq!(m.name_fill.len(), 3);
        assert!(m.take_fill(0).is_empty());
        assert_eq!(m.take_fill(99).len(), 3);
        assert!(!m.fill_ready());
    }

    #[test]
    fn settle_only_touches_attempted_sources() {
        let mut m = model(vec![slot("p1", false)]);
        m.names_attempted.insert("p1".into());
        let card = &m.build(&[], 0, &|_| None).players[0];
        assert_eq!(card.name, Field::Missing);
        assert!(card.rank.is_loading(), "data not attempted yet");
    }

    #[test]
    fn hidden_level_is_missing() {
        let mut s = slot("p1", false);
        s.roster.hide_account_level = true;
        let m = model(vec![s]);
        assert_eq!(m.build(&[], 0, &|_| None).players[0].level, Field::Missing);
    }

    // -- R154: the rank from stored matches when MMR has no answer ------

    const ACT_ID: &str = "act-now";
    /// A card built at `NOW`, inside `acts_now()`'s only act.
    const NOW: i64 = 10_000_000;

    fn acts_now() -> Vec<Act> {
        vec![Act {
            id: ACT_ID.into(),
            name: "ACT V".into(),
            episode_name: "V26".into(),
            start_ms: 5_000_000,
            end_ms: 20_000_000,
        }]
    }

    fn update(id: &str, season: &str, start_ms: i64, tier: i64, rr: i64) -> CompUpdateRow {
        CompUpdateRow {
            match_id: id.into(),
            season_id: season.into(),
            tier_after: tier,
            tier_before: Some(tier),
            rr_after: rr,
            rr_earned: 10,
            map_id: None,
            start_ms,
        }
    }

    fn rank_of(m: &LiveModel) -> Field<opd1_core::model::RankInfo> {
        m.build(&acts_now(), NOW, &|_| None).players[0].rank.clone()
    }

    fn ranked(tier: u8, rr: Option<u16>) -> Field<opd1_core::model::RankInfo> {
        Field::Value(opd1_core::model::RankInfo { tier, rr })
    }

    #[test]
    fn no_snapshot_the_card_shows_the_newest_current_act_rank_at_once() {
        let mut s = slot("p1", false);
        s.db.comp = vec![
            update("m2", ACT_ID, 9_000_000, 15, 42),
            update("m1", ACT_ID, 8_000_000, 14, 90),
            update("m0", "act-before", 4_000_000, 20, 10),
        ];
        let mut m = model(vec![s]);
        // Not synced yet: the cached rank shows, the rest is still loading.
        let card = &m.build(&acts_now(), NOW, &|_| None).players[0];
        assert_eq!(card.rank, ranked(15, Some(42)), "tier and exact RR");
        assert!(card.peak.is_loading() && card.prev_act_tier.is_loading());
        // The sync ran and MMR never answered: the rank stays, the rest is `—`.
        m.synced.insert("p1".into());
        let card = &m.build(&acts_now(), NOW, &|_| None).players[0];
        assert_eq!(card.rank, ranked(15, Some(42)));
        assert_eq!(card.peak, Field::Missing);
        assert_eq!(card.prev_act_tier, Field::Missing);
        assert_eq!(card.act_winrate, Field::Missing);
        assert_eq!(card.act_wins, Field::Missing);
        assert_eq!(card.act_games, Field::Missing);
    }

    #[test]
    fn no_snapshot_and_no_update_the_card_shows_the_stored_match_tier_without_rr() {
        let mut s = slot("p1", false);
        s.db.stored_tier = Some(StoredTier {
            start_ms: 8_000_000,
            tier: 14,
        });
        let mut m = model(vec![s]);
        m.synced.insert("p1".into());
        assert_eq!(rank_of(&m), ranked(14, None));
    }

    #[test]
    fn a_previous_act_row_or_match_is_no_rank() {
        let mut s = slot("p1", false);
        s.db.comp = vec![update("old", "act-before", 4_000_000, 20, 10)];
        s.db.stored_tier = Some(StoredTier {
            start_ms: 4_500_000,
            tier: 20,
        });
        let mut m = model(vec![s]);
        assert!(rank_of(&m).is_loading(), "nothing known: a skeleton");
        m.synced.insert("p1".into());
        assert_eq!(rank_of(&m), Field::Missing, "as before R154");
    }

    #[test]
    fn a_snapshot_is_the_authority_even_with_a_newer_update_row() {
        let mmr: PlayerMmrResponse = serde_json::from_value(json!({
            "Subject": "p1",
            "QueueSkills": {"competitive": {"SeasonalInfoBySeasonID": {ACT_ID: {
                "SeasonID": ACT_ID, "CompetitiveTier": 12, "RankedRating": 33,
                "NumberOfWins": 6, "NumberOfGames": 10, "WinsByTier": {"12": 4}
            }}}}
        }))
        .unwrap();
        let mut s = slot("p1", false);
        s.db.mmr = Some(mmr);
        s.db.comp = vec![update("m", ACT_ID, 9_500_000, 18, 77)];
        s.db.stored_tier = Some(StoredTier {
            start_ms: 9_600_000,
            tier: 19,
        });
        let m = model(vec![s]);
        assert_eq!(rank_of(&m), ranked(12, Some(33)));
    }

    #[test]
    fn the_closed_stage_own_card_shows_the_fallback() {
        // No MMR snapshot of the user, only what their matches stored.
        let db = PlayerDb {
            row: Some(StoredPlayer {
                game_name: "Me".into(),
                tag_line: "1".into(),
                account_level: 50,
                card_id: "card".into(),
            }),
            comp: vec![update("m", ACT_ID, 9_000_000, 15, 42)],
            ..PlayerDb::default()
        };
        let mut m = LiveModel::new(Some("me".into()));
        m.stage = Stage::Closed;
        m.slots = vec![closed_self_slot("me", db).unwrap()];
        assert_eq!(rank_of(&m), ranked(15, Some(42)));

        // Only a stored match's tier: the tier shows, the RR is `—`.
        let db = PlayerDb {
            row: m.slots[0].db.row.clone(),
            stored_tier: Some(StoredTier {
                start_ms: 9_000_000,
                tier: 14,
            }),
            ..PlayerDb::default()
        };
        m.slots = vec![closed_self_slot("me", db).unwrap()];
        assert_eq!(rank_of(&m), ranked(14, None));
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
        fn match_view(&self, _view: &MatchDetailsView) {}
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
    fn a_failing_self_loadout_read_is_logged_and_falls_back_to_none() {
        crate::logbuf::init();
        let dir = tempfile::tempdir().unwrap();
        // A DB without its `app_state` table: every `get_state` errors.
        let repo = Repo::new(opd1_core::db::open_db(std::path::Path::new(":memory:")).unwrap());
        repo.conn().execute_batch("DROP TABLE app_state").unwrap();
        let assets = AssetCache::with_base(dir.path().join("a"), "http://127.0.0.1:1".into());
        let st = AppState::new(
            Box::new(Sink(Arc::new(Recorder::default()))),
            repo,
            assets,
            dir.path().join("debug"),
            Some("me".into()),
        );
        assert!(lock(&st.live).self_loadout.is_none(), "falls back to None");
        assert!(
            crate::logbuf::snapshot().contains("stored self loadout unreadable"),
            "the read error reaches copy-log:\n{}",
            crate::logbuf::snapshot()
        );
    }

    #[tokio::test]
    async fn a_panicking_skin_resolve_never_leaves_a_puuid_resolving() {
        // R97: a skeleton never outlives the attempt — even when the task
        // that holds the mark panics (a poisoned lock in the asset cache).
        let (st, _rec, _dir) = test_state();
        let guard = st.begin_skin_resolve("p1");
        assert!(st.skins_resolving("p1"));
        let task = tokio::spawn(async move {
            let _guard = guard;
            panic!("resolve_skins blew up");
        });
        assert!(task.await.unwrap_err().is_panic());
        assert!(!st.skins_resolving("p1"), "the guard's Drop ended the mark");
    }

    #[test]
    fn skin_resolve_marks_count_and_end_when_dropped() {
        let (st, _rec, _dir) = test_state();
        let first = st.begin_skin_resolve("p1");
        let second = st.begin_skin_resolve("p1");
        let other = st.begin_skin_resolve("p2");
        drop(first);
        assert!(st.skins_resolving("p1"), "one fetch still runs");
        drop(second);
        assert!(!st.skins_resolving("p1"));
        assert!(st.skins_resolving("p2"), "another player is unaffected");
        drop(other);
        assert!(!st.skins_resolving("p2"));
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
            let fresh = lock(&st.live).build(&[], card_clock(now_ms()), &|_| None);
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

    // -- R165: rank first, retries of names and partial syncs ---------------

    /// A model whose players are all named (names are not what is under test).
    fn ranked_model(puuids: &[&str]) -> LiveModel {
        let mut m = model(puuids.iter().map(|p| slot(p, false)).collect());
        for p in puuids {
            m.names_attempted.insert(p.to_string());
        }
        m
    }

    #[test]
    fn the_rank_phase_of_the_whole_roster_comes_before_any_matches_phase() {
        let mut m = ranked_model(&["me", "a1", "a2", "e1", "e2"]);
        let id = m.try_start_worker().expect("work");
        // Display order, three at a time (RANK_CONCURRENCY).
        assert_eq!(
            m.next_job(id),
            Some(Job::Rank(vec!["me".into(), "a1".into(), "a2".into()]))
        );
        // Not finished yet (the worker is mid-job): the same batch again.
        assert_eq!(
            m.next_job(id),
            Some(Job::Rank(vec!["me".into(), "a1".into(), "a2".into()]))
        );
        for p in ["me", "a1", "a2"] {
            m.synced.mark_ranked(p);
        }
        assert_eq!(
            m.next_job(id),
            Some(Job::Rank(vec!["e1".into(), "e2".into()]))
        );
        m.synced.mark_ranked("e1");
        m.synced.mark_ranked("e2");
        // Only now the matches phases, in display order.
        assert_eq!(m.next_job(id), Some(Job::Sync("me".into())));
        m.synced.insert("me".into());
        assert_eq!(m.next_job(id), Some(Job::Sync("a1".into())));
    }

    #[test]
    fn a_fully_synced_player_needs_no_rank_phase_and_a_reset_brings_it_back() {
        let mut m = ranked_model(&["me", "p"]);
        m.synced.insert("me".into());
        assert!(m.synced.rank_done("me"), "a full sync includes the rank");
        assert_eq!(m.next_rank_batch(), vec!["p".to_string()]);
        // A later joiner goes through the rank phase before its matches.
        m.synced.mark_ranked("p");
        m.slots.push(slot("late", false));
        m.names_attempted.insert("late".into());
        assert_eq!(m.next_rank_batch(), vec!["late".to_string()]);
        // A match end re-syncs the party (`synced.remove`): both phases again.
        m.synced.remove("me");
        assert!(!m.synced.rank_done("me"));
        assert_eq!(
            m.next_rank_batch(),
            vec!["me".to_string(), "late".to_string()]
        );
        // A reset (Menus/Closed, account switch) forgets everything.
        m.synced.insert("p".into());
        m.synced.mark_ranked("me");
        m.synced.clear();
        assert!(!m.synced.rank_done("p") && !m.synced.rank_done("me"));
        assert!(!m.synced.contains("p"));
    }

    #[test]
    fn names_come_before_the_rank_phase() {
        let mut m = model(vec![slot("me", false), slot("a", false)]);
        m.names_attempted.insert("me".into());
        let id = m.try_start_worker().expect("work");
        assert!(matches!(m.next_job(id), Some(Job::Names(_))));
    }

    #[test]
    fn a_card_after_the_rank_phase_has_its_rank_fields_settled_and_nothing_else() {
        let mut m = model(vec![slot("p1", false)]);
        m.names_attempted.insert("p1".into());
        let card = &m.build(&[], 0, &|_| None).players[0];
        assert!(card.rank.is_loading() && card.peak.is_loading());
        // The rank phase ran (here: MMR gave nothing): the MMR fields are
        // `—` now; what the matches phase brings is still loading.
        m.synced.mark_ranked("p1");
        let card = &m.build(&[], 0, &|_| None).players[0];
        assert_eq!(card.rank, Field::Missing);
        assert_eq!(card.peak, Field::Missing);
        assert_eq!(card.act_winrate, Field::Missing);
        assert!(card.last5.is_loading(), "the matches are still coming");
        assert!(card.kd.is_loading());
        // The matches phase ended: everything has settled.
        m.synced.insert("p1".into());
        let card = &m.build(&[], 0, &|_| None).players[0];
        assert_eq!(card.last5, Field::Missing);
        assert_eq!(card.kd, Field::Missing);
    }

    #[test]
    fn a_partial_first_pass_sync_is_retried_like_a_failed_rank() {
        let mut m = settled_lobby(&["me"]);
        m.note_partial_sync("me");
        assert!(!m.has_pending_work(), "waiting is not work");
        m.advance_clock(19 * S);
        assert_eq!(m.due_rank_retry(), None);
        m.advance_clock(S);
        let id = m.try_start_worker().expect("due");
        assert_eq!(m.next_job(id), Some(Job::RankRetry("me".into())));
        // The retry is partial again: the same bounded schedule, 60 s later.
        m.note_rank_sync("me", true, true);
        assert_eq!(m.next_job(id), None);
        m.advance_clock(60 * S);
        let id = m.try_start_worker().expect("the second retry");
        assert_eq!(m.next_job(id), Some(Job::RankRetry("me".into())));
        m.note_rank_sync("me", true, true);
        assert_eq!(m.next_job(id), None);
        m.advance_clock(100_000 * S);
        assert!(m.try_start_worker().is_none(), "no third retry");
    }

    #[test]
    fn a_partial_sync_does_not_move_a_round_a_failed_rank_already_started() {
        let mut m = settled_lobby(&["me"]);
        m.note_rank_sync("me", true, false);
        m.advance_clock(15 * S);
        // The matches phase of the same pass was partial too: the round
        // started 15 s ago stays (due in 5 s), it is not restarted.
        m.note_partial_sync("me");
        m.advance_clock(5 * S);
        assert_eq!(m.due_rank_retry().as_deref(), Some("me"));
        // Likewise a hold's round.
        let mut m = settled_lobby(&["me"]);
        m.note_rank_held("me", 10 * S, false);
        m.note_partial_sync("me");
        m.advance_clock(11 * S);
        assert_eq!(m.due_rank_retry(), None, "1.5 s slack of the hold's round");
        m.advance_clock(S);
        assert_eq!(m.due_rank_retry().as_deref(), Some("me"));
    }

    #[test]
    fn a_partial_sync_off_the_roster_or_while_closed_schedules_nothing() {
        let mut m = settled_lobby(&["me"]);
        m.note_partial_sync("ghost");
        m.stage = Stage::Closed;
        m.note_partial_sync("me");
        m.stage = Stage::Menus;
        m.advance_clock(100 * S);
        assert_eq!(m.due_rank_retry(), None);
    }

    #[test]
    fn a_failed_names_lookup_is_retried_after_a_delay_and_three_tries_at_most() {
        let mut m = model(vec![slot("me", false), slot("a", false), slot("b", false)]);
        m.synced
            .extend(["me".to_string(), "a".to_string(), "b".to_string()]);
        m.names_attempted.insert("me".into());
        assert_eq!(m.names_needed().len(), 2);
        // Try 1 failed: not attempted, but not asked again straight away.
        let wait = m.note_names_failed(&["a".to_string(), "b".to_string()]);
        assert_eq!(wait, Some(NAMES_RETRY_DELAYS[0]));
        assert!(
            !m.names_attempted.contains("a"),
            "an error is not an answer"
        );
        assert!(m.names_needed().is_empty(), "not before the delay");
        m.advance_clock(NAMES_RETRY_DELAYS[0] - Duration::from_millis(1));
        assert!(m.names_needed().is_empty());
        m.advance_clock(Duration::from_millis(1));
        assert_eq!(
            m.names_needed(),
            vec![("a".to_string(), false), ("b".to_string(), false)]
        );
        // Try 2 failed.
        assert_eq!(
            m.note_names_failed(&["a".to_string(), "b".to_string()]),
            Some(NAMES_RETRY_DELAYS[1])
        );
        m.advance_clock(NAMES_RETRY_DELAYS[1]);
        assert_eq!(m.names_needed().len(), 2);
        // Try 3 failed: the last. They count as attempted from here (the
        // card shows `—`), and nothing is asked again this stage.
        assert_eq!(
            m.note_names_failed(&["a".to_string(), "b".to_string()]),
            None
        );
        assert!(m.names_attempted.contains("a") && m.names_attempted.contains("b"));
        m.advance_clock(1_000 * S);
        assert!(m.names_needed().is_empty());
    }

    #[test]
    fn an_answered_names_lookup_ends_the_tries_and_a_stage_reset_starts_them_over() {
        let mut m = model(vec![slot("me", false), slot("a", false)]);
        m.note_names_failed(&["a".to_string()]);
        m.names_attempted.insert("a".into()); // answered on the retry
        m.advance_clock(1_000 * S);
        assert!(m.names_needed().is_empty());
        // A reset forgets the tries too: try 1 of the next stage.
        m.names_attempted.clear();
        assert_eq!(
            m.note_names_failed(&["a".to_string()]),
            Some(NAMES_RETRY_DELAYS[0]),
            "a fresh stage: the first failure again"
        );
        m.names_attempted.clear();
        m.note_names_failed(&["a".to_string()]);
        m.note_names_failed(&["a".to_string()]);
        m.names_attempted.clear();
        assert_eq!(
            m.note_names_failed(&["a".to_string()]),
            Some(NAMES_RETRY_DELAYS[0]),
            "tries do not survive a reset"
        );
    }

    #[test]
    fn a_name_waiting_for_its_retry_still_settles_its_card_once_synced_and_fills_in_later() {
        let mut m = model(vec![slot("a", false)]);
        m.note_names_failed(&["a".to_string()]);
        let card = &m.build(&[], 0, &|_| None).players[0];
        assert!(card.name.is_loading(), "retry pending, nothing synced yet");
        m.synced.insert("a".into());
        let card = &m.build(&[], 0, &|_| None).players[0];
        assert_eq!(card.name, Field::Missing, "never a stuck skeleton");
        // The retry answers: the name is there.
        m.known_names
            .insert("a".into(), ("Name".into(), "TAG".into()));
        let card = &m.build(&[], 0, &|_| None).players[0];
        assert!(matches!(card.name, Field::Value(_)));
    }

    #[test]
    fn the_last_match_end_outlives_the_match_that_clears_the_current_one() {
        let mut m = LiveModel::new(Some("me".into()));
        m.match_end_ms = Some(5_000);
        m.observe_match_end();
        // The next match starts: the poll loop clears the live marker.
        m.match_end_ms = None;
        m.observe_match_end();
        assert_eq!(m.last_match_end_ms(), Some(5_000));
        // A newer end replaces it.
        m.match_end_ms = Some(9_000);
        m.observe_match_end();
        assert_eq!(m.last_match_end_ms(), Some(9_000));
        // Another account's end is not this account's.
        m.self_puuid = Some("other".into());
        assert_eq!(m.last_match_end_ms(), None);
    }

    #[test]
    fn a_matches_phase_held_by_the_other_hold_waits_for_it_and_uses_no_retry() {
        // R160 through R165: nothing was sent, so nothing is spent.
        let mut m = settled_lobby(&["me"]);
        m.note_matches_held("me", 10 * S);
        assert!(!m.has_pending_work(), "waiting is not work");
        m.advance_clock(11 * S);
        assert_eq!(m.due_rank_retry(), None, "1.5 s of slack after the hold");
        m.advance_clock(S);
        assert_eq!(m.due_rank_retry().as_deref(), Some("me"));
        let id = m.try_start_worker().expect("due");
        assert_eq!(m.next_job(id), Some(Job::RankRetry("me".into())));
        assert_eq!(m.rank_retries.retries_done("me"), Some(1));
        // Held again on the retry: the turn is given back.
        m.note_rank_held("me", 5 * S, true);
        assert_eq!(m.rank_retries.retries_done("me"), Some(0));
    }

    #[test]
    fn a_matches_hold_leaves_a_round_already_pending_alone_and_ignores_strangers() {
        let mut m = settled_lobby(&["me"]);
        m.note_rank_sync("me", true, false);
        m.advance_clock(15 * S);
        m.note_matches_held("me", 100 * S);
        m.advance_clock(5 * S);
        assert_eq!(m.due_rank_retry().as_deref(), Some("me"), "20 s round kept");
        let mut m = settled_lobby(&["me"]);
        m.note_matches_held("ghost", S);
        m.stage = Stage::Closed;
        m.note_matches_held("me", S);
        m.stage = Stage::Menus;
        m.advance_clock(100 * S);
        assert_eq!(m.due_rank_retry(), None);
    }

    #[test]
    fn staggered_names_retries_are_each_woken_and_nothing_else_is_blocked() {
        // R165 fix rounds: `a` is on its second failure (due +6 s) while `c`,
        // who joined later, is on its first (due +2 s). After every names
        // step the wake-up goes to the earliest still-waiting retry of a
        // player who still needs a name, so the tick wakes a worker for `c`
        // and then again for `a`. The wake-up blocks nothing.
        let mut m = model(vec![slot("me", false), slot("a", false)]);
        m.synced.extend(["me".to_string(), "a".to_string()]);
        m.names_attempted.insert("me".into());
        assert_eq!(m.note_names_failed(&["a".to_string()]), Some(2 * S));
        m.arm_names_retry_wake();
        assert!(!m.names_wakeup_due());
        m.advance_clock(2 * S);
        assert!(m.names_wakeup_due(), "a is due");
        m.slots.push(slot("c", false));
        m.synced.insert("c".into());
        m.note_names_failed(&["a".to_string(), "c".to_string()]);
        m.arm_names_retry_wake();
        assert!(!m.names_wakeup_due(), "c's first failure: +2 s");
        m.advance_clock(2 * S);
        assert!(m.names_wakeup_due(), "c is due");
        assert_eq!(m.names_needed(), vec![("c".to_string(), false)]);
        // c answers; a is still waiting and gets its own wake-up.
        m.names_attempted.insert("c".into());
        m.arm_names_retry_wake();
        m.advance_clock(3 * S);
        assert!(!m.names_wakeup_due());
        m.advance_clock(S);
        assert!(m.names_wakeup_due(), "a is due");
        assert_eq!(m.names_needed(), vec![("a".to_string(), false)]);
        // Nothing is waiting any more: nothing to arm.
        m.names_attempted.insert("a".into());
        m.arm_names_retry_wake();
        m.advance_clock(1_000 * S);
        assert!(!m.names_wakeup_due());
    }

    #[test]
    fn a_player_who_joins_while_a_retry_waits_is_needed_at_once() {
        let mut m = model(vec![slot("me", false), slot("a", false)]);
        m.synced.extend(["me".to_string(), "a".to_string()]);
        m.names_attempted.insert("me".into());
        m.note_names_failed(&["a".to_string()]);
        m.arm_names_retry_wake();
        m.slots.push(slot("d", false));
        assert_eq!(
            m.names_needed(),
            vec![("d".to_string(), false)],
            "d is asked now, a waits for its retry"
        );
        // The older retry still wakes on time.
        m.names_attempted.insert("d".into());
        m.arm_names_retry_wake();
        m.advance_clock(2 * S);
        assert!(m.names_wakeup_due());
        assert_eq!(m.names_needed(), vec![("a".to_string(), false)]);
    }

    #[test]
    fn the_retry_wake_ignores_players_who_left_or_no_longer_need_a_name() {
        let mut m = model(vec![slot("me", false), slot("a", false), slot("b", false)]);
        m.names_attempted.insert("me".into());
        m.note_names_failed(&["a".to_string(), "b".to_string()]);
        m.slots.retain(|s| s.roster.puuid != "a");
        m.known_names.insert("b".into(), ("B".into(), "T".into()));
        m.arm_names_retry_wake();
        m.advance_clock(100 * S);
        assert!(!m.names_wakeup_due(), "nobody left who needs one");
    }
}
