//! IPC commands (all registered in `lib.rs`). The UI never polls Riot:
//! live data arrives through the `"live"` event; these are the pull-side
//! entry points.
//!
//! - `meta()` — agent/map/tier names + tier colors (R21).
//! - `live_state()` — the last emitted `LiveState` (the UI calls it once on
//!   start so it never misses the first snapshot).
//! - `player_details(puuid)` — cached details now; a background sync (and
//!   the worker's sync of that player) then emits `"details"` if it found
//!   more.
//! - `close_details(puuid?)` — the UI left the Details page: the open Details
//!   is forgotten, so nothing is rebuilt or emitted for it. A close for
//!   another player than the open one is ignored (R163).
//! - `match_details(matchId, focusPuuid)` — one match as the Match Details
//!   scoreboard, from the DB (fetched once when it isn't stored yet).
//! - `close_match(matchId?)` — the UI left the Match Details page: the open
//!   match is forgotten, so no `"match"` event is built or emitted for it. A
//!   close for another match than the open one is ignored (R163).
//! - `my_stats()` — the user's My Stats for the current act, from the DB
//!   only (no Riot call); `null` while the user or the act is unknown.
//! - `copy_log()` — one header line (version, Windows, stage, shard, client,
//!   act) and the last 500 log lines (R30; the UI copies them).
//! - `ui_log(level, message)` — a UI failure into the same log (B1): never
//!   fails, sanitised and rate-limited here.
//! - `dump_raw()` — redacted raw responses (the last 2xx and the last error
//!   of each endpoint) and the log to `app_data/debug/`.

use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tauri::State;

use opd1_core::db::repo::NameAsk;
use opd1_core::model::{DisplayName, Field, LiveState, MatchDetailsView, MyStats, PlayerDetails};
use opd1_core::presence::Stage;
use opd1_core::stats::act_label;
use opd1_riot::assets::Meta;
use opd1_riot::RiotSession;

use crate::logbuf::short;
use crate::rank_retry::HOLD_SLACK;
use crate::state::AppState;
use crate::sync::DetailsSync;

type AppStateRef<'a> = State<'a, Arc<AppState>>;

pub(crate) fn is_uuid(s: &str) -> bool {
    s.len() == 36
        && s.char_indices().all(|(i, c)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                c == '-'
            } else {
                c.is_ascii_hexdigit()
            }
        })
}

/// Both ids of a `match_details` call must be uuids — they end up in a DB
/// key and a Riot URL path, the same check `player_details` applies to its
/// puuid. The match id is checked first.
fn check_match_args(match_id: &str, focus_puuid: &str) -> Result<(), String> {
    if !is_uuid(match_id) {
        return Err("invalid match id".into());
    }
    if !is_uuid(focus_puuid) {
        return Err("invalid player id".into());
    }
    Ok(())
}

#[tauri::command]
pub async fn meta(state: AppStateRef<'_>) -> Result<Meta, String> {
    state.ensure_tables(false).await?;
    state.assets.meta().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn live_state(state: AppStateRef<'_>) -> Result<LiveState, String> {
    Ok(state.live_state())
}

#[tauri::command]
pub async fn player_details(
    puuid: String,
    state: AppStateRef<'_>,
) -> Result<PlayerDetails, String> {
    player_details_for(state.inner(), &puuid).await
}

/// `player_details` without the Tauri wrapper: the cached details now, with
/// two background jobs behind them, each emitting `"details"` when it
/// changed the result (and the worker refreshes them after syncing this
/// player, `details::refresh`):
/// - the Riot sync ([`crate::sync::sync_details`]), at most every 120s per
///   player (`claim_details_sync`);
/// - the skin-name fetch (R83, R97), whenever a loadout skin's name isn't
///   known yet — from valorant-api only, so it is *not* gated by the 120s
///   claim. While it runs the answer carries that weapon's key absent
///   (resolving); when it ends, success or failure, the details are rebuilt
///   and a name still unknown is `null`.
pub(crate) async fn player_details_for(
    st: &Arc<AppState>,
    puuid: &str,
) -> Result<PlayerDetails, String> {
    if !is_uuid(puuid) {
        return Err("invalid player id".into());
    }
    let (st, puuid) = (st.clone(), puuid.to_string());

    // Marked before the build, so the answer itself has the resolving keys
    // absent (R97).
    // The guard ends the mark when dropped: on an early error, and if the
    // resolving task panics (R97).
    let resolving =
        crate::details::skins_pending(&st, &puuid).then(|| st.begin_skin_resolve(&puuid));
    // R163: the ticket is taken before the build, so a Back (or a newer open)
    // that happens while it runs voids this open.
    let ticket = st.begin_open_details(&puuid);
    let details = crate::details::build(&st, &puuid).await?;
    // The open view: every later rebuild of it (the jobs below, and the
    // worker's sync of this player, R23) is emitted only when it changed.
    if !st.set_open_details(ticket, &details) {
        // The page was left (or a newer open took over) while this build ran:
        // nobody sees this answer, so no name asks, skin fetch or sync for it
        // (R163). The caller still gets its answer; the UI drops it.
        return Ok(details);
    }
    // R150: the names this page shows as missing, asked between matches.
    st.queue_name_fill(details_asks(&details));

    if let Some(guard) = resolving {
        // The mark ends before the rebuild, so a name still unknown is `null`.
        crate::details::resolve_skins_in_background(&st, &puuid, guard);
    }

    if let Some(session) = st.session() {
        if st.claim_details_sync(&puuid) {
            let puuid = puuid.clone();
            let st = st.clone();
            tauri::async_runtime::spawn(async move {
                details_sync_job(st, session, puuid, HOLD_SLACK).await;
            });
        }
    }
    Ok(details)
}

/// How many times the Details sync waits for a hold and runs again (R160).
const DETAILS_HOLD_RERUNS: u32 = 3;

/// The Riot sync behind an open Player Details page (the caller holds the
/// 120 s claim, `claim_details_sync`), and what follows it: the card and the
/// page are rebuilt, the names it lacks are asked.
///
/// R160: a sync that got nothing because a request family was on hold
/// (`DetailsSync::Held`, nothing sent), or whose competitive-updates call
/// failed (`Failed`), gives the claim back: it did not sync, so the next open
/// may try at once instead of waiting out the 120 s. After a hold the sync
/// runs again once the hold is over (`slack` after it), but only while this
/// player's page is still the open one, and at most [`DETAILS_HOLD_RERUNS`]
/// times. A failure is not run again by itself (the next open does).
pub(crate) async fn details_sync_job(
    st: Arc<AppState>,
    session: Arc<RiotSession>,
    puuid: String,
    slack: Duration,
) {
    let mut session = session;
    let mut reruns = 0;
    loop {
        let ended = crate::sync::sync_details(&st, &session, &puuid).await;
        let Ok(ended) = ended else {
            // The Riot client went away: nothing was synced.
            st.release_details_sync(&puuid);
            return;
        };
        // The card on screen may have gained matches too — and, for the
        // user's own page, My Stats (B2).
        st.reload_slot(&puuid).await;
        crate::my_stats::notify_if_own(&st, session.self_puuid(), &puuid);
        crate::details::refresh(&st, &puuid).await;
        // R153: the sync may have stored matches that put new
        // teammates into the Top 3; the ones now shown unnamed are
        // asked too (a puuid already queued or answered blank is not
        // asked again: `queue_name_fill`, `Repo::names_to_ask`).
        if let Some(page) = st.open_details_of(&puuid) {
            st.queue_name_fill(details_asks(&page));
        }
        match ended {
            DetailsSync::Done => return,
            DetailsSync::Failed => {
                st.release_details_sync(&puuid);
                return;
            }
            DetailsSync::Held(left) => {
                st.release_details_sync(&puuid);
                // One waiting rerun per player: a page reopened during the wait
                // has its own job, which must not leave two.
                if reruns >= DETAILS_HOLD_RERUNS {
                    return;
                }
                let Some(waiting) = st.begin_details_rerun(&puuid) else {
                    return;
                };
                reruns += 1;
                log::debug!(
                    "details sync of {} waits for the hold ({left:?} left)",
                    short(&puuid)
                );
                tokio::time::sleep(left + slack).await;
                drop(waiting);
                // Only for the page still open, and only if nobody else
                // synced this player meanwhile (a reopen's own job).
                if !st.details_open_for(&puuid) || !st.claim_details_sync(&puuid) {
                    return;
                }
                let Some(current) = st.session() else {
                    st.release_details_sync(&puuid);
                    return;
                };
                session = current;
            }
        }
    }
}

/// R150: the names a Player Details page lacks: the hero's own (a card with
/// no name at all, or one shown hidden without a remembered name) and the
/// Top 3 teammates shown as Hidden Player or by their agent (no name in any
/// stored match or `players` row). On demand — only puuids never asked
/// before. Queued at any stage and sent between matches only; a player shown
/// hidden is asked too (R153), and whatever comes back still shows in the
/// hidden style (remembered), never as a plain name (R18).
fn details_asks(details: &PlayerDetails) -> Vec<NameAsk> {
    let unnamed = |name: &DisplayName| {
        matches!(
            name,
            DisplayName::HiddenPlayer | DisplayName::HiddenAgent { .. }
        )
    };
    let hero_unnamed = match &details.card.name {
        Field::Missing => true,
        Field::Value(name) => unnamed(name),
        _ => false,
    };
    let hero = (hero_unnamed && !details.card.is_self).then(|| details.card.puuid.clone());
    let mates = details
        .top_teammates
        .iter()
        .filter(|t| unnamed(&t.name))
        .map(|t| t.puuid.clone());
    hero.into_iter()
        .chain(mates)
        .map(|puuid| NameAsk {
            puuid,
            match_start_ms: None,
        })
        .collect()
}

/// The UI left the Player Details page (Back, or another page): nothing is
/// open any more, so a closed page is never refreshed or emitted to. The UI
/// sends the player's `puuid` (R163): a close for any other player than the
/// one open or being opened is ignored, and an open still being built for
/// this player is void.
#[tauri::command]
pub fn close_details(puuid: Option<String>, state: AppStateRef<'_>) {
    close_details_key_for(state.inner(), puuid.as_deref());
}

/// `close_details` without the Tauri wrapper and without a key: closes
/// whatever is open.
#[cfg(test)]
pub(crate) fn close_details_for(st: &Arc<AppState>) {
    st.close_details(None);
}

/// `close_details` without the Tauri wrapper.
pub(crate) fn close_details_key_for(st: &Arc<AppState>, puuid: Option<&str>) {
    st.close_details(puuid);
}

/// The UI left the Match Details page (Back, or another page): nothing is
/// open any more, so a closed scoreboard is never rebuilt or emitted to.
/// Closing with nothing open does nothing; a later `match_details` or
/// `player_details` call works as before. The UI sends the `match_id`
/// (R163): a close for any other match than the one open or being opened is
/// ignored, and an open still being built for this match is void.
#[tauri::command]
pub fn close_match(match_id: Option<String>, state: AppStateRef<'_>) {
    close_match_key_for(state.inner(), match_id.as_deref());
}

/// `close_match` without the Tauri wrapper and without a key: closes
/// whatever is open.
#[cfg(test)]
pub(crate) fn close_match_for(st: &Arc<AppState>) {
    st.close_match(None);
}

/// `close_match` without the Tauri wrapper.
pub(crate) fn close_match_key_for(st: &Arc<AppState>, match_id: Option<&str>) {
    st.close_match(match_id);
}

#[tauri::command]
pub async fn match_details(
    match_id: String,
    focus_puuid: String,
    state: AppStateRef<'_>,
) -> Result<MatchDetailsView, String> {
    check_match_args(&match_id, &focus_puuid)?;
    crate::match_details::load(state.inner(), &match_id, &focus_puuid).await
}

#[tauri::command]
pub async fn my_stats(state: AppStateRef<'_>) -> Result<Option<MyStats>, String> {
    build_my_stats(state.inner()).await
}

/// `my_stats` without the Tauri wrapper: computed on demand from the DB,
/// never a Riot call. It first awaits `ensure_tables`, the gate `meta()`
/// uses too — a no-op once the asset tables are loaded; at startup it
/// waits for the first load instead of racing it (the acts and map names
/// come from those tables). `None` until the user's puuid is known (first
/// run) or while no act is current. The computation itself is
/// [`crate::my_stats::compute`], which the `"mystats"` push shares (B2).
pub async fn build_my_stats(st: &Arc<AppState>) -> Result<Option<MyStats>, String> {
    // Tables unavailable just means no act yet (-> `None` below).
    let _ = st.ensure_tables(false).await;
    crate::my_stats::compute(st).await
}

/// `(async)`: runs on the async runtime, not on the UI thread, so the UI never
/// waits on the app locks the header reads (the live state, the acts, the
/// session).
#[tauri::command(async)]
pub fn copy_log(state: AppStateRef<'_>) -> String {
    log_text(state.inner())
}

/// The text `copy_log` returns and `dump_raw` writes as `log.txt`: the header
/// line, built fresh, then the last [`crate::logbuf::CAPACITY`] log lines.
pub(crate) fn log_text(st: &AppState) -> String {
    format!(
        "{}\n{}",
        format_header(&log_header(st)),
        crate::logbuf::snapshot()
    )
}

/// The parts of the log's first line. A part that is not known is `None` and
/// prints as `—`. Nothing here is a secret or a name: the app and Windows
/// versions, the stage, the region/shard, the game's release label and the
/// act's id and label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LogHeader {
    /// `CARGO_PKG_VERSION`.
    pub app_version: String,
    /// `major.minor.build`; `None` off Windows.
    pub windows: Option<String>,
    /// The stage as the wire spells it (`menus`, `pregame`, `ingame`, `closed`).
    pub stage: &'static str,
    pub settled: bool,
    /// The `pd` shard, the glz region and the glz shard; `None` with no session.
    pub shards: Option<[String; 3]>,
    /// The `X-Riot-ClientVersion` and where it came from; `None` with no session.
    pub client: Option<(String, &'static str)>,
    /// The current act; `None` until the asset tables are loaded.
    pub act: Option<HeaderAct>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HeaderAct {
    pub id: String,
    /// The short label the cards use (`V26 A4`).
    pub label: String,
    /// `riot` or `valorant-api`.
    pub source: &'static str,
}

/// How [`Stage`] is spelled on the wire (`LiveState`'s `stage`).
fn stage_wire(stage: Stage) -> &'static str {
    match stage {
        Stage::Closed => "closed",
        Stage::Menus => "menus",
        Stage::Pregame => "pregame",
        Stage::Ingame => "ingame",
    }
}

/// `major.minor.build` of the running Windows (`RtlGetVersion`: the real
/// number, not the compatibility shim's); `None` off Windows.
#[cfg(windows)]
fn windows_build() -> Option<String> {
    let v = windows_version::OsVersion::current();
    Some(format!("{}.{}.{}", v.major, v.minor, v.build))
}

#[cfg(not(windows))]
fn windows_build() -> Option<String> {
    None
}

/// Reads the header's parts from the live app, now.
fn log_header(st: &AppState) -> LogHeader {
    let live = st.live_state();
    let session = st.session();
    LogHeader {
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        windows: windows_build(),
        stage: stage_wire(live.stage),
        settled: live.settled,
        shards: session.as_ref().map(|s| {
            let shards = s.shards();
            [
                shards.pd.clone(),
                shards.glz_region.clone(),
                shards.glz_shard.clone(),
            ]
        }),
        client: session.as_ref().map(|s| {
            let (version, source) = s.client_version_and_source();
            (version, source.as_str())
        }),
        act: st.current_act().map(|a| HeaderAct {
            label: act_label(&a),
            id: a.id,
            source: st.act_source(),
        }),
    }
}

/// How many characters of each free-text part the header keeps: it must stay
/// one short line whatever a source hands over.
const HEADER_PART_MAX: usize = 40;
const HEADER_VERSION_MAX: usize = 64;

/// `s` on one line (whitespace runs collapsed) and at most `max` characters,
/// the closing `…` included; `—` when nothing is left.
fn header_part(s: &str, max: usize) -> String {
    let one_line = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.is_empty() {
        return "—".to_string();
    }
    if one_line.chars().count() <= max {
        return one_line;
    }
    let kept: String = one_line.chars().take(max.saturating_sub(1)).collect();
    format!("{kept}…")
}

/// The one header line:
/// `OPD1 TRCKER <version> · Windows <major.minor.build> · stage <stage> ·
/// settled <bool> · shard <pd>/<glz region>/<glz shard> · client <version>
/// (<source>) · act <first 8 of the act id> <label> (<riot|valorant-api>)`,
/// with `—` for every part that is unknown.
pub(crate) fn format_header(h: &LogHeader) -> String {
    let windows = h
        .windows
        .as_deref()
        .map_or("—".to_string(), |w| header_part(w, HEADER_PART_MAX));
    let shard = h.shards.as_ref().map_or("—".to_string(), |parts| {
        parts
            .iter()
            .map(|p| header_part(p, HEADER_PART_MAX))
            .collect::<Vec<_>>()
            .join("/")
    });
    let client = h.client.as_ref().map_or("—".to_string(), |(v, source)| {
        format!("{} ({source})", header_part(v, HEADER_VERSION_MAX))
    });
    let act = h.act.as_ref().map_or("—".to_string(), |a| {
        let id: String = a.id.chars().take(8).collect();
        format!(
            "{} {} ({})",
            header_part(&id, 8),
            header_part(&a.label, HEADER_PART_MAX),
            a.source
        )
    });
    format!(
        "OPD1 TRCKER {} · Windows {windows} · stage {} · settled {} · shard {shard} · client {client} · act {act}",
        header_part(&h.app_version, HEADER_PART_MAX),
        h.stage,
        h.settled,
    )
}

/// B1: the UI's failure lines (`uilog.ts`) into the app's log, under the `ui`
/// module. Never fails and never panics: the page may be the thing that is
/// broken, and a rejected promise there is a new failure to report. It takes
/// no app state, so a UI error that arrives before `setup()` has managed the
/// state is still logged.
///
/// What Rust does to the text: secret-named keys and JWT-shaped strings are
/// redacted, whitespace is collapsed to one line, every full uuid is
/// shortened to 8 characters, and it is cut to 500 characters. What it cannot
/// do: it cannot recognise a player's name in a free text. The barrier for
/// names is the UI (`ui/src/lib/uilog.ts` sends only an Error's name and
/// message, a string, or an object's key names, never a card or roster
/// value). More than 20 ring lines in a minute are dropped and counted (one
/// `ui_log: N lines dropped` line, itself one of the 20, when the window
/// opens again).
#[tauri::command]
pub fn ui_log(level: String, message: String) {
    crate::logbuf::log_ui(&level, &message);
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DumpResult {
    pub dir: String,
    pub files: u32,
}

/// Keeps a capture key safe as a file name (keys are already
/// `[a-z_]` today; this guards the generic-normalizer fallback).
fn file_stem(key: &str) -> String {
    let s: String = key
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if s.is_empty() {
        "unnamed".into()
    } else {
        s
    }
}

/// What the debug dump writes, as `(file name, contents)` (R158):
/// - `<key>.json`: the last 2xx body of each endpoint, redacted;
/// - `<key>.error.json` / `<key>.error.txt`: the last non-2xx answer of each
///   endpoint (status and Riot's `errorCode`), redacted and at most 4 KB;
/// - `log.txt`: exactly the text `copy_log` returns (the header line, then
///   the log lines) — written even with nothing captured, since the log is
///   what explains why there is nothing.
fn dump_files(
    captures: Vec<(String, String)>,
    errors: Vec<opd1_riot::raw_capture::ErrorDump>,
    log: String,
) -> Vec<(String, String)> {
    let mut files: Vec<(String, String)> = captures
        .into_iter()
        .map(|(key, body)| (format!("{}.json", file_stem(&key)), body))
        .collect();
    files.extend(
        errors
            .into_iter()
            .map(|e| (format!("{}.error.{}", file_stem(&e.key), e.ext), e.contents)),
    );
    files.push(("log.txt".to_string(), log));
    files
}

/// Whether `name` is an error file of an earlier dump (`<key>.error.json` /
/// `<key>.error.txt`).
fn is_error_dump_name(name: &str) -> bool {
    name.ends_with(".error.json") || name.ends_with(".error.txt")
}

/// Writes `files` into `dir` (created if needed); how many were written. The
/// error files of an earlier dump are removed first (only those, only in
/// `dir`): an error file carries no time, so one from an earlier run left
/// beside a fresh dump would blame an endpoint that works now.
fn write_dump(dir: &std::path::Path, files: &[(String, String)]) -> std::io::Result<u32> {
    std::fs::create_dir_all(dir)?;
    for entry in std::fs::read_dir(dir)?.flatten() {
        let is_stale_error = entry.file_type().is_ok_and(|t| t.is_file())
            && entry.file_name().to_str().is_some_and(is_error_dump_name);
        if is_stale_error {
            // Best effort: a file that cannot be removed is overwritten below
            // when its endpoint failed again, and otherwise stays.
            let _ = std::fs::remove_file(entry.path());
        }
    }
    let mut n = 0;
    for (name, contents) in files {
        std::fs::write(dir.join(name), contents)?;
        n += 1;
    }
    Ok(n)
}

#[tauri::command]
pub async fn dump_raw(state: AppStateRef<'_>) -> Result<DumpResult, String> {
    dump_raw_for(state.inner()).await
}

/// `dump_raw` without the Tauri wrapper. With no session (VALORANT closed, or
/// a connect that keeps failing) there are no captures, and the log is still
/// written — it is the part that says why.
pub(crate) async fn dump_raw_for(st: &Arc<AppState>) -> Result<DumpResult, String> {
    dump_raw_with(st, log_text(st)).await
}

/// [`dump_raw_for`] with the log text given (what `copy_log` returns:
/// [`log_text`]).
pub(crate) async fn dump_raw_with(st: &Arc<AppState>, log: String) -> Result<DumpResult, String> {
    // Already redacted by opd1-riot (tokens/JWTs/passwords).
    let (captures, errors) = match st.session() {
        Some(session) => (session.raw_captures(), session.raw_error_captures()),
        None => (Vec::new(), Vec::new()),
    };
    let files = dump_files(captures, errors, log);
    let dir = st.debug_dir.clone();
    let shown = dir.display().to_string();
    let files = tauri::async_runtime::spawn_blocking(move || write_dump(&dir, &files))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    log::info!("dump_raw: wrote {files} files");
    Ok(DumpResult { dir: shown, files })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    use opd1_core::db::open_db;
    use opd1_core::db::repo::{CompUpdateRow, Match, MatchPlayer, Repo};
    use opd1_core::stats::Act;
    use opd1_riot::assets::AssetCache;
    use serde_json::Value;

    use crate::my_stats::local_offset_ms;
    use crate::state::{now_ms, EventSink};

    const ME: &str = "aaaaaaaa-0000-0000-0000-000000000001";
    const DAY: i64 = 86_400_000;

    struct NoEvents;

    impl EventSink for NoEvents {
        fn live(&self, _: &LiveState) {}
        fn details(&self, _: &PlayerDetails) {}
        fn match_view(&self, _: &MatchDetailsView) {}
    }

    /// An app with an empty in-memory DB and asset tables that can't load
    /// (nothing listens on the base URL).
    fn app(self_puuid: Option<&str>, dir: &Path) -> Arc<AppState> {
        let repo = Repo::new(open_db(Path::new(":memory:")).unwrap());
        let assets = AssetCache::with_base(dir.join("assets"), "http://127.0.0.1:1".into());
        Arc::new(AppState::new(
            Box::new(NoEvents),
            repo,
            assets,
            dir.join("debug"),
            self_puuid.map(str::to_string),
        ))
    }

    fn act(id: &str, start_ms: i64, end_ms: i64) -> Act {
        Act {
            id: id.into(),
            name: "ACT".into(),
            episode_name: "V26".into(),
            start_ms,
            end_ms,
        }
    }

    fn comp_match(repo: &Repo, id: &str, season: &str, start_ms: i64) -> rusqlite::Result<()> {
        let m = Match {
            match_id: id.into(),
            map_id: "/Game/Maps/Ascent/Ascent".into(),
            queue_id: "competitive".into(),
            start_ms,
            winning_team: Some("Blue".into()),
            is_ranked: true,
            rounds_played: Some(20),
            game_length_ms: None,
        };
        let me = MatchPlayer {
            puuid: ME.into(),
            team_id: "Blue".into(),
            party_id: None,
            agent_id: Some("agent".into()),
            kills: 10,
            deaths: 5,
            assists: 2,
            score: 3000,
            rounds: Some(20),
            hs: 1,
            bs: 3,
            ls: 0,
            game_name: None,
            tag_line: None,
            competitive_tier: None,
            damage: None,
        };
        repo.upsert_match(&m, &[], &[me])?;
        repo.upsert_comp_updates(
            ME,
            &[CompUpdateRow {
                match_id: id.into(),
                season_id: season.into(),
                tier_after: 12,
                tier_before: Some(12),
                rr_after: 50,
                rr_earned: 18,
                map_id: None,
                start_ms,
            }],
        )
    }

    #[tokio::test]
    async fn my_stats_is_null_until_the_user_and_the_act_are_known() {
        let dir = tempfile::tempdir().unwrap();
        let now = now_ms();
        let unknown_user = app(None, dir.path());
        unknown_user.set_acts_for_test(vec![act("cur", now - DAY, now + DAY)]);
        assert_eq!(build_my_stats(&unknown_user).await, Ok(None));
        // Asset tables never loaded (offline first run): no current act.
        let no_act = app(Some(ME), dir.path());
        assert_eq!(build_my_stats(&no_act).await, Ok(None));
    }

    #[tokio::test]
    async fn my_stats_reads_the_current_act_from_the_db() {
        let dir = tempfile::tempdir().unwrap();
        let now = now_ms();
        let st = app(Some(ME), dir.path());
        st.set_acts_for_test(vec![
            act("prev", now - 90 * DAY, now - 30 * DAY),
            act("cur", now - 30 * DAY, now + 30 * DAY),
        ]);
        st.db_blocking(|repo| {
            comp_match(repo, "old", "prev", now - 40 * DAY)?;
            comp_match(repo, "new", "cur", now - DAY)
        })
        .unwrap();

        let stats = build_my_stats(&st).await.unwrap().expect("stats");
        let v: Value = serde_json::from_str(&serde_json::to_string(&stats).unwrap()).unwrap();
        let ids: Vec<&str> = v["last5"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["matchId"].as_str().unwrap())
            .collect();
        assert_eq!(ids, ["new"]);
        assert_eq!(v["rank"]["netRr"], serde_json::json!(18));
        assert_eq!(v["rrSeries"].as_array().unwrap().len(), 1);
        // Map tables not loaded: an unknown map is "", never the Riot path.
        assert_eq!(v["last5"][0]["mapId"], serde_json::json!(""));
        assert_eq!(v["maps"], serde_json::json!([]));
        assert!(v.get("byWeekday").is_some() && v.get("byDaypart").is_some());
    }

    #[test]
    fn local_offset_is_whole_minutes_within_a_day() {
        let offset = local_offset_ms(now_ms());
        assert_eq!(offset % 60_000, 0);
        assert!(offset.abs() < DAY);
    }

    #[test]
    fn uuid_check() {
        assert!(is_uuid("aaaaaaaa-0000-0000-0000-000000000001"));
        assert!(!is_uuid("../../etc"));
        assert!(!is_uuid(""));
    }

    #[test]
    fn match_details_args_must_be_uuids() {
        // Both ids reach a DB key and a URL path: the same shape check
        // `player_details` applies to its puuid.
        let id = "1a2b3c4d-0000-0000-0000-000000000010";
        assert_eq!(
            check_match_args("not-a-match", ME),
            Err("invalid match id".to_string())
        );
        assert_eq!(
            check_match_args("../../etc/passwd", ME),
            Err("invalid match id".to_string())
        );
        assert_eq!(
            check_match_args(id, "../x"),
            Err("invalid player id".to_string())
        );
        assert_eq!(
            check_match_args(id, ""),
            Err("invalid player id".to_string())
        );
        assert_eq!(check_match_args(id, ME), Ok(()));
        // The match id is checked first.
        assert_eq!(
            check_match_args("", "../x"),
            Err("invalid match id".to_string())
        );
    }

    #[test]
    fn the_dump_is_the_captures_the_errors_and_the_log() {
        use opd1_riot::raw_capture::ErrorDump;
        let files = dump_files(
            vec![("mmr".into(), "{}".into()), ("a/../b".into(), "x".into())],
            vec![ErrorDump {
                key: "mmr".into(),
                ext: "json",
                contents: r#"{"status":500}"#.into(),
            }],
            "line one\nline two".into(),
        );
        let names: Vec<&str> = files.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(
            names,
            ["mmr.json", "a____b.json", "mmr.error.json", "log.txt"]
        );
        assert_eq!(files.last().unwrap().1, "line one\nline two");
        // Nothing captured at all: the log alone.
        let files = dump_files(Vec::new(), Vec::new(), String::new());
        assert_eq!(files, [("log.txt".to_string(), String::new())]);
    }

    #[test]
    fn writing_the_dump_counts_the_log_and_works_with_no_captures() {
        let dir = tempfile::tempdir().unwrap();
        let debug = dir.path().join("debug");
        let files = dump_files(Vec::new(), Vec::new(), "the log".into());
        assert_eq!(write_dump(&debug, &files).unwrap(), 1);
        assert_eq!(
            std::fs::read_to_string(debug.join("log.txt")).unwrap(),
            "the log"
        );

        let files = dump_files(
            vec![("mmr".into(), "{}".into())],
            Vec::new(),
            "the log".into(),
        );
        assert_eq!(write_dump(&debug, &files).unwrap(), 2, "the log counts");
        assert!(debug.join("mmr.json").exists());
    }

    #[test]
    fn a_new_dump_removes_the_error_files_of_an_earlier_one() {
        // An error file has no time on it: one left from an earlier run would
        // blame an endpoint that works now.
        let dir = tempfile::tempdir().unwrap();
        let debug = dir.path().join("debug");
        std::fs::create_dir_all(&debug).unwrap();
        for stale in ["mmr.error.json", "comp_updates.error.txt"] {
            std::fs::write(debug.join(stale), "old").unwrap();
        }
        // Not error files: left alone (a 2xx capture is overwritten only when
        // written again; anything else in the directory is not ours to touch).
        std::fs::write(debug.join("notes.txt"), "keep").unwrap();
        std::fs::write(debug.join("errors.json"), "keep").unwrap();
        std::fs::write(debug.join("match_details.json"), "old capture").unwrap();
        std::fs::create_dir_all(debug.join("sub.error.json")).unwrap();
        // A file outside the debug directory is never looked at.
        std::fs::write(dir.path().join("outside.error.json"), "keep").unwrap();

        let files = dump_files(
            vec![("mmr".into(), "{}".into())],
            vec![opd1_riot::raw_capture::ErrorDump {
                key: "match_details".into(),
                ext: "txt",
                contents: "HTTP 502\nbad gateway".into(),
            }],
            "log".into(),
        );
        assert_eq!(write_dump(&debug, &files).unwrap(), 3);

        assert!(!debug.join("mmr.error.json").exists(), "stale: gone");
        assert!(
            !debug.join("comp_updates.error.txt").exists(),
            "stale: gone"
        );
        assert_eq!(
            std::fs::read_to_string(debug.join("match_details.error.txt")).unwrap(),
            "HTTP 502\nbad gateway",
            "the fresh error file is written"
        );
        for kept in ["notes.txt", "errors.json", "match_details.json"] {
            assert!(debug.join(kept).exists(), "{kept}");
        }
        assert!(
            debug.join("sub.error.json").is_dir(),
            "a directory is not a file of ours"
        );
        assert!(dir.path().join("outside.error.json").exists());

        // A dump with no errors at all leaves no error file behind.
        let files = dump_files(Vec::new(), Vec::new(), "log".into());
        write_dump(&debug, &files).unwrap();
        assert!(!debug.join("match_details.error.txt").exists());
    }

    // -- B2: the log header ------------------------------------------------

    fn known_header() -> LogHeader {
        LogHeader {
            app_version: "0.3.0".into(),
            windows: Some("10.0.22631".into()),
            stage: "pregame",
            settled: true,
            shards: Some(["eu".into(), "eu-1".into(), "eu".into()]),
            client: Some(("release-09.00-shipping-27-2517190".into(), "presence")),
            act: Some(HeaderAct {
                id: "3f61c77f-4d1b-4c28-9b0b-8f5d5a1c9e21".into(),
                label: "V26 A4".into(),
                source: "riot",
            }),
        }
    }

    #[test]
    fn the_header_with_every_part_known() {
        let line = format_header(&known_header());
        assert_eq!(
            line,
            "OPD1 TRCKER 0.3.0 · Windows 10.0.22631 · stage pregame · settled true · \
             shard eu/eu-1/eu · client release-09.00-shipping-27-2517190 (presence) · \
             act 3f61c77f V26 A4 (riot)"
        );
        println!("sample header: {line}");
    }

    #[test]
    fn every_unknown_part_of_the_header_prints_a_dash() {
        let line = format_header(&LogHeader {
            app_version: "0.3.0".into(),
            windows: None,
            stage: "closed",
            settled: false,
            shards: None,
            client: None,
            act: None,
        });
        assert_eq!(
            line,
            "OPD1 TRCKER 0.3.0 · Windows — · stage closed · settled false · \
             shard — · client — · act —"
        );
        // An act valorant-api and Riot both left unnamed, a blank version.
        let line = format_header(&LogHeader {
            windows: Some("  ".into()),
            client: Some((String::new(), "fallback")),
            act: Some(HeaderAct {
                id: "x".into(),
                label: "—".into(),
                source: "valorant-api",
            }),
            ..known_header()
        });
        assert!(line.contains("Windows — ·"), "{line}");
        assert!(line.contains("client — (fallback)"), "{line}");
        assert!(line.ends_with("act x — (valorant-api)"), "{line}");
    }

    #[test]
    fn a_long_act_label_or_version_cannot_break_the_one_line() {
        let line = format_header(&LogHeader {
            client: Some((format!("release-{}\n\tx", "9".repeat(300)), "log")),
            act: Some(HeaderAct {
                id: "3f61c77f-4d1b-4c28-9b0b-8f5d5a1c9e21".into(),
                label: format!(
                    "Riot named this act   {}\nsecond line",
                    "very long ".repeat(50)
                ),
                source: "riot",
            }),
            ..known_header()
        });
        assert!(!line.contains('\n') && !line.contains('\t'), "one line");
        assert!(
            line.chars().count() < 300,
            "{} chars: {line}",
            line.chars().count()
        );
        // The label is cut to 40 characters with the ellipsis, the id to 8,
        // and the source stays whole after it.
        assert!(
            line.contains(" act 3f61c77f Riot named this act very long very long… (riot)"),
            "{line}"
        );
        assert!(
            line.contains("(log) · act"),
            "the client part is whole: {line}"
        );
    }

    #[test]
    fn the_stage_is_spelled_as_on_the_wire() {
        for stage in [Stage::Closed, Stage::Menus, Stage::Pregame, Stage::Ingame] {
            assert_eq!(
                serde_json::to_value(stage).unwrap(),
                Value::String(stage_wire(stage).into())
            );
        }
        assert_eq!(stage_wire(Stage::Ingame), "ingame");
    }

    #[test]
    fn windows_build_is_known_only_on_windows() {
        let v = windows_build();
        if cfg!(windows) {
            let v = v.expect("Windows reports its version");
            let parts: Vec<&str> = v.split('.').collect();
            assert_eq!(parts.len(), 3, "{v}");
            assert!(parts.iter().all(|p| p.parse::<u32>().is_ok()), "{v}");
        } else {
            assert_eq!(v, None);
        }
    }

    #[test]
    fn log_text_is_the_header_line_then_the_ring() {
        let dir = tempfile::tempdir().unwrap();
        crate::logbuf::init();
        let st = app(Some(ME), dir.path());
        // No tables, no session: only the app and the stage are known.
        let text = log_text(&st);
        let (first, _rest) = text.split_once('\n').expect("a header line, then the ring");
        assert_eq!(first, format_header(&log_header(&st)));
        assert!(
            first.starts_with(&format!(
                "OPD1 TRCKER {} · Windows ",
                env!("CARGO_PKG_VERSION")
            )),
            "{first}"
        );
        assert!(
            first.ends_with(" · stage closed · settled false · shard — · client — · act —"),
            "{first}"
        );

        // Tables loaded: the act, from valorant-api's dates.
        let now = now_ms();
        st.set_acts_for_test(vec![Act {
            id: "3f61c77f-4d1b-4c28-9b0b-8f5d5a1c9e21".into(),
            name: "ACT IV".into(),
            episode_name: "V26".into(),
            start_ms: now - DAY,
            end_ms: now + DAY,
        }]);
        let text = log_text(&st);
        let first = text.lines().next().unwrap();
        assert!(
            first.ends_with(" · act 3f61c77f V26 A4 (valorant-api)"),
            "{first}"
        );
    }

    // -- B1: ui_log ---------------------------------------------------------

    /// The log lines that carry `marker`.
    fn ring_lines(marker: &str) -> Vec<String> {
        crate::logbuf::snapshot()
            .lines()
            .filter(|l| l.contains(marker))
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn the_ui_log_command_needs_no_app_state() {
        crate::logbuf::init();
        // No `AppState` anywhere: a UI error before `setup()` is still logged
        // (the limiter is the process-wide static, so only a few lines here).
        ui_log("error".into(), "uilog-test-cmd: before setup".into());
        ui_log("whatever".into(), "uilog-test-cmd: unknown level".into());
        let lines = ring_lines("uilog-test-cmd");
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert!(lines[0].contains(" ERROR ui: uilog-test-cmd: before setup"));
        assert!(lines[1].contains(" INFO  ui: uilog-test-cmd: unknown level"));
    }

    #[test]
    fn the_command_path_limits_to_20_a_minute_and_reports_the_drops_once() {
        crate::logbuf::init();
        let limiter = std::sync::Mutex::new(crate::logbuf::UiLogLimiter::new());
        let log = |at: i64, text: &str| crate::logbuf::log_ui_with(&limiter, at, "info", text);
        for i in 0..25 {
            log(1_000 + i, &format!("uilog-test-limit {i}"));
        }
        assert_eq!(ring_lines("uilog-test-limit").len(), 20, "five dropped");

        // A minute later the window is open: the drop count, then the line.
        log(70_000, "uilog-test-limit after");
        log(70_001, "uilog-test-limit again");
        let lines = ring_lines("uilog-test-limit");
        assert_eq!(lines.len(), 22, "{lines:?}");
        let ring = crate::logbuf::snapshot();
        let notice = ring
            .find(" ui: ui_log: 5 lines dropped")
            .expect("the drop notice");
        let after = ring
            .find("uilog-test-limit after")
            .expect("the line itself");
        assert!(notice < after, "the notice comes first");
        assert_eq!(ring.matches("ui_log: 5 lines dropped").count(), 1, "once");
    }

    #[test]
    fn the_command_path_never_panics_on_empty_or_huge_input() {
        crate::logbuf::init();
        let limiter = std::sync::Mutex::new(crate::logbuf::UiLogLimiter::new());
        let huge = "é\u{1F600}\n".repeat(500_000);
        crate::logbuf::log_ui_with(&limiter, 0, "", "");
        crate::logbuf::log_ui_with(&limiter, 1, &huge, &huge);
        crate::logbuf::log_ui_with(&limiter, 2, "error", &huge);
        crate::logbuf::log_ui_with(&limiter, i64::MIN, "warn", "\0");
        crate::logbuf::log_ui_with(&limiter, i64::MAX, "warn", "x");
        // Whatever it took, the ring has no line longer than a UI line plus
        // its prefix, and none broken in two.
        for line in crate::logbuf::snapshot()
            .lines()
            .filter(|l| l.contains(" ui: "))
        {
            assert!(line.chars().count() < 600, "{} chars", line.chars().count());
        }
    }

    #[test]
    fn file_stem_is_filesystem_safe() {
        assert_eq!(file_stem("comp_updates"), "comp_updates");
        assert_eq!(file_stem("a/../b"), "a____b");
        assert_eq!(file_stem(""), "unnamed");
    }
}
