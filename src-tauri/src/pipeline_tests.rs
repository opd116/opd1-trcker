//! The real sync pipeline (`sync.rs` + `state.rs` + `store.rs`) against a
//! mock Riot client (wiremock) and an in-memory DB: request counts for a
//! cold vs a warm DB ("sync, don't refetch"), hidden players never sent
//! to name-service, and failed calls settling to `null` instead of a
//! skeleton. The poll loop's timing itself isn't tested here (by plan);
//! its decisions are, in `roster.rs`.

use std::path::Path;
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

use opd1_core::db::open_db;
use opd1_core::db::repo::Repo;
use opd1_core::model::{DisplayName, Field, LiveState, PlayerDetails, Team};
use opd1_core::presence::Stage;
use opd1_core::stats::Act;
use opd1_riot::assets::AssetCache;
use opd1_riot::session::{ConnectOverrides, RiotSession};

use crate::roster::RosterPlayer;
use crate::state::{lock, now_ms, AppState, EventSink, Slot};
use crate::store::PlayerDb;
use crate::sync::{fetch_self_name, run_worker, sync_player};

const ME: &str = "aaaaaaaa-0000-0000-0000-000000000001";
const HIDDEN: &str = "bbbbbbbb-0000-0000-0000-000000000002";
const OTHER: &str = "cccccccc-0000-0000-0000-000000000003";
const ACT: &str = "act-current";

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
            // Post-match details carry a hidden player's real name — it gets
            // stored, but must never reach their live card.
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

struct Harness {
    server: MockServer,
    _dir: tempfile::TempDir,
    session: Arc<RiotSession>,
    st: Arc<AppState>,
    rec: Arc<Recorder>,
}

async fn harness() -> Harness {
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
    // valorant-api unreachable: map ids resolve to None, never to a path.
    let assets = AssetCache::with_base(dir.path().join("assets"), "http://127.0.0.1:1".into());
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

fn roster(puuid: &str, incognito: bool) -> RosterPlayer {
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

#[tokio::test]
async fn worker_names_only_visible_players_and_settles_failures() {
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
                roster: roster(HIDDEN, true),
                db: PlayerDb::default(),
            },
            Slot {
                roster: roster(OTHER, false),
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
        !body.contains(HIDDEN),
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

    let hidden = &last.players[1];
    assert_eq!(
        hidden.name,
        Field::Value(DisplayName::HiddenAgent {
            agent_id: "agent".into()
        })
    );
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
    // …while the competitive match they shared with ME (already in the DB)
    // still fills their last-5 — every non-name field stays visible.
    assert!(matches!(hidden.last5, Field::Value(ref v) if v.len() == 1));
    assert_eq!(hidden.kd, Field::Value(5.0 / 15.0));
    // The real name sits in the DB (from ME's match details) but never
    // reaches the live payload.
    assert!(!serde_json::to_string(last).unwrap().contains("SecretName"));

    // Player Details for ME: recent from the DB; no encounters with self.
    let d = crate::details::build(&h.st, ME).await.unwrap();
    assert_eq!(d.recent.len(), 1);
    assert_eq!(d.recent[0].rr_earned, Some(21));
    assert_eq!(
        d.recent[0].map_id, "",
        "unknown map → empty id, never the Riot path"
    );
    assert!(d.encounters.is_empty());
    // Details for the hidden enemy: encounters exist (m1, m2), and their
    // details card stays hidden.
    let d = crate::details::build(&h.st, HIDDEN).await.unwrap();
    assert_eq!(d.encounters.len(), 2);
    assert!(d.encounters.iter().all(|e| !e.with));
    assert!(!serde_json::to_string(&d).unwrap().contains("SecretName"));
}
