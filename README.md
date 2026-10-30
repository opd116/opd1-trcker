# OPD1 TRCKER

A very light Windows desktop tracker for VALORANT. It reads the **local Riot client API** and shows live Lobby, Agent Select and In Game player cards, plus your own deep competitive stats. Everything stays on your PC.

The name is spelled `OPD1 TRCKER` on purpose.

## Historical screenshots

These images show the earlier UI. For the implemented v2 screens, see `ui/src/screens/` and current regression images in `ui/tests/baselines/`.

### Earlier release

| | |
|---|---|
| ![In Game](docs/screenshots/live-ingame.png) | ![Agent Select](docs/screenshots/live-agent-select.png) |
| ![Lobby](docs/screenshots/live-lobby.png) | ![Player Details](docs/screenshots/player-details.png) |
| ![Match Details](docs/screenshots/match-details.png) | ![My Stats](docs/screenshots/my-stats.png) |

## Features

- Live lobby, Agent Select and In Game views, last-match summary, post-match recap and narrow Glance mode.
- Player ranks, competitive stats, party and encounter indicators with hover detail.
- Player profile drawer, saved players and private notes.
- Match History and scoreboards, Competitive stats, skins collection and ASCII studio.
- Settings, tray behavior, launch with Windows and saved-player notifications.
- Local SQLite history and cached assets; Riot requests run in Rust, not the UI.
- English UI, resizable 1280×800 window (minimum 1180×760), locally bundled fonts.

## Starting an AI coding session

Read [AGENTS.md](AGENTS.md), [CLAUDE.md](CLAUDE.md) and [the current handoff](docs/handoff.md). They provide commands, rules, known failures and the starting point. No additional agent plugin is required. The old implementation plan is history, not a task queue.

## Install (for friends)

1. Download `OPD1-TRCKER-<version>-setup.exe` from the repo's GitHub Releases page (each release keeps its installer for good; a `main` build also leaves it as an Actions artifact for 90 days). The repo is private and the installer is shared by hand, so you normally get it from the person who runs the project.
2. Run the installer. The build is unsigned, so on the Windows SmartScreen screen click **More info**, then **Run anyway**.
3. It installs per user (no administrator rights needed).
4. A reinstall or an update keeps your history. The uninstaller has an optional **Delete app data** checkbox (off by default) that removes it.

The app needs the WebView2 runtime, which Windows 11 and an up-to-date Windows 10 already include.

## Build from source

**Windows:** Rust stable, Node 24 and WebView2, then

```bash
npm ci
npx tauri build
```

**Linux (development):** install `libwebkit2gtk-4.1-dev libsoup-3.0-dev librsvg2-dev`, then

```bash
npm ci
npm --prefix ui run dev      # the UI in a browser; DEV demo scenes with ?scene=<name>
npm --prefix ui run build    # the Rust crate needs ui/dist to exist
cargo test -p opd1-core -p opd1-riot -p opd1-trcker
npm --prefix ui run test
npm --prefix ui run check
(cd ui && npx playwright test)   # layout, tooltip and navigation checks at 1280×800, with additional size coverage (also run by CI on Linux)
```

Anything that talks to the real Riot client can only be verified on Windows with VALORANT installed. Press Ctrl+Shift+L in the app to copy its log. Ctrl+Shift+D writes the redacted raw Riot answers to a folder, and that dump now includes `log.txt` and the error body of each failed call.

## Project structure

```text
crates/core/   opd1-core  — pure logic: parsers, card model, hidden names, party detection, stats, SQLite schema
crates/riot/   opd1-riot  — throttled Riot + valorant-api HTTP, session, asset cache
src-tauri/     opd1-trcker — Tauri shell: poll loop, roster, sync, commands, img:// scheme
ui/            Svelte 5 + Vite single-page UI
docs/          spec, plan, decisions, architecture, design spec
```

See [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) for how the parts fit together, [`docs/DECISIONS.md`](docs/DECISIONS.md) for every ruling, and [`docs/design-spec.md`](docs/design-spec.md) for the UI build spec.

## Stack

Tauri 2 (system WebView2) · Rust core (all Riot and valorant-api HTTP, polling, caching) · Svelte 5 + Vite SPA · SQLite through `rusqlite` (bundled). Rust pushes state to the UI with Tauri events; the UI never polls Riot. No Python, no Electron, no React.

## Status and roadmap

The current reviewed build is **v0.4.0** (2026-10-31). It contains the v2 UI and the bug-fix releases merged after 0.3.1 (see `CHANGELOG.md`); the Windows installer is built by GitHub Actions from the exact release commit. The published 0.3.1 installer from 2026-10-04 does not contain these changes.

Every release is verified by the full CI on its exact commit (Linux Rust/UI/Playwright checks plus the Windows installer build). Installer artifacts live on the GitHub Actions run page; no release is published automatically. The acceptance checklist for a real Windows/VALORANT session is `docs/test-checklist.md`, and the current state plus remaining manual checks are in [handoff](docs/handoff.md).

## Privacy

Local only. The app talks to the Riot client on your PC, to Riot's game servers with your own session (the same calls the game makes), and to valorant-api.com for names and images. It has no account system, no telemetry and no server of its own. It never requests a live-incognito player's name during a live stage; post-match name handling follows the rules in `CLAUDE.md`.

## Licence

Proprietary. See [`LICENSE`](LICENSE). Third-party notices are in [`THIRD-PARTY-NOTICES.md`](THIRD-PARTY-NOTICES.md).

## Disclaimer

OPD1 TRCKER is not endorsed by Riot Games and doesn't reflect the views or opinions of Riot Games or anyone officially involved in producing or managing Riot Games properties. Riot Games and all associated properties are trademarks or registered trademarks of Riot Games, Inc.
