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
//! - `weapons/skins` (1400+ entries) are never bulk-fetched either: a skin
//!   a player has equipped is fetched by uuid
//!   (`/v1/weapons/skins/{uuid}`, [`AssetCache::weapon_skin`]), reduced to
//!   its name, content tier and chroma names (~300 bytes) and kept on disk
//!   and in memory forever after. The five content tiers *are* a bulk table
//!   (`/v1/contenttiers`, a few hundred bytes).
//! - **Exception, R189 (user 2026-10-22: the Skins screen shows the user's
//!   full owned collection):** `/v1/weapons` *is* fetched once per
//!   valorant-api version, by [`AssetCache::skin_catalog`] only — the first
//!   time the Skins screen (or one of its images) is opened, never at
//!   startup. Mapping owned skin levels to skins, weapons and editions
//!   needs every skin, and one bulk call is far lighter than ~100 per-skin
//!   calls. The ~3.6MB answer is decoded into typed rows that skip every
//!   unread field (chromas, videos, asset paths) on the blocking pool, and
//!   only a compact derived file is kept (`skincatalog.json`, ~200KB:
//!   per weapon its uuid/name/icon/melee flag, per skin its uuid, name,
//!   first level, content tier and weapon). Default ("Standard", the
//!   weapon's `defaultSkinUuid`) and "Random Favorite Skin" entries are
//!   dropped while building it. It stays in memory only after first use.
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
//! - [`ensure_image`](AssetCache::ensure_image) downscales every kind
//!   whose real pixel size is far larger than the UI ever displays it
//!   (measured against a live `valorant-api.com` response — see
//!   task-5a-report.md): agent `displayIcon` (1024x1024, shown as a ≤64px
//!   head icon) and map `splash` (1920x1080, shown ≤1000px wide). Rank
//!   tier icons (256x256) are downscaled too, for the same reason at a
//!   smaller scale, and so are the player card's `largeArt` (160px wide), the
//!   weapon-skin renders (256px wide, shown at most 108x38), the
//!   content-tier icons (32px, shown at 13x13) and the map `listViewIcon`
//!   (384px wide; B9). A decoded image costs width x height x 4 bytes
//!   of WebView RAM whatever its file size, so a source bigger than its
//!   display size is pure waste: `listViewIcon` is 456x100 for 25 maps but
//!   3840x2160 (33MB decoded) for Basic Training and The Range. Every other
//!   kind is already close to its display size, or is not shown anywhere
//!   (`smallArt` 128x128, `wideArt` 452x128, `splash`), and is cached as-is.
//!   A file already on disk is served as it is, whatever its size: the
//!   target only applies to a download.
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
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use opd1_core::model::WeaponSkin;
use opd1_core::stats::Act;
use opd1_core::wire::{Collection, Edition, OwnedSkin, WeaponInfo};

use crate::locks::{locked, read_lock, write_lock};
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
/// Where valorant-api serves its images (R189: the skin level icons'
/// URL is built from it, see [`CatalogSkin::icon`]).
const DEFAULT_MEDIA_BASE: &str = "https://media.valorant-api.com";

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
    /// `/v1/maps`' `listViewIcon` (the row-background image), downscaled to
    /// 384px wide, aspect kept. `id` = map uuid.
    MapList,
    /// `/v1/maps`' `splash`. `id` = map uuid.
    MapSplash,
    /// `/v1/weapons/skinchromas/{uuid}`'s `fullRender`, downscaled to 256px
    /// wide. `id` = chroma uuid.
    Chroma,
    /// `/v1/contenttiers`' `displayIcon` (the Select / Deluxe / Premium /
    /// Exclusive / Ultra badge), downscaled to 32px. `id` = content-tier
    /// uuid (a skin's `contentTierUuid`).
    ContentTier,
    /// `/v1/weapons`' `displayIcon` (the weapon's silhouette), downscaled
    /// to 160px wide. `id` = weapon uuid, from the skin catalog (R189).
    WeaponIcon,
    /// A skin level's `displayIcon` (the Skins screen's artwork),
    /// downscaled to 256px wide. `id` = skin level uuid (an `OwnedSkin`'s
    /// `levelId`). A level the skin catalog doesn't know, or one without an
    /// icon, is `NotFound` (R189).
    SkinLevel,
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
            AssetKind::ContentTier => "contentTier",
            AssetKind::WeaponIcon => "weaponIcon",
            AssetKind::SkinLevel => "skinLevel",
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
            AssetKind::Chroma => Some(256),     // wide render, shown <=108x38 (R79)
            AssetKind::ContentTier => Some(32), // 256px source, shown 13x13
            // R189: weapon silhouettes (~500px wide sources) shown as small
            // filter/tile icons; skin artwork (~512px wide) shown on tiles.
            AssetKind::WeaponIcon => Some(160),
            AssetKind::SkinLevel => Some(256),
            // 456x100 source (two maps: 3840x2160), shown cover-cropped at
            // 116x32 (top bar), 64x36 / 56x32 (rows) and 150x56 (match header).
            // The 150x56 box binds on height: 56 css px x 1.5 DPR = 84 device
            // px, and 384 wide keeps 84 high at the source's 4.56 aspect.
            AssetKind::MapList => Some(384),
            // Not shown anywhere (no `assetUrl` use): nothing to shrink.
            AssetKind::CardSmall | AssetKind::CardWide => None,
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
        | AssetKind::Chroma
        | AssetKind::ContentTier
        | AssetKind::WeaponIcon
        | AssetKind::SkinLevel => is_valid_uuid(id),
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
struct ContentTierEntry {
    uuid: String,
    display_icon: String,
    /// `Select` / `Deluxe` / … (R189: the edition). Absent in a file cached
    /// before it was read: [`edition_for`] then falls back to the known uuids.
    #[serde(default)]
    dev_name: Option<String>,
    /// 0 (Select) … 4 (Ultra).
    #[serde(default)]
    rank: Option<u64>,
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
    /// Empty when loaded from a disk cache written before content tiers
    /// existed (see [`AssetCache::disk_cache_matches`]).
    content_tiers: Vec<ContentTierEntry>,
}

impl Tables {
    /// `TablesNotLoaded` while nothing was ever fetched or read from disk.
    fn ensure_loaded(&self) -> Result<(), AssetError> {
        if self.agents.is_empty()
            && self.maps.is_empty()
            && self.tiers.is_empty()
            && self.seasons.is_empty()
        {
            return Err(AssetError::TablesNotLoaded);
        }
        Ok(())
    }
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

/// One skin chroma, reduced to what a loadout shows.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SkinChroma {
    /// Lower-case uuid.
    uuid: String,
    /// The `X` of `(Variant N X)`; `None` for the base chroma.
    name: Option<String>,
}

/// One weapon skin as `/v1/weapons/skins/{uuid}` names it, reduced to
/// ~300 bytes: cached on disk (`skins/{uuid}.json`) and in memory forever.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SkinEntry {
    /// The display name minus the trailing weapon word.
    name: String,
    tier_id: Option<String>,
    chromas: Vec<SkinChroma>,
}

impl SkinEntry {
    /// The skin with `chroma_id` (lower-case) equipped. A chroma the skin
    /// doesn't list is still that skin, without a variant name.
    fn weapon_skin(&self, chroma_id: &str) -> WeaponSkin {
        WeaponSkin {
            chroma_id: chroma_id.to_string(),
            skin_name: self.name.clone(),
            chroma_name: self
                .chromas
                .iter()
                .find(|c| c.uuid == chroma_id)
                .and_then(|c| c.name.clone()),
            tier_id: self.tier_id.clone(),
        }
    }

    fn from_response(data: &Value) -> Option<Self> {
        let display_name = str_field(data, "displayName")?;
        let chromas = data
            .get("chromas")
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .filter_map(|c| {
                        Some(SkinChroma {
                            uuid: str_field(c, "uuid")?.to_ascii_lowercase(),
                            name: chroma_variant_name(&str_field(c, "displayName")?),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        Some(Self {
            name: skin_display_name(&display_name),
            tier_id: str_field(data, "contentTierUuid").filter(|t| !t.is_empty()),
            chromas,
        })
    }
}

/// The four weapons a loadout shows; a skin's display name ends in its
/// weapon's name (`"Reaver Vandal"`).
const WEAPON_WORDS: [&str; 4] = ["Vandal", "Phantom", "Operator", "Sheriff"];

/// A skin's display name minus the trailing weapon word (`"Reaver Vandal"`
/// -> `"Reaver"`), kept as is when it doesn't end in one of the four
/// weapons or nothing would be left.
fn skin_display_name(display: &str) -> String {
    let name = display.trim();
    for word in WEAPON_WORDS {
        let Some(split) = name.len().checked_sub(word.len() + 1) else {
            continue;
        };
        if name.is_char_boundary(split)
            && name[split..].starts_with(' ')
            && name[split + 1..].eq_ignore_ascii_case(word)
        {
            let rest = name[..split].trim_end();
            if !rest.is_empty() {
                return rest.to_string();
            }
        }
    }
    name.to_string()
}

/// The `X` of a chroma display name's `(Variant N X)` (`"Reaver Vandal
/// Level 4\r\n(Variant 1 Red)"` -> `"Red"`); `None` for the base chroma
/// (named like the skin) and for anything else.
fn chroma_variant_name(display: &str) -> Option<String> {
    let rest = &display[display.find("(Variant ")? + "(Variant ".len()..];
    let inner = &rest[..rest.find(')')?];
    let (number, name) = inner.trim_start().split_once(' ')?;
    if number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let name = name.trim();
    (!name.is_empty()).then(|| name.to_string())
}

// ---------------------------------------------------------------------
// Skin catalog (R189: the Skins screen)
// ---------------------------------------------------------------------

/// The standard store prices per edition (VP), as the design's Skins screen
/// shows them: Select 875, Deluxe 1275, Premium 1775, Exclusive 2175, Ultra
/// 2475; a melee costs twice its edition. Riot's entitlements answer has no
/// price, so these are the edition's usual prices, not what the user paid.
pub fn edition_vp(edition: Edition) -> u32 {
    match edition {
        Edition::Select => 875,
        Edition::Deluxe => 1275,
        Edition::Premium => 1775,
        Edition::Exclusive => 2175,
        Edition::Ultra => 2475,
    }
}

/// An estimate: the Champions bundles' skins (`Champions 2021 …` onward)
/// have sold at 2675 VP; the design applies it to their melee too.
pub const CHAMPIONS_VP: u32 = 2675;

/// The standard price of a skin named `name` (valorant-api's display name)
/// of `edition`, on a melee or not: [`CHAMPIONS_VP`] for a Champions skin,
/// else [`edition_vp`] (doubled on a melee).
pub fn standard_vp(edition: Edition, melee: bool, name: &str) -> u32 {
    if is_champions(name) {
        CHAMPIONS_VP
    } else if melee {
        edition_vp(edition) * 2
    } else {
        edition_vp(edition)
    }
}

fn is_champions(name: &str) -> bool {
    name.trim()
        .get(..10)
        .is_some_and(|p| p.eq_ignore_ascii_case("champions "))
}

/// valorant-api's five content tiers, for a cached `contenttiers.json`
/// that predates `devName` (checked against `/v1/contenttiers`,
/// 2026-10-22).
const KNOWN_TIERS: [(&str, Edition); 5] = [
    ("12683d76-48d7-84a3-4e09-6985794f0445", Edition::Select),
    ("0cebb8be-46d7-c12a-d306-e9907bfc5a25", Edition::Deluxe),
    ("60bca009-4182-7998-dee7-b8a2558dc369", Edition::Premium),
    ("e046854e-406c-37f4-6607-19a9ba8426fc", Edition::Exclusive),
    ("411e4a55-4e59-7757-41f0-86a53f101bb5", Edition::Ultra),
];

/// The edition of content tier `uuid`: by its `devName`, else its `rank`
/// (0 Select … 4 Ultra), else the known uuid table; `None` when unknown.
fn edition_for(tiers: &[ContentTierEntry], uuid: &str) -> Option<Edition> {
    let by_name = |name: &str| match name.trim().to_ascii_lowercase().as_str() {
        "select" => Some(Edition::Select),
        "deluxe" => Some(Edition::Deluxe),
        "premium" => Some(Edition::Premium),
        "exclusive" => Some(Edition::Exclusive),
        "ultra" => Some(Edition::Ultra),
        _ => None,
    };
    let by_rank = |rank: u64| match rank {
        0 => Some(Edition::Select),
        1 => Some(Edition::Deluxe),
        2 => Some(Edition::Premium),
        3 => Some(Edition::Exclusive),
        4 => Some(Edition::Ultra),
        _ => None,
    };
    tiers
        .iter()
        .find(|t| t.uuid.eq_ignore_ascii_case(uuid))
        .and_then(|t| {
            t.dev_name
                .as_deref()
                .and_then(by_name)
                .or_else(|| t.rank.and_then(by_rank))
        })
        .or_else(|| {
            KNOWN_TIERS
                .iter()
                .find(|(id, _)| id.eq_ignore_ascii_case(uuid))
                .map(|&(_, e)| e)
        })
}

/// A skin's display name minus its weapon's name (`"Reaver Vandal"` on the
/// Vandal -> `"Reaver"`), as the design names skins (the weapon is shown
/// beside it). Kept whole when it doesn't end in the weapon's name (every
/// melee: `"Reaver Karambit"`) or nothing would be left.
fn strip_weapon_name(display: &str, weapon: &str) -> String {
    let name = display.trim();
    let weapon = weapon.trim();
    if !weapon.is_empty() {
        if let Some(split) = name.len().checked_sub(weapon.len() + 1) {
            if name.is_char_boundary(split)
                && name[split..].starts_with(' ')
                && name[split + 1..].eq_ignore_ascii_case(weapon)
            {
                let rest = name[..split].trim_end();
                if !rest.is_empty() {
                    return rest.to_string();
                }
            }
        }
    }
    name.to_string()
}

/// One weapon of the catalog.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CatalogWeapon {
    /// Lower-case uuid.
    uuid: String,
    name: String,
    melee: bool,
    /// Its `displayIcon` (the [`AssetKind::WeaponIcon`] source).
    icon: Option<String>,
}

/// One sellable skin of the catalog (defaults already dropped).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CatalogSkin {
    /// First level uuid, lower-case: what Riot's entitlements list, and the
    /// key the skins are sorted by.
    level: String,
    /// Skin uuid, lower-case.
    uuid: String,
    /// The full display name (`"Reaver Vandal"`).
    name: String,
    /// Index into [`SkinCatalog::tiers`]; `None` without a content tier.
    tier: Option<u8>,
    /// Index into [`SkinCatalog::weapons`].
    weapon: u8,
    /// Whether the first level has a `displayIcon`. valorant-api serves it
    /// at `{media}/weaponskinlevels/{level}/displayicon.png` (all 1432 skins
    /// on 2026-10-22), so the URL itself isn't kept.
    icon: bool,
}

/// valorant-api's weapons and skins reduced to what the Skins screen uses,
/// for one valorant-api version (`skincatalog.json`). Built by
/// [`AssetCache::skin_catalog`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkinCatalog {
    version: String,
    weapons: Vec<CatalogWeapon>,
    /// Distinct content tier uuids, lower-case.
    tiers: Vec<String>,
    /// Sorted by `level`.
    skins: Vec<CatalogSkin>,
}

/// `/v1/weapons`, typed so that only the read fields are kept while
/// decoding (every other key is skipped, never built into a `Value`).
/// Everything is optional: a malformed entry is dropped, not the catalog.
#[derive(Deserialize)]
struct RawWeapons {
    #[serde(default, deserialize_with = "null_vec")]
    data: Vec<RawWeapon>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawWeapon {
    uuid: Option<String>,
    display_name: Option<String>,
    category: Option<String>,
    default_skin_uuid: Option<String>,
    display_icon: Option<String>,
    #[serde(default, deserialize_with = "null_vec")]
    skins: Vec<RawSkin>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawSkin {
    uuid: Option<String>,
    display_name: Option<String>,
    content_tier_uuid: Option<String>,
    #[serde(default, deserialize_with = "null_vec")]
    levels: Vec<RawLevel>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawLevel {
    uuid: Option<String>,
    display_icon: Option<String>,
}

/// A list that is empty when `null`.
fn null_vec<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Option::<Vec<T>>::deserialize(deserializer)?.unwrap_or_default())
}

/// A default skin: the weapon's `defaultSkinUuid`, or a tierless
/// `Standard …` / `Random Favorite Skin` entry.
fn is_default_skin(tier: Option<&str>, uuid: &str, default_uuid: Option<&str>, name: &str) -> bool {
    default_uuid.is_some_and(|d| d.eq_ignore_ascii_case(uuid))
        || (tier.is_none_or(str::is_empty)
            && (name.starts_with("Standard ") || name.starts_with("Random Favorite")))
}

impl SkinCatalog {
    /// Builds the catalog from a `/v1/weapons` body. A weapon or skin
    /// without a uuid-shaped id or a name is dropped, and so is a skin with
    /// no levels; at most 256 weapons (there are 21) and 256 content tiers
    /// (there are 5) are kept.
    fn from_weapons_json(version: String, bytes: &[u8]) -> Result<Self, AssetError> {
        let raw: RawWeapons =
            serde_json::from_slice(bytes).map_err(|e| AssetError::Decode(e.to_string()))?;
        let mut weapons = Vec::new();
        let mut tiers: Vec<String> = Vec::new();
        let mut skins = Vec::new();
        for w in raw.data {
            let (Some(uuid), Some(name)) = (w.uuid, w.display_name) else {
                continue;
            };
            let uuid = uuid.to_ascii_lowercase();
            if !is_valid_uuid(&uuid) || name.trim().is_empty() || weapons.len() > u8::MAX as usize {
                continue;
            }
            let index = weapons.len() as u8;
            let default_uuid = w.default_skin_uuid.as_deref();
            for skin in &w.skins {
                let (Some(skin_uuid), Some(skin_name)) = (&skin.uuid, &skin.display_name) else {
                    continue;
                };
                let skin_uuid = skin_uuid.to_ascii_lowercase();
                let skin_name = skin_name.trim();
                let Some(first) = skin.levels.first() else {
                    continue;
                };
                let Some(level) = first.uuid.as_deref().map(str::to_ascii_lowercase) else {
                    continue;
                };
                let tier_uuid = skin.content_tier_uuid.as_deref();
                if !is_valid_uuid(&skin_uuid)
                    || !is_valid_uuid(&level)
                    || skin_name.is_empty()
                    || is_default_skin(tier_uuid, &skin_uuid, default_uuid, skin_name)
                {
                    continue;
                }
                let tier = match tier_uuid
                    .map(str::to_ascii_lowercase)
                    .filter(|t| is_valid_uuid(t))
                {
                    None => None,
                    Some(t) => match tiers.iter().position(|x| *x == t) {
                        Some(i) => Some(i as u8),
                        None if tiers.len() <= u8::MAX as usize => {
                            tiers.push(t);
                            Some((tiers.len() - 1) as u8)
                        }
                        None => None,
                    },
                };
                skins.push(CatalogSkin {
                    level,
                    uuid: skin_uuid,
                    name: skin_name.to_string(),
                    tier,
                    weapon: index,
                    icon: first.display_icon.as_deref().is_some_and(|i| !i.is_empty()),
                });
            }
            weapons.push(CatalogWeapon {
                uuid,
                name: name.trim().to_string(),
                melee: w
                    .category
                    .as_deref()
                    .is_some_and(|c| c.ends_with("::Melee")),
                icon: w.display_icon.filter(|i| !i.is_empty()),
            });
        }
        if weapons.is_empty() {
            return Err(AssetError::Decode("no weapons in /v1/weapons".into()));
        }
        skins.sort_by(|a, b| a.level.cmp(&b.level));
        skins.dedup_by(|a, b| a.level == b.level);
        Ok(Self {
            version,
            weapons,
            tiers,
            skins,
        })
    }

    fn skin_by_level(&self, level: &str) -> Option<&CatalogSkin> {
        let level = level.to_ascii_lowercase();
        self.skins
            .binary_search_by(|s| s.level.as_str().cmp(level.as_str()))
            .ok()
            .map(|i| &self.skins[i])
    }

    fn weapon(&self, uuid: &str) -> Option<&CatalogWeapon> {
        self.weapons
            .iter()
            .find(|w| w.uuid.eq_ignore_ascii_case(uuid))
    }

    /// Every weapon, in valorant-api's order.
    pub fn weapons(&self) -> Vec<WeaponInfo> {
        self.weapons
            .iter()
            .map(|w| WeaponInfo {
                id: w.uuid.clone(),
                name: w.name.clone(),
            })
            .collect()
    }

    /// The skins whose first level is in `owned_levels` (Riot's owned skin
    /// levels; an upgrade level or an unknown id matches nothing), each
    /// once, in weapon order then by name. `edition` names a content tier.
    fn owned_with(
        &self,
        owned_levels: &[String],
        edition: impl Fn(&str) -> Option<Edition>,
    ) -> Vec<OwnedSkin> {
        let mut found: Vec<&CatalogSkin> = owned_levels
            .iter()
            .filter_map(|l| self.skin_by_level(l))
            .collect();
        found.sort_by(|a, b| (a.weapon, &a.name, &a.uuid).cmp(&(b.weapon, &b.name, &b.uuid)));
        found.dedup_by(|a, b| a.uuid == b.uuid);
        found
            .into_iter()
            .filter_map(|s| {
                let weapon = self.weapons.get(s.weapon as usize)?;
                let edition = s
                    .tier
                    .and_then(|t| self.tiers.get(t as usize))
                    .and_then(|t| edition(t));
                Some(OwnedSkin {
                    skin_id: s.uuid.clone(),
                    level_id: s.level.clone(),
                    name: strip_weapon_name(&s.name, &weapon.name),
                    weapon_id: weapon.uuid.clone(),
                    edition,
                    vp: edition.map(|e| standard_vp(e, weapon.melee, &s.name)),
                })
            })
            .collect()
    }
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
    /// [`DEFAULT_MEDIA_BASE`]; the mock server's base in tests.
    media_base: String,
    dir: PathBuf,
    tables: RwLock<Tables>,
    image_locks: ImageLocks,
    /// Every weapon skin seen so far (lower-case skin uuid -> its reduced
    /// entry): what [`AssetCache::weapon_skin_cached`] answers from, with
    /// no filesystem access (R37). Filled by [`AssetCache::weapon_skin`].
    skins: RwLock<HashMap<String, SkinEntry>>,
    /// Per-skin async locks, so concurrent [`AssetCache::weapon_skin`]
    /// calls for one skin make one request.
    skin_locks: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    /// Set when a newer table set failed to download and the old disk copy
    /// stands in ([`AssetCache::tables_stale`]).
    stale: AtomicBool,
    /// R189: the skin catalog once used ([`AssetCache::skin_catalog`]);
    /// the async lock also coalesces concurrent builds into one download.
    catalog: tokio::sync::Mutex<Option<Arc<SkinCatalog>>>,
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

        // `gzip`: on purpose, for both clients (R159 review ruling). reqwest's
        // `gzip` feature makes it every client's default; valorant-api's JSON
        // tables are large and it is an ordinary CDN, so they stay compressed
        // (only the local Riot client opts out, in `session.rs`).
        let json_client = reqwest::Client::builder()
            .timeout(JSON_TIMEOUT)
            .connect_timeout(CONNECT_TIMEOUT)
            .gzip(true)
            .build()
            .expect("building the valorant-api JSON HTTP client should never fail");
        let image_client = reqwest::Client::builder()
            .timeout(IMAGE_TIMEOUT)
            .connect_timeout(CONNECT_TIMEOUT)
            .gzip(true)
            .build()
            .expect("building the valorant-api image HTTP client should never fail");
        let media_base = if base == DEFAULT_BASE {
            DEFAULT_MEDIA_BASE.to_string()
        } else {
            base.clone()
        };
        Self {
            json_client,
            image_client,
            base,
            media_base,
            dir: cache_dir,
            tables: RwLock::new(Tables::default()),
            image_locks: Mutex::new(HashMap::new()),
            skins: RwLock::new(HashMap::new()),
            skin_locks: Mutex::new(HashMap::new()),
            stale: AtomicBool::new(false),
            catalog: tokio::sync::Mutex::new(None),
        }
    }

    // -- bulk tables --------------------------------------------------

    /// Checks `/v1/version`; if it matches what's on disk (and every table
    /// file is present), loads the cached tables into memory without
    /// another network call and returns `Ok(false)`. Otherwise re-fetches
    /// every bulk table, persists them (trimmed — see the module docs),
    /// and returns `Ok(true)`. A current cache that only lacks
    /// `contenttiers.json` (written before content tiers existed) fetches
    /// just that table (`Ok(true)`); if that one fetch fails the other
    /// four load from disk (`Ok(false)`) and the next call tries again.
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
                    *write_lock(&self.tables) = tables;
                    Ok(false)
                } else {
                    Err(network_err)
                }
            }
        }
    }

    async fn refresh_with_known_version(&self, version: String) -> Result<bool, AssetError> {
        if self.disk_cache_matches(&version) {
            self.stale.store(false, Ordering::Relaxed);
            let tables = blocking({
                let dir = self.dir.clone();
                move || load_tables_from_disk_sync(&dir)
            })
            .await?;
            *write_lock(&self.tables) = tables;
            return Ok(false);
        }

        // A cache from before content tiers existed: its four tables are
        // current, only `contenttiers.json` is missing. Load them from disk
        // and fetch just `/v1/contenttiers`, not all five tables.
        if self.only_content_tiers_missing(&version) {
            let mut tables = blocking({
                let dir = self.dir.clone();
                move || load_tables_from_disk_sync(&dir)
            })
            .await?;
            // A failed fetch keeps the four tables (no content-tier icons
            // yet, like the offline fallback): the session isn't left
            // without tables, and the file stays missing for the next try.
            let content_tiers = match self.fetch_content_tiers().await {
                Ok(content_tiers) => content_tiers,
                Err(e) => {
                    log::warn!("content tiers fetch failed, keeping the cached tables: {e}");
                    *write_lock(&self.tables) = tables;
                    return Ok(false);
                }
            };
            blocking({
                let dir = self.dir.clone();
                let content_tiers = content_tiers.clone();
                move || write_json(&dir.join("contenttiers.json"), &content_tiers)
            })
            .await?;
            tables.content_tiers = content_tiers;
            *write_lock(&self.tables) = tables;
            return Ok(true);
        }

        let fetched = async {
            Ok::<_, AssetError>((
                self.fetch_agents().await?,
                self.fetch_maps().await?,
                self.fetch_latest_tiers().await?,
                self.fetch_seasons().await?,
                self.fetch_content_tiers().await?,
            ))
        }
        .await;
        let (agents, maps, tiers, seasons, content_tiers) = match fetched {
            Ok(all) => all,
            // The version answered but a table did not (a patch is also when
            // a new act starts): the old tables on disk are better than
            // none. The manifest keeps the old version, so the next
            // `refresh_tables` fetches again.
            Err(e) if self.tables_files_exist() => {
                // Said once per stale period, not at every retry.
                if !self.stale.swap(true, Ordering::Relaxed) {
                    log::warn!("asset table fetch failed, keeping the cached tables: {e}");
                }
                let tables = blocking({
                    let dir = self.dir.clone();
                    move || load_tables_from_disk_sync(&dir)
                })
                .await?;
                *write_lock(&self.tables) = tables;
                return Ok(false);
            }
            Err(e) => return Err(e),
        };
        self.stale.store(false, Ordering::Relaxed);

        blocking({
            let dir = self.dir.clone();
            let agents = agents.clone();
            let maps = maps.clone();
            let tiers = tiers.clone();
            let seasons = seasons.clone();
            let content_tiers = content_tiers.clone();
            move || {
                std::fs::create_dir_all(&dir).map_err(io_err)?;
                write_json(&dir.join("agents.json"), &agents)?;
                write_json(&dir.join("maps.json"), &maps)?;
                write_json(&dir.join("tiers.json"), &tiers)?;
                write_json(&dir.join("seasons.json"), &seasons)?;
                write_json(&dir.join("contenttiers.json"), &content_tiers)?;
                write_json(&dir.join("manifest.json"), &Manifest { version })?;
                Ok(())
            }
        })
        .await?;

        *write_lock(&self.tables) = Tables {
            agents,
            maps,
            tiers,
            seasons,
            content_tiers,
        };
        Ok(true)
    }

    /// A single small file read + a handful of `is_file` stats — cheap
    /// enough that this isn't routed through [`blocking`] (unlike the
    /// larger reads/writes elsewhere in this module).
    fn disk_cache_matches(&self, version: &str) -> bool {
        self.core_cache_matches(version) && self.dir.join("contenttiers.json").is_file()
    }

    /// The cache is current (same version, the four core tables on disk)
    /// but has no `contenttiers.json`: a cache written before content tiers
    /// existed. The offline fallback in `refresh_tables` doesn't need the
    /// file (`tables_files_exist`).
    fn only_content_tiers_missing(&self, version: &str) -> bool {
        self.core_cache_matches(version) && !self.dir.join("contenttiers.json").is_file()
    }

    /// The manifest names `version` and the four core tables are on disk.
    fn core_cache_matches(&self, version: &str) -> bool {
        let Ok(bytes) = std::fs::read(self.dir.join("manifest.json")) else {
            return false;
        };
        let Ok(manifest) = serde_json::from_slice::<Manifest>(&bytes) else {
            return false;
        };
        manifest.version == version && self.tables_files_exist()
    }

    /// Whether the loaded tables are an older disk copy because the current
    /// version's download failed: the caller should call
    /// [`refresh_tables`](Self::refresh_tables) again soon.
    pub fn tables_stale(&self) -> bool {
        self.stale.load(Ordering::Relaxed)
    }

    /// The tables every feature needs (names, maps, tiers, acts): enough to
    /// load from disk offline. `contenttiers.json` is checked separately,
    /// where a missing one must trigger a refetch.
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

    /// `/v1/contenttiers`: the five skin editions and their badge icons.
    async fn fetch_content_tiers(&self) -> Result<Vec<ContentTierEntry>, AssetError> {
        let value = self
            .get_json(&format!("{}/v1/contenttiers", self.base))
            .await?;
        let data = data_array(&value)?;
        Ok(data
            .iter()
            .filter_map(|t| {
                Some(ContentTierEntry {
                    uuid: str_field(t, "uuid")?.to_ascii_lowercase(),
                    display_icon: str_field(t, "displayIcon")?,
                    dev_name: str_field(t, "devName"),
                    rank: t.get("rank").and_then(Value::as_u64),
                })
            })
            .collect())
    }

    // -- UI-facing lookups (require refresh_tables() to have run) ------

    fn tables_snapshot(&self) -> Result<Tables, AssetError> {
        let tables = read_lock(&self.tables);
        tables.ensure_loaded()?;
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
    ///
    /// Searches under the read lock and clones only the matched uuid: this
    /// runs once per remembered card on every publish, so it must not copy
    /// the whole [`Tables`] (unlike [`Self::tables_snapshot`]).
    pub fn map_uuid_for_path(&self, riot_map_path: &str) -> Result<Option<String>, AssetError> {
        let tables = read_lock(&self.tables);
        tables.ensure_loaded()?;
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
        let mut locks = locked(&self.image_locks);
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
            AssetKind::WeaponIcon => self
                .skin_catalog()
                .await?
                .weapon(id)
                .and_then(|w| w.icon.clone())
                .ok_or(AssetError::NotFound),
            AssetKind::SkinLevel => {
                let catalog = self.skin_catalog().await?;
                match catalog.skin_by_level(id) {
                    Some(skin) if skin.icon => Ok(format!(
                        "{}/weaponskinlevels/{}/displayicon.png",
                        self.media_base, skin.level
                    )),
                    _ => Err(AssetError::NotFound),
                }
            }
            AssetKind::ContentTier => {
                let tables = self.tables_snapshot()?;
                tables
                    .content_tiers
                    .iter()
                    .find(|t| t.uuid.eq_ignore_ascii_case(id))
                    .map(|t| t.display_icon.clone())
                    .ok_or(AssetError::NotFound)
            }
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

    // -- weapon skins -----------------------------------------------------

    /// The equipped skin named from memory only, or `None` when it wasn't
    /// resolved yet (or an id is malformed): no filesystem, no network, so
    /// it is safe on the async runtime (R37). [`Self::weapon_skin`] fills
    /// it.
    pub fn weapon_skin_cached(&self, skin_id: &str, chroma_id: &str) -> Option<WeaponSkin> {
        if !is_valid_uuid(skin_id) || !is_valid_uuid(chroma_id) {
            return None;
        }
        let skins = read_lock(&self.skins);
        skins
            .get(&skin_id.to_ascii_lowercase())
            .map(|skin| skin.weapon_skin(&chroma_id.to_ascii_lowercase()))
    }

    /// The equipped skin with its names (R83, R97): the skin's name minus
    /// the weapon word, the chroma's variant name, and the content tier.
    /// Reads the skin's cached JSON from disk (on the blocking pool), else
    /// fetches `/v1/weapons/skins/{skin_id}` once and caches it; either way
    /// it lands in the memory map [`Self::weapon_skin_cached`] answers
    /// from. A failed fetch is not cached: the next call tries again.
    /// Malformed ids are `NotFound` before anything is built or requested.
    pub async fn weapon_skin(
        &self,
        skin_id: &str,
        chroma_id: &str,
    ) -> Result<WeaponSkin, AssetError> {
        if !is_valid_uuid(skin_id) || !is_valid_uuid(chroma_id) {
            return Err(AssetError::NotFound);
        }
        let (skin, chroma) = (skin_id.to_ascii_lowercase(), chroma_id.to_ascii_lowercase());
        if let Some(found) = self.weapon_skin_cached(&skin, &chroma) {
            return Ok(found);
        }

        let lock = self.skin_lock(&skin);
        let _guard = lock.lock().await;
        // Another caller may have just filled it while this one waited.
        if let Some(found) = self.weapon_skin_cached(&skin, &chroma) {
            return Ok(found);
        }

        let path = self.dir.join("skins").join(format!("{skin}.json"));
        let from_disk: Option<SkinEntry> = blocking({
            let path = path.clone();
            move || Ok(read_json(&path).ok())
        })
        .await?;
        let entry = match from_disk {
            Some(entry) => entry,
            None => {
                let value = self
                    .get_json(&format!("{}/v1/weapons/skins/{skin}", self.base))
                    .await?;
                let entry = value
                    .get("data")
                    .and_then(SkinEntry::from_response)
                    .ok_or_else(|| AssetError::Decode("missing skin displayName".to_string()))?;
                blocking({
                    let entry = entry.clone();
                    move || write_json(&path, &entry)
                })
                .await?;
                entry
            }
        };
        let found = entry.weapon_skin(&chroma);
        write_lock(&self.skins).insert(skin, entry);
        Ok(found)
    }

    fn skin_lock(&self, skin: &str) -> Arc<tokio::sync::Mutex<()>> {
        locked(&self.skin_locks)
            .entry(skin.to_string())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone()
    }

    // -- skin catalog (R189) ---------------------------------------------

    /// The skin catalog for the current valorant-api version (the one in
    /// the tables' manifest): from memory, else `skincatalog.json`, else
    /// built from one `/v1/weapons` download (see the module docs) and
    /// written there. When the download fails, an older catalog (memory or
    /// disk) stands in until the next call tries again. `TablesNotLoaded`
    /// while no table set was ever stored (the version is unknown).
    pub async fn skin_catalog(&self) -> Result<Arc<SkinCatalog>, AssetError> {
        let version = blocking({
            let path = self.dir.join("manifest.json");
            move || Ok(read_json::<Manifest>(&path).ok().map(|m| m.version))
        })
        .await?
        .ok_or(AssetError::TablesNotLoaded)?;

        let mut slot = self.catalog.lock().await;
        if let Some(cat) = slot.as_ref().filter(|c| c.version == version) {
            return Ok(cat.clone());
        }
        let path = self.dir.join("skincatalog.json");
        let on_disk: Option<SkinCatalog> = blocking({
            let path = path.clone();
            move || Ok(read_json(&path).ok())
        })
        .await?;
        if let Some(cat) = on_disk.as_ref().filter(|c| c.version == version) {
            let cat = Arc::new(cat.clone());
            *slot = Some(cat.clone());
            return Ok(cat);
        }

        // The longer-timeout client: the answer is ~3.6MB before gzip.
        let built = async {
            let bytes = self.download(&format!("{}/v1/weapons", self.base)).await?;
            let version = version.clone();
            blocking(move || SkinCatalog::from_weapons_json(version, &bytes)).await
        }
        .await;
        match built {
            Ok(cat) => {
                blocking({
                    let cat = cat.clone();
                    move || write_json(&path, &cat)
                })
                .await?;
                let cat = Arc::new(cat);
                *slot = Some(cat.clone());
                Ok(cat)
            }
            Err(e) => {
                // An older catalog is better than none; the next call asks
                // again (the version still differs).
                let older = slot.clone().or_else(|| on_disk.map(Arc::new));
                match older {
                    Some(cat) => {
                        log::warn!("skin catalog fetch failed, using an older one: {e}");
                        *slot = Some(cat.clone());
                        Ok(cat)
                    }
                    None => Err(e),
                }
            }
        }
    }

    /// The edition of a content tier, from the cached `/v1/contenttiers`
    /// (or the known uuids when it isn't loaded).
    pub fn edition(&self, tier_uuid: &str) -> Option<Edition> {
        edition_for(&read_lock(&self.tables).content_tiers, tier_uuid)
    }

    /// The Skins screen's collection (R189): the skins of `owned_levels`
    /// (Riot's owned skin level ids) with their editions and standard
    /// prices, and every weapon. `fetched_ms` is when Riot answered.
    pub async fn collection(
        &self,
        owned_levels: &[String],
        fetched_ms: i64,
    ) -> Result<Collection, AssetError> {
        let catalog = self.skin_catalog().await?;
        let tiers = read_lock(&self.tables).content_tiers.clone();
        Ok(Collection {
            items: catalog.owned_with(owned_levels, |t| edition_for(&tiers, t)),
            weapons: catalog.weapons(),
            fetched_ms,
        })
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
        // Missing in a cache from before content tiers: none known yet.
        content_tiers: read_json(&dir.join("contenttiers.json")).unwrap_or_default(),
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
    fn locks_poisoned_by_a_panicking_caller_do_not_stop_later_calls() {
        let dir = tempfile::tempdir().unwrap();
        let cache = AssetCache::new(dir.path().join("assets"));
        std::thread::scope(|s| {
            let holders = [
                s.spawn(|| {
                    let _held = cache.tables.write().unwrap();
                    panic!("a caller panicked while it held the tables lock");
                }),
                s.spawn(|| {
                    let _held = cache.skins.write().unwrap();
                    panic!("a caller panicked while it held the skins lock");
                }),
                s.spawn(|| {
                    let _held = cache.image_locks.lock().unwrap();
                    panic!("a caller panicked while it held the image locks");
                }),
                s.spawn(|| {
                    let _held = cache.skin_locks.lock().unwrap();
                    panic!("a caller panicked while it held the skin locks");
                }),
            ];
            for holder in holders {
                let _ = holder.join();
            }
        });
        assert!(cache.tables.is_poisoned() && cache.skins.is_poisoned());
        assert!(cache.image_locks.is_poisoned() && cache.skin_locks.is_poisoned());

        // Every path that takes one of them still answers (the tables were
        // never loaded: an error, not a panic).
        assert!(cache.meta().is_err());
        assert!(cache.acts().is_err());
        assert!(cache.map_uuid_for_path("/Game/Maps/Ascent/Ascent").is_err());
        assert_eq!(
            cache.weapon_skin_cached(
                "aaaaaaaa-0000-0000-0000-000000000001",
                "bbbbbbbb-0000-0000-0000-000000000002"
            ),
            None
        );
        let _ = cache.lock_for(AssetKind::Agent, "aaaaaaaa-0000-0000-0000-000000000001");
        let _ = cache.skin_lock("a-skin");
    }

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
            AssetKind::ContentTier,
            AssetKind::MapList,
            AssetKind::MapSplash,
            AssetKind::WeaponIcon,
            AssetKind::SkinLevel,
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
    fn skin_name_drops_the_trailing_weapon_word() {
        for (display, want) in [
            ("Reaver Vandal", "Reaver"),
            ("Standard Vandal", "Standard"),
            ("Oni Phantom", "Oni"),
            ("Sentinels of Light Operator", "Sentinels of Light"),
            ("Prime//2.0 Sheriff", "Prime//2.0"),
            // Case-insensitive, and stray whitespace from the API is trimmed.
            ("Glitchpop vandal\r\n", "Glitchpop"),
            // Not ending in one of the four weapons: kept as is.
            ("Sheriff of Nottingham", "Sheriff of Nottingham"),
            ("Reaver Guardian", "Reaver Guardian"),
            // Nothing left after the weapon word: kept as is.
            ("Vandal", "Vandal"),
            ("", ""),
        ] {
            assert_eq!(skin_display_name(display), want, "{display:?}");
        }
    }

    #[test]
    fn chroma_name_is_the_variant_text_or_none() {
        for (display, want) in [
            ("Reaver Vandal Level 4\r\n(Variant 1 Red)", Some("Red")),
            ("Neptune Odin Level 3\n(Variant 1 Black)", Some("Black")),
            ("Some Skin (Variant 12 Deep Blue)", Some("Deep Blue")),
            // The base chroma is named like the skin.
            ("Reaver Vandal", None),
            ("", None),
            // No variant name after the number.
            ("Skin (Variant 3)", None),
            ("Skin (Variant 3 )", None),
            // Only `(Variant N X)` counts.
            ("Skin (Special Edition)", None),
        ] {
            assert_eq!(chroma_variant_name(display).as_deref(), want, "{display:?}");
        }
    }

    /// Pins every kind's downscale target (B9): a change here changes WebView
    /// RAM (width x height x 4 bytes per decoded image) and sharpness at 150%
    /// Windows scaling, so it must be a deliberate edit.
    #[test]
    fn downscale_targets_are_pinned_per_kind() {
        let table = [
            (AssetKind::Agent, Some(128)),
            (AssetKind::Tier, Some(128)),
            (AssetKind::CardLarge, Some(160)),
            (AssetKind::MapSplash, Some(1000)),
            (AssetKind::Chroma, Some(256)),
            (AssetKind::ContentTier, Some(32)),
            (AssetKind::MapList, Some(384)),
            (AssetKind::WeaponIcon, Some(160)),
            (AssetKind::SkinLevel, Some(256)),
            (AssetKind::CardSmall, None),
            (AssetKind::CardWide, None),
        ];
        for (kind, want) in table {
            assert_eq!(kind.downscale_target_width(), want, "{kind:?}");
        }
    }

    /// The map list image's target keeps the tallest use sharp at 1.5x DPR:
    /// the match header's 150x56 box, cover-cropped, needs 56 x 1.5 = 84 device
    /// px of height, and a 456x100 source scaled to the target keeps that.
    #[test]
    fn map_list_target_keeps_the_tallest_use_sharp_at_150_percent() {
        let target = AssetKind::MapList.downscale_target_width().unwrap();
        let scaled_height = 100 * target / 456;
        assert!(scaled_height >= 84, "{scaled_height}");
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

    // -- skin catalog (R189) ------------------------------------------------

    const VANDAL: &str = "9c82e19d-4575-0200-1a81-3eacf00cf872";
    const MELEE: &str = "2f59173c-4bed-b6c3-2191-dea9b58be9c7";
    const PREMIUM: &str = "60bca009-4182-7998-dee7-b8a2558dc369";
    const EXCLUSIVE: &str = "e046854e-406c-37f4-6607-19a9ba8426fc";

    fn uuid(n: u32) -> String {
        format!("{n:08x}-0000-0000-0000-000000000000")
    }

    fn weapons_body() -> Value {
        serde_json::json!({"status": 200, "data": [
            {"uuid": VANDAL, "displayName": "Vandal", "category": "EEquippableCategory::Rifle",
             "defaultSkinUuid": uuid(1), "displayIcon": "https://x/weapons/v.png", "skins": [
                {"uuid": uuid(1), "displayName": "Standard Vandal", "contentTierUuid": null,
                 "levels": [{"uuid": uuid(101), "displayIcon": "i"}]},
                {"uuid": uuid(2), "displayName": "Random Favorite Skin", "contentTierUuid": null,
                 "levels": [{"uuid": uuid(102), "displayIcon": null}]},
                {"uuid": uuid(3), "displayName": "Reaver Vandal", "contentTierUuid": PREMIUM,
                 "chromas": [{"uuid": uuid(9), "displayName": "x"}],
                 "levels": [{"uuid": uuid(103), "displayIcon": "i"}, {"uuid": uuid(203), "displayIcon": "i"}]},
                {"uuid": uuid(4), "displayName": "Champions 2025 Vandal", "contentTierUuid": EXCLUSIVE,
                 "levels": [{"uuid": uuid(104), "displayIcon": "i"}]},
                {"uuid": uuid(5), "displayName": "Battle Pass Vandal", "contentTierUuid": null,
                 "levels": [{"uuid": uuid(105), "displayIcon": null}]},
                // Malformed: no levels, no uuid, a non-uuid level.
                {"uuid": uuid(6), "displayName": "No Levels", "levels": null},
                {"displayName": "No Id", "levels": [{"uuid": uuid(107)}]},
                {"uuid": uuid(8), "displayName": "Bad Level", "levels": [{"uuid": "../x"}]}
            ]},
            {"uuid": MELEE, "displayName": "Melee", "category": "EEquippableCategory::Melee",
             "defaultSkinUuid": uuid(20), "displayIcon": null, "skins": [
                {"uuid": uuid(20), "displayName": "Melee", "contentTierUuid": null,
                 "levels": [{"uuid": uuid(120)}]},
                {"uuid": uuid(21), "displayName": "Reaver Karambit", "contentTierUuid": PREMIUM,
                 "levels": [{"uuid": uuid(121), "displayIcon": "i"}]},
                {"uuid": uuid(22), "displayName": "Champions 2021 Karambit", "contentTierUuid": EXCLUSIVE,
                 "levels": [{"uuid": uuid(122), "displayIcon": "i"}]}
            ]},
            {"displayName": "No Uuid Weapon", "skins": []}
        ]})
    }

    fn catalog() -> SkinCatalog {
        let bytes = serde_json::to_vec(&weapons_body()).unwrap();
        SkinCatalog::from_weapons_json("v1".into(), &bytes).unwrap()
    }

    #[test]
    fn the_catalog_drops_default_placeholder_and_malformed_skins() {
        let cat = catalog();
        assert_eq!(cat.weapons.len(), 2);
        assert!(cat.weapons[1].melee && !cat.weapons[0].melee);
        assert_eq!(cat.weapons[1].icon, None);
        let names: Vec<&str> = cat.skins.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names.len(), 5, "{names:?}");
        for dropped in [
            "Standard Vandal",
            "Random Favorite Skin",
            "Melee",
            "No Levels",
            "No Id",
            "Bad Level",
        ] {
            assert!(!names.contains(&dropped), "{dropped}");
        }
        // Sorted by first level, two content tiers kept once each.
        assert!(cat.skins.windows(2).all(|w| w[0].level < w[1].level));
        assert_eq!(cat.tiers, [PREMIUM, EXCLUSIVE]);
        // The battle-pass skin has no tier and no icon.
        let bp = cat.skin_by_level(&uuid(105)).unwrap();
        assert!(bp.tier.is_none() && !bp.icon);
        // Only the first level is a key.
        assert!(cat.skin_by_level(&uuid(203)).is_none());
        assert!(cat.skin_by_level(&uuid(103).to_ascii_uppercase()).is_some());
    }

    #[test]
    fn a_body_without_weapons_is_a_decode_error() {
        for body in [r#"{"data": []}"#, r#"{"data": null}"#, r#"{}"#, "not json"] {
            assert!(matches!(
                SkinCatalog::from_weapons_json("v".into(), body.as_bytes()),
                Err(AssetError::Decode(_))
            ));
        }
    }

    #[test]
    fn owned_levels_map_to_skins_with_editions_and_standard_prices() {
        let cat = catalog();
        let owned: Vec<String> = [103, 203, 104, 105, 121, 122, 101, 999, 103]
            .iter()
            .map(|&n| uuid(n))
            .collect();
        let items = cat.owned_with(&owned, |t| edition_for(&[], t));
        let got: Vec<(&str, &str, Option<Edition>, Option<u32>)> = items
            .iter()
            .map(|s| (s.name.as_str(), s.weapon_id.as_str(), s.edition, s.vp))
            .collect();
        assert_eq!(
            got,
            [
                // Weapon order, then name; the weapon word is dropped.
                ("Battle Pass", VANDAL, None, None),
                (
                    "Champions 2025",
                    VANDAL,
                    Some(Edition::Exclusive),
                    Some(2675)
                ),
                ("Reaver", VANDAL, Some(Edition::Premium), Some(1775)),
                // Melee: x2, except the Champions flat price.
                (
                    "Champions 2021 Karambit",
                    MELEE,
                    Some(Edition::Exclusive),
                    Some(2675)
                ),
                ("Reaver Karambit", MELEE, Some(Edition::Premium), Some(3550)),
            ]
        );
        assert_eq!(items[2].skin_id, uuid(3));
        assert_eq!(items[2].level_id, uuid(103));
        assert_eq!(cat.weapons().len(), 2);
    }

    #[test]
    fn editions_come_from_dev_name_then_rank_then_the_known_uuids() {
        let entry = |uuid: &str, dev: Option<&str>, rank: Option<u64>| ContentTierEntry {
            uuid: uuid.into(),
            display_icon: String::new(),
            dev_name: dev.map(str::to_string),
            rank,
        };
        let tiers = [
            entry(
                "aaaaaaaa-0000-0000-0000-000000000001",
                Some("Ultra"),
                Some(0),
            ),
            entry("aaaaaaaa-0000-0000-0000-000000000002", None, Some(1)),
            entry("aaaaaaaa-0000-0000-0000-000000000003", Some("?"), Some(9)),
        ];
        assert_eq!(
            edition_for(&tiers, "AAAAAAAA-0000-0000-0000-000000000001"),
            Some(Edition::Ultra)
        );
        assert_eq!(
            edition_for(&tiers, "aaaaaaaa-0000-0000-0000-000000000002"),
            Some(Edition::Deluxe)
        );
        assert_eq!(
            edition_for(&tiers, "aaaaaaaa-0000-0000-0000-000000000003"),
            None
        );
        // A cache from before `devName` was read: the known uuids.
        for (uuid, edition) in KNOWN_TIERS {
            assert_eq!(edition_for(&[], uuid), Some(edition));
        }
        assert_eq!(
            edition_for(&[], "bbbbbbbb-0000-0000-0000-000000000000"),
            None
        );
    }

    #[test]
    fn standard_prices_follow_the_design() {
        for (edition, vp) in [
            (Edition::Select, 875),
            (Edition::Deluxe, 1275),
            (Edition::Premium, 1775),
            (Edition::Exclusive, 2175),
            (Edition::Ultra, 2475),
        ] {
            assert_eq!(standard_vp(edition, false, "Skin Vandal"), vp);
            assert_eq!(standard_vp(edition, true, "Skin Knife"), vp * 2);
            assert_eq!(standard_vp(edition, false, "Champions 2023 Vandal"), 2675);
            assert_eq!(standard_vp(edition, true, "champions 2024 Blade"), 2675);
        }
        // Not the Champions series: only a name that starts with it.
        assert_eq!(standard_vp(Edition::Premium, false, "Champions"), 1775);
        assert_eq!(
            standard_vp(Edition::Premium, false, "VCT Champions Vandal"),
            1775
        );
    }

    #[test]
    fn skin_names_drop_their_own_weapon_name_only() {
        for (display, weapon, want) in [
            ("Reaver Vandal", "Vandal", "Reaver"),
            ("RGX 11z Pro Vandal\r\n", "Vandal", "RGX 11z Pro"),
            ("Reaver Karambit", "Melee", "Reaver Karambit"),
            ("Oni Phantom", "Vandal", "Oni Phantom"),
            ("Vandal", "Vandal", "Vandal"),
            ("Ion Odin", "odin", "Ion"),
            ("", "Vandal", ""),
        ] {
            assert_eq!(strip_weapon_name(display, weapon), want, "{display:?}");
        }
    }
}
