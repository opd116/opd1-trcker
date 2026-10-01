# OPD1 TRCKER

A lightweight Windows desktop tracker for VALORANT that reads the **local Riot client API** (same approach as kryotrades/Valorant-Scout) and shows live Lobby / Agent Select / In Game player cards plus the user's own deep competitive stats.

The name is spelled `OPD1 TRCKER` on purpose. Never "correct" it to TRACKER.

- User-facing spec, in Arabic, the source of truth: `docs/requirements-raw.md`
- Reference screenshots (content only, never copy their styling): `docs/refs/`
- Asset lookup table: `docs/refs/valorant-api-assets.md`
- Implementation plan: `docs/plan.md`
- Visual design (source of truth): the user's Claude Design file `.superpowers/design/cd-v8/project/OPD1 Tracker v8.dc.html` + `support.js` (project https://claude.ai/design/p/f22fa2f6-6e0d-4aad-ade4-7dda498b5c0a; original zip in the repo root). Rebuild it in Svelte; match its layout, sizes, colours and tooltips. The HTML/JS is the truth; its screens/*.png may be stale. Earlier rounds (local v1–v8, ارشيف/, the Lovable zip) are history only.
- Precedence: a newer dated user decision (requirements-raw.md or the design file) beats an older rule here; update this file in the same change. The design never overrides Window, Hidden names, Smart data, Hard limits or "No status text". Take any other clash to the user.
- Build spec: `docs/design-spec.md` (sizes, states, tokens and tests derived from the design; its [C]/[bound] items are Claude calls R74–R86 in `docs/DECISIONS.md`). Keep it in step with the code. It outranks the design file only for those [C]/[bound] gap-fills; if it disagrees with the design on something the design covers, the design wins and the spec is fixed, and any change to a visual the design covers goes to the user first (R92).

## Working with the user

- The user doesn't code. Talk to them in Arabic (Gulf dialect is fine), short and plain, no jargon walls. Use `/archify` diagrams when a picture explains better.
- The user tests on their Windows PC with VALORANT. Claude works on a Linux VPS with no game client. Anything touching the real Riot client is verified by the user. Ask for their result or the in-app "copy log" output (Ctrl+Shift+L). Ctrl+Shift+D writes the redacted raw answers to a folder, now with `log.txt` and the error bodies (`<endpoint>.error.json`) of failed calls (R158).
- Before a task a skill could help with, search for the best-in-class skill (including community skills via `npx skills find`) and load it.
- Use `graft` for codebase context once code exists (`npx @nanonets/graft init --agents claude`).
- Before context gets long, or at the end of a phase, ask the user to run `/handoff` (it's user-invoked only) so nothing important is lost. Keep `docs/requirements-raw.md` updated with every new decision as it's made.
- Anything the design file doesn't cover (Agent Select states, Lobby, Closed, Hidden Player, edge states) is Claude's call, built only from the design file's own parts and tokens. Don't ask the user about typography or spacing. Show them the result.
- Every step is committed with a clear conventional message and pushed to the private GitHub repo right away (2026-09-29). Commit explicit paths only (`git commit -m "…" -- <paths>`); never commit `.superpowers/` or root `*.zip`.
- Repo files: README.md, CHANGELOG.md (Keep a Changelog; the release section is filled at release from `git log`), docs/ARCHITECTURE.md, docs/DECISIONS.md, docs/design-spec.md, AGENTS.md, LICENSE (proprietary), THIRD-PARTY-NOTICES.md, .editorconfig, .gitattributes.

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
- DB schema: every change is a new versioned migration appended to `MIGRATIONS` (`PRAGMA user_version`); existing migrations are frozen, migration 1 included (R89, 2026-09-29). No self-heal code, no in-place edits.
- The uninstaller's optional "Delete app data" checkbox (off by default) removes everything, the DB included — accepted as the user's explicit choice (2026-09-29). A normal reinstall never touches the DB.
- **Unsigned build.** Friends click through SmartScreen "More info → Run anyway". That's accepted.

## Window

- Fixed **1000×800**. Not resizable, no maximize, always on top.
- Close button quits the app. No tray. No autostart with Windows.

## Screens

**Tabs:** `Live` and `My Stats`. The app auto-switches to `Live` when a match is found (Agent Select).

**When VALORANT is closed:**
- The app opens on `My Stats` with the last saved data. (Until My Stats is built, open on Live with the user's own card — R71.)
- `Live` shows only the user's own card.
- Everything refreshes silently when the game opens.

**Live stages** — one page in the In Game layout of the design file; the card is identical everywhere. **No scrolling** in any Live stage.
- **Lobby:** me + my party (1–5) stacked in the left column (same card size as In Game; the rest of the column stays empty — R75). Average Party Rank when 2+ players (placed in the top bar where the map sits in game). Queueing changes nothing: no timer, no searching indicator.
- **Agent Select:** my team (5) in the left column; the right column shows 5 empty card-shaped slots with a light dashed frame and no text. Map name + image in the top bar as soon as pregame knows the map.
- **In Game:** my team left, enemy team right (ranks and stats shown). Map name + image at the right end of the top bar. No live match stats or round score.
- **Deathmatch / FFA:** keep the Lobby (party) view; don't fetch or show the other players. Party detection still uses DM matches from history.
- **Match end:** return straight to Lobby. No automatic post-match screen.

**Player card fields:**
- Player Card image
- Riot ID with the Tag visually distinct
- Long names: only the name part truncates with `…`; the tag is never cut; the ⟲ badge has a fixed reserved slot; hovering a truncated name shows the full `Name#TAG` (tooltip only when truncated).
- Account Level
- Current Rank: icon + tier name + exact RR. When Riot's MMR request gives no answer, the rank comes from the player's stored competitive updates (exact RR) or, failing that, from the tier of a stored current-act match (RR shows `—`) (R154, user 2026-09-30).
- Previous Act Rank: icon + tier name (`—` if none)
- Peak Rank: icon + tier name + the act it was reached in (`V26 A4 · 1mo`, `E9 A3 · 1y`); time is since that act ended, not the day the peak was hit. Current act → `This Act`. Hover shows the full act name and dates (the design's tooltip shows dates only — add the act name). If the peak tier was reached in several acts, show the most recent (R7).
- K/D and HS% from the last 5 Competitive matches
- Current-act Win Rate
- Last 5 Competitive results as W / L / D markers only, oldest → newest left to right. The newest (rightmost) is emphasised as in the design (filled chip + arrow under the row; "Underline" option = underline instead). With fewer than 5 matches, empty dashed slots sit on the left. RR change and time since are on hover.
- Agent **head icon** (`displayIcon`) only after the agent is locked (hovering doesn't count — R99). Never show a placeholder agent.

**Card indicators** (one meaning, one look, each explained by a tooltip; no visible legend):
- Outline around the card = the user's own card.
- Coloured right-edge stripe + party tab = detected party (dashed/outlined = likely, solid/filled = confirmed). Tooltip `Party · likely` / `Party · confirmed`.
- ⟲ + count = past competitive matches with the user (see Encounters).
- Eye-off icon + dotted-underlined name + tag = hidden player's name remembered by the DB, from any source (R145). The tooltip names the source: `Hidden name · known from your match` or `Hidden name · known from a past match`, each + map · date; `Hidden name · seen before` + date only.
- Eye-off icon + italic agent name = hidden name. Tooltip `Hidden name`.

**Player details** (click any card in any stage; **scrolling allowed**):
- **Back** button.
- **Recent Competitive Matches:** 3 shown, each with map image and result. **Show More** reveals more, up to 8 total, newest first. No count picker.
- **Top 3 Teammates** for the current act, with full names and shared match count. Computed from the last 20 current-act comp matches plus any older matches already in the DB.
- **Loadout:** Vandal, Phantom, Operator, and Sheriff skins, in the equipped chroma. The user's own loadout comes from personalization v3 (`/personalization/v3/players/{puuid}/playerloadout`; v2 returns 404 on every shard, R148).
- Encounter history with the user (see Smart data).
- Hero stats: Win%, Games, W / L for the current act; K/D and HS% from the last ≤20 current-act competitive matches (tooltip `Last N competitive · this act`) (R116, user 2026-09-29).
- Clicking a recent match or an encounter row opens **Match Details** (the design's scoreboard: ACS, K/D/A, +/−, ADR, HS%, half scores, rank per player). Hidden-name rules apply inside it (see Hidden names).

**My Stats** (the user's own account; **scrolling allowed**; period is the current act):
1. Rank card: tier, RR, W–L, RR to next tier, net RR, average win/loss RR, recent rank changes
2. RR progression chart
3. Session: net RR and W/L streak. A new session starts after a 6h gap with no comp match
4. Last 5 comp matches: map, result + score, agent, K/D/A, K/D, ACS, HS%, RR change
5. Maps table
6. Agents table
7. Win rate by weekday and by time of day
8. Auto insights (best/worst map, time of day, weekday vs weekend, RR economy, last 7 days)

**Later (build only when the user asks):** Match History page, Settings page. Tabs stay `Live` / `My Stats` until then.

## Design rules

- English UI only. Dark, calm, low-glare palette with one accent. No bright white surfaces, and body text is off-white.
- Follow the design file's type scale (12–28px; tabular numerals for stats). Never below 12px, never shrink fonts to fit. Card size follows the design; only the 1000×800 window is fixed.
- **No text may overlap or crowd numbers.** Every label/value pair gets its own space and a consistent alignment.
- **No status text or badges anywhere:** no "Offline", "Last updated", "BETA", "Saved data", "refresh unavailable".
- Loading uses neutral skeleton shapes with no text. Missing values show `—`.
- Put extra detail behind hover tooltips instead of adding visible text.
- Stat scope is not printed on screen (no "Last 5 comp" / "This Act" headings). Each stat's tooltip names its scope: K/D and HS% → "Last 5 competitive"; Win% → "This act · W · L · games"; last 5 → per-match tooltip. The Peak label `This Act` stays.
- The `docs/refs/` screenshots are the *content* reference only. The user dislikes their fonts and number/label arrangement.
- **Verify every screen before showing the user.** Render it at exactly 1000×800 in a headless browser, screenshot it, and check for overlap, clipping, and scrollbars on the no-scroll stages.
- Fonts are bundled locally (latin-subset woff2, only weights used). No runtime requests to Google Fonts or any CDN; images go through the Rust img cache (CSP stays strict).
- The app ships the design file's options at their defaults (tone Neutral, accent #e3a54f, results Green / Red, fill Tinted, font Barlow, last-5 Arrow). Switching them belongs to the later Settings page.

## Hidden names (incognito)

- If a player's identity is hidden, hide **only** the Riot ID + Tag. Every other field stays visible.
- Until the player locks an agent (only possible in Agent Select): `Hidden Player` in the hidden-name style, empty agent slot (never a placeholder agent).
- Once the player locks an agent (hovering doesn't count — R99, user 2026-09-29): the agent name, no tag.
- The user's own card always shows the user's own name (from the local chat session, never name-service) (R43/R49).
- These rules apply everywhere a live player appears, including Player Details (Top 3 Teammates, encounter rows) and Match Details (R18).
- Account level shows `—` only when Riot itself withholds it (HideAccountLevel) — a separate player setting, not part of the hidden name — except on the user's own card, which always shows the user's own level (R100).
- **Exception:** if the DB already knows that player's name, show it during live stages in the remembered-name style (Card indicators). Which sources count is in the R145 paragraph below.
- **Never request a hidden player's name** from any endpoint (name-service or otherwise) while that player is incognito in a live stage (R150 below).
- **Names from any source (R145, user 2026-09-30) and name-service (R150):** During live stages, a hidden player's name shows in the remembered style when the DB knows it from any source: the user's own matches, other matches, or a name-service answer from when the player was visible. The tooltip names the source. Outside live stages, name-service may be asked for match participants with no known name. Riot blanks anyone who currently hides their name. A live-incognito player's **name** is never requested (name-service is never called for them during a live stage); their rank, stats and history are fetched like anyone's ("every other field stays visible").
- After a match ends, show names as the client/match-details shows them. The game's own Match History shows every player's real name after the match, so the app asks name-service once for that match's other players, including the ones hidden during it (R151–R153, user 2026-09-30), stores the names by PUUID, and shows them later in the remembered style. Player Details / Match Details ask for the names they lack. Asks are only sent between matches: nothing is asked while the stage is Agent Select or In Game (a Deathmatch counts as a match too).
- Match Details: in every view, a player incognito on the live screen now shows as on their card: the remembered name when the DB knows it from any source (R145), else the agent name (R93). Otherwise, a match the user played shows names as match-details gives them, and so does a match the user did not play when Riot returns them (R143); a player with no stored name shows as the agent name (or the remembered name). A row Riot left blank shows the name stored in the players table, else the agent name (R145 supersedes R74's rule that only the user's own matches supply known names; match-details has no incognito flag).

## Smart data (SQLite)

- **Players are keyed by PUUID**, so they're recognized even after a name change.
- **Match details are immutable:** fetch once, store forever, never refetch. Store extracted compact rows (~1–2KB per match), not raw JSON.
- **Sync, don't refetch:** for a known player, fetch the history list (one cheap call), diff it against stored match IDs, and fetch only the new matches. No new comp match means rank is unchanged, so skip the MMR call.
- **Own history backfill (R101, user 2026-09-29):** on connect and after each match, page the user's own match history (all queues) back to the last match a finished backfill confirmed, and fetch match-details only for unknown ids. Throttled, lowest priority, behind live cards. The first run stops at the current act start or 200 matches (Claude's lightness bound, not the user's).
- **Assets** (valorant-api JSON + images) are cached on disk after first download.
- **Retention:** keep everything. The DB is the app's memory and survives reinstalls.

**Party detection** (automatic, both teams, Agent Select + In Game):
- Look at each player's last 2 matches in **any mode, including Deathmatch**.
- Two players whose **most recent** match is the same match on the same team are a **likely party**. In Deathmatch there are no teams, so "same match" is enough.
- They're a **confirmed party** if both of their last 2 matches are shared, **or** if match-details gives both the same `partyId` in their shared most recent match. With a matching `partyId`, one shared game is enough.
- Link transitively: A–B plus B–C means A–B–C.
- Display is a per-group color stripe on the card: dashed for likely, solid for confirmed. No text.
- The user's own party comes directly from the client.
- Players on opposite teams in the current match are never linked.
- Marks come only from history synced in this session; a stripe may appear a few seconds after the card.
- Each group has its own colour across the screen. The design has 2 party colours; extend to 4 per accent (max real groups: 2 ally + 2 enemy, since the user's own party is excluded), distinct from W/L colours and the accent.

**Encounters:**
- Count every past **competitive** match shared with the user, with or against (user 2026-09-30, R156). Deathmatch and other queues do not count; party detection still uses any mode.
- The card shows ⟲ + count only when the count is above 0. Hover: `N with · M against`.
- Player Details lists shared matches as the design shows them (map, date, side, both players' agent + rank), with the result from the user's side under the header `Your result`. Clicking a row opens Match Details.
- **The user and their own party members get no party stripe and no encounter badge.**

## Riot local API essentials

- **Lockfile:** `%LOCALAPPDATA%\Riot Games\Riot Client\Config\lockfile`, format `name:pid:port:password:protocol`. Local calls use `https://127.0.0.1:{port}` with Basic auth `riot:{password}` and a self-signed cert.
- **Tokens:** `/entitlements/v1/token` gives the access token + entitlements JWT.
- **Remote headers:**
  - `Authorization: Bearer`
  - `X-Riot-Entitlements-JWT`
  - `X-Riot-ClientPlatform`
  - `X-Riot-ClientVersion`
  - `User-Agent` (Scout's value, `ShooterGame/13 Windows/10.0.19043.1.256.64bit`; remote calls only) (R159)
- **Client version:** from the game's own presence first (`partyClientVersion`), then valorant-api, then `ShooterGame.log`, then `release-09.00`, in Scout's order (R159, supersedes R17). Never `/product-session/v1/external-sessions`. Its source is logged once.
- **Region/shard:** parse the `pd.` and `glz-` URLs from `%LOCALAPPDATA%\VALORANT\Saved\Logs\ShooterGame.log`.
- **Stage:** comes from the `/chat/v4/presences` private blob (`sessionLoopState`), confirmed by the `pregame/v1/players/{puuid}` and `core-game/v1/players/{puuid}` endpoints.
- **Endpoints:**
  - `pregame`, `core-game` (+ loadouts; the user's own loadout via `personalization/v3`)
  - `mmr/v1/players/{puuid}` (+ `/competitiveupdates`)
  - `match-history`, `match-details`
  - `name-service/v2/players` (never for a hidden player during a live stage; after the match every participant without a stored name is asked once)
  - `parties`
  - `content-service/v3/content` on `shared.{shard}.a.pvp.net` (the current act: `IsActive` act, previous = `EndTime == StartTime`; valorant-api dates only as the fallback, R162)
- **Throttle** every remote call with a token bucket and honor `429 Retry-After`. Reference implementation: Scout's `backend/riot_client.py`.

## Hard limits

- No instalock, no agent-select automation, no dodge, no writes to the Riot client beyond what reading requires.
- **Demo/fake-data mode is a temporary dev tool only.** It covers Lobby / Queue / Agent Select / In Game, lives behind a dev-only flag, `import.meta.env.DEV` only (no Rust feature, R40), must not affect layout, and is compiled out of release builds. Never design around it.
