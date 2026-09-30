//! Parses the region/shard the local client is connected to out of
//! `%LOCALAPPDATA%\VALORANT\Saved\Logs\ShooterGame.log`.
//!
//! Follows Valorant-Scout's `backend/riot_client.py::LocalAuth._get_region`:
//! the `pd` shard is read out of a `pd.{shard}.a.pvp.net/account-xp/v1/` URL,
//! and the glz region/shard pair out of a
//! `glz-{region}.{shard}.a.pvp.net` URL (techchrism's valorant-api-docs
//! `common-components.md` documents these as `base_endpoint` and
//! `base_endpoint_glz`). Unlike Scout (which stops at the first match), we
//! keep scanning and let the **last** occurrence in the log win, since the
//! client can reconnect to a different region within one log file.

const PD_ANCHOR: &str = ".a.pvp.net/account-xp/v1/";
const GLZ_ANCHOR: &str = "https://glz-";

/// The region/shard pair needed to build `pd.` and `glz-` base URLs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shards {
    /// The `pd` shard, e.g. `"eu"`.
    pub pd: String,
    /// The first segment after `glz-`, e.g. `"eu-1"`.
    pub glz_region: String,
    /// The shard segment of the glz host, e.g. `"eu"`.
    pub glz_shard: String,
}

/// Scans `log` (the contents of `ShooterGame.log`) for the `pd.` and `glz-`
/// URLs, taking the last occurrence of each. Returns `None` if either is
/// missing.
pub fn parse_shards(log: &str) -> Option<Shards> {
    let mut pd: Option<String> = None;
    let mut glz_region: Option<String> = None;
    let mut glz_shard: Option<String> = None;

    for line in log.lines() {
        if let Some(anchor_idx) = line.find(PD_ANCHOR) {
            let before = &line[..anchor_idx];
            if let Some(dot_idx) = before.rfind('.') {
                pd = Some(before[dot_idx + 1..].to_string());
            }
        }

        if let Some(anchor_idx) = line.find(GLZ_ANCHOR) {
            let after = &line[anchor_idx + GLZ_ANCHOR.len()..];
            let mut segments = after.splitn(3, '.');
            if let (Some(region), Some(shard)) = (segments.next(), segments.next()) {
                glz_region = Some(region.to_string());
                glz_shard = Some(shard.to_string());
            }
        }
    }

    match (pd, glz_region, glz_shard) {
        (Some(pd), Some(glz_region), Some(glz_shard)) => Some(Shards {
            pd,
            glz_region,
            glz_shard,
        }),
        _ => None,
    }
}

const CI_VERSION_ANCHOR: &str = "CI server version:";

/// The game's own client version from the same log (R159): the rest of the
/// first `CI server version:` line, trimmed — Valorant-Scout's third source
/// for `X-Riot-ClientVersion` (`riot_client.py::_client_version`). A line
/// with nothing after the colon says nothing and is skipped; `None` when no
/// line carries a version.
pub fn parse_client_version(log: &str) -> Option<String> {
    log.lines().find_map(|line| {
        let after = &line[line.find(CI_VERSION_ANCHOR)? + CI_VERSION_ANCHOR.len()..];
        let version = after.trim();
        (!version.is_empty()).then(|| version.to_string())
    })
}
