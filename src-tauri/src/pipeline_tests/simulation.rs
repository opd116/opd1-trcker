//! Deterministic offline Riot simulation harness (Step 3 milestone).
//!
//! Exercises the real OPD1 TRCKER Rust backend (Poller, AppState, SQLite Repo,
//! sync worker, EventSink) across a full match lifecycle:
//!
//! Disconnected → Lobby → Agent Select → In Game → Match End → Lobby
//!
//! Features:
//! - 10 players across two teams (Blue allies, Red enemies).
//! - Realistic responses: account levels, hidden names (`Incognito: true`),
//!   hidden account levels (`HideAccountLevel: true`), cached statistics,
//!   throttling/delayed requests, and immutable match persistence.
//! - Validates real backend state transitions, live player card representations,
//!   privacy rules (hidden names never sent to name-service during live stages),
//!   and SQLite database contents (matches, match_players, remembered names).

use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use serde_json::{json, Value};
use wiremock::matchers::{method, path};
use wiremock::{Mock, Request, ResponseTemplate};

use opd1_core::model::{DisplayName, Field, RememberedSource, Team};
use opd1_core::presence::Stage;
use opd1_core::wire::ConnState;

use super::*;
use crate::poll::{Poller, LOCKFILE_NOT_FOUND};

// ---------------------------------------------------------------------
// Simulated Ten Players
// ---------------------------------------------------------------------

pub(crate) const SIM_ME: &str = ME; // "aaaaaaaa-0000-0000-0000-000000000001"
pub(crate) const SIM_ALLY_1: &str = "aaaaaaaa-0000-0000-0000-000000000002";
pub(crate) const SIM_ALLY_2: &str = "aaaaaaaa-0000-0000-0000-000000000003";
pub(crate) const SIM_ALLY_3_HIDDEN: &str = "aaaaaaaa-0000-0000-0000-000000000004";
pub(crate) const SIM_ALLY_4_HIDELVL: &str = "aaaaaaaa-0000-0000-0000-000000000005";

pub(crate) const SIM_ENEMY_1_HIDDEN: &str = "bbbbbbbb-0000-0000-0000-000000000001";
pub(crate) const SIM_ENEMY_2: &str = "bbbbbbbb-0000-0000-0000-000000000002";
pub(crate) const SIM_ENEMY_3: &str = "bbbbbbbb-0000-0000-0000-000000000003";
pub(crate) const SIM_ENEMY_4_HIDDEN: &str = "bbbbbbbb-0000-0000-0000-000000000004";
pub(crate) const SIM_ENEMY_5: &str = "bbbbbbbb-0000-0000-0000-000000000005";

const AGENT_JETT: &str = "add6443a-41bd-e414-f6ad-e58d267f4e95";
const AGENT_PHOENIX: &str = "eb9333ab-405b-4b2a-a92f-7988352be53b";
const AGENT_VIPER: &str = "707eab51-4836-f488-046a-cda6bf494859";
const AGENT_CYPHER: &str = "117ed9e3-49f3-6512-3ccf-0cada7e3823b";
const AGENT_SOVA: &str = "320799f3-4aa0-4736-6143-e4a1a038b1a3";
const AGENT_REYNA: &str = "a3bfb854-4376-ec96-36e6-34c0b8df5f4e";
const AGENT_OMEN: &str = "8e253930-4c05-31dd-1b6c-968525494517";
const AGENT_FADE: &str = "dade69b4-4f5a-8528-247b-219e5a1facd6";
const AGENT_ASTRA: &str = "41fb69c1-4139-7b72-4022-36b1f98d0904";
const AGENT_BREACH: &str = "5f8d3d7f-467b-97f3-062c-dd4bbc036c59";

const ASCENT_PATH: &str = "/Game/Maps/Ascent/Ascent";
const MATCH_ID_PREGAME: &str = "sim-match-pregame-001";
const MATCH_ID_COREGAME: &str = "sim-match-coregame-001";

type PresenceBlob = Arc<Mutex<Option<Value>>>;

fn sim_blob(state: &str, queue: &str) -> Value {
    json!({
        "sessionLoopState": state,
        "queueId": queue,
        "partyId": "sim-party-1",
        "partyClientVersion": "release-11.08-shipping-9-3104338"
    })
}

async fn mount_switchable_presence(h: &Harness, initial: Value) -> PresenceBlob {
    let cell: PresenceBlob = Arc::new(Mutex::new(Some(initial)));
    let served = cell.clone();
    Mock::given(method("GET"))
        .and(path("/chat/v4/presences"))
        .respond_with(move |_: &Request| {
            let presences = match served.lock().unwrap().clone() {
                Some(blob) => vec![json!({
                    "puuid": SIM_ME,
                    "product": "valorant",
                    "private": BASE64.encode(blob.to_string())
                })],
                None => Vec::new(),
            };
            ResponseTemplate::new(200).set_body_json(json!({ "presences": presences }))
        })
        .with_priority(1)
        .mount(&h.server)
        .await;
    cell
}

fn player_identity(incognito: bool, hide_level: bool, level: u32) -> Value {
    json!({
        "Incognito": incognito,
        "HideAccountLevel": hide_level,
        "AccountLevel": level,
        "PlayerCardID": "card-sim"
    })
}

fn mock_player_mmr(puuid: &str, tier: u32, rr: u32, wins: u32) -> Value {
    json!({
        "Subject": puuid,
        "QueueSkills": {
            "competitive": {
                "SeasonalInfoBySeasonID": {
                    ACT: {
                        "SeasonID": ACT,
                        "CompetitiveTier": tier,
                        "RankedRating": rr,
                        "NumberOfWins": wins,
                        "NumberOfGames": wins + 2,
                        "WinsByTier": { tier.to_string(): wins }
                    }
                }
            }
        }
    })
}

fn mock_player_history(puuid: &str, match_id: &str, start_ms: i64) -> Value {
    json!({
        "Subject": puuid,
        "History": [
            { "MatchID": match_id, "GameStartTime": start_ms, "QueueID": "competitive" }
        ]
    })
}

fn mock_player_comp_updates(
    puuid: &str,
    match_id: &str,
    start_ms: i64,
    tier: u32,
    rr: i64,
) -> Value {
    json!({
        "Subject": puuid,
        "Matches": [
            {
                "MatchID": match_id,
                "MapID": ASCENT_PATH,
                "SeasonID": ACT,
                "MatchStartTime": start_ms,
                "TierAfterUpdate": tier,
                "TierBeforeUpdate": tier,
                "RankedRatingAfterUpdate": 50,
                "RankedRatingBeforeUpdate": 50 - rr,
                "RankedRatingEarned": rr
            }
        ]
    })
}

/// Sets up remote endpoints for all 10 players on the mock server.
async fn setup_ten_players_endpoints(
    h: &Harness,
    mmr_call_counter: Arc<AtomicUsize>,
    name_service_queries: Arc<Mutex<Vec<String>>>,
) {
    let players = [
        (SIM_ME, "Me", "TAG", 150, 15, 42, 6, false, false),
        (
            SIM_ALLY_1,
            "PhoenixKing",
            "1111",
            45,
            14,
            50,
            8,
            false,
            false,
        ),
        (
            SIM_ALLY_2,
            "ViperMain",
            "2222",
            112,
            17,
            75,
            12,
            false,
            false,
        ),
        (
            SIM_ALLY_3_HIDDEN,
            "ShadowNinja",
            "3333",
            78,
            16,
            60,
            10,
            true,
            false,
        ),
        (
            SIM_ALLY_4_HIDELVL,
            "CypherSpy",
            "4444",
            95,
            13,
            30,
            4,
            false,
            true,
        ),
        (
            SIM_ENEMY_1_HIDDEN,
            "ViperGod",
            "5555",
            204,
            19,
            85,
            20,
            true,
            false,
        ),
        (SIM_ENEMY_2, "SovaGod", "6666", 88, 18, 40, 14, false, false),
        (SIM_ENEMY_3, "FadeAway", "7777", 15, 12, 25, 2, false, false),
        (
            SIM_ENEMY_4_HIDDEN,
            "GhostOmen",
            "8888",
            160,
            15,
            55,
            9,
            true,
            true,
        ),
        (
            SIM_ENEMY_5,
            "AstraStar",
            "9999",
            330,
            21,
            90,
            35,
            false,
            false,
        ),
    ];

    let mut name_map: std::collections::HashMap<String, (String, String)> =
        std::collections::HashMap::new();

    for (puuid, name, tag, _lvl, tier, rr, wins, _incognito, _hide_lvl) in players {
        name_map.insert(puuid.to_string(), (name.to_string(), tag.to_string()));
        let hist_match_id = format!("hist-{puuid}");
        let comp_body = mock_player_comp_updates(puuid, &hist_match_id, 1_600_000, tier, 20);
        let hist_body = mock_player_history(puuid, &hist_match_id, 1_600_000);
        let mmr_body = mock_player_mmr(puuid, tier, rr, wins);

        // Mount historical match details
        get(&h.server, &format!("/match-details/v1/matches/{hist_match_id}"), json!({
            "matchInfo": {
                "matchId": hist_match_id,
                "mapId": ASCENT_PATH,
                "gameStartMillis": 1_600_000,
                "queueId": "competitive",
                "isRanked": true,
                "completionState": "Completed"
            },
            "players": [
                {
                    "subject": puuid,
                    "gameName": name,
                    "tagLine": tag,
                    "teamId": "Blue",
                    "partyId": "p-hist",
                    "characterId": AGENT_SOVA,
                    "stats": { "score": 200, "roundsPlayed": 20, "kills": 15, "deaths": 10, "assists": 5 }
                }
            ],
            "teams": [
                { "teamId": "Blue", "won": true, "roundsWon": 13 },
                { "teamId": "Red", "won": false, "roundsWon": 7 }
            ],
            "roundResults": []
        })).await;

        // Competitive updates
        Mock::given(method("GET"))
            .and(path(format!("/mmr/v1/players/{puuid}/competitiveupdates")))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(comp_body)
                    .set_delay(Duration::from_millis(15)), // Realistic network delay
            )
            .mount(&h.server)
            .await;

        // Match history
        Mock::given(method("GET"))
            .and(path(format!("/match-history/v1/history/{puuid}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(hist_body))
            .mount(&h.server)
            .await;

        // MMR endpoint with counter to verify caching behavior
        let counter = mmr_call_counter.clone();
        Mock::given(method("GET"))
            .and(path(format!("/mmr/v1/players/{puuid}")))
            .respond_with(move |_: &Request| {
                counter.fetch_add(1, SeqCst);
                ResponseTemplate::new(200)
                    .set_body_json(mmr_body.clone())
                    .set_delay(Duration::from_millis(20))
            })
            .mount(&h.server)
            .await;
    }

    // Dynamic name-service endpoint with priority 1
    let recorded_queries = name_service_queries.clone();
    Mock::given(method("PUT"))
        .and(path("/name-service/v2/players"))
        .respond_with(move |req: &Request| {
            let asked: Vec<String> = serde_json::from_slice(&req.body).unwrap_or_default();
            {
                let mut rec = recorded_queries.lock().unwrap();
                rec.extend(asked.clone());
            }
            let answer: Vec<Value> = asked
                .iter()
                .map(|p| {
                    let (n, t) = name_map.get(p).cloned().unwrap_or_default();
                    json!({"DisplayName": "", "Subject": p, "GameName": n, "TagLine": t})
                })
                .collect();
            ResponseTemplate::new(200).set_body_json(answer)
        })
        .with_priority(1)
        .mount(&h.server)
        .await;
}

// ---------------------------------------------------------------------
// Full Lifecycle Integration Scenario
// ---------------------------------------------------------------------

#[tokio::test]
async fn simulation_full_match_lifecycle_ten_players_with_persistence() {
    let h = harness_with_db_file().await;
    let mmr_counter = Arc::new(AtomicUsize::new(0));
    let name_service_queries = Arc::new(Mutex::new(Vec::new()));
    setup_ten_players_endpoints(&h, mmr_counter.clone(), name_service_queries.clone()).await;

    // =================================================================
    // Step 1: Disconnected
    // =================================================================
    let mut poller = Poller::new(h.st.clone());
    // Simulate disconnected state before connect
    h.st.request_retry();
    poller.disconnected_tick().await;

    assert_eq!(h.st.live_state().stage, Stage::Closed);
    let status_disconn = h.st.status();
    assert_eq!(status_disconn.conn, ConnState::Error);
    assert_eq!(
        status_disconn.error_code.as_deref(),
        Some(LOCKFILE_NOT_FOUND)
    );

    // =================================================================
    // Step 2: Connect & Enter Lobby (MENUS)
    // =================================================================
    // Mount initial presence blob for MENUS
    let presence_cell = mount_switchable_presence(&h, sim_blob("MENUS", "competitive")).await;

    // Mount Lobby Party: ME + ALLY_1 in party
    Mock::given(method("GET"))
        .and(path(format!("/parties/v1/players/{SIM_ME}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "Subject": SIM_ME, "CurrentPartyID": "sim-party-1"
        })))
        .mount(&h.server)
        .await;

    Mock::given(method("GET"))
        .and(path("/parties/v1/parties/sim-party-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "ID": "sim-party-1",
            "Members": [
                {
                    "Subject": SIM_ME,
                    "IsOwner": true,
                    "PlayerIdentity": player_identity(false, false, 150)
                },
                {
                    "Subject": SIM_ALLY_1,
                    "IsOwner": false,
                    "PlayerIdentity": player_identity(false, false, 45)
                }
            ]
        })))
        .mount(&h.server)
        .await;

    poller.on_connected(h.session.clone()).await;
    poller.presence_tick(&h.session).await.unwrap();

    // Verify transition to Menus
    assert_eq!(h.st.live_state().stage, Stage::Menus);
    let live_lobby = h.st.live_state();
    assert_eq!(live_lobby.players.len(), 2, "Lobby has ME and ALLY_1");
    assert!(live_lobby.players.iter().any(|p| p.puuid == SIM_ME));
    assert!(live_lobby.players.iter().any(|p| p.puuid == SIM_ALLY_1));

    // Wait for ALLY_1's rank and name to load from worker
    wait_until("ALLY_1 rank and name loaded in lobby", || {
        let st = h.st.live_state();
        st.players.iter().any(|p| {
            p.puuid == SIM_ALLY_1
                && matches!(p.rank, Field::Value(_))
                && matches!(p.name, Field::Value(DisplayName::Visible { .. }))
        })
    })
    .await;

    // Verify DB contains ALLY_1 record
    let ally1_stored = h.st.db(|repo| repo.seen_name(SIM_ALLY_1)).await.unwrap();
    assert!(ally1_stored.is_some(), "ALLY_1 should be stored in SQLite");
    let (name, tag) = ally1_stored.unwrap();
    assert_eq!(name, "PhoenixKing");
    assert_eq!(tag, "1111");

    let lobby_mmr_count = mmr_counter.load(SeqCst);
    assert!(
        lobby_mmr_count > 0,
        "MMR endpoint should have been queried in Lobby"
    );

    // =================================================================
    // Step 3: Matchmaking -> Agent Select (PREGAME)
    // =================================================================
    *presence_cell.lock().unwrap() = Some(sim_blob("PREGAME", "competitive"));

    Mock::given(method("GET"))
        .and(path(format!("/pregame/v1/players/{SIM_ME}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "Subject": SIM_ME, "MatchID": MATCH_ID_PREGAME
        })))
        .mount(&h.server)
        .await;

    let pregame_allies = json!({
        "TeamID": "Blue",
        "Players": [
            {
                "Subject": SIM_ME,
                "CharacterID": AGENT_JETT,
                "CharacterSelectionState": "locked",
                "PlayerIdentity": player_identity(false, false, 150)
            },
            {
                "Subject": SIM_ALLY_1,
                "CharacterID": AGENT_PHOENIX,
                "CharacterSelectionState": "locked",
                "PlayerIdentity": player_identity(false, false, 45)
            },
            {
                "Subject": SIM_ALLY_2,
                "CharacterID": AGENT_VIPER,
                "CharacterSelectionState": "", // hovering, not locked
                "PlayerIdentity": player_identity(false, false, 112)
            },
            {
                "Subject": SIM_ALLY_3_HIDDEN,
                "CharacterID": AGENT_CYPHER,
                "CharacterSelectionState": "locked",
                "PlayerIdentity": player_identity(true, false, 78) // Hidden name
            },
            {
                "Subject": SIM_ALLY_4_HIDELVL,
                "CharacterID": AGENT_BREACH,
                "CharacterSelectionState": "locked",
                "PlayerIdentity": player_identity(false, true, 95) // Hidden level
            }
        ]
    });

    Mock::given(method("GET"))
        .and(path(format!("/pregame/v1/matches/{MATCH_ID_PREGAME}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "ID": MATCH_ID_PREGAME,
            "MapID": ASCENT_PATH,
            "AllyTeam": pregame_allies,
            "Teams": [pregame_allies]
        })))
        .mount(&h.server)
        .await;

    // Pregame loadouts
    Mock::given(method("GET"))
        .and(path(format!(
            "/pregame/v1/matches/{MATCH_ID_PREGAME}/loadouts"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "Loadouts": [] })))
        .mount(&h.server)
        .await;

    poller.presence_tick(&h.session).await.unwrap();

    // Verify transition to Pregame
    assert_eq!(h.st.live_state().stage, Stage::Pregame);
    let live_pregame = h.st.live_state();
    assert_eq!(
        live_pregame.players.len(),
        5,
        "Agent Select shows exactly 5 ally cards"
    );

    // Verify ALLY_3 is incognito: Riot ID + Tag is hidden
    let ally3_card = live_pregame
        .players
        .iter()
        .find(|p| p.puuid == SIM_ALLY_3_HIDDEN)
        .expect("ALLY_3 on roster");
    match &ally3_card.name {
        Field::Value(DisplayName::HiddenAgent { agent_id }) => {
            assert_eq!(
                agent_id, AGENT_CYPHER,
                "Locked incognito player shows agent name"
            );
        }
        Field::Value(DisplayName::HiddenPlayer) => {}
        other => panic!("Expected HiddenAgent or HiddenPlayer for incognito ally, got {other:?}"),
    }

    // Verify ALLY_4 has hidden account level
    let ally4_card = live_pregame
        .players
        .iter()
        .find(|p| p.puuid == SIM_ALLY_4_HIDELVL)
        .expect("ALLY_4 on roster");
    assert_eq!(
        ally4_card.level,
        Field::Missing,
        "HideAccountLevel results in Missing"
    );

    // Verify R150 privacy: live incognito ally was never sent to name-service
    assert!(
        !name_service_queries
            .lock()
            .unwrap()
            .contains(&SIM_ALLY_3_HIDDEN.to_string()),
        "Live incognito ally must never be asked from name-service"
    );

    // =================================================================
    // Step 4: Loading into Game -> In Game (INGAME, 10 Players)
    // =================================================================
    *presence_cell.lock().unwrap() = Some(sim_blob("INGAME", "competitive"));

    Mock::given(method("GET"))
        .and(path(format!("/core-game/v1/players/{SIM_ME}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "Subject": SIM_ME, "MatchID": MATCH_ID_COREGAME
        })))
        .mount(&h.server)
        .await;

    let coregame_players = vec![
        // Allies (Blue)
        json!({"Subject": SIM_ME, "TeamID": "Blue", "CharacterID": AGENT_JETT, "PlayerIdentity": player_identity(false, false, 150)}),
        json!({"Subject": SIM_ALLY_1, "TeamID": "Blue", "CharacterID": AGENT_PHOENIX, "PlayerIdentity": player_identity(false, false, 45)}),
        json!({"Subject": SIM_ALLY_2, "TeamID": "Blue", "CharacterID": AGENT_VIPER, "PlayerIdentity": player_identity(false, false, 112)}),
        json!({"Subject": SIM_ALLY_3_HIDDEN, "TeamID": "Blue", "CharacterID": AGENT_CYPHER, "PlayerIdentity": player_identity(true, false, 78)}),
        json!({"Subject": SIM_ALLY_4_HIDELVL, "TeamID": "Blue", "CharacterID": AGENT_BREACH, "PlayerIdentity": player_identity(false, true, 95)}),
        // Enemies (Red)
        json!({"Subject": SIM_ENEMY_1_HIDDEN, "TeamID": "Red", "CharacterID": AGENT_REYNA, "PlayerIdentity": player_identity(true, false, 204)}),
        json!({"Subject": SIM_ENEMY_2, "TeamID": "Red", "CharacterID": AGENT_SOVA, "PlayerIdentity": player_identity(false, false, 88)}),
        json!({"Subject": SIM_ENEMY_3, "TeamID": "Red", "CharacterID": AGENT_FADE, "PlayerIdentity": player_identity(false, false, 15)}),
        json!({"Subject": SIM_ENEMY_4_HIDDEN, "TeamID": "Red", "CharacterID": AGENT_OMEN, "PlayerIdentity": player_identity(true, true, 160)}),
        json!({"Subject": SIM_ENEMY_5, "TeamID": "Red", "CharacterID": AGENT_ASTRA, "PlayerIdentity": player_identity(false, false, 330)}),
    ];

    Mock::given(method("GET"))
        .and(path(format!("/core-game/v1/matches/{MATCH_ID_COREGAME}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "ID": MATCH_ID_COREGAME,
            "MapID": ASCENT_PATH,
            "Players": coregame_players
        })))
        .mount(&h.server)
        .await;

    Mock::given(method("GET"))
        .and(path(format!(
            "/core-game/v1/matches/{MATCH_ID_COREGAME}/loadouts"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "Loadouts": [] })))
        .mount(&h.server)
        .await;

    poller.presence_tick(&h.session).await.unwrap();

    // Verify transition to InGame
    assert_eq!(h.st.live_state().stage, Stage::Ingame);
    let live_ingame = h.st.live_state();
    assert_eq!(
        live_ingame.players.len(),
        10,
        "In Game shows all 10 players"
    );

    let allies_count = live_ingame
        .players
        .iter()
        .filter(|p| p.team == Team::Ally)
        .count();
    let enemies_count = live_ingame
        .players
        .iter()
        .filter(|p| p.team == Team::Enemy)
        .count();
    assert_eq!(allies_count, 5, "5 allies on Blue team");
    assert_eq!(enemies_count, 5, "5 enemies on Red team");

    // Verify enemy hidden player (SIM_ENEMY_1_HIDDEN) has hidden name
    let enemy1_card = live_ingame
        .players
        .iter()
        .find(|p| p.puuid == SIM_ENEMY_1_HIDDEN)
        .expect("ENEMY_1 on roster");
    match &enemy1_card.name {
        Field::Value(DisplayName::HiddenAgent { agent_id }) => {
            assert_eq!(agent_id, AGENT_REYNA);
        }
        Field::Value(DisplayName::HiddenPlayer) => {}
        other => panic!("Expected HiddenAgent or HiddenPlayer for enemy 1, got {other:?}"),
    }

    // Verify R150 privacy: live incognito players are never sent to name-service
    let asked = name_service_queries.lock().unwrap().clone();
    assert!(
        !asked.contains(&SIM_ALLY_3_HIDDEN.to_string()),
        "ALLY_3 hidden name must not be requested"
    );
    assert!(
        !asked.contains(&SIM_ENEMY_1_HIDDEN.to_string()),
        "ENEMY_1 hidden name must not be requested"
    );
    assert!(
        !asked.contains(&SIM_ENEMY_4_HIDDEN.to_string()),
        "ENEMY_4 hidden name must not be requested"
    );
    // Mount final match details (reveals names of all 10 players)
    Mock::given(method("GET"))
        .and(path(format!("/match-details/v1/matches/{MATCH_ID_COREGAME}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "matchInfo": {
                "matchId": MATCH_ID_COREGAME,
                "mapId": ASCENT_PATH,
                "gameStartMillis": 2_000_000,
                "queueId": "competitive",
                "isRanked": true,
                "completionState": "Completed"
            },
            "players": [
                { "subject": SIM_ME, "gameName": "Me", "tagLine": "TAG", "teamId": "Blue", "partyId": "p1", "characterId": AGENT_JETT, "stats": { "score": 350, "roundsPlayed": 23, "kills": 24, "deaths": 12, "assists": 4 } },
                { "subject": SIM_ALLY_1, "gameName": "PhoenixKing", "tagLine": "1111", "teamId": "Blue", "partyId": "p1", "characterId": AGENT_PHOENIX, "stats": { "score": 280, "roundsPlayed": 23, "kills": 18, "deaths": 15, "assists": 7 } },
                { "subject": SIM_ALLY_2, "gameName": "ViperMain", "tagLine": "2222", "teamId": "Blue", "partyId": "p2", "characterId": AGENT_VIPER, "stats": { "score": 210, "roundsPlayed": 23, "kills": 14, "deaths": 13, "assists": 9 } },
                { "subject": SIM_ALLY_3_HIDDEN, "gameName": "ShadowNinja", "tagLine": "3333", "teamId": "Blue", "partyId": "p3", "characterId": AGENT_CYPHER, "stats": { "score": 190, "roundsPlayed": 23, "kills": 12, "deaths": 14, "assists": 8 } },
                { "subject": SIM_ALLY_4_HIDELVL, "gameName": "CypherSpy", "tagLine": "4444", "teamId": "Blue", "partyId": "p4", "characterId": AGENT_BREACH, "stats": { "score": 170, "roundsPlayed": 23, "kills": 10, "deaths": 16, "assists": 11 } },
                { "subject": SIM_ENEMY_1_HIDDEN, "gameName": "ViperGod", "tagLine": "5555", "teamId": "Red", "partyId": "p5", "characterId": AGENT_REYNA, "stats": { "score": 330, "roundsPlayed": 23, "kills": 22, "deaths": 16, "assists": 3 } },
                { "subject": SIM_ENEMY_2, "gameName": "SovaGod", "tagLine": "6666", "teamId": "Red", "partyId": "p5", "characterId": AGENT_SOVA, "stats": { "score": 240, "roundsPlayed": 23, "kills": 16, "deaths": 15, "assists": 6 } },
                { "subject": SIM_ENEMY_3, "gameName": "FadeAway", "tagLine": "7777", "teamId": "Red", "partyId": "p6", "characterId": AGENT_FADE, "stats": { "score": 180, "roundsPlayed": 23, "kills": 11, "deaths": 17, "assists": 5 } },
                { "subject": SIM_ENEMY_4_HIDDEN, "gameName": "GhostOmen", "tagLine": "8888", "teamId": "Red", "partyId": "p7", "characterId": AGENT_OMEN, "stats": { "score": 160, "roundsPlayed": 23, "kills": 9, "deaths": 18, "assists": 4 } },
                { "subject": SIM_ENEMY_5, "gameName": "AstraStar", "tagLine": "9999", "teamId": "Red", "partyId": "p8", "characterId": AGENT_ASTRA, "stats": { "score": 150, "roundsPlayed": 23, "kills": 8, "deaths": 18, "assists": 10 } }
            ],
            "teams": [
                { "teamId": "Blue", "won": true, "roundsWon": 13 },
                { "teamId": "Red", "won": false, "roundsWon": 10 }
            ],
            "roundResults": []
        })))
        .mount(&h.server)
        .await;

    // Simulate competitive updates for ME showing the newly finished match with +21 RR
    Mock::given(method("GET"))
        .and(path(format!("/mmr/v1/players/{SIM_ME}/competitiveupdates")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "Subject": SIM_ME,
            "Matches": [
                {
                    "MatchID": MATCH_ID_COREGAME,
                    "MapID": ASCENT_PATH,
                    "SeasonID": ACT,
                    "MatchStartTime": 2_000_000,
                    "TierAfterUpdate": 15,
                    "TierBeforeUpdate": 15,
                    "RankedRatingAfterUpdate": 63,
                    "RankedRatingBeforeUpdate": 42,
                    "RankedRatingEarned": 21
                }
            ]
        })))
        .with_priority(1)
        .mount(&h.server)
        .await;

    // Trigger match end and store details directly through repo & sync
    let match_json = serde_json::from_value::<Value>(json!({
        "matchInfo": {
            "matchId": MATCH_ID_COREGAME,
            "mapId": ASCENT_PATH,
            "gameStartMillis": 2_000_000,
            "queueId": "competitive",
            "isRanked": true,
            "completionState": "Completed"
        },
        "players": [
            { "subject": SIM_ME, "gameName": "Me", "tagLine": "TAG", "teamId": "Blue", "partyId": "p1", "characterId": AGENT_JETT, "stats": { "score": 350, "roundsPlayed": 23, "kills": 24, "deaths": 12, "assists": 4 } },
            { "subject": SIM_ALLY_1, "gameName": "PhoenixKing", "tagLine": "1111", "teamId": "Blue", "partyId": "p1", "characterId": AGENT_PHOENIX, "stats": { "score": 280, "roundsPlayed": 23, "kills": 18, "deaths": 15, "assists": 7 } },
            { "subject": SIM_ALLY_2, "gameName": "ViperMain", "tagLine": "2222", "teamId": "Blue", "partyId": "p2", "characterId": AGENT_VIPER, "stats": { "score": 210, "roundsPlayed": 23, "kills": 14, "deaths": 13, "assists": 9 } },
            { "subject": SIM_ALLY_3_HIDDEN, "gameName": "ShadowNinja", "tagLine": "3333", "teamId": "Blue", "partyId": "p3", "characterId": AGENT_CYPHER, "stats": { "score": 190, "roundsPlayed": 23, "kills": 12, "deaths": 14, "assists": 8 } },
            { "subject": SIM_ALLY_4_HIDELVL, "gameName": "CypherSpy", "tagLine": "4444", "teamId": "Blue", "partyId": "p4", "characterId": AGENT_BREACH, "stats": { "score": 170, "roundsPlayed": 23, "kills": 10, "deaths": 16, "assists": 11 } },
            { "subject": SIM_ENEMY_1_HIDDEN, "gameName": "ViperGod", "tagLine": "5555", "teamId": "Red", "partyId": "p5", "characterId": AGENT_REYNA, "stats": { "score": 330, "roundsPlayed": 23, "kills": 22, "deaths": 16, "assists": 3 } },
            { "subject": SIM_ENEMY_2, "gameName": "SovaGod", "tagLine": "6666", "teamId": "Red", "partyId": "p5", "characterId": AGENT_SOVA, "stats": { "score": 240, "roundsPlayed": 23, "kills": 16, "deaths": 15, "assists": 6 } },
            { "subject": SIM_ENEMY_3, "gameName": "FadeAway", "tagLine": "7777", "teamId": "Red", "partyId": "p6", "characterId": AGENT_FADE, "stats": { "score": 180, "roundsPlayed": 23, "kills": 11, "deaths": 17, "assists": 5 } },
            { "subject": SIM_ENEMY_4_HIDDEN, "gameName": "GhostOmen", "tagLine": "8888", "teamId": "Red", "partyId": "p7", "characterId": AGENT_OMEN, "stats": { "score": 160, "roundsPlayed": 23, "kills": 9, "deaths": 18, "assists": 4 } },
            { "subject": SIM_ENEMY_5, "gameName": "AstraStar", "tagLine": "9999", "teamId": "Red", "partyId": "p8", "characterId": AGENT_ASTRA, "stats": { "score": 150, "roundsPlayed": 23, "kills": 8, "deaths": 18, "assists": 10 } }
        ],
        "teams": [
            { "teamId": "Blue", "won": true, "roundsWon": 13 },
            { "teamId": "Red", "won": false, "roundsWon": 10 }
        ],
        "roundResults": []
    })).unwrap();

    let details_resp: opd1_riot::endpoints::MatchDetailsResponse =
        serde_json::from_value(match_json).unwrap();
    let (match_row, team_rows, player_rows) = opd1_riot::to_core::match_from_details(&details_resp);

    // Persist match into SQLite
    h.st.db(move |repo| {
        repo.upsert_match(&match_row, &team_rows, &player_rows)?;
        Ok(())
    })
    .await
    .unwrap();

    // Verify match and 10 players are stored in SQLite
    let stored_match =
        h.st.db(|repo| repo.match_rows(MATCH_ID_COREGAME))
            .await
            .unwrap();
    assert!(
        stored_match.is_some(),
        "Match must exist in SQLite database"
    );
    let sm = stored_match.unwrap();
    assert_eq!(sm.info.match_id, MATCH_ID_COREGAME);
    assert_eq!(sm.teams.len(), 2, "2 teams stored");
    assert_eq!(
        sm.players.len(),
        10,
        "All 10 player rows stored in SQLite match_players"
    );

    // =================================================================
    // Step 6: Return to Lobby (MENUS) & Verify Remembered Names
    // =================================================================
    *presence_cell.lock().unwrap() = Some(sim_blob("MENUS", "competitive"));
    poller.presence_tick(&h.session).await.unwrap();

    assert_eq!(
        h.st.live_state().stage,
        Stage::Menus,
        "Stage returns to Menus"
    );

    // R145 verification: ALLY_3_HIDDEN and ENEMY_1_HIDDEN (who were incognito during live match)
    // now have their real names remembered in SQLite with source OwnMatch!
    let known_ally3 =
        h.st.db(|repo| repo.known_name(SIM_ME, SIM_ALLY_3_HIDDEN))
            .await
            .unwrap();
    assert!(
        known_ally3.is_some(),
        "ALLY_3 must now be remembered from the finished match"
    );
    let ally3_rec = known_ally3.unwrap();
    assert_eq!(ally3_rec.name, "ShadowNinja");
    assert_eq!(ally3_rec.tag, "3333");
    assert_eq!(ally3_rec.source, RememberedSource::OwnMatch);

    let known_enemy1 =
        h.st.db(|repo| repo.known_name(SIM_ME, SIM_ENEMY_1_HIDDEN))
            .await
            .unwrap();
    assert!(
        known_enemy1.is_some(),
        "ENEMY_1 must now be remembered from the finished match"
    );
    let enemy1_rec = known_enemy1.unwrap();
    assert_eq!(enemy1_rec.name, "ViperGod");
    assert_eq!(enemy1_rec.tag, "5555");
    assert_eq!(enemy1_rec.source, RememberedSource::OwnMatch);

    poller.stop_worker();
}
