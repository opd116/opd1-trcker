//! valorant-api.com asset cache: bulk metadata tables (agents, maps, the
//! latest competitive-tier set, seasons) refreshed on disk when
//! `/v1/version` changes, plus [`AssetCache::ensure_image`] to
//! download-once-and-cache the actual images the UI displays.
//!
//! **Lightness rulings** (CLAUDE.md: "Lightness is priority #1"):
//! - `weapons`/`skinchromas` are *never* bulk-fetched — the full
//!   `/v1/weapons` payload is ~3.5MB (every skin ever made, for every
//!   weapon) and this app only ever needs one specific chroma's image at a
//!   time (the uuid comes from a player's loadout, not from browsing a
//!   catalogue). [`AssetKind::Chroma`] fetches
//!   `/v1/weapons/skinchromas/{uuid}` directly instead — one small
//!   response (~400 bytes of JSON) per chroma actually seen, cached
//!   forever after that.
//! - `playercards` (1016 entries) are likewise never bulk-fetched — only
//!   `/v1/playercards/{uuid}` for cards actually seen in a roster.
//! - Only the *latest* competitive-tier set is kept on disk (the full
//!   `/v1/competitivetiers` response has one table per episode revision;
//!   older ones are discarded after picking the last array entry, per the
//!   real API's ordering — see [`fetch_latest_tiers`]).
//! - Every bulk table is stored as a small, trimmed struct (just the
//!   fields this app uses), not the raw valorant-api JSON blob — the same
//!   "compact rows, not raw JSON" philosophy CLAUDE.md applies to match
//!   data.
//! - [`ensure_image`](AssetCache::ensure_image) downscales the two kinds
//!   whose real pixel size is far larger than the UI ever displays them
//!   (measured against a live `valorant-api.com` response — see
//!   task-5a-report.md): agent `displayIcon` (1024x1024, shown as a ≤64px
//!   head icon) and map `splash` (1920x1080, shown ≤1000px wide). Rank
//!   tier icons (256x256) are downscaled too, for the same reason at a
//!   smaller scale. Every other kind is already close to its display size
//!   and is cached as-is.

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::to_core::{self, SeasonEntry};
use opd1_core::stats::Act;

/// Timeout for every valorant-api.com call (table fetch, per-id fetch, or
/// image download) — valorant-api is a public third party, not the Riot
/// client, so no token bucket is needed (ruling, per the brief: "not
/// Riot — no Riot token bucket needed, but set timeouts"), but an
/// unresponsive server must still not hang the caller forever.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(8);

const DEFAULT_BASE: &str = "https://valorant-api.com";

// ---------------------------------------------------------------------
// errors
// ---------------------------------------------------------------------

#[derive(Debug)]
pub enum AssetError {
    Network(String),
    Http(u16),
    Decode(String),
    Io(String),
    /// [`AssetCache::meta`]/`map_uuid_for_path`/`acts`/[`ensure_image`]'s
    /// bulk-table kinds were called before [`AssetCache::refresh_tables`]
    /// ever succeeded once.
    TablesNotLoaded,
    NotFound,
}

impl fmt::Display for AssetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AssetError::Network(msg) => write!(f, "network error talking to valorant-api: {msg}"),
            AssetError::Http(code) => write!(f, "unexpected HTTP status {code} from valorant-api"),
            AssetError::Decode(msg) => write!(f, "couldn't parse valorant-api's response: {msg}"),
            AssetError::Io(msg) => write!(f, "asset cache I/O error: {msg}"),
            AssetError::TablesNotLoaded => write!(f, "asset tables have not been loaded yet"),
            AssetError::NotFound => write!(f, "asset id not found in the cached table"),
        }
    }
}

impl std::error::Error for AssetError {}

fn io_err(e: std::io::Error) -> AssetError {
    AssetError::Io(e.to_string())
}

fn net_err(e: reqwest::Error) -> AssetError {
    AssetError::Network(e.to_string())
}

// ---------------------------------------------------------------------
// AssetKind (R21)
// ---------------------------------------------------------------------

/// Which image to fetch, and from which valorant-api field — see each
/// variant's doc comment for its source field and this module's own docs
/// for which kinds get downscaled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetKind {
    /// `/v1/agents`' `displayIcon` (a square head icon — CLAUDE.md: "Agent
    /// head icon", never `fullPortrait`/`bustPortrait`). `id` = agent uuid.
    Agent,
    /// `/v1/playercards/{uuid}`'s `smallArt`. `id` = player-card uuid.
    CardSmall,
    /// `/v1/playercards/{uuid}`'s `wideArt`. `id` = player-card uuid.
    CardWide,
    /// The latest competitive-tier set's `tiers[].largeIcon` — chosen over
    /// `smallIcon` since both are the same 256x256 resolution today and
    /// `largeIcon` matches a prominent profile-card-style rank display
    /// (CLAUDE.md's "Current Rank icon"), rather than `smallIcon`'s
    /// typical inline-chat-badge use. `id` = the tier number as a decimal
    /// string (e.g. `"21"`).
    Tier,
    /// `/v1/maps`' `listViewIcon` (the row-background image). `id` = map
    /// uuid.
    MapList,
    /// `/v1/maps`' `splash`. `id` = map uuid.
    MapSplash,
    /// `/v1/weapons/skinchromas/{uuid}`'s `fullRender`. `id` = chroma
    /// uuid.
    Chroma,
}

impl AssetKind {
    fn dir_name(self) -> &'static str {
        match self {
            AssetKind::Agent => "agent",
            AssetKind::CardSmall => "cardSmall",
            AssetKind::CardWide => "cardWide",
            AssetKind::Tier => "tier",
            AssetKind::MapList => "mapList",
            AssetKind::MapSplash => "mapSplash",
            AssetKind::Chroma => "chroma",
        }
    }

    /// The target width to downscale to, or `None` to cache as-is. See
    /// this module's top doc comment for the measured source sizes behind
    /// these numbers.
    fn downscale_target_width(self) -> Option<u32> {
        match self {
            AssetKind::Agent => Some(128),      // 1024px source, shown <=64px
            AssetKind::Tier => Some(128),       // 256px source, shown small
            AssetKind::MapSplash => Some(1000), // 1920px source, shown <=1000px wide
            AssetKind::CardSmall | AssetKind::CardWide | AssetKind::MapList | AssetKind::Chroma => {
                None
            }
        }
    }
}

// ---------------------------------------------------------------------
// Meta (the UI-facing lookup builder)
// ---------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentMeta {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MapMeta {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TierMeta {
    pub name: String,
    /// A CSS-ready `#rrggbbaa` string — valorant-api's own `color` field
    /// is the same 8 hex digits without the leading `#`.
    pub color: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct Meta {
    pub agents: HashMap<String, AgentMeta>,
    pub maps: HashMap<String, MapMeta>,
    pub tiers: HashMap<u8, TierMeta>,
}

// ---------------------------------------------------------------------
// on-disk / in-memory table shapes
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AgentEntry {
    uuid: String,
    name: String,
    display_icon: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MapEntry {
    uuid: String,
    name: String,
    map_url: String,
    list_view_icon: String,
    splash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TierEntry {
    tier: u8,
    name: String,
    color: String,
    large_icon: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Manifest {
    version: String,
}

#[derive(Debug, Clone, Default)]
struct Tables {
    agents: Vec<AgentEntry>,
    maps: Vec<MapEntry>,
    tiers: Vec<TierEntry>,
    seasons: Vec<SeasonEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PlayerCardEntry {
    small_art: String,
    wide_art: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChromaEntry {
    full_render: String,
}

// ---------------------------------------------------------------------
// AssetCache
// ---------------------------------------------------------------------

pub struct AssetCache {
    client: reqwest::Client,
    base: String,
    dir: PathBuf,
    tables: RwLock<Tables>,
}

impl AssetCache {
    /// `cache_dir` is the caller-given root (Task 5b: `app_cache_dir/assets/`)
    /// — created lazily on first write, never required to exist up front.
    pub fn new(cache_dir: PathBuf) -> Self {
        Self::with_base(cache_dir, DEFAULT_BASE.to_string())
    }

    /// Like [`new`](Self::new), but pointed at a different base URL — the
    /// test-only entry point for mocking valorant-api.com with `wiremock`.
    #[doc(hidden)]
    pub fn with_base(cache_dir: PathBuf, base: String) -> Self {
        // Same one-time process-wide setup `RiotSession::connect` does —
        // `reqwest`'s `rustls-no-provider` backend needs a crypto provider
        // installed before any TLS connection, and this cache can be built
        // (e.g. in tests, or before a `RiotSession` ever connects) without
        // that having happened yet. Installing twice is harmless (`Err` is
        // ignored, same as `session.rs`).
        let _ = rustls::crypto::ring::default_provider().install_default();

        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .expect("building the valorant-api HTTP client should never fail");
        Self {
            client,
            base,
            dir: cache_dir,
            tables: RwLock::new(Tables::default()),
        }
    }

    // -- bulk tables --------------------------------------------------

    /// Checks `/v1/version`; if it matches what's on disk (and every table
    /// file is present), loads the cached tables into memory without
    /// another network call and returns `Ok(false)`. Otherwise re-fetches
    /// every bulk table, persists them (trimmed — see the module docs),
    /// and returns `Ok(true)`.
    pub async fn refresh_tables(&self) -> Result<bool, AssetError> {
        let version = self.fetch_version().await?;

        if self.disk_cache_matches(&version) {
            let tables = self.load_tables_from_disk()?;
            *self.tables.write().unwrap() = tables;
            return Ok(false);
        }

        let agents = self.fetch_agents().await?;
        let maps = self.fetch_maps().await?;
        let tiers = self.fetch_latest_tiers().await?;
        let seasons = self.fetch_seasons().await?;

        std::fs::create_dir_all(&self.dir).map_err(io_err)?;
        write_json(&self.dir.join("agents.json"), &agents)?;
        write_json(&self.dir.join("maps.json"), &maps)?;
        write_json(&self.dir.join("tiers.json"), &tiers)?;
        write_json(&self.dir.join("seasons.json"), &seasons)?;
        write_json(&self.dir.join("manifest.json"), &Manifest { version })?;

        *self.tables.write().unwrap() = Tables {
            agents,
            maps,
            tiers,
            seasons,
        };
        Ok(true)
    }

    fn disk_cache_matches(&self, version: &str) -> bool {
        let manifest_path = self.dir.join("manifest.json");
        let Ok(bytes) = std::fs::read(&manifest_path) else {
            return false;
        };
        let Ok(manifest) = serde_json::from_slice::<Manifest>(&bytes) else {
            return false;
        };
        if manifest.version != version {
            return false;
        }
        ["agents.json", "maps.json", "tiers.json", "seasons.json"]
            .iter()
            .all(|f| self.dir.join(f).is_file())
    }

    fn load_tables_from_disk(&self) -> Result<Tables, AssetError> {
        Ok(Tables {
            agents: read_json(&self.dir.join("agents.json"))?,
            maps: read_json(&self.dir.join("maps.json"))?,
            tiers: read_json(&self.dir.join("tiers.json"))?,
            seasons: read_json(&self.dir.join("seasons.json"))?,
        })
    }

    async fn fetch_version(&self) -> Result<String, AssetError> {
        let value = self.get_json(&format!("{}/v1/version", self.base)).await?;
        value
            .get("data")
            .and_then(|d| d.get("version"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| AssetError::Decode("missing data.version".to_string()))
    }

    async fn fetch_agents(&self) -> Result<Vec<AgentEntry>, AssetError> {
        let value = self
            .get_json(&format!("{}/v1/agents?isPlayableCharacter=true", self.base))
            .await?;
        let data = data_array(&value)?;
        Ok(data
            .iter()
            .filter_map(|a| {
                Some(AgentEntry {
                    uuid: str_field(a, "uuid")?,
                    name: str_field(a, "displayName")?,
                    display_icon: str_field(a, "displayIcon")?,
                })
            })
            .collect())
    }

    async fn fetch_maps(&self) -> Result<Vec<MapEntry>, AssetError> {
        let value = self.get_json(&format!("{}/v1/maps", self.base)).await?;
        let data = data_array(&value)?;
        Ok(data
            .iter()
            .filter_map(|m| {
                Some(MapEntry {
                    uuid: str_field(m, "uuid")?,
                    name: str_field(m, "displayName")?,
                    map_url: str_field(m, "mapUrl")?,
                    list_view_icon: str_field(m, "listViewIcon")?,
                    splash: str_field(m, "splash")?,
                })
            })
            .collect())
    }

    /// "Latest set" = the last entry of `/v1/competitivetiers`'s `data`
    /// array — matches the real API's ordering (oldest episode first);
    /// see the module docs for why only this one set is kept on disk.
    async fn fetch_latest_tiers(&self) -> Result<Vec<TierEntry>, AssetError> {
        let value = self
            .get_json(&format!("{}/v1/competitivetiers", self.base))
            .await?;
        let data = data_array(&value)?;
        let latest = data
            .last()
            .ok_or_else(|| AssetError::Decode("empty competitivetiers data".to_string()))?;
        let tiers = latest
            .get("tiers")
            .and_then(Value::as_array)
            .ok_or_else(|| AssetError::Decode("missing tiers array".to_string()))?;
        Ok(tiers
            .iter()
            .filter_map(|t| {
                Some(TierEntry {
                    tier: t.get("tier")?.as_u64()? as u8,
                    name: str_field(t, "tierName")?,
                    color: str_field(t, "color")?,
                    large_icon: str_field(t, "largeIcon")?,
                })
            })
            .collect())
    }

    async fn fetch_seasons(&self) -> Result<Vec<SeasonEntry>, AssetError> {
        let value = self.get_json(&format!("{}/v1/seasons", self.base)).await?;
        let data = value
            .get("data")
            .cloned()
            .ok_or_else(|| AssetError::Decode("missing data".to_string()))?;
        serde_json::from_value(data).map_err(|e| AssetError::Decode(e.to_string()))
    }

    // -- UI-facing lookups (require refresh_tables() to have run) ------

    fn tables_snapshot(&self) -> Result<Tables, AssetError> {
        let tables = self.tables.read().unwrap();
        if tables.agents.is_empty()
            && tables.maps.is_empty()
            && tables.tiers.is_empty()
            && tables.seasons.is_empty()
        {
            return Err(AssetError::TablesNotLoaded);
        }
        Ok(tables.clone())
    }

    /// `{ agents: {uuid: {name}}, maps: {uuid: {name}}, tiers: {n: {name, color}} }`.
    pub fn meta(&self) -> Result<Meta, AssetError> {
        let tables = self.tables_snapshot()?;
        Ok(Meta {
            agents: tables
                .agents
                .iter()
                .map(|a| {
                    (
                        a.uuid.clone(),
                        AgentMeta {
                            name: a.name.clone(),
                        },
                    )
                })
                .collect(),
            maps: tables
                .maps
                .iter()
                .map(|m| {
                    (
                        m.uuid.clone(),
                        MapMeta {
                            name: m.name.clone(),
                        },
                    )
                })
                .collect(),
            tiers: tables
                .tiers
                .iter()
                .map(|t| {
                    (
                        t.tier,
                        TierMeta {
                            name: t.name.clone(),
                            color: format!("#{}", t.color),
                        },
                    )
                })
                .collect(),
        })
    }

    /// Resolves a Riot map path (e.g. `/Game/Maps/Ascent/Ascent`) to its
    /// valorant-api map uuid, by matching `mapUrl` (CLAUDE.md/the asset
    /// table: "match by mapUrl"). `Ok(None)` when no cached map matches —
    /// distinct from [`AssetError::TablesNotLoaded`], which means the
    /// tables were never even fetched.
    pub fn map_uuid_for_path(&self, riot_map_path: &str) -> Result<Option<String>, AssetError> {
        let tables = self.tables_snapshot()?;
        Ok(tables
            .maps
            .iter()
            .find(|m| m.map_url == riot_map_path)
            .map(|m| m.uuid.clone()))
    }

    /// The ordered act list, via `to_core::acts_from_seasons` over the
    /// cached `/v1/seasons` table.
    pub fn acts(&self) -> Result<Vec<Act>, AssetError> {
        let tables = self.tables_snapshot()?;
        Ok(to_core::acts_from_seasons(&tables.seasons))
    }

    // -- images ---------------------------------------------------------

    /// Downloads (once) and caches the image for `kind`/`id`, downscaling
    /// it first if that kind's real size is far larger than the UI ever
    /// displays it (see the module docs). Returns the cached file's path
    /// immediately, without touching the network, on every call after the
    /// first for the same `(kind, id)`.
    pub async fn ensure_image(&self, kind: AssetKind, id: &str) -> Result<PathBuf, AssetError> {
        let file_path = self.image_path(kind, id);
        if file_path.is_file() {
            return Ok(file_path);
        }

        let source_url = self.resolve_source_url(kind, id).await?;
        let bytes = self.download(&source_url).await?;
        let processed = self.maybe_downscale(kind, bytes)?;

        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent).map_err(io_err)?;
        }
        std::fs::write(&file_path, processed).map_err(io_err)?;
        Ok(file_path)
    }

    fn image_path(&self, kind: AssetKind, id: &str) -> PathBuf {
        self.dir
            .join("images")
            .join(kind.dir_name())
            .join(format!("{id}.png"))
    }

    async fn resolve_source_url(&self, kind: AssetKind, id: &str) -> Result<String, AssetError> {
        match kind {
            AssetKind::Agent => {
                let tables = self.tables_snapshot()?;
                tables
                    .agents
                    .iter()
                    .find(|a| a.uuid == id)
                    .map(|a| a.display_icon.clone())
                    .ok_or(AssetError::NotFound)
            }
            AssetKind::MapList => {
                let tables = self.tables_snapshot()?;
                tables
                    .maps
                    .iter()
                    .find(|m| m.uuid == id)
                    .map(|m| m.list_view_icon.clone())
                    .ok_or(AssetError::NotFound)
            }
            AssetKind::MapSplash => {
                let tables = self.tables_snapshot()?;
                tables
                    .maps
                    .iter()
                    .find(|m| m.uuid == id)
                    .map(|m| m.splash.clone())
                    .ok_or(AssetError::NotFound)
            }
            AssetKind::Tier => {
                let tier_num: u8 = id.parse().map_err(|_| AssetError::NotFound)?;
                let tables = self.tables_snapshot()?;
                tables
                    .tiers
                    .iter()
                    .find(|t| t.tier == tier_num)
                    .map(|t| t.large_icon.clone())
                    .ok_or(AssetError::NotFound)
            }
            AssetKind::CardSmall => Ok(self.fetch_or_load_playercard(id).await?.small_art),
            AssetKind::CardWide => Ok(self.fetch_or_load_playercard(id).await?.wide_art),
            AssetKind::Chroma => Ok(self.fetch_or_load_chroma(id).await?.full_render),
        }
    }

    /// `/v1/playercards/{uuid}` — fetched (and cached to disk as a tiny
    /// JSON file) the first time this card is seen, never bulk-downloaded
    /// (see the module docs).
    async fn fetch_or_load_playercard(&self, uuid: &str) -> Result<PlayerCardEntry, AssetError> {
        let path = self.dir.join("playercards").join(format!("{uuid}.json"));
        if let Ok(bytes) = std::fs::read(&path) {
            if let Ok(entry) = serde_json::from_slice(&bytes) {
                return Ok(entry);
            }
        }
        let value = self
            .get_json(&format!("{}/v1/playercards/{uuid}", self.base))
            .await?;
        let data = value
            .get("data")
            .ok_or_else(|| AssetError::Decode("missing data".to_string()))?;
        let entry = PlayerCardEntry {
            small_art: str_field(data, "smallArt")
                .ok_or_else(|| AssetError::Decode("missing smallArt".to_string()))?,
            wide_art: str_field(data, "wideArt")
                .ok_or_else(|| AssetError::Decode("missing wideArt".to_string()))?,
        };
        write_json(&path, &entry)?;
        Ok(entry)
    }

    /// `/v1/weapons/skinchromas/{uuid}` — the single-chroma endpoint, so
    /// the ~3.5MB full `/v1/weapons` table is never fetched (see the
    /// module docs).
    async fn fetch_or_load_chroma(&self, uuid: &str) -> Result<ChromaEntry, AssetError> {
        let path = self.dir.join("chromas").join(format!("{uuid}.json"));
        if let Ok(bytes) = std::fs::read(&path) {
            if let Ok(entry) = serde_json::from_slice(&bytes) {
                return Ok(entry);
            }
        }
        let value = self
            .get_json(&format!("{}/v1/weapons/skinchromas/{uuid}", self.base))
            .await?;
        let data = value
            .get("data")
            .ok_or_else(|| AssetError::Decode("missing data".to_string()))?;
        let entry = ChromaEntry {
            full_render: str_field(data, "fullRender")
                .ok_or_else(|| AssetError::Decode("missing fullRender".to_string()))?,
        };
        write_json(&path, &entry)?;
        Ok(entry)
    }

    async fn download(&self, url: &str) -> Result<Vec<u8>, AssetError> {
        let resp = self.client.get(url).send().await.map_err(net_err)?;
        if !resp.status().is_success() {
            return Err(AssetError::Http(resp.status().as_u16()));
        }
        resp.bytes().await.map(|b| b.to_vec()).map_err(net_err)
    }

    /// Downscales `bytes` to `kind`'s target width (see
    /// [`AssetKind::downscale_target_width`]), preserving aspect ratio, if
    /// the source is wider than that target. Re-encodes as PNG regardless
    /// of the source format (every valorant-api image asset is PNG or
    /// JPEG; PNG is a safe, lossless, universally-supported cache format
    /// for both). Returns `bytes` unchanged for a kind with no downscale
    /// target, or a source already at/under the target width.
    fn maybe_downscale(&self, kind: AssetKind, bytes: Vec<u8>) -> Result<Vec<u8>, AssetError> {
        let Some(target_width) = kind.downscale_target_width() else {
            return Ok(bytes);
        };

        let img = image::load_from_memory(&bytes).map_err(|e| AssetError::Decode(e.to_string()))?;
        if img.width() <= target_width {
            return Ok(bytes);
        }

        let target_height =
            ((img.height() as u64 * target_width as u64) / img.width() as u64).max(1) as u32;
        // `resize_exact`, not `resize`: `target_height` was already
        // computed to preserve aspect ratio above, and `resize`'s own
        // aspect-fit-within-a-box logic can round to one pixel narrower
        // than `target_width` (double rounding) — `resize_exact` honors
        // the exact dimensions we already worked out.
        let resized = img.resize_exact(
            target_width,
            target_height,
            image::imageops::FilterType::Lanczos3,
        );

        let mut out = Vec::new();
        resized
            .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
            .map_err(|e| AssetError::Decode(e.to_string()))?;
        Ok(out)
    }

    async fn get_json(&self, url: &str) -> Result<Value, AssetError> {
        let resp = self.client.get(url).send().await.map_err(net_err)?;
        if !resp.status().is_success() {
            return Err(AssetError::Http(resp.status().as_u16()));
        }
        resp.json().await.map_err(net_err)
    }
}

fn data_array(value: &Value) -> Result<&Vec<Value>, AssetError> {
    value
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| AssetError::Decode("missing data array".to_string()))
}

fn str_field(value: &Value, key: &str) -> Option<String> {
    value.get(key)?.as_str().map(str::to_string)
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), AssetError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(io_err)?;
    }
    let bytes = serde_json::to_vec(value).map_err(|e| AssetError::Decode(e.to_string()))?;
    std::fs::write(path, bytes).map_err(io_err)
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, AssetError> {
    let bytes = std::fs::read(path).map_err(io_err)?;
    serde_json::from_slice(&bytes).map_err(|e| AssetError::Decode(e.to_string()))
}
