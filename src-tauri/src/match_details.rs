//! The `match_details` command's logic (spec §6.10): one stored match as
//! the Match Details scoreboard. Read from the DB; a match the DB doesn't
//! have is fetched from match-details once (stored forever, like every
//! match) and read again — see [`load`].

use std::collections::HashMap;
use std::sync::Arc;

use opd1_core::matchview::{build_match_view, needs_known_name, needs_seen_name, MatchViewInput};
use opd1_core::model::MatchDetailsView;
use opd1_core::names::KnownName;

use crate::state::{lock, AppState};
use crate::sync::fetch_missing_details;

/// `match_id` as a scoreboard seen from `focus_puuid`'s side. Reads the DB;
/// on a miss and with a Riot session it fetches the match's details once
/// and reads again. `Err("unknown match")` when it still isn't there
/// (no session, the fetch failed, or the id names no match).
///
/// The caller has already validated both ids (`commands::check_match_args`).
pub async fn load(
    st: &Arc<AppState>,
    match_id: &str,
    focus_puuid: &str,
) -> Result<MatchDetailsView, String> {
    if let Some(view) = read(st, match_id, focus_puuid).await? {
        return Ok(view);
    }
    if let Some(session) = st.session() {
        // A client that went away mid-fetch is just a miss here: the poll
        // loop notices it on its own next tick.
        let _ = fetch_missing_details(st, &session, vec![match_id.to_string()]).await;
        if let Some(view) = read(st, match_id, focus_puuid).await? {
            return Ok(view);
        }
    }
    Err("unknown match".into())
}

/// The view from what is stored, or `None` when the match isn't.
async fn read(
    st: &Arc<AppState>,
    match_id: &str,
    focus_puuid: &str,
) -> Result<Option<MatchDetailsView>, String> {
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
            Ok(Some((stored, remembered, seen)))
        })
        .await
        .ok_or_else(|| "database unavailable".to_string())?;
    let Some((stored, remembered, seen)) = loaded else {
        return Ok(None);
    };

    Ok(Some(build_match_view(MatchViewInput {
        stored: &stored,
        map_uuid: &|path| st.map_uuid(path),
        self_puuid: self_puuid.as_deref(),
        self_name,
        focus_puuid,
        hidden_now: &hidden_now,
        remembered: &remembered,
        seen: &seen,
    })))
}
