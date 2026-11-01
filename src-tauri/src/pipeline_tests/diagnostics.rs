//! R158's pipeline tests: a field report that explains itself. A failed remote
//! call's warn line carries the HTTP status, Riot's `errorCode` and only the
//! short puuid; `dump_raw` writes the last error of each endpoint and the log.
//! A child of `pipeline_tests`, so it shares that module's harness (mock
//! Riot, in-memory DB) and helpers through `super::*`.

use super::*;
use crate::commands::{dump_raw_for, dump_raw_with, log_text};

const JWT: &str = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dQw4w9WgXcQ";

/// `puuid`'s `mmr` answers a 500 with Riot's JSON error, `code` as its
/// `errorCode`, a token-looking value and the puuid in the message.
async fn mmr_fails_with(h: &Harness, puuid: &str, code: &str) {
    Mock::given(method("GET"))
        .and(path(format!("/mmr/v1/players/{puuid}").as_str()))
        .respond_with(ResponseTemplate::new(500).set_body_json(json!({
            "httpStatus": 500,
            "errorCode": code,
            "message": format!("could not read {puuid}"),
            "accessToken": "opaque-secret-access-token",
            "detail": JWT,
        })))
        .with_priority(1)
        .mount(&h.server)
        .await;
}

/// The log lines that mention `marker`.
fn log_lines(marker: &str) -> Vec<String> {
    crate::logbuf::snapshot()
        .lines()
        .filter(|l| l.contains(marker))
        .map(str::to_string)
        .collect()
}

#[tokio::test]
async fn a_failed_mmr_call_logs_its_status_and_errorcode_and_only_the_short_puuid() {
    crate::logbuf::init();
    let h = harness().await;
    mmr_fails_with(&h, ME, "ERR_CODE_LOG_TEST").await;
    sync_player(&h.st, &h.session, ME).await.unwrap();

    let lines = log_lines("ERR_CODE_LOG_TEST");
    assert_eq!(lines.len(), 1, "{lines:?}");
    let line = &lines[0];
    assert!(line.contains("WARN"), "{line}");
    assert!(
        line.contains("mmr for aaaaaaaa failed"),
        "the short puuid: {line}"
    );
    assert!(line.contains("500"), "the status: {line}");
    assert!(line.contains("ERR_CODE_LOG_TEST"), "the errorCode: {line}");
    for leaked in [ME, "opaque-secret-access-token", JWT, "http://", "/mmr/v1"] {
        assert!(!line.contains(leaked), "{leaked} leaked: {line}");
    }
}

#[tokio::test]
async fn dump_raw_writes_the_last_error_of_each_endpoint_and_the_log() {
    crate::logbuf::init();
    let h = harness().await;
    mmr_fails_with(&h, ME, "ERR_CODE_DUMP_TEST").await;
    h.st.set_session(Some(h.session.clone()));
    sync_player(&h.st, &h.session, ME).await.unwrap();

    let result = dump_raw_with(&h.st, "the log, as copy_log returns it".into())
        .await
        .unwrap();
    let dir = std::path::PathBuf::from(&result.dir);
    let read = |name: &str| std::fs::read_to_string(dir.join(name)).unwrap_or_default();

    // The 2xx captures are still there, next to the error and the log.
    assert!(dir.join("comp_updates.json").exists());
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    assert!(names.contains(&"mmr.error.json".to_string()), "{names:?}");
    assert!(names.contains(&"log.txt".to_string()), "{names:?}");
    assert!(
        !names.contains(&"mmr.json".to_string()),
        "mmr never answered 2xx: {names:?}"
    );
    assert!(
        names.contains(&"manifest.json".to_string()),
        "the dump wrote its manifest: {names:?}"
    );
    assert_eq!(
        result.files as usize,
        names.len() - 1,
        "the count includes the log but not the manifest"
    );
    let manifest: Value = serde_json::from_str(&read("manifest.json")).unwrap();
    let listed: Vec<&str> = manifest["files"]
        .as_array()
        .expect("the manifest lists its files")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(listed.contains(&"comp_updates.json"), "{manifest}");
    assert!(listed.contains(&"log.txt"), "{manifest}");
    assert_eq!(manifest["version"], env!("CARGO_PKG_VERSION"));

    let error: Value = serde_json::from_str(&read("mmr.error.json")).unwrap();
    assert_eq!(error["status"], 500);
    assert_eq!(error["body"]["errorCode"], "ERR_CODE_DUMP_TEST");
    let text = read("mmr.error.json");
    assert!(
        !text.contains("opaque-secret-access-token") && !text.contains(JWT),
        "{text}"
    );

    assert_eq!(read("log.txt"), "the log, as copy_log returns it");
}

#[tokio::test]
async fn dump_raw_writes_the_log_even_without_a_session() {
    crate::logbuf::init();
    let h = harness().await;
    assert!(h.st.session().is_none());
    let result = dump_raw_with(&h.st, "no session yet".into()).await.unwrap();
    let dir = std::path::PathBuf::from(&result.dir);
    assert_eq!(result.files, 1, "only the log");
    assert_eq!(
        std::fs::read_to_string(dir.join("log.txt")).unwrap(),
        "no session yet"
    );

    // The command itself writes the real log (whatever it holds by now).
    let result = dump_raw_for(&h.st).await.unwrap();
    assert_eq!(result.files, 1);
    assert!(dir.join("log.txt").exists());
}

/// B2: `log.txt` and `copy_log` start with the same freshly built header: the
/// shard the session's `ShooterGame.log` named, the client version and where
/// it came from, the act, and the stage.
#[tokio::test]
async fn the_dump_and_copy_log_start_with_the_same_header_line() {
    crate::logbuf::init();
    let h = harness().await;
    h.st.set_session(Some(h.session.clone()));

    let result = dump_raw_for(&h.st).await.unwrap();
    let dir = std::path::PathBuf::from(&result.dir);
    let written = std::fs::read_to_string(dir.join("log.txt")).unwrap();
    let header = written.lines().next().unwrap();

    let version = h.session.client_version();
    let source = h.session.client_version_source().as_str();
    let want_tail = format!(
        " · stage closed · settled false · shard eu/eu-1/eu · client {version} ({source}) \
         · act act-curr V26 A5 (valorant-api)"
    );
    assert!(
        header.starts_with(&format!(
            "OPD1 TRCKER {} · Windows ",
            env!("CARGO_PKG_VERSION")
        )),
        "{header}"
    );
    assert!(header.ends_with(&want_tail), "{header}");
    println!("sample header: {header}");

    // The same builder feeds `copy_log`; the header is rebuilt on each call
    // (here: with no session, the session parts are dashes).
    let first_again = log_text(&h.st).lines().next().unwrap().to_string();
    assert_eq!(first_again, header);
    h.st.set_session(None);
    let without = log_text(&h.st);
    assert!(
        without
            .lines()
            .next()
            .unwrap()
            .ends_with(" · shard — · client — · act act-curr V26 A5 (valorant-api)"),
        "{without}"
    );
    // After the header come the log's own lines, and no secret anywhere.
    assert!(!written.contains(JWT) && !written.contains("opaque-secret"));
}

/// R201/F04: a dump is an exact snapshot of this session, not a folder that
/// accumulates successful endpoint files from earlier sessions.
#[tokio::test]
async fn a_second_dump_without_the_old_session_removes_its_successes_and_errors() {
    crate::logbuf::init();
    let h = harness().await;
    mmr_fails_with(&h, ME, "ERR_CODE_STALE_DUMP").await;
    h.st.set_session(Some(h.session.clone()));
    sync_player(&h.st, &h.session, ME).await.unwrap();

    let first = dump_raw_for(&h.st).await.unwrap();
    let dir = std::path::PathBuf::from(&first.dir);
    assert!(dir.join("comp_updates.json").exists(), "A's success");
    assert!(dir.join("mmr.error.json").exists(), "A's error");

    // The client went away: the next dump has no captures at all.
    h.st.set_session(None);
    let second = dump_raw_with(&h.st, "B, no session".into()).await.unwrap();
    assert_eq!(second.files, 1, "only the log is a payload");
    assert!(
        !dir.join("comp_updates.json").exists(),
        "an earlier session's success survived"
    );
    assert!(
        !dir.join("mmr.error.json").exists(),
        "an earlier session's error survived"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("log.txt")).unwrap(),
        "B, no session"
    );
    let manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["files"], json!(["log.txt"]));
    assert_eq!(manifest["version"], env!("CARGO_PKG_VERSION"));
    assert!(h.st.dump_is_free(), "the dump gate leaked");
}

/// R201: unrelated files and subdirectories survive a dump, while a legacy
/// known-endpoint file from before manifests is still cleaned.
#[tokio::test]
async fn a_dump_keeps_unrelated_files_and_subdirectories() {
    let h = harness().await;
    let first = dump_raw_with(&h.st, "first".into()).await.unwrap();
    let dir = std::path::PathBuf::from(&first.dir);
    std::fs::write(dir.join("keep.txt"), "mine").unwrap();
    std::fs::create_dir(dir.join("sub")).unwrap();
    std::fs::write(dir.join("sub").join("keep.json"), "mine").unwrap();
    // A legacy success from before the manifest existed: known endpoint.
    std::fs::write(dir.join("names.json"), "old").unwrap();

    let second = dump_raw_with(&h.st, "second".into()).await.unwrap();
    assert_eq!(second.files, 1);
    assert!(
        !dir.join("names.json").exists(),
        "the legacy success is stale"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("keep.txt")).unwrap(),
        "mine"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("sub").join("keep.json")).unwrap(),
        "mine"
    );
    let names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert!(
        !names.iter().any(|n| n.contains(".tmp-")),
        "a temp file leaked: {names:?}"
    );
}

/// R201: two dumps do not interleave; the final manifest lists exactly the
/// payload files left in the folder.
#[tokio::test]
async fn two_concurrent_dumps_leave_one_complete_inventory() {
    crate::logbuf::init();
    let h = harness().await;
    mmr_fails_with(&h, ME, "ERR_CODE_CONCURRENT_DUMP").await;
    h.st.set_session(Some(h.session.clone()));
    sync_player(&h.st, &h.session, ME).await.unwrap();

    let (a, b) = tokio::join!(
        dump_raw_with(&h.st, "A".into()),
        dump_raw_with(&h.st, "B".into())
    );
    let a = a.unwrap();
    let b = b.unwrap();
    assert!(a.files > 1 && a.files == b.files);
    let dir = std::path::PathBuf::from(&a.dir);
    let manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("manifest.json")).unwrap()).unwrap();
    let listed: Vec<&str> = manifest["files"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    for name in &listed {
        assert!(
            dir.join(name).is_file(),
            "the manifest lists {name} but it is missing"
        );
    }
    let payloads = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n != "manifest.json")
        .count();
    assert_eq!(payloads, listed.len(), "the folder and manifest disagree");
    assert!(
        !std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .any(|e| e.file_name().to_string_lossy().contains(".tmp-")),
        "a temp file leaked"
    );
    assert!(h.st.dump_is_free(), "the dump gate leaked");
}
