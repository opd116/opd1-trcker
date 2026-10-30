//! Names on screen survive a stage change: the live `names()` answer of a
//! stage is written to the DB, but a roster slot keeps the DB bundle it was
//! installed with. A reset transition (into the Lobby) clears the in-memory
//! answers (`forget_synced`) — the renamed-player refresh — so unless the
//! slot's `players` row is read again the card has no name even though the
//! DB stored it seconds earlier.
//!
//! The test below puts the app in Agent Select (the app opened while the
//! queue was already drafting), waits until a name is on screen while its
//! rank phase still runs (the bundle provably holds the older row), then
//! dodges back to the Lobby: the player stays on the roster and the DB knows
//! the name, so no published state may show the name as loading/missing.

use std::time::Duration;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;

use opd1_core::model::{DisplayName, Field, LiveState};
use opd1_core::presence::Stage;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use super::*;
use crate::poll::Poller;
use crate::state::lock;

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

fn identity() -> Value {
    json!({"Incognito": false, "HideAccountLevel": false,
           "AccountLevel": 10, "PlayerCardID": "card"})
}

/// The user's party: ME and OTHER (both visible), id `party-1`.
async fn mount_party(h: &Harness) {
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
            {"Subject": OTHER, "PlayerIdentity": identity()}
        ]}),
    )
    .await;
}

/// Agent Select `pg-1` with ME and OTHER as the allies.
async fn mount_pregame(h: &Harness) {
    get(
        &h.server,
        &format!("/pregame/v1/players/{ME}"),
        json!({"Subject": ME, "MatchID": "pg-1"}),
    )
    .await;
    let ally = json!({"TeamID": "Blue", "Players": [
        {"Subject": ME, "CharacterID": "agent", "CharacterSelectionState": "locked",
         "PlayerIdentity": identity()},
        {"Subject": OTHER, "CharacterID": "agent", "CharacterSelectionState": "locked",
         "PlayerIdentity": identity()}
    ]});
    get(
        &h.server,
        "/pregame/v1/matches/pg-1",
        json!({"MapID": SPLIT_PATH, "AllyTeam": ally, "Teams": [ally]}),
    )
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

#[tokio::test]
async fn a_dodge_back_to_the_lobby_keeps_a_name_the_db_already_stores() {
    let h = harness().await;
    // The rank phase of OTHER must not finish before the assertions below:
    // the dodge happens once the name is on screen, so the slot still holds
    // the bundle read before the name was stored. A 60 s answer is
    // effectively non-completing (the worker is aborted at the dodge and this
    // test never awaits it), which keeps the window open regardless of the
    // machine's load; a short delay could lose the race on a slow runner.
    Mock::given(method("GET"))
        .and(path(format!("/mmr/v1/players/{OTHER}/competitiveupdates")))
        .respond_with(ResponseTemplate::new(500).set_delay(Duration::from_secs(60)))
        .with_priority(1)
        .mount(&h.server)
        .await;
    mount_party(&h).await;
    mount_pregame(&h).await;
    let cell = presence(&h, blob("PREGAME", "competitive")).await;

    let mut poller = Poller::new(h.st.clone());
    poller.on_connected(h.session.clone()).await;
    poller.presence_tick(&h.session).await.unwrap();
    assert_eq!(h.st.live_state().stage, Stage::Pregame);

    // The live names answer arrives and is published: OTHER's name is on
    // screen. The DB has it too (`names_step` stores before it publishes).
    let want = Some(("Other".to_string(), "OTH".to_string()));
    wait_until("OTHER's name on the Agent Select card", || {
        shown_name(&h.st.live_state()).is_some()
    })
    .await;
    assert_eq!(shown_name(&h.st.live_state()), want);
    let stored =
        h.st.db(|repo| crate::store::load_player(repo.conn(), OTHER))
            .await
            .flatten()
            .and_then(|row| row.name());
    assert_eq!(stored, want);
    {
        let live = lock(&h.st.live);
        let slot = live.slots.iter().find(|s| s.roster.puuid == OTHER).unwrap();
        assert!(
            slot.db.row.as_ref().and_then(|r| r.name()).is_none(),
            "the slot's DB bundle is stale while the rank phase runs"
        );
    }

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

    // Every state published for the Lobby that still shows OTHER must carry
    // the name the DB stored: a name that was on screen a moment ago may not
    // go back to a loading skeleton (or `—`) because the stage changed.
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
            "state {i} after the dodge shows OTHER's name as {shown:?}, but the DB stores it"
        );
    }

    // The rename rule is untouched: the stage reset asks name-service again,
    // so a player who changed their Riot ID gets the new name.
    wait_until("the Lobby asks name-service again after the reset", || {
        lock(&h.st.live).known_names.contains_key(OTHER)
    })
    .await;
    assert!(
        puts(&h).await > puts_before,
        "a fresh name-service request goes out after the reset"
    );
    poller.stop_worker();
}
