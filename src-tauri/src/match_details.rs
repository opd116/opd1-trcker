//! The `match_details` command's logic (spec §6.10): one stored match as
//! the Match Details scoreboard. Read from the DB; a match the DB doesn't
//! have is fetched from match-details once (stored forever, like every
//! match) and read again — see [`load`].

use std::collections::HashMap;
use std::sync::Arc;

use opd1_core::db::repo::NameAsk;
use opd1_core::matchview::{
    build_match_view, lacks_stored_name, needs_known_name, needs_seen_name, MatchViewInput,
};
use opd1_core::model::MatchDetailsView;
use opd1_core::names::KnownName;

use crate::state::{lock, AppState};
use crate::sync::fetch_missing_details;

/// `match_id` as a scoreboard seen from `focus_puuid`'s side. Reads the DB;
/// on a miss and with a Riot session it fetches the match's details once
/// and reads again. `Err("unknown match")` when it still isn't there
/// (no session, the fetch failed, or the id names no match).
///
/// R150: the view is also recorded as the open Match Details, and the rows
/// that have no name anywhere are queued for the names fill (a player shown
/// hidden included, R153; the fill sends between matches only): they show the
/// names when it answers, through a `"match"` event ([`refresh_open`]).
///
/// The caller has already validated both ids (`commands::check_match_args`).
pub async fn load(
    st: &Arc<AppState>,
    match_id: &str,
    focus_puuid: &str,
) -> Result<MatchDetailsView, String> {
    // R163: the ticket is taken before the build, so a Back (or a newer open)
    // that happens while it runs voids this open.
    let ticket = st.begin_open_match(match_id);
    let found = match read(st, match_id, focus_puuid).await? {
        Some(found) => Some(found),
        None => match st.session() {
            Some(session) => {
                // A client that went away mid-fetch is just a miss here: the
                // poll loop notices it on its own next tick.
                let _ = fetch_missing_details(st, &session, vec![match_id.to_string()]).await;
                read(st, match_id, focus_puuid).await?
            }
            None => None,
        },
    };
    let Some((view, asks)) = found else {
        return Err("unknown match".into());
    };
    st.set_open_match(ticket, match_id, focus_puuid, &view);
    st.queue_name_fill(asks);
    Ok(view)
}

/// Rebuilds the open Match Details, if any, from the DB and emits it
/// (`"match"`) when it changed (R150: names arrived from the names fill).
pub async fn refresh_open(st: &Arc<AppState>) {
    let Some((match_id, focus_puuid)) = st.open_match_args() else {
        return;
    };
    match read(st, &match_id, &focus_puuid).await {
        Ok(Some((view, _))) => {
            st.emit_match_if_changed(&match_id, &focus_puuid, view);
        }
        Ok(None) => {}
        Err(e) => log::warn!("match rebuild failed: {e}"),
    }
}

/// The view from what is stored — with the names fill's asks for its rows
/// (on demand: puuids never asked before) — or `None` when the match isn't.
async fn read(
    st: &Arc<AppState>,
    match_id: &str,
    focus_puuid: &str,
) -> Result<Option<(MatchDetailsView, Vec<NameAsk>)>, String> {
    let (self_puuid, self_name, hidden_now) = {
        let live = lock(&st.live);
        (
            live.self_puuid.clone(),
            live.self_name.clone(),
            live.hidden_players(),
        )
    };
    let m = match_id.to_string();
    let me = self_puuid.clone();
    let hidden = hidden_now.clone();
    let loaded = st
        .db(move |repo| {
            let Some(stored) = repo.match_rows(&m)? else {
                return Ok(None);
            };
            // Names the DB knows (R145). A row without a stored name that is
            // not hidden on the live screen shows the `players`-table name
            // as a plain name; every other row that isn't shown by its
            // stored name gets the best known name from any source
            // (`known_name`), for the remembered style. Nothing is ever
            // requested from Riot here.
            let me = me.as_deref().unwrap_or_default();
            let mut remembered: HashMap<String, KnownName> = HashMap::new();
            let mut seen: HashMap<String, (String, String)> = HashMap::new();
            for p in stored.players.iter().filter(|p| p.puuid != me) {
                if needs_seen_name(p, &hidden) && !seen.contains_key(&p.puuid) {
                    if let Some(name) = repo.seen_name(&p.puuid)? {
                        seen.insert(p.puuid.clone(), name);
                        continue;
                    }
                }
                if needs_known_name(p, &hidden) && !remembered.contains_key(&p.puuid) {
                    if let Some(name) = repo.known_name(me, &p.puuid)? {
                        remembered.insert(p.puuid.clone(), name);
                    }
                }
            }
            // R150: a row with no name in this match, none in the `players`
            // table and none remembered from any match is worth asking
            // name-service about — also one hidden on the live screen now
            // (R153): the fill only sends between matches, and the answer
            // still shows in the hidden style for that player.
            let asks: Vec<NameAsk> = stored
                .players
                .iter()
                .filter(|p| p.puuid != me)
                .filter(|p| lacks_stored_name(p))
                .filter(|p| !seen.contains_key(&p.puuid) && !remembered.contains_key(&p.puuid))
                .map(|p| NameAsk {
                    puuid: p.puuid.clone(),
                    match_start_ms: None,
                })
                .collect();
            Ok(Some((stored, remembered, seen, asks)))
        })
        .await
        .ok_or_else(|| "database unavailable".to_string())?;
    let Some((stored, remembered, seen, asks)) = loaded else {
        return Ok(None);
    };

    let view = build_match_view(MatchViewInput {
        stored: &stored,
        map_uuid: &|path| st.map_uuid(path),
        self_puuid: self_puuid.as_deref(),
        self_name,
        focus_puuid,
        hidden_now: &hidden_now,
        remembered: &remembered,
        seen: &seen,
    });
    Ok(Some((view, asks)))
}
