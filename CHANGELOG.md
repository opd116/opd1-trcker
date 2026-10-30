# Changelog

All notable changes to OPD1 TRCKER are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- A new look for the whole app (the v2 design): a slim title bar with a connection light and a side menu with Live, Saved, History, Comp, Skins, ASCII, Settings and About.
- Live: team tables with rank, win rate, K/D, HS%, streaks, parties, past encounters and a smurf warning; an Agent Select timer; a waiting screen with your last match, today's session and rank; a lost-connection banner and an error screen with Retry; a match recap after each game; a narrow Glance mode that stays on top.
- A player profile that slides in from the side, with your private note on any player and a Save button.
- Saved players, Match History with a full scoreboard, your Competitive stats, your skins collection with its estimated VP value, an ASCII art studio, Settings and About.
- Settings: highlight standout stats, enemy team first, smurf warning level, a Windows notification (and optional sound) when a saved player is in your match, keep on top, launch with Windows, close to tray.

### Changed

- The window is resizable now (1280×800 by default, at least 1180×760), with its own title bar.

### Fixed

- Fast single clicks on Back, navigation, profile Close and scoreboard controls now work immediately after opening a page; keyboard activation also works. The 400 ms guard only suppresses repeated mouse clicks or a second click at the opening position, so double-clicking a player still cannot click through the new drawer.

- Reinstalling into a folder you picked by hand (say `D:\Games`) no longer deletes everything else in that folder; the installer also asks to close a running copy before it removes anything.
- A rank Riot sends without its RR now shows (with the RR from your stored matches when they have it) instead of `—`.
- A player who changed their Riot ID shows the new name after the next match, not the old one until the app restarts.
- Switching Riot accounts in a Riot Client that stays open reconnects for the new account.
- A Riot Client that hangs while renewing its login no longer stalls the app for about a minute.
- One unreadable character in the game's log no longer stops the app from finding your region.
- If the app cannot start (its database cannot open), it leaves its log in `%APPDATA%\com.opd1.trcker\startup-error.log`.
- "12mo" reads "1y".
- A player you saved from a History scoreboard or a recap opens their profile, instead of an empty one.
- A player who now hides their level shows `Lv —` in their profile, not their old level.
- A Riot Client that is signed out is no longer taken for another account (which cleared your data), and signing back in is picked up within seconds.
- Launch VALORANT finds the Riot Client when Windows is not on drive C.
- The catch-up of your past matches waits out a Riot rate limit and goes on, instead of stopping until the next match.
- Your party members no longer get a party stripe and ⟲ badge in Agent Select when the first party lookup failed.
- A Deathmatch shows its recap also when its queue was unknown or the game closed at its end.
- Skins keeps working when the disk refuses to save the cache, keeps each account's collection, and says when the collection loads.
- A damaged cache file (after a power cut, say) is downloaded again.
- The local connection to the Riot Client never goes through a proxy set on the PC.
- Open History and Saved pages show the match that just ended; Back from a scoreboard keeps Saved's scroll and the keyboard focus.
- Cancelling a new save keeps the note the player already had; saves, notes and dismissals that fail are put back, and quick ones are stored in order.
- The scoreboard's YOUR RR shows the rank after the match.
- A crash in the profile drawer or in Glance no longer leaves an empty half screen or a Glance window with no title bar.
- The Skins screen no longer claims the collection was never fetched while VALORANT is running: one failed Riot error is retried on the screen's next one-minute retry instead of hiding the collection for ten minutes.
- A player's name no longer drops back to the loading skeleton after a dodge or a match end when the app already has it stored; a stage reset now re-reads the stored player row for the cards that stay on screen.
- Switching Riot accounts while the app is open no longer leaves the previous account's data on the History, Skins and Saved screens; an open profile or scoreboard closes instead of showing the old user's numbers.

### Security

- The debug dump (Ctrl+Shift+D) hides the game server's address and your match join key.
- The TLS library is updated for six published advisories (rustls, rustls-webpki, webpki-root-certs).

## [0.3.1] - 2026-10-04

Filled from `git log` (`493d4f9..da61be2`). The five features picked for v0.4.0 and the act-change work are not in this release (see `docs/next-session-prompt.md`).

### Added

- Each chip of your current Session on My Stats shows the map, the RR change and how long ago on hover, and clicking it opens that match (`f974b75`).
- The RR chart and the Session chips work from the keyboard: Tab to the chart, then the arrow keys move along it; Tab to a chip shows its tooltip and Enter opens the match (`1be4fcc`).
- If a page crashes or something fails in the window, a line about it goes into the app's log, so Ctrl+Shift+L and Ctrl+Shift+D now show it (`598b5d0`, `6a03275`).
- The copied log and the debug dump start with one line naming the app version, Windows version, stage, server region, game client version and current act (`6a03275`).
- Releases: each version is published on the repo's GitHub Releases page with its installer, named `OPD1-TRCKER-<version>-setup.exe` (`cb48d22`).

### Changed

- When Riot asks the app to slow down, that pause now also holds after a reconnect or an internal restart, so the app never asks again too early (`7093684`).
- Player cards and Player Details read faster from a large history: a stranger's encounter lookup went from about 8 ms to 0.1 ms (`fa435a3`).
- Map pictures are stored smaller (two maps' pictures were 3840×2160), which saves memory with no visible change (`73d1db0`).
- Every change is now checked automatically (format, lint, all tests and screenshot comparisons) before it reaches `main` (`40aaa0d`, `5742ba5`, `cb48d22`).

### Fixed

- My Stats can no longer show the previous account's numbers for a moment after switching accounts (`7093684`).
- My Stats pictures retry after a failed load, like the other pages (`1be4fcc`).
- Two tests that failed at random under load (`5ba6f2f`, `80f10f9`).

## [0.3.0] - 2026-10-03

Filled from `git log` (`a1e3315..44ae56d`, then the v0.3.0 round `44ae56d..6ebdf5c`).

### Added

- **My Stats page**: your rank with RR, W / L, the RR left to the next tier, net RR, average win and loss and recent rank changes; the RR chart for the act; the current session; your last 5 competitive matches (click one for Match Details, Back keeps your place); the Maps and Agents tables; win rate by weekday and time of day; and automatic insights. The `My Stats` tab works, and with VALORANT closed the app opens on it (`0809ee9`, `f118477`, `d960872`, `efa8a7f`, `861ac67`, `0562ebb`).
- My Stats updates by itself: after each of your matches, as your history catches up, and every 15 minutes so an old session drops off (`369df2c`, `bee2ade`, `64e6b4a`, `6ebdf5c`).
- Your competitive updates for the whole act are fetched quietly in the background, so the RR chart, net RR and rank changes are complete (`07bfec1`, `5bac8f1`).
- A hidden player's name shows in the remembered style when the app knows it from any source, and the tooltip says where it came from: your match, a past match, or seen before (`d32e6d6`, `e0d9ca6`).
- Names after a match: once a match ends, the app asks Riot once for the names of that match's other players, including the ones who were hidden during it, and remembers them. Player Details and Match Details ask for the names they lack, and an open Match Details updates when they arrive. Nothing is asked during Agent Select or In Game (`d1a7976`, `d3a2e7a`, `461cf71`, `69e64bd`, `82c4c1a`).
- The rank never stays empty when Riot's rank request fails: it comes from your stored competitive updates (exact RR), then from the tier Riot sends with the pregame or party answer, then from a stored match (RR shows `—`). My Stats' rank card falls back the same way (`caaf8fb`, `ca994ec`, `95bac32`, `a8c8b63`).
- The current act comes from Riot's own content service, with the valorant-api dates only as a fallback, so the act change no longer shows `—` for a few hours (`78b6a57`, `1542508`).
- The debug dump (Ctrl+Shift+D) also writes the log and the error body of each failed call (`58b6d43`).

### Changed

- Back from a Match restores the scroll position of Player Details too (`0562ebb`).
- The build also runs the whole UI layout, tooltip and navigation test suite on Linux, beside the Windows installer (`2437efe`, `2581e07`).
- Requests are sent the way Valorant-Scout sends them (client version and User-Agent), and Riot's "slow down" answers are honoured in full, per kind of request, so the rank requests never hold up the rest (`e496ee3`, `1886649`, `6a6b644`, `bc2f1ef`).
- Cards fill faster: the whole roster's ranks load first, then each player's matches; In Game shows both teams before the loadouts arrive; the roster is retried every 2 s while the game is still loading it; the rank after a match refreshes within seconds (`2d75653`, `f3cd55a`, `f09586f`).
- Peak is never below the current rank (`f3cd55a`).
- Encounters count competitive matches only (`e3a0436`).
- Riot answers are read tolerantly: a missing field no longer fails a whole roster, history or rank answer. The sign-in token is renewed before it expires and after Riot rejects it (`6053d3a`, `b9829ea`).

### Fixed

- If the background loop that follows the game ever crashes, it restarts by itself within seconds instead of leaving old cards on screen until the app is reopened (`2674799`, `686386d`, `6ebdf5c`).
- One failed internal step can no longer make every later Riot request fail (`f27f468`, `e5eac55`).
- Player Details tries again after a match download fails or Riot asks it to slow down, instead of waiting until you reopen it (`03f970e`, `d02a94d`).
- No name lookup is sent in the moment a match starts, and the names of players on screen never wait for it (`7d8c689`, `5e4090c`).
- A failed rank or names request, or a half-finished match download, is retried a few times instead of leaving a card without a name or with partial stats for the whole stage (`108dae0`, `2d75653`, `cff0f1e`, `ca763db`, `cbe9736`, `1f43bf4`).
- A remembered name reaches the card on screen at once, and a mid-match burst of strangers' matches can no longer crowd out the names of the match that just ended (`4726852`, `8ab2e55`, `cabaab3`, `221cef5`).
- A match where neither team won is stored as a draw instead of failing (`d7588a7`).
- Player cards keep their art across updates, and closing a page while it loads no longer reopens it (`a6f6dfa`, `9b3285c`).
- Asset tables on disk are kept when a download fails right after a game patch (`5c98d89`).
- Act labels never go blank, and the act's episode is picked by date (`9c79d4c`, `fd22774`, `9b4b318`).
- The live tier from the roster answer is tied to its match and cleared on every stage change (`5bf5c1d`).

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
