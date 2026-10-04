# Handoff: OPD1 TRCKER after v0.3.0 (2026-10-03)

Written by hand at the end of the v0.3.0 phase (`/handoff` is not installed in the cloud sessions). It replaces the session 2 → 3 note.

**Next session's job, in order:**
1. Wait for the user's Windows test of v0.3.0 (`docs/test-checklist.md`) and act on their result, log (Ctrl+Shift+L) or dump (Ctrl+Shift+D).
2. After the act change on **2026-10-14**: check that My Stats starts over for the new act and that the first content-service answer is read right (R162).
3. Push the `v0.2.0` and `v0.3.0` tags (see Open items).
4. Build nothing new until the user asks. Match History and Settings stay "later" (CLAUDE.md).

## Release status

- **`main` = `493d4f9`** `chore(release): v0.3.0` (the working branch `claude/bold-johnson-tfn8bb` is at the same commit).
- **CI:** run 42, https://github.com/opd116/opd1-trcker/actions/runs/37145897060. Both jobs green: `ui-e2e` (Linux, Playwright) and `build` (Windows: Rust tests, UI tests, svelte-check, NSIS, the dist guard, the 10 MB check). Artifact `OPD1-TRCKER-setup`, 3.8 MB zipped, expires 2027-01-01.
- **Local checks at `6ebdf5c`:**
  - Rust: every test passes (`opd1-trcker` 467), clippy `-D warnings` and `cargo fmt --check` clean.
  - Vitest: 500 passed.
  - svelte-check and tsc: 0 errors.
  - Playwright: 370 passed, 19 skipped (the specs that need the design file).
- **The user's last word:** the 2026-10-01 build (`44ae56d`) "works" (R168).

## What v0.3.0 did (details in `docs/DECISIONS.md`)

| Ruling | What |
|---|---|
| R168 | The user's scope: fixes plus the My Stats page, released on `main` |
| R169 | The poll loop restarts after a panic (1 s, 5 s, then 30 s), shows the Closed card at once, aborts the dead worker |
| R170 | `opd1-riot` takes its locks poison-tolerantly |
| R171 | Player Details retries after a failed match-details download; a sent 429 counts as held |
| R172 | The names fill re-reads the presence before it sends (R165 finding 9 closed) |
| R173 | The user's own competitive updates are backfilled for the act (R72 done): pages of 20, 2.5 s apart, at most 10 |
| R174 | The `"mystats"` push event: 750 ms coalescing, a 15-minute tick, account-safe |
| R175 | The My Stats page; `LiveState.settled` picks the start page; `MyStats.puuid`; Back restores the scroll position |
| R176 | Playwright runs in CI on Linux |

## Where the truth lives

| What | Path |
|---|---|
| Project rules | `CLAUDE.md` |
| The user's words (newest line 2026-10-03) | `docs/requirements-raw.md` |
| Every ruling, R1–R176 | `docs/DECISIONS.md` |
| UI build spec (§9 is My Stats) | `docs/design-spec.md` |
| How the parts fit | `docs/ARCHITECTURE.md` |
| Release notes | `CHANGELOG.md` `[0.3.0]` |
| The user's test list, in Arabic, plus the controller's dump checks | `docs/test-checklist.md` |
| History of the build plan (Task 14+ checkboxes are historical) | `docs/plan.md` |

## Open items

- **Tags not pushed.** `v0.2.0` (`a1e3315`) and `v0.3.0` (`493d4f9`) were created in the session but the session's git proxy answered HTTP 403 on tag pushes. Push them from a session allowed to, or have the user create them as GitHub Releases in the web UI. They only label the history; the download is not affected.
- **Review minors left as they are:**
  - The Session streak chips have no tooltip: `SessionSummary` carries the results only, not per-match data.
  - A µs window in `src-tauri/src/my_stats.rs` can emit the old account's stats once right after an account switch; the switch's own push replaces them at once.
  - The `ui/tests/shots/mystats__*.png` screenshots are written but never compared with references.
- **Still unverified on the user's PC** (checklist controller section): the R99 lock value, the R100 level, pregame `Loadouts[].Subject`, the v3 loadout `Guns[]` shape, whether match-details returns hidden players' names (R74, R94), the cause of the MMR 500 (R159), the competitive-updates paging in the log (R173), the first content-service answer (R162) and the RAM figure.

## Environment notes (cloud container)

- **Tauri Linux deps:** `apt-get install -y libwebkit2gtk-4.1-dev libsoup-3.0-dev librsvg2-dev build-essential pkg-config libssl-dev`, then `npm ci` and `npm --prefix ui run build` before any cargo command (the Tauri crate needs `ui/dist`).
- **Playwright:** the repo's 1.63 expects Chromium build 1243; `/opt/pw-browsers` has 1194. Never run `playwright install` locally. The workaround that worked: `mkdir /opt/pw-browsers/chromium_headless_shell-1243`, link `chrome-headless-shell-linux64` in it to `/opt/pw-browsers/chromium_headless_shell-1194/chrome-linux`, link `chrome-headless-shell` → `headless_shell` inside that folder, and touch `INSTALLATION_COMPLETE` and `DEPENDENCIES_VALIDATED`.
- **Node:** the container has Node 22 / npm 10. `npm version` there rewrites the `libc` fields of `package-lock.json`; bump the three version fields in the lock by hand instead.
- **Missing here:** `graft` is not installed, and `.superpowers/` (the design file) is not in the cloud clone, so the two visual specs skip.
- **Worktree agents** branch from the local `origin/main` ref, which can be stale. Run `git fetch` first, or merge the working branch into the worktree before its work is reviewed.
- **Agents:** Claude orchestrates; one implementer and one fresh reviewer per step, on Sonnet (R147).
- **CI** runs only on a push to `main` or `workflow_dispatch`. This session developed on `claude/bold-johnson-tfn8bb` and fast-forwarded `main` with the user's OK (R168).

## Notes about the user

- Short Gulf Arabic, blunt. Show pictures, not jargon.
- Wants progress updates, not "should I continue?" questions.
- Doesn't code; tests on Windows with VALORANT and sends the log or a dump when something is wrong.
