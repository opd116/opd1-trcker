# Changelog

All notable changes to OPD1 TRCKER are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

Backfilled from `git log` (`6e9655f..c9ad7f4`). The release section is filled from `git log` at release time.

### Added

- Tauri 2 + Svelte 5 app shell (fixed 1000×800 window, always on top) and a per-user NSIS installer built by GitHub Actions on Windows (`61b5989`).
- Reading of the local Riot client: lockfile, region and shard from the VALORANT log, and the game stage from presence (`ab47bd3`).
- Throttled Riot API client with typed endpoints and automatic re-authentication (`7922033`).
- Local SQLite memory that keeps every player and match the app has seen, with K/D, HS%, win rate and peak-rank math (`209ade9`).
- Player cards: rank and RR, previous act and peak with the act it was reached in, last 5 competitive results as win / loss / draw with the RR change and start time kept for the hover tip, and images cached on disk (`2f8583d`).
- Live pipeline: the app follows Lobby, Agent Select and In Game, fills cards from the local memory first, then syncs only what is new (`ca0b5fe`).
- Hidden-name rules (incognito players show as `Hidden Player`, then as their agent), party detection from recent matches, and encounter counts with the user (`37b9631`).
- My Stats aggregation for the user's own account (backend only, no page yet): rank card, sessions, maps, agents, weekday and time-of-day win rates, and insights (`e9f9acd`).

### Fixed

- Riot requests time out instead of hanging, fields Riot leaves null no longer break parsing, and secrets are redacted from logs (`78b47d7`).
- Display names, match info and the image cache are more robust: non-competitive matches keep their map, and image writes are safe (`f745ee9`).
- Free-for-all lobbies, live updates arriving out of order, the user's own name, and reconnecting after the Riot client restarts (`2a16055`).
- My Stats: placement games no longer skew the average RR, act membership is consistent, and edge cases in session, streak and insight math are handled (`c9ad7f4`).
