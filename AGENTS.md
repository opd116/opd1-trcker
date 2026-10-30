# Coding agent guide

Read `CLAUDE.md` for product rules and `docs/handoff.md` for current status before changing anything. Read the relevant dated user requirements in `docs/requirements-raw.md` and rulings in `docs/DECISIONS.md`. Newer explicit user decisions win over older text.

## Current checkout

- Project name: **OPD1 TRCKER** (intentional spelling).
- Final reviewed release: **v0.4.0** on `main` (2026-10-31). Names learned locally are shared across Riot accounts by PUUID while account-scoped data (history/My Stats/encounters/collection) follows the current session; see `CLAUDE.md` and `docs/handoff.md`.
- Tauri 2 + Rust + Svelte 5 + Vite + SQLite. English UI; brief Gulf Arabic communication.
- Current UI: Live, Saved, History, Comp, Skins, ASCII, Settings, About; profile drawer and History scoreboard.
- Default 1280×800; minimum 1180×760; resizable; Glance 420px wide.
- `docs/design-spec.md`, current Svelte/CSS and `ui/tests/baselines/` are the available design references. No external design folder or agent plugin is required.
- `docs/plan.md` is historical, not an executable backlog. No old branch ownership or machine-specific worktree paths apply.
- Keep the app light. Protect hidden names, credentials and saved SQLite data. Do not add runtime dependencies without the user's approval.

## Commands (from repository root)

Use Node 24, Rust stable and the checked-in lockfiles.

```bash
npm ci
npm --prefix ui run build
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p opd1-core -p opd1-riot -p opd1-trcker
npm --prefix ui run test
npm --prefix ui run check
(cd ui && npx playwright test)
```

Playwright defaults to 1280×800; fit tests also cover the minimum size and Glance. Match its browser version to the pinned CI image; do not use old machine-specific browser symlink workarounds.

Linux Tauri builds require WebKitGTK 4.1, libsoup 3, librsvg, build-essential, pkg-config and OpenSSL development packages. Windows installer: `npx tauri build`. Browser UI development: `npm --prefix ui run dev`.

Only Windows with VALORANT can verify the real Riot client. Browser fixtures cannot establish live-client correctness.

## Changes and verification

- Work from the actual current branch; use a focused branch for review when appropriate.
- Commit explicit paths only. Never force-push, commit credentials, or include unrelated changes.
- Keep user decisions, rules and handoff in step with the implementation.
- Verify CI for the exact changed commit; do not use an older run as proof. The handoff records the pre-fix failures and navigation fix.
- Do not weaken tests or regenerate screenshot baselines merely to make CI pass.
- Publish a release only after its exact commit passes the required checks. An installer artifact alone is not a passing release.
- Update `docs/handoff.md` directly at the end of a session; no slash command, plugin or skill installation is required.
