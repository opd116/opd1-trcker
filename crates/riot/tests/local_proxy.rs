//! The local client never goes through a proxy: the calls to 127.0.0.1
//! carry the lockfile's Basic auth. Its own test binary, because it sets
//! process-wide proxy variables.

mod support;

use support::{connect_against, mount_entitlements, write_lockfile, write_shooter_log, TempDir};
use wiremock::MockServer;

#[tokio::test]
async fn a_proxy_in_the_environment_never_sees_the_local_calls() {
    let tmp = TempDir::new();
    write_shooter_log(tmp.path());
    let server = MockServer::start().await;
    write_lockfile(tmp.path(), "http", server.address().port(), "pw");
    mount_entitlements(&server, support::SELF_PUUID).await;
    // A proxy that is not there: a call sent through it would fail.
    for var in [
        "ALL_PROXY",
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "all_proxy",
        "http_proxy",
    ] {
        // SAFETY: the only test in this binary; nothing reads the
        // environment concurrently.
        unsafe { std::env::set_var(var, "http://127.0.0.1:9") };
    }
    let session = connect_against(tmp.path(), &server).await;
    assert_eq!(session.self_puuid(), support::SELF_PUUID);
}
