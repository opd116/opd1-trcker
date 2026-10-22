//! `app_info` (R189): the About screen.
//!
//! - `version`: `CARGO_PKG_VERSION`.
//! - `commit` / `build_ms`: set by `build.rs` (`OPD1_GIT_COMMIT`,
//!   `OPD1_BUILD_MS`); `None` when the build had no git.
//! - `riot_client_version`: the `X-Riot-ClientVersion` the session sends, the
//!   copy-log header's source (`RiotSession::client_version_and_source`) —
//!   not the literal last-resort fallback, which is no knowledge; else what
//!   the status carries.
//! - `valorant_version`: the status's game version (e.g. `release-11.08`),
//!   else the release label at the head of the client version.
//! - `db_matches` / `db_bytes`: the stored matches, and the size of the
//!   SQLite file the connection opened (its WAL side file not included).

use opd1_core::wire::AppInfo;
use opd1_riot::session::VersionSource;

use super::AppStateRef;
use crate::state::AppState;

/// `release-13.06` out of `release-13.06-shipping-18-5590001`: the part
/// before `-shipping`, else the first two `-` parts of a `release-…` string.
fn release_label(client_version: &str) -> Option<String> {
    let v = client_version.trim();
    if let Some((head, _)) = v.split_once("-shipping") {
        return (!head.is_empty()).then(|| head.to_string());
    }
    let mut parts = v.splitn(3, '-');
    match (parts.next(), parts.next()) {
        (Some("release"), Some(n)) if !n.is_empty() => Some(format!("release-{n}")),
        _ => None,
    }
}

/// The two versions: `(riot client, valorant)`.
fn versions(st: &AppState) -> (Option<String>, Option<String>) {
    let status = st.status();
    let client = st
        .session()
        .map(|s| s.client_version_and_source())
        .filter(|(v, source)| *source != VersionSource::Fallback && !v.is_empty())
        .map(|(v, _)| v)
        .or(status.riot_client_version);
    let game = status
        .game_version
        .or_else(|| client.as_deref().and_then(release_label));
    (client, game)
}

pub async fn app_info_for(st: &AppState) -> Result<AppInfo, String> {
    let (riot_client_version, valorant_version) = versions(st);
    let (db_matches, db_path) = st
        .db(|repo| {
            Ok((
                repo.stored_match_count()?,
                repo.conn().path().map(str::to_string),
            ))
        })
        .await
        .ok_or_else(|| "database unavailable".to_string())?;
    let db_bytes = db_path
        .filter(|p| !p.is_empty())
        .and_then(|p| std::fs::metadata(p).ok())
        .map_or(0, |m| m.len());
    Ok(AppInfo {
        version: env!("CARGO_PKG_VERSION").into(),
        commit: option_env!("OPD1_GIT_COMMIT").map(str::to_string),
        build_ms: option_env!("OPD1_BUILD_MS").and_then(|ms| ms.parse().ok()),
        riot_client_version,
        valorant_version,
        db_matches,
        db_bytes,
    })
}

#[tauri::command]
pub async fn app_info(state: AppStateRef<'_>) -> Result<AppInfo, String> {
    app_info_for(&state).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_release_label_is_the_head_of_the_client_version() {
        assert_eq!(
            release_label("release-13.06-shipping-18-5590001").as_deref(),
            Some("release-13.06")
        );
        assert_eq!(
            release_label("release-09.00").as_deref(),
            Some("release-09.00")
        );
        assert_eq!(release_label("nonsense"), None);
        assert_eq!(release_label(""), None);
    }

    #[test]
    fn the_build_facts_are_set_by_build_rs() {
        // `build.rs` always sets the build time; the hash only with git.
        assert!(option_env!("OPD1_BUILD_MS").is_some_and(|ms| ms.parse::<i64>().is_ok()));
    }

    struct Quiet;

    impl crate::state::EventSink for Quiet {
        fn live(&self, _: &opd1_core::model::LiveState) {}
        fn details(&self, _: &opd1_core::model::PlayerDetails) {}
        fn match_view(&self, _: &opd1_core::model::MatchDetailsView) {}
    }

    #[tokio::test]
    async fn app_info_counts_the_stored_matches_and_sizes_the_db_file() {
        let dir = tempfile::tempdir().unwrap();
        let st =
            crate::build_state(Box::new(Quiet), dir.path(), &dir.path().join("cache")).unwrap();
        st.db_blocking(|repo| {
            repo.conn().execute_batch(
                "INSERT INTO matches (match_id, map_id, queue_id, start_ms, winning_team, is_ranked)
                 VALUES ('a', '/Game/Maps/Ascent/Ascent', 'competitive', 1, NULL, 1),
                        ('b', '/Game/Maps/Ascent/Ascent', 'unrated', 2, NULL, 0);",
            )
        })
        .unwrap();
        let info = app_info_for(&st).await.unwrap();
        assert_eq!(info.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(info.db_matches, 2);
        let on_disk = std::fs::metadata(dir.path().join("data").join("opd1.sqlite"))
            .unwrap()
            .len();
        assert!(info.db_bytes > 0);
        assert_eq!(info.db_bytes, on_disk);
        // No session, no status yet: the versions are unknown, not invented.
        assert_eq!(info.riot_client_version, None);
        assert_eq!(info.valorant_version, None);
    }
}
