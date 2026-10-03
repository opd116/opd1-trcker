//! Per-player "sync, don't refetch" (CLAUDE.md "Smart data", R33) and the
//! background worker that walks the live roster.
//!
//! Per player, in two phases (R165):
//!
//! *Rank phase* ([`sync_rank`]):
//! 1. `comp_updates(0..5)` — the last-5 competitive match ids + RR deltas
//!    (one mmr-bucket call; upserted, and indexed as competitive history).
//! 2. `mmr` only when `mmr_refresh_needed(latest comp match)` — no new
//!    competitive match means the rank can't have changed, unless the stored
//!    snapshot predates the current snapshot version (refetched once). When
//!    `comp_updates` failed there is no "latest" to compare (R160): `mmr` is
//!    then asked for a player with no stored snapshot, and skipped when one
//!    exists (it keeps showing).
//!    The card is stored and published right after (rank, peak, win rate).
//!
//! *Matches phase* ([`sync_matches`]):
//! 3. `match_history(0..2, any queue)` — each player's last two matches in
//!    any mode (Task 6's party detection needs them; indexed).
//! 4. `match_details` only for ids not already in the DB (immutable —
//!    fetched once, stored forever as compact rows), up to
//!    [`DETAILS_CONCURRENCY`] in flight (the session's token bucket still
//!    paces the actual requests). The ids are the player's newest stored
//!    competitive updates and the history's.
//!
//! The worker runs the rank phase for the whole roster first (up to
//! [`RANK_CONCURRENCY`](crate::state::RANK_CONCURRENCY) players at once, one
//! `Job::Rank`), then the matches phases one player at a time in display
//! order, so no card waits for another player's match-details downloads
//! (R165). The request count is unchanged.
//!
//! Behind all of that, at the lowest priority, the worker also runs the
//! user's own-history backfill ([`backfill_step`], R101), behind that the
//! user's own competitive updates, one page a job ([`comp_backfill_step`],
//! B1: My Stats' chart wants the whole act), and, ahead of both,
//! the names fill ([`fill_names_step`], R150): between matches only, one
//! batched name-service request for the players of a match that just ended,
//! of a Details page, or of the matches the backfill stored — including the
//! players who hid their name during a match (R153: the game's Match History
//! shows their real names after it) and never while a match is running.
//!
//! Any failure other than `ClientGone` is logged and skipped: the fields
//! it would have filled become Missing (`—`) through the card builder's
//! "attempted" rule, never a stuck skeleton. `ClientGone` aborts the whole
//! run — the poll loop notices the client is gone on its own next tick.
//!
//! A failed rank part (`comp_updates` or `mmr`) or a partial matches phase (the
//! history call or a match-details download failed, R165) is retried, bounded
//! (R155, `crate::rank_retry`): the worker syncs the player again (both
//! phases; only the missing ids are fetched) 20 s after the failure and 60 s
//! after that, while they are on the live roster, behind the first-pass syncs
//! and ahead of the names fill and the backfill. A rank part that did not run
//! only because the mmr request family was on hold after a Riot 429 (R160,
//! `RiotError::Held`: nothing was sent) is not a failure: the player is synced
//! again when the hold ends and no retry is used up. The worker never waits
//! for a hold.
//!
//! A failed live names lookup (R165) is not an answer either: the player is
//! asked again after a short delay, at most three tries a stage
//! ([`names_step`], `LiveModel::note_names_failed`).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use tokio::task::JoinSet;

use opd1_core::db::repo::{CompUpdateRow, HistoryEntry, MmrSnapshot, NameAsk};
use opd1_core::presence::{decode_presence, Stage};
use opd1_riot::endpoints::{MatchDetailsResponse, MatchHistoryResponse};
use opd1_riot::to_core::{comp_update_rows_from_response, match_from_details};
use opd1_riot::{RiotError, RiotSession, VisiblePuuid};

use crate::logbuf::short;
use crate::rank_retry::HOLD_SLACK;
use crate::roster::{extract_loadouts, extract_player_loadout, own_valorant_private};
use crate::state::{
    lock, now_ms, unnamed_others, wait_for_details_fetch, AppState, BackfillRun, DetailsClaim,
    DetailsFetchGuard, Job,
};
use crate::store::{
    mmr_refresh_needed, recent_comp_updates, save_player, trim_mmr, PlayerUpdate, SELF_LOADOUT_KEY,
};

/// Match-details requests in flight at once. They're large responses
/// (hundreds of KB each); a small window overlaps latency without holding
/// many bodies in memory.
pub const DETAILS_CONCURRENCY: usize = 4;

/// Player Details: how many recent competitive matches to show (CLAUDE.md:
/// "up to 8 total").
pub const RECENT_COUNT: u32 = 8;

/// Player Details: how many competitive updates to scan for the current
/// act's teammates (CLAUDE.md: "the last 20 current-act comp matches").
pub const TEAMMATE_WINDOW: u32 = 20;

/// R101: match-history's default page (techchrism `MatchHistory.ts`:
/// `endIndex` defaults to 20); the backfill requests pages of this size.
/// Riot may answer with fewer entries, so the cursor advances by what a page
/// actually held, and only `Total` (or an empty page) ends the history — a
/// short page counts as the end only when `Total` is missing
/// ([`history_ended`]).
pub const BACKFILL_PAGE: u32 = 20;

/// R101: the first run's cap on history entries looked at, when the current
/// act's start doesn't stop it sooner (Claude's lightness bound).
pub const BACKFILL_FIRST_RUN_MAX: u32 = 200;

/// R150: the most puuids one backfill run asks name-service about, whatever
/// it stored (Claude's lightness bound): two requests. The newest matches'
/// players come first; older players are asked at their next match end or
/// when a Details page needs them.
pub const BACKFILL_FILL_MAX: usize = 200;

/// The `app_state` key prefix of the backfill anchors (see [`anchor_key`]).
const ANCHOR_KEY_PREFIX: &str = "self_history_anchor";

/// R137: the `app_state` key holding `puuid`'s backfill anchor: the newest
/// match the account's last *finished* run saw, JSON
/// `{"matchId": "…", "startMs": N}`. One key per account, so an account
/// switch neither deletes an anchor nor lets one account's anchor bound
/// another's run, and switching back finds the first account's anchor
/// again. (The pre-R137 single key `self_history_anchor` is no longer read;
/// its one row stays, unused — the account's next run is a first run.)
pub fn anchor_key(puuid: &str) -> String {
    format!("{ANCHOR_KEY_PREFIX}:{puuid}")
}

/// B1: how many competitive updates one page asks for (`startIndex` to
/// `endIndex`; the endpoint's page, as `TEAMMATE_WINDOW` already reads).
pub const COMP_BACKFILL_PAGE: u32 = 20;

/// B1: the most pages one own competitive-updates run fetches, 200 rows
/// (Claude's lightness bound; the act start or the anchor usually stops it
/// sooner).
pub const COMP_BACKFILL_MAX_PAGES: u32 = 10;

/// Review round 1: the least time between two pages of one run (the job
/// never sleeps: the run is not ready until then, `LiveModel::pause_comp_backfill`,
/// and the poll tick wakes a worker). A first run would otherwise take 10 of
/// the mmr bucket's 24 burst tokens back to back, in front of a roster's rank
/// phase if a match starts soon.
pub const COMP_BACKFILL_PAGE_GAP: Duration = Duration::from_millis(2_500);

/// The `app_state` key prefix of the competitive-updates backfill anchors.
const COMP_ANCHOR_KEY_PREFIX: &str = "self_comp_anchor";

/// B1: the `app_state` key holding `puuid`'s competitive-updates anchor: the
/// newest row the account's last *finished* run saw, the same JSON as
/// [`anchor_key`]'s. One key per account, for the same reasons (R137).
pub fn comp_anchor_key(puuid: &str) -> String {
    format!("{COMP_ANCHOR_KEY_PREFIX}:{puuid}")
}

/// The `app_state` key prefix of the backfill failure counts (see
/// [`failures_key`]).
const FAILURES_KEY_PREFIX: &str = "self_history_failures";

/// R137: the `app_state` key holding `puuid`'s per-match failure counts of
/// the backfill, JSON `{"<match id>": N}`: how many runs in a row ended at
/// that id because its match-details failed ([`counts_toward_skip`]). One
/// key per account, like [`anchor_key`], so another account's finished run
/// never resets them. Cleared when that account's run finishes. (The
/// pre-fix single key `self_history_failures` is no longer read.)
pub fn failures_key(puuid: &str) -> String {
    format!("{FAILURES_KEY_PREFIX}:{puuid}")
}

/// R137: a match whose match-details failed in this many runs in a row
/// (with a failure that [`counts_toward_skip`]) is skipped, logged, and the
/// anchor passes it.
pub const BACKFILL_MAX_FAILURES: u32 = 3;

/// R139: whether a failed match-details fetch says something about the id
/// itself, and so counts toward the [`BACKFILL_MAX_FAILURES`] skip: a Riot
/// status other than 404 (skipped at once) and 429 (the rate limit), or a
/// body that doesn't decode. Rate-limit (`RateLimited`), network (`Http`)
/// and auth errors (`AuthFailed`, and a 401/403 still there after the
/// session's re-auth) — and `ShardsUnavailable` / `ClientGone` — end the run
/// without counting: a network outage across three runs must not skip ids
/// for good (the gap R101 forbids).
pub fn counts_toward_skip(e: &RiotError) -> bool {
    match e {
        RiotError::Status(code, _) => !matches!(code, 401 | 403 | 404 | 429),
        RiotError::Decode(_) => true,
        RiotError::ClientGone
        | RiotError::ShardsUnavailable
        | RiotError::AuthFailed(_)
        | RiotError::RateLimited
        | RiotError::Held(_)
        | RiotError::Http(_) => false,
    }
}

/// The Riot client went away mid-sync.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Gone;

/// What a rank phase learned beyond the DB writes (R165).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RankOutcome {
    /// The rank part really failed: the `comp_updates` call or the `mmr` call
    /// answered with an error (any but `ClientGone`, which aborts the sync
    /// instead, and `Held`, which is `held`). R155: such a player is synced
    /// again later (`LiveModel::note_rank_sync`).
    pub failed: bool,
    /// R160: the rank part did not run, only because the mmr request family
    /// was on hold — nothing was sent — and this is the longest of what was
    /// left of the hold. `None` when a call really failed (`failed`) or
    /// nothing was held. Not a failure: `LiveModel::note_rank_held` syncs the
    /// player again when the hold ends and uses up no retry.
    pub held: Option<Duration>,
}

/// What a matches phase learned beyond the DB writes (R165).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MatchesOutcome {
    /// The player's match-history call answered *and* was written to
    /// SQLite. Party detection reads the stored history, so a player whose
    /// history failed this cycle (the call or the write) must not take part:
    /// the stored one may be from an earlier session (a false stripe).
    pub history_ok: bool,
    /// The phase did not get everything: the history call failed, or a
    /// match-details download did (or its rows could not be stored). R165:
    /// such a player is synced again later (`LiveModel::note_partial_sync`),
    /// and the retry fetches only what is still missing.
    pub partial: bool,
    /// R160: a call of the phase (the history, a match-details download) was
    /// refused only because the `other` request family was on hold — nothing
    /// was sent (`RiotError::Held`, when more than 10 s of its hold was
    /// left) — and this is the longest of what was left. Not a failure and
    /// not `partial`: the player is synced again when the hold ends and no
    /// R155 retry is used up (`LiveModel::note_matches_held`).
    pub held: Option<Duration>,
}

/// What a whole per-player sync learned ([`sync_player`]).
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncOutcome {
    pub rank: RankOutcome,
    pub matches: MatchesOutcome,
}

/// The warn line a failed call leaves in the log (R158): what failed, for
/// whom — the short puuid only — and why: the HTTP status with Riot's own
/// answer (its `errorCode`), `rate-limited`, the kind of transport failure, or
/// that the body did not parse. All of it is the error's own text, which the
/// riot crate keeps free of URLs, tokens and full puuids; this only adds the
/// short puuid.
fn failure_line(what: &str, puuid: &str, err: &RiotError) -> String {
    format!("{what} for {} failed: {err}", short(puuid))
}

/// `ClientGone` -> [`Gone`]; anything else is logged and swallowed.
fn check(err: RiotError, what: &str, puuid: &str) -> Result<(), Gone> {
    match err {
        RiotError::ClientGone => Err(Gone),
        other => {
            log::warn!("{}", failure_line(what, puuid, &other));
            Ok(())
        }
    }
}

/// How a rank-part call (`comp_updates`, `mmr`) ended, for [`sync_player`].
enum RankCall<T> {
    Ok(T),
    /// Not sent: the mmr family is on hold for this much longer (R160).
    Held(Duration),
    /// Failed for real (logged).
    Failed,
}

/// Maps a rank call's result: `ClientGone` aborts, `Held` is noted (debug
/// only: the hold itself was logged when it started), anything else is logged
/// like [`check`] and counts as a real failure.
fn rank_call<T>(res: Result<T, RiotError>, what: &str, puuid: &str) -> Result<RankCall<T>, Gone> {
    match res {
        Ok(v) => Ok(RankCall::Ok(v)),
        Err(RiotError::Held(left)) => {
            log::debug!("{}", failure_line(what, puuid, &RiotError::Held(left)));
            Ok(RankCall::Held(left))
        }
        Err(e) => check(e, what, puuid).map(|()| RankCall::Failed),
    }
}

/// What the rank part of one sync learned: a real failure and/or a hold.
#[derive(Default)]
struct RankPart {
    failed: bool,
    held: Option<Duration>,
}

impl RankPart {
    fn held(&mut self, left: Duration) {
        self.held = Some(self.held.map_or(left, |h| h.max(left)));
    }

    /// A hold only counts when nothing failed for real (a real failure
    /// schedules the R155 retry, which covers the hold).
    fn outcome(&self) -> RankOutcome {
        RankOutcome {
            failed: self.failed,
            held: self.held.filter(|_| !self.failed),
        }
    }
}

/// Order-preserving de-duplication.
pub fn dedupe(ids: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    ids.into_iter()
        .filter(|id| seen.insert(id.clone()))
        .collect()
}

/// The newest competitive match id among `rows`, or `""` when the player
/// has none — stored as the MMR snapshot's `last_comp_match_id`, so a
/// player with no competitive history gets one MMR fetch, not one per
/// sync.
pub fn latest_comp_match(rows: &[CompUpdateRow]) -> String {
    rows.iter()
        .max_by_key(|r| r.start_ms)
        .map(|r| r.match_id.clone())
        .unwrap_or_default()
}

/// Player Details' match-details targets from a `comp_updates(0..20)`
/// response: the newest [`RECENT_COUNT`] (any act, for "Recent") plus
/// every one in the current act (for "Top Teammates").
pub fn details_targets(rows: &[CompUpdateRow], current_season: Option<&str>) -> Vec<String> {
    let mut sorted: Vec<&CompUpdateRow> = rows.iter().collect();
    sorted.sort_by_key(|r| std::cmp::Reverse(r.start_ms));
    let recent = sorted
        .iter()
        .take(RECENT_COUNT as usize)
        .map(|r| r.match_id.clone());
    let act = sorted
        .iter()
        .filter(|r| Some(r.season_id.as_str()) == current_season)
        .map(|r| r.match_id.clone());
    dedupe(recent.chain(act).collect())
}

fn history_entries(resp: &MatchHistoryResponse) -> Vec<HistoryEntry> {
    resp.history
        .iter()
        .map(|h| HistoryEntry {
            match_id: h.match_id.clone(),
            queue_id: h.queue_id.clone(),
            start_ms: h.game_start_time,
        })
        .collect()
}

fn comp_history_entries(rows: &[CompUpdateRow]) -> Vec<HistoryEntry> {
    rows.iter()
        .map(|r| HistoryEntry {
            match_id: r.match_id.clone(),
            queue_id: "competitive".into(),
            start_ms: r.start_ms,
        })
        .collect()
}

/// `comp_updates(start..end)` -> DB, telling a hold apart from a failure
/// (R160).
async fn comp_updates_call(
    st: &Arc<AppState>,
    session: &RiotSession,
    puuid: &str,
    end: u32,
) -> Result<RankCall<Vec<CompUpdateRow>>, Gone> {
    match rank_call(
        session.comp_updates(puuid, Some(0), Some(end)).await,
        "comp updates",
        puuid,
    )? {
        RankCall::Ok(resp) => {
            let rows = comp_update_rows_from_response(&resp);
            let (p, r) = (puuid.to_string(), rows.clone());
            let stored = st
                .db(move |repo| {
                    repo.upsert_comp_updates(&p, &r)?;
                    repo.index_history(&p, &comp_history_entries(&r))
                })
                .await;
            // The user's newest rows, now stored: the competitive-updates
            // backfill's first page is skipped when they hold its anchor.
            if stored.is_some() && puuid == session.self_puuid() {
                let head = rows
                    .iter()
                    .map(|r| (r.match_id.clone(), r.start_ms))
                    .collect();
                lock(&st.live).own_comp_head = Some((puuid.to_string(), head));
            }
            Ok(RankCall::Ok(rows))
        }
        RankCall::Held(left) => Ok(RankCall::Held(left)),
        RankCall::Failed => Ok(RankCall::Failed),
    }
}

/// A match-details response -> the compact rows stored in SQLite (~1-2 KB;
/// the large body is dropped before the write). `None`: the write failed
/// (`AppState::db` already logged it); otherwise the match's start and the
/// puuids of its players other than the user whose names match-details left
/// blank, for the own-history backfill's names fill. Shared by the live sync
/// ([`fetch_missing_details`]) and the backfill.
///
/// R150: when the stored match is the one that just ended
/// ([`AppState::match_end_asks`]), its players are queued for the names fill
/// right here, once the rows are stored — whichever path fetched it.
pub async fn store_details(
    st: &Arc<AppState>,
    details: MatchDetailsResponse,
) -> Option<(i64, Vec<String>)> {
    let (m, teams, players) = match_from_details(&details);
    drop(details);
    let asks = st.match_end_asks(&m, &players);
    let start_ms = m.start_ms;
    let others: Vec<String> = {
        let me = lock(&st.live).self_puuid.clone().unwrap_or_default();
        unnamed_others(&players, &me).collect()
    };
    st.db(move |repo| repo.upsert_match(&m, &teams, &players))
        .await?;
    st.queue_name_fill(asks);
    Some((start_ms, others))
}

/// Fetches match-details for every id the DB doesn't have yet and stores
/// the compact rows. An id another task is downloading right now (a
/// Details sync and the worker's sync of the same player, the backfill,
/// Match Details) is not downloaded again: this call waits for that
/// download instead, so it still returns with the rows stored.
pub async fn fetch_missing_details(
    st: &Arc<AppState>,
    session: &Arc<RiotSession>,
    ids: Vec<String>,
) -> Result<(), Gone> {
    fetch_missing_details_full(st, session, ids)
        .await
        .map(|_| ())
}

/// R167: the low-priority asks one stored match raises: its players whose
/// names match-details left blank, each tagged with the match's start.
fn match_asks(start_ms: i64, others: Vec<String>) -> Vec<NameAsk> {
    others
        .into_iter()
        .map(|puuid| NameAsk {
            puuid,
            match_start_ms: Some(start_ms),
        })
        .collect()
}

/// How a [`fetch_missing_details_full`] went.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DetailsFetch {
    /// R160: a download was refused only because the `other` request family
    /// was on hold (`Held`: nothing sent): the longest of what was left of the
    /// hold. Those ids are not stored; a caller that can run again when the
    /// hold ends does.
    pub held: Option<Duration>,
    /// R165: some id is still not stored for a real reason — a download
    /// failed (anything but a gone client, which is `Err`, and a hold), its
    /// rows could not be written, or the download this call waited for did
    /// not end up stored. A 429 Riot actually sent counts (R165 ruled it a
    /// real failure for the live sync's retries); see `rate_limited`.
    pub incomplete: bool,
    /// Review round 1: a download got a 429 Riot actually sent
    /// (`RiotError::RateLimited`; the other download refused locally meanwhile
    /// are `held`): what is left of the `other` hold it set (zero when it is
    /// over already). The id is not stored, and the caller that can run again
    /// when the hold ends treats it like `held`.
    pub rate_limited: Option<Duration>,
    /// `incomplete` for a reason other than a sent 429 (and other than a
    /// hold in effect for a download this call only waited for).
    pub failed: bool,
}

/// [`fetch_missing_details`] that also says how the fetch went ([`DetailsFetch`]).
pub async fn fetch_missing_details_full(
    st: &Arc<AppState>,
    session: &Arc<RiotSession>,
    ids: Vec<String>,
) -> Result<DetailsFetch, Gone> {
    fetch_missing_details_asking(st, session, ids, false).await
}

/// [`fetch_missing_details_full`]; with `ask_names` (R167: a roster player's
/// matches phase) the players of each match it stores whose names were blank
/// are queued for the names fill at low priority **as the match is stored**,
/// so a client that goes away mid-call loses none of the stored matches' asks.
async fn fetch_missing_details_asking(
    st: &Arc<AppState>,
    session: &Arc<RiotSession>,
    ids: Vec<String>,
    ask_names: bool,
) -> Result<DetailsFetch, Gone> {
    let ids = dedupe(ids);
    if ids.is_empty() {
        return Ok(DetailsFetch::default());
    }
    let Some(missing) = st.db(move |repo| repo.new_match_ids(&ids)).await else {
        return Ok(DetailsFetch::default());
    };
    if missing.is_empty() {
        return Ok(DetailsFetch::default());
    }
    let mut claimed: HashMap<String, DetailsFetchGuard> = HashMap::new();
    let mut busy = Vec::new();
    for id in missing {
        match st.claim_details_fetch(&id) {
            DetailsClaim::Mine(guard) => {
                claimed.insert(id, guard);
            }
            DetailsClaim::Busy(waiter) => busy.push((id, waiter)),
        }
    }
    log::debug!(
        "fetching {} match details ({} already in flight)",
        claimed.len(),
        busy.len()
    );
    let mine: Vec<String> = claimed.keys().cloned().collect();

    let mut pending = mine.into_iter();
    let mut set = JoinSet::new();
    let spawn_next = |set: &mut JoinSet<_>, pending: &mut std::vec::IntoIter<String>| {
        if let Some(id) = pending.next() {
            let s = session.clone();
            set.spawn(async move {
                let res = s.match_details(&id).await;
                (id, res)
            });
        }
    };
    for _ in 0..DETAILS_CONCURRENCY {
        spawn_next(&mut set, &mut pending);
    }

    let mut gone = false;
    let mut failed = false;
    let mut rate_limited: Option<Duration> = None;
    let mut held: Option<Duration> = None;
    while let Some(joined) = set.join_next().await {
        match joined {
            Ok((id, Ok(details))) => {
                // A failed write is logged; the live sync goes on (partial).
                match store_details(st, details).await {
                    Some((start_ms, others)) if ask_names => {
                        st.queue_name_fill_low(match_asks(start_ms, others));
                    }
                    Some(_) => {}
                    None => failed = true,
                }
                // Stored: whoever waits on this id can read it now.
                claimed.remove(&id);
            }
            Ok((_, Err(RiotError::ClientGone))) => {
                gone = true;
                break;
            }
            Ok((id, Err(RiotError::Held(left)))) => {
                log::debug!("match details {} not sent: hold, {left:?} left", short(&id));
                held = Some(held.map_or(left, |h| h.max(left)));
                claimed.remove(&id);
            }
            // Riot no longer has the match (404): terminal, like the backfill
            // (R137) treats it. A retry cannot get it, so the sync is not
            // partial because of it.
            Ok((id, Err(e @ RiotError::Status(404, _)))) => {
                log::warn!("match details {} skipped: {e}", short(&id));
                st.note_details_404(&id);
                claimed.remove(&id);
            }
            // Riot answered 429: the `other` hold it set is what is left to
            // wait for (the session has already logged the 429).
            Ok((id, Err(RiotError::RateLimited))) => {
                let left = session.other_hold_remaining().unwrap_or(Duration::ZERO);
                log::debug!(
                    "match details {} rate-limited: hold, {left:?} left",
                    short(&id)
                );
                rate_limited = Some(rate_limited.map_or(left, |h| h.max(left)));
                claimed.remove(&id);
            }
            Ok((id, Err(e))) => {
                log::warn!("match details {} failed: {e}", short(&id));
                claimed.remove(&id);
                failed = true;
            }
            Err(e) => {
                log::warn!("match details task failed: {e}");
                failed = true;
            }
        }
        spawn_next(&mut set, &mut pending);
    }
    // Released before any wait below: two calls waiting on each other's
    // ids never hold their own.
    drop(claimed);
    if gone {
        return Err(Gone);
    }
    let (waited, waiters): (Vec<String>, Vec<_>) = busy.into_iter().unzip();
    for waiter in waiters {
        wait_for_details_fetch(waiter).await;
    }
    // The downloads this call waited for may have failed in their own task:
    // look again, so a retry is scheduled for them too.
    if !failed && rate_limited.is_none() && !waited.is_empty() {
        let owner = st.clone();
        let still_missing = st
            .db(move |repo| repo.new_match_ids(&waited))
            .await
            // A waited-for id that answered 404 in its own task is terminal.
            .is_some_and(|still| still.iter().any(|id| !owner.details_404(id)));
        if still_missing {
            // The other task's download failed. With the `other` hold in
            // effect that is most likely its 429: wait for the hold like a
            // 429 of ours; otherwise a failure.
            match session.other_hold_remaining() {
                Some(left) => rate_limited = Some(left),
                None => failed = true,
            }
        }
    }
    Ok(DetailsFetch {
        held,
        incomplete: failed || rate_limited.is_some(),
        rate_limited,
        failed,
    })
}

/// The rank phase (R165): `comp_updates(0..5)`, then `mmr` when it is needed.
/// Stores both; the caller publishes the card ([`rank_step`]).
pub async fn sync_rank(
    st: &Arc<AppState>,
    session: &Arc<RiotSession>,
    puuid: &str,
) -> Result<RankOutcome, Gone> {
    // R155/R160: how the rank part ended, for the retry.
    let mut rank = RankPart::default();

    let comp = match comp_updates_call(st, session, puuid, 5).await? {
        RankCall::Ok(rows) => Some(rows),
        RankCall::Held(left) => {
            rank.held(left);
            None
        }
        RankCall::Failed => {
            rank.failed = true;
            None
        }
    };

    // MMR when a new competitive match appeared since the snapshot (or the
    // snapshot is from an older shape). R160: no longer tied to comp_updates
    // having answered. Without its rows there is no "latest match" to compare,
    // so the mmr call is made for a player with no stored snapshot (or when
    // that check cannot run) and skipped when one exists: it keeps showing.
    let (needs, latest) = match &comp {
        Some(rows) => {
            let latest = latest_comp_match(rows);
            let (p, l) = (puuid.to_string(), latest.clone());
            let needs = st
                .db(move |repo| mmr_refresh_needed(repo, &p, &l))
                .await
                .unwrap_or(true);
            (needs, Some(latest))
        }
        None => {
            let p = puuid.to_string();
            let missing = st
                .db(move |repo| repo.load_mmr_snapshot(&p).map(|s| s.is_none()))
                .await
                .unwrap_or(true);
            (missing, None)
        }
    };
    if needs {
        match rank_call(session.mmr(puuid).await, "mmr", puuid)? {
            RankCall::Ok(resp) => {
                let snap = MmrSnapshot {
                    puuid: puuid.to_string(),
                    json: trim_mmr(&resp).to_string(),
                    // `None`: the newest match was never seen, so the next
                    // sync that does see it refreshes the snapshot.
                    last_comp_match_id: latest,
                    fetched_ms: now_ms(),
                };
                st.db(move |repo| repo.save_mmr_snapshot(&snap)).await;
            }
            RankCall::Held(left) => rank.held(left),
            RankCall::Failed => rank.failed = true,
        }
    }
    Ok(rank.outcome())
}

/// The matches phase (R165): `match_history(0..2)`, then match-details for
/// the ids the DB lacks — the history's and the player's newest five stored
/// competitive updates (the rank phase's rows). Stores them; the caller
/// publishes the card ([`matches_step`]).
pub async fn sync_matches(
    st: &Arc<AppState>,
    session: &Arc<RiotSession>,
    puuid: &str,
) -> Result<MatchesOutcome, Gone> {
    let p = puuid.to_string();
    let mut ids: Vec<String> = st
        .db(move |repo| {
            Ok(recent_comp_updates(repo.conn(), &p, 5)?
                .into_iter()
                .map(|r| r.match_id)
                .collect())
        })
        .await
        .unwrap_or_default();

    let mut partial = false;
    let mut held: Option<Duration> = None;
    let mut hold = |left: Duration| held = Some(held.map_or(left, |h| h.max(left)));
    let history_ok = match session.match_history(puuid, Some(0), Some(2), None).await {
        Ok(resp) => {
            let entries = history_entries(&resp);
            ids.extend(entries.iter().map(|e| e.match_id.clone()));
            let p = puuid.to_string();
            // A SQLite failure (`None`) leaves the earlier session's rows.
            let written = st
                .db(move |repo| repo.index_history(&p, &entries))
                .await
                .is_some();
            partial |= !written;
            written
        }
        Err(RiotError::Held(left)) => {
            log::debug!(
                "{}",
                failure_line("match history", puuid, &RiotError::Held(left))
            );
            hold(left);
            false
        }
        Err(e) => {
            check(e, "match history", puuid)?;
            partial = true;
            false
        }
    };
    // A history that did not answer, held or failed, leaves no fresh history
    // (`history_ok`): party marks wait. Only a real failure is `partial`.
    // R167: the players of the matches this phase stores are queued as each
    // is stored (low priority), named between matches like the Details
    // sync's (`LiveModel::take_fill` holds them in a match;
    // `Repo::names_to_ask` drops the known and the already-asked).
    let fetched = fetch_missing_details_asking(st, session, ids, true).await?;
    partial |= fetched.incomplete;
    if let Some(left) = fetched.held {
        hold(left);
    }
    Ok(MatchesOutcome {
        history_ok,
        partial,
        held: held.filter(|_| !partial),
    })
}

/// Both phases back to back, nothing published in between (the tests; the
/// worker publishes after each phase: [`rank_step`], [`matches_step`]).
#[cfg(test)]
pub async fn sync_player(
    st: &Arc<AppState>,
    session: &Arc<RiotSession>,
    puuid: &str,
) -> Result<SyncOutcome, Gone> {
    let rank = sync_rank(st, session, puuid).await?;
    let matches = sync_matches(st, session, puuid).await?;
    Ok(SyncOutcome { rank, matches })
}

/// How [`sync_details`] ended (R160).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailsSync {
    /// The competitive updates answered and the details were fetched.
    Done,
    /// Something was refused only because a request family was on hold
    /// (`RiotError::Held`, nothing sent); this much of the hold is left. The
    /// data is not there yet: run again when the hold ends.
    Held(Duration),
    /// The competitive-updates call failed for real (logged), or a
    /// match-details download did ([`DetailsFetch::incomplete`]): the page is
    /// short of data and the claim goes back.
    Failed,
}

/// Player Details' deeper sync: the last [`TEAMMATE_WINDOW`] competitive
/// updates, then details for the newest 8 + the current act's matches
/// (only those not in the DB). Also picks up the core-game loadouts (the
/// equipped skin ids) if the live loop's fetch of them failed, and then
/// resolves their names from valorant-api (`details::resolve_skins`)
/// before returning, so the caller's rebuild already has them.
///
/// Says how it ended ([`DetailsSync`]): a hold, a failed competitive-updates
/// call and a failed match-details download all leave the data missing, and
/// the caller must not count this run as the page's sync. A failure wins over
/// a hold seen in the same run.
pub async fn sync_details(
    st: &Arc<AppState>,
    session: &Arc<RiotSession>,
    puuid: &str,
) -> Result<DetailsSync, Gone> {
    let season = st.current_act().map(|a| a.id);
    let mut held: Option<Duration> = None;
    let mut hold = |left: Duration| held = Some(held.map_or(left, |h| h.max(left)));
    let mut failed = false;
    match comp_updates_call(st, session, puuid, TEAMMATE_WINDOW).await? {
        RankCall::Ok(rows) => {
            let targets = details_targets(&rows, season.as_deref());
            // R165 (finding 8): a download that failed leaves the page short
            // of matches, so the sync is `Failed` and gives its claim back. A
            // hold is `Held`, and so is a 429 Riot sent (review round 1): the
            // `other` hold it set is the wait, after which R160 runs the
            // sync again.
            let fetched = fetch_missing_details_full(st, session, targets).await?;
            failed |= fetched.failed;
            for left in [fetched.held, fetched.rate_limited].into_iter().flatten() {
                hold(left);
            }
        }
        RankCall::Held(left) => hold(left),
        RankCall::Failed => failed = true,
    }

    let loadout_match = {
        let live = lock(&st.live);
        live.coregame_match_id
            .clone()
            .filter(|_| live.loadouts.is_empty())
    };
    if let Some(match_id) = loadout_match {
        match session.coregame_loadouts(&match_id).await {
            Ok(resp) => {
                let loadouts = extract_loadouts(&resp);
                {
                    let mut live = lock(&st.live);
                    if live.coregame_match_id.as_deref() == Some(match_id.as_str()) {
                        live.loadouts = loadouts;
                    }
                }
                // R83, R97: skins that are now known to be equipped get
                // their names before the caller rebuilds the details, or the
                // rebuild would show `—` for them until the next open. The
                // guard ends the mark even if the fetch panics.
                if crate::details::skins_pending(st, puuid) {
                    let resolving = st.begin_skin_resolve(puuid);
                    crate::details::resolve_skins(st, puuid).await;
                    drop(resolving);
                }
            }
            Err(RiotError::Held(left)) => hold(left),
            Err(e) => check(e, "loadouts", puuid)?,
        }
    }
    Ok(match (failed, held) {
        (true, _) => DetailsSync::Failed,
        (false, Some(left)) => DetailsSync::Held(left),
        (false, None) => DetailsSync::Done,
    })
}

// ---------------------------------------------------------------------
// own-history backfill (R101)
// ---------------------------------------------------------------------

/// The stored anchor's JSON shape.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredAnchor {
    match_id: String,
    start_ms: i64,
}

pub fn anchor_json(anchor: &(String, i64)) -> String {
    serde_json::to_string(&StoredAnchor {
        match_id: anchor.0.clone(),
        start_ms: anchor.1,
    })
    .unwrap_or_default()
}

/// `None` for anything that isn't a stored anchor (bad JSON, an empty id):
/// the run is then a first run again.
pub fn parse_anchor(raw: &str) -> Option<(String, i64)> {
    let a: StoredAnchor = serde_json::from_str(raw).ok()?;
    (!a.match_id.is_empty()).then_some((a.match_id, a.start_ms))
}

/// What one history page contributes to a backfill run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackfillTake {
    /// The entries to look at (still to be checked against the DB), newest
    /// first.
    pub entries: Vec<HistoryEntry>,
    /// A stop was hit (or the history ended): no further page is needed.
    pub done: bool,
}

/// The history's `Total` for a log line.
fn show_total(total: Option<u32>) -> String {
    total.map_or_else(|| "unknown".to_string(), |t| t.to_string())
}

/// Pure: whether the page of `page_len` entries that began at `index` was
/// the last of the history.
///
/// An empty page always is (nothing more can be read, whatever `Total`
/// claims). With `total` (Riot's whole-history count) it is the last page
/// once `index + page_len` reaches it; a page shorter than requested but
/// short of `total` is not the end (clamping, a server-side page cap), so
/// the entries behind it are still read. Only without `total` does a page
/// shorter than [`BACKFILL_PAGE`] stand in for the end.
pub fn history_ended(index: u32, page_len: usize, total: Option<u32>) -> bool {
    if page_len == 0 {
        return true;
    }
    match total {
        Some(total) => u64::from(index) + page_len as u64 >= u64::from(total),
        None => page_len < BACKFILL_PAGE as usize,
    }
}

/// Pure: which entries of `page` (newest first, read from `index` of a
/// history holding `total` matches) a backfill run takes.
/// Takes entries until, exclusive:
/// - with an anchor: the anchor's match id, or the first entry older than
///   the anchor's start (the anchor match may have left the history);
/// - without one (first run): the first entry older than `act_start_ms`, or
///   once `scanned` + taken reaches [`BACKFILL_FIRST_RUN_MAX`].
///
/// `done` when one of those stops was hit, or the history ended
/// ([`history_ended`]).
pub fn backfill_take(
    page: &[HistoryEntry],
    index: u32,
    total: Option<u32>,
    anchor: Option<&(String, i64)>,
    act_start_ms: Option<i64>,
    scanned: u32,
) -> BackfillTake {
    let cap = BACKFILL_FIRST_RUN_MAX as usize;
    let mut entries = Vec::new();
    let mut done = false;
    for e in page {
        let stop = match anchor {
            Some((id, start)) => e.match_id == *id || e.start_ms < *start,
            None => {
                act_start_ms.is_some_and(|act| e.start_ms < act)
                    || scanned as usize + entries.len() >= cap
            }
        };
        if stop {
            done = true;
            break;
        }
        entries.push(e.clone());
    }
    // The cap landed exactly on the end of the page: no empty extra page.
    if anchor.is_none() && scanned as usize + entries.len() >= cap {
        done = true;
    }
    if history_ended(index, page.len(), total) {
        done = true;
    }
    BackfillTake { entries, done }
}

/// R101: starts a backfill run from the stored anchor (a no-op while a run
/// is active). The poll loop calls it on connect (the run waits there until
/// VALORANT is running and a roster's worker starts, R137) and when a match
/// ends (then it makes sure a worker is running).
pub async fn start_backfill_run(st: &Arc<AppState>) {
    // The run is the current account's: its anchor is the account's own.
    let Some(me) = lock(&st.live).self_puuid.clone() else {
        return;
    };
    let key = anchor_key(&me);
    // A SQLite failure (`None`, already logged) skips the run: a run without
    // its anchor would be a first run again.
    let Some(stored) = st.db(move |repo| repo.get_state(&key)).await else {
        return;
    };
    let anchor = stored.as_deref().and_then(parse_anchor);
    log::debug!(
        "own history backfill: {}",
        match &anchor {
            Some((id, _)) => format!("from anchor {}", short(id)),
            None => "first run".to_string(),
        }
    );
    // The act's start only bounds the first run.
    let act_start_ms = if anchor.is_none() {
        st.current_act().map(|a| a.start_ms)
    } else {
        None
    };
    let mut live = lock(&st.live);
    if live.self_puuid.as_deref() == Some(me.as_str()) {
        live.start_backfill(anchor, act_start_ms);
    }
}

/// B1: starts both own-history runs (match history and competitive updates)
/// from their stored anchors. What the poll loop calls on connect and when a
/// match ends; each is a no-op while its run is active.
pub async fn start_own_backfills(st: &Arc<AppState>) {
    start_backfill_run(st).await;
    start_comp_backfill_run(st).await;
}

/// B1: starts an own competitive-updates run from the stored anchor (a no-op
/// while one is active). The act's start bounds every run, not just the first:
/// the chart wants the whole current act and nothing older.
pub async fn start_comp_backfill_run(st: &Arc<AppState>) {
    let Some(me) = lock(&st.live).self_puuid.clone() else {
        return;
    };
    let key = comp_anchor_key(&me);
    // A SQLite failure (`None`, already logged) skips the run: a run without
    // its anchor would page the whole act again.
    let Some(stored) = st.db(move |repo| repo.get_state(&key)).await else {
        return;
    };
    let anchor = stored.as_deref().and_then(parse_anchor);
    log::debug!(
        "own competitive updates backfill: {}",
        match &anchor {
            Some((id, _)) => format!("from anchor {}", short(id)),
            None => "first run".to_string(),
        }
    );
    let act_start_ms = st.current_act().map(|a| a.start_ms);
    let mut live = lock(&st.live);
    if live.self_puuid.as_deref() == Some(me.as_str()) {
        live.start_comp_backfill(anchor, act_start_ms);
    }
}

/// Runs `f` on the active run — only while it is still `me`'s: after an
/// account switch the model holds another user's run (or none), and a step
/// of the old session must not touch it.
fn with_run<R>(st: &AppState, me: &str, f: impl FnOnce(&mut BackfillRun) -> R) -> Option<R> {
    let mut live = lock(&st.live);
    if live.self_puuid.as_deref() != Some(me) {
        return None;
    }
    live.backfill.as_mut().map(f)
}

/// Drops `me`'s run without touching the anchor (the next trigger starts
/// over from it). R150: the players of the matches it stored go to the names
/// fill, however the run ended — they are stored, and nothing else will ask
/// about them.
fn end_run(st: &AppState, me: &str) {
    let asks = {
        let mut live = lock(&st.live);
        if live.self_puuid.as_deref() != Some(me) {
            return;
        }
        live.backfill
            .take()
            .map(|run| run.participants)
            .unwrap_or_default()
    };
    st.queue_name_fill(asks);
}

/// One history page into the run. `Ok(false)`: the run ended (a Riot or
/// SQLite error, or it is no longer this account's).
async fn backfill_page(
    st: &Arc<AppState>,
    session: &RiotSession,
    me: &str,
    index: u32,
) -> Result<bool, Gone> {
    let resp = match session
        .match_history(me, Some(index), Some(index + BACKFILL_PAGE), None)
        .await
    {
        Ok(resp) => resp,
        Err(RiotError::ClientGone) => {
            end_run(st, me);
            return Err(Gone);
        }
        Err(e) => {
            log::warn!("own history page {index} failed: {e}; the backfill stops");
            end_run(st, me);
            return Ok(false);
        }
    };
    let page = history_entries(&resp);
    let total = resp.total;
    let Some((anchor, act_start_ms, scanned)) =
        with_run(st, me, |r| (r.anchor.clone(), r.act_start_ms, r.scanned))
    else {
        return Ok(false);
    };
    // The asset tables may have loaded since the run started (a fresh
    // install downloads them beside the connect): the first run's act bound
    // is looked up again until it is known.
    let act_start_ms = match act_start_ms {
        None if anchor.is_none() => st.current_act().map(|a| a.start_ms),
        known => known,
    };
    let take = backfill_take(&page, index, total, anchor.as_ref(), act_start_ms, scanned);
    log::debug!(
        "own history page {index}: {} entries, history total {}{}",
        page.len(),
        show_total(total),
        if take.done {
            ", last page of the run"
        } else {
            ""
        }
    );
    if page.is_empty() && total.is_some_and(|t| t > index) {
        // The one way `Total` and the pager disagree: it ends the run (an
        // empty page can't be paged past), but say so in the log.
        log::warn!(
            "own history page {index} came back empty although the history total is {}; \
             the backfill treats the history as ended",
            show_total(total)
        );
    }

    let (p, taken) = (me.to_string(), take.entries.clone());
    let unknown = st
        .db(move |repo| {
            repo.index_history(&p, &taken)?;
            let ids: Vec<String> = taken.iter().map(|e| e.match_id.clone()).collect();
            repo.new_match_ids(&ids)
        })
        .await;
    let Some(unknown) = unknown else {
        end_run(st, me);
        return Ok(false);
    };

    let applied = with_run(st, me, |run| {
        if run.next_index != index {
            return false; // a different run replaced this one meanwhile
        }
        if index == 0 {
            run.newest = page.first().map(|e| (e.match_id.clone(), e.start_ms));
        }
        run.scanned += take.entries.len() as u32;
        // Kept once found: the later pages don't look the act up again.
        run.act_start_ms = act_start_ms;
        // By what the page held, not by what was asked for: a short page
        // mid-history must not skip the entries behind it.
        run.next_index = index + page.len() as u32;
        run.total = total;
        run.pending = unknown.into();
        run.done = take.done;
        true
    });
    Ok(applied == Some(true))
}

/// B1: [`with_run`] for the competitive-updates run.
fn with_comp_run<R>(
    st: &AppState,
    me: &str,
    f: impl FnOnce(&mut crate::state::CompBackfillRun) -> R,
) -> Option<R> {
    let mut live = lock(&st.live);
    if live.self_puuid.as_deref() != Some(me) {
        return None;
    }
    live.comp_backfill.as_mut().map(f)
}

/// B1: drops `me`'s competitive-updates run without touching the anchor.
fn end_comp_run(st: &AppState, me: &str) {
    let mut live = lock(&st.live);
    if live.self_puuid.as_deref() == Some(me) {
        live.comp_backfill = None;
    }
}

/// B1, pure: whether the page just fetched (`rows`, newest first; the
/// `pages`-th of the run) is the run's last. It is when
/// - it is short or empty (the history ended: Riot has no `Total` here);
/// - it holds the anchor's match, or a row older than the anchor's start (the
///   anchor row may have left the history): everything newer is in;
/// - it holds a row older than the current act's start minus
///   [`ACT_EDGE_MS`](opd1_core::mystats::ACT_EDGE_MS): My Stats decides act
///   membership by each row's Riot season and accepts rows up to a day before
///   valorant-api's start (the rollout's hours), so the run reaches that far;
/// - it is the [`COMP_BACKFILL_MAX_PAGES`]-th.
pub fn comp_backfill_done(
    rows: &[CompUpdateRow],
    anchor: Option<&(String, i64)>,
    act_start_ms: Option<i64>,
    pages: u32,
) -> bool {
    rows.len() < COMP_BACKFILL_PAGE as usize
        || anchor.is_some_and(|(id, start)| {
            rows.iter()
                .any(|r| r.match_id == *id || r.start_ms < *start)
        })
        || act_start_ms.is_some_and(|act| {
            let edge = act.saturating_sub(opd1_core::mystats::ACT_EDGE_MS);
            rows.iter().any(|r| r.start_ms < edge)
        })
        || pages >= COMP_BACKFILL_MAX_PAGES
}

/// B1: one page of the own competitive-updates backfill. The page is asked
/// for from the run's cursor, stored in `comp_updates` (the same upsert the
/// live sync uses, so rows it already stored are simply written again), and
/// the cursor moves by the rows the page held. The run ends with the anchor
/// moved to page 0's newest row when [`comp_backfill_done`] says so.
///
/// Never more than one page per call: the worker's next job decision comes
/// between pages, so live cards, the names fill and the match backfill go
/// first. R160: an mmr-family hold (`RiotError::Held`, nothing sent) pauses
/// the run until the hold's end plus slack, no failure counted
/// (`LiveModel::pause_comp_backfill`; the poll tick wakes a worker). Any real
/// failure (a 429 that was sent included) ends the run without moving the
/// anchor; the next trigger starts over from it.
///
/// `Err(Gone)`: the client went away, or `session` belongs to an account that
/// is no longer the current one.
pub async fn comp_backfill_step(
    st: &Arc<AppState>,
    session: &Arc<RiotSession>,
) -> Result<(), Gone> {
    let me = session.self_puuid();
    {
        let live = lock(&st.live);
        if live.self_puuid.as_deref() != Some(me) {
            return Err(Gone);
        }
        // A match started since the job was handed out, or the hold is not
        // over: not even a page.
        if !live.comp_backfill_ready() {
            return Ok(());
        }
    }
    let Some((index, anchor)) = with_comp_run(st, me, |run| (run.next_index, run.anchor.clone()))
    else {
        return Ok(());
    };
    // The first page is what the user's own rank sync just fetched when its
    // newest rows (`comp_updates(0..5)`, stored by then) hold the anchor's
    // match: everything newer than the anchor is among them, so the call would
    // bring nothing new. The run ends with its anchor at the newest of those
    // rows; a match that ended since is read by the next run, which pages back
    // from the newest to that anchor.
    if index == 0 {
        let head_newest = anchor.as_ref().and_then(|(id, _)| {
            let live = lock(&st.live);
            live.own_comp_head
                .as_ref()
                .filter(|(owner, head)| {
                    owner == me && head.iter().any(|(head_id, _)| head_id == id)
                })
                .and_then(|(_, head)| head.first().cloned())
        });
        if let Some(newest) = head_newest {
            log::debug!(
                "own competitive updates: page 0 skipped, the rank sync's rows hold the anchor"
            );
            with_comp_run(st, me, |run| run.newest = Some(newest));
            finish_comp_backfill(st, me).await;
            return Ok(());
        }
    }
    let answer = session
        .comp_updates(me, Some(index), Some(index + COMP_BACKFILL_PAGE))
        .await;
    let resp = match rank_call(answer, "own competitive updates page", me) {
        Err(Gone) => {
            end_comp_run(st, me);
            return Err(Gone);
        }
        Ok(RankCall::Held(left)) => {
            lock(&st.live).pause_comp_backfill(left + HOLD_SLACK);
            return Ok(());
        }
        // Logged by `rank_call`.
        Ok(RankCall::Failed) => {
            end_comp_run(st, me);
            return Ok(());
        }
        Ok(RankCall::Ok(resp)) => resp,
    };
    let rows = comp_update_rows_from_response(&resp);
    let Some((anchor, act_start_ms, pages)) =
        with_comp_run(st, me, |r| (r.anchor.clone(), r.act_start_ms, r.pages))
    else {
        return Ok(());
    };
    // The asset tables may have loaded since the run started.
    let act_start_ms = act_start_ms.or_else(|| st.current_act().map(|a| a.start_ms));
    let done = comp_backfill_done(&rows, anchor.as_ref(), act_start_ms, pages + 1);
    log::debug!(
        "own competitive updates page {index}: {} rows{}",
        rows.len(),
        if done { ", last page of the run" } else { "" }
    );

    let (p, stored_rows) = (me.to_string(), rows.clone());
    if st
        .db(move |repo| repo.upsert_comp_updates(&p, &stored_rows))
        .await
        .is_none()
    {
        end_comp_run(st, me);
        return Ok(());
    }
    let applied = with_comp_run(st, me, |run| {
        if run.next_index != index {
            return false; // a different run replaced this one meanwhile
        }
        if index == 0 {
            run.newest = rows.first().map(|r| (r.match_id.clone(), r.start_ms));
        }
        run.pages += 1;
        // Kept once found: the later pages don't look the act up again.
        run.act_start_ms = act_start_ms;
        // By what the page held, not by what was asked for.
        run.next_index = index + rows.len() as u32;
        run.resume_at = None;
        true
    });
    // B2: rows were stored; the chart and the rank changes read them.
    if !rows.is_empty() {
        crate::my_stats::notify(st);
    }
    if applied == Some(true) {
        if done {
            finish_comp_backfill(st, me).await;
        } else {
            // The next page waits (the worker does not): see the gap's doc.
            lock(&st.live).pause_comp_backfill(COMP_BACKFILL_PAGE_GAP);
        }
    }
    Ok(())
}

/// B1: the run is done and its last page is stored: writes the anchor (the
/// newest row page 0 showed, if it is a new one) and drops the run.
async fn finish_comp_backfill(st: &Arc<AppState>, me: &str) {
    let Some((anchor, newest, pages)) =
        with_comp_run(st, me, |r| (r.anchor.clone(), r.newest.clone(), r.pages))
    else {
        return;
    };
    if let Some(newest) = newest.filter(|n| Some(n) != anchor.as_ref()) {
        // The account's own key: a write that lands after an account switch
        // is still that account's finished run, so it needs no guard.
        let (key, raw) = (comp_anchor_key(me), anchor_json(&newest));
        if st
            .db(move |repo| repo.set_state(&key, &raw))
            .await
            .is_some()
        {
            log::info!(
                "own competitive updates backfill finished: {pages} pages, anchor {}",
                short(&newest.0)
            );
        }
    } else {
        log::debug!("own competitive updates backfill finished: nothing new ({pages} pages)");
    }
    // Written first, cleared second: an abort in between just finishes the
    // same run again.
    end_comp_run(st, me);
}

/// The run is done and everything it found is stored: writes the anchor (the
/// newest entry page 0 showed) and drops the run.
async fn finish_backfill(st: &Arc<AppState>, me: &str) {
    let Some((anchor, newest, scanned, total)) = with_run(st, me, |r| {
        (r.anchor.clone(), r.newest.clone(), r.scanned, r.total)
    }) else {
        return;
    };
    let total = show_total(total);
    if let Some(newest) = newest.filter(|n| Some(n) != anchor.as_ref()) {
        // The account's own key: a write that lands after an account switch
        // is still that account's finished run, so it needs no guard.
        let (key, raw, failures) = (anchor_key(me), anchor_json(&newest), failures_key(me));
        let written = st
            .db(move |repo| {
                repo.set_state(&key, &raw)?;
                // Every id of the run is stored, known or skipped by now:
                // no failure count of this account is still needed.
                repo.delete_state(&failures)
            })
            .await;
        if written.is_some() {
            log::info!(
                "own history backfill finished: {scanned} matches looked at of {total} in the history, anchor {}",
                short(&newest.0)
            );
        }
    } else {
        log::debug!(
            "own history backfill finished: nothing new ({scanned} looked at of {total} in the history)"
        );
    }
    // Written first, cleared second: an abort in between just finishes the
    // same run again.
    end_run(st, me);
}

/// R137: counts a failed match-details fetch of `id` (this run ends at it)
/// in `me`'s `app_state` counts, and returns how many runs in a row have
/// failed at it, this one included. At [`BACKFILL_MAX_FAILURES`] the count
/// is reset: the caller skips the id. `None`: SQLite failed (the caller ends
/// the run).
async fn record_details_failure(st: &Arc<AppState>, me: &str, id: &str) -> Option<u32> {
    let (id, key) = (id.to_string(), failures_key(me));
    st.db(move |repo| {
        let mut counts: BTreeMap<String, u32> = repo
            .get_state(&key)?
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default();
        let failed = counts.get(&id).copied().unwrap_or(0) + 1;
        if failed >= BACKFILL_MAX_FAILURES {
            counts.remove(&id);
        } else {
            counts.insert(id, failed);
        }
        if counts.is_empty() {
            repo.delete_state(&key)?;
        } else {
            let raw = serde_json::to_string(&counts).unwrap_or_default();
            repo.set_state(&key, &raw)?;
        }
        Ok(failed)
    })
    .await
}

/// One step of the own-history backfill (R101): fetch the next history page
/// if the last one is used up, then match-details for the page's unknown
/// ids, one at a time (through the session's throttle), yielding as soon as
/// a live `Names`/`Sync` job exists — the cursor stays in the run. When the
/// run is done and nothing is pending, the anchor moves and the run ends.
///
/// Every failure other than `ClientGone` ends the run without moving the
/// anchor (the gap is retried from it next time), a decode error included
/// (schema drift is systematic: skipping would lose every match for good).
/// Two exceptions (R137): a match Riot no longer has (404) is skipped at
/// once, since retrying can never succeed; and a match whose failures say
/// something about the id itself ([`counts_toward_skip`], R139) in
/// [`BACKFILL_MAX_FAILURES`] runs in a row is skipped (logged), so the
/// anchor can't be stuck behind it forever.
///
/// `Err(Gone)`: the client went away, or `session` belongs to an account
/// that is no longer the current one.
pub async fn backfill_step(st: &Arc<AppState>, session: &Arc<RiotSession>) -> Result<(), Gone> {
    let me = session.self_puuid();
    {
        let live = lock(&st.live);
        if live.self_puuid.as_deref() != Some(me) {
            return Err(Gone);
        }
        // A match started since the job was handed out: not even a page.
        if !live.backfill_ready() {
            return Ok(());
        }
    }
    let Some(page_at) = with_run(st, me, |run| {
        (run.pending.is_empty() && !run.done).then_some(run.next_index)
    }) else {
        return Ok(());
    };
    if let Some(index) = page_at {
        if !backfill_page(st, session, me, index).await? {
            return Ok(());
        }
    }

    loop {
        {
            // Yield at every id: to a live job, to a names fill (R150: the
            // match end's, right after its details were stored), and to a
            // match that started meanwhile (R137; a free-for-all one never
            // changes the view).
            let live = lock(&st.live);
            if live.live_job_pending() || live.fill_ready() || !live.backfill_ready() {
                return Ok(());
            }
        }
        let Some(next) = with_run(st, me, |run| run.pending.front().cloned()) else {
            return Ok(());
        };
        let Some(id) = next else { break };
        let pop = |st: &AppState| {
            with_run(st, me, |run| {
                if run.pending.front() == Some(&id) {
                    run.pending.pop_front();
                }
            });
        };

        // The live sync may have stored it since the page was read.
        let check = id.clone();
        let unknown = st.db(move |repo| repo.new_match_ids(&[check])).await;
        if unknown.is_some_and(|u| u.is_empty()) {
            pop(st);
            continue;
        }
        // Another task is downloading it right now: wait, then look again
        // (stored: popped above; failed there: claimed and fetched here).
        let _claim = match st.claim_details_fetch(&id) {
            DetailsClaim::Mine(guard) => guard,
            DetailsClaim::Busy(waiter) => {
                wait_for_details_fetch(waiter).await;
                continue;
            }
        };
        match session.match_details(&id).await {
            Ok(details) => {
                let Some((start_ms, others)) = store_details(st, details).await else {
                    end_run(st, me);
                    return Ok(());
                };
                // B2: right as it is stored, not when the pass returns — a pass
                // the worker's abort ends mid-await still told My Stats.
                crate::my_stats::notify(st);
                with_run(st, me, |run| run.add_participants(start_ms, others));
                pop(st);
            }
            Err(RiotError::ClientGone) => {
                end_run(st, me);
                return Err(Gone);
            }
            Err(e @ RiotError::Status(404, _)) => {
                log::warn!("backfill: match {} skipped: {e}", short(&id));
                pop(st);
            }
            // R139: rate-limit, network and auth errors say nothing about
            // the id — the run ends, uncounted, and the next one retries it.
            Err(e) if !counts_toward_skip(&e) => {
                log::warn!(
                    "backfill: match {} failed: {e}; the backfill stops",
                    short(&id)
                );
                end_run(st, me);
                return Ok(());
            }
            Err(e) => match record_details_failure(st, me, &id).await {
                Some(failed) if failed >= BACKFILL_MAX_FAILURES => {
                    log::warn!(
                        "backfill: match {} failed in {failed} runs in a row ({e}); skipped",
                        short(&id)
                    );
                    pop(st);
                }
                _ => {
                    log::warn!(
                        "backfill: match {} failed: {e}; the backfill stops",
                        short(&id)
                    );
                    end_run(st, me);
                    return Ok(());
                }
            },
        }
    }

    if with_run(st, me, |run| run.done && run.pending.is_empty()) == Some(true) {
        finish_backfill(st, me).await;
    }
    Ok(())
}

/// The live roster's background worker (worker `id`, claimed via
/// `LiveModel::try_start_worker`). Each job — one `names()` call for the
/// visible players still unnamed, one player's sync in display order, a due
/// rank retry (R155), one names-fill request, or, last of all, one step of the
/// own-history backfill or one page of the own competitive updates (B1) — is
/// chosen by
/// `LiveModel::next_job` under the model's lock, which also
/// releases the claim when nothing is left (R45: no roster change can
/// slip between "nothing to do" and the task ending). Roster changes
/// mid-run are simply picked up by the next job.
pub async fn run_worker(st: Arc<AppState>, session: Arc<RiotSession>, id: u64) {
    loop {
        let job = lock(&st.live).next_job(id);
        let outcome = match job {
            None => return,
            Some(Job::Names(needed)) => names_step(&st, &session, needed).await,
            Some(Job::Rank(batch)) => rank_batch_step(&st, &session, batch).await,
            Some(Job::Sync(puuid)) => sync_step(&st, &session, puuid).await,
            Some(Job::RankRetry(puuid)) => retry_step(&st, &session, puuid).await,
            Some(Job::FillNames) => fill_names_step(&st, &session).await,
            Some(Job::Backfill) => backfill_step(&st, &session).await,
            Some(Job::CompBackfill) => comp_backfill_step(&st, &session).await,
        };
        if outcome.is_err() {
            lock(&st.live).release_worker(id);
            return;
        }
    }
}

/// The player's stored bundle (`None`: the read failed, the bundle already on
/// screen stays) into the live slot, `mark` applied under the same lock, then
/// the card is published and an open Details of the player follows (R23).
async fn publish_player(
    st: &Arc<AppState>,
    session: &RiotSession,
    puuid: &str,
    mark: impl FnOnce(&mut crate::state::LiveModel, bool),
) {
    let (p, me) = (puuid.to_string(), session.self_puuid().to_string());
    // A failed read keeps the bundle already on screen.
    let db = st
        .db(move |repo| crate::store::load_player_db(repo, Some(&me), &p))
        .await;
    {
        let mut live = lock(&st.live);
        mark(&mut live, db.is_some());
        if let (Some(db), Some(slot)) =
            (db, live.slots.iter_mut().find(|s| s.roster.puuid == puuid))
        {
            slot.db = db;
        }
    }
    st.publish();
    // R23: an open Details of this player shows the settled card too.
    crate::details::refresh(st, puuid).await;
}

/// R165: the rank phase of one player, then their card is published: the
/// MMR fields (rank, peak, previous act, win rate) settle, the matches-derived
/// ones (K/D, HS%, last 5) wait for the matches phase.
///
/// `first_pass`: the outcome is noted for the R155/R160 schedule in the same
/// critical section as the `ranked` mark, before the awaits of the publish: a
/// worker aborted by a stage change cannot leave a player marked ranked with
/// the retry of its failure never recorded. (A retry is noted once, after both
/// of its phases: [`retry_step`]; its turn was counted when it was handed out.)
pub(crate) async fn rank_step(
    st: &Arc<AppState>,
    session: &Arc<RiotSession>,
    puuid: &str,
    first_pass: bool,
) -> Result<RankOutcome, Gone> {
    let outcome = sync_rank(st, session, puuid).await?;
    publish_player(st, session, puuid, |live, _| {
        live.synced.mark_ranked(puuid);
        if first_pass {
            match outcome.held {
                Some(left) => live.note_rank_held(puuid, left, false),
                None => live.note_rank_sync(puuid, outcome.failed, false),
            }
        }
    })
    .await;
    // B2: the user's own rank card, RR rows and snapshot may have changed.
    crate::my_stats::notify_if_own(st, session.self_puuid(), puuid);
    log::debug!("rank synced {}", short(puuid));
    Ok(outcome)
}

/// R165: the matches phase of one player, then their card is published. Party
/// marks need the stored history to be this cycle's: the history was written
/// and the bundle built from it was read back; either SQLite step failing
/// leaves the slot on earlier rows. `first_pass`: a partial outcome joins the
/// R155 retries (`LiveModel::note_partial_sync`), one that only met the
/// `other` hold waits for it and uses no retry (`note_matches_held`, R160) —
/// noted under the same lock as the `synced` mark, like [`rank_step`].
pub(crate) async fn matches_step(
    st: &Arc<AppState>,
    session: &Arc<RiotSession>,
    puuid: &str,
    first_pass: bool,
) -> Result<MatchesOutcome, Gone> {
    let outcome = sync_matches(st, session, puuid).await?;
    publish_player(st, session, puuid, |live, db_ok| {
        live.synced.insert(puuid.to_string());
        if outcome.history_ok && db_ok {
            live.party_ready.insert(puuid.to_string());
        }
        if first_pass {
            match (outcome.partial, outcome.held) {
                (true, _) => live.note_partial_sync(puuid),
                (false, Some(left)) => live.note_matches_held(puuid, left),
                (false, None) => {}
            }
        }
    })
    .await;
    // B2: …and so may the user's own matches.
    crate::my_stats::notify_if_own(st, session.self_puuid(), puuid);
    log::debug!("synced {}", short(puuid));
    Ok(outcome)
}

/// One `Job::Rank`: the rank phases of up to
/// [`RANK_CONCURRENCY`](crate::state::RANK_CONCURRENCY) players at once, each
/// published as it ends. Inside the session's buckets and holds; the worker
/// stays one job loop (this is one job). R155/R160: a first-pass failure or
/// hold schedules the player's retry. A gone client ends the job and aborts
/// the rest of the batch (the set is dropped).
pub(crate) async fn rank_batch_step(
    st: &Arc<AppState>,
    session: &Arc<RiotSession>,
    batch: Vec<String>,
) -> Result<(), Gone> {
    let mut set = JoinSet::new();
    for puuid in &batch {
        let (st, session, puuid) = (st.clone(), session.clone(), puuid.clone());
        set.spawn(async move { rank_step(&st, &session, &puuid, true).await.map(|_| ()) });
    }
    let (mut gone, mut panicked) = (false, false);
    while let Some(joined) = set.join_next().await {
        match joined {
            Ok(Err(Gone)) => {
                gone = true;
                break;
            }
            Ok(Ok(())) => {}
            Err(e) => {
                log::warn!("rank sync task failed: {e}");
                panicked = true;
            }
        }
    }
    if gone {
        return Err(Gone);
    }
    // A task that panicked must not be handed out again forever.
    if panicked {
        let mut live = lock(&st.live);
        for puuid in &batch {
            live.synced.mark_ranked(puuid);
        }
    }
    Ok(())
}

/// One `Job::Sync`: the matches phase of one player (R165).
async fn sync_step(
    st: &Arc<AppState>,
    session: &Arc<RiotSession>,
    puuid: String,
) -> Result<(), Gone> {
    matches_step(st, session, &puuid, true).await.map(|_| ())
}

/// One `Job::RankRetry` (R155): both phases of the player again, the card
/// published after each. The matches phase fetches only the ids still
/// missing. One outcome is noted: a real failure (a failed rank part, a
/// partial matches phase) moves the schedule on; otherwise a hold (R160)
/// uses up no retry.
async fn retry_step(
    st: &Arc<AppState>,
    session: &Arc<RiotSession>,
    puuid: String,
) -> Result<(), Gone> {
    let rank = rank_step(st, session, &puuid, false).await?;
    let matches = matches_step(st, session, &puuid, false).await?;
    let failed = rank.failed || matches.partial;
    let held = rank.held.into_iter().chain(matches.held).max();
    let mut live = lock(&st.live);
    match held.filter(|_| !failed) {
        Some(left) => live.note_rank_held(&puuid, left, true),
        None => live.note_rank_sync(&puuid, failed, true),
    }
    Ok(())
}

/// One `names()` call. Hidden players are filtered twice: `names_needed()`
/// skips anyone whose `Incognito` is true (and the user, whose name comes
/// from the chat session — R43), and `VisiblePuuid::new` refuses an
/// incognito flag at the type level.
async fn names_step(
    st: &Arc<AppState>,
    session: &RiotSession,
    needed: Vec<(String, bool)>,
) -> Result<(), Gone> {
    let visible: Vec<VisiblePuuid> = needed
        .iter()
        .filter_map(|(p, incognito)| VisiblePuuid::new(p.clone(), *incognito))
        .collect();
    let asked: Vec<String> = needed.into_iter().map(|(p, _)| p).collect();

    let result = if visible.is_empty() {
        Ok(Vec::new())
    } else {
        session.names(&visible).await
    };
    let entries = match result {
        Ok(entries) => entries,
        Err(RiotError::ClientGone) => return Err(Gone),
        // R160: nothing was sent, so nobody was asked: they stay unasked (no
        // `names_attempted`), and the model hands out no names job until the
        // hold is over.
        Err(RiotError::Held(left)) => {
            log::debug!(
                "names for {} players not sent: hold, {left:?} left",
                asked.len()
            );
            lock(&st.live).defer_names(left + HOLD_SLACK);
            return Ok(());
        }
        Err(e) => {
            // R165: an error is not an answer. The players are not marked
            // attempted: they are asked again after a short delay, at most
            // three tries a stage (`LiveModel::note_names_failed`), and until
            // then the card keeps its skeleton or, once synced, shows `—`.
            let retry = {
                let mut live = lock(&st.live);
                let retry = live.note_names_failed(&asked);
                // The worker never waits: the poll loop's tick starts one when
                // the earliest waiting retry is due
                // (`LiveModel::names_wakeup_due`). Nothing else is blocked:
                // a player who joins meanwhile is asked at once.
                live.arm_names_retry_wake();
                retry
            };
            log::warn!(
                "names for {} players failed: {e}; {}",
                asked.len(),
                match retry {
                    Some(wait) => format!("asking again in {}s", wait.as_secs()),
                    None => "giving up".to_string(),
                }
            );
            st.publish();
            return Ok(());
        }
    };

    let asked_set: HashSet<&str> = asked.iter().map(String::as_str).collect();
    let learned: Vec<(String, String, String)> = entries
        .into_iter()
        .filter(|e| asked_set.contains(e.subject.as_str()))
        .filter(|e| !e.game_name.is_empty() && !e.tag_line.is_empty())
        .map(|e| (e.subject, e.game_name, e.tag_line))
        .collect();
    {
        let mut live = lock(&st.live);
        // Answered (any 2xx, blank names included): attempted from here on.
        live.names_attempted.extend(asked.iter().cloned());
        for (p, n, t) in &learned {
            live.known_names.insert(p.clone(), (n.clone(), t.clone()));
        }
        // Players of an earlier failed lookup may still be waiting for their
        // retry (their counts differ): keep a wake-up for the next one.
        live.arm_names_retry_wake();
    }
    log::debug!("names: {} of {} resolved", learned.len(), asked.len());
    save_names(st, learned).await;
    st.publish();
    // R23: an open Details of one of them shows the name (or `—`) too.
    if let Some(open) = asked.iter().find(|p| st.details_open_for(p)) {
        crate::details::refresh(st, open).await;
    }
    Ok(())
}

/// R150: Riot's limit on one name-service request.
pub const NAME_BATCH_MAX: usize = 100;

/// R150: one names-fill request. Takes queued asks (oldest first), drops
/// those the fill may not ask ([`LiveModel::fill_allowed`]: the user, an empty
/// id) and those the DB says aren't worth asking ([`Repo::names_to_ask`]: a
/// name is already known somewhere, or a blank answer already covers this
/// ask), until 100 are left or the queue is empty — then sends **one** `PUT`
/// for them. Every non-empty answer is stored in `players` (its `Seen` kind,
/// R145); the rest are marked, each with the start of the match that asked,
/// so that match (or an older one) never asks twice and the next, newer match
/// asks again.
///
/// R153 (user, 2026-09-30): players who were hidden in the match that just
/// ended, or earlier this run (`LiveModel::seen_hidden`), are asked like
/// anyone else. After a match the game's own Match History shows the real
/// names of the players who hid them during it, and this fill is what
/// captures those names — by puuid, so that the next time that player is met
/// hidden, their card shows the stored name in the remembered style (R145).
/// The guard is the stage: **nothing is ever sent while a match is running**
/// (Agent Select, In Game, a free-for-all match; `LiveModel::between_matches`).
/// It is checked where asks leave the queue ([`LiveModel::take_fill`], which
/// hands out nothing then) and once more under the `live` lock right before
/// the request is built: a match that starts in between sends nothing and the
/// asks go back to the queue, to go out after it. A worker the poll loop
/// aborts on the stage change ends a request still waiting in the throttle.
/// The model's stage can lag Riot's by a poll tick, so the user's own presence
/// is read once more right before the request (a match that began in that lag
/// sends nothing either; see the comment at the read).
///
/// The request goes through the session's throttle like every pd call. A
/// failed request stores nothing: an error is not an answer, and the asks
/// are not kept — a later trigger asks again (a loop on a persistent error
/// would be worse).
///
/// The asks it has taken are held by a [`TakenAsks`] guard until the request
/// is answered: a step that is dropped on the way — the poll loop aborts the
/// worker on a stage change — or that loses the Riot client puts them back in
/// the queue, because for a player who hid their name they are the only
/// route to it (a stored match queues its players once).
pub async fn fill_names_step(st: &Arc<AppState>, session: &RiotSession) -> Result<(), Gone> {
    let mut held = TakenAsks::new(st);
    while held.asks.len() < NAME_BATCH_MAX {
        let batch = lock(&st.live).take_fill(NAME_BATCH_MAX - held.asks.len());
        if batch.is_empty() {
            break;
        }
        held.checking = batch.clone();
        // A SQLite failure (`None`, already logged) drops the batch.
        let worth = st.db(move |repo| repo.names_to_ask(&batch)).await;
        held.checking.clear();
        if let Some(worth) = worth {
            held.asks.extend(worth);
        }
    }

    // The last look, under the lock the `VisiblePuuid`s are built under: a
    // match may have started while the DB was read — then nothing is sent and
    // the asks go back to the queue (`held` puts them there when it goes out
    // of scope, after `live`; R153: the stage is the guard, not who was
    // hidden).
    let visible: Vec<VisiblePuuid> = {
        let live = lock(&st.live);
        if !live.between_matches() {
            return Ok(());
        }
        // Kept next to the door although `take_fill` already dropped these:
        // an account switch while the DB was read can make a taken ask the
        // user's, and `held.asks` must not mark it spent.
        held.asks.retain(|a| live.fill_allowed(&a.puuid));
        // Between matches a player who hid their name *during* the match is
        // asked about on purpose (R153); `fill_visible` builds them only
        // while no match runs (R167).
        live.fill_visible(&held.asks)
    };
    if visible.is_empty() {
        return Ok(());
    }

    // R165 finding 9 (second half): the model only learns of Agent Select at
    // the poll loop's next presence tick, up to 2 s after Riot changed it, and
    // those players may be hidden there (a party member hidden in the Lobby is
    // asked on purpose, R153). One local read, only now that a request is
    // about to go out: when the user's own presence already says a match
    // began, nothing is sent and the asks stay (`held` puts them back, nobody
    // marked), and the fill stands down until the poll loop has seen the stage.
    // A read that fails changes nothing: names are not held up by a local
    // hiccup.
    if matches!(
        own_stage_now(session).await,
        Some(Stage::Pregame | Stage::Ingame)
    ) {
        log::debug!("names fill not sent: the presence says a match has started");
        lock(&st.live).stand_down_fill(FILL_RACE_STAND_DOWN);
        return Ok(());
    }

    let entries = match session.names(&visible).await {
        Ok(entries) => entries,
        // Nothing was answered: `held` keeps the asks for the next connection.
        Err(RiotError::ClientGone) => return Err(Gone),
        // R160: not sent because of a hold: the asks go back to the queue
        // (`held` does it) and wait for the hold to end.
        Err(RiotError::Held(left)) => {
            log::debug!("names fill not sent: hold, {left:?} left");
            lock(&st.live).defer_names(left + HOLD_SLACK);
            return Ok(());
        }
        Err(e) => {
            log::warn!("names fill for {} players failed: {e}", visible.len());
            held.asks.clear();
            return Ok(());
        }
    };
    // Answered: from here on the asks are spent, whatever happens next.
    let asks = held.answered();
    let asked: HashSet<&str> = asks.iter().map(|a| a.puuid.as_str()).collect();
    let learned: Vec<(String, String, String)> = entries
        .into_iter()
        .filter(|e| asked.contains(e.subject.as_str()))
        .filter(|e| !e.game_name.is_empty() && !e.tag_line.is_empty())
        .map(|e| (e.subject, e.game_name, e.tag_line))
        .collect();
    let named: HashSet<&str> = learned.iter().map(|(p, _, _)| p.as_str()).collect();
    let blank: Vec<NameAsk> = asks
        .iter()
        .filter(|a| !named.contains(a.puuid.as_str()))
        .cloned()
        .collect();
    log::debug!(
        "names fill: {} of {} asked answered with a name",
        learned.len(),
        asks.len()
    );
    let now = now_ms();
    let named_ids: Vec<String> = learned.iter().map(|(p, _, _)| p.clone()).collect();
    let learned_any = !named_ids.is_empty();
    let stored = st
        .db(move |repo| repo.record_name_answers(&learned, &blank, now))
        .await;
    if stored.is_some() && learned_any {
        // R167: a player on the roster right now (a hidden Lobby party member,
        // R153) shows the remembered name at once: their cards are rebuilt and
        // published once for the whole fill.
        st.reload_slots(&named_ids).await;
        // The open Player Details and Match Details show the new names.
        crate::details::refresh_open(st).await;
        crate::match_details::refresh_open(st).await;
    }
    Ok(())
}

/// How long the names fill (only it: `LiveModel::stand_down_fill`, the live
/// roster's names step is not held) stands down when the user's own presence
/// says a match has started before the poll loop has seen it
/// ([`fill_names_step`]): a little over the poll tick (2 s), so by the time the
/// fill may run again the stage is applied (`LiveModel::in_match`), or the
/// presence was only a blip and the asks go out.
const FILL_RACE_STAND_DOWN: Duration = Duration::from_secs(3);

/// The user's own stage from one local presence read; `None` when the read
/// fails or VALORANT shows no presence.
async fn own_stage_now(session: &RiotSession) -> Option<Stage> {
    let presences = session.presences().await.ok()?;
    let private = own_valorant_private(&presences, session.self_puuid())?;
    decode_presence(&private).map(|p| p.stage)
}

/// The asks a names-fill step has taken off the queue and had no answer for
/// yet ([`fill_names_step`]). Dropping it — the step's future dropped, or the
/// step returning without an answer — puts them back in the queue
/// ([`LiveModel::queue_name_fill`], which drops the user and empty ids again
/// and merges with what is queued). [`Self::answered`] hands them over for
/// good.
struct TakenAsks<'a> {
    st: &'a Arc<AppState>,
    /// Taken and being checked against the DB (`Repo::names_to_ask`).
    checking: Vec<NameAsk>,
    /// Worth asking, waiting for the request and its answer.
    asks: Vec<NameAsk>,
}

impl<'a> TakenAsks<'a> {
    fn new(st: &'a Arc<AppState>) -> Self {
        Self {
            st,
            checking: Vec::new(),
            asks: Vec::new(),
        }
    }

    /// The request was answered: the asks are spent and stay out of the queue.
    fn answered(&mut self) -> Vec<NameAsk> {
        std::mem::take(&mut self.asks)
    }
}

impl Drop for TakenAsks<'_> {
    fn drop(&mut self) {
        let back: Vec<NameAsk> = self.checking.drain(..).chain(self.asks.drain(..)).collect();
        if !back.is_empty() {
            lock(&self.st.live).queue_name_fill(back);
        }
    }
}

async fn save_names(st: &Arc<AppState>, learned: Vec<(String, String, String)>) {
    if learned.is_empty() {
        return;
    }
    let now = now_ms();
    st.db(move |repo| {
        for (p, n, t) in &learned {
            save_player(
                repo.conn(),
                &PlayerUpdate {
                    puuid: p.clone(),
                    name: Some((n.clone(), t.clone())),
                    account_level: 0,
                    card_id: String::new(),
                },
                now,
            )?;
        }
        Ok(())
    })
    .await;
}

/// `GET /chat/v1/session` on the local client (techchrism
/// `ChatSession.ts`: `{game_name, game_tag, puuid, loaded, ...}`).
#[derive(Debug, Deserialize)]
struct ChatSession {
    #[serde(default)]
    game_name: String,
    #[serde(default)]
    game_tag: String,
    #[serde(default)]
    puuid: String,
}

/// R43: the user's own Riot ID, from the local chat session — never from
/// name-service. Retried by the poll loop (on stage changes) until it
/// yields a name; `self_name_attempted` makes an unknown own name `—`
/// instead of a skeleton in the meantime.
pub async fn fetch_self_name(st: &Arc<AppState>, session: &RiotSession) -> Result<(), Gone> {
    let me = session.self_puuid().to_string();
    let name = match session.local::<ChatSession>("/chat/v1/session").await {
        Ok(s) if s.puuid == me && !s.game_name.is_empty() && !s.game_tag.is_empty() => {
            Some((s.game_name, s.game_tag))
        }
        Ok(_) => {
            log::info!("chat session has no Riot ID yet");
            None
        }
        Err(RiotError::ClientGone) => return Err(Gone),
        Err(e) => {
            log::warn!("chat session failed: {e}");
            None
        }
    };
    {
        let mut live = lock(&st.live);
        live.self_name_attempted = true;
        if name.is_some() {
            live.self_name = name.clone();
        }
    }
    if let Some((n, t)) = name {
        save_names(st, vec![(me, n, t)]).await;
    }
    st.publish();
    Ok(())
}

/// R90: the user's own equipped skins, read once per connect (at the
/// first presence showing VALORANT, `Poller::read_self_loadout_once`) from
/// `pd .../personalization/v3/players/{self}/playerloadout` (a read-only
/// `GET`, always the user's own puuid): kept in the live model for their
/// Details, and in `app_state` (only when it changed) so it still shows
/// while VALORANT is closed. A failure is logged and not retried until the
/// next connect.
pub async fn fetch_self_loadout(
    st: &Arc<AppState>,
    session: &Arc<RiotSession>,
) -> Result<(), Gone> {
    fetch_self_loadout_after_holds(st, session, HOLD_SLACK).await
}

/// How many times the own-loadout read waits for a hold and tries again
/// (R160): the read is made once per connect, so a call refused because the
/// `other` family is on hold (`Held`, nothing sent) must not end it.
const SELF_LOADOUT_HOLD_RETRIES: u32 = 3;

/// [`fetch_self_loadout`] with the wait after a hold's end given (tests).
pub(crate) async fn fetch_self_loadout_after_holds(
    st: &Arc<AppState>,
    session: &Arc<RiotSession>,
    slack: Duration,
) -> Result<(), Gone> {
    let me = session.self_puuid().to_string();
    let mut held_retries = 0;
    let resp = loop {
        match session.player_loadout().await {
            Ok(resp) => break resp,
            Err(RiotError::Held(left)) if held_retries < SELF_LOADOUT_HOLD_RETRIES => {
                held_retries += 1;
                log::debug!("player loadout not sent: hold, {left:?} left; again after it");
                tokio::time::sleep(left + slack).await;
            }
            Err(e) => return check(e, "player loadout", &me),
        }
    };
    let loadout = extract_player_loadout(&resp);
    let stored = serde_json::to_string(&loadout).unwrap_or_default();
    {
        // The answer can land after an account switch (a slow read, a 429
        // hold): it is only this session's account's loadout.
        let mut live = lock(&st.live);
        if live.self_puuid.as_deref() != Some(me.as_str()) {
            log::debug!("own loadout of a previous account dropped");
            return Ok(());
        }
        live.self_loadout = Some(loadout);
    }
    let owner = st.clone();
    st.db(move |repo| {
        // Checked again under the DB lock: an account switch updates the
        // live model first and then deletes the stored copy under this same
        // lock, so a write for the previous account lands before that
        // delete or not at all. (Lock order: the repo, then the live model;
        // nothing takes the repo while holding the live model.)
        if lock(&owner.live).self_puuid.as_deref() != Some(me.as_str()) {
            return Ok(());
        }
        if repo.get_state(SELF_LOADOUT_KEY)?.as_deref() != Some(stored.as_str()) {
            repo.set_state(SELF_LOADOUT_KEY, &stored)?;
        }
        Ok(())
    })
    .await;
    log::debug!("own loadout read");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, season: &str, start_ms: i64) -> CompUpdateRow {
        CompUpdateRow {
            match_id: id.into(),
            season_id: season.into(),
            tier_after: 10,
            tier_before: Some(10),
            rr_after: 10,
            rr_earned: 10,
            map_id: None,
            start_ms,
        }
    }

    #[test]
    fn only_per_id_failures_count_toward_the_skip() {
        // R139: only an answer about the id itself counts — a Riot status
        // other than 404 (skipped at once) and 429 (rate limit), or a body
        // that doesn't decode. Rate-limit, network and auth errors say
        // nothing about the id: they end the run uncounted.
        for counted in [
            RiotError::Status(500, String::new()),
            RiotError::Status(502, String::new()),
            RiotError::Status(400, String::new()),
            RiotError::Decode("missing field".into()),
        ] {
            assert!(counts_toward_skip(&counted), "{counted:?} counts");
        }
        for uncounted in [
            RiotError::Status(404, String::new()),
            RiotError::Status(429, String::new()),
            RiotError::RateLimited,
            RiotError::Held(Duration::from_secs(30)),
            RiotError::Http("operation timed out".into()),
            RiotError::AuthFailed("no token".into()),
            RiotError::Status(401, String::new()),
            RiotError::Status(403, String::new()),
            RiotError::ShardsUnavailable,
            RiotError::ClientGone,
        ] {
            assert!(
                !counts_toward_skip(&uncounted),
                "{uncounted:?} is not counted"
            );
        }
    }

    #[test]
    fn a_failure_line_says_what_failed_for_whom_and_why_and_nothing_more() {
        const PUUID: &str = "aaaaaaaa-0000-0000-0000-000000000001";
        let status = RiotError::Status(
            500,
            r#"{"errorCode":"INTERNAL_SERVER_ERROR","httpStatus":500}"#.into(),
        );
        let line = failure_line("mmr", PUUID, &status);
        assert_eq!(
            line,
            "mmr for aaaaaaaa failed: unexpected HTTP status 500 from Riot: \
             {\"errorCode\":\"INTERNAL_SERVER_ERROR\",\"httpStatus\":500}"
        );
        assert_eq!(
            failure_line(
                "comp updates",
                PUUID,
                &RiotError::Status(429, String::new())
            ),
            "comp updates for aaaaaaaa failed: unexpected HTTP status 429 from Riot"
        );
        // The other kinds: rate limit, decode, and the transport failure kind.
        for (err, needle) in [
            (RiotError::RateLimited, "rate-limited"),
            (RiotError::Held(Duration::from_secs(30)), "on hold"),
            (RiotError::Decode("missing field `Matches`".into()), "parse"),
            (
                RiotError::Http("timeout: error sending request".into()),
                "timeout",
            ),
            (
                RiotError::Http("connect: error sending request".into()),
                "connect",
            ),
        ] {
            let line = failure_line("mmr", PUUID, &err);
            assert!(line.starts_with("mmr for aaaaaaaa failed: "), "{line}");
            assert!(line.contains(needle), "{line}");
            assert!(!line.contains(PUUID), "no full puuid: {line}");
            assert!(
                !line.contains("http://") && !line.contains("https://"),
                "{line}"
            );
        }
    }

    #[test]
    fn failure_counts_are_kept_per_account() {
        // Like the anchors (R137): one key per account.
        assert_eq!(failures_key("a"), "self_history_failures:a");
        assert_ne!(failures_key("a"), failures_key("b"));
    }

    #[test]
    fn dedupe_keeps_first_occurrence_order() {
        let ids = vec!["a".into(), "b".into(), "a".into(), "c".into(), "b".into()];
        assert_eq!(dedupe(ids), vec!["a", "b", "c"]);
    }

    #[test]
    fn latest_comp_match_is_the_newest_or_empty() {
        assert_eq!(
            latest_comp_match(&[row("old", "s", 1), row("new", "s", 9)]),
            "new"
        );
        assert_eq!(latest_comp_match(&[]), "");
    }

    #[test]
    fn details_targets_are_recent_eight_plus_current_act() {
        // 12 rows: 10 in the current act (newest), 2 older.
        let mut rows: Vec<CompUpdateRow> = (0..10)
            .map(|i| row(&format!("cur{i}"), "act", 100 + i))
            .collect();
        rows.push(row("old1", "prev", 50));
        rows.push(row("old2", "prev", 40));
        let t = details_targets(&rows, Some("act"));
        assert_eq!(
            t.len(),
            10,
            "8 newest are all current-act; the rest of the act adds 2"
        );
        assert_eq!(t[0], "cur9", "newest first");
        assert!(!t.contains(&"old1".to_string()));

        // Few current-act matches: recent 8 still reach into the old act.
        let rows = vec![
            row("cur", "act", 100),
            row("old1", "prev", 50),
            row("old2", "prev", 40),
        ];
        let t = details_targets(&rows, Some("act"));
        assert_eq!(t, vec!["cur", "old1", "old2"]);

        // Unknown current act: just the recent ones.
        assert_eq!(details_targets(&rows, None).len(), 3);
    }

    /// `n` history entries, newest first, one minute apart: entry 0 starts
    /// at `BASE`, entry `i` at `BASE - i * 60_000`. Ids are `e{offset + i}`.
    const BASE: i64 = 10_000_000;
    fn entries(n: usize) -> Vec<HistoryEntry> {
        entries_from(0, n)
    }
    fn entries_from(offset: usize, n: usize) -> Vec<HistoryEntry> {
        (0..n)
            .map(|i| HistoryEntry {
                match_id: format!("e{}", offset + i),
                queue_id: "unrated".into(),
                start_ms: BASE - (offset + i) as i64 * 60_000,
            })
            .collect()
    }
    fn ids(t: &BackfillTake) -> Vec<&str> {
        t.entries.iter().map(|e| e.match_id.as_str()).collect()
    }

    /// `n` competitive-update rows, newest first, one minute apart (row `i`
    /// starts at `BASE - i * 60_000`), ids `c{offset + i}`.
    fn comp_rows(offset: usize, n: usize) -> Vec<CompUpdateRow> {
        (0..n)
            .map(|i| {
                row(
                    &format!("c{}", offset + i),
                    "act",
                    BASE - (offset + i) as i64 * 60_000,
                )
            })
            .collect()
    }

    #[test]
    fn comp_backfill_done_on_a_short_or_empty_page() {
        assert!(
            comp_backfill_done(&[], None, None, 1),
            "empty: the history ended"
        );
        assert!(
            comp_backfill_done(&comp_rows(0, 19), None, None, 1),
            "short"
        );
        assert!(
            !comp_backfill_done(&comp_rows(0, 20), None, None, 1),
            "full: more may follow"
        );
    }

    #[test]
    fn comp_backfill_done_at_the_anchor_or_older_than_its_start() {
        let page = comp_rows(0, 20);
        let anchor = ("c12".to_string(), BASE - 12 * 60_000);
        assert!(
            comp_backfill_done(&page, Some(&anchor), None, 1),
            "the anchor row is on the page"
        );
        // The anchor left the history: rows 15+ are older than its start.
        let gone = ("gone".to_string(), BASE - 15 * 60_000 + 1);
        assert!(comp_backfill_done(&page, Some(&gone), None, 1));
        // An anchor older than the whole page: keep paging.
        let older = ("c30".to_string(), BASE - 30 * 60_000);
        assert!(!comp_backfill_done(&page, Some(&older), None, 1));
    }

    #[test]
    fn comp_backfill_done_only_past_the_act_edge() {
        let page = comp_rows(0, 20);
        let edge = opd1_core::mystats::ACT_EDGE_MS;
        // The act starts a day (My Stats' edge) after the point between rows 7
        // and 8: rows 8 and older are before `start - edge`.
        let act_start = BASE - 7 * 60_000 - 30_000 + edge;
        assert!(comp_backfill_done(&page, None, Some(act_start), 1));
        // Rows before the act's start but within the edge still count for My
        // Stats (Riot's rollout is hours off valorant-api's date): keep paging.
        let act_start = BASE - 7 * 60_000 - 30_000;
        assert!(!comp_backfill_done(&page, None, Some(act_start), 1));
        // Every row of the page is inside the act: keep paging.
        let act_start = BASE - 25 * 60_000;
        assert!(!comp_backfill_done(&page, None, Some(act_start), 1));
        // No act known: only the other bounds apply.
        assert!(!comp_backfill_done(&page, None, None, 1));
    }

    #[test]
    fn comp_backfill_done_on_the_tenth_page() {
        let page = comp_rows(0, 20);
        assert!(!comp_backfill_done(
            &page,
            None,
            None,
            COMP_BACKFILL_MAX_PAGES - 1
        ));
        assert!(comp_backfill_done(
            &page,
            None,
            None,
            COMP_BACKFILL_MAX_PAGES
        ));
    }

    #[test]
    fn comp_anchor_keys_are_per_account_and_apart_from_the_match_anchors() {
        assert_eq!(comp_anchor_key("p1"), "self_comp_anchor:p1");
        assert_ne!(comp_anchor_key("p1"), comp_anchor_key("p2"));
        assert_ne!(comp_anchor_key("p1"), anchor_key("p1"));
    }

    #[test]
    fn backfill_take_stops_at_anchor_id() {
        let page = entries(20);
        let anchor = ("e12".to_string(), BASE - 12 * 60_000);
        let t = backfill_take(&page, 0, None, Some(&anchor), None, 0);
        assert_eq!(t.entries.len(), 12, "entries 0..12, the anchor excluded");
        assert_eq!(t.entries[11].match_id, "e11");
        assert!(t.done);
    }

    #[test]
    fn backfill_take_stops_older_than_anchor() {
        // The anchor match left the history (its id is not on the page);
        // entries 15+ are older than its start.
        let page = entries(20);
        let anchor = ("gone".to_string(), BASE - 15 * 60_000 + 1);
        let t = backfill_take(&page, 0, None, Some(&anchor), None, 0);
        assert_eq!(t.entries.len(), 15);
        assert_eq!(t.entries[14].match_id, "e14");
        assert!(t.done);
    }

    #[test]
    fn backfill_take_first_run_stops_at_act_start() {
        // The act starts between entries 7 and 8: entry 7 is in, 8 is older.
        let page = entries(20);
        let act_start = BASE - 7 * 60_000 - 30_000;
        let t = backfill_take(&page, 0, None, None, Some(act_start), 0);
        assert_eq!(t.entries.len(), 8);
        assert_eq!(t.entries[7].match_id, "e7");
        assert!(t.done);
    }

    #[test]
    fn backfill_take_first_run_caps_at_200() {
        let page = entries_from(190, 20);
        let act_start = BASE - 10_000 * 60_000; // far back: all inside
        let t = backfill_take(&page, 190, None, None, Some(act_start), 190);
        assert_eq!(t.entries.len(), 10, "190 scanned + 10 = the cap");
        assert!(t.done);
        // A full page that lands exactly on the cap is done at once — no
        // 11th history request for an empty page.
        let page = entries_from(180, 20);
        let t = backfill_take(&page, 180, None, None, Some(act_start), 180);
        assert_eq!(t.entries.len(), 20);
        assert!(t.done);
        // Below the cap, a full page in the act just continues.
        let t = backfill_take(&entries(20), 0, None, None, Some(act_start), 0);
        assert_eq!(t.entries.len(), 20);
        assert!(!t.done);
        // No act tables: the cap alone bounds the first run.
        let t = backfill_take(&entries(20), 0, None, None, None, 0);
        assert!(!t.done);
        let t = backfill_take(&entries_from(190, 20), 190, None, None, None, 190);
        assert_eq!(t.entries.len(), 10);
        assert!(t.done);
    }

    #[test]
    fn backfill_take_short_page_is_done() {
        // `Total` absent: a page shorter than the page size stands in for
        // the end of the history.
        let page = entries(6);
        let anchor = ("e99".to_string(), BASE - 99 * 60_000);
        let t = backfill_take(&page, 0, None, Some(&anchor), None, 0);
        assert_eq!(ids(&t), ["e0", "e1", "e2", "e3", "e4", "e5"]);
        assert!(t.done, "the history ended");
        // A full page with no stop hit is not done; an empty one is.
        assert!(!backfill_take(&entries(20), 0, None, Some(&anchor), None, 0).done);
        assert!(backfill_take(&[], 0, None, Some(&anchor), None, 0).done);
    }

    #[test]
    fn backfill_take_short_page_before_the_total_is_not_done() {
        // Riot answered with 15 entries of a 40-match history (clamping or a
        // server-side page cap): the entries behind the page are still to
        // read, so the run goes on — a gap here would be permanent.
        let page = entries(15);
        let anchor = ("e99".to_string(), BASE - 99 * 60_000);
        let t = backfill_take(&page, 0, Some(40), Some(&anchor), None, 0);
        assert_eq!(t.entries.len(), 15, "the whole page is taken");
        assert!(!t.done, "a short page short of Total is not the end");
        // The same for a first run.
        let act_start = BASE - 10_000 * 60_000;
        let t = backfill_take(&page, 0, Some(40), None, Some(act_start), 0);
        assert_eq!(t.entries.len(), 15);
        assert!(!t.done);
        // A short page in the middle, and an exactly-full page short of Total.
        let t = backfill_take(&entries_from(15, 15), 15, Some(40), Some(&anchor), None, 15);
        assert!(!t.done, "15 + 15 = 30 < 40");
        let t = backfill_take(&entries(20), 0, Some(40), Some(&anchor), None, 0);
        assert!(!t.done, "20 < 40");
    }

    #[test]
    fn backfill_take_reaching_the_total_is_done() {
        let anchor = ("e99".to_string(), BASE - 99 * 60_000);
        // The last page holds the last 10 of 40 (index 30 + 10 = Total).
        let t = backfill_take(&entries_from(30, 10), 30, Some(40), Some(&anchor), None, 30);
        assert_eq!(t.entries.len(), 10);
        assert!(t.done);
        // A full page that ends exactly on Total is the last one too: no
        // empty extra request.
        let t = backfill_take(&entries_from(20, 20), 20, Some(40), Some(&anchor), None, 20);
        assert_eq!(t.entries.len(), 20);
        assert!(t.done);
        // A short page that reaches Total is done, and so is one past it
        // (Total lower than the index: the history shrank meanwhile).
        let t = backfill_take(&entries_from(25, 15), 25, Some(40), Some(&anchor), None, 25);
        assert!(t.done);
        let t = backfill_take(&entries_from(45, 5), 45, Some(40), Some(&anchor), None, 45);
        assert!(t.done);
        // An empty page ends the run whatever Total claims, or it would page
        // forever past the end.
        assert!(backfill_take(&[], 30, Some(99), Some(&anchor), None, 30).done);
    }

    #[test]
    fn history_ended_rules() {
        // With Total: only reaching it ends the history.
        assert!(!history_ended(0, 15, Some(40)));
        assert!(!history_ended(20, 19, Some(40)));
        assert!(history_ended(20, 20, Some(40)));
        assert!(history_ended(25, 15, Some(40)));
        assert!(history_ended(0, 20, Some(20)));
        assert!(history_ended(0, 5, Some(0)), "a zero Total is the end");
        // No overflow at the top of the range.
        assert!(history_ended(u32::MAX, 20, Some(u32::MAX)));
        // Without Total: a short page is the end, a full one is not.
        assert!(history_ended(0, 19, None));
        assert!(!history_ended(0, 20, None));
        // An empty page is always the end.
        assert!(history_ended(0, 0, None));
        assert!(history_ended(0, 0, Some(99)));
    }

    #[test]
    fn anchor_json_roundtrips_and_bad_json_is_no_anchor() {
        let a = ("m1".to_string(), 1_234);
        assert_eq!(
            anchor_json(&a),
            r#"{"matchId":"m1","startMs":1234}"#,
            "the stored shape"
        );
        assert_eq!(parse_anchor(&anchor_json(&a)), Some(a));
        assert_eq!(parse_anchor("not json"), None);
        assert_eq!(parse_anchor(r#"{"matchId":"","startMs":1}"#), None);
        assert_eq!(parse_anchor(r#"{"matchId":"x"}"#), None);
    }
}
