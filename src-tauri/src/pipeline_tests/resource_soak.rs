//! V04's bounded local resource checks (no live Riot, no installed data).
//!
//! These are synthetic cohorts, not an endurance claim: a 100-cycle roster /
//! view cohort, same-key request coalescing under 32 callers, 800 valid image
//! cache hits, and 1/8/32 MiB image bodies. Every case has an outer timeout,
//! creates its own temporary harness, and asserts the R01/R02/R201 gates and
//! the details claim drain to free afterwards. Process working-set sampling
//! is not available in-process without a new dependency; durations and body
//! sizes are recorded instead (the plan allows an explicit measurement limit).

use std::time::{Duration, Instant};

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use opd1_riot::assets::AssetKind;

use super::*;

/// A valid 1x1 PNG (no `image` dependency in this crate).
fn png() -> Vec<u8> {
    BASE64
        .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==")
        .expect("the fixture png")
}

/// Width/height from a PNG's IHDR, after checking its signature.
fn png_dims(bytes: &[u8]) -> (u32, u32) {
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n", "not a png");
    (
        u32::from_be_bytes(bytes[16..20].try_into().unwrap()),
        u32::from_be_bytes(bytes[20..24].try_into().unwrap()),
    )
}

const CYCLE_PLAYERS: usize = 10;
/// A second Riot account for the owner switch in the lifecycle cohort.
const ALT: &str = "ffffffff-0000-0000-0000-00000000000f";

fn pid(i: usize) -> String {
    format!("{i:08}-0000-0000-0000-000000000000")
}

/// Saves `players` so an off-screen Details build has a row.
async fn save_players(h: &Harness, players: &[&str]) {
    let players: Vec<String> = players.iter().map(|p| p.to_string()).collect();
    h.st.db(move |repo| {
        for puuid in players {
            save_player(
                repo.conn(),
                &PlayerUpdate {
                    puuid,
                    name: Some(("Soak".into(), "SOAK".into())),
                    account_level: 10,
                    card_id: "card".into(),
                },
                now_ms(),
            )?;
        }
        Ok(())
    })
    .await
    .expect("the synthetic players are saved");
}

/// 100 cycles of a 10-player roster across the four stages, an owner switch
/// every fifth cycle, and a same-key Details open/refresh/close every cycle.
/// At the end all bundle/view/dump gates must be free and the last live event
/// must carry the current owner.
#[tokio::test]
async fn a_hundred_cycle_roster_and_view_cohort_drains_with_all_gates_free() {
    let h = harness().await;
    let ids: Vec<String> = (0..CYCLE_PLAYERS).map(pid).collect();
    let id_refs: Vec<&str> = ids.iter().map(String::as_str).collect();
    save_players(&h, &id_refs).await;
    let started = Instant::now();

    for cycle in 0..100 {
        let owner = if (cycle / 5) % 2 == 0 { ME } else { ALT };
        if cycle % 5 == 0 {
            lock(&h.st.live).set_account(Some(owner.to_string()));
        }
        let mut slots = Vec::with_capacity(CYCLE_PLAYERS);
        for (i, puuid) in ids.iter().enumerate() {
            let (owner, p) = (owner.to_string(), puuid.clone());
            let db =
                h.st.db(move |repo| load_player_db(repo, Some(&owner), &p))
                    .await
                    .expect("the synthetic bundle");
            slots.push(Slot {
                roster: roster(puuid, i % 3 == 0),
                db,
            });
        }
        let stage = match cycle % 4 {
            0 => Stage::Menus,
            1 => Stage::Pregame,
            2 => Stage::Ingame,
            _ => Stage::Menus,
        };
        {
            let mut live = lock(&h.st.live);
            live.stage = stage;
            live.map_path = None;
            live.map_id = None;
            live.slots = slots;
        }
        h.st.publish();
        tokio::time::timeout(Duration::from_secs(10), h.st.reload_slots(&ids))
            .await
            .expect("a bundle reload did not drain");

        // The same key, opened again every cycle: the R02 build/commit path.
        let page = tokio::time::timeout(
            Duration::from_secs(10),
            crate::commands::player_details_for(&h.st, &ids[0]),
        )
        .await
        .expect("a Details open did not drain")
        .expect("the synthetic player has a page");
        assert_eq!(page.card.puuid, ids[0]);
        tokio::time::timeout(
            Duration::from_secs(10),
            crate::details::refresh(&h.st, &ids[0]),
        )
        .await
        .expect("a Details refresh did not drain");
        crate::commands::close_details_key_for(&h.st, Some(&ids[0]));

        let last = h.live_events().last().cloned().expect("a live state");
        assert_eq!(
            last.self_puuid.as_deref(),
            Some(owner),
            "cycle {cycle}: the live state crossed owners"
        );
    }

    // A small concurrent contention phase against the same open page.
    crate::commands::player_details_for(&h.st, &ids[0])
        .await
        .expect("the page opens");
    let mut tasks = Vec::new();
    for _ in 0..8 {
        let st = h.st.clone();
        let ids = ids.clone();
        tasks.push(tokio::spawn(async move { st.reload_slots(&ids).await }));
    }
    for _ in 0..8 {
        let st = h.st.clone();
        let p = ids[0].clone();
        tasks.push(tokio::spawn(async move {
            crate::details::refresh(&st, &p).await
        }));
    }
    for task in tasks {
        tokio::time::timeout(Duration::from_secs(30), task)
            .await
            .expect("a contention task did not drain")
            .expect("a contention task panicked");
    }
    assert!(h.st.bundle_reload_is_free(), "the bundle gate leaked");
    assert!(h.st.view_builds_are_free(), "a view build gate leaked");
    assert!(h.st.dump_is_free(), "the dump gate leaked");
    let last = h.live_events().last().cloned().expect("a live state");
    // Cycle 99 is in the ALT half of the switch pattern (99 / 5 = 19, odd).
    assert_eq!(last.self_puuid.as_deref(), Some(ALT));
    println!("100-cycle cohort: {:?}", started.elapsed());
}

/// V04: 32 callers for one missing match and 32 for one missing image each
/// coalesce into exactly one download; every caller resolves and the details
/// claim/asset lock drains.
#[tokio::test]
async fn thirty_two_same_key_callers_coalesce_to_one_download() {
    let h = harness().await;
    h.st.set_session(Some(h.session.clone()));
    Mock::given(method("GET"))
        .and(path("/match-details/v1/matches/soak-1"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(details("soak-1", "competitive", 5_000))
                .set_delay(Duration::from_millis(100)),
        )
        .mount(&h.server)
        .await;

    let mut match_tasks = Vec::new();
    for _ in 0..32 {
        let st = h.st.clone();
        let session = h.session.clone();
        match_tasks.push(tokio::spawn(async move {
            crate::sync::fetch_missing_details(&st, &session, vec!["soak-1".into()]).await
        }));
    }
    for task in match_tasks {
        tokio::time::timeout(Duration::from_secs(15), task)
            .await
            .expect("a match caller did not drain")
            .expect("a match caller panicked")
            .expect("the match fetch");
    }
    let match_gets = h
        .server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.url.path() == "/match-details/v1/matches/soak-1")
        .count();
    assert_eq!(match_gets, 1, "one download for 32 callers");

    // One image, 32 callers: the per-(kind, id) async lock.
    mount_asset_tables(&h.assets_server).await;
    Mock::given(method("GET"))
        .and(path("/img/agent.png"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(png())
                .set_delay(Duration::from_millis(100)),
        )
        .mount(&h.assets_server)
        .await;
    h.st.ensure_tables(false).await.expect("the tables load");
    let agent = "aaaa1111-0000-0000-0000-000000000001";
    let mut image_tasks = Vec::new();
    for _ in 0..32 {
        let assets = h.st.assets.clone();
        image_tasks.push(tokio::spawn(async move {
            assets.ensure_image(AssetKind::Agent, agent).await
        }));
    }
    for task in image_tasks {
        let path = tokio::time::timeout(Duration::from_secs(15), task)
            .await
            .expect("an image caller did not drain")
            .expect("an image caller panicked")
            .expect("the image resolves");
        assert_eq!(png_dims(&std::fs::read(path).unwrap()), (1, 1));
    }
    let image_gets = h
        .assets_server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.url.path() == "/img/agent.png")
        .count();
    assert_eq!(image_gets, 1, "one download for 32 callers");
}

/// V04/R03: 800 valid cache hits (8 ids x 100 repeats) decode every time and
/// make no new HTTP request after the first 8.
#[tokio::test]
async fn a_hundred_repeats_of_cached_images_stay_valid_and_make_no_requests() {
    let h = harness().await;
    let mut ids = Vec::new();
    for i in 0..8 {
        let id = pid(i + 20);
        let url = format!("{}/img/soak-chroma-{i}.png", h.assets_server.uri());
        Mock::given(method("GET"))
            .and(path(format!("/v1/weapons/skinchromas/{id}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "status": 200,
                "data": {"fullRender": url}
            })))
            .mount(&h.assets_server)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("/img/soak-chroma-{i}.png")))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(png()))
            .mount(&h.assets_server)
            .await;
        ids.push(id);
    }
    for id in &ids {
        h.st.assets
            .ensure_image(AssetKind::Chroma, id)
            .await
            .expect("the image seeds");
    }
    let started = Instant::now();
    for _ in 0..100 {
        for id in &ids {
            let path = tokio::time::timeout(
                Duration::from_secs(10),
                h.st.assets.ensure_image(AssetKind::Chroma, id),
            )
            .await
            .expect("a cache hit did not drain")
            .expect("a valid cache hit");
            assert_eq!(png_dims(&std::fs::read(path).unwrap()), (1, 1));
        }
    }
    let image_gets: usize = h
        .assets_server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.url.path().contains("soak-chroma-") && r.url.path().starts_with("/img/"))
        .count();
    assert_eq!(image_gets, 8, "valid hits made no HTTP request");
    println!("800 image cache hits: {:?}", started.elapsed());
}

/// V04: a 1/8/32 MiB non-image body fails within the time ceiling (the
/// transport buffers it, R03's decode rejects it) and a later valid image
/// still heals. No byte cap is claimed; the body sizes and durations are the
/// measured boundary.
#[tokio::test]
async fn large_image_bodies_fail_safely_and_a_valid_image_still_recovers() {
    let h = harness().await;
    let sizes = [1usize << 20, 8 << 20, 32 << 20];
    let mut ids = Vec::new();
    for (i, size) in sizes.into_iter().enumerate() {
        let id = pid(i + 40);
        let url = format!("{}/img/soak-huge-{i}.png", h.assets_server.uri());
        Mock::given(method("GET"))
            .and(path(format!("/v1/weapons/skinchromas/{id}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "status": 200,
                "data": {"fullRender": url}
            })))
            .mount(&h.assets_server)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("/img/soak-huge-{i}.png")))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![0xFF; size]))
            .mount(&h.assets_server)
            .await;
        let started = Instant::now();
        let err = tokio::time::timeout(
            Duration::from_secs(15),
            h.st.assets.ensure_image(AssetKind::Chroma, &id),
        )
        .await
        .unwrap_or_else(|_| panic!("a {size}-byte body exceeded 15 s"))
        .expect_err("a non-image body must not become a hit");
        println!("{size}-byte body: {err} in {:?}", started.elapsed());
        ids.push(id);
    }

    let good = pid(50);
    let url = format!("{}/img/soak-good.png", h.assets_server.uri());
    Mock::given(method("GET"))
        .and(path(format!("/v1/weapons/skinchromas/{good}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status": 200,
            "data": {"fullRender": url}
        })))
        .mount(&h.assets_server)
        .await;
    Mock::given(method("GET"))
        .and(path("/img/soak-good.png"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(png()))
        .mount(&h.assets_server)
        .await;
    let path =
        h.st.assets
            .ensure_image(AssetKind::Chroma, &good)
            .await
            .expect("a valid image heals after the large failures");
    assert_eq!(png_dims(&std::fs::read(path).unwrap()), (1, 1));
}
