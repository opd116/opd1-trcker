//! `app_status`, `retry_now`, `launch_valorant` (R189).

use std::path::{Path, PathBuf};

use opd1_core::wire::AppStatus;

use super::AppStateRef;

/// The current connection status; later changes arrive as `"status"`.
#[tauri::command]
pub fn app_status(state: AppStateRef<'_>) -> AppStatus {
    state.status()
}

/// "Retry now": the poll loop tries the Riot Client again at once (a connect
/// while disconnected, whatever the backoff; a presence read while
/// connected). It only cuts the loop's current wait short.
#[tauri::command]
pub fn retry_now(state: AppStateRef<'_>) {
    state.request_retry();
}

/// "Launch VALORANT": starts the game through the Riot Client
/// (`RiotClientServices.exe --launch-product=valorant
/// --launch-patchline=live`), the same command line the Riot Client's own
/// shortcut uses. This starts a local program; it sends nothing to the Riot
/// client API, so it is no write to the Riot client (CLAUDE.md "Hard
/// limits"). Errors are user-readable.
#[tauri::command]
pub fn launch_valorant() -> Result<(), String> {
    launch()
}

/// Where Riot keeps the list of its installs: under `%ProgramData%`, which
/// is on another drive when Windows is (`C:\ProgramData` only when unset).
#[cfg_attr(not(windows), allow(dead_code))] // Windows only; the tests use it everywhere.
fn riot_installs(program_data: Option<std::ffi::OsString>) -> PathBuf {
    program_data
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"))
        .join("Riot Games")
        .join("RiotClientInstalls.json")
}

#[cfg(windows)]
fn launch() -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    /// No console window flashes up for the child.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let exe = riot_client_exe(&riot_installs(std::env::var_os("ProgramData")))?;
    std::process::Command::new(&exe)
        .args(LAUNCH_ARGS)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map(|_| ())
        .map_err(|e| {
            log::warn!("launch_valorant: spawn failed: {e}");
            "Couldn't start the Riot Client.".to_string()
        })
}

#[cfg(not(windows))]
fn launch() -> Result<(), String> {
    Err("VALORANT can only be launched on Windows.".into())
}

/// The Riot Client's arguments that start VALORANT on the live patchline.
#[cfg_attr(not(windows), allow(dead_code))] // Windows only; the tests use it everywhere.
pub const LAUNCH_ARGS: [&str; 2] = ["--launch-product=valorant", "--launch-patchline=live"];

/// `RiotClientServices.exe` from Riot's install list (`rc_default`, else
/// `rc_live`), when that file exists.
#[cfg_attr(not(windows), allow(dead_code))] // Windows only; the tests use it everywhere.
pub fn riot_client_exe(installs: &Path) -> Result<PathBuf, String> {
    let text = std::fs::read_to_string(installs)
        .map_err(|_| "The Riot Client isn't installed on this PC.".to_string())?;
    candidates(&text)
        .into_iter()
        .find(|p| p.is_file())
        .ok_or_else(|| "Couldn't find the Riot Client. Start it once, then try again.".to_string())
}

/// The paths `RiotClientInstalls.json` names, in the order tried
/// (`rc_default`, then `rc_live`); none for a file that is not JSON.
#[cfg_attr(not(windows), allow(dead_code))] // Windows only; the tests use it everywhere.
fn candidates(text: &str) -> Vec<PathBuf> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return Vec::new();
    };
    ["rc_default", "rc_live"]
        .iter()
        .filter_map(|key| value.get(key)?.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_install_list_is_read_from_program_data_on_any_drive() {
        assert_eq!(
            riot_installs(Some(r"D:\ProgramData".into())),
            Path::new(r"D:\ProgramData")
                .join("Riot Games")
                .join("RiotClientInstalls.json")
        );
        assert!(riot_installs(None).starts_with(r"C:\ProgramData"));
    }

    #[test]
    fn the_install_list_names_the_client_rc_default_first() {
        let dir = tempfile::tempdir().unwrap();
        let default = dir.path().join("default.exe");
        let live = dir.path().join("live.exe");
        std::fs::write(&default, b"").unwrap();
        std::fs::write(&live, b"").unwrap();
        let installs = dir.path().join("RiotClientInstalls.json");
        let json = serde_json::json!({
            "associated_client": {},
            "rc_default": default.to_string_lossy(),
            "rc_live": live.to_string_lossy(),
        });
        std::fs::write(&installs, json.to_string()).unwrap();
        assert_eq!(riot_client_exe(&installs).unwrap(), default);
    }

    #[test]
    fn a_missing_rc_default_falls_back_to_rc_live() {
        let dir = tempfile::tempdir().unwrap();
        let live = dir.path().join("live.exe");
        std::fs::write(&live, b"").unwrap();
        let installs = dir.path().join("RiotClientInstalls.json");
        let json = serde_json::json!({
            "rc_default": dir.path().join("gone.exe").to_string_lossy(),
            "rc_live": live.to_string_lossy(),
        });
        std::fs::write(&installs, json.to_string()).unwrap();
        assert_eq!(riot_client_exe(&installs).unwrap(), live);
    }

    #[test]
    fn errors_are_readable_sentences() {
        let dir = tempfile::tempdir().unwrap();
        let none = riot_client_exe(&dir.path().join("nope.json")).unwrap_err();
        assert_eq!(none, "The Riot Client isn't installed on this PC.");
        let installs = dir.path().join("RiotClientInstalls.json");
        std::fs::write(&installs, r#"{"rc_default": "Z:\\nowhere\\x.exe"}"#).unwrap();
        let gone = riot_client_exe(&installs).unwrap_err();
        assert!(gone.starts_with("Couldn't find the Riot Client"), "{gone}");
        std::fs::write(&installs, "not json").unwrap();
        assert!(riot_client_exe(&installs).is_err());
    }

    #[test]
    fn the_launch_arguments_start_valorant_on_live() {
        assert_eq!(
            LAUNCH_ARGS,
            ["--launch-product=valorant", "--launch-patchline=live"]
        );
    }
}
