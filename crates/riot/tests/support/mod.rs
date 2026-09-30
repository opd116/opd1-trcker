//! Shared test scaffolding for the `opd1-riot` integration tests: a
//! throwaway `local_app_data` directory (lockfile + `ShooterGame.log`) and
//! `wiremock` helpers for the local-client endpoints every `RiotSession`
//! needs at `connect()` time.
//!
//! Each `tests/*.rs` file is compiled as its own crate, and each only uses
//! a subset of these helpers — `dead_code` is allowed here rather than in
//! each caller.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

pub const SELF_PUUID: &str = "aaaaaaaa-0000-0000-0000-000000000001";

/// A directory under the OS temp dir that's removed when dropped — stands
/// in for `%LOCALAPPDATA%` in tests.
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("opd1-riot-test-{pid}-{nanos}-{n}"));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        Self(dir)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Writes `<dir>/Riot Games/Riot Client/Config/lockfile` in the standard
/// `name:pid:port:password:protocol` shape.
pub fn write_lockfile(dir: &Path, protocol: &str, port: u16, password: &str) {
    let cfg_dir = dir.join("Riot Games").join("Riot Client").join("Config");
    std::fs::create_dir_all(&cfg_dir).expect("create lockfile dir");
    std::fs::write(
        cfg_dir.join("lockfile"),
        format!("Riot Client:1234:{port}:{password}:{protocol}"),
    )
    .expect("write lockfile");
}

pub fn delete_lockfile(dir: &Path) {
    let path = dir
        .join("Riot Games")
        .join("Riot Client")
        .join("Config")
        .join("lockfile");
    let _ = std::fs::remove_file(path);
}

/// Writes a `ShooterGame.log` with just enough `pd.`/`glz-` URLs for
/// `opd1_core::shards::parse_shards` to succeed (region "eu").
pub fn write_shooter_log(dir: &Path) {
    let log_dir = dir.join("VALORANT").join("Saved").join("Logs");
    std::fs::create_dir_all(&log_dir).expect("create log dir");
    std::fs::write(
        log_dir.join("ShooterGame.log"),
        "[0001] LogPlatform: Info: https://pd.eu.a.pvp.net/account-xp/v1/players/abc\n\
         [0002] LogPlatform: Info: https://glz-eu-1.eu.a.pvp.net/pregame/v1/players/abc\n",
    )
    .expect("write ShooterGame.log");
}

/// Like [`write_shooter_log`], plus a `CI server version:` line carrying
/// `version` (the raw text after the colon, whitespace included).
pub fn write_shooter_log_with_version(dir: &Path, version: &str) {
    let log_dir = dir.join("VALORANT").join("Saved").join("Logs");
    std::fs::create_dir_all(&log_dir).expect("create log dir");
    std::fs::write(
        log_dir.join("ShooterGame.log"),
        format!(
            "[0000] LogInit: CI server version:{version}\n\
             [0001] LogPlatform: Info: https://pd.eu.a.pvp.net/account-xp/v1/players/abc\n\
             [0002] LogPlatform: Info: https://glz-eu-1.eu.a.pvp.net/pregame/v1/players/abc\n"
        ),
    )
    .expect("write ShooterGame.log");
}

/// Mounts `/entitlements/v1/token` so it can be hit any number of times
/// (both the initial `connect()` and any 401/403 re-auth).
pub async fn mount_entitlements(server: &MockServer, puuid: &str) {
    Mock::given(method("GET"))
        .and(path("/entitlements/v1/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "accessToken": "access-token-1",
            "entitlements": [],
            "issuer": "issuer",
            "subject": puuid,
            "token": "entitlements-jwt-1",
        })))
        .mount(server)
        .await;
}

/// Mounts `/chat/v4/presences` with a single self-presence whose `private`
/// blob decodes to `{"partyClientVersion": version, ...}` (the flat shape).
pub async fn mount_presence_with_version(server: &MockServer, puuid: &str, version: &str) {
    mount_presence_private(
        server,
        puuid,
        json!({
            "isValid": true,
            "sessionLoopState": "MENUS",
            "partyClientVersion": version,
        }),
    )
    .await;
}

/// `private` as the client sends it: the JSON, base64-encoded.
pub fn presence_private_b64(private: &serde_json::Value) -> String {
    use base64::engine::general_purpose::STANDARD as BASE64;
    use base64::Engine as _;
    BASE64.encode(private.to_string())
}

/// Mounts `/chat/v4/presences` with a single VALORANT self-presence whose
/// `private` blob decodes to `private`.
pub async fn mount_presence_private(server: &MockServer, puuid: &str, private: serde_json::Value) {
    let private_b64 = presence_private_b64(&private);

    Mock::given(method("GET"))
        .and(path("/chat/v4/presences"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "presences": [
                {
                    "actor": null,
                    "basic": "",
                    "details": null,
                    "game_name": "Shanky",
                    "game_tag": "OPD1",
                    "location": null,
                    "msg": null,
                    "name": "",
                    "patchline": null,
                    "pid": puuid,
                    "platform": null,
                    "private": private_b64,
                    "privateJwt": null,
                    "product": "valorant",
                    "puuid": puuid,
                    "region": "eu",
                    "resource": "",
                    "state": "chat",
                    "summary": "",
                    "time": 1_758_970_000_000u64,
                }
            ]
        })))
        .mount(server)
        .await;
}

/// Fully connects a `RiotSession` against `server` for all three remote
/// bases (local via the lockfile, pd/glz via the override constructor) —
/// the common case where tests don't care which base a call goes through.
pub async fn connect_against(tmp: &Path, server: &MockServer) -> opd1_riot::session::RiotSession {
    let base = server.uri();
    opd1_riot::session::RiotSession::connect_with_overrides(
        tmp,
        opd1_riot::session::ConnectOverrides {
            pd_base: Some(base.clone()),
            glz_base: Some(base.clone()),
            valorant_api_base: Some(base),
            ..Default::default()
        },
    )
    .await
    .expect("connect should succeed")
}

/// Mounts `valorant-api.com`'s `/v1/version` (the mock server doubles as
/// the valorant-api base) with `data.riotClientVersion` = `version`.
pub async fn mount_valorant_api_version(server: &MockServer, version: &str) {
    Mock::given(method("GET"))
        .and(path("/v1/version"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status": 200,
            "data": {
                "manifestId": "ABC",
                "branch": "release-13.06",
                "version": "13.06.00.5590001",
                "buildVersion": "18",
                "engineVersion": "5.3.2",
                "riotClientVersion": version,
                "riotClientBuild": "123.4.5.6",
                "buildDate": "2026-09-01T00:00:00Z",
            }
        })))
        .mount(server)
        .await;
}
