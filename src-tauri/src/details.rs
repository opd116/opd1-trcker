//! Player Details (`player_details` command): card + recent matches + top
//! teammates + loadout + encounter history + the hero's act K/D and HS%,
//! built from what's cached; the caller then syncs in the background and
//! emits `"details"` when that produced more data.

use std::collections::HashMap;
use std::sync::Arc;

use opd1_core::db::repo::{EncounterRow, TeammateRow};
use opd1_core::model::{
    display_tier, DisplayName, EncounterDetailRow, Field, HeroStats, Loadout, MatchResultKind,
    PlayerCard, PlayerDetails, TeammateSummary,
};
use opd1_core::names::{display_name, KnownName};

use crate::logbuf::short;
use crate::roster::{EquippedSkin, RawLoadout, RosterPlayer};
use crate::state::{card_clock, closed_self_slot, lock, now_ms, AppState, LiveModel, Slot};
use crate::store::{act_hero_stats, load_player_db, recent_matches};
use crate::sync::RECENT_COUNT;

/// CLAUDE.md: "Top 3 Teammates".
const TOP_TEAMMATES: usize = 3;

/// R18: a teammate who is incognito in the current live roster
/// (`hidden_now`: puuid -> the agent their card shows) is shown by the
/// same rules as their card — the name remembered from a finished match
/// the user played in (`remembered`, from `Repo::known_name`), else their
/// agent, else Hidden Player — never with the name in the details
/// player's own match history. Anyone else: the name from their most
/// recent shared row; a row without a full name (never resolved) is
/// hidden too. A remembered name's source map arrives as the stored Riot
/// path and is converted here with `map_uuid` (`KnownName::with_map_uuid`).
pub fn teammate_name(
    row: &TeammateRow,
    hidden_now: &HashMap<String, Option<String>>,
    remembered: &HashMap<String, KnownName>,
    map_uuid: &dyn Fn(&str) -> Option<String>,
) -> DisplayName {
    match hidden_now.get(&row.puuid) {
        Some(agent) => display_name(
            true,
            None,
            agent.as_deref(),
            remembered
                .get(&row.puuid)
                .cloned()
                .map(|known| known.with_map_uuid(map_uuid)),
        ),
        None => display_name(
            false,
            row.game_name.clone().zip(row.tag_line.clone()),
            None,
            None,
        ),
    }
}

// ---------------------------------------------------------------------
// loadout (R83, R90, R97)
// ---------------------------------------------------------------------

/// The equipped skins of `puuid` we can read (R83): another player's come
/// from the current match's loadouts only (core-game In Game, pre-game for
/// Agent Select allies), never from anywhere else. The user's are the
/// match's first (the freshest), else their own last read (R90: once per
/// connect; `live.self_loadout` is also the copy restored from `app_state`
/// at startup, so a Closed-stage build needs no `app_state` read of its own
/// and `skins_pending` can stay synchronous).
fn raw_loadout(live: &LiveModel, puuid: &str) -> Option<RawLoadout> {
    live.loadouts.get(puuid).cloned().or_else(|| {
        (live.self_puuid.as_deref() == Some(puuid))
            .then(|| live.self_loadout.clone())
            .flatten()
    })
}

/// The Details loadout with the R97 tri-state per weapon: a skin whose name
/// is in memory is the [`WeaponSkin`](opd1_core::model::WeaponSkin); a name
/// that isn't is [`Field::Loading`] (key absent) only while a name fetch for
/// `puuid` runs, else [`Field::Missing`] (`null`) — so a skeleton never
/// outlives the attempt; no skin equipped, or nothing readable, is `null`.
/// Memory only (`weapon_skin_cached`): nothing here touches disk or network.
fn loadout_view(st: &AppState, puuid: &str, raw: Option<&RawLoadout>) -> Loadout {
    let Some(raw) = raw else {
        return Loadout::default();
    };
    let resolving = st.skins_resolving(puuid);
    let weapon = |slot: &Option<EquippedSkin>| match slot {
        None => Field::Missing,
        Some(e) => match st.assets.weapon_skin_cached(&e.skin_id, &e.chroma_id) {
            Some(skin) => Field::Value(skin),
            None if resolving => Field::Loading,
            None => Field::Missing,
        },
    };
    Loadout {
        vandal: weapon(&raw.vandal),
        phantom: weapon(&raw.phantom),
        operator: weapon(&raw.operator),
        sheriff: weapon(&raw.sheriff),
    }
}

/// The equipped skins of `puuid` whose names aren't in memory yet: the
/// caller then resolves them (`resolve_skins`) whether or not a Riot sync
/// runs (R83).
fn unnamed_skins(st: &AppState, puuid: &str) -> Vec<EquippedSkin> {
    let Some(raw) = raw_loadout(&lock(&st.live), puuid) else {
        return Vec::new();
    };
    raw.slots()
        .into_iter()
        .flatten()
        .filter(|e| {
            st.assets
                .weapon_skin_cached(&e.skin_id, &e.chroma_id)
                .is_none()
        })
        .cloned()
        .collect()
}

/// `puuid` has an equipped skin (from [`raw_loadout`]) whose name isn't in
/// memory yet.
pub fn skins_pending(st: &AppState, puuid: &str) -> bool {
    !unnamed_skins(st, puuid).is_empty()
}

/// Fetches the names of `puuid`'s unnamed skins from valorant-api (R83):
/// up to four small JSON requests at once, each cached forever, independent
/// of the 120s Riot details sync. `true` when at least one name became
/// known. A failure is logged and leaves that skin unnamed; it is retried
/// on the next open of the Details, not before (R97).
pub async fn resolve_skins(st: &Arc<AppState>, puuid: &str) -> bool {
    let mut set = tokio::task::JoinSet::new();
    for skin in unnamed_skins(st, puuid) {
        let assets = st.assets.clone();
        set.spawn(async move {
            let named = assets.weapon_skin(&skin.skin_id, &skin.chroma_id).await;
            (skin, named)
        });
    }
    let mut learned = false;
    while let Some(joined) = set.join_next().await {
        match joined {
            Ok((_, Ok(_))) => learned = true,
            Ok((skin, Err(e))) => log::debug!(
                "skin {} for {} not resolved: {e}",
                short(&skin.skin_id),
                short(puuid)
            ),
            Err(e) => log::warn!("skin task failed: {e}"),
        }
    }
    learned
}

/// One shared match as Player Details' Encounters row, from the user's
/// side: the result and score are the user's, the tiers are each side's
/// rank during the match (below 3 shows no rank). `lookup` turns the stored
/// Riot map path into a valorant-api uuid (`""` when unknown).
fn encounter_row(e: EncounterRow, lookup: &dyn Fn(&str) -> Option<String>) -> EncounterDetailRow {
    EncounterDetailRow {
        map_id: lookup(&e.map_id).unwrap_or_default(),
        match_id: e.match_id,
        start_ms: e.start_ms,
        with: e.with,
        result: match e.self_won {
            Some(true) => MatchResultKind::Win,
            Some(false) => MatchResultKind::Loss,
            None => MatchResultKind::Draw,
        },
        rounds_won: e.rounds_won,
        rounds_lost: e.rounds_lost,
        duration_ms: e.game_length_ms,
        them_agent_id: e.other_agent_id,
        them_tier: display_tier(e.other_tier),
        self_agent_id: e.self_agent_id,
        self_tier: display_tier(e.self_tier),
    }
}

/// A card for a player that isn't on screen: identity from the `players`
/// row (as the Closed stage does for the user), data from the DB. A player
/// whose card showed them hidden when last on screen (`seen_hidden`: the
/// Details was opened in Agent Select, then a dodge or the match end took
/// them off screen) keeps that card's rule — Hidden Player, the agent, or
/// the name remembered from the user's own finished match — never the
/// `players` row name, which name-service gave at an earlier visible
/// sighting.
fn offscreen_card(st: &AppState, puuid: &str, db: crate::store::PlayerDb) -> Option<PlayerCard> {
    let mut slot: Slot = closed_self_slot(puuid, db)?;
    let live = lock(&st.live);
    let is_self = live.self_puuid.as_deref() == Some(puuid);
    let hidden = (!is_self).then(|| live.seen_hidden.get(puuid)).flatten();
    slot.roster = RosterPlayer {
        is_self_party: is_self,
        incognito: hidden.is_some(),
        character_id: hidden.cloned().flatten().unwrap_or_default(),
        selection_state: match hidden {
            Some(Some(_)) => "locked".into(),
            _ => String::new(),
        },
        ..slot.roster
    };
    let acts = st.acts();
    let now = card_clock(now_ms());
    let act_id = crate::roster::current_act(&acts, now)
        .map(|a| a.id.clone())
        .unwrap_or_default();
    let mut card = live.card_for(&slot, &acts, &act_id, now, None, &|p| st.map_uuid(p));
    // Nothing is being fetched for it right now: unknown means `—`.
    crate::state::settle_card(&mut card, true, true);
    Some(card)
}

/// Rebuilds `puuid`'s details and emits them (`"details"`) when they are
/// the open Player Details and changed since the UI was last given them
/// (`AppState::emit_details_if_changed`). A no-op for anyone else, so the
/// worker can call it after every player it syncs.
pub async fn refresh(st: &Arc<AppState>, puuid: &str) {
    if !st.details_open_for(puuid) {
        return;
    }
    match build(st, puuid).await {
        Ok(fresh) => {
            if st.emit_details_if_changed(fresh) {
                log::debug!("details for {} updated", short(puuid));
            }
        }
        Err(e) => log::warn!("details rebuild failed: {e}"),
    }
}

/// Everything cached for `puuid`'s details view.
pub async fn build(st: &Arc<AppState>, puuid: &str) -> Result<PlayerDetails, String> {
    let (on_screen, hidden_now, raw_loadout, self_puuid) = {
        let live = lock(&st.live);
        (
            live.current
                .players
                .iter()
                .find(|c| c.puuid == puuid)
                .cloned(),
            live.hidden_players(),
            raw_loadout(&live, puuid),
            live.self_puuid.clone(),
        )
    };
    let season_start = st.current_act().map(|a| a.start_ms);

    let p = puuid.to_string();
    let assets = st.assets.clone();
    let need_card = on_screen.is_none();
    let hidden_ids: Vec<String> = hidden_now.keys().cloned().collect();
    let (recent, hero, teammates, remembered, encounters, db) = st
        .db(move |repo| {
            let lookup = |path: &str| {
                crate::roster::map_uuid(path, |x| assets.map_uuid_for_path(x).ok().flatten())
            };
            let recent = recent_matches(repo, &p, RECENT_COUNT, &lookup)?;
            // R116: the hero's K/D and HS% — the act's recent matches; with
            // no current act known, no match belongs to it.
            let (teammates, hero) = match season_start {
                Some(start) => (
                    repo.top_teammates(&p, start, TOP_TEAMMATES)?,
                    act_hero_stats(repo, &p, start)?,
                ),
                None => (Vec::new(), HeroStats::default()),
            };
            // R18: names the user learned in their own matches, only for
            // the teammates hidden on screen right now.
            let mut remembered = HashMap::new();
            if let Some(me) = self_puuid.as_deref() {
                for t in teammates.iter().filter(|t| hidden_ids.contains(&t.puuid)) {
                    if let Some(name) = repo.known_name(me, &t.puuid)? {
                        remembered.insert(t.puuid.clone(), name);
                    }
                }
            }
            // The user's own details have no encounter list (CLAUDE.md:
            // the user gets no encounter badge; "with yourself" is noise).
            let encounters = match self_puuid.as_deref() {
                Some(me) if me != p => repo
                    .encounters(me, &p)?
                    .matches
                    .into_iter()
                    .map(|e| encounter_row(e, &lookup))
                    .collect(),
                _ => Vec::new(),
            };
            let db = if need_card {
                Some(load_player_db(repo, self_puuid.as_deref(), &p)?)
            } else {
                None
            };
            Ok((recent, hero, teammates, remembered, encounters, db))
        })
        .await
        .ok_or_else(|| "database unavailable".to_string())?;

    let card = match on_screen {
        Some(card) => card,
        None => db
            .and_then(|db| offscreen_card(st, puuid, db))
            .ok_or_else(|| "unknown player".to_string())?,
    };

    Ok(PlayerDetails {
        card,
        recent,
        top_teammates: teammates
            .iter()
            .map(|t| TeammateSummary {
                puuid: t.puuid.clone(),
                name: teammate_name(t, &hidden_now, &remembered, &|p| st.map_uuid(p)),
                count: t.count,
            })
            .collect(),
        loadout: loadout_view(st, puuid, raw_loadout.as_ref()),
        encounters,
        hero_kd: hero.kd,
        hero_hs: hero.hs_pct,
        hero_matches: hero.matches,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(puuid: &str, name: Option<&str>) -> TeammateRow {
        TeammateRow {
            puuid: puuid.into(),
            game_name: name.map(str::to_string),
            tag_line: name.map(|_| "TAG".to_string()),
            count: 3,
        }
    }

    fn none() -> HashMap<String, KnownName> {
        HashMap::new()
    }

    fn known(name: &str, tag: &str, map_id: Option<&str>, start_ms: Option<i64>) -> KnownName {
        KnownName {
            name: name.into(),
            tag: tag.into(),
            map_id: map_id.map(str::to_string),
            start_ms,
        }
    }

    /// No map is known to the lookup: every converted source loses its map.
    fn no_maps(_: &str) -> Option<String> {
        None
    }

    #[test]
    fn teammate_hidden_in_the_live_roster_is_never_named() {
        // The name in someone else's match history is never used for a
        // player who is incognito on screen right now (R18).
        let before_pick: HashMap<String, Option<String>> = [("duo".to_string(), None)].into();
        assert_eq!(
            teammate_name(
                &row("duo", Some("RealName")),
                &before_pick,
                &none(),
                &no_maps
            ),
            DisplayName::HiddenPlayer
        );
        let after_pick: HashMap<String, Option<String>> =
            [("duo".to_string(), Some("agent-jett".to_string()))].into();
        assert_eq!(
            teammate_name(
                &row("duo", Some("RealName")),
                &after_pick,
                &none(),
                &no_maps
            ),
            DisplayName::HiddenAgent {
                agent_id: "agent-jett".into()
            }
        );
    }

    #[test]
    fn hidden_teammate_shows_the_name_remembered_from_the_users_own_match() {
        let hidden: HashMap<String, Option<String>> =
            [("duo".to_string(), Some("agent-jett".to_string()))].into();
        let remembered: HashMap<String, KnownName> =
            [("duo".to_string(), known("Seen", "AFTER", None, None))].into();
        assert_eq!(
            teammate_name(
                &row("duo", Some("RealName")),
                &hidden,
                &remembered,
                &no_maps
            ),
            DisplayName::Remembered {
                name: "Seen".into(),
                tag: "AFTER".into(),
                from_map_id: None,
                from_start_ms: None,
            }
        );
        // A remembered name only matters for a hidden teammate.
        assert_eq!(
            teammate_name(
                &row("duo", Some("Name")),
                &HashMap::new(),
                &remembered,
                &no_maps
            ),
            DisplayName::Visible {
                name: "Name".into(),
                tag: "TAG".into()
            }
        );
    }

    #[test]
    fn teammate_name_maps_remembered_source() {
        let hidden: HashMap<String, Option<String>> = [("duo".to_string(), None)].into();
        // The DB hands over the stored Riot path (R84).
        let remembered: HashMap<String, KnownName> = [(
            "duo".to_string(),
            known("Seen", "AFTER", Some("/Game/Maps/Bonsai/Bonsai"), Some(99)),
        )]
        .into();
        let name_with = |lookup: &dyn Fn(&str) -> Option<String>| {
            teammate_name(&row("duo", Some("RealName")), &hidden, &remembered, lookup)
        };

        let split = |p: &str| (p == "/Game/Maps/Bonsai/Bonsai").then(|| "uuid-split".to_string());
        assert_eq!(
            name_with(&split),
            DisplayName::Remembered {
                name: "Seen".into(),
                tag: "AFTER".into(),
                from_map_id: Some("uuid-split".into()),
                from_start_ms: Some(99),
            }
        );
        // A map the tables don't know: no source map, never the path.
        assert_eq!(
            name_with(&no_maps),
            DisplayName::Remembered {
                name: "Seen".into(),
                tag: "AFTER".into(),
                from_map_id: None,
                from_start_ms: Some(99),
            }
        );
    }

    #[test]
    fn visible_teammate_keeps_the_stored_name() {
        assert_eq!(
            teammate_name(&row("x", Some("Name")), &HashMap::new(), &none(), &no_maps),
            DisplayName::Visible {
                name: "Name".into(),
                tag: "TAG".into()
            }
        );
    }

    #[test]
    fn teammate_without_a_stored_name_is_hidden() {
        assert_eq!(
            teammate_name(&row("x", None), &HashMap::new(), &none(), &no_maps),
            DisplayName::HiddenPlayer
        );
        let mut empty = row("x", Some(""));
        empty.tag_line = Some(String::new());
        assert_eq!(
            teammate_name(&empty, &HashMap::new(), &none(), &no_maps),
            DisplayName::HiddenPlayer
        );
    }
}
