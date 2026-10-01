//! R161's pipeline tests: the tier Riot's roster answers already carry
//! (pregame `CompetitiveTier`, a party member's `CompetitiveTier`) shows on
//! the card at once, before any rank call returns, and is carried from Agent
//! Select into In Game, where core-game answers carry none. A child of
//! `pipeline_tests`, so it shares that module's harness and helpers through
//! `super::*`.

use super::*;
use crate::poll::Poller;

/// Every rank call of `puuid` (competitive updates, MMR) fails, so only the
/// roster's own tier can fill its rank.
async fn rank_calls_fail(h: &Harness, puuid: &str) {
    for p in [
        format!("/mmr/v1/players/{puuid}/competitiveupdates"),
        format!("/mmr/v1/players/{puuid}"),
    ] {
        Mock::given(method("GET"))
            .and(path(p.as_str()))
            .respond_with(ResponseTemplate::new(500))
            .with_priority(1)
            .mount(&h.server)
            .await;
    }
}

fn pregame_player(puuid: &str, tier: Value, incognito: bool) -> Value {
    json!({"Subject": puuid, "CharacterID": "", "CharacterSelectionState": "",
           "CompetitiveTier": tier,
           "PlayerIdentity": {"Incognito": incognito, "HideAccountLevel": false,
                              "AccountLevel": 10, "PlayerCardID": "card"}})
}

/// Agent Select `match_id` with these players. Mounted at `priority`, so a
/// later Agent Select (a lower number) answers ahead of an earlier one.
async fn mock_pregame(
    h: &Harness,
    match_id: &str,
    priority: u8,
    ally: Vec<Value>,
    enemy: Vec<Value>,
) {
    let ally = json!({"TeamID": "Blue", "Players": ally});
    let enemy = json!({"TeamID": "Red", "Players": enemy});
    Mock::given(method("GET"))
        .and(path(format!("/pregame/v1/players/{ME}").as_str()))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"Subject": ME, "MatchID": match_id})),
        )
        .with_priority(priority)
        .mount(&h.server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/pregame/v1/matches/{match_id}").as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"MapID": SPLIT_PATH, "Teams": [ally.clone(), enemy.clone()],
                   "AllyTeam": ally, "EnemyTeam": enemy}),
        ))
        .with_priority(priority)
        .mount(&h.server)
        .await;
}

/// Agent Select `pg-1`: ME (22), OTHER (19, incognito) and MATE (0, unranked)
/// as allies; STRANGER (24) on the enemy team.
async fn mock_pregame_with_tiers(h: &Harness) {
    mock_pregame(
        h,
        "pg-1",
        5,
        vec![
            pregame_player(ME, json!(22), false),
            pregame_player(OTHER, json!(19), true),
            pregame_player(MATE, json!(0), false),
        ],
        vec![pregame_player(STRANGER, json!(24), true)],
    )
    .await;
}

/// In Game `match_id` (the Agent Select's id when it is the same match); core-game
/// answers carry no tier.
async fn mock_coregame(h: &Harness, match_id: &str) {
    let player = |puuid: &str, team: &str, incognito: bool| {
        json!({"Subject": puuid, "TeamID": team, "CharacterID": "agent",
               "PlayerIdentity": {"Incognito": incognito, "HideAccountLevel": false,
                                  "AccountLevel": 10, "PlayerCardID": "card"}})
    };
    get(
        &h.server,
        &format!("/core-game/v1/players/{ME}"),
        json!({"Subject": ME, "MatchID": match_id}),
    )
    .await;
    get(
        &h.server,
        &format!("/core-game/v1/matches/{match_id}"),
        json!({"MapID": SPLIT_PATH, "Players": [
            player(ME, "Blue", false), player(OTHER, "Blue", true),
            player(MATE, "Blue", false), player(STRANGER, "Red", true)]}),
    )
    .await;
}

async fn all_rank_calls_fail(h: &Harness) {
    for p in [ME, OTHER, MATE, STRANGER] {
        rank_calls_fail(h, p).await;
    }
}

#[tokio::test]
async fn agent_select_shows_the_tiers_riot_sent_before_any_rank_call() {
    let h = harness().await;
    all_rank_calls_fail(&h).await;
    mock_pregame_with_tiers(&h).await;
    let mut poller = Poller::new(h.st.clone());
    poller.fetch_pregame(&h.session, ME).await.unwrap();
    poller.stop_worker();

    // No sync has run for anyone: the rank is the tier from the roster, the
    // RR is `—` (null), and everything else from MMR is still loading.
    assert_eq!(
        card_json(&h.st, ME)["rank"],
        json!({"tier": 22, "rr": null})
    );
    let other = card_json(&h.st, OTHER);
    assert_eq!(
        other["rank"],
        json!({"tier": 19, "rr": null}),
        "a hidden player's rank shows like anyone's"
    );
    assert_eq!(
        other["name"]["kind"], "hiddenPlayer",
        "the name stays hidden"
    );
    assert!(other.get("peak").is_none(), "peak is still a skeleton");
    // Unranked: no rank, a skeleton while the sync is still to run.
    assert!(card_json(&h.st, MATE).get("rank").is_none());
}

#[tokio::test]
async fn in_game_keeps_the_agent_select_tiers_for_both_teams() {
    let h = harness().await;
    all_rank_calls_fail(&h).await;
    mock_pregame_with_tiers(&h).await;
    mock_coregame(&h, "pg-1").await;
    let mut poller = Poller::new(h.st.clone());
    poller.fetch_pregame(&h.session, ME).await.unwrap();
    poller.stop_worker();
    poller.fetch_ingame(&h.session, ME).await.unwrap();
    poller.stop_worker();

    assert_eq!(lock(&h.st.live).stage, Stage::Ingame);
    assert_eq!(
        card_json(&h.st, ME)["rank"],
        json!({"tier": 22, "rr": null})
    );
    assert_eq!(
        card_json(&h.st, OTHER)["rank"],
        json!({"tier": 19, "rr": null})
    );
    assert_eq!(
        card_json(&h.st, STRANGER)["rank"],
        json!({"tier": 24, "rr": null}),
        "the enemy's tier came with the pregame answer"
    );
    assert!(card_json(&h.st, MATE).get("rank").is_none(), "unranked");
}

#[tokio::test]
async fn in_game_with_no_agent_select_seen_has_no_tier_to_show() {
    // The app started mid-match: core-game answers carry no tier, so the
    // rank waits for the rank calls as before.
    let h = harness().await;
    all_rank_calls_fail(&h).await;
    mock_coregame(&h, "pg-1").await;
    let mut poller = Poller::new(h.st.clone());
    poller.fetch_ingame(&h.session, ME).await.unwrap();
    poller.stop_worker();
    assert!(card_json(&h.st, STRANGER).get("rank").is_none());
}

#[tokio::test]
async fn the_lobby_party_shows_each_members_tier() {
    let h = harness().await;
    all_rank_calls_fail(&h).await;
    get(
        &h.server,
        &format!("/parties/v1/players/{ME}"),
        json!({"Subject": ME, "CurrentPartyID": "party-1"}),
    )
    .await;
    let member = |puuid: &str, tier: Value| {
        json!({"Subject": puuid, "CompetitiveTier": tier,
               "PlayerIdentity": {"Incognito": false, "HideAccountLevel": false,
                                  "AccountLevel": 10, "PlayerCardID": "card"}})
    };
    get(
        &h.server,
        "/parties/v1/parties/party-1",
        json!({"ID": "party-1", "Members": [member(ME, json!(22)), member(MATE, json!(0))]}),
    )
    .await;
    let mut poller = Poller::new(h.st.clone());
    poller.fetch_party(&h.session, ME).await.unwrap();
    poller.stop_worker();

    assert_eq!(
        card_json(&h.st, ME)["rank"],
        json!({"tier": 22, "rr": null})
    );
    assert!(card_json(&h.st, MATE).get("rank").is_none());
}

#[tokio::test]
async fn a_rank_answer_replaces_the_live_tier() {
    // ME's rank calls work (the harness's mock: tier 15, 42 RR). The roster
    // said 22; once the sync ran, Riot's own rank answer wins.
    let h = harness().await;
    {
        let mut live = lock(&h.st.live);
        live.stage = Stage::Pregame;
        let mut me = roster(ME, false);
        me.live_tier = Some(22);
        live.slots = vec![Slot {
            roster: me,
            db: PlayerDb::default(),
        }];
    }
    h.st.publish();
    assert_eq!(
        card_json(&h.st, ME)["rank"],
        json!({"tier": 22, "rr": null})
    );

    let id = lock(&h.st.live).try_start_worker().expect("work to do");
    run_worker(h.st.clone(), h.session.clone(), id).await;
    assert_eq!(card_json(&h.st, ME)["rank"], json!({"tier": 15, "rr": 42}));
}

fn tiers_of_match(h: &Harness, match_id: &str) -> Vec<(String, u8)> {
    let mut v: Vec<_> = lock(&h.st.live)
        .match_tiers
        .for_match(match_id)
        .into_iter()
        .collect();
    v.sort();
    v
}

#[tokio::test]
async fn a_second_agent_select_replaces_the_first_one_wholesale() {
    let h = harness().await;
    all_rank_calls_fail(&h).await;
    mock_pregame_with_tiers(&h).await;
    let mut poller = Poller::new(h.st.clone());
    poller.fetch_pregame(&h.session, ME).await.unwrap();
    poller.stop_worker();
    assert_eq!(tiers_of_match(&h, "pg-1").len(), 3);

    // A dodge and a new Agent Select: pg-2, only ME carries a tier.
    mock_pregame(
        &h,
        "pg-2",
        1,
        vec![pregame_player(ME, json!(23), false)],
        vec![],
    )
    .await;
    poller.fetch_pregame(&h.session, ME).await.unwrap();
    poller.stop_worker();

    assert_eq!(tiers_of_match(&h, "pg-2"), [(ME.to_string(), 23)]);
    assert!(tiers_of_match(&h, "pg-1").is_empty(), "no leftover puuids");
    assert_eq!(
        card_json(&h.st, ME)["rank"],
        json!({"tier": 23, "rr": null})
    );
}

#[tokio::test]
async fn a_re_poll_of_the_same_agent_select_keeps_a_tier_the_answer_leaves_out() {
    let h = harness().await;
    all_rank_calls_fail(&h).await;
    mock_pregame_with_tiers(&h).await;
    let mut poller = Poller::new(h.st.clone());
    poller.fetch_pregame(&h.session, ME).await.unwrap();
    poller.stop_worker();

    // Same match, the re-poll's answer has no tier for OTHER and a new one
    // for ME.
    let no_tier = json!({"Subject": OTHER, "CharacterID": "", "CharacterSelectionState": "",
        "PlayerIdentity": {"Incognito": true, "HideAccountLevel": false,
                           "AccountLevel": 10, "PlayerCardID": "card"}});
    mock_pregame(
        &h,
        "pg-1",
        1,
        vec![pregame_player(ME, json!(23), false), no_tier],
        vec![],
    )
    .await;
    poller.pregame_repoll(&h.session).await.unwrap();
    poller.stop_worker();

    assert_eq!(
        tiers_of_match(&h, "pg-1"),
        [
            (ME.to_string(), 23),
            (OTHER.to_string(), 19),
            (STRANGER.to_string(), 24)
        ]
    );
}

#[tokio::test]
async fn a_lobby_fetch_after_a_match_replaces_the_agent_select_tiers() {
    let h = harness().await;
    all_rank_calls_fail(&h).await;
    mock_pregame_with_tiers(&h).await;
    let mut poller = Poller::new(h.st.clone());
    poller.fetch_pregame(&h.session, ME).await.unwrap();
    poller.stop_worker();
    assert!(!tiers_of_match(&h, "pg-1").is_empty());

    get(
        &h.server,
        &format!("/parties/v1/players/{ME}"),
        json!({"Subject": ME, "CurrentPartyID": "party-1"}),
    )
    .await;
    get(
        &h.server,
        "/parties/v1/parties/party-1",
        json!({"ID": "party-1", "Members": [{"Subject": ME, "CompetitiveTier": 22,
            "PlayerIdentity": {"Incognito": false, "HideAccountLevel": false,
                               "AccountLevel": 10, "PlayerCardID": "card"}}]}),
    )
    .await;
    poller.fetch_party(&h.session, ME).await.unwrap();
    poller.stop_worker();

    assert!(lock(&h.st.live).match_tiers.is_empty());
    assert!(tiers_of_match(&h, "pg-1").is_empty());
}

#[tokio::test]
async fn closing_empties_the_match_tiers() {
    let h = harness().await;
    mock_pregame_with_tiers(&h).await;
    let mut poller = Poller::new(h.st.clone());
    poller.fetch_pregame(&h.session, ME).await.unwrap();
    poller.stop_worker();
    assert!(!lock(&h.st.live).match_tiers.is_empty());

    poller.enter_closed().await;
    assert!(lock(&h.st.live).match_tiers.is_empty());
}

#[tokio::test]
async fn an_account_switch_leaves_no_tiers_of_the_previous_account() {
    let h = harness().await;
    {
        let mut live = lock(&h.st.live);
        live.self_puuid = Some("previous-account".into());
        live.match_tiers
            .set_pregame("pg-1", [(OTHER.to_string(), 19)].into());
    }
    let mut poller = Poller::new(h.st.clone());
    poller.on_connected(h.session.clone()).await;
    poller.stop_worker();
    assert!(lock(&h.st.live).match_tiers.is_empty());
}

#[tokio::test]
async fn in_game_after_a_missed_agent_select_does_not_take_the_old_matches_tiers() {
    // The Agent Select of match pg-1 was read; the next match (cg-9) starts
    // with no Agent Select seen by the app (its fetch failed). Nothing of
    // pg-1 may reach its cards.
    let h = harness().await;
    all_rank_calls_fail(&h).await;
    mock_pregame_with_tiers(&h).await;
    mock_coregame(&h, "cg-9").await;
    let mut poller = Poller::new(h.st.clone());
    poller.fetch_pregame(&h.session, ME).await.unwrap();
    poller.stop_worker();
    poller.fetch_ingame(&h.session, ME).await.unwrap();
    poller.stop_worker();

    assert_eq!(lock(&h.st.live).stage, Stage::Ingame);
    for puuid in [ME, OTHER, STRANGER] {
        assert!(
            card_json(&h.st, puuid).get("rank").is_none(),
            "{puuid} shows no tier of another match"
        );
    }
}
