//! Tauri's build step, plus two facts the About screen shows (R189):
//! `OPD1_GIT_COMMIT` (the short git hash of the checkout) and
//! `OPD1_BUILD_MS` (when this build ran, epoch ms). `v2::appinfo` reads both
//! with `option_env!`, so a build without git (a source copy, no `git` on
//! the PATH) just leaves the commit unknown — nothing here can fail the build.

use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

/// `git <args>` in this crate's directory, trimmed; `None` when git is
/// missing, fails, or prints nothing.
fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (!text.is_empty()).then_some(text)
}

fn main() {
    if let Some(hash) = git(&["rev-parse", "--short", "HEAD"]) {
        println!("cargo:rustc-env=OPD1_GIT_COMMIT={hash}");
        // Run again when HEAD moves (a commit, a checkout), so the hash is
        // never a stale one. Works in a linked worktree too: HEAD lives in
        // its own git dir, the branch refs in the common one.
        if let Some(git_dir) = git(&["rev-parse", "--git-dir"]) {
            println!(
                "cargo:rerun-if-changed={}",
                Path::new(&git_dir).join("HEAD").display()
            );
        }
        if let (Some(common), Some(head_ref)) = (
            git(&["rev-parse", "--git-common-dir"]),
            git(&["rev-parse", "--symbolic-full-name", "HEAD"]),
        ) {
            let common = Path::new(&common);
            if head_ref.starts_with("refs/") {
                println!(
                    "cargo:rerun-if-changed={}",
                    common.join(&head_ref).display()
                );
            }
            println!(
                "cargo:rerun-if-changed={}",
                common.join("packed-refs").display()
            );
        }
    }
    if let Ok(now) = SystemTime::now().duration_since(UNIX_EPOCH) {
        println!("cargo:rustc-env=OPD1_BUILD_MS={}", now.as_millis());
    }
    tauri_build::build()
}
