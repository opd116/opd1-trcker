# Changelog

All notable changes to OPD1 TRCKER are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0] - 2026-09-30

Filled from `git log` (`6e9655f..c9ad7f4`, then Phase 2: Tasks 13 to 18, 15b and the merge of the UI lane).

### Added

- One Live page in the approved design v8 for all three stages: Lobby (you and your party stacked on the left, Average Party Rank in the top bar with 2 or more players), Agent Select (your team on the left, five empty dashed slots on the right, the map in the top bar) and In Game (both teams, the map at the right end). No scrolling, the same card everywhere (`b578936`, `a13db4a`, `c3bd3ca`).
- Design v8 foundation: the dark palette and tokens, locally bundled fonts with no runtime font requests, and hover tooltips for every stat, tag and indicator (`ab8257a`, `876b8ca`).
- Player Details page (click any card): the hero with the act's K/D, HS%, Win%, games and W / L, the last 5 results, Recent Competitive Matches (3, then Show more up to 8), Top 3 Teammates, Loadout and Encounters, with Back and skeleton loading (`df4dae7`, `ce94fb2`).
- Match Details page (click a recent match or an encounter): header with map, result and half scores, and the scoreboard with ACS, K/D/A, +/−, ADR, HS% and each player's rank, with the hidden-name rules applied; the header date drops the year in the current year when opened from an encounter (`df4dae7`, `ce94fb2`).
- The card's Prev row shows the previous act's rank with a greyed icon first, then the tier name (`4ed8613`).
- The Live cards, Player Details and Match Details are covered by fit checks on 3 fonts, a design comparison against the approved file, tooltip and navigation tests, and a golden contract between the Rust core and the UI (`dbe2a64`, `e701e34`, `42caa16`).
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

- The build is now checked more strictly: CI type-checks the UI and its Playwright suite, fails when demo code or a CDN reference reaches the built UI, and rejects an installer over 10 MB. `THIRD-PARTY-NOTICES.md` is bundled with the installer, and the version is 0.2.0 (`5d38936`, `7dd7b2e`).
- The release build keeps unwinding on a panic instead of aborting, so a stray panic in one background task is logged and does not end the tracker mid-match (R138).
- An agent, or a hidden player's agent name, shows only once it is locked in. Hovering an agent shows nothing (`b75e93b`).
- The user's own card always shows the user's own account level, even when the level is hidden in the game (`b75e93b`, `f02c83c`).
- The CI build now builds the UI first and also runs the app crate's tests (`0cb68c1`).

### Fixed

- Remembered hidden names now show their dotted underline (`a13db4a`).
- Player Details never shows the previous player's name or a stale answer: it clears when you leave it, a newer update beats an older answer still on its way, and the loading hero is the same height as the loaded one (`f09fcfc`).
- Long and wrapped names in Player Details keep the eye icon and the underline with their first line (`f09fcfc`).
- The own-history backfill never runs during a match, free-for-all included: it waits for the Lobby or a closed game and picks up where it stopped (`bab8ca7`, `f9a962a`).
- The backfill no longer skips matches after a network or rate-limit error: it stops and retries, and only a match that fails on its own three runs in a row is skipped. Its anchor and failure counts are kept per account (`bab8ca7`, `f9a962a`).
- Each match's details are downloaded once at a time even when several parts of the app ask for it, a party change in the middle of a match refreshes the stripes, and an open Player Details fills its skeletons once the player's data arrives (`f9a962a`).
- A late answer for the user's own loadout is never stored for another account, and Riot request errors in the log no longer carry player ids (`f9a962a`).
- Windows: the UI tests no longer fail on `/D:/...` file paths on the Windows build machine (`f09fcfc`).
- Riot requests time out instead of hanging, fields Riot leaves null no longer break parsing, and secrets are redacted from logs (`78b47d7`).
- Display names, match info and the image cache are more robust: non-competitive matches keep their map, and image writes are safe (`f745ee9`).
- Free-for-all lobbies, live updates arriving out of order, the user's own name, and reconnecting after the Riot client restarts (`2a16055`).
- My Stats: placement games no longer skew the average RR, act membership is consistent, and edge cases in session, streak and insight math are handled (`c9ad7f4`).
- A failed match-history call no longer feeds party stripes (`9933e19`).
- A match without a stats row shows empty kills, deaths and assists instead of 0 (`b2d93f6`, `6ddbb00`).
- After an upgrade from an older asset cache, only the small edition-badge table is downloaded again (`6ddbb00`).
- The backfill follows Riot's history total for where the history ends, and a short page leaves no gap (`22ff8d1`).
