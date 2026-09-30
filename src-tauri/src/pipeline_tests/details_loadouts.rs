//! Task 15's pipeline tests: the `match_details` command, Player Details'
//! extended encounters and hero stats, and the loadout / skin-name flow
//! (pre-game polling, R83/R97 skin names, the user's own loadout, R90).
//! A child of `pipeline_tests`, so it shares that module's harness (mock
//! Riot, mock valorant-api, in-memory DB) and helpers through `super::*`.

use super::*;

// ---------------------------------------------------------------------
// Task 15: match_details command, extended encounters, hero stats
// ---------------------------------------------------------------------

/// The `name` of one scoreboard row.
fn row_name(view: &opd1_core::model::MatchDetailsView, puuid: &str) -> DisplayName {
    view.teams
        .iter()
        .flat_map(|t| t.players.iter())
        .find(|p| p.puuid == puuid)
        .unwrap_or_else(|| panic!("{puuid} not on the scoreboard"))
        .name
        .clone()
}

#[tokio::test]
async fn match_details_reads_db_and_fetches_unknown_once() {
    let h = harness().await;
    h.st.set_session(Some(h.session.clone()));
    let path = "/match-details/v1/matches/m1";

    // Unknown to the DB: exactly one match-details request, then stored.
    let view = crate::match_details::load(&h.st, "m1", HIDDEN)
        .await
        .expect("fetched and stored");
    assert_eq!(count(&h.server, "GET", path).await, 1);
    assert_eq!(view.teams.len(), 2);
    assert_eq!(view.match_id, "m1");
    assert_eq!(view.queue_id, "competitive");
    assert_eq!(
        view.map_id, "",
        "asset tables not loaded: no uuid, never the path"
    );
    assert!(!serde_json::to_string(&view).unwrap().contains("/Game/"));
    // ME played it: their own view, the user's team first.
    assert_eq!(view.perspective, opd1_core::model::Perspective::SelfView);
    assert_eq!(view.teams[0].team_id, "Blue");
    let me = &view.teams[0].players[0];
    assert!(me.is_self && !me.is_focus && me.puuid == ME);
    let hidden = &view.teams[1].players[0];
    assert!(hidden.is_focus && !hidden.is_self && hidden.puuid == HIDDEN);
    // Post-match names show as match-details gives them (CLAUDE.md).
    assert_eq!(
        row_name(&view, HIDDEN),
        DisplayName::Visible {
            name: "SecretName".into(),
            tag: "HID".into()
        }
    );

    // Known now: served from the DB, no new request.
    let again = crate::match_details::load(&h.st, "m1", HIDDEN)
        .await
        .unwrap();
    assert_eq!(count(&h.server, "GET", path).await, 1);
    assert_eq!(again, view);

    // A match nobody has: one attempt, then "unknown match".
    assert_eq!(
        crate::match_details::load(&h.st, "nope", HIDDEN).await,
        Err("unknown match".to_string())
    );

    // Closed (no session): only what the DB has, and no request at all.
    h.st.set_session(None);
    assert_eq!(
        crate::match_details::load(&h.st, "m2", HIDDEN).await,
        Err("unknown match".to_string())
    );
    assert_eq!(
        count(&h.server, "GET", "/match-details/v1/matches/m2").await,
        0
    );
}

#[tokio::test]
async fn match_details_hides_live_incognito_rows() {
    let h = harness().await;
    store_split_match(&h.st).await; // ME and HIDDEN, "m-split"
    store_foreign_match(&h.st).await; // HIDDEN and STRANGER, without ME
    {
        // HIDDEN and STRANGER are incognito on the live screen right now.
        let slots = vec![
            slot_for(&h.st, ME, false).await,
            slot_for(&h.st, HIDDEN, true).await,
            slot_for(&h.st, STRANGER, true).await,
        ];
        let mut live = lock(&h.st.live);
        live.stage = Stage::Ingame;
        live.slots = slots;
    }

    // A match ME played: HIDDEN follows their card, which shows the name
    // ME learned in this very match, in the remembered style (R93).
    let view = crate::match_details::load(&h.st, "m-split", ME)
        .await
        .unwrap();
    assert!(
        matches!(
            row_name(&view, HIDDEN),
            DisplayName::Remembered { ref name, .. } if name == "SecretName"
        ),
        "{:?}",
        row_name(&view, HIDDEN)
    );

    // A match ME did not play: HIDDEN is remembered from ME's own match;
    // STRANGER (never met ME) shows no name, and the leaked names stay out.
    let view = crate::match_details::load(&h.st, "m-foreign", HIDDEN)
        .await
        .unwrap();
    assert_eq!(view.perspective, opd1_core::model::Perspective::Focus);
    assert!(matches!(
        row_name(&view, HIDDEN),
        DisplayName::Remembered { ref name, .. } if name == "SecretName"
    ));
    assert_eq!(row_name(&view, STRANGER), DisplayName::HiddenPlayer);
    let text = serde_json::to_string(&view).unwrap();
    assert!(
        !text.contains("LeakedName") && !text.contains("LEAK"),
        "the foreign match's stored names never reach the wire: {text}"
    );
    assert!(!text.contains("/Game/"));
}

/// ME (Jett, tier 21 during the match) lost 10-13 against OTHER (Reyna,
/// whose tier only the RR row's tier after the match knows) on Split, and
/// they later won together on the same team. Returns nothing: the rows are
/// in the DB.
async fn store_encounters(st: &AppState) -> (i64, i64) {
    let (against_start, with_start) = (now_ms() - 7_200_000, now_ms() - 3_600_000);
    st.db(move |repo| {
        let stats = |puuid: &str, team: &str, agent: &str, name: &str, tag: &str| MatchPlayer {
            agent_id: Some(agent.into()),
            ..player_row(puuid, team, name, tag)
        };
        let m = |id: &str, start_ms: i64, winner: &str, length: Option<i64>| Match {
            match_id: id.into(),
            map_id: SPLIT_PATH.into(),
            queue_id: "competitive".into(),
            start_ms,
            winning_team: Some(winner.into()),
            is_ranked: true,
            rounds_played: Some(23),
            game_length_ms: length,
        };
        let team = |id: &str, won: bool, rounds: i64| MatchTeam {
            team_id: id.into(),
            won,
            rounds_won: Some(rounds),
            rounds_won_h1: None,
        };
        let mut me = stats(ME, "Blue", "agent-jett", "Me", "TAG");
        me.competitive_tier = Some(21);
        repo.upsert_match(
            &m("enc-against", against_start, "Red", Some(2_460_000)),
            &[team("Blue", false, 10), team("Red", true, 13)],
            &[me, stats(OTHER, "Red", "agent-reyna", "Other", "OTH")],
        )?;
        repo.upsert_comp_updates(
            OTHER,
            &[CompUpdateRow {
                match_id: "enc-against".into(),
                season_id: ACT.into(),
                tier_after: 24,
                tier_before: Some(23),
                rr_after: 10,
                rr_earned: 20,
                map_id: None,
                start_ms: against_start,
            }],
        )?;
        known_player(repo, OTHER);
        repo.upsert_match(
            &m("enc-with", with_start, "Blue", None),
            &[team("Blue", true, 13), team("Red", false, 5)],
            &[
                stats(ME, "Blue", "agent-sova", "Me", "TAG"),
                stats(OTHER, "Blue", "agent-omen", "Other", "OTH"),
            ],
        )
    })
    .await
    .unwrap();
    (against_start, with_start)
}

#[tokio::test]
async fn player_details_encounter_rows_are_extended() {
    let h = harness().await;
    mount_asset_tables(&h.assets_server).await;
    h.st.ensure_tables(false).await.expect("tables load");
    let (against_start, with_start) = store_encounters(&h.st).await;

    let d = crate::details::build(&h.st, OTHER).await.unwrap();
    let json = serde_json::to_value(&d).unwrap();
    let rows = json["encounters"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    // Newest first; every key of the wire type, `selfWon` gone.
    assert_eq!(
        rows[1],
        json!({
            "matchId": "enc-against", "mapId": SPLIT_UUID, "startMs": against_start,
            "with": false, "result": "loss", "roundsWon": 10, "roundsLost": 13,
            "durationMs": 2_460_000,
            "themAgentId": "agent-reyna", "themTier": 24,
            "selfAgentId": "agent-jett", "selfTier": 21
        })
    );
    assert_eq!(
        rows[0],
        json!({
            "matchId": "enc-with", "mapId": SPLIT_UUID, "startMs": with_start,
            "with": true, "result": "win", "roundsWon": 13, "roundsLost": 5,
            "durationMs": null,
            "themAgentId": "agent-omen", "themTier": null,
            "selfAgentId": "agent-sova", "selfTier": null
        })
    );
    assert!(rows.iter().all(|r| r.get("selfWon").is_none()));
    assert!(!json.to_string().contains("/Game/"));
}

/// A `players` row, so Details can build an off-screen card for `puuid`.
fn known_player(repo: &Repo, puuid: &str) {
    save_player(
        repo.conn(),
        &PlayerUpdate {
            puuid: puuid.into(),
            name: Some(("Someone".into(), "TAG".into())),
            account_level: 50,
            card_id: "card".into(),
        },
        now_ms(),
    )
    .unwrap();
}

/// `start_ms` of the i-th newest act match (1 = an hour ago).
fn act_match_start(i: i64) -> i64 {
    now_ms() - i * 3_600_000
}

/// Stores `who`'s competitive match `i`: `kills` kills over 2 deaths, 1
/// headshot of 4 shots, or no stats row at all when `kills` is `None`.
fn store_act_match(repo: &Repo, who: &str, i: i64, start_ms: i64, kills: Option<i64>) {
    let m = Match {
        match_id: format!("act-{i}"),
        map_id: SPLIT_PATH.into(),
        queue_id: "competitive".into(),
        start_ms,
        winning_team: Some("Blue".into()),
        is_ranked: true,
        rounds_played: kills.map(|_| 20),
        game_length_ms: None,
    };
    let mut p = player_row(who, "Blue", "Someone", "TAG");
    if let Some(kills) = kills {
        (p.kills, p.deaths, p.rounds, p.score) = (kills, 2, Some(20), 4_000);
        (p.hs, p.bs, p.ls) = (1, 3, 0);
    }
    repo.upsert_match(&m, &[], &[p]).unwrap();
}

#[tokio::test]
async fn player_details_hero_stats_cover_the_acts_recent_matches() {
    let h = harness().await;
    // OTHER's current-act competitive matches: 1 (newest) .. 22. Match 3
    // has no stats row; 21 and 22 lie beyond the 20 newest and carry absurd
    // kills; an older match (before the act began) does too.
    h.st.db(|repo| {
        known_player(repo, OTHER);
        for i in 1..=22 {
            let kills = match i {
                3 => None,
                21 | 22 => Some(1_000),
                _ => Some(i),
            };
            store_act_match(repo, OTHER, i, act_match_start(i), kills);
        }
        store_act_match(repo, OTHER, 99, now_ms() - 40 * 86_400_000, Some(999));
        Ok(())
    })
    .await
    .unwrap();

    let d = crate::details::build(&h.st, OTHER).await.unwrap();
    // The 20 newest act matches, minus the one without stats: 19 used,
    // 207 kills (1..=20 without 3) over 19 * 2 deaths, 19 of 76 shots.
    assert_eq!(d.hero_matches, 19);
    assert_eq!(d.hero_kd, Some(207.0 / 38.0));
    assert_eq!(d.hero_hs, Some(0.25));
    let json = serde_json::to_value(&d).unwrap();
    assert_eq!(json["heroMatches"], json!(19));
    assert!((json["heroKd"].as_f64().unwrap() - 207.0 / 38.0).abs() < 1e-5);
    assert_eq!(json["heroHs"], json!(0.25));
    // R102 on the same payload: the recent row without stats is null.
    let blank = json["recent"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["matchId"] == "act-3")
        .expect("act-3 is among the 8 newest");
    for key in ["kills", "deaths", "assists", "kd", "acs", "hsPct"] {
        assert_eq!(blank[key], Value::Null, "{key}");
    }
}

#[tokio::test]
async fn player_details_hero_stats_are_null_without_act_matches() {
    let h = harness().await;
    // STRANGER only has a match from before the act began.
    h.st.db(|repo| {
        known_player(repo, STRANGER);
        store_act_match(repo, STRANGER, 1, now_ms() - 40 * 86_400_000, Some(30));
        Ok(())
    })
    .await
    .unwrap();
    let d = crate::details::build(&h.st, STRANGER).await.unwrap();
    assert_eq!((d.hero_kd, d.hero_hs, d.hero_matches), (None, None, 0));
    let json = serde_json::to_value(&d).unwrap();
    assert_eq!(json["heroKd"], Value::Null);
    assert_eq!(json["heroHs"], Value::Null);
    assert_eq!(json["heroMatches"], json!(0));
    // The pre-act match still shows in Recent (any act).
    assert_eq!(d.recent.len(), 1);
}

// ---------------------------------------------------------------------
// Task 15: loadouts — pre-game polling, skin names (R83, R97), own loadout (R90)
// ---------------------------------------------------------------------

use crate::poll::Poller;
use crate::roster::{EquippedSkin, RawLoadout};

/// A fixture from `opd1-riot`'s test fixtures.
fn riot_fixture(name: &str) -> String {
    let path = format!(
        "{}/../crates/riot/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("missing fixture {name}: {e}"))
}

/// The Reaver Vandal as `/v1/weapons/skins/{uuid}` answers, for `uuid`
/// with `chroma` as its red variant.
fn skin_body(uuid: &str, chroma: &str) -> Value {
    let mut body: Value =
        serde_json::from_str(&riot_fixture("weapon_skin_reaver_vandal.json")).unwrap();
    body["data"]["uuid"] = json!(uuid);
    body["data"]["chromas"] = json!([{
        "uuid": chroma, "displayName": "Reaver Vandal Level 4\r\n(Variant 1 Red)"
    }]);
    body
}

async fn mount_skin(server: &MockServer, uuid: &str, chroma: &str) {
    get(
        server,
        &format!("/v1/weapons/skins/{uuid}"),
        skin_body(uuid, chroma),
    )
    .await;
}

async fn mount_failing_skin(server: &MockServer, uuid: &str) {
    Mock::given(method("GET"))
        .and(path(format!("/v1/weapons/skins/{uuid}")))
        .respond_with(ResponseTemplate::new(500))
        .mount(server)
        .await;
}

async fn skin_requests(h: &Harness) -> usize {
    h.assets_server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.url.path().starts_with("/v1/weapons/skins/"))
        .count()
}

fn equipped(skin: &str, chroma: &str) -> Option<EquippedSkin> {
    Some(EquippedSkin {
        skin_id: skin.into(),
        chroma_id: chroma.into(),
    })
}

/// OTHER equips the Reaver Vandal (red) and nothing else readable.
fn reaver_loadout() -> RawLoadout {
    RawLoadout {
        vandal: equipped(REAVER, REAVER_RED),
        ..RawLoadout::default()
    }
}

/// `puuid`'s `details` events after the first `seen`, waiting (up to 5s)
/// for one that satisfies `ok`.
async fn details_event_after(
    rec: &Recorder,
    seen: usize,
    puuid: &str,
    ok: impl Fn(&Value) -> bool,
) -> Value {
    for _ in 0..100 {
        let found = rec
            .details
            .lock()
            .unwrap()
            .iter()
            .skip(seen)
            .filter(|d| d.card.puuid == puuid)
            .map(|d| serde_json::to_value(d).unwrap())
            .find(|v| ok(v));
        if let Some(found) = found {
            return found;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("no matching details event for {puuid}");
}

fn details_events(rec: &Recorder) -> usize {
    rec.details.lock().unwrap().len()
}

fn loadout_of(details: &PlayerDetails) -> Value {
    serde_json::to_value(details).unwrap()["loadout"].clone()
}

/// The pre-game endpoints of one Agent Select `pg-1` with ME, OTHER and
/// MATE as allies. The first `/loadouts` answer is `LoadoutsValid: false`
/// (Riot nulls it until the picks are in), the following ones are the
/// capture-based valid response for the three allies.
async fn mock_pregame(server: &MockServer) {
    let player = |puuid: &str| {
        json!({"Subject": puuid, "CharacterID": "", "CharacterSelectionState": "",
               "PlayerIdentity": {"Incognito": false, "HideAccountLevel": false,
                                  "AccountLevel": 10, "PlayerCardID": "card"}})
    };
    let team = json!({"TeamID": "Blue", "Players": [player(ME), player(OTHER), player(MATE)]});
    get(
        server,
        &format!("/pregame/v1/players/{ME}"),
        json!({"Subject": ME, "MatchID": "pg-1"}),
    )
    .await;
    get(
        server,
        "/pregame/v1/matches/pg-1",
        json!({"MapID": SPLIT_PATH, "Teams": [team.clone()], "AllyTeam": team, "EnemyTeam": null}),
    )
    .await;

    let mut valid: Value = serde_json::from_str(&riot_fixture("pregame_loadouts.json")).unwrap();
    let entries = valid["Loadouts"].as_array_mut().unwrap();
    let third = entries[0].clone();
    entries.truncate(2);
    entries.push(third);
    for (entry, puuid) in entries.iter_mut().zip([ME, OTHER, MATE]) {
        entry["Subject"] = json!(puuid);
    }
    let loadouts = "/pregame/v1/matches/pg-1/loadouts";
    Mock::given(method("GET"))
        .and(path(loadouts))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"Loadouts": [], "LoadoutsValid": false})),
        )
        .up_to_n_times(1)
        .mount(server)
        .await;
    get(server, loadouts, valid).await;
}

#[tokio::test]
async fn pregame_loadouts_polled_once_then_stops() {
    let loadouts = "/pregame/v1/matches/pg-1/loadouts";
    let h = harness().await;
    mock_pregame(&h.server).await;
    let mut poller = Poller::new(h.st.clone());
    poller.fetch_pregame(&h.session, ME).await.unwrap();
    assert_eq!(count(&h.server, "GET", loadouts).await, 0);

    // Re-poll 1: Riot's answer is not valid yet — asked, nothing read.
    poller.pregame_repoll(&h.session).await.unwrap();
    assert_eq!(count(&h.server, "GET", loadouts).await, 1);
    assert!(lock(&h.st.live).loadouts.is_empty());

    // Re-poll 2: valid — the three allies' skins are in.
    poller.pregame_repoll(&h.session).await.unwrap();
    assert_eq!(count(&h.server, "GET", loadouts).await, 2);
    {
        let live = lock(&h.st.live);
        let mut who: Vec<&str> = live.loadouts.keys().map(String::as_str).collect();
        who.sort_unstable();
        assert_eq!(who, [ME, OTHER, MATE]);
        assert_eq!(
            live.loadouts[ME].vandal,
            equipped(REAVER, REAVER_RED),
            "the capture's first entry, lower-case ids"
        );
    }

    // Re-polls 3 and 4: a valid answer was seen, so no more calls.
    poller.pregame_repoll(&h.session).await.unwrap();
    poller.pregame_repoll(&h.session).await.unwrap();
    assert_eq!(
        count(&h.server, "GET", loadouts).await,
        2,
        "exactly two GETs over four re-polls"
    );

    // A second Agent Select where every ally already has a loadout asks for
    // nothing at all.
    let h = harness().await;
    mock_pregame(&h.server).await;
    lock(&h.st.live).loadouts = [ME, OTHER, MATE]
        .into_iter()
        .map(|p| (p.to_string(), RawLoadout::default()))
        .collect();
    let mut poller = Poller::new(h.st.clone());
    poller.fetch_pregame(&h.session, ME).await.unwrap();
    for _ in 0..4 {
        poller.pregame_repoll(&h.session).await.unwrap();
    }
    assert_eq!(count(&h.server, "GET", loadouts).await, 0);
}

/// Stage `menus` with `puuid` on screen (a `players` row is enough for its
/// Details card).
async fn lobby_with(h: &Harness, puuid: &'static str) {
    h.st.db(move |repo| {
        known_player(repo, puuid);
        Ok(())
    })
    .await
    .unwrap();
    lock(&h.st.live).stage = Stage::Menus;
}

#[tokio::test]
async fn player_details_resolves_skins_without_riot_sync() {
    let h = harness().await;
    lobby_with(&h, OTHER).await;
    // With a session, Details also runs the (120s-gated) Riot sync: its
    // claim must not decide whether the skin names resolve.
    h.st.set_session(Some(h.session.clone()));
    mount_skin(&h.assets_server, REAVER, REAVER_RED).await;
    let comp = format!("/mmr/v1/players/{OTHER}/competitiveupdates");

    // 1. Opened before any loadout arrived: all four weapons are `null`
    //    (nothing known), never absent, and no name is fetched.
    let first = crate::commands::player_details_for(&h.st, OTHER)
        .await
        .unwrap();
    let null = Value::Null;
    let loadout = loadout_of(&first);
    for weapon in ["vandal", "phantom", "operator", "sheriff"] {
        assert_eq!(loadout[weapon], null, "{weapon}: {loadout}");
    }
    assert_eq!(skin_requests(&h).await, 0);
    // The Riot sync claimed OTHER for the next 120 s.
    assert!(!h.st.claim_details_sync(OTHER));
    let seen = details_events(&h.rec);

    // 2. The loadouts arrive. Reopening inside the 120 s (the sync claim
    //    refuses) still resolves the names: the Vandal is resolving (key
    //    absent), the weapons nobody equipped are `null`.
    lock(&h.st.live)
        .loadouts
        .insert(OTHER.into(), reaver_loadout());
    let second = crate::commands::player_details_for(&h.st, OTHER)
        .await
        .unwrap();
    let loadout = loadout_of(&second);
    assert!(
        loadout.get("vandal").is_none(),
        "resolving: the key is absent: {loadout}"
    );
    for weapon in ["phantom", "operator", "sheriff"] {
        assert_eq!(loadout[weapon], null, "{weapon} is not equipped");
    }
    // ... and the details event carries the named skin.
    let event = details_event_after(&h.rec, seen, OTHER, |d| {
        d["loadout"].get("vandal").is_some_and(|v| v.is_object())
    })
    .await;
    assert_eq!(
        event["loadout"]["vandal"],
        json!({
            "chromaId": REAVER_RED, "skinName": "Reaver",
            "chromaName": "Red", "tierId": TIER_PREMIUM
        })
    );
    assert_eq!(skin_requests(&h).await, 1);
    assert!(!h.st.skins_resolving(OTHER), "the attempt is over");
    assert_eq!(
        count(&h.server, "GET", &comp).await,
        1,
        "the second open made no Riot sync"
    );

    // 3. The next open has the skin in memory: answered directly, and no
    //    new skin request.
    let third = crate::commands::player_details_for(&h.st, OTHER)
        .await
        .unwrap();
    assert_eq!(loadout_of(&third)["vandal"], event["loadout"]["vandal"]);
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert_eq!(skin_requests(&h).await, 1);
}

/// `core-game/.../loadouts` for `puuid`: the Reaver Vandal in red from the
/// capture's first Pre-game entry (its only item kept, so one skin name is
/// to be fetched), wrapped the way core-game nests it (`Loadout.Subject`
/// and `Loadout.Items`).
async fn mock_coregame_loadouts(server: &MockServer, match_id: &str, puuid: &str) {
    let capture: Value = serde_json::from_str(&riot_fixture("pregame_loadouts.json")).unwrap();
    let vandal = crate::roster::VANDAL;
    let items = json!({ vandal: capture["Loadouts"][0]["Items"][vandal].clone() });
    get(
        server,
        &format!("/core-game/v1/matches/{match_id}/loadouts"),
        json!({"Loadouts": [{
            "CharacterID": "agent",
            "Loadout": {"Subject": puuid, "Items": items}
        }]}),
    )
    .await;
}

#[tokio::test]
async fn details_sync_fallback_loadouts_are_resolved_before_the_rebuild() {
    // The poll's own core-game loadouts fetch failed (`live.loadouts` is
    // empty), so Details opens with every weapon `null`. The details sync's
    // fallback fetch then fills `live.loadouts`: its skin names must be
    // resolved (R83) before the details are rebuilt, or the rebuild would
    // show `—` for skins that are known to be equipped.
    let h = harness().await;
    lobby_with(&h, OTHER).await;
    {
        let mut live = lock(&h.st.live);
        live.stage = Stage::Ingame;
        live.coregame_match_id = Some("cg-1".into());
    }
    h.st.set_session(Some(h.session.clone()));
    mock_coregame_loadouts(&h.server, "cg-1", OTHER).await;
    mount_skin(&h.assets_server, REAVER, REAVER_RED).await;

    let seen = details_events(&h.rec);
    let first = crate::commands::player_details_for(&h.st, OTHER)
        .await
        .unwrap();
    assert_eq!(loadout_of(&first)["vandal"], Value::Null, "nothing known");
    // The sync's fallback asked core-game for the loadouts, once.
    let loadouts = "/core-game/v1/matches/cg-1/loadouts";
    for _ in 0..100 {
        if count(&h.server, "GET", loadouts).await > 0 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert_eq!(count(&h.server, "GET", loadouts).await, 1);

    let event = details_event_after(&h.rec, seen, OTHER, |d| {
        d["loadout"].get("vandal").is_some_and(|v| v.is_object())
    })
    .await;
    assert_eq!(event["loadout"]["vandal"]["skinName"], "Reaver");
    assert_eq!(skin_requests(&h).await, 1);
    assert!(!h.st.skins_resolving(OTHER), "the attempt is over");
    // The rebuild waited for the names: no event ever carried a skin that
    // was equipped but unnamed (`null`).
    for d in h.rec.details.lock().unwrap().iter().skip(seen) {
        let vandal = serde_json::to_value(d).unwrap()["loadout"]["vandal"].clone();
        assert!(
            vandal.is_object(),
            "an unnamed vandal was emitted: {vandal}"
        );
    }
}

#[tokio::test]
async fn failed_skin_fetch_settles_to_null() {
    // R97: a failed name fetch never leaves a skeleton behind.
    let h = harness().await;
    lobby_with(&h, OTHER).await;
    mount_failing_skin(&h.assets_server, REAVER).await;
    lock(&h.st.live)
        .loadouts
        .insert(OTHER.into(), reaver_loadout());

    let seen = details_events(&h.rec);
    let first = crate::commands::player_details_for(&h.st, OTHER)
        .await
        .unwrap();
    assert!(
        loadout_of(&first).get("vandal").is_none(),
        "resolving while the fetch runs"
    );
    let event = details_event_after(&h.rec, seen, OTHER, |d| {
        d["loadout"].get("vandal") == Some(&Value::Null)
    })
    .await;
    assert_eq!(event["loadout"]["vandal"], Value::Null, "settled to null");
    assert!(!h.st.skins_resolving(OTHER));
    assert_eq!(skin_requests(&h).await, 1);
    // Nothing stays absent once `resolve_skins` has returned.
    let settled = crate::details::build(&h.st, OTHER).await.unwrap();
    assert_eq!(loadout_of(&settled)["vandal"], Value::Null);

    // The next open tries the fetch once more (R83), and settles again.
    let seen = details_events(&h.rec);
    let again = crate::commands::player_details_for(&h.st, OTHER)
        .await
        .unwrap();
    assert!(loadout_of(&again).get("vandal").is_none());
    details_event_after(&h.rec, seen, OTHER, |d| {
        d["loadout"].get("vandal") == Some(&Value::Null)
    })
    .await;
    assert_eq!(skin_requests(&h).await, 2);
    assert!(!h.st.skins_resolving(OTHER));
}

#[tokio::test]
async fn others_loadout_null_outside_match() {
    // R83: another player's loadout is readable only In Game and for Agent
    // Select allies; in the Lobby all four weapons are `null` (not absent)
    // and no skin request is made — even though the user's own is known.
    let h = harness().await;
    lobby_with(&h, OTHER).await;
    lock(&h.st.live).self_loadout = Some(reaver_loadout());
    mount_skin(&h.assets_server, REAVER, REAVER_RED).await;

    let details = crate::commands::player_details_for(&h.st, OTHER)
        .await
        .unwrap();
    let loadout = loadout_of(&details);
    for weapon in ["vandal", "phantom", "operator", "sheriff"] {
        assert_eq!(
            loadout.get(weapon),
            Some(&Value::Null),
            "{weapon}: {loadout}"
        );
    }
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert_eq!(skin_requests(&h).await, 0);
    assert!(!h.st.skins_resolving(OTHER));
}

/// The player-loadout fixture's four equipped skins, lower-case.
fn fixture_loadout() -> (RawLoadout, Value) {
    let body: Value = serde_json::from_str(&riot_fixture("player_loadout.json")).unwrap();
    let gun = |i: usize| {
        equipped(
            &body["Guns"][i]["SkinID"]
                .as_str()
                .unwrap()
                .to_ascii_lowercase(),
            &body["Guns"][i]["ChromaID"]
                .as_str()
                .unwrap()
                .to_ascii_lowercase(),
        )
    };
    let raw = RawLoadout {
        vandal: gun(0),
        phantom: gun(1),
        operator: gun(2),
        sheriff: gun(3),
    };
    (raw, body)
}

async fn playerloadout_requests(h: &Harness) -> Vec<String> {
    h.server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.url.path().starts_with("/personalization/"))
        .map(|r| r.url.path().to_string())
        .collect()
}

/// Waits (up to 5s) until `n` own-loadout requests were made and read.
async fn wait_for_playerloadouts(h: &Harness, n: usize) {
    for _ in 0..100 {
        if playerloadout_requests(h).await.len() >= n {
            // Give the response a moment to land in the live model.
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("no own-loadout request #{n}");
}

#[tokio::test]
async fn self_loadout_fetched_once_per_connect() {
    let h = harness().await;
    let (raw, body) = fixture_loadout();
    let own_path = format!("/personalization/v2/players/{ME}/playerloadout");
    get(&h.server, &own_path, body).await;
    let vandal = raw.vandal.clone().unwrap();
    mount_skin(&h.assets_server, &vandal.skin_id, &vandal.chroma_id).await;
    lobby_with(&h, ME).await;

    // The connect asks once.
    let mut poller = Poller::new(h.st.clone());
    poller.on_connected(h.session.clone()).await;
    wait_for_playerloadouts(&h, 1).await;
    assert_eq!(
        playerloadout_requests(&h).await,
        std::slice::from_ref(&own_path)
    );
    assert_eq!(lock(&h.st.live).self_loadout, Some(raw.clone()));

    // The user's Details in the Lobby (no match loadouts): their own skins.
    lock(&h.st.live).stage = Stage::Menus;
    let seen = details_events(&h.rec);
    let details = crate::commands::player_details_for(&h.st, ME)
        .await
        .unwrap();
    assert!(loadout_of(&details).get("vandal").is_none(), "resolving");
    let event = details_event_after(&h.rec, seen, ME, |d| {
        d["loadout"].get("vandal").is_some_and(|v| v.is_object())
    })
    .await;
    assert_eq!(
        event["loadout"]["vandal"]["chromaId"],
        json!(vandal.chroma_id)
    );
    assert_eq!(event["loadout"]["vandal"]["skinName"], json!("Reaver"));
    assert_eq!(event["loadout"]["vandal"]["chromaName"], json!("Red"));
    // The other three skins aren't known to valorant-api here: `null`.
    for weapon in ["phantom", "operator", "sheriff"] {
        assert_eq!(event["loadout"][weapon], Value::Null, "{weapon}");
    }

    // More activity (publishes, another open) asks for nothing more.
    h.st.publish();
    crate::commands::player_details_for(&h.st, ME)
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    assert_eq!(playerloadout_requests(&h).await.len(), 1);

    // A reconnect asks once more, and only ever for the user.
    poller.on_connected(h.session.clone()).await;
    wait_for_playerloadouts(&h, 2).await;
    assert_eq!(
        playerloadout_requests(&h).await,
        [own_path.clone(), own_path]
    );
}

#[tokio::test]
async fn self_loadout_kept_in_app_state() {
    let h = harness_with_db_file().await;
    let (raw, body) = fixture_loadout();
    get(
        &h.server,
        &format!("/personalization/v2/players/{ME}/playerloadout"),
        body,
    )
    .await;
    let vandal = raw.vandal.clone().unwrap();
    mount_skin(&h.assets_server, &vandal.skin_id, &vandal.chroma_id).await;
    lobby_with(&h, ME).await;

    let mut poller = Poller::new(h.st.clone());
    poller.on_connected(h.session.clone()).await;
    wait_for_playerloadouts(&h, 1).await;
    // The four skins are kept in `app_state`.
    let stored =
        h.st.db(|repo| repo.get_state("self_loadout"))
            .await
            .unwrap()
            .expect("stored");
    let stored: RawLoadout = serde_json::from_str(&stored).unwrap();
    assert_eq!(stored, raw);
    assert!(stored.slots().iter().all(|slot| slot.is_some()));

    // A fresh app on the same DB, with no Riot session (VALORANT closed):
    // the user's Details still resolves the same skins from the stored copy.
    let rec = Arc::new(Recorder::default());
    let repo = Repo::new(open_db(h.db_path.as_deref().unwrap()).unwrap());
    let assets = AssetCache::with_base(h._dir.path().join("assets-restart"), h.assets_server.uri());
    let restarted = Arc::new(AppState::new(
        Box::new(Sink(rec.clone())),
        repo,
        assets,
        h._dir.path().join("debug-restart"),
        Some(ME.into()),
    ));
    assert!(restarted.session().is_none());
    assert_eq!(lock(&restarted.live).self_loadout, Some(raw));
    let details = crate::commands::player_details_for(&restarted, ME)
        .await
        .unwrap();
    assert!(loadout_of(&details).get("vandal").is_none(), "resolving");
    let event = details_event_after(&rec, 0, ME, |d| {
        d["loadout"].get("vandal").is_some_and(|v| v.is_object())
    })
    .await;
    assert_eq!(event["loadout"]["vandal"]["skinName"], json!("Reaver"));
    assert_eq!(
        playerloadout_requests(&h).await.len(),
        1,
        "the restart read the stored copy, no request"
    );
}

#[tokio::test]
async fn own_loadout_prefers_the_matches_over_the_stored_copy() {
    // R90 source order for the user's own Details: the current match's
    // loadout first, else their own last read (the copy restored from
    // `app_state` at start), else `null`. Here both exist and differ: the
    // match's skin is the one named, and the stored one is never fetched.
    let h = harness_with_db_file().await;
    let (stored_raw, _) = fixture_loadout();
    assert_ne!(
        stored_raw.vandal.as_ref().unwrap().chroma_id,
        REAVER_RED,
        "the two loadouts must be distinguishable"
    );
    let stored = serde_json::to_string(&stored_raw).unwrap();
    h.st.db(move |repo| repo.set_state("self_loadout", &stored))
        .await
        .unwrap();
    lobby_with(&h, ME).await;
    mount_skin(&h.assets_server, REAVER, REAVER_RED).await;

    // A fresh app on the same DB, with no Riot session: the stored copy is
    // the user's own read...
    let rec = Arc::new(Recorder::default());
    let repo = Repo::new(open_db(h.db_path.as_deref().unwrap()).unwrap());
    let assets = AssetCache::with_base(h._dir.path().join("assets-restart"), h.assets_server.uri());
    let restarted = Arc::new(AppState::new(
        Box::new(Sink(rec.clone())),
        repo,
        assets,
        h._dir.path().join("debug-restart"),
        Some(ME.into()),
    ));
    assert_eq!(lock(&restarted.live).self_loadout, Some(stored_raw));
    // ...and the current match's loadout for the user outranks it.
    lock(&restarted.live)
        .loadouts
        .insert(ME.into(), reaver_loadout());

    let details = crate::commands::player_details_for(&restarted, ME)
        .await
        .unwrap();
    assert!(loadout_of(&details).get("vandal").is_none(), "resolving");
    let event = details_event_after(&rec, 0, ME, |d| {
        d["loadout"].get("vandal").is_some_and(|v| v.is_object())
    })
    .await;
    assert_eq!(event["loadout"]["vandal"]["chromaId"], json!(REAVER_RED));
    assert_eq!(
        skin_requests(&h).await,
        1,
        "only the match's skin was looked up"
    );
}

#[tokio::test]
async fn resolve_skins_reports_whether_a_name_became_known() {
    // A failing fetch: nothing learned, still pending (retried next open).
    let h = harness().await;
    mount_failing_skin(&h.assets_server, REAVER).await;
    lock(&h.st.live)
        .loadouts
        .insert(OTHER.into(), reaver_loadout());
    assert!(crate::details::skins_pending(&h.st, OTHER));
    assert!(!crate::details::resolve_skins(&h.st, OTHER).await);
    assert!(crate::details::skins_pending(&h.st, OTHER));

    // A working one: learned once, then nothing left to resolve (and no
    // second request).
    let h = harness().await;
    mount_skin(&h.assets_server, REAVER, REAVER_RED).await;
    lock(&h.st.live)
        .loadouts
        .insert(OTHER.into(), reaver_loadout());
    assert!(crate::details::skins_pending(&h.st, OTHER));
    assert!(crate::details::resolve_skins(&h.st, OTHER).await);
    assert!(!crate::details::skins_pending(&h.st, OTHER));
    assert!(!crate::details::resolve_skins(&h.st, OTHER).await);
    assert_eq!(skin_requests(&h).await, 1);

    // No loadout at all (or none equipped): nothing pending.
    assert!(!crate::details::skins_pending(&h.st, STRANGER));
    lock(&h.st.live)
        .loadouts
        .insert(STRANGER.into(), RawLoadout::default());
    assert!(!crate::details::skins_pending(&h.st, STRANGER));
    assert!(!crate::details::resolve_skins(&h.st, STRANGER).await);
}

#[tokio::test]
async fn account_switch_forgets_the_previous_users_loadout() {
    let h = harness_with_db_file().await;
    // A previous account's skins, in memory and in the stored copy.
    let (raw, _) = fixture_loadout();
    {
        let mut live = lock(&h.st.live);
        live.self_puuid = Some("previous-account".into());
        live.self_loadout = Some(raw.clone());
    }
    let stored = serde_json::to_string(&raw).unwrap();
    h.st.db(move |repo| repo.set_state("self_loadout", &stored))
        .await
        .unwrap();

    // ME connects (the playerloadout mock isn't mounted: the read fails and
    // is not retried): the previous account's loadout is gone everywhere.
    let mut poller = Poller::new(h.st.clone());
    poller.on_connected(h.session.clone()).await;
    wait_for_playerloadouts(&h, 1).await;
    assert_eq!(lock(&h.st.live).self_puuid.as_deref(), Some(ME));
    assert_eq!(lock(&h.st.live).self_loadout, None);
    let kept =
        h.st.db(|repo| repo.get_state("self_loadout"))
            .await
            .unwrap();
    assert_eq!(
        kept, None,
        "deleted, not left (or blanked) for the next start"
    );

    // A restart does not resurrect it.
    let repo = Repo::new(open_db(h.db_path.as_deref().unwrap()).unwrap());
    let assets = AssetCache::with_base(h._dir.path().join("assets-restart"), h.assets_server.uri());
    let restarted = AppState::new(
        Box::new(Sink(Arc::new(Recorder::default()))),
        repo,
        assets,
        h._dir.path().join("debug-restart"),
        Some(ME.into()),
    );
    assert_eq!(lock(&restarted.live).self_loadout, None);
}
