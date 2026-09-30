# Changelog

All notable changes to OPD1 TRCKER are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

Backfilled from `git log` (`6e9655f..c9ad7f4`, then the Phase 2 backend, Tasks 13 to 15b, up to `22ff8d1`). The UI changes and the release section are filled from `git log` at release time.

### Added

- Tauri 2 + Svelte 5 app shell (fixed 1000×800 window, always on top) and a per-user NSIS installer built by GitHub Actions on Windows (`61b5989`).
- Reading of the local Riot client: lockfile, region and shard from the VALORANT log, and the game stage from presence (`ab47bd3`).
- Throttled Riot API client with typed endpoints and automatic re-authentication (`7922033`).
- Local SQLite memory that keeps every player and match the app has seen, with K/D, HS%, win rate and peak-rank math (`209ade9`).
- Player cards: rank and RR, previous act and peak with the act it was reached in, last 5 competitive results as win / loss / draw with the RR change and start time kept for the hover tip, and images cached on disk (`2f8583d`).
- Live pipeline: the app follows Lobby, Agent Select and In Game, fills cards from the local memory first, then syncs only what is new (`ca0b5fe`).
- Hidden-name rules (incognito players show as `Hidden Player`, then as their agent), party detection from recent matches, and encounter counts with the user (`37b9631`).
- My Stats aggregation for the user's own account (backend only, no page yet): rank card, sessions, maps, agents, weekday and time-of-day win rates, and insights (`e9f9acd`).
- Live cards flag the user's own card, and the act win rate counts placement games the way the game client does (`b75e93b`).
- Larger, sharper Player Card art (`b75e93b`).
- A remembered hidden name carries the map and date of the match it was learned in, for its tooltip (`b75e93b`).
- Match Details: any stored match opens as a scoreboard with ACS, K/D/A, +/−, ADR, HS%, half scores and each player's rank in that match, fetched once when the app doesn't have it yet (`07fee46`).
- Player Details: K/D and HS% now cover the player's last 20 competitive matches of the act, and each shared match in Encounters carries its score, length, both players' agents and ranks (`07fee46`).
- Loadout shows named skins (skin, chroma and edition badge) for the Vandal, Phantom, Operator and Sheriff, and allies' skins now show in Agent Select (`1230e32`).
- The user's own skins are read once per connect and kept, so they show in the Lobby and while VALORANT is closed (`1230e32`).
- Database migration 2 (game length, first-half rounds, per-match rank and damage). An existing database upgrades itself and keeps everything it has (`07fee46`).
- Own match-history backfill: matches played while the app was closed are added in the background on connect and after each match, behind the live cards. The first run stops at the current act or 200 matches (`d25705c`).

### Changed

- An agent, or a hidden player's agent name, shows only once it is locked in. Hovering an agent shows nothing (`b75e93b`).
- The user's own card always shows the user's own account level, even when the level is hidden in the game (`b75e93b`, `f02c83c`).
- The CI build now builds the UI first and also runs the app crate's tests (`0cb68c1`).

### Fixed

- Riot requests time out instead of hanging, fields Riot leaves null no longer break parsing, and secrets are redacted from logs (`78b47d7`).
- Display names, match info and the image cache are more robust: non-competitive matches keep their map, and image writes are safe (`f745ee9`).
- Free-for-all lobbies, live updates arriving out of order, the user's own name, and reconnecting after the Riot client restarts (`2a16055`).
- My Stats: placement games no longer skew the average RR, act membership is consistent, and edge cases in session, streak and insight math are handled (`c9ad7f4`).
- A failed match-history call no longer feeds party stripes (`9933e19`).
- A match without a stats row shows empty kills, deaths and assists instead of 0 (`b2d93f6`, `6ddbb00`).
- After an upgrade from an older asset cache, only the small edition-badge table is downloaded again (`6ddbb00`).
- The backfill follows Riot's history total for where the history ends, and a short page leaves no gap (`22ff8d1`).
