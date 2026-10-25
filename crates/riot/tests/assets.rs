//! Tests for `opd1_riot::assets`: bulk-table caching/refresh-on-version-
//! change, per-id (playercard/chroma) on-demand fetch+cache, and image
//! download + downscale. All HTTP is mocked with `wiremock` — nothing here
//! touches the real valorant-api.com.

use image::ImageFormat;
use serde_json::{json, Value};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use opd1_core::model::WeaponSkin;
use opd1_riot::assets::{AssetCache, AssetError, AssetKind};

const AGENT_UUID: &str = "aaaa1111-0000-0000-0000-000000000001";
const MAP_UUID: &str = "bbbb2222-0000-0000-0000-000000000001";
const CARD_UUID: &str = "cccc3333-0000-0000-0000-000000000001";
const CHROMA_UUID: &str = "dddd4444-0000-0000-0000-000000000001";
const MAP_URL: &str = "/Game/Maps/Ascent/Ascent";
/// valorant-api's Premium Edition content tier.
const CONTENT_TIER_UUID: &str = "60bca009-4182-7998-dee7-b8a2558dc369";
/// The Reaver Vandal skin and its chromas
/// (`tests/fixtures/weapon_skin_reaver_vandal.json`, captured from the live API).
const SKIN_UUID: &str = "30388628-42f0-606c-82c0-73ad43de997f";
const BASE_CHROMA: &str = "2bd28382-48c6-8579-83e8-e9b64b783de3";
const RED_CHROMA: &str = "b2a065c0-4632-ccaa-f496-7681dc2a6185";
const BLACK_CHROMA: &str = "db4461e5-40dd-1173-3b64-e5836f92f4dd";

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

async fn mount_content_tiers(server: &MockServer, times: Option<u64>) {
    let body = json!({
        "status": 200,
        "data": [{
            "uuid": CONTENT_TIER_UUID,
            "displayName": "Premium Edition",
            "devName": "Premium",
            "displayIcon": format!("{}/img/content-tier.png", server.uri()),
        }]
    });
    let mut mock = Mock::given(method("GET"))
        .and(path("/v1/contenttiers"))
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
    mount_content_tiers(server, Some(1)).await;
}

async fn mount_all_tables(server: &MockServer, version: &str) {
    mount_version(server, version).await;
    mount_agents(server, None).await;
    mount_maps(server, None).await;
    mount_tiers(server, None).await;
    mount_seasons(server, None).await;
    mount_content_tiers(server, None).await;
}

/// Mounts `/v1/playercards/{CARD_UUID}` with small/wide art on this server
/// and `large_art` as the `largeArt` value (a URL string, or `Value::Null`
/// for a card that has none).
async fn mount_playercard(server: &MockServer, large_art: Value) {
    Mock::given(method("GET"))
        .and(path(format!("/v1/playercards/{CARD_UUID}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status": 200,
            "data": {
                "uuid": CARD_UUID,
                "smallArt": format!("{}/img/card-small.png", server.uri()),
                "wideArt": format!("{}/img/card-wide.png", server.uri()),
                "largeArt": large_art,
            }
        })))
        .mount(server)
        .await;
}

/// How many times `/v1/playercards/{CARD_UUID}` has been requested.
async fn playercard_requests(server: &MockServer) -> usize {
    server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.url.path() == format!("/v1/playercards/{CARD_UUID}"))
        .count()
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
        "contenttiers.json",
        "manifest.json",
    ] {
        assert!(dir.path().join(f).is_file(), "expected {f} to be written");
    }
}

#[tokio::test]
async fn a_gzip_encoded_table_answer_is_decoded() {
    // The valorant-api clients keep gzip on (large JSON tables, an ordinary
    // CDN): a compressed answer must decode, and be written decoded.
    use std::io::Write;
    let dir = tempdir();
    let server = MockServer::start().await;
    mount_version(&server, "v1").await;
    mount_maps(&server, None).await;
    mount_tiers(&server, None).await;
    mount_seasons(&server, None).await;
    mount_content_tiers(&server, None).await;
    let agents = json!({
        "status": 200,
        "data": [{
            "uuid": AGENT_UUID,
            "displayName": "Gekko",
            "displayIcon": format!("{}/img/agent.png", server.uri()),
        }]
    });
    let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    enc.write_all(agents.to_string().as_bytes()).unwrap();
    Mock::given(method("GET"))
        .and(path("/v1/agents"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-encoding", "gzip")
                .set_body_bytes(enc.finish().unwrap()),
        )
        .mount(&server)
        .await;

    let cache = AssetCache::with_base(dir.path().to_path_buf(), server.uri());
    assert!(cache.refresh_tables().await.unwrap());
    // Decoded before it was parsed and cached.
    let meta = cache.meta().unwrap();
    assert_eq!(meta.agents.get(AGENT_UUID).unwrap().name, "Gekko");
    // The request said it accepts gzip.
    let asked = server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .find(|r| r.url.path() == "/v1/agents")
        .unwrap();
    let accept = asked
        .headers
        .get("accept-encoding")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(accept.contains("gzip"), "{accept}");
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
    mount_content_tiers(&server, Some(1)).await;

    let cache = AssetCache::with_base(dir.path().to_path_buf(), server.uri());
    assert!(cache.refresh_tables().await.unwrap());

    // A new version -> must re-fetch everything, so allow one more hit on
    // every bulk endpoint.
    mount_agents(&server, Some(1)).await;
    mount_maps(&server, Some(1)).await;
    mount_tiers(&server, Some(1)).await;
    mount_seasons(&server, Some(1)).await;
    mount_content_tiers(&server, Some(1)).await;
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

/// Fetches the map list image for [`MAP_UUID`] from a mock server serving a
/// `width`x`height` PNG, and returns the stored file's dimensions.
async fn stored_map_list_dimensions(width: u32, height: u32) -> (u32, u32) {
    let dir = tempdir();
    let server = MockServer::start().await;
    mount_all_tables(&server, "v1").await;
    Mock::given(method("GET"))
        .and(path("/img/map-list.png"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(png_bytes(width, height)))
        .mount(&server)
        .await;

    let cache = AssetCache::with_base(dir.path().to_path_buf(), server.uri());
    cache.refresh_tables().await.unwrap();

    let path = cache
        .ensure_image(AssetKind::MapList, MAP_UUID)
        .await
        .unwrap();
    png_dimensions(&std::fs::read(&path).unwrap())
}

#[tokio::test]
async fn ensure_image_downscales_map_list_icon_to_384_wide_preserving_aspect() {
    // The real listViewIcon of 25 maps (valorant-api, measured): 456x100.
    // 100 * 384 / 456 = 84.2, rounded down: 84 device px high, the 56 css px
    // of the match header's 150x56 cover box at 150% Windows scaling.
    assert_eq!(stored_map_list_dimensions(456, 100).await, (384, 84));
}

#[tokio::test]
async fn ensure_image_downscales_the_huge_map_list_icons_of_the_range_and_basic_training() {
    // Those two maps' listViewIcon is 3840x2160 (33MB decoded as it is).
    assert_eq!(stored_map_list_dimensions(3840, 2160).await, (384, 216));
}

#[tokio::test]
async fn ensure_image_keeps_a_map_list_icon_that_is_not_wider_than_the_target() {
    assert_eq!(stored_map_list_dimensions(300, 66).await, (300, 66));
    assert_eq!(stored_map_list_dimensions(384, 84).await, (384, 84));
}

#[tokio::test]
async fn ensure_image_serves_a_map_list_icon_cached_at_its_original_size_untouched() {
    // A file an older version cached as-is (456x100): found on disk, served
    // as it is, no table lookup and no request. The target applies to a
    // download only.
    let dir = tempdir();
    let server = MockServer::start().await;
    let cached = dir
        .path()
        .join("images")
        .join("mapList")
        .join(format!("{MAP_UUID}.png"));
    std::fs::create_dir_all(cached.parent().unwrap()).unwrap();
    let original = png_bytes(456, 100);
    std::fs::write(&cached, &original).unwrap();

    let cache = AssetCache::with_base(dir.path().to_path_buf(), server.uri());
    let path = cache
        .ensure_image(AssetKind::MapList, MAP_UUID)
        .await
        .unwrap();

    assert_eq!(path, cached);
    assert_eq!(std::fs::read(&path).unwrap(), original);
    assert!(requested_paths(&server).await.is_empty());
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
async fn card_large_uses_large_art_downscaled_to_160() {
    let dir = tempdir();
    let server = MockServer::start().await;
    mount_all_tables(&server, "v1").await;
    mount_playercard(
        &server,
        json!(format!("{}/img/card-large.png", server.uri())),
    )
    .await;
    Mock::given(method("GET"))
        .and(path("/img/card-large.png"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(png_bytes(512, 1024)))
        .mount(&server)
        .await;

    let cache = AssetCache::with_base(dir.path().to_path_buf(), server.uri());
    cache.refresh_tables().await.unwrap();

    let path = cache
        .ensure_image(AssetKind::CardLarge, CARD_UUID)
        .await
        .unwrap();
    assert!(path.starts_with(dir.path().join("images").join("cardLarge")));
    let (w, h) = png_dimensions(&std::fs::read(&path).unwrap());
    assert_eq!((w, h), (160, 320), "160px wide, aspect ratio kept");
}

#[tokio::test]
async fn playercard_cache_without_large_art_refetches() {
    let dir = tempdir();
    let server = MockServer::start().await;
    mount_all_tables(&server, "v1").await;
    mount_playercard(
        &server,
        json!(format!("{}/img/card-large.png", server.uri())),
    )
    .await;
    Mock::given(method("GET"))
        .and(path("/img/card-large.png"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(png_bytes(512, 1024)))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/img/card-small.png"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(png_bytes(128, 128)))
        .mount(&server)
        .await;

    // A playercard json cached by an earlier build: no large art in it.
    let cached = dir
        .path()
        .join("playercards")
        .join(format!("{CARD_UUID}.json"));
    std::fs::create_dir_all(cached.parent().unwrap()).unwrap();
    std::fs::write(
        &cached,
        json!({
            "small_art": format!("{}/img/card-small.png", server.uri()),
            "wide_art": format!("{}/img/card-wide.png", server.uri()),
        })
        .to_string(),
    )
    .unwrap();

    let cache = AssetCache::with_base(dir.path().to_path_buf(), server.uri());
    cache.refresh_tables().await.unwrap();

    // Small art is served from the old json as before: no refetch for it.
    cache
        .ensure_image(AssetKind::CardSmall, CARD_UUID)
        .await
        .unwrap();
    assert_eq!(playercard_requests(&server).await, 0);

    let path = cache
        .ensure_image(AssetKind::CardLarge, CARD_UUID)
        .await
        .unwrap();
    assert!(path.is_file());
    assert_eq!(
        playercard_requests(&server).await,
        1,
        "the stale json is fetched once more"
    );

    // The rewritten json has the large art: no third fetch.
    std::fs::remove_file(&path).unwrap();
    cache
        .ensure_image(AssetKind::CardLarge, CARD_UUID)
        .await
        .unwrap();
    assert_eq!(playercard_requests(&server).await, 1);
}

#[tokio::test]
async fn card_without_large_art_shows_nothing_and_never_breaks_small_art() {
    // R107: a card whose valorant-api entry has no `largeArt` has no large
    // image — not another kind's — and its small art still works.
    let dir = tempdir();
    let server = MockServer::start().await;
    mount_all_tables(&server, "v1").await;
    mount_playercard(&server, Value::Null).await;
    Mock::given(method("GET"))
        .and(path("/img/card-small.png"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(png_bytes(128, 128)))
        .mount(&server)
        .await;

    let cache = AssetCache::with_base(dir.path().to_path_buf(), server.uri());
    cache.refresh_tables().await.unwrap();

    for _ in 0..2 {
        let err = cache
            .ensure_image(AssetKind::CardLarge, CARD_UUID)
            .await
            .unwrap_err();
        assert!(
            matches!(err, opd1_riot::assets::AssetError::NotFound),
            "{err:?}"
        );
    }
    assert_eq!(
        playercard_requests(&server).await,
        1,
        "one fetch, not one per request"
    );
    assert!(!dir.path().join("images").join("cardLarge").exists());

    let small = cache
        .ensure_image(AssetKind::CardSmall, CARD_UUID)
        .await
        .unwrap();
    assert!(small.is_file());
    assert_eq!(
        playercard_requests(&server).await,
        1,
        "small art reads the cached json"
    );
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
    // Shown at most 108x38: the 512px render is downscaled to 256 wide.
    assert_eq!((w, h), (256, 74));
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

// ---------------------------------------------------------------------
// Fix round 1, item 4: `ensure_image` rejects path-traversal-shaped ids
// before ever building a path or hitting the network.
// ---------------------------------------------------------------------

#[tokio::test]
async fn ensure_image_rejects_path_traversal_attempts_without_touching_disk_or_network() {
    let dir = tempdir();
    let server = MockServer::start().await;
    mount_all_tables(&server, "v1").await;
    // Deliberately no image/per-id mocks mounted — if `ensure_image` ever
    // tried to resolve or download a traversal id, this test would fail
    // with a connection/404 error rather than the expected `NotFound`.

    let cache = AssetCache::with_base(dir.path().to_path_buf(), server.uri());
    cache.refresh_tables().await.unwrap();

    for kind in [
        AssetKind::Agent,
        AssetKind::CardSmall,
        AssetKind::CardWide,
        AssetKind::CardLarge,
        AssetKind::Chroma,
        AssetKind::ContentTier,
        AssetKind::MapList,
        AssetKind::MapSplash,
    ] {
        for malicious in [
            "../../../etc/passwd",
            "..%2f..%2fetc%2fpasswd",
            "a/../../b",
            "",
        ] {
            let err = cache.ensure_image(kind, malicious).await.unwrap_err();
            assert!(
                matches!(err, opd1_riot::assets::AssetError::NotFound),
                "expected NotFound for {kind:?}/{malicious:?}, got {err:?}"
            );
        }
    }

    // Nothing must have been written under the cache dir as a result of
    // these attempts (validation runs before any path is even built, so
    // there is nothing to write in the first place).
    let images_dir = dir.path().join("images");
    if images_dir.is_dir() {
        let written = walk_files(&images_dir);
        assert!(
            written.is_empty(),
            "unexpected files written during traversal attempts: {written:?}"
        );
    }
}

fn walk_files(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                out.extend(walk_files(&path));
            } else {
                out.push(path);
            }
        }
    }
    out
}

// ---------------------------------------------------------------------
// Fix round 1, item 5: concurrent `ensure_image` calls for the same
// (kind, id) are coalesced into exactly one download.
// ---------------------------------------------------------------------

#[tokio::test]
async fn ensure_image_deduplicates_concurrent_requests_for_the_same_asset() {
    let dir = tempdir();
    let server = MockServer::start().await;
    mount_all_tables(&server, "v1").await;
    // A delay widens the race window so concurrent callers are actually
    // likely to overlap while the first download is still in flight.
    Mock::given(method("GET"))
        .and(path("/img/agent.png"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(png_bytes(64, 64))
                .set_delay(std::time::Duration::from_millis(50)),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;

    let cache = std::sync::Arc::new(AssetCache::with_base(
        dir.path().to_path_buf(),
        server.uri(),
    ));
    cache.refresh_tables().await.unwrap();

    let mut handles = Vec::new();
    for _ in 0..10 {
        let cache = cache.clone();
        handles.push(tokio::spawn(async move {
            cache.ensure_image(AssetKind::Agent, AGENT_UUID).await
        }));
    }

    for handle in handles {
        // If a second request had ever reached the server, it would hit
        // the exhausted (`up_to_n_times(1)`) mock and come back as an
        // error here, failing this unwrap.
        handle.await.unwrap().unwrap();
    }
}

// ---------------------------------------------------------------------
// Fix round 1, item 6: offline fallback — `refresh_tables` loads from
// disk when `/v1/version` can't be reached, as long as a full cache
// already exists there.
// ---------------------------------------------------------------------

#[tokio::test]
async fn refresh_tables_falls_back_to_disk_cache_when_version_is_unreachable() {
    let dir = tempdir();

    // First, populate a real on-disk cache against a working server.
    let server = MockServer::start().await;
    mount_all_tables(&server, "v1").await;
    let online_cache = AssetCache::with_base(dir.path().to_path_buf(), server.uri());
    assert!(online_cache.refresh_tables().await.unwrap());

    // Now build a *new* cache instance, same directory, pointed at an
    // address nothing is listening on — a real connection failure, not a
    // mocked one.
    let offline_cache =
        AssetCache::with_base(dir.path().to_path_buf(), "http://127.0.0.1:1".to_string());
    let changed = offline_cache.refresh_tables().await.unwrap();
    assert!(!changed, "offline fallback shouldn't report a fresh fetch");

    // meta/acts/map lookups must work fully offline off that fallback.
    let meta = offline_cache.meta().unwrap();
    assert!(meta.agents.contains_key(AGENT_UUID));
    assert_eq!(
        offline_cache.map_uuid_for_path(MAP_URL).unwrap().as_deref(),
        Some(MAP_UUID)
    );
    // (acts() just must not error offline; the seasons fixture here has 1 act.)
    assert_eq!(offline_cache.acts().unwrap().len(), 1);
}

#[tokio::test]
async fn refresh_tables_fails_when_offline_and_no_disk_cache_exists() {
    let dir = tempdir();
    let cache = AssetCache::with_base(dir.path().to_path_buf(), "http://127.0.0.1:1".to_string());
    assert!(cache.refresh_tables().await.is_err());
}

// ---------------------------------------------------------------------
// Task 15: content-tier icons, chroma downscale, weapon skins (R83, R97)
// ---------------------------------------------------------------------

#[tokio::test]
async fn content_tier_icon_downscaled_to_32() {
    let dir = tempdir();
    let server = MockServer::start().await;
    mount_all_tables(&server, "v1").await;
    Mock::given(method("GET"))
        .and(path("/img/content-tier.png"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(png_bytes(256, 256)))
        .mount(&server)
        .await;

    let cache = AssetCache::with_base(dir.path().to_path_buf(), server.uri());
    cache.refresh_tables().await.unwrap();

    let path = cache
        .ensure_image(AssetKind::ContentTier, CONTENT_TIER_UUID)
        .await
        .unwrap();
    assert!(
        path.ends_with(format!("contentTier/{CONTENT_TIER_UUID}.png")),
        "{path:?}"
    );
    let (w, h) = png_dimensions(&std::fs::read(&path).unwrap());
    assert_eq!((w, h), (32, 32));

    // A tier the table doesn't list has no icon.
    let err = cache
        .ensure_image(
            AssetKind::ContentTier,
            "aaaaaaaa-0000-0000-0000-000000000001",
        )
        .await
        .unwrap_err();
    assert!(matches!(err, AssetError::NotFound), "{err:?}");
}

#[tokio::test]
async fn content_tier_needs_the_tables() {
    let dir = tempdir();
    let cache = AssetCache::with_base(dir.path().to_path_buf(), "http://127.0.0.1:1".into());
    let err = cache
        .ensure_image(AssetKind::ContentTier, CONTENT_TIER_UUID)
        .await
        .unwrap_err();
    assert!(matches!(err, AssetError::TablesNotLoaded), "{err:?}");
}

#[tokio::test]
async fn chroma_downscaled_to_256() {
    let dir = tempdir();
    let server = MockServer::start().await;
    mount_all_tables(&server, "v1").await;
    Mock::given(method("GET"))
        .and(path(format!("/v1/weapons/skinchromas/{CHROMA_UUID}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status": 200,
            "data": {"uuid": CHROMA_UUID, "fullRender": format!("{}/img/chroma.png", server.uri())}
        })))
        .mount(&server)
        .await;
    // A wide render (as valorant-api serves) and a small one that is
    // already at or under the target.
    Mock::given(method("GET"))
        .and(path("/img/chroma.png"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(png_bytes(1024, 300)))
        .mount(&server)
        .await;

    let cache = AssetCache::with_base(dir.path().to_path_buf(), server.uri());
    cache.refresh_tables().await.unwrap();
    let path = cache
        .ensure_image(AssetKind::Chroma, CHROMA_UUID)
        .await
        .unwrap();
    let (w, h) = png_dimensions(&std::fs::read(&path).unwrap());
    assert_eq!((w, h), (256, 75), "1024x300 -> 256 wide, aspect kept");
}

fn skin_fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/weapon_skin_reaver_vandal.json")).unwrap()
}

async fn mount_skin(server: &MockServer, times: Option<u64>) {
    let mut mock = Mock::given(method("GET"))
        .and(path(format!("/v1/weapons/skins/{SKIN_UUID}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(skin_fixture()));
    if let Some(n) = times {
        mock = mock.up_to_n_times(n);
    }
    mock.mount(server).await;
}

async fn skin_requests(server: &MockServer) -> usize {
    server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.url.path().starts_with("/v1/weapons/skins"))
        .count()
}

#[tokio::test]
async fn weapon_skin_names_and_tier() {
    let dir = tempdir();
    let server = MockServer::start().await;
    mount_skin(&server, None).await;
    let cache = AssetCache::with_base(dir.path().to_path_buf(), server.uri());

    // The skin's name minus the weapon word, the `X` of `(Variant N X)`,
    // and the skin's content tier straight from the response.
    let red = cache.weapon_skin(SKIN_UUID, RED_CHROMA).await.unwrap();
    assert_eq!(
        red,
        WeaponSkin {
            chroma_id: RED_CHROMA.into(),
            skin_name: "Reaver".into(),
            chroma_name: Some("Red".into()),
            tier_id: Some(CONTENT_TIER_UUID.into()),
        }
    );
    assert_eq!(
        skin_fixture()["data"]["contentTierUuid"],
        json!(CONTENT_TIER_UUID)
    );
    // The base chroma has no variant name.
    let base = cache.weapon_skin(SKIN_UUID, BASE_CHROMA).await.unwrap();
    assert_eq!(
        (base.skin_name.as_str(), base.chroma_name),
        ("Reaver", None)
    );
    assert_eq!(base.tier_id.as_deref(), Some(CONTENT_TIER_UUID));
    let black = cache.weapon_skin(SKIN_UUID, BLACK_CHROMA).await.unwrap();
    assert_eq!(black.chroma_name.as_deref(), Some("Black"));

    // Ids in any case name the same skin; the chroma id comes back
    // lower-case, like the loadout extraction stores it.
    let upper = cache
        .weapon_skin(&SKIN_UUID.to_uppercase(), &RED_CHROMA.to_uppercase())
        .await
        .unwrap();
    assert_eq!(upper, red);

    // A chroma the skin doesn't list is still that skin, unnamed.
    let unknown = cache
        .weapon_skin(SKIN_UUID, "aaaaaaaa-0000-0000-0000-000000000001")
        .await
        .unwrap();
    assert_eq!(unknown.skin_name, "Reaver");
    assert_eq!(unknown.chroma_name, None);
}

#[tokio::test]
async fn weapon_skin_without_a_content_tier_has_none() {
    // Default and battle-pass skins have `contentTierUuid: null`.
    let dir = tempdir();
    let server = MockServer::start().await;
    let mut body = skin_fixture();
    body["data"]["contentTierUuid"] = Value::Null;
    body["data"]["displayName"] = json!("Standard Vandal");
    Mock::given(method("GET"))
        .and(path(format!("/v1/weapons/skins/{SKIN_UUID}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(&server)
        .await;
    let cache = AssetCache::with_base(dir.path().to_path_buf(), server.uri());
    let skin = cache.weapon_skin(SKIN_UUID, BASE_CHROMA).await.unwrap();
    assert_eq!(skin.skin_name, "Standard");
    assert_eq!(skin.tier_id, None);
}

#[tokio::test]
async fn weapon_skin_fetched_once_then_cached() {
    let dir = tempdir();
    let server = MockServer::start().await;
    mount_skin(&server, None).await;
    let cache = AssetCache::with_base(dir.path().to_path_buf(), server.uri());
    assert_eq!(cache.weapon_skin_cached(SKIN_UUID, RED_CHROMA), None);

    let first = cache.weapon_skin(SKIN_UUID, RED_CHROMA).await.unwrap();
    // Another chroma of the same skin, and a repeat: no second request.
    cache.weapon_skin(SKIN_UUID, BLACK_CHROMA).await.unwrap();
    assert_eq!(
        cache.weapon_skin(SKIN_UUID, RED_CHROMA).await.unwrap(),
        first
    );
    assert_eq!(skin_requests(&server).await, 1, "one request per skin");
    // ... and `weapon_skin_cached` now answers from memory.
    assert_eq!(
        cache.weapon_skin_cached(SKIN_UUID, RED_CHROMA),
        Some(first.clone())
    );
    assert_eq!(
        cache
            .weapon_skin_cached(SKIN_UUID, BLACK_CHROMA)
            .and_then(|s| s.chroma_name)
            .as_deref(),
        Some("Black")
    );

    // A new cache on the same directory starts with an empty memory map:
    // `weapon_skin_cached` knows nothing until `weapon_skin` has run, and
    // that reads the skin's JSON from disk — no request.
    let fresh = AssetCache::with_base(dir.path().to_path_buf(), server.uri());
    assert_eq!(fresh.weapon_skin_cached(SKIN_UUID, RED_CHROMA), None);
    assert_eq!(
        fresh.weapon_skin(SKIN_UUID, RED_CHROMA).await.unwrap(),
        first
    );
    assert_eq!(skin_requests(&server).await, 1, "served from the disk copy");
    assert_eq!(fresh.weapon_skin_cached(SKIN_UUID, RED_CHROMA), Some(first));

    // The per-skin copy on disk is a compact extract, not the response.
    let on_disk =
        std::fs::read(dir.path().join("skins").join(format!("{SKIN_UUID}.json"))).unwrap();
    assert!(on_disk.len() < 1024, "{} bytes", on_disk.len());
}

#[tokio::test]
async fn weapon_skin_cached_is_none_before_fetch() {
    let dir = tempdir();
    let server = MockServer::start().await;
    mount_skin(&server, None).await;
    let cache = AssetCache::with_base(dir.path().to_path_buf(), server.uri());
    assert_eq!(cache.weapon_skin_cached(SKIN_UUID, RED_CHROMA), None);
    // Never a request or a disk read: it is a memory lookup only (R37).
    assert_eq!(cache.weapon_skin_cached("../etc/passwd", RED_CHROMA), None);
    assert_eq!(skin_requests(&server).await, 0);
}

#[tokio::test]
async fn weapon_skin_failure_is_not_cached() {
    // R97/R83: a failed fetch is retried on the next attempt.
    let dir = tempdir();
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/v1/weapons/skins/{SKIN_UUID}")))
        .respond_with(ResponseTemplate::new(500))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    mount_skin(&server, None).await;
    let cache = AssetCache::with_base(dir.path().to_path_buf(), server.uri());

    let err = cache.weapon_skin(SKIN_UUID, RED_CHROMA).await.unwrap_err();
    assert!(matches!(err, AssetError::Http(500)), "{err:?}");
    assert_eq!(cache.weapon_skin_cached(SKIN_UUID, RED_CHROMA), None);
    assert!(!dir
        .path()
        .join("skins")
        .join(format!("{SKIN_UUID}.json"))
        .exists());

    let ok = cache.weapon_skin(SKIN_UUID, RED_CHROMA).await.unwrap();
    assert_eq!(ok.skin_name, "Reaver");
    assert_eq!(skin_requests(&server).await, 2);
}

#[tokio::test]
async fn weapon_skin_rejects_malformed_ids_without_a_request() {
    let dir = tempdir();
    let server = MockServer::start().await;
    mount_skin(&server, None).await;
    let cache = AssetCache::with_base(dir.path().to_path_buf(), server.uri());
    for (skin, chroma) in [
        ("../../etc/passwd", RED_CHROMA),
        (SKIN_UUID, "../x"),
        ("", RED_CHROMA),
        (SKIN_UUID, ""),
        ("a/b", "c/d"),
    ] {
        let err = cache.weapon_skin(skin, chroma).await.unwrap_err();
        assert!(
            matches!(err, AssetError::NotFound),
            "{skin:?} {chroma:?}: {err:?}"
        );
    }
    assert_eq!(skin_requests(&server).await, 0);
    assert!(!dir.path().join("skins").exists());
}

#[tokio::test]
async fn concurrent_weapon_skin_calls_share_one_request() {
    let dir = tempdir();
    let server = MockServer::start().await;
    mount_skin(&server, None).await;
    let cache = std::sync::Arc::new(AssetCache::with_base(
        dir.path().to_path_buf(),
        server.uri(),
    ));
    let calls: Vec<_> = (0..6)
        .map(|_| {
            let cache = cache.clone();
            tokio::spawn(async move { cache.weapon_skin(SKIN_UUID, RED_CHROMA).await })
        })
        .collect();
    for call in calls {
        assert_eq!(call.await.unwrap().unwrap().skin_name, "Reaver");
    }
    assert_eq!(skin_requests(&server).await, 1);
}

/// The paths a mock server has been asked for, in order.
async fn requested_paths(server: &MockServer) -> Vec<String> {
    server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .map(|r| r.url.path().to_string())
        .collect()
}

/// A cache written before content tiers existed: every table current, no
/// `contenttiers.json`.
async fn pre_upgrade_cache_dir() -> tempfile::TempDir {
    let dir = tempdir();
    let server = MockServer::start().await;
    mount_all_tables(&server, "v1").await;
    let cache = AssetCache::with_base(dir.path().to_path_buf(), server.uri());
    assert!(cache.refresh_tables().await.unwrap());
    std::fs::remove_file(dir.path().join("contenttiers.json")).unwrap();
    dir
}

#[tokio::test]
async fn a_missing_content_tier_file_refetches_only_the_content_tiers() {
    let dir = pre_upgrade_cache_dir().await;

    // The server knows the version and content tiers only: a request for
    // agents, maps, tiers or seasons would 404 and fail the refresh.
    let server = MockServer::start().await;
    mount_version(&server, "v1").await;
    mount_content_tiers(&server, None).await;
    let cache = AssetCache::with_base(dir.path().to_path_buf(), server.uri());
    assert!(
        cache.refresh_tables().await.unwrap(),
        "something downloaded"
    );

    assert_eq!(
        requested_paths(&server).await,
        ["/v1/version", "/v1/contenttiers"],
        "one call for the version, one for the missing table, nothing else"
    );
    assert!(dir.path().join("contenttiers.json").is_file());
    // The other four tables came from disk, and the new one is in memory.
    assert!(cache.meta().unwrap().agents.contains_key(AGENT_UUID));
    assert_eq!(
        cache.map_uuid_for_path(MAP_URL).unwrap().as_deref(),
        Some(MAP_UUID)
    );
    Mock::given(method("GET"))
        .and(path("/img/content-tier.png"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(png_bytes(64, 64)))
        .mount(&server)
        .await;
    cache
        .ensure_image(AssetKind::ContentTier, CONTENT_TIER_UUID)
        .await
        .expect("the fetched content tier is known");

    // Now the cache is complete: the next start is version-only.
    let again_server = MockServer::start().await;
    mount_version(&again_server, "v1").await;
    let again = AssetCache::with_base(dir.path().to_path_buf(), again_server.uri());
    assert!(!again.refresh_tables().await.unwrap());
    assert_eq!(requested_paths(&again_server).await, ["/v1/version"]);
}

#[tokio::test]
async fn a_failed_content_tier_fetch_still_loads_the_other_tables() {
    let dir = pre_upgrade_cache_dir().await;

    // The version answers but `/v1/contenttiers` does not (404): the four
    // current tables load from disk anyway, so the session is not left
    // without tables until the retry; the file stays missing for that retry.
    let server = MockServer::start().await;
    mount_version(&server, "v1").await;
    let cache = AssetCache::with_base(dir.path().to_path_buf(), server.uri());
    assert!(!cache.refresh_tables().await.unwrap(), "nothing downloaded");
    assert!(cache.meta().unwrap().agents.contains_key(AGENT_UUID));
    assert!(cache.acts().is_ok());
    assert!(!dir.path().join("contenttiers.json").is_file());

    // The retry, with the endpoint back, fetches just that table.
    let retry_server = MockServer::start().await;
    mount_version(&retry_server, "v1").await;
    mount_content_tiers(&retry_server, None).await;
    let retry = AssetCache::with_base(dir.path().to_path_buf(), retry_server.uri());
    assert!(retry.refresh_tables().await.unwrap());
    assert_eq!(
        requested_paths(&retry_server).await,
        ["/v1/version", "/v1/contenttiers"]
    );
    assert!(dir.path().join("contenttiers.json").is_file());
}

#[tokio::test]
async fn refresh_tables_refetches_when_the_content_tier_file_is_missing() {
    // A cache written before content tiers existed has every other table
    // but no `contenttiers.json`: an unchanged version still refetches once.
    let dir = tempdir();
    let server = MockServer::start().await;
    mount_all_tables(&server, "v1").await;
    let cache = AssetCache::with_base(dir.path().to_path_buf(), server.uri());
    assert!(cache.refresh_tables().await.unwrap());
    std::fs::remove_file(dir.path().join("contenttiers.json")).unwrap();

    let again = AssetCache::with_base(dir.path().to_path_buf(), server.uri());
    assert!(again.refresh_tables().await.unwrap(), "refetched");
    assert!(dir.path().join("contenttiers.json").is_file());

    // Offline, the same legacy cache still loads what it has (names, acts,
    // maps) — the missing table only means no content-tier icons yet.
    std::fs::remove_file(dir.path().join("contenttiers.json")).unwrap();
    let offline = AssetCache::with_base(dir.path().to_path_buf(), "http://127.0.0.1:1".into());
    assert!(!offline.refresh_tables().await.unwrap());
    assert!(offline.meta().unwrap().agents.contains_key(AGENT_UUID));
    let err = offline
        .ensure_image(AssetKind::ContentTier, CONTENT_TIER_UUID)
        .await
        .unwrap_err();
    assert!(matches!(err, AssetError::NotFound), "{err:?}");
}

#[tokio::test]
async fn a_failed_table_fetch_after_a_patch_keeps_the_disk_tables_and_is_retried() {
    // After a patch the version changes and every table is refetched. When
    // `/v1/version` answers but one table does not (here seasons: 500), the
    // old tables on disk are loaded rather than leaving the app with none
    // (no acts, no agents), and the next refresh tries again.
    let dir = tempdir();
    let first = MockServer::start().await;
    mount_all_tables(&first, "v1").await;
    let old = AssetCache::with_base(dir.path().to_path_buf(), first.uri());
    assert!(old.refresh_tables().await.unwrap());
    assert!(!old.tables_stale());

    let patched = MockServer::start().await;
    mount_version(&patched, "v2").await;
    mount_agents(&patched, None).await;
    mount_maps(&patched, None).await;
    mount_tiers(&patched, None).await;
    mount_content_tiers(&patched, None).await;
    Mock::given(method("GET"))
        .and(path("/v1/seasons"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&patched)
        .await;
    let cache = AssetCache::with_base(dir.path().to_path_buf(), patched.uri());
    assert!(
        !cache.refresh_tables().await.unwrap(),
        "nothing new was stored"
    );
    assert!(cache.meta().unwrap().agents.contains_key(AGENT_UUID));
    assert!(!cache.acts().unwrap().is_empty(), "the old acts are there");
    assert!(cache.tables_stale(), "the refetch is still owed");
    let manifest = std::fs::read_to_string(dir.path().join("manifest.json")).unwrap();
    assert!(
        manifest.contains("v1"),
        "the cache still says v1: {manifest}"
    );

    // The retry, with seasons back, downloads the new set.
    let healed = MockServer::start().await;
    mount_all_tables(&healed, "v2").await;
    let retry = AssetCache::with_base(dir.path().to_path_buf(), healed.uri());
    assert!(retry.refresh_tables().await.unwrap());
    assert!(!retry.tables_stale());
    // The same cache object recovers too.
    let again = MockServer::start().await;
    mount_all_tables(&again, "v3").await;
    let same = AssetCache::with_base(dir.path().to_path_buf(), again.uri());
    assert!(same.refresh_tables().await.unwrap());
}

#[tokio::test]
async fn a_failed_table_fetch_with_no_disk_tables_is_still_an_error() {
    let dir = tempdir();
    let server = MockServer::start().await;
    mount_version(&server, "v1").await;
    mount_agents(&server, None).await;
    let cache = AssetCache::with_base(dir.path().to_path_buf(), server.uri());
    assert!(cache.refresh_tables().await.is_err());
    assert!(!cache.tables_stale(), "no tables at all is not 'stale'");
}

#[tokio::test]
async fn tables_that_cannot_be_written_to_disk_are_still_used() {
    // The cache dir cannot be created (a file is in its place): the tables
    // just downloaded still serve this session.
    let dir = tempdir();
    let blocked = dir.path().join("assets");
    std::fs::write(&blocked, b"not a directory").unwrap();
    let server = MockServer::start().await;
    mount_all_tables(&server, "v1").await;
    let cache = AssetCache::with_base(blocked.join("inner"), server.uri());
    cache
        .refresh_tables()
        .await
        .expect("downloaded, even if not stored");
    assert!(cache.meta().is_ok(), "names and acts are there");
}

#[tokio::test]
async fn a_failed_cache_write_leaves_no_temp_file_behind() {
    // agents.json cannot be replaced (a directory is in its place): the
    // write fails, and its temp file goes with it.
    let dir = tempdir();
    std::fs::create_dir_all(dir.path().join("agents.json")).unwrap();
    let server = MockServer::start().await;
    mount_all_tables(&server, "v1").await;
    let cache = AssetCache::with_base(dir.path().to_path_buf(), server.uri());
    for _ in 0..3 {
        let _ = cache.refresh_tables().await;
    }
    let leftovers: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains(".tmp-"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
    assert!(cache.meta().is_ok());
}

// ---------------------------------------------------------------------
// R189: the skin catalog, the collection and the Skins screen's images
// ---------------------------------------------------------------------

const VANDAL: &str = "9c82e19d-4575-0200-1a81-3eacf00cf872";
/// Reaver Vandal's first level (the live API's).
const REAVER_LEVEL: &str = "578e9077-4f88-260c-e54c-b988425c60e4";
const NO_ICON_LEVEL: &str = "eeee5555-0000-0000-0000-000000000001";
const NO_ICON_SKIN: &str = "eeee5555-0000-0000-0000-000000000002";

/// `/v1/weapons` with one weapon, its default skin, the Reaver (Premium)
/// and a tierless skin without a level icon. `times`: how often it may be
/// asked.
async fn mount_weapons(server: &MockServer, times: Option<u64>) {
    let body = json!({"status": 200, "data": [{
        "uuid": VANDAL, "displayName": "Vandal", "category": "EEquippableCategory::Rifle",
        "defaultSkinUuid": "27f21d97-4c4b-bd1c-1f08-31830ab0be84",
        "displayIcon": format!("{}/img/vandal.png", server.uri()),
        "skins": [
            {"uuid": "27f21d97-4c4b-bd1c-1f08-31830ab0be84", "displayName": "Standard Vandal",
             "contentTierUuid": null,
             "levels": [{"uuid": "bbbb0000-0000-0000-0000-000000000001", "displayIcon": null}]},
            {"uuid": SKIN_UUID, "displayName": "Reaver Vandal", "contentTierUuid": CONTENT_TIER_UUID,
             "chromas": [{"uuid": RED_CHROMA, "displayName": "Reaver Vandal (Variant 1 Red)"}],
             "levels": [{"uuid": REAVER_LEVEL,
                         "displayIcon": format!("{}/weaponskinlevels/{REAVER_LEVEL}/displayicon.png", server.uri())}]},
            {"uuid": NO_ICON_SKIN, "displayName": "Bare Vandal", "contentTierUuid": null,
             "levels": [{"uuid": NO_ICON_LEVEL, "displayIcon": null}]}
        ]
    }]});
    let mut mock = Mock::given(method("GET"))
        .and(path("/v1/weapons"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body));
    if let Some(n) = times {
        mock = mock.up_to_n_times(n);
    }
    mock.mount(server).await;
}

async fn weapons_requests(server: &MockServer) -> usize {
    server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.url.path() == "/v1/weapons")
        .count()
}

#[tokio::test]
async fn the_skin_catalog_needs_the_tables_version_first() {
    let dir = tempdir();
    let server = MockServer::start().await;
    mount_weapons(&server, None).await;
    let cache = AssetCache::with_base(dir.path().to_path_buf(), server.uri());
    assert!(matches!(
        cache.collection(&[REAVER_LEVEL.to_string()], 1).await,
        Err(AssetError::TablesNotLoaded)
    ));
    assert_eq!(weapons_requests(&server).await, 0);
}

#[tokio::test]
async fn the_collection_maps_owned_levels_and_the_catalog_is_fetched_once_per_version() {
    let dir = tempdir();
    let server = MockServer::start().await;
    mount_all_tables(&server, "v1").await;
    mount_weapons(&server, None).await;

    let cache = AssetCache::with_base(dir.path().to_path_buf(), server.uri());
    cache.refresh_tables().await.unwrap();
    let owned = [
        REAVER_LEVEL.to_string(),
        NO_ICON_LEVEL.to_string(),
        "bbbb0000-0000-0000-0000-000000000001".to_string(), // the default skin
        "ffffffff-0000-0000-0000-000000000000".to_string(), // unknown
    ];
    let got = cache.collection(&owned, 42).await.unwrap();
    assert_eq!(got.fetched_ms, 42);
    assert_eq!(got.weapons.len(), 1);
    assert_eq!(
        (got.weapons[0].id.as_str(), got.weapons[0].name.as_str()),
        (VANDAL, "Vandal")
    );
    let items: Vec<_> = got
        .items
        .iter()
        .map(|s| {
            (
                s.skin_id.as_str(),
                s.level_id.as_str(),
                s.name.as_str(),
                s.edition,
                s.vp,
            )
        })
        .collect();
    assert_eq!(
        items,
        [
            (NO_ICON_SKIN, NO_ICON_LEVEL, "Bare", None, None),
            (
                SKIN_UUID,
                REAVER_LEVEL,
                "Reaver",
                Some(opd1_core::wire::Edition::Premium),
                Some(1775)
            ),
        ]
    );

    // A compact derived file is cached, not the raw answer.
    let file = dir.path().join("skincatalog.json");
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(!text.contains("chromas") && !text.contains("Standard Vandal"));

    // Same version: neither the network nor (from memory) the disk again,
    // and a fresh cache over the same dir reads the file.
    cache.collection(&owned, 43).await.unwrap();
    let again = AssetCache::with_base(dir.path().to_path_buf(), server.uri());
    again.refresh_tables().await.unwrap();
    assert_eq!(again.collection(&owned, 44).await.unwrap().items.len(), 2);
    assert_eq!(weapons_requests(&server).await, 1);
}

#[tokio::test]
async fn a_new_version_rebuilds_the_catalog_and_a_failed_rebuild_keeps_the_old_one() {
    let dir = tempdir();
    let first = MockServer::start().await;
    mount_all_tables(&first, "v1").await;
    mount_weapons(&first, None).await;
    let cache = AssetCache::with_base(dir.path().to_path_buf(), first.uri());
    cache.refresh_tables().await.unwrap();
    cache.skin_catalog().await.unwrap();

    // v2's tables answer, its /v1/weapons does not (no mock: 404).
    let second = MockServer::start().await;
    mount_all_tables(&second, "v2").await;
    let cache = AssetCache::with_base(dir.path().to_path_buf(), second.uri())
        .with_catalog_retry_after(std::time::Duration::ZERO);
    cache.refresh_tables().await.unwrap();
    let got = cache
        .collection(&[REAVER_LEVEL.to_string()], 1)
        .await
        .unwrap();
    assert_eq!(got.items.len(), 1, "the v1 catalog stands in");

    // Once /v1/weapons answers, the next call rebuilds for v2.
    mount_weapons(&second, None).await;
    cache.skin_catalog().await.unwrap();
    assert_eq!(weapons_requests(&second).await, 2);
    cache.skin_catalog().await.unwrap();
    assert_eq!(weapons_requests(&second).await, 2);
}

#[tokio::test]
async fn a_failed_catalog_build_is_not_downloaded_again_on_every_call() {
    let dir = tempdir();
    let first = MockServer::start().await;
    mount_all_tables(&first, "v1").await;
    mount_weapons(&first, None).await;
    let cache = AssetCache::with_base(dir.path().to_path_buf(), first.uri());
    cache.refresh_tables().await.unwrap();
    cache.skin_catalog().await.unwrap();

    // v2's /v1/weapons fails: the v1 catalog stands in, and the calls right
    // after (one per uncached icon on the Skins screen) don't download again.
    let second = MockServer::start().await;
    mount_all_tables(&second, "v2").await;
    let cache = AssetCache::with_base(dir.path().to_path_buf(), second.uri());
    cache.refresh_tables().await.unwrap();
    for _ in 0..5 {
        cache.skin_catalog().await.unwrap();
    }
    assert_eq!(weapons_requests(&second).await, 1);

    // With no older catalog at all, those calls fail at once, also without
    // a download each.
    let empty = tempdir();
    let fresh = AssetCache::with_base(empty.path().to_path_buf(), second.uri());
    fresh.refresh_tables().await.unwrap();
    assert!(fresh.skin_catalog().await.is_err());
    assert!(fresh.skin_catalog().await.is_err());
    assert_eq!(weapons_requests(&second).await, 2);
}

#[tokio::test]
async fn skin_level_and_weapon_icons_are_served_downscaled_and_unknown_ones_are_not_found() {
    let dir = tempdir();
    let server = MockServer::start().await;
    mount_all_tables(&server, "v1").await;
    mount_weapons(&server, Some(1)).await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/weaponskinlevels/{REAVER_LEVEL}/displayicon.png"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(png_bytes(512, 128)))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/img/vandal.png"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(png_bytes(480, 120)))
        .expect(1)
        .mount(&server)
        .await;

    let cache = AssetCache::with_base(dir.path().to_path_buf(), server.uri());
    cache.refresh_tables().await.unwrap();

    let skin = cache
        .ensure_image(AssetKind::SkinLevel, REAVER_LEVEL)
        .await
        .unwrap();
    assert_eq!(png_dimensions(&std::fs::read(&skin).unwrap()), (256, 64));
    let weapon = cache
        .ensure_image(AssetKind::WeaponIcon, VANDAL)
        .await
        .unwrap();
    assert_eq!(png_dimensions(&std::fs::read(&weapon).unwrap()), (160, 40));
    // Cached: the second ask touches nothing remote (`expect(1)` above).
    cache
        .ensure_image(AssetKind::SkinLevel, REAVER_LEVEL)
        .await
        .unwrap();

    for (kind, id) in [
        (AssetKind::SkinLevel, NO_ICON_LEVEL), // no displayIcon
        (AssetKind::SkinLevel, "ffffffff-0000-0000-0000-000000000000"), // unknown level
        (AssetKind::SkinLevel, SKIN_UUID),     // a skin, not a level
        (
            AssetKind::WeaponIcon,
            "ffffffff-0000-0000-0000-000000000000",
        ),
        (AssetKind::SkinLevel, "../../etc/passwd"),
    ] {
        assert!(
            matches!(
                cache.ensure_image(kind, id).await,
                Err(AssetError::NotFound)
            ),
            "{kind:?} {id}"
        );
    }
}
