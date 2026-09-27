//! The Riot local-client session: lockfile -> local auth -> entitlements
//! -> remote (pd/glz) headers, with auto re-auth on 401/403 and a
//! `ClientGone` signal the moment the lockfile disappears or changes.
//!
//! All HTTP goes through three private choke points — [`RiotSession::call_local`]
//! and [`RiotSession::call_remote`] — so a later task can add raw-response
//! capture in one place.

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use reqwest::header::{AUTHORIZATION, RETRY_AFTER};
use reqwest::{Client, Method, Response, StatusCode};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::Value;

use opd1_core::lockfile::parse_lockfile;
use opd1_core::shards::parse_shards;

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

/// Used when every client-version source fails. Requests will likely still
/// work — Riot's remote endpoints don't appear to hard-reject on a stale
/// `X-Riot-ClientVersion` — but this is unverified without a real client
/// (see task-3-report.md concerns).
const FALLBACK_CLIENT_VERSION: &str = "unknown";

/// The standard `X-Riot-ClientPlatform` value: base64 of
/// `{"platformType":"PC","platformOS":"Windows","platformOSVersion":"10.0.19042.1.256.64bit","platformChipset":"Unknown"}`.
/// Verbatim from Valorant-Scout's `riot_client.py` and techchrism's
/// `common-components.md` (identical in both sources).
const CLIENT_PLATFORM_B64: &str = "ew0KCSJwbGF0Zm9ybVR5cGUiOiAiUEMiLA0KCSJwbGF0Zm9ybU9TIjogIldpbmRvd3MiLA0KCSJwbGF0Zm9ybU9TVmVyc2lvbiI6ICIxMC4wLjE5MDQyLjEuMjU2LjY0Yml0IiwNCgkicGxhdGZvcm1DaGlwc2V0IjogIlVua25vd24iDQp9";

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
    /// re-auth, and 429, which triggers a throttled retry).
    Status(u16),
    /// The response body wasn't the JSON shape the caller expected.
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
            RiotError::Status(code) => write!(f, "unexpected HTTP status {code} from Riot"),
            RiotError::Decode(msg) => write!(f, "couldn't parse Riot's response: {msg}"),
        }
    }
}

impl std::error::Error for RiotError {}

/// The port/password pair a [`RiotSession`] was built from — re-checked
/// (cheaply, from disk) before every call so a Riot client restart is
/// caught immediately instead of hanging on a dead port.
#[derive(Debug)]
struct LockfileSnapshot {
    port: u16,
    password: String,
}

#[derive(Debug)]
struct AuthState {
    access_token: String,
    entitlements_jwt: String,
    client_version: String,
}

#[derive(Debug)]
enum RemoteKind {
    Pd,
    Glz,
}

/// A live connection to the local Riot client, plus everything needed to
/// call `pd.*.a.pvp.net` and `glz-*.*.a.pvp.net` on its behalf.
#[derive(Debug)]
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

#[derive(Debug, Deserialize)]
struct EntitlementsResponse {
    #[serde(rename = "accessToken")]
    access_token: String,
    subject: String,
    token: String,
}

impl RiotSession {
    /// Connects to the local Riot client: reads the lockfile and
    /// `ShooterGame.log` under `local_app_data`, fetches an entitlements
    /// token (which also gives us `self_puuid`), and determines the
    /// client version to send on remote calls.
    pub async fn connect(local_app_data: &Path) -> Result<Self, RiotError> {
        Self::connect_with_overrides(local_app_data, None, None, None).await
    }

    /// Like [`connect`](Self::connect), but lets tests point the pd/glz/
    /// valorant-api bases at a mock server instead of the real Riot
    /// shards and `valorant-api.com`. Not meant for production use — the
    /// public entry point is `connect`.
    #[doc(hidden)]
    pub async fn connect_with_overrides(
        local_app_data: &Path,
        pd_base_override: Option<String>,
        glz_base_override: Option<String>,
        valorant_api_base_override: Option<String>,
    ) -> Result<Self, RiotError> {
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
        // validation.
        let local_client = Client::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .map_err(|e| RiotError::Http(e.to_string()))?;
        let remote_client = Client::builder()
            .build()
            .map_err(|e| RiotError::Http(e.to_string()))?;

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

        let client_version = fetch_client_version(
            &local_client,
            &remote_client,
            &local_base,
            &basic_auth,
            &entitlements.subject,
            &valorant_api_base,
        )
        .await;

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
            }),
            global_throttle: Throttle::new(GLOBAL_BURST, GLOBAL_RPS),
            mmr_throttle: Throttle::new(MMR_BURST, MMR_RPS),
        })
    }

    /// The local player's PUUID, from the entitlements token fetched at
    /// `connect()` time.
    pub fn self_puuid(&self) -> &str {
        &self.puuid
    }

    /// `GET`s a path on the local client (`127.0.0.1:{port}`), with Basic
    /// auth. Not throttled — only remote (pd/glz) calls are rate-limited.
    pub async fn local<T: DeserializeOwned>(&self, path: &str) -> Result<T, RiotError> {
        self.check_alive()?;
        let resp = self.call_local(Method::GET, path, None).await?;
        parse_json(resp).await
    }

    /// `GET`s a path on `pd.{shard}.a.pvp.net`, throttled (and, for
    /// `/mmr/` paths, additionally throttled by the mmr bucket). Retries
    /// once on 401/403 (re-authenticating first) and up to
    /// [`MAX_RATE_LIMIT_RETRIES`] times on 429.
    pub async fn pd<T: DeserializeOwned>(&self, path: &str) -> Result<T, RiotError> {
        self.remote(RemoteKind::Pd, Method::GET, path, None).await
    }

    /// `PUT`s a JSON body to a path on `pd.{shard}.a.pvp.net` (used by
    /// `names()`), with the same throttling/retry behavior as [`pd`](Self::pd).
    pub async fn pd_put<T: DeserializeOwned>(
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

    /// The single choke point for local-client HTTP. `raw-response
    /// capture` (a later task) hooks in here.
    async fn call_local(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
    ) -> Result<Response, RiotError> {
        let url = format!("{}{}", self.local_base, path);
        let mut req = self
            .local_client
            .request(method, url)
            .header(AUTHORIZATION, &self.basic_auth);
        if let Some(b) = body {
            req = req.json(b);
        }
        req.send().await.map_err(|e| RiotError::Http(e.to_string()))
    }

    /// The single choke point for pd/glz HTTP (before throttling/retry
    /// logic is applied in [`remote`](Self::remote)). A later task's
    /// raw-response capture hooks in here.
    async fn call_remote(
        &self,
        kind: &RemoteKind,
        method: Method,
        path: &str,
        body: Option<&Value>,
    ) -> Result<Response, RiotError> {
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
            .header("X-Riot-ClientVersion", client_version);
        if let Some(b) = body {
            req = req.json(b);
        }
        req.send().await.map_err(|e| RiotError::Http(e.to_string()))
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
            match resp.status() {
                StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN if !reauthed => {
                    reauthed = true;
                    self.refresh_entitlements().await?;
                }
                StatusCode::TOO_MANY_REQUESTS => {
                    let retry_after = retry_after_seconds(&resp).unwrap_or(backoff);
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
                _ => return parse_json(resp).await,
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
        .map_err(|e| RiotError::Http(e.to_string()))?;
    parse_json(resp)
        .await
        .map_err(|e| RiotError::AuthFailed(e.to_string()))
}

async fn parse_json<T: DeserializeOwned>(resp: Response) -> Result<T, RiotError> {
    let status = resp.status();
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| RiotError::Http(e.to_string()))?;
    if !status.is_success() {
        return Err(RiotError::Status(status.as_u16()));
    }
    serde_json::from_slice(&bytes).map_err(|e| RiotError::Decode(e.to_string()))
}

fn retry_after_seconds(resp: &Response) -> Option<f64> {
    resp.headers()
        .get(RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse::<f64>()
        .ok()
}

/// Client version, in priority order (ruling R17):
/// 1. Local `/product-session/v1/external-sessions` (the running
///    VALORANT process's own `version`).
/// 2. The local player's own `/chat/v4/presences` entry (Scout's
///    `partyClientVersion` fallback).
/// 3. `valorant-api.com/v1/version`'s `data.riotClientVersion`.
///
/// Each step is best-effort — any failure (network, missing field, bad
/// JSON) just falls through to the next step, matching Scout's try/except
/// chain in `_client_version`.
async fn fetch_client_version(
    local_client: &Client,
    remote_client: &Client,
    local_base: &str,
    basic_auth: &str,
    self_puuid: &str,
    valorant_api_base: &str,
) -> String {
    if let Some(v) = fetch_version_from_sessions(local_client, local_base, basic_auth).await {
        return v;
    }
    if let Some(v) =
        fetch_version_from_presence(local_client, local_base, basic_auth, self_puuid).await
    {
        return v;
    }
    if let Some(v) = fetch_version_from_valorant_api(remote_client, valorant_api_base).await {
        return v;
    }
    FALLBACK_CLIENT_VERSION.to_string()
}

async fn fetch_version_from_sessions(
    client: &Client,
    local_base: &str,
    basic_auth: &str,
) -> Option<String> {
    let resp = client
        .get(format!("{local_base}/product-session/v1/external-sessions"))
        .header(AUTHORIZATION, basic_auth)
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let value: Value = resp.json().await.ok()?;
    version_from_sessions_json(&value)
}

/// Pulls the `version` field from whichever session entry has
/// `productId == "valorant"` (the running game process, as opposed to the
/// Riot Client itself).
fn version_from_sessions_json(value: &Value) -> Option<String> {
    value.as_object()?.values().find_map(|session| {
        let obj = session.as_object()?;
        if obj.get("productId")?.as_str()? == "valorant" {
            obj.get("version")?.as_str().map(str::to_string)
        } else {
            None
        }
    })
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
        let private_b64 = presence.get("private")?.as_str()?;
        let decoded = BASE64.decode(private_b64).ok()?;
        let private_json: Value = serde_json::from_slice(&decoded).ok()?;
        version_from_presence_value(&private_json)
    })
}

/// Reads `partyClientVersion`, at the top level (the flat shape) or under
/// `partyPresenceData` (the nested shape) — same fallback pattern as
/// `opd1_core::presence::decode_presence`.
fn version_from_presence_value(value: &Value) -> Option<String> {
    value
        .get("partyClientVersion")
        .and_then(Value::as_str)
        .or_else(|| {
            value
                .get("partyPresenceData")?
                .get("partyClientVersion")?
                .as_str()
        })
        .map(str::to_string)
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
    value
        .get("data")?
        .get("riotClientVersion")?
        .as_str()
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn version_from_sessions_json_finds_the_valorant_entry() {
        let value = json!({
            "session-1": { "productId": "riot_client", "version": "not-this-one" },
            "session-2": { "productId": "valorant", "version": "release-09.08" },
        });
        assert_eq!(
            version_from_sessions_json(&value).as_deref(),
            Some("release-09.08")
        );
    }

    #[test]
    fn version_from_sessions_json_none_when_no_valorant_entry() {
        let value = json!({
            "session-1": { "productId": "riot_client", "version": "not-this-one" },
        });
        assert_eq!(version_from_sessions_json(&value), None);
    }

    #[test]
    fn version_from_sessions_json_none_for_empty_object() {
        assert_eq!(version_from_sessions_json(&json!({})), None);
    }

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
}
