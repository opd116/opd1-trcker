# Current handoff — 2026-10-29

## Read first

Read `AGENTS.md`, `CLAUDE.md`, then the relevant user requirements and decisions. No agent plugin, external design folder or special handoff command is required. This note supersedes the 2026-10-04 task brief.

## Code and release state

- Reviewed code: `bf41497278578c37527216c9611365dbedbb8a5c`, merged PR #4 on 2026-10-26 (bug hunt after the v2 UI).
- The source version fields still say 0.3.1. The published `v0.3.1` release is from 2026-10-04 and does **not** contain the later v2 UI and fixes.
- The v2 screens, saved players/notes, settings, tray/autostart, profile drawer and scoreboard are implemented. Do not restart their old implementation tasks.
- `CHANGELOG.md` lists this work under Unreleased.
- This maintenance pass removes retired agent integrations and refreshes onboarding documentation; it does not fix app behavior or change the app version.

## Known verification blocker

Build run [40299043306](https://github.com/opd116/opd1-trcker/actions/runs/40299043306), for the reviewed code:

- Windows installer build and Windows Rust tests: passed.
- Linux UI build, release guards, formatting, clippy, Rust tests, Vitest and svelte-check: passed.
- Playwright: **281 passed, 8 failed**. Overall checks failed.

Failures are in `close-match.spec.ts` (2), `focus-ring.spec.ts` (1), `live-v2.spec.ts` (2), `profile.spec.ts` (1), `saved-history.spec.ts` (2). They concern scoreboard navigation/return, focus restoration, drawer dismissal and returning to saved/history lists. These are observed test failures, not yet a proven root cause. Reproduce and investigate implementation and harness before changing either.

The installer artifact of that run exists, but successful packaging is not evidence of a passing release. Always check CI for the exact new commit.

## Next work

1. Handle the user's requested task within current rules; if the goal is release readiness, investigate the eight Playwright failures first.
2. Run the relevant tests, then the required full checks before publishing. Keep intentional screenshot changes separate from functional fixes.
3. Refresh release notes, version fields and user checklist together when actually releasing. Do not publish or tag just because an old prompt told you to.
4. Keep this file current at session end.

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
