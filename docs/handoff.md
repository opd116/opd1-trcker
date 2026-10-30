# Current handoff — 2026-10-29

## Read first

Read `AGENTS.md`, `CLAUDE.md`, then the relevant user requirements and decisions. No agent plugin, external design folder or special handoff command is required. This note supersedes the 2026-10-04 task brief.

## Code and release state

- Reviewed code: `bf41497278578c37527216c9611365dbedbb8a5c`, merged PR #4 on 2026-10-26 (bug hunt after the v2 UI).
- The source version fields still say 0.3.1. The published `v0.3.1` release is from 2026-10-04 and does **not** contain the later v2 UI and fixes.
- The v2 screens, saved players/notes, settings, tray/autostart, profile drawer and scoreboard are implemented. Do not restart their old implementation tasks.
- `CHANGELOG.md` lists this work under Unreleased.
- This maintenance pass removes retired agent integrations and refreshes onboarding documentation; it does not fix app behavior or change the app version.

## Previous verification failures (before the navigation fix)

Build run [40299043306](https://github.com/opd116/opd1-trcker/actions/runs/40299043306), for the reviewed code:

- Windows installer build and Windows Rust tests: passed.
- Linux UI build, release guards, formatting, clippy, Rust tests, Vitest and svelte-check: passed.
- Playwright: **281 passed, 8 failed**. Overall checks failed.

Failures are in `close-match.spec.ts` (2), `focus-ring.spec.ts` (1), `live-v2.spec.ts` (2), `profile.spec.ts` (1), `saved-history.spec.ts` (2). They concern scoreboard navigation/return, focus restoration, drawer dismissal and returning to saved/history lists. These are observed test failures, not yet a proven root cause. Reproduce and investigate implementation and harness before changing either.

The installer artifact of that run exists, but successful packaging is not evidence of a passing release. Always check CI for the exact new commit.

## Navigation fix — 2026-10-29

The shared cause found in `App.svelte` / `clickGuard.ts`: for 400 ms after opening a drawer or scoreboard, the capture handler blocked **all** clicks, including independent single clicks and keyboard-generated clicks (detail 0). This also explains timing-dependent failures: the 2026-10-29 documentation-only run had 14 failures rather than 8.

The fix passes the click's detail and coordinates into the guard: repeated mouse clicks (detail > 1), or a second pointer click within 4px of the opening click, are suppressed within the interval. The opening position is recorded only during synchronous click dispatch, so later keyboard or programmatic opens cannot inherit stale coordinates. Fresh clicks elsewhere, keyboard activation and programmatic clicks remain usable. The existing double-click tests remain; new unit and browser regressions cover immediate single-click Close and keyboard Back while the guard clock is held armed.

Local deterministic reproduction confirmed the old guard blocked detail 0 and 1; the corrected module passed single/keyboard/repeat/expiry/re-arm checks. Full app validation runs in GitHub Actions because dependency downloads are unavailable in the local container and Browser reported no originating browser context. Do not infer full-suite success from this local check: inspect the exact current commit's Build run and installer job.

The runtime change is confined to the UI click guard; no Rust, database schema, product design or dependency changed. See `docs/test-checklist.md` for the separate real Windows/VALORANT acceptance checks.

## Bug-hunt round 1 — 2026-10-30

Baseline for this round: `0428c52e803f0aa2f6bb97ecaf4f429836aa8da6` (Build run 40806318208, green). Nothing was merged, tagged or published in this session; every change is on a focused branch with its PR left open for external review. The full evidence ledger is `docs/bug-hunt-2026-10-30.md`.

- **Skins / owned collection — PR [#5](https://github.com/opd116/opd1-trcker/pull/5), branch `fix/skins-collection`, final head `8ebd9d72e78de8eb0ad5e823906a8549cd90f012`.** Confirmed bug: one transient Riot failure on the owned-skins request counted as the ten-minute refresh (`LAST_ASK` written before the answer was examined) and a failed first fetch returned `Ok(None)`, so the Skins screen showed "No collection yet — open this page while VALORANT is running" while the game ran, for up to ten minutes. Fix: only a stored answer counts; a refresh attempt that stored nothing is `Err`, so the screen keeps its retry state (its normal one-minute re-ask). Regression tests: 500-then-200 reaches Riot twice and stores the copy; a stored answer still holds the gate; a failed refresh still serves a stored copy; the no-refresh-attempt states (no session, game closed) stay `Ok(None)` with zero requests. An independent reviewer's nits were fixed in the follow-up commit; the PR body carries the final evidence.
- **Live names — PR [#6](https://github.com/opd116/opd1-trcker/pull/6), branch `fix/names-live`, final head `5d4610b6fb4f8216e518c37d34959f3e7c884e0f`.** Confirmed bug: on a reset stage transition (dodge from Agent Select back to the Lobby, and a match end) `forget_synced()` cleared the stage's live name answers while carried roster slots kept a DB bundle read before the name was stored, so a name that was on screen was replaced by the loading skeleton until name-service answered again. Fix: a reset transition marks the next roster install to read back one keyed `players` row per carried player (restoring it only when the slot has no name, so a concurrent newer row is never overwritten); the rename re-ask after a reset is preserved. Regression test: `src-tauri/src/pipeline_tests/name_stability.rs` fails on baseline with `name: Loading` after the dodge and passes with the fix. An independent reviewer's nits were fixed in the follow-up commit.
- **Requests / retries / resources — investigated, no confirmed bug.** Throttling, token-refresh and 429 caps, poll-loop cadences and retry backoffs, worker job selection, backfills, image retries and table refresh windows were audited and found bounded. A headless-Chromium endurance probe (40 cycles over all screens, profile drawer, scoreboard and stage changes) showed constant active timers/listeners/DOM nodes and a decelerating heap warm-up (~6.5 → ~10.4 MB, no linear leak); scratch scripts were removed. Two suspected-but-unreproduced items are recorded in the ledger (History's UI cache is not account-scoped; one full local Rust run exited 1 with the visible tail green and did not reproduce).
- **Local verification:** Rust `fmt`/`clippy` and the full core+riot+trcker tests are green on both branches; UI `build`/`vitest`/`svelte-check` green. A local functional Playwright run on baseline was 275 passed / 2 failed: both are `fit-v2` "scoreboard, longest names" clipping checks that only fail with Windows font rendering; CI on the same commit with the pinned Linux image is green, so they are not treated as app bugs (visual baselines are CI-image specific).
- **Combined integration verification:** temporary local branch `integrate/bug-hunt-round1` merged exactly the two final heads above on top of `0428c52`; local merge commit `1e86a5a`, no conflicts. Results: UI build, Vitest (618 passed, 1 skipped), `svelte-check`/`tsc`, `cargo fmt --check` and `cargo clippy -D warnings` clean; `cargo test -p opd1-core -p opd1-riot -p opd1-trcker` = **1160 passed, 0 failed** (baseline 1155 + 5 new regression tests). Browser CI per PR's exact head remains the authority; check the final report for the exact run numbers.
- **Docs/changelog** are consolidated here so the fix PRs stay code + tests only. Recommended merge order: #5, #6, then this docs PR.

## Next work

1. Verify each fix PR's CI on its exact final head; the docs PR has no code risk but its own CI must be green before review too.
2. Continue the hunt in the ledger's open leads/residual gaps: Skins catalog/image edge cases (valorant-api identifier/chroma relationships against the documented shapes) and the account-switch display/race, name stability in the transitions the current test does not cover (match end, `show_self_only`, second reset, hidden/own player, `db.remembered`), History's UI cache account scope, and the unreproduced flake.
3. Keep the ledger and this file current; do not merge, tag or publish from a bug-hunt session.
4. Refresh release notes, version fields and user checklist together when actually releasing.

## References and history

- Rules: `CLAUDE.md`; user decisions: `docs/requirements-raw.md`; ruling ledger: `docs/DECISIONS.md`.
- Design: latest v2 sections of `docs/design-spec.md`, current `ui/src/`, regression images `ui/tests/baselines/`.
- `docs/plan.md` is the historical implementation record; its unchecked boxes, paths, branches and plugin instructions are not a current backlog.
- `docs/ARCHITECTURE.md` has a current orientation followed by a labeled legacy technical reference.
- Original design exports are not required and are not in the repository. Do not install tooling to recreate their old folder.
- `docs/parked/` retains the historical act-change patch. Do not apply it automatically; inspect current behavior and ask the user before reviving that postponed product work.
- Real Riot client behavior must be verified on Windows with VALORANT; use Ctrl+Shift+L for logs and Ctrl+Shift+D for redacted diagnostics.

## Environment

Use Node 24 and Rust stable, `npm ci`, then build the UI before compiling Tauri. Follow `AGENTS.md` and the current CI workflows for commands and Linux dependencies. Playwright must match the workflow's pinned image. Do not reuse old absolute paths, browser symlink tricks or branch-lane assignments.

## Removed optional tooling

The user requested removal of Graft and obsolete Superpowers dependencies on 2026-10-29. Graft hooks, status lines, MCP registration, skill and search overrides have been removed. Its generated graph was not tracked. Re-adding it later is optional: install/configure it afresh and regenerate its cache from the checkout; no app data depends on it. No global software on the user's PC was uninstalled by this repository change.
