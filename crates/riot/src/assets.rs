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
//!   `/v1/playercards/{uuid}` for cards actually seen in a roster. A
//!   playercard json cached before `largeArt` was read has no
//!   `large_art`; the first [`AssetKind::CardLarge`] request for such a
//!   card refetches it once (R107).
//! - Only the *latest* competitive-tier set is kept on disk (the full
//!   `/v1/competitivetiers` response has one table per episode revision;
//!   older ones are discarded after picking the last array entry, per the
//!   real API's ordering — see [`AssetCache::fetch_latest_tiers`]).
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
//!   smaller scale, and so is the player card's `largeArt` (160px wide).
//!   Every other kind is already close to its display size and is cached
//!   as-is.
//!
//! **Fix-round 1 hardening** (see task-5a-report.md's fix-round section
//! for the full findings this responds to):
//! - `ensure_image`'s `id` is validated (UUID shape, or an in-range tier
//!   number) *before* it ever becomes part of a path or URL — 5b feeds
//!   this straight from a URI path, so an unvalidated `id` would be a
//!   path-traversal / SSRF-shaped hole.
//! - Every on-disk write (images and JSON caches alike) goes through
//!   [`write_atomic`]: written to a temp file in the same directory, then
//!   renamed, so a crash mid-write never leaves a corrupt cache file.
//! - A per-`(kind, id)` async lock ([`AssetCache::lock_for`]) coalesces
//!   concurrent `ensure_image` calls for the same asset into exactly one
//!   download.
//! - `refresh_tables` falls back to whatever's on disk when
//!   `/v1/version` can't be reached at all, as long as a full table set
//!   already exists there (offline support).
//! - Image decode/resize (CPU-bound) and every blocking filesystem
//!   operation run inside [`tokio::task::spawn_blocking`] via the
//!   [`blocking`] helper, so they never stall the async executor.
//! - Image downloads use a separate `reqwest::Client` with a longer total
//!   timeout (30s) than JSON calls (8s) — a multi-hundred-KB/multi-MB
//!   image over a slow connection shouldn't be held to the same budget as
//!   a small JSON response — while keeping the *connect* timeout short on
//!   both.

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use opd1_core::stats::Act;

use crate::to_core::{self, SeasonEntry};

/// How long to wait for the TCP+TLS handshake alone — short regardless of
/// what's being fetched, matching `RiotSession`'s own `REMOTE_CONNECT_TIMEOUT`
/// convention (a slow *connect* means the server isn't there; a slow
/// *transfer* is normal for a large image).
const CONNECT_TIMEOUT: Duration = Duration::from_secs(8);
/// Total timeout for a JSON call (table fetch or per-id metadata fetch) —
/// these are all small (a few KB to ~100KB for the bulk tables).
const JSON_TIMEOUT: Duration = Duration::from_secs(8);
/// Total timeout for an image download (fix-round item 10 / R37) — a map
/// splash before downscaling is ~3MB; 8s was too tight a budget for that
/// over a slow connection, so images get their own, longer-lived client.
const IMAGE_TIMEOUT: Duration = Duration::from_secs(30);

const DEFAULT_BASE: &str = "https://valorant-api.com";

/// The highest tier [`AssetKind::Tier`] will accept as *shaped* like a
/// real tier (fix-round item 4: this is a structural check, not an
/// existence check — an in-range-but-unknown tier still resolves to
/// [`AssetError::NotFound`] once looked up against the loaded table).
/// Real tiers run 3..=27 (Radiant) today; 30 leaves headroom without
/// hardcoding today's exact ceiling.
const MAX_VALID_TIER: u8 = 30;

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

/// Runs a blocking closure on tokio's blocking-task pool (fix-round item
/// 10 / R37) — every filesystem write/read and every image decode/resize
/// in this module goes through this instead of running inline on an async
/// task, which would otherwise stall the executor.
async fn blocking<T, F>(f: F) -> Result<T, AssetError>
where
    F: FnOnce() -> Result<T, AssetError> + Send + 'static,
    T: Send + 'static,
{
    match tokio::task::spawn_blocking(f).await {
        Ok(result) => result,
        Err(e) => Err(AssetError::Io(format!(
            "blocking task panicked or was cancelled: {e}"
        ))),
    }
}

// ---------------------------------------------------------------------
// AssetKind (R21)
// ---------------------------------------------------------------------

/// Which image to fetch, and from which valorant-api field — see each
/// variant's doc comment for its source field and this module's own docs
/// for which kinds get downscaled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AssetKind {
    /// `/v1/agents`' `displayIcon` (a square head icon — CLAUDE.md: "Agent
    /// head icon", never `fullPortrait`/`bustPortrait`). `id` = agent uuid.
    Agent,
    /// `/v1/playercards/{uuid}`'s `smallArt`. `id` = player-card uuid.
    CardSmall,
    /// `/v1/playercards/{uuid}`'s `wideArt`. `id` = player-card uuid.
    CardWide,
    /// `/v1/playercards/{uuid}`'s `largeArt`, downscaled to 160px wide.
    /// `id` = player-card uuid. A card without `largeArt` has no image of
    /// this kind (`NotFound`): it never falls back to another kind.
    CardLarge,
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
            AssetKind::CardLarge => "cardLarge",
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
            AssetKind::CardLarge => Some(160),  // shown <=109px wide; 160 covers 150% DPI (R79)
            AssetKind::MapSplash => Some(1000), // 1920px source, shown <=1000px wide
            AssetKind::CardSmall | AssetKind::CardWide | AssetKind::MapList | AssetKind::Chroma => {
                None
            }
        }
    }
}

/// Fix-round item 4: structural validation of `id` before it becomes part
/// of any path or URL. A UUID-shaped kind must actually look like a UUID;
/// `Tier`'s `id` must parse as a small, in-range number. This runs before
/// anything else in `ensure_image` — no path is built, no file existence
/// checked, no lock acquired, until this passes.
fn validate_id(kind: AssetKind, id: &str) -> Result<(), AssetError> {
    let ok = match kind {
        AssetKind::Tier => matches!(id.parse::<u8>(), Ok(t) if t <= MAX_VALID_TIER),
        AssetKind::Agent
        | AssetKind::CardSmall
        | AssetKind::CardWide
        | AssetKind::CardLarge
        | AssetKind::MapList
        | AssetKind::MapSplash
        | AssetKind::Chroma => is_valid_uuid(id),
    };
    if ok {
        Ok(())
    } else {
        Err(AssetError::NotFound)
    }
}

fn is_valid_uuid(s: &str) -> bool {
    s.len() == 36
        && s.char_indices().all(|(i, c)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                c == '-'
            } else {
                c.is_ascii_hexdigit()
            }
        })
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
    /// `None` only in a json cached before `largeArt` was read (R107).
    /// A freshly fetched entry always has it: `Some("")` records "the
    /// card has no large art", so that isn't refetched on every request.
    #[serde(default)]
    large_art: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChromaEntry {
    full_render: String,
}

// ---------------------------------------------------------------------
// AssetCache
// ---------------------------------------------------------------------

/// Per-`(kind, id)` async locks, created on demand — see
/// [`AssetCache::lock_for`] and fix-round item 5.
type ImageLocks = Mutex<HashMap<(AssetKind, String), Arc<tokio::sync::Mutex<()>>>>;

pub struct AssetCache {
    json_client: reqwest::Client,
    image_client: reqwest::Client,
    base: String,
    dir: PathBuf,
    tables: RwLock<Tables>,
    image_locks: ImageLocks,
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

        let json_client = reqwest::Client::builder()
            .timeout(JSON_TIMEOUT)
            .connect_timeout(CONNECT_TIMEOUT)
            .build()
            .expect("building the valorant-api JSON HTTP client should never fail");
        let image_client = reqwest::Client::builder()
            .timeout(IMAGE_TIMEOUT)
            .connect_timeout(CONNECT_TIMEOUT)
            .build()
            .expect("building the valorant-api image HTTP client should never fail");
        Self {
            json_client,
            image_client,
            base,
            dir: cache_dir,
            tables: RwLock::new(Tables::default()),
            image_locks: Mutex::new(HashMap::new()),
        }
    }

    // -- bulk tables --------------------------------------------------

    /// Checks `/v1/version`; if it matches what's on disk (and every table
    /// file is present), loads the cached tables into memory without
    /// another network call and returns `Ok(false)`. Otherwise re-fetches
    /// every bulk table, persists them (trimmed — see the module docs),
    /// and returns `Ok(true)`.
    ///
    /// Fix-round item 6: if `/v1/version` itself can't be reached at all
    /// (network down), this falls back to whatever's on disk — as long as
    /// a *full* table set already exists there — rather than failing
    /// outright. `meta`/`acts`/`map_uuid_for_path` all work fully offline
    /// once that fallback has loaded a cache.
    pub async fn refresh_tables(&self) -> Result<bool, AssetError> {
        match self.fetch_version().await {
            Ok(version) => self.refresh_with_known_version(version).await,
            Err(network_err) => {
                if self.tables_files_exist() {
                    let tables = blocking({
                        let dir = self.dir.clone();
                        move || load_tables_from_disk_sync(&dir)
                    })
                    .await?;
                    *self.tables.write().unwrap() = tables;
                    Ok(false)
                } else {
                    Err(network_err)
                }
            }
        }
    }

    async fn refresh_with_known_version(&self, version: String) -> Result<bool, AssetError> {
        if self.disk_cache_matches(&version) {
            let tables = blocking({
                let dir = self.dir.clone();
                move || load_tables_from_disk_sync(&dir)
            })
            .await?;
            *self.tables.write().unwrap() = tables;
            return Ok(false);
        }

        let agents = self.fetch_agents().await?;
        let maps = self.fetch_maps().await?;
        let tiers = self.fetch_latest_tiers().await?;
        let seasons = self.fetch_seasons().await?;

        blocking({
            let dir = self.dir.clone();
            let agents = agents.clone();
            let maps = maps.clone();
            let tiers = tiers.clone();
            let seasons = seasons.clone();
            move || {
                std::fs::create_dir_all(&dir).map_err(io_err)?;
                write_json(&dir.join("agents.json"), &agents)?;
                write_json(&dir.join("maps.json"), &maps)?;
                write_json(&dir.join("tiers.json"), &tiers)?;
                write_json(&dir.join("seasons.json"), &seasons)?;
                write_json(&dir.join("manifest.json"), &Manifest { version })?;
                Ok(())
            }
        })
        .await?;

        *self.tables.write().unwrap() = Tables {
            agents,
            maps,
            tiers,
            seasons,
        };
        Ok(true)
    }

    /// A single small file read + a handful of `is_file` stats — cheap
    /// enough that this isn't routed through [`blocking`] (unlike the
    /// larger reads/writes elsewhere in this module).
    fn disk_cache_matches(&self, version: &str) -> bool {
        let manifest_path = self.dir.join("manifest.json");
        let Ok(bytes) = std::fs::read(&manifest_path) else {
            return false;
        };
        let Ok(manifest) = serde_json::from_slice::<Manifest>(&bytes) else {
            return false;
        };
        manifest.version == version && self.tables_files_exist()
    }

    fn tables_files_exist(&self) -> bool {
        ["agents.json", "maps.json", "tiers.json", "seasons.json"]
            .iter()
            .all(|f| self.dir.join(f).is_file())
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
    /// first for the same `(kind, id)` — and coalesces concurrent callers
    /// for the same `(kind, id)` into exactly one download (fix-round
    /// item 5).
    pub async fn ensure_image(&self, kind: AssetKind, id: &str) -> Result<PathBuf, AssetError> {
        // Fix-round item 4: validated *before* any path/URL is built, any
        // file-existence check runs, or any lock is taken.
        validate_id(kind, id)?;

        let file_path = self.image_path(kind, id);

        let lock = self.lock_for(kind, id);
        let _guard = lock.lock().await;

        // Re-check after acquiring the lock: another caller may have just
        // finished downloading this exact asset while we were waiting.
        if file_path.is_file() {
            return Ok(file_path);
        }

        let source_url = self.resolve_source_url(kind, id).await?;
        let bytes = self.download(&source_url).await?;
        let processed = blocking(move || downscale_image(kind, bytes)).await?;

        let write_path = file_path.clone();
        blocking(move || write_atomic(&write_path, &processed)).await?;

        Ok(file_path)
    }

    /// Gets (creating on first use) the async lock guarding downloads for
    /// `(kind, id)`. Held for `ensure_image`'s whole "check -> resolve ->
    /// download -> write" critical section, so concurrent callers for the
    /// same asset serialize instead of racing.
    fn lock_for(&self, kind: AssetKind, id: &str) -> Arc<tokio::sync::Mutex<()>> {
        let mut locks = self.image_locks.lock().unwrap();
        locks
            .entry((kind, id.to_string()))
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone()
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
                // Already validated as parseable/in-range by `validate_id`
                // — a parse failure here would mean `ensure_image` had a
                // bug letting an invalid id through.
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
            AssetKind::CardLarge => self.large_art_url(id).await,
            AssetKind::Chroma => Ok(self.fetch_or_load_chroma(id).await?.full_render),
        }
    }

    /// `/v1/playercards/{uuid}` — fetched (and cached to disk as a tiny
    /// JSON file) the first time this card is seen, never bulk-downloaded
    /// (see the module docs). `uuid` has already passed [`validate_id`] by
    /// the time this is reachable (only called from `resolve_source_url`,
    /// itself only called from `ensure_image` after validation).
    async fn fetch_or_load_playercard(&self, uuid: &str) -> Result<PlayerCardEntry, AssetError> {
        let path = self.dir.join("playercards").join(format!("{uuid}.json"));

        let cached: Option<PlayerCardEntry> = blocking({
            let path = path.clone();
            move || Ok(read_json(&path).ok())
        })
        .await?;
        if let Some(entry) = cached {
            return Ok(entry);
        }
        self.fetch_playercard(uuid).await
    }

    /// Always hits `/v1/playercards/{uuid}` and rewrites the cached json.
    async fn fetch_playercard(&self, uuid: &str) -> Result<PlayerCardEntry, AssetError> {
        let path = self.dir.join("playercards").join(format!("{uuid}.json"));

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
            // A card without `largeArt` must not break its small/wide art.
            large_art: Some(str_field(data, "largeArt").unwrap_or_default()),
        };

        blocking({
            let path = path.clone();
            let entry = entry.clone();
            move || write_json(&path, &entry)
        })
        .await?;
        Ok(entry)
    }

    /// R107: the `largeArt` url. A cached entry that predates `largeArt`
    /// is refetched once (which rewrites the cache, so it can't repeat);
    /// a card that still has none is `NotFound` — never another image kind.
    async fn large_art_url(&self, uuid: &str) -> Result<String, AssetError> {
        let mut entry = self.fetch_or_load_playercard(uuid).await?;
        if entry.large_art.is_none() {
            entry = self.fetch_playercard(uuid).await?;
        }
        entry
            .large_art
            .filter(|url| !url.is_empty())
            .ok_or(AssetError::NotFound)
    }

    /// `/v1/weapons/skinchromas/{uuid}` — the single-chroma endpoint, so
    /// the ~3.5MB full `/v1/weapons` table is never fetched (see the
    /// module docs). `uuid` has already passed [`validate_id`] — same
    /// reachability note as [`fetch_or_load_playercard`].
    async fn fetch_or_load_chroma(&self, uuid: &str) -> Result<ChromaEntry, AssetError> {
        let path = self.dir.join("chromas").join(format!("{uuid}.json"));

        let cached: Option<ChromaEntry> = blocking({
            let path = path.clone();
            move || Ok(read_json(&path).ok())
        })
        .await?;
        if let Some(entry) = cached {
            return Ok(entry);
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

        blocking({
            let path = path.clone();
            let entry = entry.clone();
            move || write_json(&path, &entry)
        })
        .await?;
        Ok(entry)
    }

    /// Downloads image bytes via the longer-timeout `image_client`
    /// (fix-round item 10 / R37).
    async fn download(&self, url: &str) -> Result<Vec<u8>, AssetError> {
        let resp = self.image_client.get(url).send().await.map_err(net_err)?;
        if !resp.status().is_success() {
            return Err(AssetError::Http(resp.status().as_u16()));
        }
        resp.bytes().await.map(|b| b.to_vec()).map_err(net_err)
    }

    /// JSON calls (tables, version, per-id metadata) via the short-timeout
    /// `json_client`.
    async fn get_json(&self, url: &str) -> Result<Value, AssetError> {
        let resp = self.json_client.get(url).send().await.map_err(net_err)?;
        if !resp.status().is_success() {
            return Err(AssetError::Http(resp.status().as_u16()));
        }
        resp.json().await.map_err(net_err)
    }
}

/// Downscales `bytes` to `kind`'s target width (see
/// [`AssetKind::downscale_target_width`]), preserving aspect ratio, if the
/// source is wider than that target. Re-encodes as PNG regardless of the
/// source format (every valorant-api image asset is PNG or JPEG; PNG is a
/// safe, lossless, universally-supported cache format for both). Returns
/// `bytes` unchanged for a kind with no downscale target, or a source
/// already at/under the target width.
///
/// A free function (not a method) so it can be moved into a
/// [`tokio::task::spawn_blocking`] closure by value, with no `&self`
/// lifetime to fight (fix-round item 10 / R37 — this is the CPU-bound
/// decode/resize step).
fn downscale_image(kind: AssetKind, bytes: Vec<u8>) -> Result<Vec<u8>, AssetError> {
    let Some(target_width) = kind.downscale_target_width() else {
        return Ok(bytes);
    };

    let img = image::load_from_memory(&bytes).map_err(|e| AssetError::Decode(e.to_string()))?;
    if img.width() <= target_width {
        return Ok(bytes);
    }

    let target_height =
        ((img.height() as u64 * target_width as u64) / img.width() as u64).max(1) as u32;
    // `resize_exact`, not `resize`: `target_height` was already computed
    // to preserve aspect ratio above, and `resize`'s own aspect-fit-
    // within-a-box logic can round to one pixel narrower than
    // `target_width` (double rounding) — `resize_exact` honors the exact
    // dimensions already worked out.
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

fn load_tables_from_disk_sync(dir: &Path) -> Result<Tables, AssetError> {
    Ok(Tables {
        agents: read_json(&dir.join("agents.json"))?,
        maps: read_json(&dir.join("maps.json"))?,
        tiers: read_json(&dir.join("tiers.json"))?,
        seasons: read_json(&dir.join("seasons.json"))?,
    })
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

/// A process-wide counter for [`write_atomic`]'s temp-file names — plain
/// uniqueness, not a security boundary (the temp file briefly exists in
/// the same, already-trusted cache directory).
static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Writes `bytes` to `path` atomically: written to a temp file in the same
/// directory, then renamed into place (fix-round item 5). `std::fs::rename`
/// is atomic within a single filesystem/directory on every platform this
/// app targets, so a crash mid-write can never leave a corrupt/partial
/// file at `path` — readers only ever see the old complete file or the new
/// complete file, never something in between.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), AssetError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(io_err)?;
    }
    let n = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp_path = path.with_extension(format!("tmp-{}-{n}", std::process::id()));
    std::fs::write(&tmp_path, bytes).map_err(io_err)?;
    std::fs::rename(&tmp_path, path).map_err(io_err)?;
    Ok(())
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), AssetError> {
    let bytes = serde_json::to_vec(value).map_err(|e| AssetError::Decode(e.to_string()))?;
    write_atomic(path, &bytes)
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, AssetError> {
    let bytes = std::fs::read(path).map_err(io_err)?;
    serde_json::from_slice(&bytes).map_err(|e| AssetError::Decode(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_id_accepts_a_well_formed_uuid() {
        assert!(validate_id(AssetKind::Agent, "aaaaaaaa-0000-0000-0000-000000000001").is_ok());
    }

    #[test]
    fn validate_id_rejects_path_traversal_attempts() {
        for kind in [
            AssetKind::Agent,
            AssetKind::CardSmall,
            AssetKind::CardWide,
            AssetKind::CardLarge,
            AssetKind::Chroma,
            AssetKind::MapList,
            AssetKind::MapSplash,
        ] {
            assert!(matches!(
                validate_id(kind, "../../../etc/passwd"),
                Err(AssetError::NotFound)
            ));
            assert!(matches!(
                validate_id(kind, "..%2f..%2fsecret"),
                Err(AssetError::NotFound)
            ));
            assert!(matches!(
                validate_id(kind, "a/../../b"),
                Err(AssetError::NotFound)
            ));
        }
    }

    #[test]
    fn validate_id_accepts_an_in_range_tier_and_rejects_garbage() {
        assert!(validate_id(AssetKind::Tier, "15").is_ok());
        assert!(matches!(
            validate_id(AssetKind::Tier, "../../etc/passwd"),
            Err(AssetError::NotFound)
        ));
        assert!(matches!(
            validate_id(AssetKind::Tier, "9999"),
            Err(AssetError::NotFound)
        ));
        assert!(matches!(
            validate_id(AssetKind::Tier, "not-a-number"),
            Err(AssetError::NotFound)
        ));
    }
}
