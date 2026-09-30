//! The Riot local-client session: lockfile -> local auth -> entitlements
//! -> remote (pd/glz) headers, with auto re-auth on 401/403 and a
//! `ClientGone` signal the moment the lockfile disappears or changes.
//!
//! All HTTP goes through two private choke points — [`RiotSession::call_local`]
//! and [`RiotSession::call_remote`] — which is also where Task 5's
//! raw-response capture (`crate::raw_capture`) hooks in, via
//! [`RiotSession::raw_captures`].

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, RETRY_AFTER, USER_AGENT};
use reqwest::{Client, Method, Response, StatusCode};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::Value;

use opd1_core::lockfile::parse_lockfile;
use opd1_core::shards::{parse_client_version, parse_shards};

use crate::raw_capture::{excerpt, ErrorDump, RawCaptures, LOG_EXCERPT_CHARS};
use crate::throttle::Throttle;

/// The global remote-call bucket (every pd/glz call): burst 10, 10 rps.
/// Values from Valorant-Scout's `backend/riot_client.py` (`_RIOT_BUCKET`).
const GLOBAL_BURST: f64 = 10.0;
const GLOBAL_RPS: f64 = 10.0;

/// The `/mmr/` bucket, consumed *in addition to* the global bucket for mmr
/// calls. Values from Scout's `_MMR_BUCKET`.
const MMR_BURST: f64 = 24.0;
const MMR_RPS: f64 = 0.4;

/// How many times a 429 is retried (after the first attempt) before giving
/// up with [`RiotError::RateLimited`]. Bounded so a rate-limited field
/// "eventually becomes `—`, never hangs forever" (CLAUDE.md).
const MAX_RATE_LIMIT_RETRIES: u32 = 2;

/// Fallback backoff (seconds) when a `429` has no usable `Retry-After`,
/// growing by this much on each successive retry — matches Scout's
/// `pd_get` backoff (`backoff = 3.0; backoff += 3.0`).
const FALLBACK_BACKOFF_SECS: f64 = 3.0;

/// A `Retry-After` value is only ever trusted up to this many seconds.
/// Guards against a malformed or absurd header (`"inf"`, a huge number)
/// turning into a multi-hour (or panicking, pre-fix) hold — see the Task 3
/// fix-round report.
const MAX_RETRY_AFTER_SECS: f64 = 30.0;

/// Total-request timeouts, matching Scout's `riot_client.py` (`timeout=5`
/// for local calls, `timeout=8` for pd/glz). Without these, a
/// non-responding Riot client/server would hang the caller forever —
/// exactly the kind of thing CLAUDE.md's "a failed field must eventually
/// become `—`" rules out.
const LOCAL_TIMEOUT: Duration = Duration::from_secs(5);
const REMOTE_TIMEOUT: Duration = Duration::from_secs(8);
/// How long to wait for the TCP+TLS handshake alone, on remote calls.
const REMOTE_CONNECT_TIMEOUT: Duration = Duration::from_secs(8);

/// Used when every client-version source fails: Valorant-Scout's own last
/// resort (`riot_client.py::_client_version`), a well-formed version, never a
/// word the server cannot parse.
const FALLBACK_CLIENT_VERSION: &str = "release-09.00";

/// The standard `X-Riot-ClientPlatform` value: base64 of
/// `{"platformType":"PC","platformOS":"Windows","platformOSVersion":"10.0.19042.1.256.64bit","platformChipset":"Unknown"}`.
/// Verbatim from Valorant-Scout's `riot_client.py` and techchrism's
/// `common-components.md` (identical in both sources).
const CLIENT_PLATFORM_B64: &str = "ew0KCSJwbGF0Zm9ybVR5cGUiOiAiUEMiLA0KCSJwbGF0Zm9ybU9TIjogIldpbmRvd3MiLA0KCSJwbGF0Zm9ybU9TVmVyc2lvbiI6ICIxMC4wLjE5MDQyLjEuMjU2LjY0Yml0IiwNCgkicGxhdGZvcm1DaGlwc2V0IjogIlVua25vd24iDQp9";

/// The `User-Agent` every remote (pd/glz) call sends, exactly as
/// Valorant-Scout does (`riot_client.py::headers`). Local calls send none.
const REMOTE_USER_AGENT: &str = "ShooterGame/13 Windows/10.0.19043.1.256.64bit";

/// Everything that can go wrong talking to the Riot client or its remote
/// services.
#[derive(Debug)]
pub enum RiotError {
    /// The lockfile is gone, unreadable, or its port/password changed —
    /// the Riot client this session was connected to is no longer there
    /// (closed or restarted). Callers should drop the session and
    /// reconnect via [`RiotSession::connect`].
    ClientGone,
    /// `ShooterGame.log` doesn't have the `pd.`/`glz-` URLs yet, so the
    /// region/shard can't be determined (e.g. the Riot Client is open but
    /// VALORANT hasn't finished loading).
    ShardsUnavailable,
    /// The local entitlements endpoint didn't return a usable token.
    AuthFailed(String),
    /// A remote call was still rate-limited after
    /// [`MAX_RATE_LIMIT_RETRIES`] retries.
    RateLimited,
    /// A network-level failure (couldn't send the request / read the
    /// response body).
    Http(String),
    /// A non-2xx HTTP response (other than 401/403, which trigger a
    /// re-auth, and 429, which triggers a throttled retry): the status and a
    /// short log-safe excerpt of Riot's answer (R158: its `errorCode`) —
    /// redacted, on one line, every full puuid shortened, at most
    /// [`LOG_EXCERPT_CHARS`] characters; empty when the body was.
    Status(u16, String),
    /// The response body wasn't the JSON shape the caller expected. The text
    /// is serde's message, made log-safe like a [`Status`](Self::Status)
    /// excerpt (a message can quote a fragment of the body).
    Decode(String),
}

impl fmt::Display for RiotError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RiotError::ClientGone => write!(f, "the Riot client is no longer reachable"),
            RiotError::ShardsUnavailable => write!(f, "the client's region/shard isn't known yet"),
            RiotError::AuthFailed(msg) => write!(f, "Riot local auth failed: {msg}"),
            RiotError::RateLimited => write!(f, "rate-limited by Riot after retrying"),
            RiotError::Http(msg) => write!(f, "HTTP error talking to Riot: {msg}"),
            RiotError::Status(code, body) if body.is_empty() => {
                write!(f, "unexpected HTTP status {code} from Riot")
            }
            RiotError::Status(code, body) => {
                write!(f, "unexpected HTTP status {code} from Riot: {body}")
            }
            RiotError::Decode(msg) => write!(f, "couldn't parse Riot's response: {msg}"),
        }
    }
}

impl std::error::Error for RiotError {}

/// The port/password pair a [`RiotSession`] was built from — re-checked
/// (cheaply, from disk) before every call so a Riot client restart is
/// caught immediately instead of hanging on a dead port.
///
/// Deliberately **not** `#[derive(Debug)]`: `password` is the lockfile
/// secret used to build the local Basic-auth header, and the user shares
/// in-app logs (CLAUDE.md), so it must never come out of a `{:?}`.
struct LockfileSnapshot {
    port: u16,
    password: String,
}

/// Deliberately **not** `#[derive(Debug)]`: `access_token`/`entitlements_jwt`
/// are the live Bearer token and entitlements JWT — the same reasoning as
/// [`LockfileSnapshot`].
struct AuthState {
    access_token: String,
    entitlements_jwt: String,
    client_version: String,
    /// Where `client_version` came from (logged once per session, R158).
    client_version_source: VersionSource,
}

/// Where the `X-Riot-ClientVersion` a session sends came from (R158): logged
/// once per session, so a field report says which source the version had.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionSource {
    /// The user's own VALORANT presence (`partyClientVersion`): what the game
    /// itself reports.
    Presence,
    /// `valorant-api.com/v1/version`'s `riotClientVersion`.
    ValorantApi,
    /// The `CI server version:` line of `ShooterGame.log`.
    Log,
    /// Nothing answered: the literal fallback.
    Fallback,
}

impl VersionSource {
    /// The name the log line uses.
    pub fn as_str(self) -> &'static str {
        match self {
            VersionSource::Presence => "presence",
            VersionSource::ValorantApi => "valorant-api",
            VersionSource::Log => "log",
            VersionSource::Fallback => "fallback",
        }
    }
}

#[derive(Debug)]
enum RemoteKind {
    Pd,
    Glz,
}

/// A live connection to the local Riot client, plus everything needed to
/// call `pd.*.a.pvp.net` and `glz-*.*.a.pvp.net` on its behalf.
///
/// `Debug` is hand-written (see [`fmt::Debug`] impl below) rather than
/// derived, so that printing a session — e.g. in a panic message or a
/// future "copy log" feature — can never leak `basic_auth` (a Basic-auth
/// header built from the lockfile password) or the live Bearer
/// token/entitlements JWT held in `auth`.
pub struct RiotSession {
    local_app_data: PathBuf,
    lockfile_snapshot: LockfileSnapshot,
    local_base: String,
    basic_auth: String,
    local_client: Client,
    remote_client: Client,
    pd_base: String,
    glz_base: String,
    puuid: String,
    auth: Mutex<AuthState>,
    global_throttle: Throttle,
    mmr_throttle: Throttle,
    /// The last response body seen at each of `call_local`/`call_remote`,
    /// for Task 5b's `dump_raw()` (via [`RiotSession::raw_captures`]). See
    /// `raw_capture.rs`'s module docs for what is/isn't captured.
    raw_captures: RawCaptures,
}

impl fmt::Debug for RiotSession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RiotSession")
            .field("local_app_data", &self.local_app_data)
            .field("local_base", &self.local_base)
            .field("basic_auth", &"<redacted>")
            .field("pd_base", &self.pd_base)
            .field("glz_base", &self.glz_base)
            .field("puuid", &self.puuid)
            .field("auth", &"<redacted>")
            .finish_non_exhaustive()
    }
}

fn lockfile_path(local_app_data: &Path) -> PathBuf {
    local_app_data
        .join("Riot Games")
        .join("Riot Client")
        .join("Config")
        .join("lockfile")
}

fn shooter_log_path(local_app_data: &Path) -> PathBuf {
    local_app_data
        .join("VALORANT")
        .join("Saved")
        .join("Logs")
        .join("ShooterGame.log")
}

/// Deliberately **not** `#[derive(Debug)]` — `access_token`/`token` are the
/// live Bearer token and entitlements JWT, same reasoning as [`AuthState`].
#[derive(Deserialize)]
struct EntitlementsResponse {
    #[serde(rename = "accessToken")]
    access_token: String,
    subject: String,
    token: String,
}

/// Test-only overrides for [`RiotSession::connect_with_overrides`]: mock
/// server bases in place of the real Riot shards/`valorant-api.com`, and
/// tighter timeouts so a "server never responds" test doesn't have to wait
/// out the real 5s/8s production timeouts. Every field defaults to the
/// real, production behavior.
#[doc(hidden)]
#[derive(Debug, Default)]
pub struct ConnectOverrides {
    pub pd_base: Option<String>,
    pub glz_base: Option<String>,
    pub valorant_api_base: Option<String>,
    pub local_timeout: Option<Duration>,
    pub remote_timeout: Option<Duration>,
}

impl RiotSession {
    /// Connects to the local Riot client: reads the lockfile and
    /// `ShooterGame.log` under `local_app_data`, fetches an entitlements
    /// token (which also gives us `self_puuid`), and determines the
    /// client version to send on remote calls.
    pub async fn connect(local_app_data: &Path) -> Result<Self, RiotError> {
        Self::connect_with_overrides(local_app_data, ConnectOverrides::default()).await
    }

    /// Like [`connect`](Self::connect), but lets tests point the pd/glz/
    /// valorant-api bases at a mock server (and override the request
    /// timeouts, so a "the server never responds" test doesn't have to
    /// wait out the real 5s/8s timeouts) instead of the real Riot shards,
    /// `valorant-api.com`, and production timeouts. Not meant for
    /// production use — the public entry point is `connect`.
    #[doc(hidden)]
    pub async fn connect_with_overrides(
        local_app_data: &Path,
        overrides: ConnectOverrides,
    ) -> Result<Self, RiotError> {
        // `reqwest`'s `rustls-no-provider` feature (chosen over `rustls` to
        // avoid pulling in the much heavier aws-lc-rs/aws-lc-sys backend —
        // see task-3-report.md's fix-round notes) needs a process-wide
        // crypto provider installed before any TLS connection is made.
        // Installing twice (e.g. a second `connect()` call, or another
        // test in the same process) just returns `Err` harmlessly.
        let _ = rustls::crypto::ring::default_provider().install_default();

        let ConnectOverrides {
            pd_base: pd_base_override,
            glz_base: glz_base_override,
            valorant_api_base: valorant_api_base_override,
            local_timeout,
            remote_timeout,
        } = overrides;
        let local_timeout = local_timeout.unwrap_or(LOCAL_TIMEOUT);
        let remote_timeout = remote_timeout.unwrap_or(REMOTE_TIMEOUT);

        let raw_lockfile = std::fs::read_to_string(lockfile_path(local_app_data))
            .map_err(|_| RiotError::ClientGone)?;
        let lockfile = parse_lockfile(&raw_lockfile).map_err(|_| RiotError::ClientGone)?;

        let raw_log = std::fs::read_to_string(shooter_log_path(local_app_data))
            .map_err(|_| RiotError::ShardsUnavailable)?;
        let shards = parse_shards(&raw_log).ok_or(RiotError::ShardsUnavailable)?;

        let local_base = format!("{}://127.0.0.1:{}", lockfile.protocol, lockfile.port);
        let basic_auth = format!(
            "Basic {}",
            BASE64.encode(format!("riot:{}", lockfile.password))
        );

        // `danger_accept_invalid_certs` is ONLY ever set on this client —
        // the local Riot client's cert is self-signed. The remote
        // (pd/glz/valorant-api) client below uses normal certificate
        // validation. `no_gzip`: with reqwest's `gzip` feature on, every
        // client asks for gzip by default; the local client talks to
        // 127.0.0.1, where compression only costs CPU, so it opts out.
        let local_client = Client::builder()
            .danger_accept_invalid_certs(true)
            .timeout(local_timeout)
            .no_gzip()
            .build()
            .map_err(http_error)?;
        // `gzip`: pd/glz bodies (match-details is about 1 MB) come back
        // compressed, as they do for Valorant-Scout's `requests`; reqwest
        // decodes them before `buffer_response`, so captures and parsing see
        // plain JSON. Explicit, because with the feature on it is the default
        // of every client (`assets.rs`' two keep it, on purpose).
        let remote_client = Client::builder()
            .timeout(remote_timeout)
            .connect_timeout(REMOTE_CONNECT_TIMEOUT)
            .gzip(true)
            .build()
            .map_err(http_error)?;

        let pd_base =
            pd_base_override.unwrap_or_else(|| format!("https://pd.{}.a.pvp.net", shards.pd));
        let glz_base = glz_base_override.unwrap_or_else(|| {
            format!(
                "https://glz-{}.{}.a.pvp.net",
                shards.glz_region, shards.glz_shard
            )
        });
        let valorant_api_base =
            valorant_api_base_override.unwrap_or_else(|| "https://valorant-api.com".to_string());

        let entitlements = fetch_entitlements(&local_client, &local_base, &basic_auth).await?;

        let (client_version, client_version_source) = fetch_client_version(
            &local_client,
            &remote_client,
            &local_base,
            &basic_auth,
            &entitlements.subject,
            &valorant_api_base,
            &raw_log,
        )
        .await;
        // Once per session: the version string is not a secret, and which
        // source it came from is the first thing a "the game rejects the
        // header" report needs.
        log::info!(
            "client version {client_version} (from {})",
            client_version_source.as_str()
        );

        Ok(Self {
            local_app_data: local_app_data.to_path_buf(),
            lockfile_snapshot: LockfileSnapshot {
                port: lockfile.port,
                password: lockfile.password,
            },
            local_base,
            basic_auth,
            local_client,
            remote_client,
            pd_base,
            glz_base,
            puuid: entitlements.subject,
            auth: Mutex::new(AuthState {
                access_token: entitlements.access_token,
                entitlements_jwt: entitlements.token,
                client_version,
                client_version_source,
            }),
            global_throttle: Throttle::new(GLOBAL_BURST, GLOBAL_RPS),
            mmr_throttle: Throttle::new(MMR_BURST, MMR_RPS),
            raw_captures: RawCaptures::new(),
        })
    }

    /// Replaces the global remote bucket's burst and refill rate, so a test
    /// that fires hundreds of requests at a mock server doesn't wait out the
    /// production 10 rps.
    ///
    /// Test-only at compile time (R132): it exists in this crate's own unit
    /// tests, and for other crates only when they enable the `test-support`
    /// feature — `src-tauri` does so from `[dev-dependencies]` alone, which
    /// resolver 2 keeps out of a normal or release build. No production code
    /// can raise the 10 rps bucket that keeps the app under Riot's 429s.
    #[cfg(any(test, feature = "test-support"))]
    pub fn with_global_rate(mut self, rps: f64) -> Self {
        self.global_throttle = Throttle::new(rps, rps);
        self
    }

    /// The local player's PUUID, from the entitlements token fetched at
    /// `connect()` time.
    pub fn self_puuid(&self) -> &str {
        &self.puuid
    }

    /// The last, redacted response body for every endpoint captured so
    /// far (via `call_local`/`call_remote`), sorted by endpoint name — for
    /// Task 5b's `dump_raw()`. See `raw_capture.rs`'s module docs for what
    /// this deliberately does not include.
    pub fn raw_captures(&self) -> Vec<(String, String)> {
        self.raw_captures.snapshot()
    }

    /// R158: the last non-2xx answer seen at each endpoint (redacted, at most
    /// 4 KB each), ready to be written as `<key>.error.<ext>` beside the 2xx
    /// captures, sorted by endpoint key. A later 2xx neither removes an entry
    /// nor replaces its 2xx capture.
    pub fn raw_error_captures(&self) -> Vec<ErrorDump> {
        self.raw_captures.error_snapshot()
    }

    /// R159: the poll loop hands over the user's `valorant` presence `private`
    /// blob (base64) the first time it sees one after the session started —
    /// no request of ours. A session built before the game's presence existed
    /// took its version from a lower source (valorant-api, the log, the
    /// literal); the game's own presence is the top of the chain
    /// ([`fetch_client_version`]), so a usable `partyClientVersion` replaces
    /// them, for the next request on. A presence without one changes nothing,
    /// and once the presence has spoken it is not asked again.
    ///
    /// Returns whether the header value changed (logged once, at info level).
    pub fn note_presence_version(&self, private_b64: &str) -> bool {
        let Some(version) = version_from_private_b64(private_b64) else {
            return false;
        };
        let (was, was_from) = {
            let mut auth = self.auth.lock().unwrap();
            if auth.client_version_source == VersionSource::Presence {
                return false;
            }
            (
                std::mem::replace(&mut auth.client_version, version.clone()),
                std::mem::replace(&mut auth.client_version_source, VersionSource::Presence),
            )
        };
        // Logged after the lock is released: a request in flight never waits
        // on the logger.
        let changed = was != version;
        if changed {
            log::info!(
                "client version changed to {version} (from presence; was {was} from {})",
                was_from.as_str()
            );
        }
        changed
    }

    /// The `X-Riot-ClientVersion` this session sends on pd/glz calls.
    pub fn client_version(&self) -> String {
        self.auth.lock().unwrap().client_version.clone()
    }

    /// Where [`client_version`](Self::client_version) came from.
    pub fn client_version_source(&self) -> VersionSource {
        self.auth.lock().unwrap().client_version_source
    }

    /// `GET`s a path on the local client (`127.0.0.1:{port}`), with Basic
    /// auth. Not throttled — only remote (pd/glz) calls are rate-limited.
    pub async fn local<T: DeserializeOwned>(&self, path: &str) -> Result<T, RiotError> {
        self.check_alive()?;
        let resp = self.call_local(Method::GET, path, None).await?;
        parse_json(&resp)
    }

    /// `GET`s a path on `pd.{shard}.a.pvp.net`, throttled (and, for
    /// `/mmr/` paths, additionally throttled by the mmr bucket). Retries
    /// once on 401/403 (re-authenticating first) and up to
    /// [`MAX_RATE_LIMIT_RETRIES`] times on 429.
    pub async fn pd<T: DeserializeOwned>(&self, path: &str) -> Result<T, RiotError> {
        self.remote(RemoteKind::Pd, Method::GET, path, None).await
    }

    /// `PUT`s a JSON body to a path on `pd.{shard}.a.pvp.net`. `pub(crate)`,
    /// not `pub`: its only caller is `names()` in `endpoints.rs`, and
    /// keeping it crate-private closes off a way to bypass
    /// [`VisiblePuuid`](crate::endpoints::VisiblePuuid) (ruling R16) by
    /// PUTing straight to `/name-service/v2/players` with raw strings.
    pub(crate) async fn pd_put<T: DeserializeOwned>(
        &self,
        path: &str,
        body: &Value,
    ) -> Result<T, RiotError> {
        self.remote(RemoteKind::Pd, Method::PUT, path, Some(body))
            .await
    }

    /// `GET`s a path on `glz-{region}.{shard}.a.pvp.net`, throttled by the
    /// global bucket with the same 401/403/429 handling as [`pd`](Self::pd).
    pub async fn glz<T: DeserializeOwned>(&self, path: &str) -> Result<T, RiotError> {
        self.remote(RemoteKind::Glz, Method::GET, path, None).await
    }

    /// Re-reads the lockfile and compares it against the snapshot taken at
    /// `connect()` time. Cheap (one small file read), called before every
    /// local/pd/glz call so a closed or restarted Riot client is caught
    /// immediately instead of failing on a dead connection.
    fn check_alive(&self) -> Result<(), RiotError> {
        let raw = std::fs::read_to_string(lockfile_path(&self.local_app_data))
            .map_err(|_| RiotError::ClientGone)?;
        let lockfile = parse_lockfile(&raw).map_err(|_| RiotError::ClientGone)?;
        if lockfile.port != self.lockfile_snapshot.port
            || lockfile.password != self.lockfile_snapshot.password
        {
            return Err(RiotError::ClientGone);
        }
        Ok(())
    }

    /// The single choke point for local-client HTTP. Buffers the whole
    /// body eagerly (via [`buffer_response`]) and records it for
    /// [`raw_captures`](Self::raw_captures) before returning.
    async fn call_local(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
    ) -> Result<CapturedResponse, RiotError> {
        let url = format!("{}{}", self.local_base, path);
        let mut req = self
            .local_client
            .request(method, url)
            .header(AUTHORIZATION, &self.basic_auth);
        if let Some(b) = body {
            req = req.json(b);
        }
        let resp = req.send().await.map_err(http_error)?;
        let captured = buffer_response(resp).await?;
        // Fix-round item 9 / R36: only a 2xx body is ever recorded — an
        // error body (a 401 before re-auth, a transient 500, etc.) isn't
        // what Task 11 wants as a fixture, and must not be allowed to
        // overwrite a good, previously-captured success body.
        if captured.status.is_success() {
            self.raw_captures.record(path, &captured.body);
        } else {
            // R158: what Riot said when it refused, kept apart.
            self.raw_captures
                .record_error(path, captured.status.as_u16(), &captured.body);
        }
        Ok(captured)
    }

    /// The single choke point for pd/glz HTTP (before throttling/retry
    /// logic is applied in [`remote`](Self::remote)). Buffers the whole
    /// body eagerly (via [`buffer_response`]) and records it for
    /// [`raw_captures`](Self::raw_captures) before returning.
    async fn call_remote(
        &self,
        kind: &RemoteKind,
        method: Method,
        path: &str,
        body: Option<&Value>,
    ) -> Result<CapturedResponse, RiotError> {
        let base = match kind {
            RemoteKind::Pd => &self.pd_base,
            RemoteKind::Glz => &self.glz_base,
        };
        let url = format!("{base}{path}");
        let (access_token, entitlements_jwt, client_version) = {
            let auth = self.auth.lock().unwrap();
            (
                auth.access_token.clone(),
                auth.entitlements_jwt.clone(),
                auth.client_version.clone(),
            )
        };
        let mut req = self
            .remote_client
            .request(method, url)
            .header(AUTHORIZATION, format!("Bearer {access_token}"))
            .header("X-Riot-Entitlements-JWT", entitlements_jwt)
            .header("X-Riot-ClientPlatform", CLIENT_PLATFORM_B64)
            .header("X-Riot-ClientVersion", client_version)
            .header(USER_AGENT, REMOTE_USER_AGENT);
        if let Some(b) = body {
            req = req.json(b);
        }
        let resp = req.send().await.map_err(http_error)?;
        let captured = buffer_response(resp).await?;
        // See `call_local`'s matching comment: only 2xx bodies are ever
        // recorded (fix-round item 9 / R36) — this also means a 401 (pre
        // re-auth) or 429 response never overwrites a good prior capture.
        if captured.status.is_success() {
            self.raw_captures.record(path, &captured.body);
        } else {
            self.raw_captures
                .record_error(path, captured.status.as_u16(), &captured.body);
        }
        Ok(captured)
    }

    /// Throttled pd/glz call with 401/403 re-auth (once) and 429 retry
    /// (bounded). This is where the mmr-vs-global bucket wiring lives: an
    /// `/mmr/` path takes from the mmr bucket *and then* the global
    /// bucket; every other remote call takes only from the global bucket.
    /// A 429 on an mmr path holds the mmr bucket; any other 429 holds the
    /// global bucket (which, since mmr calls also draw from the global
    /// bucket, means a global hold conservatively slows mmr calls too —
    /// see task-3-report.md).
    async fn remote<T: DeserializeOwned>(
        &self,
        kind: RemoteKind,
        method: Method,
        path: &str,
        body: Option<&Value>,
    ) -> Result<T, RiotError> {
        self.check_alive()?;
        let is_mmr = matches!(kind, RemoteKind::Pd) && path.starts_with("/mmr/");
        let mut backoff = FALLBACK_BACKOFF_SECS;
        let mut reauthed = false;
        let mut rate_limit_attempts = 0u32;

        loop {
            if is_mmr {
                self.mmr_throttle.take().await;
            }
            self.global_throttle.take().await;

            let resp = self.call_remote(&kind, method.clone(), path, body).await?;
            match resp.status {
                StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN if !reauthed => {
                    reauthed = true;
                    self.refresh_entitlements().await?;
                }
                StatusCode::TOO_MANY_REQUESTS => {
                    let retry_after = retry_after_seconds(&resp.headers).unwrap_or(backoff);
                    if is_mmr {
                        self.mmr_throttle.hold_for(retry_after);
                    } else {
                        self.global_throttle.hold_for(retry_after);
                    }
                    backoff += FALLBACK_BACKOFF_SECS;
                    rate_limit_attempts += 1;
                    if rate_limit_attempts > MAX_RATE_LIMIT_RETRIES {
                        return Err(RiotError::RateLimited);
                    }
                }
                _ => return parse_json(&resp),
            }
        }
    }

    /// Re-fetches the entitlements token (used on a 401/403) and updates
    /// the cached headers. `self_puuid()` never changes — same account.
    async fn refresh_entitlements(&self) -> Result<(), RiotError> {
        self.check_alive()?;
        let entitlements =
            fetch_entitlements(&self.local_client, &self.local_base, &self.basic_auth).await?;
        let mut auth = self.auth.lock().unwrap();
        auth.access_token = entitlements.access_token;
        auth.entitlements_jwt = entitlements.token;
        Ok(())
    }
}

/// A fully-buffered HTTP response: status, headers, and the whole body —
/// read eagerly at both `call_local`/`call_remote` so the raw body can be
/// captured (see `raw_capture.rs`) without anything downstream needing to
/// touch a live `reqwest::Response` (whose `.bytes()` consumes it).
struct CapturedResponse {
    status: StatusCode,
    headers: HeaderMap,
    body: Vec<u8>,
}

/// Buffers a live `reqwest::Response` into a [`CapturedResponse`]. Used by
/// `call_local`/`call_remote` (which then also record the body into
/// `raw_captures`) and by [`fetch_entitlements`] (which deliberately does
/// *not* record — see `raw_capture.rs`'s module docs on why the
/// entitlements endpoint is excluded).
async fn buffer_response(resp: Response) -> Result<CapturedResponse, RiotError> {
    let status = resp.status();
    let headers = resp.headers().clone();
    let body = resp.bytes().await.map_err(http_error)?.to_vec();
    Ok(CapturedResponse {
        status,
        headers,
        body,
    })
}

async fn fetch_entitlements(
    local_client: &Client,
    local_base: &str,
    basic_auth: &str,
) -> Result<EntitlementsResponse, RiotError> {
    let resp = local_client
        .get(format!("{local_base}/entitlements/v1/token"))
        .header(AUTHORIZATION, basic_auth)
        .send()
        .await
        .map_err(http_error)?;
    let captured = buffer_response(resp).await?;
    parse_json(&captured).map_err(|e| RiotError::AuthFailed(e.to_string()))
}

/// A reqwest failure as [`RiotError::Http`], without the request URL:
/// reqwest's `Display` appends ` for url (…)`, and a pd path carries a full
/// puuid (match history, mmr, a player's pregame/core-game/party path) —
/// the user shares copy-log, where ids are shortened. The caller knows
/// which endpoint failed.
fn http_error(e: reqwest::Error) -> RiotError {
    // reqwest's own text ("error sending request") does not say whether it
    // timed out, could not connect or failed mid-body: the kind does.
    let kind = if e.is_timeout() {
        "timeout"
    } else if e.is_connect() {
        "connect"
    } else if e.is_body() {
        "body"
    } else if e.is_decode() {
        "decode"
    } else if e.is_builder() {
        "builder"
    } else if e.is_request() {
        "request"
    } else {
        "other"
    };
    RiotError::Http(format!("{kind}: {}", e.without_url()))
}

fn parse_json<T: DeserializeOwned>(resp: &CapturedResponse) -> Result<T, RiotError> {
    if !resp.status.is_success() {
        return Err(RiotError::Status(
            resp.status.as_u16(),
            excerpt(&resp.body, LOG_EXCERPT_CHARS),
        ));
    }
    serde_json::from_slice(&resp.body)
        .map_err(|e| RiotError::Decode(excerpt(e.to_string().as_bytes(), LOG_EXCERPT_CHARS)))
}

/// Parses `Retry-After` as a plain number of seconds (Riot doesn't use the
/// HTTP-date form). Returns `None` for anything non-finite or negative
/// (`"inf"`, `"nan"`, a malformed header) so the caller falls back to its
/// own escalating backoff instead of feeding a bogus value to
/// [`Throttle::hold_for`](crate::throttle::Throttle::hold_for) — which
/// would otherwise be the last line of defense against a value that can't
/// become a valid `Duration`. A huge-but-finite value is clamped to
/// [`MAX_RETRY_AFTER_SECS`] rather than trusted outright.
fn retry_after_seconds(headers: &HeaderMap) -> Option<f64> {
    let raw: f64 = headers
        .get(RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse()
        .ok()?;
    if !raw.is_finite() || raw < 0.0 {
        return None;
    }
    Some(raw.min(MAX_RETRY_AFTER_SECS))
}

/// Client version, in Valorant-Scout's order (`riot_client.py::_client_version`,
/// R159):
/// 1. The user's own VALORANT presence in the local `/chat/v4/presences`:
///    `partyClientVersion`, under `partyPresenceData` or flat.
/// 2. `valorant-api.com/v1/version`'s `data.riotClientVersion`.
/// 3. The `CI server version:` line of `ShooterGame.log` (`raw_log`, already
///    read for the shards).
/// 4. Last resort, Scout's literal `release-09.00`.
///
/// `/product-session/v1/external-sessions` is not a source (R159). Each step
/// is best-effort — a failure (network, missing field, bad JSON) falls through
/// to the next step, and so does an empty or whitespace-only answer (R158): it
/// is no answer, never a blank `X-Riot-ClientVersion`. The winner's name comes
/// back with the value, for the log.
async fn fetch_client_version(
    local_client: &Client,
    remote_client: &Client,
    local_base: &str,
    basic_auth: &str,
    self_puuid: &str,
    valorant_api_base: &str,
    raw_log: &str,
) -> (String, VersionSource) {
    if let Some(v) =
        fetch_version_from_presence(local_client, local_base, basic_auth, self_puuid).await
    {
        return (v, VersionSource::Presence);
    }
    if let Some(v) = fetch_version_from_valorant_api(remote_client, valorant_api_base).await {
        return (v, VersionSource::ValorantApi);
    }
    if let Some(v) = parse_client_version(raw_log).and_then(|v| usable_version(&v)) {
        return (v, VersionSource::Log);
    }
    (FALLBACK_CLIENT_VERSION.to_string(), VersionSource::Fallback)
}

/// `raw` trimmed, or `None` when nothing is left or what is left could not be
/// sent as a header: a version source that answers an empty, whitespace-only
/// or unsendable string has not answered.
fn usable_version(raw: &str) -> Option<String> {
    let v = raw.trim();
    // A value the HTTP layer would refuse as a header (a control character)
    // would fail every remote call of the session at `send()`: it is no answer
    // either, and the chain goes on. `HeaderValue` lets bytes above 127 through
    // (obs-text), but no real version has any: a non-ASCII one is garbage too.
    (!v.is_empty() && v.is_ascii() && HeaderValue::from_str(v).is_ok()).then(|| v.to_string())
}

async fn fetch_version_from_presence(
    client: &Client,
    local_base: &str,
    basic_auth: &str,
    self_puuid: &str,
) -> Option<String> {
    let resp = client
        .get(format!("{local_base}/chat/v4/presences"))
        .header(AUTHORIZATION, basic_auth)
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let value: Value = resp.json().await.ok()?;
    let presences = value.get("presences")?.as_array()?;
    presences.iter().find_map(|presence| {
        if presence.get("puuid")?.as_str()? != self_puuid {
            return None;
        }
        if presence.get("product").and_then(Value::as_str) != Some("valorant") {
            return None;
        }
        version_from_private_b64(presence.get("private")?.as_str()?)
    })
}

/// The `partyClientVersion` inside a presence's base64 `private` blob.
fn version_from_private_b64(private_b64: &str) -> Option<String> {
    let decoded = BASE64.decode(private_b64).ok()?;
    let private_json: Value = serde_json::from_slice(&decoded).ok()?;
    version_from_presence_value(&private_json)
}

/// Reads `partyClientVersion`, under `partyPresenceData` (the nested shape)
/// or at the top level (the flat shape) — Scout looks at the nested one first
/// — same fallback pattern as `opd1_core::presence::decode_presence`.
fn version_from_presence_value(value: &Value) -> Option<String> {
    let nested = || {
        value
            .get("partyPresenceData")?
            .get("partyClientVersion")?
            .as_str()
    };
    let flat = || value.get("partyClientVersion")?.as_str();
    // An empty value in one place does not hide a usable one in the other.
    nested()
        .and_then(usable_version)
        .or_else(|| flat().and_then(usable_version))
}

async fn fetch_version_from_valorant_api(client: &Client, base: &str) -> Option<String> {
    let resp = client.get(format!("{base}/v1/version")).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let value: Value = resp.json().await.ok()?;
    version_from_valorant_api_json(&value)
}

fn version_from_valorant_api_json(value: &Value) -> Option<String> {
    usable_version(value.get("data")?.get("riotClientVersion")?.as_str()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn version_from_presence_value_reads_top_level_field() {
        let value = json!({ "partyClientVersion": "release-flat-1" });
        assert_eq!(
            version_from_presence_value(&value).as_deref(),
            Some("release-flat-1")
        );
    }

    #[test]
    fn version_from_presence_value_falls_back_to_nested_field() {
        let value = json!({ "partyPresenceData": { "partyClientVersion": "release-nested-1" } });
        assert_eq!(
            version_from_presence_value(&value).as_deref(),
            Some("release-nested-1")
        );
    }

    #[test]
    fn version_from_presence_value_none_when_absent() {
        assert_eq!(version_from_presence_value(&json!({})), None);
    }

    #[test]
    fn version_from_valorant_api_json_reads_nested_field() {
        let value = json!({ "data": { "riotClientVersion": "release-api-1" }, "status": 200 });
        assert_eq!(
            version_from_valorant_api_json(&value).as_deref(),
            Some("release-api-1")
        );
    }

    #[test]
    fn version_from_valorant_api_json_none_when_absent() {
        assert_eq!(
            version_from_valorant_api_json(&json!({"status": 200})),
            None
        );
    }

    #[test]
    fn an_empty_or_blank_version_is_no_answer_at_any_source() {
        // R158: never a blank `X-Riot-ClientVersion`; the chain goes on.
        for blank in ["", "   ", "\t\n"] {
            let presence = json!({ "partyClientVersion": blank });
            assert_eq!(version_from_presence_value(&presence), None, "{blank:?}");
            let nested = json!({ "partyPresenceData": { "partyClientVersion": blank } });
            assert_eq!(version_from_presence_value(&nested), None, "{blank:?}");
            let api = json!({ "data": { "riotClientVersion": blank } });
            assert_eq!(version_from_valorant_api_json(&api), None, "{blank:?}");
        }
    }

    #[test]
    fn a_blank_presence_version_does_not_hide_a_usable_one_beside_it() {
        // A blank nested field next to a real flat one, and back.
        let both = json!({
            "partyClientVersion": "release-flat-2",
            "partyPresenceData": { "partyClientVersion": "" },
        });
        assert_eq!(
            version_from_presence_value(&both).as_deref(),
            Some("release-flat-2")
        );
        let both = json!({
            "partyClientVersion": " ",
            "partyPresenceData": { "partyClientVersion": "release-nested-2" },
        });
        assert_eq!(
            version_from_presence_value(&both).as_deref(),
            Some("release-nested-2")
        );
    }

    #[test]
    fn the_nested_presence_version_comes_first_like_scouts() {
        let both = json!({
            "partyClientVersion": "release-flat-3",
            "partyPresenceData": { "partyClientVersion": "release-nested-3" },
        });
        assert_eq!(
            version_from_presence_value(&both).as_deref(),
            Some("release-nested-3")
        );
    }

    #[test]
    fn a_version_the_http_layer_would_refuse_is_no_answer() {
        // A control character makes `send()` fail for every remote call of the
        // session; a non-ASCII one is garbage no real version has: such a value
        // falls through instead.
        for bad in [
            "release-\u{7f}1",
            "release-\u{1}1",
            "release-é-1",
            "rel\nease",
            "版本",
        ] {
            assert_eq!(usable_version(bad), None, "{bad:?}");
            let api = json!({ "data": { "riotClientVersion": bad } });
            assert_eq!(version_from_valorant_api_json(&api), None, "{bad:?}");
            let presence = json!({ "partyClientVersion": bad });
            assert_eq!(version_from_presence_value(&presence), None, "{bad:?}");
        }
        // A real one, and one with inner spaces, are fine.
        assert_eq!(
            usable_version(" release-13.06-shipping-18-5590001 ").as_deref(),
            Some("release-13.06-shipping-18-5590001")
        );
        assert_eq!(usable_version("release 1").as_deref(), Some("release 1"));
    }

    #[test]
    fn a_version_is_trimmed() {
        let api =
            json!({ "data": { "riotClientVersion": "  release-13.06-shipping-18-5590001\n" } });
        assert_eq!(
            version_from_valorant_api_json(&api).as_deref(),
            Some("release-13.06-shipping-18-5590001")
        );
    }

    #[test]
    fn version_source_names_are_the_ones_the_log_uses() {
        assert_eq!(VersionSource::Presence.as_str(), "presence");
        assert_eq!(VersionSource::ValorantApi.as_str(), "valorant-api");
        assert_eq!(VersionSource::Log.as_str(), "log");
        assert_eq!(VersionSource::Fallback.as_str(), "fallback");
    }

    #[test]
    fn a_status_error_prints_the_code_then_riots_answer_and_nothing_else() {
        let e = RiotError::Status(400, r#"{"errorCode":"BAD_CLAIMS"}"#.into());
        assert_eq!(
            e.to_string(),
            r#"unexpected HTTP status 400 from Riot: {"errorCode":"BAD_CLAIMS"}"#
        );
        // An empty body prints as before.
        assert_eq!(
            RiotError::Status(500, String::new()).to_string(),
            "unexpected HTTP status 500 from Riot"
        );
    }

    #[test]
    fn a_decode_error_never_quotes_a_puuid_or_a_token() {
        // serde's message can quote the offending value.
        let resp = CapturedResponse {
            status: StatusCode::OK,
            headers: HeaderMap::new(),
            body: br#"{"Subject": 5}"#.to_vec(),
        };
        #[derive(serde::Deserialize, Debug)]
        #[allow(dead_code)]
        struct Wants {
            #[serde(rename = "Subject")]
            subject: String,
        }
        let e = parse_json::<Wants>(&resp).unwrap_err();
        let RiotError::Decode(msg) = &e else {
            panic!("{e:?}")
        };
        assert!(msg.contains("invalid type"), "{msg}");
        // A message that quotes a puuid: shortened.
        let resp = CapturedResponse {
            status: StatusCode::OK,
            headers: HeaderMap::new(),
            body: br#"{"Subject": "aaaaaaaa-0000-0000-0000-000000000001"}"#.to_vec(),
        };
        #[derive(serde::Deserialize, Debug)]
        #[allow(dead_code)]
        struct WantsNumber {
            #[serde(rename = "Subject")]
            subject: u64,
        }
        let e = parse_json::<WantsNumber>(&resp).unwrap_err();
        let msg = e.to_string();
        assert!(msg.contains("aaaaaaaa"), "{msg}");
        assert!(!msg.contains("aaaaaaaa-0000"), "{msg}");
    }
}
