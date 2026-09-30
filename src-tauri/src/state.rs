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
    MmrInput, PartyMark, PlayerCard, PlayerDetails, Team,
};
use opd1_core::party::detect_parties;
use opd1_core::presence::Stage;
use opd1_core::stats::Act;
use opd1_riot::assets::AssetCache;
use opd1_riot::to_core::{current_rank_from_mmr, seasonal_map_from_mmr};
use opd1_riot::RiotSession;

use crate::logbuf::short;
use crate::rank_retry::{RankRetries, RANK_RETRY_DELAYS};
use crate::roster::{current_act, RawLoadout, RosterPlayer};
use crate::store::{PlayerDb, StoredPlayer, SELF_LOADOUT_KEY};

/// A failed asset-table load isn't retried more often than this (offline
/// first run: `meta()` and every `img` request would otherwise each hit
/// valorant-api again).
const TABLES_RETRY_AFTER: Duration = Duration::from_secs(30);

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

/// One unit of work for the roster worker (see [`LiveModel::next_job`]).
#[derive(Debug, Clone, PartialEq)]
pub enum Job {
    /// One `names()` call: `(puuid, incognito)` pairs — `incognito` is
    /// always `false` (filtered), carried so the worker builds each
    /// `VisiblePuuid` from the real flag.
    Names(Vec<(String, bool)>),
    /// The per-player sync of one puuid.
    Sync(String),
    /// R155: the sync of one puuid again, because its rank part failed
    /// (`LiveModel::due_rank_retry`). Handed out after the first-pass syncs
    /// and before the names fill and the backfill.
    RankRetry(String),
    /// R150: one names-fill request (`sync::fill_names_step`): the queued
    /// asks worth sending, in one name-service `PUT`. Handed out when no
    /// `Names` or `Sync` job is left and before the backfill, and only
    /// between matches, like the backfill (`LiveModel::between_matches`).
    FillNames,
    /// One step of the own-history backfill (R101): only handed out when no
    /// `Names`, `Sync` or `FillNames` job is left, so it is always behind
    /// live cards, and only in Menus or Closed (R137), never during a match
    /// — a free-for-all one included (`LiveModel::in_match`).
    Backfill,
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
    /// Names from `names()` this run (visible, non-self players only).
    pub known_names: HashMap<String, (String, String)>,
    /// Players whose card showed them hidden the last time they were on
    /// screen this run, with the agent it showed (once locked, R99) — see
    /// [`Self::note_hidden`]. Player Details builds such a player's card
    /// off screen by the same hidden-name rule (after a dodge or the match
    /// end), never with a name-service name from an earlier visible
    /// sighting. Cleared when the account changes.
    pub seen_hidden: HashMap<String, Option<String>>,
    /// Players a `names()` call was attempted for (since the last reset):
    /// a visible name still unknown after that is Missing, not Loading.
    pub names_attempted: HashSet<String>,
    /// Players whose per-player sync ran (since the last reset): a field
    /// still unknown after that is Missing, not Loading.
    pub synced: HashSet<String>,
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
    /// R150: the asks waiting for the names fill (`Job::FillNames`), oldest
    /// first, one per puuid. In memory only: a restart loses them, and the
    /// next match end, Details open or backfill asks again.
    pub name_fill: VecDeque<NameAsk>,
    /// R150: when the last match ended (epoch ms) — In Game to Menus, a
    /// free-for-all one included — until the next one starts. A match stored
    /// meanwhile that the user played in, that started shortly before, is the
    /// match that just ended: its players are asked about
    /// (`AppState::match_end_asks`).
    pub match_end_ms: Option<i64>,
    /// R155: the players whose rank sync failed and are synced again later
    /// (`Self::note_rank_sync`, `Self::due_rank_retry`). Only players on the
    /// live roster; dropped with a roster change, an account switch, and when
    /// VALORANT closes.
    pub rank_retries: RankRetries,
    /// Test clock: added to "now" wherever the retries are timed, so a test
    /// moves time forward instead of sleeping (`Self::advance_clock`).
    #[cfg(test)]
    clock_offset: Duration,
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
            known_names: HashMap::new(),
            seen_hidden: HashMap::new(),
            names_attempted: HashSet::new(),
            synced: HashSet::new(),
            party_ready: HashSet::new(),
            worker_id: None,
            next_worker_id: 0,
            backfill: None,
            name_fill: VecDeque::new(),
            match_end_ms: None,
            rank_retries: RankRetries::default(),
            #[cfg(test)]
            clock_offset: Duration::ZERO,
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

    /// A `Names`, `Sync` or (R155) due `RankRetry` job is waiting: what the
    /// backfill and the names fill must yield to.
    pub fn live_job_pending(&self) -> bool {
        !self.names_needed().is_empty()
            || self.next_unsynced().is_some()
            || self.due_rank_retry().is_some()
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

    /// R150: a names fill is queued and it is between matches. The asks wait
    /// in the queue through a match and run after it.
    pub fn fill_ready(&self) -> bool {
        !self.name_fill.is_empty() && self.between_matches()
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
        self.live_job_pending() || self.fill_ready() || self.backfill_ready()
    }

    /// R150: queues asks for the names fill, one per puuid. Asks
    /// [`Self::fill_allowed`] refuses (the user, anyone shown hidden) are not
    /// queued at all — and are checked again when taken. Asked twice, the
    /// newer match's ask stays (an ask raised by a finished match covers a
    /// puuid asked before; an on-demand one never overrides it). A full queue
    /// ([`NAME_FILL_QUEUE_MAX`]) takes no new puuid.
    pub fn queue_name_fill(&mut self, asks: impl IntoIterator<Item = NameAsk>) {
        let hidden = self.hidden_players();
        for ask in asks {
            if !self.fill_allowed_in(&hidden, &ask.puuid) {
                continue;
            }
            if let Some(queued) = self.name_fill.iter_mut().find(|q| q.puuid == ask.puuid) {
                // `None` (on demand) is older than any match.
                queued.match_start_ms = queued.match_start_ms.max(ask.match_start_ms);
            } else if self.name_fill.len() < NAME_FILL_QUEUE_MAX {
                self.name_fill.push_back(ask);
            }
        }
    }

    /// R150 with R16: whether the names fill may ask about `puuid` at all. Not
    /// the user (their name comes from the chat session, R43), not an empty
    /// id, and never a player shown hidden on the live screen now
    /// (`hidden_players`) or earlier this run (`seen_hidden`). Riot's
    /// name-service answers blank for anyone hiding their name, but the
    /// question itself is not sent (CLAUDE.md "Never request a hidden
    /// player's name").
    pub fn fill_allowed(&self, puuid: &str) -> bool {
        self.fill_allowed_in(&self.hidden_players(), puuid)
    }

    fn fill_allowed_in(&self, hidden: &HashMap<String, Option<String>>, puuid: &str) -> bool {
        !puuid.is_empty()
            && puuid != self.self_id()
            && !hidden.contains_key(puuid)
            && !self.seen_hidden.contains_key(puuid)
    }

    /// R150: takes up to `n` asks off the queue, oldest first. Asks that
    /// [`Self::fill_allowed`] refuses are dropped on the way (and don't count
    /// towards `n`); they are not kept for later — a later trigger asks again
    /// if the puuid is fair game then.
    pub fn take_fill(&mut self, n: usize) -> Vec<NameAsk> {
        let hidden = self.hidden_players();
        let mut taken = Vec::new();
        while taken.len() < n {
            let Some(ask) = self.name_fill.pop_front() else {
                break;
            };
            if self.fill_allowed_in(&hidden, &ask.puuid) {
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
    /// The Match Details the UI has open, if any (R150): replaced by the next
    /// `match_details` call, dropped when the Player Details is opened again
    /// (Back from the match). The UI has no signal for leaving the match
    /// page for Live, so it can outlive the page: a rebuild is one DB read,
    /// and it emits only on a change.
    open_match: Mutex<Option<OpenMatch>>,
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
            acts: RwLock::new(Arc::new(Vec::new())),
            tables: tokio::sync::Mutex::new(TablesStatus::default()),
            details_synced: Mutex::new(HashMap::new()),
            skins_inflight: Mutex::new(HashMap::new()),
            open_details: Mutex::new(None),
            details_inflight: Mutex::new(HashMap::new()),
            open_match: Mutex::new(None),
            worker_kick: AtomicBool::new(false),
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

    /// Records `details` as the open Player Details, as the UI was just
    /// given them (the `player_details` answer). Opening a Player Details
    /// also closes any Match Details: the UI only gets here from the match
    /// page by going Back.
    pub fn set_open_details(&self, details: &PlayerDetails) {
        *lock(&self.open_details) = Some(details.clone());
        *lock(&self.open_match) = None;
    }

    /// Records `view` as the open Match Details, as the UI was just given it
    /// (the `match_details` answer).
    pub fn set_open_match(&self, match_id: &str, focus_puuid: &str, view: &MatchDetailsView) {
        *lock(&self.open_match) = Some(OpenMatch {
            match_id: match_id.to_string(),
            focus_puuid: focus_puuid.to_string(),
            view: view.clone(),
        });
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
        let live = lock(&self.live);
        let (Some(me), Some(end)) = (live.self_puuid.as_deref(), live.match_end_ms) else {
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

    /// The player whose Details the UI has open, if any.
    pub fn open_details_puuid(&self) -> Option<String> {
        lock(&self.open_details)
            .as_ref()
            .map(|d| d.card.puuid.clone())
    }

    /// The UI left the Details page (`close_details`): nothing is open, so
    /// no rebuild is done or emitted for it until the next open.
    pub fn close_details(&self) {
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

/// The Match Details the UI asked for last (`match_details`), as it was last
/// given to the UI (R150): rebuilt and re-emitted when names arrive.
struct OpenMatch {
    match_id: String,
    focus_puuid: String,
    view: MatchDetailsView,
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
    fn backfill_job_is_lowest_priority() {
        let mut m = model(vec![slot("me", false), slot("p1", false)]);
        m.stage = Stage::Menus;
        m.start_backfill(None, None);
        let id = m.try_start_worker().expect("work pending");

        // A name is needed and slots are unsynced: both come before the run.
        assert!(m.live_job_pending());
        assert!(matches!(m.next_job(id), Some(Job::Names(_))));
        m.names_attempted.extend(["p1".to_string()]);
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
        assert_eq!(m.next_job(id), Some(Job::Sync("p2".into())));
        m.synced.insert("p2".into());
        // The due retry goes before the names fill and the backfill.
        assert_eq!(m.next_job(id), Some(Job::RankRetry("p1".into())));
        m.note_rank_sync("p1", false, true);
        assert_eq!(m.next_job(id), Some(Job::FillNames));
        m.name_fill.clear();
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
    fn the_fill_job_comes_after_live_cards_and_before_the_backfill() {
        let mut m = model(vec![slot("me", false), slot("p1", false)]);
        m.stage = Stage::Menus;
        m.start_backfill(None, None);
        m.queue_name_fill(vec![ask("x", Some(1))]);
        let id = m.try_start_worker().expect("work pending");

        assert!(matches!(m.next_job(id), Some(Job::Names(_))));
        m.names_attempted.insert("p1".into());
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
        assert_eq!(m.next_job(id), Some(Job::Sync("late".into())));
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
    fn take_fill_never_hands_out_the_user_or_anyone_hidden() {
        // R150 with R16: a puuid shown hidden on the live screen now
        // (`hidden_players`), one shown hidden earlier this run
        // (`seen_hidden`), the user, and an empty id are dropped from the
        // queue — never asked, and not kept for later.
        let mut m = model(vec![
            slot("me", false),
            slot("live-hidden", true),
            slot("visible", false),
        ]);
        m.stage = Stage::Menus;
        m.seen_hidden.insert("was-hidden".into(), None);
        // Straight into the queue: it is `take_fill` that is under test (the
        // player may have turned hidden after the ask was queued).
        m.name_fill.extend([
            ask("live-hidden", Some(1)),
            ask("was-hidden", Some(1)),
            ask("me", Some(1)),
            ask("", Some(1)),
            ask("visible", Some(1)),
            ask("stranger", None),
        ]);
        let taken: Vec<String> = m.take_fill(10).into_iter().map(|a| a.puuid).collect();
        assert_eq!(taken, ["visible", "stranger"]);
        assert!(m.name_fill.is_empty(), "the hidden asks are dropped");
        // The same rule as a check, for the last look before a request.
        assert!(!m.fill_allowed("live-hidden"));
        assert!(!m.fill_allowed("was-hidden"));
        assert!(!m.fill_allowed("me"));
        assert!(m.fill_allowed("stranger"));
    }

    #[test]
    fn hidden_players_are_not_even_queued() {
        let mut m = model(vec![slot("me", false), slot("live-hidden", true)]);
        m.seen_hidden.insert("was-hidden".into(), None);
        m.queue_name_fill(vec![
            ask("live-hidden", None),
            ask("was-hidden", Some(1)),
            ask("me", Some(1)),
            ask("", None),
            ask("visible", None),
        ]);
        assert_eq!(queued(&m), [("visible", None)]);
    }

    #[test]
    fn take_fill_takes_at_most_n_in_arrival_order() {
        let mut m = model(vec![]);
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
}
