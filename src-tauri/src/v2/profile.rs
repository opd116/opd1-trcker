//! `profile_extras` (R189): the profile drawer's maps and "in your match"
//! chips, DB only (no request to Riot).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use opd1_core::db::repo::Repo;
use opd1_core::presence::Stage;
use opd1_core::wire::{LiveMates, ProfileExtras};

use super::AppStateRef;
use crate::state::{lock, now_ms, AppState};

/// How many maps the drawer lists (most played first).
pub const PROFILE_MAPS: usize = 5;

#[tauri::command]
pub async fn profile_extras(
    puuid: String,
    state: AppStateRef<'_>,
) -> Result<ProfileExtras, String> {
    build(state.inner(), puuid).await
}

/// The command's body (the test seam).
///
/// - `maps`: the player's current-act competitive record per map, most
///   played first, at most [`PROFILE_MAPS`] — the My Stats Maps table's own
///   builder (`store::load_my_stats`), run for this puuid. Empty while no
///   act is known.
/// - `live_mates`: for each of the player's recent competitive matches (the
///   same [`crate::sync::RECENT_COUNT`] newest that Player Details' `recent`
///   lists), the players of the current live roster (the user included,
///   unless the profile is the user's) who were in that stored match. Only
///   matches with at least one such player are listed; none while VALORANT is
///   closed (there is no live match).
pub async fn build(st: &Arc<AppState>, puuid: String) -> Result<ProfileExtras, String> {
    if !is_uuid(&puuid) {
        return Err("invalid puuid".into());
    }
    let act = st.current_act();
    let on_screen: HashSet<String> = {
        let live = lock(&st.live);
        if live.stage == Stage::Closed {
            HashSet::new()
        } else {
            live.slots
                .iter()
                .map(|s| s.roster.puuid.clone())
                .filter(|p| *p != puuid)
                .collect()
        }
    };
    let assets = st.assets.clone();
    let p = puuid.clone();
    let (maps, live_mates) = st
        .db(move |repo| {
            let lookup = |path: &str| {
                crate::roster::map_uuid(path, |x| assets.map_uuid_for_path(x).ok().flatten())
            };
            let maps = match act {
                Some(act) => {
                    let mut maps = crate::store::load_my_stats(
                        repo,
                        &p,
                        &act,
                        now_ms(),
                        &lookup,
                        &HashMap::new(),
                        &|_| 0,
                    )?
                    .maps;
                    maps.truncate(PROFILE_MAPS);
                    maps
                }
                None => Vec::new(),
            };
            Ok((maps, live_mates(repo, &p, &on_screen)?))
        })
        .await
        .ok_or_else(|| "database unavailable".to_string())?;
    Ok(ProfileExtras {
        puuid,
        maps,
        live_mates,
    })
}

/// See [`build`]: newest match first, puuids in the order the stored match
/// lists them (by puuid).
fn live_mates(
    repo: &Repo,
    puuid: &str,
    on_screen: &HashSet<String>,
) -> rusqlite::Result<Vec<LiveMates>> {
    if on_screen.is_empty() {
        return Ok(Vec::new());
    }
    let recent = repo.comp_matches(puuid, i64::MIN..i64::MAX, crate::sync::RECENT_COUNT)?;
    // One primary-key range read per match (`match_players(match_id, puuid)`).
    let mut stmt = repo
        .conn()
        .prepare_cached("SELECT puuid FROM match_players WHERE match_id = ?1 ORDER BY puuid")?;
    let mut out = Vec::new();
    for m in recent {
        let players = stmt.query_map([&m.match_id], |r| r.get::<_, String>(0))?;
        let mut puuids = Vec::new();
        for p in players {
            let p = p?;
            if on_screen.contains(&p) {
                puuids.push(p);
            }
        }
        if !puuids.is_empty() {
            out.push(LiveMates {
                match_id: m.match_id,
                puuids,
            });
        }
    }
    Ok(out)
}

/// A canonical 8-4-4-4-12 hex uuid (a puuid ends up in DB keys).
fn is_uuid(s: &str) -> bool {
    s.len() == 36
        && s.char_indices().all(|(i, c)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                c == '-'
            } else {
                c.is_ascii_hexdigit()
            }
        })
}

#[cfg(test)]
mod tests {
    use super::is_uuid;

    #[test]
    fn only_a_uuid_is_a_puuid() {
        assert!(is_uuid("aaaaaaaa-0000-0000-0000-000000000001"));
        assert!(!is_uuid("aaaaaaaa-0000-0000-0000-00000000000"));
        assert!(!is_uuid("aaaaaaaa_0000-0000-0000-000000000001"));
        assert!(!is_uuid("../../etc/passwd"));
        assert!(!is_uuid(""));
    }
}
