//! Per-player "sync, don't refetch" (CLAUDE.md "Smart data", R33) and the
//! background worker that walks the live roster.
//!
//! Per player, in order:
//! 1. `comp_updates(0..5)` — the last-5 competitive match ids + RR deltas
//!    (one mmr-bucket call; upserted, and indexed as competitive history).
//! 2. `match_history(0..2, any queue)` — each player's last two matches in
//!    any mode (Task 6's party detection needs them; indexed).
//! 3. `match_details` only for ids not already in the DB (immutable —
//!    fetched once, stored forever as compact rows), up to
//!    [`DETAILS_CONCURRENCY`] in flight (the session's token bucket still
//!    paces the actual requests).
//! 4. `mmr` only when `mmr_refresh_needed(latest comp match)` — no new
//!    competitive match means the rank can't have changed, unless the stored
//!    snapshot predates the current snapshot version (refetched once).
//!
//! Any failure other than `ClientGone` is logged and skipped: the fields
//! it would have filled become Missing (`—`) through the card builder's
//! "attempted" rule, never a stuck skeleton. `ClientGone` aborts the whole
//! run — the poll loop notices the client is gone on its own next tick.

use std::collections::HashSet;
use std::sync::Arc;

use serde::Deserialize;

use tokio::task::JoinSet;

use opd1_core::db::repo::{CompUpdateRow, HistoryEntry, MmrSnapshot};
use opd1_riot::endpoints::MatchHistoryResponse;
use opd1_riot::to_core::{comp_update_rows_from_response, match_from_details};
use opd1_riot::{RiotError, RiotSession, VisiblePuuid};

use crate::logbuf::short;
use crate::roster::{extract_loadouts, extract_player_loadout};
use crate::state::{lock, now_ms, AppState, Job};
use crate::store::{mmr_refresh_needed, save_player, trim_mmr, PlayerUpdate, SELF_LOADOUT_KEY};

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

/// The Riot client went away mid-sync.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Gone;

/// What a per-player sync learned beyond the DB writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncOutcome {
    /// The player's match-history call answered *and* was written to
    /// SQLite. Party detection reads the stored history, so a player whose
    /// history failed this cycle (the call or the write) must not take part:
    /// the stored one may be from an earlier session (a false stripe).
    pub history_ok: bool,
}

/// `ClientGone` -> [`Gone`]; anything else is logged and swallowed.
fn check(err: RiotError, what: &str, puuid: &str) -> Result<(), Gone> {
    match err {
        RiotError::ClientGone => Err(Gone),
        other => {
            log::warn!("{what} for {} failed: {other}", short(puuid));
            Ok(())
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

/// `comp_updates(start..end)` -> DB. Returns the rows (`None` on failure).
async fn sync_comp_updates(
    st: &Arc<AppState>,
    session: &RiotSession,
    puuid: &str,
    end: u32,
) -> Result<Option<Vec<CompUpdateRow>>, Gone> {
    match session.comp_updates(puuid, Some(0), Some(end)).await {
        Ok(resp) => {
            let rows = comp_update_rows_from_response(&resp);
            let (p, r) = (puuid.to_string(), rows.clone());
            st.db(move |repo| {
                repo.upsert_comp_updates(&p, &r)?;
                repo.index_history(&p, &comp_history_entries(&r))
            })
            .await;
            Ok(Some(rows))
        }
        Err(e) => check(e, "comp updates", puuid).map(|()| None),
    }
}

/// Fetches match-details for every id the DB doesn't have yet and stores
/// the compact rows.
pub async fn fetch_missing_details(
    st: &Arc<AppState>,
    session: &Arc<RiotSession>,
    ids: Vec<String>,
) -> Result<(), Gone> {
    let ids = dedupe(ids);
    if ids.is_empty() {
        return Ok(());
    }
    let Some(missing) = st.db(move |repo| repo.new_match_ids(&ids)).await else {
        return Ok(());
    };
    if missing.is_empty() {
        return Ok(());
    }
    log::debug!("fetching {} match details", missing.len());

    let mut pending = missing.into_iter();
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
    while let Some(joined) = set.join_next().await {
        match joined {
            Ok((_, Ok(details))) => {
                let (m, teams, players) = match_from_details(&details);
                drop(details);
                st.db(move |repo| repo.upsert_match(&m, &teams, &players))
                    .await;
            }
            Ok((_, Err(RiotError::ClientGone))) => {
                gone = true;
                break;
            }
            Ok((id, Err(e))) => log::warn!("match details {} failed: {e}", short(&id)),
            Err(e) => log::warn!("match details task failed: {e}"),
        }
        spawn_next(&mut set, &mut pending);
    }
    if gone {
        Err(Gone)
    } else {
        Ok(())
    }
}

/// The full per-player sync (see the module docs for the call order).
pub async fn sync_player(
    st: &Arc<AppState>,
    session: &Arc<RiotSession>,
    puuid: &str,
) -> Result<SyncOutcome, Gone> {
    let mut ids = Vec::new();

    let comp = sync_comp_updates(st, session, puuid, 5).await?;
    if let Some(rows) = &comp {
        ids.extend(rows.iter().map(|r| r.match_id.clone()));
    }

    let history_ok = match session.match_history(puuid, Some(0), Some(2), None).await {
        Ok(resp) => {
            let entries = history_entries(&resp);
            ids.extend(entries.iter().map(|e| e.match_id.clone()));
            let p = puuid.to_string();
            // A SQLite failure (`None`) leaves the earlier session's rows.
            st.db(move |repo| repo.index_history(&p, &entries))
                .await
                .is_some()
        }
        Err(e) => {
            check(e, "match history", puuid)?;
            false
        }
    };

    fetch_missing_details(st, session, ids).await?;

    // MMR only when a new competitive match appeared since the snapshot (or
    // the snapshot is from an older shape).
    // Skipped entirely when comp_updates failed (it shares the mmr rate
    // bucket; the stored snapshot, if any, keeps showing).
    if let Some(rows) = comp {
        let latest = latest_comp_match(&rows);
        let (p, l) = (puuid.to_string(), latest.clone());
        let needs = st
            .db(move |repo| mmr_refresh_needed(repo, &p, &l))
            .await
            .unwrap_or(true);
        if needs {
            match session.mmr(puuid).await {
                Ok(resp) => {
                    let snap = MmrSnapshot {
                        puuid: puuid.to_string(),
                        json: trim_mmr(&resp).to_string(),
                        last_comp_match_id: Some(latest),
                        fetched_ms: now_ms(),
                    };
                    st.db(move |repo| repo.save_mmr_snapshot(&snap)).await;
                }
                Err(e) => check(e, "mmr", puuid)?,
            }
        }
    }
    Ok(SyncOutcome { history_ok })
}

/// Player Details' deeper sync: the last [`TEAMMATE_WINDOW`] competitive
/// updates, then details for the newest 8 + the current act's matches
/// (only those not in the DB). Also picks up the core-game loadouts (the
/// equipped skin ids; their names resolve separately, from valorant-api,
/// `details::resolve_skins`) if the live loop's fetch of them failed.
pub async fn sync_details(
    st: &Arc<AppState>,
    session: &Arc<RiotSession>,
    puuid: &str,
) -> Result<(), Gone> {
    let season = st.current_act().map(|a| a.id);
    if let Some(rows) = sync_comp_updates(st, session, puuid, TEAMMATE_WINDOW).await? {
        let targets = details_targets(&rows, season.as_deref());
        fetch_missing_details(st, session, targets).await?;
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
                let mut live = lock(&st.live);
                if live.coregame_match_id.as_deref() == Some(match_id.as_str()) {
                    live.loadouts = loadouts;
                }
            }
            Err(e) => check(e, "loadouts", puuid)?,
        }
    }
    Ok(())
}

/// The live roster's background worker (worker `id`, claimed via
/// `LiveModel::try_start_worker`). Each job — one `names()` call for the
/// visible players still unnamed, or one player's sync in display order —
/// is chosen by `LiveModel::next_job` under the model's lock, which also
/// releases the claim when nothing is left (R45: no roster change can
/// slip between "nothing to do" and the task ending). Roster changes
/// mid-run are simply picked up by the next job.
pub async fn run_worker(st: Arc<AppState>, session: Arc<RiotSession>, id: u64) {
    loop {
        let job = lock(&st.live).next_job(id);
        let outcome = match job {
            None => return,
            Some(Job::Names(needed)) => names_step(&st, &session, needed).await,
            Some(Job::Sync(puuid)) => sync_step(&st, &session, puuid).await,
        };
        if outcome.is_err() {
            lock(&st.live).release_worker(id);
            return;
        }
    }
}

async fn sync_step(
    st: &Arc<AppState>,
    session: &Arc<RiotSession>,
    puuid: String,
) -> Result<(), Gone> {
    let outcome = sync_player(st, session, &puuid).await?;
    let (p, me) = (puuid.clone(), session.self_puuid().to_string());
    // A failed read keeps the bundle already on screen.
    let db = st
        .db(move |repo| crate::store::load_player_db(repo, Some(&me), &p))
        .await;
    {
        let mut live = lock(&st.live);
        live.synced.insert(puuid.clone());
        // Party marks need the stored history to be this cycle's: the
        // history was written and the bundle built from it was read back.
        // Either SQLite step failing leaves the slot on earlier rows.
        if outcome.history_ok && db.is_some() {
            live.party_ready.insert(puuid.clone());
        }
        if let (Some(db), Some(slot)) =
            (db, live.slots.iter_mut().find(|s| s.roster.puuid == puuid))
        {
            slot.db = db;
        }
    }
    log::debug!("synced {}", short(&puuid));
    st.publish();
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
        Err(e) => {
            log::warn!("names for {} players failed: {e}", asked.len());
            Vec::new()
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
        live.names_attempted.extend(asked.iter().cloned());
        for (p, n, t) in &learned {
            live.known_names.insert(p.clone(), (n.clone(), t.clone()));
        }
    }
    log::debug!("names: {} of {} resolved", learned.len(), asked.len());
    save_names(st, learned).await;
    st.publish();
    Ok(())
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

/// R90: the user's own equipped skins, read once per connect from
/// `pd .../personalization/v2/players/{self}/playerloadout` (a read-only
/// `GET`, always the user's own puuid): kept in the live model for their
/// Details, and in `app_state` (only when it changed) so it still shows
/// while VALORANT is closed. A failure is logged and not retried until the
/// next connect.
pub async fn fetch_self_loadout(
    st: &Arc<AppState>,
    session: &Arc<RiotSession>,
) -> Result<(), Gone> {
    let me = session.self_puuid().to_string();
    let resp = match session.player_loadout(&me).await {
        Ok(resp) => resp,
        Err(e) => return check(e, "player loadout", &me),
    };
    let loadout = extract_player_loadout(&resp);
    let stored = serde_json::to_string(&loadout).unwrap_or_default();
    lock(&st.live).self_loadout = Some(loadout);
    st.db(move |repo| {
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
}
