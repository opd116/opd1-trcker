//! Names on screen survive a stage change: the live `names()` answer of a
//! stage is written to the DB, but a roster slot keeps the DB bundle it was
//! installed with. A reset transition (into the Lobby) clears the in-memory
//! answers (`forget_synced`) — the renamed-player refresh — so unless the
//! slot's `players` row is read again the card has no name even though the
//! DB stored it seconds earlier.
//!
//! Four tests, all through the real `Poller`:
//! - the dodge case (Agent Select -> Lobby) — the regression for the fix;
//! - the match-end case (In Game -> Lobby) — the other reset transition the
//!   defect reaches, with the same stale-slot setup;
//! - the failed roster fetch after a reset (`show_self_only`, then the retry):
//!   it shows the DB name on the retry install. This one passes on the
//!   baseline too, and on purpose: `show_self_only` replaces every slot with
//!   the user's card, so the retry installs every other player as a *new*
//!   slot and reads their full DB bundle — the carried stale row this fix
//!   covers cannot survive that path. It is kept as coverage of the retry
//!   transition, not as a reproducer of the defect.
//! - the hidden player's remembered name (R145): a match row naming an
//!   incognito party member is stored while its slot is carried, so the DB
//!   knows a newer remembered name than the slot's bundle; the reset install
//!   must show it (and still never asks name-service for the hidden player).
//!
//! Each test starts with the app opening into a live stage (mid Agent Select
//! or mid match), waits until a name is on screen while its rank phase is
//! still in flight (the slot provably holds the older row), then lets the
//! stage go back to the Lobby: the player stays on the roster and the DB
//! knows the name, so no published Lobby state may show the name as
//! loading/missing.

use std::time::Duration;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;

use opd1_core::model::{DisplayName, Field, LiveState, RememberedSource};
use opd1_core::presence::Stage;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use super::*;
use crate::poll::Poller;
use crate::state::lock;

/// The name `OTHER`'s card shows once its Riot ID resolved.
fn want_name() -> Option<(String, String)> {
    Some(("Other".to_string(), "OTH".to_string()))
}

/// The switchable presence blob of the user's own VALORANT presence.
type Blob = Arc<Mutex<Option<Value>>>;

fn blob(state: &str, queue: &str) -> Value {
    json!({"sessionLoopState": state, "queueId": queue, "partyId": "party-1",
           "partyClientVersion": "release-11.08-shipping-9-3104338"})
}

async fn presence(h: &Harness, start: Value) -> Blob {
    let cell: Blob = Arc::new(Mutex::new(Some(start)));
    let served = cell.clone();
    Mock::given(method("GET"))
        .and(path("/chat/v4/presences"))
        .respond_with(move |_: &Request| {
            let presences = match served.lock().unwrap().clone() {
                Some(blob) => vec![json!({
                    "puuid": ME, "product": "valorant",
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

fn identity_of(incognito: bool) -> Value {
    json!({"Incognito": incognito, "HideAccountLevel": false,
           "AccountLevel": 10, "PlayerCardID": "card"})
}

fn identity() -> Value {
    identity_of(false)
}

/// The user's party: ME (visible) and `other`, id `party-1`.
async fn mount_party_of(h: &Harness, other: &str, incognito: bool) {
    get(
        &h.server,
        &format!("/parties/v1/players/{ME}"),
        json!({"Subject": ME, "CurrentPartyID": "party-1"}),
    )
    .await;
    get(
        &h.server,
        "/parties/v1/parties/party-1",
        json!({"ID": "party-1", "Members": [
            {"Subject": ME, "PlayerIdentity": identity()},
            {"Subject": other, "PlayerIdentity": identity_of(incognito)}
        ]}),
    )
    .await;
}

/// The user's party: ME and OTHER (both visible), id `party-1`.
async fn mount_party(h: &Harness) {
    mount_party_of(h, OTHER, false).await;
}

/// Agent Select `pg-1` with ME and `other` as the allies.
async fn mount_pregame_of(h: &Harness, other: &str, incognito: bool) {
    get(
        &h.server,
        &format!("/pregame/v1/players/{ME}"),
        json!({"Subject": ME, "MatchID": "pg-1"}),
    )
    .await;
    let ally = json!({"TeamID": "Blue", "Players": [
        {"Subject": ME, "CharacterID": "agent", "CharacterSelectionState": "locked",
         "PlayerIdentity": identity()},
        {"Subject": other, "CharacterID": "agent", "CharacterSelectionState": "locked",
         "PlayerIdentity": identity_of(incognito)}
    ]});
    get(
        &h.server,
        "/pregame/v1/matches/pg-1",
        json!({"MapID": SPLIT_PATH, "AllyTeam": ally, "Teams": [ally]}),
    )
    .await;
}

/// Agent Select `pg-1` with ME and OTHER as the allies.
async fn mount_pregame(h: &Harness) {
    mount_pregame_of(h, OTHER, false).await;
}

/// In Game `cg-1` with ME and OTHER on the same team.
async fn mount_coregame(h: &Harness) {
    get(
        &h.server,
        &format!("/core-game/v1/players/{ME}"),
        json!({"Subject": ME, "MatchID": "cg-1"}),
    )
    .await;
    let player = |puuid: &str| {
        json!({"Subject": puuid, "TeamID": "Blue", "CharacterID": "agent",
               "PlayerIdentity": identity()})
    };
    get(
        &h.server,
        "/core-game/v1/matches/cg-1",
        json!({"MapID": SPLIT_PATH, "Players": [player(ME), player(OTHER)]}),
    )
    .await;
}

/// `puuid`'s rank phase must not finish inside a test: the stage change
/// happens once its name/state is on screen, so its slot still holds the
/// bundle read before the DB learned anything new. A 60 s answer is
/// effectively non-completing (the worker is aborted at the stage change and
/// no test awaits it), which keeps the window open regardless of the machine's
/// load; a short delay could lose the race on a slow runner.
async fn hold_rank_phase(h: &Harness, puuid: &str) {
    Mock::given(method("GET"))
        .and(path(format!("/mmr/v1/players/{puuid}/competitiveupdates")))
        .respond_with(ResponseTemplate::new(500).set_delay(Duration::from_secs(60)))
        .with_priority(1)
        .mount(&h.server)
        .await;
}

/// The name `OTHER`'s card shows in `state`, and whether it is a real name
/// (not the loading / missing wire state).
fn shown_name(state: &LiveState) -> Option<(String, String)> {
    let card = state.players.iter().find(|c| c.puuid == OTHER)?;
    match &card.name {
        Field::Value(DisplayName::Visible { name, tag }) => Some((name.clone(), tag.clone())),
        _ => None,
    }
}

/// Waits for OTHER's name to be published, then asserts the DB stores it and
/// the slot's bundle is still the stale one (read before the name).
async fn wait_for_named_but_stale(h: &Harness) {
    wait_until("OTHER's name on the live card", || {
        shown_name(&h.st.live_state()).is_some()
    })
    .await;
    assert_eq!(shown_name(&h.st.live_state()), want_name());
    let stored =
        h.st.db(|repo| crate::store::load_player(repo.conn(), OTHER))
            .await
            .flatten()
            .and_then(|row| row.name());
    assert_eq!(stored, want_name(), "the DB stores the resolved name");
    {
        let live = lock(&h.st.live);
        let slot = live.slots.iter().find(|s| s.roster.puuid == OTHER).unwrap();
        assert!(
            slot.db.row.as_ref().and_then(|r| r.name()).is_none(),
            "the slot's DB bundle is stale while the rank phase runs"
        );
    }
}

/// Asserts every published state from `events_before` on that still shows
/// OTHER (stage `menus`) carries the name the DB stores.
fn assert_lobby_never_blanks_other(h: &Harness, events_before: usize) {
    let want = want_name();
    let events = h.live_events();
    let after: Vec<&LiveState> = events[events_before..]
        .iter()
        .filter(|s| s.stage == Stage::Menus)
        .collect();
    assert!(!after.is_empty(), "the Lobby was published");
    for (i, state) in after.iter().enumerate() {
        let shown = state
            .players
            .iter()
            .find(|c| c.puuid == OTHER)
            .map(|c| format!("{:?}", c.name));
        assert_eq!(
            shown_name(state),
            want,
            "state {i} after the reset shows OTHER's name as {shown:?}, but the DB stores it"
        );
    }
}

async fn puts(h: &Harness) -> usize {
    h.server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r: &&Request| {
            r.method.as_str() == "PUT" && r.url.path() == "/name-service/v2/players"
        })
        .count()
}

/// The rename rule is untouched: a stage reset asks name-service again.
async fn assert_names_asked_again(h: &Harness, puts_before: usize) {
    wait_until("the Lobby asks name-service again after the reset", || {
        lock(&h.st.live).known_names.contains_key(OTHER)
    })
    .await;
    assert!(
        puts(h).await > puts_before,
        "a fresh name-service request goes out after the reset"
    );
}

#[tokio::test]
async fn a_dodge_back_to_the_lobby_keeps_a_name_the_db_already_stores() {
    let h = harness().await;
    hold_rank_phase(&h, OTHER).await;
    mount_party(&h).await;
    mount_pregame(&h).await;
    let cell = presence(&h, blob("PREGAME", "competitive")).await;

    let mut poller = Poller::new(h.st.clone());
    poller.on_connected(h.session.clone()).await;
    poller.presence_tick(&h.session).await.unwrap();
    assert_eq!(h.st.live_state().stage, Stage::Pregame);
    wait_for_named_but_stale(&h).await;

    // A dodge: the Lobby again. OTHER stays on the roster (same party), the
    // sync state — and with it the in-memory names — resets.
    let events_before = h.live_events().len();
    let puts_before = puts(&h).await;
    *cell.lock().unwrap() = Some(blob("MENUS", "competitive"));
    poller.presence_tick(&h.session).await.unwrap();
    assert_eq!(h.st.live_state().stage, Stage::Menus);
    assert!(lock(&h.st.live)
        .slots
        .iter()
        .any(|s| s.roster.puuid == OTHER));

    assert_lobby_never_blanks_other(&h, events_before);
    assert_names_asked_again(&h, puts_before).await;
    poller.stop_worker();
}

#[tokio::test]
async fn a_match_end_keeps_a_name_the_db_already_stores() {
    // The other reset transition: In Game -> Lobby. The app opened mid-match,
    // so every slot was installed before the live names lookup stored the
    // name; the rank phase is still in flight when the match ends.
    let h = harness().await;
    hold_rank_phase(&h, OTHER).await;
    mount_party(&h).await;
    mount_coregame(&h).await;
    let cell = presence(&h, blob("INGAME", "unrated")).await;

    let mut poller = Poller::new(h.st.clone());
    poller.ingame_loadouts_timeout = Duration::from_millis(50);
    poller.on_connected(h.session.clone()).await;
    poller.presence_tick(&h.session).await.unwrap();
    assert_eq!(h.st.live_state().stage, Stage::Ingame);
    wait_for_named_but_stale(&h).await;

    // The match ends: `match_ended` + the reset transition to the Lobby. The
    // party is still ME + OTHER, so OTHER stays on the roster.
    let events_before = h.live_events().len();
    let puts_before = puts(&h).await;
    *cell.lock().unwrap() = Some(blob("MENUS", "unrated"));
    poller.presence_tick(&h.session).await.unwrap();
    assert_eq!(h.st.live_state().stage, Stage::Menus);
    assert!(lock(&h.st.live)
        .slots
        .iter()
        .any(|s| s.roster.puuid == OTHER));

    assert_lobby_never_blanks_other(&h, events_before);
    assert_names_asked_again(&h, puts_before).await;
    poller.stop_worker();
}

#[tokio::test]
async fn a_failed_roster_fetch_after_the_reset_shows_the_stored_name_on_the_retry() {
    // A reset transition whose Lobby roster fetch fails once: `show_self_only`
    // publishes the user's card alone (and drops every other slot), then the
    // retry installs the party again.
    //
    // This test passes on the baseline too, deliberately: because the failed
    // fetch replaced the slots, the retry installs OTHER as a NEW slot and
    // reads its full DB bundle, so the carried stale row cannot reach the
    // screen through this path. It is coverage of the retry transition (the
    // stored name is shown, never a skeleton), not a reproducer of the fixed
    // defect — which needs a carried slot, i.e. a transition whose fetch
    // succeeds (the dodge and match-end tests).
    let h = harness().await;
    hold_rank_phase(&h, OTHER).await;
    mount_party(&h).await;
    mount_pregame(&h).await;
    let cell = presence(&h, blob("PREGAME", "competitive")).await;

    let mut poller = Poller::new(h.st.clone());
    poller.on_connected(h.session.clone()).await;
    poller.presence_tick(&h.session).await.unwrap();
    assert_eq!(h.st.live_state().stage, Stage::Pregame);
    wait_for_named_but_stale(&h).await;

    // The next party fetch (the reset transition's roster fetch) fails once.
    Mock::given(method("GET"))
        .and(path("/parties/v1/parties/party-1"))
        .respond_with(ResponseTemplate::new(500))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&h.server)
        .await;

    let events_before = h.live_events().len();
    let puts_before = puts(&h).await;
    *cell.lock().unwrap() = Some(blob("MENUS", "competitive"));
    poller.presence_tick(&h.session).await.unwrap();
    assert_eq!(h.st.live_state().stage, Stage::Menus, "still the Lobby");
    {
        let state = h.st.live_state();
        assert_eq!(
            state.players.len(),
            1,
            "the failed fetch shows the user only"
        );
        assert!(state.players[0].is_self);
        assert_eq!(
            shown_name(&state),
            None,
            "OTHER is not on the failed roster"
        );
    }
    // Nothing published in that window may have shown OTHER without a name
    // (it was not shown at all).
    for state in &h.live_events()[events_before..] {
        if state.players.iter().any(|c| c.puuid == OTHER) {
            assert_eq!(
                shown_name(state),
                want_name(),
                "a state that shows OTHER must carry the stored name"
            );
        }
    }

    // The retry has the fetch's own path (`fetch_or_retry` -> `fetch_roster` ->
    // `fetch_party`); called directly instead of waiting out the 2 s retry
    // timer, so the test needs no wall-clock wait.
    poller.fetch_party(&h.session, ME).await.unwrap();
    assert_eq!(h.st.live_state().stage, Stage::Menus);
    assert_eq!(
        shown_name(&h.st.live_state()),
        want_name(),
        "the retry install shows the name the DB stores"
    );
    assert_names_asked_again(&h, puts_before).await;
    poller.stop_worker();
}

/// Asserts every Lobby state published from `events_before` on that still
/// shows HIDDEN carries the remembered name the DB holds.
fn assert_lobby_shows_remembered_for_hidden(h: &Harness, events_before: usize, want: &DisplayName) {
    let events = h.live_events();
    let after: Vec<&LiveState> = events[events_before..]
        .iter()
        .filter(|s| s.stage == Stage::Menus)
        .collect();
    assert!(!after.is_empty(), "the Lobby was published");
    for (i, state) in after.iter().enumerate() {
        let Some(card) = state.players.iter().find(|c| c.puuid == HIDDEN) else {
            continue;
        };
        match &card.name {
            Field::Value(shown) if shown == want => {}
            other => panic!(
                "state {i} after the reset shows HIDDEN's card name as {other:?}, but the DB knows {want:?}"
            ),
        }
    }
}

#[tokio::test]
async fn a_hidden_player_carried_through_a_reset_shows_the_remembered_name_the_db_now_knows() {
    // R145: a hidden player's card shows `slot.db.remembered`, refreshed at a
    // roster install, by that player's own sync publish, or by a names fill
    // (`reload_slots`). A match row naming them can be stored in between
    // (another roster player's matches phase, the post-match store, the
    // backfill: all `Repo::upsert_match` through `sync::store_details`), and
    // the reset install only re-read the `players` row, so the newer
    // remembered data would not reach the card: it kept `HiddenPlayer`
    // although the DB knew a name.
    let h = harness().await;
    // HIDDEN is the user's party member and stays incognito; its rank phase is
    // held, so nothing reloads its bundle after the roster install.
    hold_rank_phase(&h, HIDDEN).await;
    mount_party_of(&h, HIDDEN, true).await;
    mount_pregame_of(&h, HIDDEN, true).await;
    let cell = presence(&h, blob("PREGAME", "competitive")).await;

    let mut poller = Poller::new(h.st.clone());
    poller.on_connected(h.session.clone()).await;
    poller.presence_tick(&h.session).await.unwrap();
    assert_eq!(h.st.live_state().stage, Stage::Pregame);

    // The installed bundle knows no name yet: the card is in the hidden style.
    {
        let live = lock(&h.st.live);
        let slot = live
            .slots
            .iter()
            .find(|s| s.roster.puuid == HIDDEN)
            .unwrap();
        assert!(
            slot.db.remembered.is_none(),
            "nothing known at the bundle read"
        );
    }
    let hidden_style =
        h.st.live_state()
            .players
            .iter()
            .find(|c| c.puuid == HIDDEN)
            .map(|c| c.name.clone())
            .unwrap();
    assert!(
        matches!(
            hidden_style,
            Field::Value(DisplayName::HiddenAgent { .. }) | Field::Value(DisplayName::HiddenPlayer)
        ),
        "the card is in the hidden style, not a name: {hidden_style:?}"
    );

    // The DB learns the name while the slot is carried. `store_split_match`
    // writes a match ME and HIDDEN played (`Repo::upsert_match`, the same
    // write `sync::store_details` does), so `known_name` is `OwnMatch`.
    let start = store_split_match(&h.st).await;
    let known =
        h.st.db(|repo| repo.known_name(ME, HIDDEN))
            .await
            .flatten()
            .expect("the DB knows HIDDEN now");
    assert_eq!(
        (known.name.as_str(), known.tag.as_str(), known.source),
        ("SecretName", "HID", RememberedSource::OwnMatch)
    );
    {
        let live = lock(&h.st.live);
        let slot = live
            .slots
            .iter()
            .find(|s| s.roster.puuid == HIDDEN)
            .unwrap();
        assert!(
            slot.db.remembered.is_none(),
            "the carried slot still has the bundle read before the store"
        );
    }
    let want = DisplayName::Remembered {
        name: "SecretName".into(),
        tag: "HID".into(),
        source: RememberedSource::OwnMatch,
        from_map_id: None, // the asset tables are not loaded here
        from_start_ms: Some(start),
    };

    // A dodge: the Lobby. HIDDEN stays in the party and the sync state resets.
    let events_before = h.live_events().len();
    *cell.lock().unwrap() = Some(blob("MENUS", "competitive"));
    poller.presence_tick(&h.session).await.unwrap();
    assert_eq!(h.st.live_state().stage, Stage::Menus);
    assert!(lock(&h.st.live)
        .slots
        .iter()
        .any(|s| s.roster.puuid == HIDDEN));
    assert_lobby_shows_remembered_for_hidden(&h, events_before, &want);

    // A second reset transition (Lobby -> Agent Select, no reset -> Lobby,
    // reset) keeps it as well.
    let events_before = h.live_events().len();
    *cell.lock().unwrap() = Some(blob("PREGAME", "competitive"));
    poller.presence_tick(&h.session).await.unwrap();
    assert_eq!(h.st.live_state().stage, Stage::Pregame);
    *cell.lock().unwrap() = Some(blob("MENUS", "competitive"));
    poller.presence_tick(&h.session).await.unwrap();
    assert_eq!(h.st.live_state().stage, Stage::Menus);
    assert_lobby_shows_remembered_for_hidden(&h, events_before, &want);

    // Privacy: a live-incognito player's name is never requested.
    for r in h.server.received_requests().await.unwrap() {
        if r.method.as_str() == "PUT" && r.url.path() == "/name-service/v2/players" {
            let body = String::from_utf8_lossy(&r.body);
            assert!(
                !body.contains(HIDDEN),
                "a hidden player was sent to name-service: {body}"
            );
        }
    }
    poller.stop_worker();
}
