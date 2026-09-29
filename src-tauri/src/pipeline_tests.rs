//! The real sync pipeline (`sync.rs` + `state.rs` + `store.rs`) against a
//! mock Riot client (wiremock) and an in-memory DB: request counts for a
//! cold vs a warm DB ("sync, don't refetch"), hidden players never sent
//! to name-service, hidden names shown only when remembered from the
//! user's own matches, and failed calls settling to `null` instead of a
//! skeleton. The poll loop's timing itself isn't tested here (by plan);
//! its decisions are, in `roster.rs`.

use std::path::Path;
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

use opd1_core::db::open_db;
use opd1_core::db::repo::{HistoryEntry, Match, MatchPlayer, MmrSnapshot, Repo};
use opd1_core::model::{DisplayName, EncounterCount, Field, LiveState, PlayerDetails, Team};
use opd1_core::presence::Stage;
use opd1_core::stats::Act;
use opd1_riot::assets::AssetCache;
use opd1_riot::endpoints::PlayerMmrResponse;
use opd1_riot::session::{ConnectOverrides, RiotSession};

use crate::roster::RosterPlayer;
use crate::state::{lock, now_ms, AppState, EventSink, Slot};
use crate::store::{load_player_db, save_player, trim_mmr, PlayerDb, PlayerUpdate};
use crate::sync::{fetch_self_name, run_worker, sync_player};

pub(crate) const ME: &str = "aaaaaaaa-0000-0000-0000-000000000001";
const HIDDEN: &str = "bbbbbbbb-0000-0000-0000-000000000002";
const OTHER: &str = "cccccccc-0000-0000-0000-000000000003";
/// Incognito, and never in a match with ME: their name is only in the DB
/// from someone else's match.
const STRANGER: &str = "dddddddd-0000-0000-0000-000000000004";
/// OTHER's teammate in the party-mark tests.
const MATE: &str = "eeeeeeee-0000-0000-0000-000000000005";
const ACT: &str = "act-current";
/// valorant-api's Split: the map the remembered-name tests play on.
const SPLIT_PATH: &str = "/Game/Maps/Bonsai/Bonsai";
const SPLIT_UUID: &str = "d960549e-485c-e861-8d71-aa9d1aed12a2";

#[derive(Default)]
struct Recorder {
    live: Mutex<Vec<LiveState>>,
}

struct Sink(Arc<Recorder>);

impl EventSink for Sink {
    fn live(&self, state: &LiveState) {
        self.0.live.lock().unwrap().push(state.clone());
    }
    fn details(&self, _details: &PlayerDetails) {}
}

fn comp_updates(matches: &[(&str, i64, i64)]) -> Value {
    json!({
        "Subject": ME,
        "Matches": matches.iter().map(|(id, start, rr)| json!({
            "MatchID": id, "MapID": "/Game/Maps/Ascent/Ascent", "SeasonID": ACT,
            "MatchStartTime": start, "TierAfterUpdate": 15, "TierBeforeUpdate": 15,
            "RankedRatingAfterUpdate": 42, "RankedRatingBeforeUpdate": 42 - rr,
            "RankedRatingEarned": rr
        })).collect::<Vec<_>>()
    })
}

fn details(match_id: &str, queue: &str, start: i64) -> Value {
    json!({
        "matchInfo": {
            "matchId": match_id, "mapId": "/Game/Maps/Ascent/Ascent",
            "gameStartMillis": start, "queueId": queue,
            "isRanked": queue == "competitive", "completionState": "Completed"
        },
        "players": [
            {"subject": ME, "gameName": "Me", "tagLine": "TAG", "teamId": "Blue",
             "partyId": "p", "characterId": "agent-me",
             "stats": {"score": 1, "roundsPlayed": 20, "kills": 20, "deaths": 10, "assists": 3}},
            // Post-match details carry a hidden player's real name — ME
            // played this match, so it's the "remembered" name (CLAUDE.md's
            // hidden-name exception), shown in its own style.
            {"subject": HIDDEN, "gameName": "SecretName", "tagLine": "HID", "teamId": "Red",
             "partyId": "q", "characterId": "agent-h",
             "stats": {"score": 1, "roundsPlayed": 20, "kills": 5, "deaths": 15, "assists": 1}}
        ],
        "teams": [
            {"teamId": "Blue", "won": true, "roundsWon": 13},
            {"teamId": "Red", "won": false, "roundsWon": 7}
        ],
        "roundResults": []
    })
}

fn mmr() -> Value {
    json!({
        "Subject": ME,
        "QueueSkills": {"competitive": {"SeasonalInfoBySeasonID": {ACT: {
            "SeasonID": ACT, "CompetitiveTier": 15, "RankedRating": 42,
            "NumberOfWins": 6, "NumberOfGames": 10, "WinsByTier": {"15": 4}
        }}}}
    })
}

async fn get(server: &MockServer, p: &str, body: Value) {
    Mock::given(method("GET"))
        .and(path(p))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(server)
        .await;
}

/// Mounts the local-client endpoints `connect()` needs and ME's remote
/// endpoints: comp updates (m1), history (m1 + a deathmatch m2), details
/// for both, MMR, names.
async fn mock_riot(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/entitlements/v1/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "accessToken": "a", "entitlements": [], "issuer": "i", "subject": ME, "token": "t"
        })))
        .mount(server)
        .await;
    get(
        server,
        "/product-session/v1/external-sessions",
        json!({"s": {"productId": "valorant", "version": "release-test"}}),
    )
    .await;

    let comp = format!("/mmr/v1/players/{ME}/competitiveupdates");
    // The first two syncs see one competitive match…
    Mock::given(method("GET"))
        .and(path(comp.as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(comp_updates(&[("m1", 1_000, 21)])))
        .up_to_n_times(2)
        .mount(server)
        .await;
    // …then a new one (m3) lands.
    get(
        server,
        &comp,
        comp_updates(&[("m3", 3_000, -18), ("m1", 1_000, 21)]),
    )
    .await;

    get(
        server,
        &format!("/match-history/v1/history/{ME}"),
        json!({"Subject": ME, "History": [
            {"MatchID": "m1", "GameStartTime": 1_000, "QueueID": "competitive"},
            {"MatchID": "m2", "GameStartTime": 2_000, "QueueID": "deathmatch"}
        ]}),
    )
    .await;
    get(
        server,
        "/match-details/v1/matches/m1",
        details("m1", "competitive", 1_000),
    )
    .await;
    get(
        server,
        "/match-details/v1/matches/m2",
        details("m2", "deathmatch", 2_000),
    )
    .await;
    get(
        server,
        "/match-details/v1/matches/m3",
        details("m3", "competitive", 3_000),
    )
    .await;
    get(server, &format!("/mmr/v1/players/{ME}"), mmr()).await;

    // The user's own Riot ID comes from the local chat session (R43)…
    get(
        server,
        "/chat/v1/session",
        json!({"federated": true, "game_name": "Me", "game_tag": "TAG", "loaded": true,
               "name": "", "pid": "", "puuid": ME, "region": "eu", "resource": "", "state": "connected"}),
    )
    .await;
    // …name-service only ever answers for other visible players.
    Mock::given(method("PUT"))
        .and(path("/name-service/v2/players"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"DisplayName": "", "Subject": OTHER, "GameName": "Other", "TagLine": "OTH"}
        ])))
        .mount(server)
        .await;

    // Every HIDDEN / OTHER call fails: their cards must settle to `—`.
    for p in [
        format!("/mmr/v1/players/{HIDDEN}/competitiveupdates"),
        format!("/match-history/v1/history/{HIDDEN}"),
        format!("/mmr/v1/players/{HIDDEN}"),
        format!("/mmr/v1/players/{OTHER}/competitiveupdates"),
        format!("/match-history/v1/history/{OTHER}"),
        format!("/mmr/v1/players/{OTHER}"),
    ] {
        Mock::given(method("GET"))
            .and(path(p.as_str()))
            .respond_with(ResponseTemplate::new(500))
            .mount(server)
            .await;
    }
}

fn write_client_files(dir: &Path, port: u16) {
    let cfg = dir.join("Riot Games").join("Riot Client").join("Config");
    std::fs::create_dir_all(&cfg).unwrap();
    std::fs::write(
        cfg.join("lockfile"),
        format!("Riot Client:1:{port}:pw:http"),
    )
    .unwrap();
    let logs = dir.join("VALORANT").join("Saved").join("Logs");
    std::fs::create_dir_all(&logs).unwrap();
    std::fs::write(
        logs.join("ShooterGame.log"),
        "https://pd.eu.a.pvp.net/account-xp/v1/players/x\nhttps://glz-eu-1.eu.a.pvp.net/x\n",
    )
    .unwrap();
}

pub(crate) struct Harness {
    server: MockServer,
    /// valorant-api. No routes are mounted until a test calls
    /// [`mount_asset_tables`], so it answers 404: the asset tables stay
    /// unloaded and every map id resolves to `None`.
    assets_server: MockServer,
    _dir: tempfile::TempDir,
    pub(crate) session: Arc<RiotSession>,
    pub(crate) st: Arc<AppState>,
    rec: Arc<Recorder>,
}

pub(crate) async fn harness() -> Harness {
    let server = MockServer::start().await;
    mock_riot(&server).await;
    let dir = tempfile::tempdir().unwrap();
    write_client_files(dir.path(), server.address().port());
    let base = server.uri();
    let session = RiotSession::connect_with_overrides(
        dir.path(),
        ConnectOverrides {
            pd_base: Some(base.clone()),
            glz_base: Some(base.clone()),
            valorant_api_base: Some(base),
            ..Default::default()
        },
    )
    .await
    .expect("connect to the mock client");

    let rec = Arc::new(Recorder::default());
    let repo = Repo::new(open_db(Path::new(":memory:")).unwrap());
    // No asset tables until a test mounts them: map ids resolve to None,
    // never to a path.
    let assets_server = MockServer::start().await;
    let assets = AssetCache::with_base(dir.path().join("assets"), assets_server.uri());
    let st = Arc::new(AppState::new(
        Box::new(Sink(rec.clone())),
        repo,
        assets,
        dir.path().join("debug"),
        Some(ME.into()),
    ));
    let now = now_ms();
    st.set_acts_for_test(vec![Act {
        id: ACT.into(),
        name: "ACT V".into(),
        episode_name: "V26".into(),
        start_ms: now - 30 * 86_400_000,
        end_ms: now + 30 * 86_400_000,
    }]);
    Harness {
        server,
        assets_server,
        _dir: dir,
        session: Arc::new(session),
        st,
        rec,
    }
}

async fn count(server: &MockServer, verb: &str, p: &str) -> usize {
    server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r: &&Request| r.method.as_str() == verb && r.url.path() == p)
        .count()
}

/// Mounts minimal valorant-api tables (`crate::state::AppState::ensure_tables`
/// then loads them): Split as the only map, one act (`ACT`) that spans every
/// stored match, and one tier.
async fn mount_asset_tables(server: &MockServer) {
    get(
        server,
        "/v1/version",
        json!({"status": 200, "data": {"version": "test-1"}}),
    )
    .await;
    get(
        server,
        "/v1/agents",
        json!({"status": 200, "data": [{
            "uuid": "aaaa1111-0000-0000-0000-000000000001", "displayName": "Gekko",
            "displayIcon": format!("{}/img/agent.png", server.uri())
        }]}),
    )
    .await;
    get(
        server,
        "/v1/maps",
        json!({"status": 200, "data": [{
            "uuid": SPLIT_UUID, "displayName": "Split", "mapUrl": SPLIT_PATH,
            "listViewIcon": format!("{}/img/map-list.png", server.uri()),
            "splash": format!("{}/img/map-splash.png", server.uri())
        }]}),
    )
    .await;
    get(
        server,
        "/v1/competitivetiers",
        json!({"status": 200, "data": [{
            "assetObjectName": "Episode1_CompetitiveTierDataTable",
            "tiers": [{"tier": 15, "tierName": "PLATINUM 1", "color": "4f514fff",
                       "largeIcon": format!("{}/img/tier.png", server.uri())}]
        }]}),
    )
    .await;
    get(
        server,
        "/v1/seasons",
        json!({"status": 200, "data": [
            {"uuid": "episode-1", "displayName": "EPISODE 1", "title": null, "type": null,
             "startTime": "2020-01-01T00:00:00Z", "endTime": "2099-01-01T00:00:00Z",
             "parentUuid": null},
            {"uuid": ACT, "displayName": "ACT I", "title": null,
             "type": "EAresSeasonType::Act",
             "startTime": "2020-01-01T00:00:00Z", "endTime": "2099-01-01T00:00:00Z",
             "parentUuid": "episode-1"}
        ]}),
    )
    .await;
}

#[tokio::test]
async fn warm_db_skips_known_details_and_unchanged_mmr() {
    let h = harness().await;
    let comp = format!("/mmr/v1/players/{ME}/competitiveupdates");
    let mmr_path = format!("/mmr/v1/players/{ME}");

    // Cold: everything is fetched once.
    sync_player(&h.st, &h.session, ME).await.unwrap();
    assert_eq!(count(&h.server, "GET", &comp).await, 1);
    assert_eq!(
        count(&h.server, "GET", "/match-details/v1/matches/m1").await,
        1
    );
    assert_eq!(
        count(&h.server, "GET", "/match-details/v1/matches/m2").await,
        1
    );
    assert_eq!(count(&h.server, "GET", &mmr_path).await, 1);

    // Warm, nothing new: only the two cheap list calls.
    sync_player(&h.st, &h.session, ME).await.unwrap();
    assert_eq!(count(&h.server, "GET", &comp).await, 2);
    assert_eq!(
        count(&h.server, "GET", &format!("/match-history/v1/history/{ME}")).await,
        2
    );
    assert_eq!(
        count(&h.server, "GET", "/match-details/v1/matches/m1").await,
        1
    );
    assert_eq!(
        count(&h.server, "GET", "/match-details/v1/matches/m2").await,
        1
    );
    assert_eq!(
        count(&h.server, "GET", &mmr_path).await,
        1,
        "no new comp match → no MMR call"
    );

    // A new competitive match: only its details + one MMR refresh.
    sync_player(&h.st, &h.session, ME).await.unwrap();
    assert_eq!(
        count(&h.server, "GET", "/match-details/v1/matches/m3").await,
        1
    );
    assert_eq!(
        count(&h.server, "GET", "/match-details/v1/matches/m1").await,
        1
    );
    assert_eq!(count(&h.server, "GET", &mmr_path).await, 2);

    let stored: i64 =
        h.st.db(|repo| {
            repo.conn()
                .query_row("SELECT COUNT(*) FROM matches", [], |r| r.get(0))
        })
        .await
        .unwrap();
    assert_eq!(stored, 3);
    // Every sync ran the comp-updates query with the right window.
    let first = h
        .server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .find(|r| r.url.path() == comp)
        .unwrap();
    let q = first.url.query().unwrap_or_default().to_string();
    assert!(
        q.contains("startIndex=0") && q.contains("endIndex=5") && q.contains("queue=competitive")
    );
}

pub(crate) fn roster(puuid: &str, incognito: bool) -> RosterPlayer {
    RosterPlayer {
        puuid: puuid.into(),
        team_id: if puuid == ME { "Blue" } else { "Red" }.into(),
        team: if puuid == ME { Team::Ally } else { Team::Enemy },
        is_self_party: puuid == ME,
        incognito,
        hide_account_level: false,
        account_level: 100,
        card_id: "card".into(),
        character_id: "agent".into(),
        selection_state: "locked".into(),
    }
}

/// A competitive match of the current act that ME did not play: HIDDEN and
/// STRANGER as teammates, with real names (as someone else's match history
/// would carry them) — so each is in the other's Top Teammates.
async fn store_foreign_match(st: &AppState) {
    let player = |puuid: &str, name: &str| MatchPlayer {
        puuid: puuid.into(),
        team_id: "Blue".into(),
        party_id: None,
        agent_id: None,
        kills: 0,
        deaths: 0,
        assists: 0,
        score: 0,
        rounds: None,
        hs: 0,
        bs: 0,
        ls: 0,
        game_name: Some(name.into()),
        tag_line: Some("LEAK".into()),
    };
    let m = Match {
        match_id: "m-foreign".into(),
        map_id: "/Game/Maps/Ascent/Ascent".into(),
        queue_id: "competitive".into(),
        start_ms: now_ms() - 60_000,
        winning_team: None,
        is_ranked: false,
        rounds_played: None,
    };
    let players = vec![player(HIDDEN, "SecretName"), player(STRANGER, "LeakedName")];
    st.db(move |repo| repo.upsert_match(&m, &[], &players))
        .await
        .unwrap();
}

#[tokio::test]
async fn worker_names_only_visible_players_and_settles_failures() {
    let h = harness().await;
    store_foreign_match(&h.st).await;
    {
        let mut live = lock(&h.st.live);
        live.stage = Stage::Ingame;
        live.slots = vec![
            Slot {
                roster: roster(ME, false),
                db: PlayerDb::default(),
            },
            Slot {
                roster: roster(HIDDEN, true),
                db: PlayerDb::default(),
            },
            Slot {
                roster: roster(OTHER, false),
                db: PlayerDb::default(),
            },
            Slot {
                roster: roster(STRANGER, true),
                db: PlayerDb::default(),
            },
        ];
    }
    fetch_self_name(&h.st, &h.session).await.unwrap();
    let id = lock(&h.st.live).try_start_worker().expect("work pending");
    run_worker(h.st.clone(), h.session.clone(), id).await;
    assert!(!lock(&h.st.live).worker_running(), "claim released on exit");

    // Exactly one name-service call: the other visible player only —
    // never the hidden player, never the user (R43).
    let puts: Vec<Request> = h
        .server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.method.as_str() == "PUT")
        .collect();
    assert_eq!(puts.len(), 1);
    let body = String::from_utf8(puts[0].body.clone()).unwrap();
    assert!(body.contains(OTHER));
    assert!(
        !body.contains(HIDDEN) && !body.contains(STRANGER),
        "a hidden player's name must never be requested"
    );
    assert!(!body.contains(ME), "the user is never sent to name-service");

    let emitted = h.rec.live.lock().unwrap().clone();
    assert!(emitted.len() >= 2, "re-emits as each player completes");
    let last = emitted.last().unwrap();
    let me = &last.players[0];
    assert_eq!(
        me.name,
        Field::Value(DisplayName::Visible {
            name: "Me".into(),
            tag: "TAG".into()
        })
    );
    assert!(matches!(me.rank, Field::Value(r) if r.tier == 15 && r.rr == 42));
    assert_eq!((me.party, me.encounters), (None, None));
    assert!(matches!(me.last5, Field::Value(ref v) if v.len() == 1));
    assert_eq!(me.kd, Field::Value(2.0));

    let other = &last.players[2];
    assert_eq!(
        other.name,
        Field::Value(DisplayName::Visible {
            name: "Other".into(),
            tag: "OTH".into()
        })
    );

    assert_eq!(other.encounters, None, "never met: no badge");

    // HIDDEN played m1 + m2 against ME: their name as shown after those
    // matches is remembered, and the badge counts both.
    let hidden = &last.players[1];
    assert_eq!(
        hidden.name,
        Field::Value(DisplayName::Remembered {
            name: "SecretName".into(),
            tag: "HID".into(),
            // The newest shared match is m2; the asset tables aren't loaded,
            // so its map has no uuid.
            from_map_id: None,
            from_start_ms: Some(2_000),
        })
    );
    assert_eq!(
        hidden.encounters,
        Some(EncounterCount {
            with: 0,
            against: 2
        })
    );
    assert_eq!(hidden.party, None, "no synced history: no party link");

    // STRANGER's name is only in a match ME didn't play: never shown.
    let stranger = &last.players[3];
    assert_eq!(
        stranger.name,
        Field::Value(DisplayName::HiddenAgent {
            agent_id: "agent".into()
        })
    );
    assert_eq!(stranger.encounters, None);
    // All of HIDDEN's own calls failed: the MMR fields are `—`, never a
    // stuck skeleton…
    let json = serde_json::to_value(hidden).unwrap();
    for key in ["rank", "prevActTier", "peak", "actWinrate"] {
        assert_eq!(
            json.get(key),
            Some(&Value::Null),
            "{key} must settle to null"
        );
    }
    // …while the competitive matches already in the DB (m1 with ME, and
    // the foreign one) still fill their last-5 — every non-name field
    // stays visible.
    assert!(matches!(hidden.last5, Field::Value(ref v) if v.len() == 2));
    assert_eq!(hidden.kd, Field::Value(5.0 / 15.0));
    // A name seen only in someone else's match never reaches the live
    // payload (nor does HIDDEN's name from that foreign match).
    let payload = serde_json::to_string(last).unwrap();
    assert!(!payload.contains("LeakedName") && !payload.contains("LEAK"));

    // Player Details for ME: recent from the DB; no encounters with self.
    let d = crate::details::build(&h.st, ME).await.unwrap();
    assert_eq!(d.recent.len(), 1);
    assert_eq!(d.recent[0].rr_earned, Some(21));
    assert_eq!(
        d.recent[0].map_id, "",
        "unknown map → empty id, never the Riot path"
    );
    assert!(d.encounters.is_empty());
    // Details for the hidden enemy: encounters exist (m1, m2), their card
    // keeps the remembered name, and STRANGER (their teammate in the
    // foreign match, hidden on screen) is never named in Top Teammates.
    let d = crate::details::build(&h.st, HIDDEN).await.unwrap();
    assert_eq!(d.encounters.len(), 2);
    assert!(d.encounters.iter().all(|e| !e.with));
    assert!(matches!(
        d.card.name,
        Field::Value(DisplayName::Remembered { .. })
    ));
    assert_eq!(d.top_teammates.len(), 1);
    assert_eq!(d.top_teammates[0].puuid, STRANGER);
    assert_eq!(
        d.top_teammates[0].name,
        DisplayName::HiddenAgent {
            agent_id: "agent".into()
        }
    );
    let json = serde_json::to_string(&d).unwrap();
    assert!(!json.contains("LeakedName") && !json.contains("LEAK"));

    // STRANGER's details: HIDDEN (hidden on screen) as a teammate shows the
    // name remembered from ME's own match — never the foreign one.
    let d = crate::details::build(&h.st, STRANGER).await.unwrap();
    assert_eq!(d.top_teammates.len(), 1);
    assert_eq!(
        d.top_teammates[0].name,
        DisplayName::Remembered {
            name: "SecretName".into(),
            tag: "HID".into(),
            from_map_id: None,
            from_start_ms: Some(2_000),
        }
    );
    let json = serde_json::to_string(&d).unwrap();
    assert!(!json.contains("LeakedName") && !json.contains("LEAK"));
}

// ---------------------------------------------------------------------
// Task 14: isSelf, remembered-name source, act wins/games
// ---------------------------------------------------------------------

fn player_row(puuid: &str, team: &str, name: &str, tag: &str) -> MatchPlayer {
    MatchPlayer {
        puuid: puuid.into(),
        team_id: team.into(),
        party_id: None,
        agent_id: None,
        kills: 0,
        deaths: 0,
        assists: 0,
        score: 0,
        rounds: None,
        hs: 0,
        bs: 0,
        ls: 0,
        game_name: Some(name.into()),
        tag_line: Some(tag.into()),
    }
}

/// ME and HIDDEN played a match on Split together (so HIDDEN's name as the
/// match showed it is "remembered"); returns the match's start.
async fn store_split_match(st: &AppState) -> i64 {
    let start = now_ms() - 120_000;
    let m = Match {
        match_id: "m-split".into(),
        map_id: SPLIT_PATH.into(),
        queue_id: "unrated".into(),
        start_ms: start,
        winning_team: None,
        is_ranked: false,
        rounds_played: None,
    };
    let players = vec![
        player_row(ME, "Blue", "Me", "TAG"),
        player_row(HIDDEN, "Red", "SecretName", "HID"),
    ];
    st.db(move |repo| repo.upsert_match(&m, &[], &players))
        .await
        .unwrap();
    start
}

/// `puuid`'s DB bundle, as the poll loop loads it for a slot.
async fn slot_for(st: &AppState, puuid: &str, incognito: bool) -> Slot {
    let p = puuid.to_string();
    let db = st
        .db(move |repo| load_player_db(repo, Some(ME), &p))
        .await
        .unwrap();
    Slot {
        roster: roster(puuid, incognito),
        db,
    }
}

pub(crate) fn card_json(st: &AppState, puuid: &str) -> Value {
    let state = st.live_state();
    let card = state
        .players
        .iter()
        .find(|c| c.puuid == puuid)
        .expect("card on screen");
    serde_json::to_value(card).unwrap()
}

#[tokio::test]
async fn self_card_is_self_and_others_not() {
    let h = harness().await;
    {
        let mut live = lock(&h.st.live);
        live.stage = Stage::Ingame;
        live.slots = vec![
            Slot {
                roster: roster(ME, false),
                db: PlayerDb::default(),
            },
            Slot {
                roster: roster(OTHER, false),
                db: PlayerDb::default(),
            },
            Slot {
                roster: roster(HIDDEN, true),
                db: PlayerDb::default(),
            },
        ];
    }
    h.st.publish();
    assert_eq!(card_json(&h.st, ME)["isSelf"], json!(true));
    assert_eq!(card_json(&h.st, OTHER)["isSelf"], json!(false));
    assert_eq!(card_json(&h.st, HIDDEN)["isSelf"], json!(false));
}

#[tokio::test]
async fn remembered_name_source_is_map_uuid() {
    let h = harness().await;
    let start = store_split_match(&h.st).await;
    {
        let slots = vec![
            slot_for(&h.st, ME, false).await,
            slot_for(&h.st, HIDDEN, true).await,
        ];
        let mut live = lock(&h.st.live);
        live.stage = Stage::Ingame;
        live.slots = slots;
    }

    // The asset tables aren't loaded: the source map is unknown, and the
    // Riot path never stands in for it.
    h.st.publish();
    let name = card_json(&h.st, HIDDEN)["name"].clone();
    assert_eq!(name["kind"], "remembered");
    assert_eq!(name["name"], "SecretName");
    assert_eq!(name["fromMapId"], Value::Null);
    assert_eq!(name["fromStartMs"], json!(start));
    assert!(!serde_json::to_string(&h.st.live_state())
        .unwrap()
        .contains("/Game/"));

    // With the tables: Split's valorant-api uuid.
    mount_asset_tables(&h.assets_server).await;
    h.st.ensure_tables(false).await.expect("tables load");
    h.st.publish();
    let name = card_json(&h.st, HIDDEN)["name"].clone();
    assert_eq!(name["fromMapId"], SPLIT_UUID);
    assert_eq!(name["fromStartMs"], json!(start));
    assert!(!serde_json::to_string(&h.st.live_state())
        .unwrap()
        .contains("/Game/"));
}

#[tokio::test]
async fn top3_remembered_source_is_map_uuid() {
    let h = harness().await;
    let start = store_split_match(&h.st).await;
    // HIDDEN and STRANGER are teammates in a competitive match of the
    // current act that ME did not play.
    store_foreign_match(&h.st).await;
    mount_asset_tables(&h.assets_server).await;
    h.st.ensure_tables(false).await.expect("tables load");
    {
        let slots = vec![
            slot_for(&h.st, ME, false).await,
            slot_for(&h.st, HIDDEN, true).await,
            slot_for(&h.st, STRANGER, false).await,
        ];
        let mut live = lock(&h.st.live);
        live.stage = Stage::Ingame;
        live.slots = slots;
    }
    h.st.publish();

    // STRANGER's Top 3: HIDDEN, hidden on screen and remembered from the
    // user's own Split match.
    let d = crate::details::build(&h.st, STRANGER).await.unwrap();
    assert_eq!(d.top_teammates.len(), 1);
    assert_eq!(d.top_teammates[0].puuid, HIDDEN);
    assert_eq!(
        d.top_teammates[0].name,
        DisplayName::Remembered {
            name: "SecretName".into(),
            tag: "HID".into(),
            from_map_id: Some(SPLIT_UUID.into()),
            from_start_ms: Some(start),
        }
    );
    let json = serde_json::to_string(&d).unwrap();
    assert!(!json.contains("/Game/"), "no Riot map path in {json}");
    assert!(!json.contains("LEAK"), "the foreign match's name stays out");
}

#[tokio::test]
async fn player_details_card_carries_act_wins_games() {
    let h = harness().await;
    // OTHER is not on screen: a `players` row and a stored MMR snapshot are
    // all the DB has. 34 wins without placements, 39 with them, in 61 games.
    let resp: PlayerMmrResponse = serde_json::from_value(json!({
        "Subject": OTHER,
        "QueueSkills": {"competitive": {"SeasonalInfoBySeasonID": {ACT: {
            "SeasonID": ACT, "CompetitiveTier": 15, "RankedRating": 42,
            "NumberOfWins": 34, "NumberOfWinsWithPlacements": 39, "NumberOfGames": 61,
            "WinsByTier": {"15": 20}
        }}}}
    }))
    .unwrap();
    let snapshot = MmrSnapshot {
        puuid: OTHER.into(),
        json: trim_mmr(&resp).to_string(),
        last_comp_match_id: Some("m-old".into()),
        fetched_ms: 1,
    };
    let now = now_ms();
    h.st.db(move |repo| {
        save_player(
            repo.conn(),
            &PlayerUpdate {
                puuid: OTHER.into(),
                name: Some(("Other".into(), "OTH".into())),
                account_level: 55,
                card_id: "card".into(),
            },
            now,
        )?;
        repo.save_mmr_snapshot(&snapshot)
    })
    .await
    .unwrap();
    assert!(lock(&h.st.live).slots.is_empty());

    let d = crate::details::build(&h.st, OTHER).await.unwrap();
    let card = serde_json::to_value(&d.card).unwrap();
    assert_eq!(card["actWins"], json!(39));
    assert_eq!(card["actGames"], json!(61));
    let rate = card["actWinrate"].as_f64().unwrap();
    assert!((rate - 39.0 / 61.0).abs() < 1e-3, "actWinrate {rate}");
    assert_eq!(card["isSelf"], json!(false));
}

// ---------------------------------------------------------------------
// Task 14 fix round 1: the user's own level survives HideAccountLevel
// ---------------------------------------------------------------------

/// The user's own roster row while `HideAccountLevel` is on (level 87), saved
/// the way `apply_roster` saves it: the level must reach the DB (R100).
async fn save_hidden_self(st: &AppState) {
    let mut me = roster(ME, false);
    me.hide_account_level = true;
    me.account_level = 87;
    let now = now_ms();
    st.db(move |repo| save_player(repo.conn(), &me.player_update(ME), now))
        .await
        .unwrap();
}

#[tokio::test]
async fn own_hidden_level_shows_on_the_closed_card() {
    let h = harness().await;
    save_hidden_self(&h.st).await;
    let db =
        h.st.db(|repo| load_player_db(repo, Some(ME), ME))
            .await
            .unwrap();
    assert_eq!(db.row.as_ref().unwrap().account_level, 87);
    {
        let mut live = lock(&h.st.live);
        live.stage = Stage::Closed;
        live.slots = vec![crate::state::closed_self_slot(ME, db).unwrap()];
    }
    h.st.publish();
    let card = card_json(&h.st, ME);
    assert_eq!(card["isSelf"], json!(true));
    assert_eq!(
        card["level"],
        json!(87),
        "the Closed stage shows the user's own level"
    );
}

#[tokio::test]
async fn own_hidden_level_shows_in_own_off_screen_details() {
    let h = harness().await;
    save_hidden_self(&h.st).await;
    assert!(lock(&h.st.live).slots.is_empty());
    let d = crate::details::build(&h.st, ME).await.unwrap();
    let card = serde_json::to_value(&d.card).unwrap();
    assert_eq!(card["isSelf"], json!(true));
    assert_eq!(card["level"], json!(87));
}

#[tokio::test]
async fn another_players_hidden_level_is_never_stored() {
    let h = harness().await;
    let mut other = roster(OTHER, false);
    other.hide_account_level = true;
    other.account_level = 120;
    let now = now_ms();
    let db =
        h.st.db(move |repo| {
            save_player(repo.conn(), &other.player_update(ME), now)?;
            load_player_db(repo, Some(ME), OTHER)
        })
        .await
        .unwrap();
    assert_eq!(db.row.unwrap().account_level, 0);
}

// ---------------------------------------------------------------------
// Task 14: a failed match history never feeds party marks
// ---------------------------------------------------------------------

/// OTHER and MATE played their last two matches together in an earlier
/// session: both are in the DB with the same team, and both histories are
/// indexed. Party detection reads exactly this.
async fn store_earlier_shared_matches(st: &AppState) {
    let matches = [("s1", 5_000), ("s2", 6_000)];
    st.db(move |repo| {
        for (id, start_ms) in matches {
            let m = Match {
                match_id: id.into(),
                map_id: "/Game/Maps/Ascent/Ascent".into(),
                queue_id: "unrated".into(),
                start_ms,
                winning_team: None,
                is_ranked: false,
                rounds_played: None,
            };
            let players = [
                player_row(OTHER, "Red", "Other", "OTH"),
                player_row(MATE, "Red", "Mate", "MAT"),
            ];
            repo.upsert_match(&m, &[], &players)?;
            for puuid in [OTHER, MATE] {
                repo.index_history(
                    puuid,
                    &[HistoryEntry {
                        match_id: id.into(),
                        queue_id: "unrated".into(),
                        start_ms,
                    }],
                )?;
            }
        }
        Ok(())
    })
    .await
    .unwrap();
}

fn history_of(puuid: &str) -> Value {
    json!({"Subject": puuid, "History": [
        {"MatchID": "s2", "GameStartTime": 6_000, "QueueID": "unrated"},
        {"MatchID": "s1", "GameStartTime": 5_000, "QueueID": "unrated"}
    ]})
}

/// Syncs ME, OTHER and MATE in an In Game roster where OTHER and MATE
/// shared their last two matches. MATE's history always answers; OTHER's
/// answers only when `other_history_ok` (otherwise it stays the harness's 500).
async fn run_party_scenario(other_history_ok: bool) -> Harness {
    let h = harness().await;
    store_earlier_shared_matches(&h.st).await;
    get(
        &h.server,
        &format!("/match-history/v1/history/{MATE}"),
        history_of(MATE),
    )
    .await;
    if other_history_ok {
        // Beats the harness's 500 for OTHER's history (lower number = first).
        Mock::given(method("GET"))
            .and(path(format!("/match-history/v1/history/{OTHER}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(history_of(OTHER)))
            .with_priority(1)
            .mount(&h.server)
            .await;
    }
    {
        let slots = vec![
            slot_for(&h.st, ME, false).await,
            slot_for(&h.st, OTHER, false).await,
            slot_for(&h.st, MATE, false).await,
        ];
        let mut live = lock(&h.st.live);
        live.stage = Stage::Ingame;
        live.slots = slots;
    }
    fetch_self_name(&h.st, &h.session).await.unwrap();
    let id = lock(&h.st.live).try_start_worker().expect("work pending");
    run_worker(h.st.clone(), h.session.clone(), id).await;
    h
}

#[tokio::test]
async fn failed_history_never_feeds_party_marks() {
    // OTHER's match history fails (500): the history stored from an earlier
    // session must not link OTHER to MATE, so neither card gets a stripe.
    let h = run_party_scenario(false).await;
    {
        let live = lock(&h.st.live);
        assert!(live.synced.contains(OTHER), "the sync itself ran");
        assert!(live.synced.contains(MATE));
        assert!(!live.party_ready.contains(OTHER), "no history, no party");
        assert!(live.party_ready.contains(MATE));
    }
    assert_eq!(card_json(&h.st, OTHER)["party"], Value::Null);
    assert_eq!(card_json(&h.st, MATE)["party"], Value::Null);

    // Control: with the history answering, the same DB links the two.
    let h = run_party_scenario(true).await;
    assert!(lock(&h.st.live).party_ready.contains(OTHER));
    let (other, mate) = (
        card_json(&h.st, OTHER)["party"].clone(),
        card_json(&h.st, MATE)["party"].clone(),
    );
    assert!(other.is_object(), "OTHER is in a party: {other}");
    assert_eq!(other["group"], mate["group"], "same party");
}
