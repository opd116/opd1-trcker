//! Player Details (`player_details` command): card + recent matches + top
//! teammates + loadout + encounter history, built from what's cached; the
//! caller then syncs in the background and emits `"details"` when that
//! produced more data.

use std::collections::HashSet;
use std::sync::Arc;

use opd1_core::db::repo::TeammateRow;
use opd1_core::model::{
    DisplayName, EncounterDetailRow, PlayerCard, PlayerDetails, TeammateSummary,
};

use crate::roster::RosterPlayer;
use crate::state::{card_clock, closed_self_slot, lock, now_ms, AppState, Slot};
use crate::store::{load_player_db, recent_matches};
use crate::sync::RECENT_COUNT;

/// CLAUDE.md: "Top 3 Teammates".
const TOP_TEAMMATES: usize = 3;

/// R18: a teammate who is incognito in the current live roster is never
/// shown by name — even though the name is right there in someone else's
/// match history. Otherwise the name from their most recent shared row;
/// a row without a full name (never resolved) is hidden too. (Task 6
/// refines this with names remembered from the user's own matches.)
pub fn teammate_name(row: &TeammateRow, hidden_now: &HashSet<String>) -> DisplayName {
    if hidden_now.contains(&row.puuid) {
        return DisplayName::HiddenPlayer;
    }
    match (&row.game_name, &row.tag_line) {
        (Some(name), Some(tag)) if !name.is_empty() && !tag.is_empty() => DisplayName::Visible {
            name: name.clone(),
            tag: tag.clone(),
        },
        _ => DisplayName::HiddenPlayer,
    }
}

/// A card for a player that isn't on screen: identity from the `players`
/// row (as the Closed stage does for the user), data from the DB.
fn offscreen_card(st: &AppState, puuid: &str, db: crate::store::PlayerDb) -> Option<PlayerCard> {
    let mut slot: Slot = closed_self_slot(puuid, db)?;
    let live = lock(&st.live);
    let is_self = live.self_puuid.as_deref() == Some(puuid);
    slot.roster = RosterPlayer {
        is_self_party: is_self,
        ..slot.roster
    };
    let acts = st.acts();
    let now = card_clock(now_ms());
    let act_id = crate::roster::current_act(&acts, now)
        .map(|a| a.id.clone())
        .unwrap_or_default();
    let mut card = live.card_for(&slot, &acts, &act_id, now);
    // Nothing is being fetched for it right now: unknown means `—`.
    crate::state::settle_card(&mut card, true, true);
    Some(card)
}

/// Everything cached for `puuid`'s details view.
pub async fn build(st: &Arc<AppState>, puuid: &str) -> Result<PlayerDetails, String> {
    let (on_screen, hidden_now, loadout, self_puuid) = {
        let live = lock(&st.live);
        (
            live.current
                .players
                .iter()
                .find(|c| c.puuid == puuid)
                .cloned(),
            live.hidden_puuids(),
            live.loadouts.get(puuid).cloned().unwrap_or_default(),
            live.self_puuid.clone(),
        )
    };
    let season_start = st.current_act().map(|a| a.start_ms);

    let p = puuid.to_string();
    let assets = st.assets.clone();
    let need_card = on_screen.is_none();
    let (recent, teammates, encounters, db) = st
        .db(move |repo| {
            let lookup = |path: &str| {
                crate::roster::map_uuid(path, |x| assets.map_uuid_for_path(x).ok().flatten())
            };
            let recent = recent_matches(repo, &p, RECENT_COUNT, &lookup)?;
            let teammates = match season_start {
                Some(start) => repo.top_teammates(&p, start, TOP_TEAMMATES)?,
                None => Vec::new(),
            };
            // The user's own details have no encounter list (CLAUDE.md:
            // the user gets no encounter badge; "with yourself" is noise).
            let encounters = match self_puuid.as_deref() {
                Some(me) if me != p => repo
                    .encounters(me, &p)?
                    .matches
                    .into_iter()
                    .map(|e| EncounterDetailRow {
                        map_id: lookup(&e.map_id).unwrap_or_default(),
                        match_id: e.match_id,
                        start_ms: e.start_ms,
                        with: e.with,
                        self_won: e.self_won,
                    })
                    .collect(),
                _ => Vec::new(),
            };
            let db = if need_card {
                Some(load_player_db(repo, &p)?)
            } else {
                None
            };
            Ok((recent, teammates, encounters, db))
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
                name: teammate_name(t, &hidden_now),
                count: t.count,
            })
            .collect(),
        loadout,
        encounters,
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

    #[test]
    fn teammate_hidden_in_the_live_roster_is_never_named() {
        let hidden: HashSet<String> = ["duo".to_string()].into();
        assert_eq!(
            teammate_name(&row("duo", Some("RealName")), &hidden),
            DisplayName::HiddenPlayer
        );
    }

    #[test]
    fn visible_teammate_keeps_the_stored_name() {
        assert_eq!(
            teammate_name(&row("x", Some("Name")), &HashSet::new()),
            DisplayName::Visible {
                name: "Name".into(),
                tag: "TAG".into()
            }
        );
    }

    #[test]
    fn teammate_without_a_stored_name_is_hidden() {
        assert_eq!(
            teammate_name(&row("x", None), &HashSet::new()),
            DisplayName::HiddenPlayer
        );
        let mut empty = row("x", Some(""));
        empty.tag_line = Some(String::new());
        assert_eq!(
            teammate_name(&empty, &HashSet::new()),
            DisplayName::HiddenPlayer
        );
    }
}
