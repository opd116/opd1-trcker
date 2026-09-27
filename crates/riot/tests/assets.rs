//! Tests for `opd1_riot::assets`: bulk-table caching/refresh-on-version-
//! change, per-id (playercard/chroma) on-demand fetch+cache, and image
//! download + downscale. All HTTP is mocked with `wiremock` — nothing here
//! touches the real valorant-api.com.

use image::ImageFormat;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use opd1_riot::assets::{AssetCache, AssetKind};

const AGENT_UUID: &str = "aaaa1111-0000-0000-0000-000000000001";
const MAP_UUID: &str = "bbbb2222-0000-0000-0000-000000000001";
const CARD_UUID: &str = "cccc3333-0000-0000-0000-000000000001";
const CHROMA_UUID: &str = "dddd4444-0000-0000-0000-000000000001";
const MAP_URL: &str = "/Game/Maps/Ascent/Ascent";

fn tempdir() -> tempfile::TempDir {
    tempfile::tempdir().expect("create tempdir")
}

fn png_bytes(width: u32, height: u32) -> Vec<u8> {
    let img = image::RgbImage::from_pixel(width, height, image::Rgb([200, 50, 50]));
    let mut out = Vec::new();
    image::DynamicImage::ImageRgb8(img)
        .write_to(&mut std::io::Cursor::new(&mut out), ImageFormat::Png)
        .expect("encode png");
    out
}

fn png_dimensions(bytes: &[u8]) -> (u32, u32) {
    use image::GenericImageView;
    image::load_from_memory(bytes)
        .expect("decode png")
        .dimensions()
}

async fn mount_version(server: &MockServer, version: &str) {
    Mock::given(method("GET"))
        .and(path("/v1/version"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status": 200,
            "data": { "version": version }
        })))
        .mount(server)
        .await;
}

async fn mount_agents(server: &MockServer, times: Option<u64>) {
    let body = json!({
        "status": 200,
        "data": [{
            "uuid": AGENT_UUID,
            "displayName": "Gekko",
            "displayIcon": format!("{}/img/agent.png", server.uri()),
        }]
    });
    let mut mock = Mock::given(method("GET"))
        .and(path("/v1/agents"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body));
    if let Some(n) = times {
        mock = mock.up_to_n_times(n);
    }
    mock.mount(server).await;
}

async fn mount_maps(server: &MockServer, times: Option<u64>) {
    let body = json!({
        "status": 200,
        "data": [{
            "uuid": MAP_UUID,
            "displayName": "Ascent",
            "mapUrl": MAP_URL,
            "listViewIcon": format!("{}/img/map-list.png", server.uri()),
            "splash": format!("{}/img/map-splash.png", server.uri()),
        }]
    });
    let mut mock = Mock::given(method("GET"))
        .and(path("/v1/maps"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body));
    if let Some(n) = times {
        mock = mock.up_to_n_times(n);
    }
    mock.mount(server).await;
}

async fn mount_tiers(server: &MockServer, times: Option<u64>) {
    // Two "episode" tables — only the LAST one's tiers should end up
    // cached, proving "latest set" picks the last array entry.
    let body = json!({
        "status": 200,
        "data": [
            {
                "assetObjectName": "Episode1_CompetitiveTierDataTable",
                "tiers": [{ "tier": 3, "tierName": "OLD IRON 1", "color": "000000ff", "largeIcon": "http://example.invalid/old.png" }]
            },
            {
                "assetObjectName": "Episode2_CompetitiveTierDataTable",
                "tiers": [{
                    "tier": 3,
                    "tierName": "IRON 1",
                    "color": "4f514fff",
                    "largeIcon": format!("{}/img/tier.png", server.uri()),
                }]
            }
        ]
    });
    let mut mock = Mock::given(method("GET"))
        .and(path("/v1/competitivetiers"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body));
    if let Some(n) = times {
        mock = mock.up_to_n_times(n);
    }
    mock.mount(server).await;
}

async fn mount_seasons(server: &MockServer, times: Option<u64>) {
    let body = json!({
        "status": 200,
        "data": [
            {
                "uuid": "episode-1",
                "displayName": "EPISODE 4",
                "title": null,
                "type": null,
                "startTime": "2022-01-11T00:00:00Z",
                "endTime": "2022-06-21T00:00:00Z",
                "parentUuid": null
            },
            {
                "uuid": "act-1",
                "displayName": "ACT III",
                "title": null,
                "type": "EAresSeasonType::Act",
                "startTime": "2022-04-26T00:00:00Z",
                "endTime": "2022-06-21T00:00:00Z",
                "parentUuid": "episode-1"
            }
        ]
    });
    let mut mock = Mock::given(method("GET"))
        .and(path("/v1/seasons"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body));
    if let Some(n) = times {
        mock = mock.up_to_n_times(n);
    }
    mock.mount(server).await;
}

/// Mounts every bulk-table endpoint, each allowed to be hit only once —
/// so a test using this can prove `refresh_tables()` doesn't re-fetch by
/// simply calling it again and checking it still succeeds.
async fn mount_all_tables_once(server: &MockServer, version: &str) {
    mount_version(server, version).await;
    mount_agents(server, Some(1)).await;
    mount_maps(server, Some(1)).await;
    mount_tiers(server, Some(1)).await;
    mount_seasons(server, Some(1)).await;
}

async fn mount_all_tables(server: &MockServer, version: &str) {
    mount_version(server, version).await;
    mount_agents(server, None).await;
    mount_maps(server, None).await;
    mount_tiers(server, None).await;
    mount_seasons(server, None).await;
}

// ---------------------------------------------------------------------
// refresh_tables
// ---------------------------------------------------------------------

#[tokio::test]
async fn refresh_tables_fetches_and_persists_on_first_call() {
    let dir = tempdir();
    let server = MockServer::start().await;
    mount_all_tables_once(&server, "v1").await;

    let cache = AssetCache::with_base(dir.path().to_path_buf(), server.uri());
    let changed = cache.refresh_tables().await.unwrap();
    assert!(changed);

    for f in [
        "agents.json",
        "maps.json",
        "tiers.json",
        "seasons.json",
        "manifest.json",
    ] {
        assert!(dir.path().join(f).is_file(), "expected {f} to be written");
    }
}

#[tokio::test]
async fn refresh_tables_skips_network_when_version_is_unchanged() {
    let dir = tempdir();
    let server = MockServer::start().await;
    mount_all_tables_once(&server, "v1").await; // each bulk endpoint answers only once

    let cache = AssetCache::with_base(dir.path().to_path_buf(), server.uri());
    assert!(cache.refresh_tables().await.unwrap());

    // If this called the bulk endpoints again, they'd already be
    // exhausted (up_to_n_times(1)) and wiremock would 404, turning this
    // into an Err — so a plain `unwrap()` here is the real assertion.
    let changed_again = cache.refresh_tables().await.unwrap();
    assert!(!changed_again);

    let meta = cache.meta().unwrap();
    assert!(meta.agents.contains_key(AGENT_UUID));
}

#[tokio::test]
async fn refresh_tables_refetches_when_version_changes() {
    let dir = tempdir();
    let server = MockServer::start().await;
    // `up_to_n_times(1)` on the version mock too: wiremock prefers the
    // earliest-mounted still-available mock (see `session.rs`'s tests for
    // the same convention), so the "v1" mock must expire before the "v2"
    // one (mounted later) can ever be selected.
    Mock::given(method("GET"))
        .and(path("/v1/version"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "status": 200, "data": { "version": "v1" } })),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;
    mount_agents(&server, Some(1)).await;
    mount_maps(&server, Some(1)).await;
    mount_tiers(&server, Some(1)).await;
    mount_seasons(&server, Some(1)).await;

    let cache = AssetCache::with_base(dir.path().to_path_buf(), server.uri());
    assert!(cache.refresh_tables().await.unwrap());

    // A new version -> must re-fetch everything, so allow one more hit on
    // every bulk endpoint.
    mount_agents(&server, Some(1)).await;
    mount_maps(&server, Some(1)).await;
    mount_tiers(&server, Some(1)).await;
    mount_seasons(&server, Some(1)).await;
    Mock::given(method("GET"))
        .and(path("/v1/version"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "status": 200, "data": { "version": "v2" } })),
        )
        .mount(&server)
        .await;

    assert!(cache.refresh_tables().await.unwrap());
}

#[tokio::test]
async fn meta_and_map_uuid_lookup_require_tables_to_be_loaded() {
    let dir = tempdir();
    let cache = AssetCache::with_base(
        dir.path().to_path_buf(),
        "http://example.invalid".to_string(),
    );
    assert!(cache.meta().is_err());
    assert!(cache.map_uuid_for_path(MAP_URL).is_err());
    assert!(cache.acts().is_err());
}

#[tokio::test]
async fn meta_reflects_cached_tables_including_latest_tier_set_only() {
    let dir = tempdir();
    let server = MockServer::start().await;
    mount_all_tables(&server, "v1").await;

    let cache = AssetCache::with_base(dir.path().to_path_buf(), server.uri());
    cache.refresh_tables().await.unwrap();

    let meta = cache.meta().unwrap();
    assert_eq!(meta.agents.get(AGENT_UUID).unwrap().name, "Gekko");
    assert_eq!(meta.maps.get(MAP_UUID).unwrap().name, "Ascent");
    let tier = meta.tiers.get(&3).unwrap();
    // The *latest* (second) tiers table's name/color, not the first
    // ("OLD IRON 1" / "000000ff").
    assert_eq!(tier.name, "IRON 1");
    assert_eq!(tier.color, "#4f514fff");
}

#[tokio::test]
async fn map_uuid_for_path_matches_by_map_url() {
    let dir = tempdir();
    let server = MockServer::start().await;
    mount_all_tables(&server, "v1").await;

    let cache = AssetCache::with_base(dir.path().to_path_buf(), server.uri());
    cache.refresh_tables().await.unwrap();

    assert_eq!(
        cache.map_uuid_for_path(MAP_URL).unwrap().as_deref(),
        Some(MAP_UUID)
    );
    assert_eq!(
        cache.map_uuid_for_path("/Game/Maps/Nope/Nope").unwrap(),
        None
    );
}

#[tokio::test]
async fn acts_builds_from_the_cached_seasons_table() {
    let dir = tempdir();
    let server = MockServer::start().await;
    mount_all_tables(&server, "v1").await;

    let cache = AssetCache::with_base(dir.path().to_path_buf(), server.uri());
    cache.refresh_tables().await.unwrap();

    let acts = cache.acts().unwrap();
    assert_eq!(acts.len(), 1);
    assert_eq!(acts[0].name, "ACT III");
    assert_eq!(acts[0].episode_name, "EPISODE 4");
}

// ---------------------------------------------------------------------
// ensure_image
// ---------------------------------------------------------------------

#[tokio::test]
async fn ensure_image_downloads_once_and_downscales_agent_icon() {
    let dir = tempdir();
    let server = MockServer::start().await;
    mount_all_tables(&server, "v1").await;
    Mock::given(method("GET"))
        .and(path("/img/agent.png"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(png_bytes(300, 300)))
        .up_to_n_times(1)
        .mount(&server)
        .await;

    let cache = AssetCache::with_base(dir.path().to_path_buf(), server.uri());
    cache.refresh_tables().await.unwrap();

    let path1 = cache
        .ensure_image(AssetKind::Agent, AGENT_UUID)
        .await
        .unwrap();
    let (w, h) = png_dimensions(&std::fs::read(&path1).unwrap());
    assert_eq!(
        (w, h),
        (128, 128),
        "300x300 agent icon should downscale to 128x128"
    );

    // Second call must succeed without hitting the network again (the
    // image mock only answers once).
    let path2 = cache
        .ensure_image(AssetKind::Agent, AGENT_UUID)
        .await
        .unwrap();
    assert_eq!(path1, path2);
}

#[tokio::test]
async fn ensure_image_downscales_map_splash_to_1000_wide_preserving_aspect() {
    let dir = tempdir();
    let server = MockServer::start().await;
    mount_all_tables(&server, "v1").await;
    Mock::given(method("GET"))
        .and(path("/img/map-splash.png"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(png_bytes(1920, 1080)))
        .mount(&server)
        .await;

    let cache = AssetCache::with_base(dir.path().to_path_buf(), server.uri());
    cache.refresh_tables().await.unwrap();

    let path = cache
        .ensure_image(AssetKind::MapSplash, MAP_UUID)
        .await
        .unwrap();
    let (w, h) = png_dimensions(&std::fs::read(&path).unwrap());
    assert_eq!(w, 1000);
    assert_eq!(h, 562); // 1080 * 1000 / 1920, rounded down
}

#[tokio::test]
async fn ensure_image_does_not_downscale_map_list_icon() {
    let dir = tempdir();
    let server = MockServer::start().await;
    mount_all_tables(&server, "v1").await;
    Mock::given(method("GET"))
        .and(path("/img/map-list.png"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(png_bytes(456, 100)))
        .mount(&server)
        .await;

    let cache = AssetCache::with_base(dir.path().to_path_buf(), server.uri());
    cache.refresh_tables().await.unwrap();

    let path = cache
        .ensure_image(AssetKind::MapList, MAP_UUID)
        .await
        .unwrap();
    let (w, h) = png_dimensions(&std::fs::read(&path).unwrap());
    assert_eq!((w, h), (456, 100));
}

#[tokio::test]
async fn ensure_image_downscales_tier_icon_from_latest_set() {
    let dir = tempdir();
    let server = MockServer::start().await;
    mount_all_tables(&server, "v1").await;
    Mock::given(method("GET"))
        .and(path("/img/tier.png"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(png_bytes(256, 256)))
        .mount(&server)
        .await;

    let cache = AssetCache::with_base(dir.path().to_path_buf(), server.uri());
    cache.refresh_tables().await.unwrap();

    let path = cache.ensure_image(AssetKind::Tier, "3").await.unwrap();
    let (w, h) = png_dimensions(&std::fs::read(&path).unwrap());
    assert_eq!((w, h), (128, 128));
}

#[tokio::test]
async fn ensure_image_playercard_fetches_on_demand_and_caches_the_json_separately_from_the_image() {
    let dir = tempdir();
    let server = MockServer::start().await;
    mount_all_tables(&server, "v1").await;
    Mock::given(method("GET"))
        .and(path(format!("/v1/playercards/{CARD_UUID}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status": 200,
            "data": {
                "uuid": CARD_UUID,
                "smallArt": format!("{}/img/card-small.png", server.uri()),
                "wideArt": format!("{}/img/card-wide.png", server.uri()),
            }
        })))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/img/card-small.png"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(png_bytes(128, 128)))
        .mount(&server)
        .await;

    let cache = AssetCache::with_base(dir.path().to_path_buf(), server.uri());
    cache.refresh_tables().await.unwrap();

    let image_path = cache
        .ensure_image(AssetKind::CardSmall, CARD_UUID)
        .await
        .unwrap();
    assert!(dir
        .path()
        .join("playercards")
        .join(format!("{CARD_UUID}.json"))
        .is_file());

    // Delete just the cached image (not the playercard json) and fetch
    // again: it must succeed by reading the cached json rather than
    // re-hitting `/v1/playercards/{uuid}` (mounted `up_to_n_times(1)`).
    std::fs::remove_file(&image_path).unwrap();
    let image_path_2 = cache
        .ensure_image(AssetKind::CardSmall, CARD_UUID)
        .await
        .unwrap();
    assert_eq!(image_path, image_path_2);
    assert!(image_path_2.is_file());
}

#[tokio::test]
async fn ensure_image_chroma_uses_the_single_chroma_endpoint_not_the_bulk_weapons_table() {
    let dir = tempdir();
    let server = MockServer::start().await;
    mount_all_tables(&server, "v1").await;
    // Deliberately no mock for `/v1/weapons` at all — if `ensure_image`
    // ever hit the bulk weapons table, this test would fail with a 404.
    Mock::given(method("GET"))
        .and(path(format!("/v1/weapons/skinchromas/{CHROMA_UUID}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status": 200,
            "data": {
                "uuid": CHROMA_UUID,
                "fullRender": format!("{}/img/chroma.png", server.uri()),
            }
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/img/chroma.png"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(png_bytes(512, 148)))
        .mount(&server)
        .await;

    let cache = AssetCache::with_base(dir.path().to_path_buf(), server.uri());
    cache.refresh_tables().await.unwrap();

    let path = cache
        .ensure_image(AssetKind::Chroma, CHROMA_UUID)
        .await
        .unwrap();
    let (w, h) = png_dimensions(&std::fs::read(&path).unwrap());
    assert_eq!((w, h), (512, 148));
}

#[tokio::test]
async fn ensure_image_returns_not_found_for_an_unknown_agent_uuid() {
    let dir = tempdir();
    let server = MockServer::start().await;
    mount_all_tables(&server, "v1").await;

    let cache = AssetCache::with_base(dir.path().to_path_buf(), server.uri());
    cache.refresh_tables().await.unwrap();

    let err = cache
        .ensure_image(AssetKind::Agent, "not-a-real-uuid")
        .await
        .unwrap_err();
    assert!(matches!(err, opd1_riot::assets::AssetError::NotFound));
}
