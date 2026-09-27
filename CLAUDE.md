# OPD1 TRCKER

A lightweight Windows desktop tracker for VALORANT that reads the **local Riot client API** (same approach as kryotrades/Valorant-Scout) and shows live Lobby / Agent Select / In Game player cards plus the user's own deep competitive stats.

The name is spelled `OPD1 TRCKER` on purpose. Never "correct" it to TRACKER.

- User-facing spec, in Arabic, the source of truth: `docs/requirements-raw.md`
- Reference screenshots (content only, never copy their styling): `docs/refs/`
- Asset lookup table: `docs/refs/valorant-api-assets.md`
- Implementation plan: `docs/plan.md`

## Working with the user

- The user doesn't code. Talk to them in Arabic (Gulf dialect is fine), short and plain, no jargon walls. Use `/archify` diagrams when a picture explains better.
- The user tests on their Windows PC with VALORANT. Claude works on a Linux VPS with no game client. Anything touching the real Riot client is verified by the user. Ask for their result or the in-app "copy log" output.
- Before a task a skill could help with, search for the best-in-class skill (including community skills via `npx skills find`) and load it.
- Use `graft` for codebase context once code exists (`npx @nanonets/graft init --agents claude`).
- Before context gets long, or at the end of a phase, ask the user to run `/handoff` (it's user-invoked only) so nothing important is lost. Keep `docs/requirements-raw.md` updated with every new decision as it's made.
- Design decisions not covered by the spec are Claude's call. Don't ask the user about typography or spacing. Show them the result.

## Stack (decided, don't revisit without the user)

- **Tauri 2** shell, using the system WebView2.
- **Rust core:** all Riot + valorant-api HTTP, polling, caching, and the SQLite DB.
- **Svelte 5 + Vite** SPA (no SSR, no SvelteKit server). Plain CSS custom properties for tokens. Tailwind v4 allowed. No runtime CSS-in-JS.
- **No Python, no Electron, no React, no Framer Motion.** Use Svelte transitions + CSS for motion.
- **Rust → UI data flow:** Tauri events (push). The UI never polls Riot.
- **SQLite** (`rusqlite`, bundled), stored in the app data dir.
- Lightness is priority #1 and the tie-breaker for every decision (RAM, CPU, bundle size, request count). Elegant design is #2.

## Build and distribution

- **GitHub:** private repo only. No public repo, no auto-updater.
- **GitHub Actions** (`windows-latest`) builds the NSIS `Setup.exe` as a workflow artifact. The user downloads it and sends it to friends manually.
- **The installer must remove the previous version's program files, settings, and image cache.** It must NOT delete the SQLite DB. DB schema changes go through versioned, automatic migrations (`PRAGMA user_version`).
- **Unsigned build.** Friends click through SmartScreen "More info → Run anyway". That's accepted.

## Window

- Fixed **1000×800**. Not resizable, no maximize, always on top.
- Close button quits the app. No tray. No autostart with Windows.

## Screens

**Tabs:** `Live` and `My Stats`. The app auto-switches to `Live` when a match is found (Agent Select).

**When VALORANT is closed:**
- The app opens on `My Stats` with the last saved data.
- `Live` shows only the user's own card.
- Everything refreshes silently when the game opens.

**Live stages** (the player card is identical in all three; only who's shown and the layout change):
- **Lobby:** the user's party, 1–5 players. Layout and card size adapt to the count. Show Average Party Rank when there are 2 or more players. Queueing changes nothing: no timer, no searching indicator.
- **Agent Select:** the user's 5 teammates only. **No scrolling.**
- **In Game:** my team as 5 stacked cards on the left, enemy team as 5 stacked cards on the right, map image + name above (sketch: `docs/refs/ingame-layout-sketch.png`). Enemy ranks and stats are shown. **No scrolling.** No live match stats or round score.
- **Match end:** return straight to Lobby. No post-match screen in v1.

**Player card fields:**
- Player Card image
- Riot ID with the Tag visually distinct
- Account Level
- Current Rank icon + exact RR
- Previous Act Rank
- Peak Rank icon + the act it was reached in (short label like `E8 A3 · 1y`), where the time is since that act ended, not the day the peak was hit. For the current act, say "This Act". Hover shows the full act name and dates.
- K/D and HS% from the last 5 Competitive matches
- Current-act Win Rate
- Last 5 Competitive results, newest first, each showing W/L, RR change, and time since
- Agent **head icon** (`displayIcon`) only after the agent is selected. Never show a placeholder agent.

**Player details** (click any card in any stage; **scrolling allowed**):
- **Back** button.
- **Recent Competitive Matches:** 3 shown, each with map image and result. **Show More** reveals more, up to 8 total, newest first. No count picker.
- **Top 3 Teammates** for the current act, with full names and shared match count. Computed from the last 20 current-act comp matches plus any older matches already in the DB.
- **Loadout:** Vandal, Phantom, Operator, and Sheriff skins, in the equipped chroma.
- Encounter history with the user (see Smart data).

**My Stats** (the user's own account; **scrolling allowed**; period is the current act):
1. Rank card: tier, RR, W–L, RR to next tier, net RR, average win/loss RR, recent rank changes
2. RR progression chart
3. Session: net RR and W/L streak. A new session starts after a 6h gap with no comp match
4. Last 5 comp matches: map, result + score, agent, K/D/A, K/D, ACS, HS%, RR change
5. Maps table
6. Agents table
7. Win rate by weekday and by time of day
8. Auto insights (best/worst map, time of day, weekday vs weekend, RR economy, last 7 days)

## Design rules

- English UI only. Dark, calm, low-glare palette with one accent. No bright white surfaces, and body text is off-white.
- **Never shrink fonts to make things fit.** Minimum 12px. Key numbers are larger. Use tabular numerals for stats.
- **No text may overlap or crowd numbers.** Every label/value pair gets its own space and a consistent alignment.
- **No status text or badges anywhere:** no "Offline", "Last updated", "BETA", "Saved data", "refresh unavailable".
- Loading uses neutral skeleton shapes with no text. Missing values show `—`.
- Put extra detail behind hover tooltips instead of adding visible text.
- The `docs/refs/` screenshots are the *content* reference only. The user dislikes their fonts and number/label arrangement.
- **Verify every screen before showing the user.** Render it at exactly 1000×800 in a headless browser, screenshot it, and check for overlap, clipping, and scrollbars on the no-scroll stages.

## Hidden names (incognito)

- If a player's identity is hidden, hide **only** the Riot ID + Tag. Every other field stays visible.
- **Before agent select:** show `Hidden Player`.
- **After agent select:** show the agent name, with no tag.
- **Exception:** if the DB already knows that player's name from a finished match **the user personally played in** (where the name was shown after the match), show that name during live stages in a distinct color/style.
- **Never request a hidden player's name** from any endpoint (name-service or otherwise). Never take names from other players' match histories.
- After a match ends, show names as the client/match-details shows them.

## Smart data (SQLite)

- **Players are keyed by PUUID**, so they're recognized even after a name change.
- **Match details are immutable:** fetch once, store forever, never refetch. Store extracted compact rows (~1–2KB per match), not raw JSON.
- **Sync, don't refetch:** for a known player, fetch the history list (one cheap call), diff it against stored match IDs, and fetch only the new matches. No new comp match means rank is unchanged, so skip the MMR call.
- **Assets** (valorant-api JSON + images) are cached on disk after first download.
- **Retention:** keep everything. The DB is the app's memory and survives reinstalls.

**Party detection** (automatic, both teams, Agent Select + In Game):
- Look at each player's last 2 matches in **any mode, including Deathmatch**.
- Two players whose **most recent** match is the same match on the same team are a **likely party**. In Deathmatch there are no teams, so "same match" is enough.
- They're a **confirmed party** if both of their last 2 matches are shared, **or** if match-details gives both the same `partyId` in their shared most recent match. With a matching `partyId`, one shared game is enough.
- Link transitively: A–B plus B–C means A–B–C.
- Display is a per-group color stripe on the card: dashed for likely, solid for confirmed. No text.
- The user's own party comes directly from the client.

**Encounters:**
- Count every past match shared with the user, with or against.
- The card shows a small ⟲ icon with the count. Hover shows `N with · M against`.
- The details view lists the shared matches: map, date, with/against, result.
- **The user and their own party members get no party stripe and no encounter badge.**

## Riot local API essentials

- **Lockfile:** `%LOCALAPPDATA%\Riot Games\Riot Client\Config\lockfile`, format `name:pid:port:password:protocol`. Local calls use `https://127.0.0.1:{port}` with Basic auth `riot:{password}` and a self-signed cert.
- **Tokens:** `/entitlements/v1/token` gives the access token + entitlements JWT.
- **Remote headers:**
  - `Authorization: Bearer`
  - `X-Riot-Entitlements-JWT`
  - `X-Riot-ClientPlatform`
  - `X-Riot-ClientVersion`
- **Region/shard:** parse the `pd.` and `glz-` URLs from `%LOCALAPPDATA%\VALORANT\Saved\Logs\ShooterGame.log`.
- **Stage:** comes from the `/chat/v4/presences` private blob (`sessionLoopState`), confirmed by the `pregame/v1/players/{puuid}` and `core-game/v1/players/{puuid}` endpoints.
- **Endpoints:**
  - `pregame`, `core-game` (+ loadouts)
  - `mmr/v1/players/{puuid}` (+ `/competitiveupdates`)
  - `match-history`, `match-details`
  - `name-service/v2/players` (never for hidden players)
  - `parties`
- **Throttle** every remote call with a token bucket and honor `429 Retry-After`. Reference implementation: Scout's `backend/riot_client.py`.

## Hard limits

- No instalock, no agent-select automation, no dodge, no writes to the Riot client beyond what reading requires.
- **Demo/fake-data mode is a temporary dev tool only.** It covers Lobby / Queue / Agent Select / In Game, lives behind a dev-only flag (`import.meta.env.DEV` + a Rust `demo` cargo feature), must not affect layout, and is compiled out of release builds. Never design around it.
