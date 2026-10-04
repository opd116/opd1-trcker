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
    assert_eq!(
        result.files as usize,
        names.len(),
        "the count includes the log"
    );

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
