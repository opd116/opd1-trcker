//! Saved players, private notes and dismissed smurf flags (R189).
//!
//! Stored in migration 4's tables; the SQL and the rules (the empty note,
//! the 2000-character cap, a saved player's hidden-name rules) are in
//! `opd1_core::db::marks`. Every write is followed by the fresh
//! [`PlayerMarks`] as a `"marks"` event, so every open screen (the live
//! cards' bookmark, the Saved screen, the profile drawer) follows at once.

use std::collections::HashMap;

use opd1_core::db::marks::saved_player;
use opd1_core::wire::{PlayerMarks, SavedPlayer};

use super::AppStateRef;
use crate::commands::is_uuid;
use crate::state::{lock, now_ms, AppState};

/// A puuid from the UI ends up as a DB key: the same uuid check
/// `player_details` applies.
fn check_puuid(puuid: &str) -> Result<(), String> {
    if is_uuid(puuid) {
        Ok(())
    } else {
        Err("invalid player id".into())
    }
}

/// Every mark, read fresh.
pub async fn marks_now(st: &AppState) -> Result<PlayerMarks, String> {
    st.db(|repo| repo.player_marks())
        .await
        .ok_or_else(|| "marks unavailable".to_string())
}

/// Runs a mark write, then emits the fresh marks (`"marks"`).
async fn write_then_emit<F>(st: &AppState, write: F) -> Result<(), String>
where
    F: FnOnce(&opd1_core::db::repo::Repo) -> rusqlite::Result<()> + Send + 'static,
{
    let marks = st
        .db(move |repo| {
            write(repo)?;
            repo.player_marks()
        })
        .await
        .ok_or_else(|| "mark not stored".to_string())?;
    st.emit_marks(&marks);
    Ok(())
}

pub async fn set_saved_for(st: &AppState, puuid: String, saved: bool) -> Result<(), String> {
    check_puuid(&puuid)?;
    let now = now_ms();
    write_then_emit(st, move |repo| repo.set_saved(&puuid, saved, now)).await
}

pub async fn set_note_for(st: &AppState, puuid: String, note: String) -> Result<(), String> {
    check_puuid(&puuid)?;
    let now = now_ms();
    write_then_emit(st, move |repo| repo.set_note(&puuid, &note, now)).await
}

pub async fn dismiss_smurf_for(st: &AppState, puuid: String) -> Result<(), String> {
    check_puuid(&puuid)?;
    let now = now_ms();
    write_then_emit(st, move |repo| repo.dismiss_smurf(&puuid, now)).await
}

/// The Saved screen's rows (see `Repo::saved_rows` for what each field
/// reads). A saved player who is incognito on the live screen right now
/// follows the live rules (R18/R145): the remembered name, else their agent,
/// else Hidden Player — the same `hidden_players()` the Player Details'
/// Top 3 reads. Nothing is requested from Riot.
pub async fn saved_players_for(st: &AppState) -> Result<Vec<SavedPlayer>, String> {
    let (self_puuid, hidden_now): (Option<String>, HashMap<String, Option<String>>) = {
        let live = lock(&st.live);
        (live.self_puuid.clone(), live.hidden_players())
    };
    let rows = st
        .db(move |repo| repo.saved_rows(self_puuid.as_deref()))
        .await
        .ok_or_else(|| "saved players unavailable".to_string())?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let hidden = hidden_now.get(&row.puuid).map(|agent| agent.as_deref());
            saved_player(row, hidden, &|p| st.map_uuid(p))
        })
        .collect())
}

/// Every mark; later changes arrive as `"marks"`.
#[tauri::command]
pub async fn player_marks(state: AppStateRef<'_>) -> Result<PlayerMarks, String> {
    marks_now(&state).await
}

/// The Saved screen's rows, newest saved first.
#[tauri::command]
pub async fn saved_players(state: AppStateRef<'_>) -> Result<Vec<SavedPlayer>, String> {
    saved_players_for(&state).await
}

/// Saves or unsaves a player; emits `"marks"`.
#[tauri::command]
pub async fn set_saved(puuid: String, saved: bool, state: AppStateRef<'_>) -> Result<(), String> {
    set_saved_for(&state, puuid, saved).await
}

/// Sets (or, with `""` or only whitespace, clears) the private note on a
/// player; a note over 2000 characters is cut. Emits `"marks"`.
#[tauri::command]
pub async fn set_note(puuid: String, note: String, state: AppStateRef<'_>) -> Result<(), String> {
    set_note_for(&state, puuid, note).await
}

/// "Not a smurf, hide flag"; emits `"marks"`.
#[tauri::command]
pub async fn dismiss_smurf(puuid: String, state: AppStateRef<'_>) -> Result<(), String> {
    dismiss_smurf_for(&state, puuid).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::sync::{Arc, Mutex};

    use opd1_core::db::open_db;
    use opd1_core::db::repo::{Match, MatchPlayer, MatchTeam, Repo};
    use opd1_core::model::{DisplayName, LiveState, MatchDetailsView, PlayerDetails, Team};
    use opd1_riot::assets::AssetCache;

    use crate::roster::RosterPlayer;
    use crate::state::{EventSink, Slot};
    use crate::store::PlayerDb;

    const ME: &str = "aaaaaaaa-0000-0000-0000-000000000001";
    const SAVED: &str = "bbbbbbbb-0000-0000-0000-000000000002";
    const OTHER: &str = "cccccccc-0000-0000-0000-000000000003";

    #[derive(Default)]
    struct Marks(Mutex<Vec<PlayerMarks>>);

    struct Sink(Arc<Marks>);

    impl EventSink for Sink {
        fn live(&self, _: &LiveState) {}
        fn details(&self, _: &PlayerDetails) {}
        fn match_view(&self, _: &MatchDetailsView) {}
        fn marks(&self, marks: &PlayerMarks) {
            self.0 .0.lock().unwrap().push(marks.clone());
        }
    }

    fn app(dir: &Path) -> (Arc<AppState>, Arc<Marks>) {
        let rec = Arc::new(Marks::default());
        let repo = Repo::new(open_db(Path::new(":memory:")).unwrap());
        let assets = AssetCache::with_base(dir.join("assets"), "http://127.0.0.1:1".into());
        let st = Arc::new(AppState::new(
            Box::new(Sink(rec.clone())),
            repo,
            assets,
            dir.join("debug"),
            Some(ME.into()),
        ));
        (st, rec)
    }

    fn player(puuid: &str, team: &str, name: &str) -> MatchPlayer {
        MatchPlayer {
            puuid: puuid.into(),
            team_id: team.into(),
            party_id: None,
            agent_id: Some("agent-jett".into()),
            kills: 10,
            deaths: 10,
            assists: 2,
            score: 3000,
            rounds: Some(20),
            hs: 5,
            bs: 10,
            ls: 5,
            game_name: Some(name.into()),
            tag_line: Some("TAG".into()),
            competitive_tier: Some(15),
            damage: None,
        }
    }

    #[tokio::test]
    async fn every_write_emits_the_fresh_marks() {
        let dir = tempfile::tempdir().unwrap();
        let (st, rec) = app(dir.path());

        set_saved_for(&st, SAVED.into(), true).await.unwrap();
        set_note_for(&st, SAVED.into(), "entry every round".into())
            .await
            .unwrap();
        dismiss_smurf_for(&st, OTHER.into()).await.unwrap();
        let events = rec.0.lock().unwrap().clone();
        assert_eq!(events.len(), 3);
        let last = events.last().unwrap();
        assert_eq!(last.saved, vec![SAVED.to_string()]);
        assert_eq!(
            last.notes.get(SAVED).map(String::as_str),
            Some("entry every round")
        );
        assert_eq!(last.not_smurf, vec![OTHER.to_string()]);
        assert_eq!(&marks_now(&st).await.unwrap(), last);

        // A blank note deletes it; unsaving removes the player.
        set_note_for(&st, SAVED.into(), "   ".into()).await.unwrap();
        set_saved_for(&st, SAVED.into(), false).await.unwrap();
        let now = marks_now(&st).await.unwrap();
        assert!(now.saved.is_empty());
        assert!(now.notes.is_empty());
        assert_eq!(rec.0.lock().unwrap().len(), 5);
    }

    #[tokio::test]
    async fn a_puuid_that_is_not_a_uuid_is_refused_and_nothing_is_emitted() {
        let dir = tempfile::tempdir().unwrap();
        let (st, rec) = app(dir.path());
        assert!(set_saved_for(&st, "x' OR 1=1".into(), true).await.is_err());
        assert!(set_note_for(&st, "".into(), "n".into()).await.is_err());
        assert!(dismiss_smurf_for(&st, "nope".into()).await.is_err());
        assert!(rec.0.lock().unwrap().is_empty());
        assert_eq!(marks_now(&st).await.unwrap(), PlayerMarks::default());
    }

    #[tokio::test]
    async fn a_saved_player_hidden_on_the_live_screen_follows_the_live_rules() {
        let dir = tempfile::tempdir().unwrap();
        let (st, _) = app(dir.path());
        st.db_blocking(|repo| {
            repo.upsert_match(
                &Match {
                    match_id: "m1".into(),
                    map_id: "/Game/Maps/Ascent/Ascent".into(),
                    queue_id: "competitive".into(),
                    start_ms: 1_000,
                    winning_team: Some("Blue".into()),
                    is_ranked: true,
                    rounds_played: Some(20),
                    game_length_ms: None,
                },
                &[
                    MatchTeam {
                        team_id: "Blue".into(),
                        won: true,
                        rounds_won: Some(13),
                        rounds_won_h1: None,
                    },
                    MatchTeam {
                        team_id: "Red".into(),
                        won: false,
                        rounds_won: Some(7),
                        rounds_won_h1: None,
                    },
                ],
                &[player(ME, "Blue", "Me"), player(SAVED, "Blue", "Duo")],
            )?;
            repo.set_saved(SAVED, true, 5)
        })
        .unwrap();

        // Off the live screen: the known name, plainly.
        let rows = saved_players_for(&st).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].name,
            DisplayName::Visible {
                name: "Duo".into(),
                tag: "TAG".into()
            }
        );
        assert_eq!((rows[0].with.games, rows[0].with.wins), (1, 1));
        assert_eq!(rows[0].last_seen_ms, Some(1_000));
        assert_eq!(rows[0].tier, Some(15));

        // Incognito in the live match: the remembered style (R145).
        lock(&st.live).slots = vec![Slot {
            roster: RosterPlayer {
                puuid: SAVED.into(),
                team_id: "Blue".into(),
                team: Team::Ally,
                is_self_party: false,
                incognito: true,
                hide_account_level: false,
                account_level: 50,
                card_id: "card".into(),
                character_id: "agent-jett".into(),
                selection_state: "locked".into(),
                live_tier: None,
            },
            db: PlayerDb::default(),
        }];
        let rows = saved_players_for(&st).await.unwrap();
        assert!(
            matches!(&rows[0].name, DisplayName::Remembered { name, .. } if name == "Duo"),
            "{:?}",
            rows[0].name
        );
    }
}
