هلا. نكمل على OPD1 TRCKER من بعد نسخة 0.3.0. اقرأ `docs/handoff.md` و`CLAUDE.md` أول شي.

أبيك تمشي على الخطة اللي تحت: إصلاحات، وتقوية، وتحسينات للبناء والتوزيع، وترتيب الكود. وبعدها اقترح علي ميزات جديدة، واسألني قبل ما تبني أي ميزة.

كلّمني بالعربي وباختصار. لما تخلص ويكون كل شي أخضر، ارفع على main مثل قبل وعطني رابط التنزيل.

---

# Brief for Claude (English)

You are continuing **OPD1 TRCKER**, a light Windows VALORANT tracker (Tauri 2 + Rust core + Svelte 5 SPA + SQLite). The name is spelled `OPD1 TRCKER` on purpose. The last release is **v0.3.0** (release commit `493d4f9`, CI run 42 green; `main` carries only docs commits after it). The user tested the 2026-10-01 build and said it works. v0.3.0 added the My Stats page and a robustness round (rulings R168–R176).

## 0. Ground rules (binding)

- **Read first:**
  - `CLAUDE.md` (project rules; they override defaults)
  - `docs/handoff.md` (state, open items, environment notes)
  - `docs/DECISIONS.md` R150–R176
  - `docs/requirements-raw.md` (the user's words; newest block last)
  - `docs/design-spec.md` §9 (My Stats)
  - `docs/ARCHITECTURE.md`
  - `docs/test-checklist.md`
- **Start in plan mode.** Explore, then write the plan, then ask the user only what is genuinely theirs to decide (Phase E), then execute.
- **Talking to the user:** short Gulf Arabic, no jargon. Progress updates, never "should I continue?". Show screenshots, not descriptions.
- **Lightness is priority #1** (RAM, CPU, bundle size, request count) and the tie-breaker; elegant design is #2.
- **Never break these:**
  - The hidden-name rules: never request a live-incognito player's name; name asks only between matches (R150–R153, R172).
  - No status text or badges; skeletons with no text; `—` for missing values.
  - Minimum 12px, tabular numerals, no text crowding numbers.
  - Fixed 1000×800 window; no scrolling on Live.
  - No instalock, dodge or client writes. No auto-updater, tray or autostart. English UI only.
- **Design:** a screen or state the design file doesn't cover is Claude's call, built only from existing tokens and components (`ui/src/lib/theme.ts`, `ui/src/components/**`). Don't ask the user about type or spacing. Verify every screen at exactly 1000×800 in headless Chromium, look at the screenshots yourself, then show the user.
- **Git:**
  - Develop on the session's designated branch.
  - Commit every step with a conventional message and explicit paths (`git commit -m "…" -- <paths>`), and push right away.
  - Never commit `.superpowers/` or a root `*.zip`.
  - When everything is green, fast-forward `main` (the user approves this in the Arabic lines above).
- **Database:** every schema change is a new migration appended to `MIGRATIONS` (`PRAGMA user_version`). Existing migrations are frozen. The installer never deletes the DB.
- **Working method:**
  - Claude orchestrates; subagents implement, on Sonnet (R147).
  - Every step gets a fresh, skeptical reviewer subagent (correctness, rule violations, races, test quality), then a fix round, then a scoped re-review.
  - Rust and UI work can run in parallel only in separate worktrees. Merge the working branch into a worktree before reviewing it, because worktrees branch from a possibly stale `origin/main`.
- **Records:**
  - New rulings get **R177 and up** in `docs/DECISIONS.md`.
  - Every user decision gets a dated line in `docs/requirements-raw.md`.
  - Keep `docs/design-spec.md` and `docs/ARCHITECTURE.md` in step with the code.

## 1. Session start: environment and baseline

1. Container setup (details in `docs/handoff.md` → Environment notes):
   - `apt-get install -y libwebkit2gtk-4.1-dev libsoup-3.0-dev librsvg2-dev build-essential pkg-config libssl-dev`
   - `npm ci && npm --prefix ui run build`
   - The Playwright Chromium symlink workaround (Playwright 1.63 expects build 1243, `/opt/pw-browsers` has 1194). Never run `playwright install` locally.
2. Baseline, recorded before any change:
   - `cargo test -p opd1-core -p opd1-riot -p opd1-trcker`
   - `cargo clippy --workspace --all-targets -- -D warnings`
   - `cargo fmt --check`
   - `npm --prefix ui run test`
   - `npm --prefix ui run check`
   - `cd ui && npx playwright test`
   - Expect roughly: trcker 467 Rust tests, 500 Vitest, 370 Playwright passed with 19 skipped (the specs that need the design file).
3. Check the GitHub state through the GitHub MCP tools: the last runs on `main`, open issues and PRs.
4. **Ask the user for their v0.3.0 test result** (in Arabic, one short message). Ask for the checklist items that failed, a screenshot, the Ctrl+Shift+L log, and the Ctrl+Shift+D dump folder if anything looked wrong. Their report outranks everything below.

## 2. Phase A — The user's test (top priority)

- A1. Triage every problem the user reports. Reproduce it from their log or dump, or with a DEV demo scene. Root-cause it, fix it, add a test that fails without the fix, and record a ruling.
- A2. Work the controller checks in `docs/test-checklist.md` (bottom section) against the user's dumps and DB:
  - The pregame lock value (R99).
  - The user's `AccountLevel` while `HideAccountLevel` is on (R100).
  - Pregame `Loadouts[].Subject`.
  - The v3 loadout `Guns[]` shape (R90, R148).
  - Whether match-details returns hidden players' names (R74, R94). If it does, apply the R74 follow-up described there.
  - The cause of the MMR 500 (R159).
  - The competitive-updates paging in the log (R173): no repeated 429 on `/mmr/`.
  - After **2026-10-14**: the first content-service answer (R162), and that My Stats, Win% and the Peak `This Act` label switch to the new act cleanly.
  - The RAM figure against the lightness goal.
- A3. If the test shows any request slowness, compare with Valorant-Scout's `backend/riot_client.py` and `live_match.py` (the reference implementation) before changing the throttle (R160).

## 3. Phase B — Fixes and hardening (no user decision needed)

- **B1. UI errors reach the log.**
  - Today a crashed page or an unhandled promise only goes to `console.error` (`ui/src/App.svelte` boundary), which is invisible in a release build.
  - Add a `ui_log(level, message)` command that writes into the Rust log ring (`logbuf`), so Ctrl+Shift+L and the dump include UI failures.
  - Wire it to the page boundary, `window.onerror` and `unhandledrejection`.
  - Rate-limit it (e.g. 20 lines a minute), cap each line (500 chars), and shorten uuids the way `raw_capture::excerpt` does.
  - Tests on both sides.
- **B2. A useful log header.** `copy_log` starts with the app version, Windows version (if cheap), stage and `settled`, the shard/region, the client-version source (R159) and the current act id. It costs one line and saves a round trip with the user every time.
- **B3. Request holds survive a poller restart.** After a supervisor restart (R169) the new session starts with empty 429 holds, so it can hit Riot again during a block. Keep the hold instants (per family, R160) in `AppState` and seed the new session's throttle from them. Test it.
- **B4. Close the µs window in `src-tauri/src/my_stats.rs`.** Read the current `self_puuid` under the push lock, so an old account's stats can never be emitted after a switch (the review noted it corrects itself, but make it exact).
- **B5. Session chips get tooltips.**
  - Extend `SessionSummary` with per-match entries (match id, map id, RR change, start time).
  - Update the `my_stats.json` golden, the TS types, the descriptors and the demo fixtures.
  - The My Stats session chips show a per-match tooltip like the card's last-5 (map · RR · time ago).
  - Clicking a chip may open Match Details. Make it consistent with Recent Matches.
- **B6. Act-change hardening.** Add a pipeline test that switches the content-service act mid-session and checks:
  - My Stats is rebuilt for the new act and pushed (R174).
  - The competitive-updates backfill restarts for the new act without paging past its start.
  - The live card's Win%, Peak and Prev follow the new act.
  - Nothing shows stale old-act numbers.
- **B7. DB performance at scale.**
  - Build a synthetic DB (about 5,000 matches, 50,000 `match_players` rows, 2,000 competitive updates).
  - Add bounded-time tests for `card_for`, `load_my_stats`, Player Details and encounters, e.g. under 50 ms each in release.
  - Check every hot query with `EXPLAIN QUERY PLAN`. Add indexes only through a new migration, and only if a plan shows a scan.
  - Run `PRAGMA optimize` once on clean exit (cheap). No VACUUM by default.
- **B8. Idle cost audit.** With VALORANT closed the only periodic work should be the 5 s lockfile check and the 15-minute My Stats tick. Prove it with a test or a trace. Also check that no image or table request runs while Closed after the first load.
- **B9. Image memory.** Measure the decoded size of the Player Card art and the map images on the Live page (WebView memory is the biggest RAM cost). If the art is larger than its display size × 2, have the `img://` handler serve a resized copy cached on disk (once per asset). Note the RAM before and after.
- **B10. Visual regression baselines.**
  - Today the `ui/tests/shots/*.png` files are written but never compared.
  - Add self-baseline comparisons (`toHaveScreenshot`, small `maxDiffPixelRatio`) for the main demo scenes: Live in game, Agent Select, Lobby with 3 players, Details, Match, My Stats, My Stats new account.
  - Generate and compare them only inside the pinned Playwright Docker image (`mcr.microsoft.com/playwright:v1.63.0-jammy`) in CI, so fonts render identically.
  - Update the baselines only on purpose, through a `workflow_dispatch` input.
- **B11. Small UX consistency pass** (Claude's call, from existing tokens):
  - Keyboard focus order on My Stats.
  - Tooltips reachable by keyboard focus.
  - The empty and loading states of every My Stats section match the Details skeleton style.
  - Re-screenshot and check everything at 1000×800.

## 4. Phase C — Build, CI and distribution

- **C1. GitHub Releases instead of expiring artifacts.**
  - Artifacts expire after about 90 days, and a cloud session cannot push tags (HTTP 403).
  - Add a `release.yml` workflow (`workflow_dispatch` with a `version` input, `permissions: contents: write`).
  - It checks that the version matches `tauri.conf.json`, builds the NSIS installer on `windows-latest`, creates the tag `v<version>` itself with `GITHUB_TOKEN`, and publishes a GitHub Release (still private) with the `Setup.exe` attached and the matching `CHANGELOG.md` section as notes.
  - Use it for this phase's release. Also create the missing `v0.2.0` (`a1e3315`) and `v0.3.0` (`493d4f9`) tags and Releases if the workflow can tag a given commit; otherwise leave them and say so.
- **C2. A versioned installer name.** The artifact and installer are named `OPD1-TRCKER-<version>-setup`.
- **C3. Rust lint in CI.**
  - Add `cargo fmt --check` and `cargo clippy --workspace --all-targets -- -D warnings` to a Linux job: install the Tauri Linux deps there and build `ui/dist` first.
  - Merge it with the Playwright job into one `checks` job to save minutes.
- **C4. Cheap checks on every branch.**
  - Run the Linux `checks` job on pushes to any branch and on pull requests.
  - Keep the Windows installer build on `main`, `workflow_dispatch` and `release.yml` only, because Windows minutes cost double in a private repo.
- **C5. Dependency hygiene.**
  - Add a non-blocking `cargo audit` and `npm audit --omit=dev` step (or a monthly scheduled workflow).
  - Bump patch and minor versions of Tauri, Svelte and Vite only if the full suite stays green and the installer and JS bundle don't grow without a reason. Record any notable bump.
- **C6. Size tracking.** The CI log prints the installer size and the gzipped JS and CSS size. Current reference: about 3.8 MB zipped installer, about 46 kB gzipped JS.

## 5. Phase D — Code health (pure refactors, no behaviour change)

- **D1. Split the three largest files** into focused modules, with their tests moved alongside. One file per commit, and the full suite stays green with **identical test counts**.
  - `src-tauri/src/state.rs` (~4,800 lines), e.g. `state/{live_model,jobs,names_queue,backfill,publish}.rs`
  - `src-tauri/src/poll.rs` (~3,400 lines), e.g. `poll/{supervisor,stage,roster_fetch,post_match}.rs`
  - `src-tauri/src/sync.rs` (~2,600 lines), e.g. `sync/{rank,matches,details,backfill,comp_backfill,names}.rs`
- **D2. The two re-deferred minors** from `docs/plan.md` ("Deferred minors re-deferred (2026-09-29)": the double sync and the connect-backoff reset) were parked "until the poll.rs split". Fix them right after D1, each with a test and a ruling.
- **D3. Share the backfill anchor code.** The match backfill (`self_history_anchor:{puuid}`) and the competitive-updates backfill (`self_comp_anchor:{puuid}`) use the same JSON shape and per-account rules.
- **D4. Share the table row styles** between Details' Recent Competitive Matches and My Stats' Recent Matches and tables (one component or one CSS module) to cut duplicate CSS. Re-check the pixel alignment test that compares the two pages.
- **D5. Docs.** Update the ARCHITECTURE file map after the split, and keep `docs/plan.md` as history with a dated pointer section.

## 6. Phase E — Product improvements (ask the user first)

CLAUDE.md says new pages are built only when the user asks. After Phases A–D are planned, ask **one** AskUserQuestion: multi-select, short Gulf Arabic, each option with a one-line benefit and its lightness cost, your recommendation first. Build only what they pick, each as its own step with a reviewer and 1000×800 screenshots shown to the user.

- **E1. Settings page.**
  - Exposes the six design options that already exist (tone, accent, result colours, result fill, font, last-5 style; `ui/src/lib/theme.ts`).
  - Saved in `app_state`; ship the two other bundled fonts (R149 parked them).
  - Becomes a third tab only if the user agrees (CLAUDE.md says tabs stay Live / My Stats until then).
- **E2. Match History page.** The user's stored matches from the DB, newest first, filters for queue, map and agent, a virtualized list (no scroll lag), and a click opens Match Details. No new requests: the DB already holds the history from the backfills.
- **E3. My Stats for previous acts.** An act picker; the data needs a bounded per-act backfill (match history and competitive updates), run only when the user opens a past act.
- **E4. Private player notes.** A short note per PUUID, shown as a small icon on the card with the note in the tooltip, edited from Player Details. Needs a new migration.
- **E5. Team average rank In Game.** The ally and enemy average rank in the top bar beside the map, the same block as the Lobby's Average Party Rank.
- **E6. Richer encounter tooltip.** The last time you met the player, the result and the map.
- **E7. Remember the window position** on screen between runs (the size stays fixed at 1000×800).
- **E8. Show/hide hotkey** for the always-on-top window (e.g. Ctrl+Shift+H), so it can be hidden in game without closing it. Only if it needs no extra background process.

## 7. Release and hand-off

- The version is **v0.3.1** if only Phases A–D ship, **v0.4.0** if any Phase E feature ships.
- Bump `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`, `package.json` and `ui/package.json`. Edit the three version fields in `package-lock.json` by hand (the container's npm 10 rewrites `libc` fields). Run `cargo check -p opd1-trcker` for `Cargo.lock`.
- Fill the `CHANGELOG.md` release section from `git log`, with hashes, in plain user-facing English.
- Update the README (features, screenshots in `docs/screenshots/`) and rewrite `docs/test-checklist.md` in short Gulf Arabic for the new version, keeping the controller section.
- Run the full local suite. Fast-forward `main`, wait for CI to go green, and publish through `release.yml` (C1).
- Send the user, in Arabic:
  - the Release link (or the Actions run link);
  - the install steps: download, run over the old version, More info → Run anyway, the history is kept;
  - the screenshots of anything visual that changed;
  - the checklist.
- Update `docs/handoff.md` for the next session (same layout as now), commit and push.

## 8. Verification standard (every step)

- Local, before every push:
  - `npm --prefix ui run build`
  - `cargo test -p opd1-core -p opd1-riot -p opd1-trcker`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo fmt --check`
  - `npm --prefix ui run test`
  - `npm --prefix ui run check`
  - `cd ui && npx playwright test`
  - `grep -rqE "demo-feed|VesperUnbreakabl|Vantablack|fonts.googleapis|fonts.gstatic|unpkg.com" ui/dist` must find nothing.
- A new test for every fix that fails without the fix. No sleep-based timing tests: use the existing model-clock and test seams.
- Every UI change is screenshotted at exactly 1000×800 in all three fonts and checked for overlap, clipping and scrollbars.
- An independent reviewer subagent for every step, and a scoped re-review after each fix round.
- CI green on `main` before handing anything to the user.
- Anything that touches the real Riot client is verified by the user on Windows. Ask for the result or the Ctrl+Shift+L log.
