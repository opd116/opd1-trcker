# OPD1 TRCKER Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A ~5–10MB Windows installer for a fixed 1000×800 always-on-top VALORANT tracker that reads the local Riot client, remembers every player in a local SQLite DB, and shows Live (Lobby / Agent Select / In Game) cards plus the user's own My Stats page.

**Architecture:**
- **Cargo workspace:**
  - `crates/core`: pure logic with no Tauri dependency, unit-tested on Linux. Covers parsing, models, party detection, stats math, and the DB.
  - `crates/riot`: HTTP to the local client, pd/glz, and valorant-api, with throttling.
  - `src-tauri`: a thin shell that runs the poll loop, emits Tauri events, and exposes commands.
- **Svelte 5 SPA** (`ui/`) renders events. A dev-only demo feed drives the same UI in a plain browser so screens can be screenshot-verified on Linux.
- **GitHub Actions** on `windows-latest` produces the NSIS `Setup.exe`.

**Tech Stack:**
- Rust stable, Tauri 2, `reqwest` (rustls), `rusqlite` (bundled), `tokio`, `serde`, `base64`
- Svelte 5, Vite, TypeScript, Vitest, Playwright (dev-only)
- NSIS via `tauri build`

**Spec:**
- `CLAUDE.md`: the authoritative rules, in English
- `docs/requirements-raw.md`: the user's words, in Arabic
- `docs/refs/`

## Global Constraints

- **App name** is exactly `OPD1 TRCKER`. The bundle identifier is `com.opd1.trcker`.
- **Window:** `width: 1000, height: 800, resizable: false, maximizable: false, alwaysOnTop: true`. Close quits. No tray, no autostart.
- **UI:** English only. Dark, low-glare. Minimum font size 12px. Tabular numerals for stats. No status text or badges (Offline / Last updated / BETA / Saved data). Skeletons for loading, `—` for missing values.
- **No-scroll stages:** Agent Select, In Game. Scroll is allowed in Player Details and My Stats.
- **Agent image** is `displayIcon` from `/v1/agents?isPlayableCharacter=true`. Only show it after the agent is selected.
- **Never** call `name-service` or any endpoint for a player whose `PlayerIdentity.Incognito` is true.
- **No** instalock, dodge, or client automation.
- **Demo feed:** compiled only when `import.meta.env.DEV` (UI) or cargo feature `demo` (Rust). Release CI must build without it.
- **Installer:** deletes old program files, `%LOCALAPPDATA%\com.opd1.trcker` (image cache + WebView2 data) and settings. It never deletes `%APPDATA%\com.opd1.trcker\data\opd1.sqlite`.
- **Remote Riot calls:** a global token bucket (burst 10, 10 rps) and an MMR bucket (burst 24, 0.4 rps), with `429 Retry-After` honored. Values come from Scout's `riot_client.py`.
- **Idle cost:** when VALORANT is closed, the only work is one lockfile existence check every 5s. The presence poll (local only) runs every 2s while the client is up. Remote calls happen only on stage change or a user click.

- **The user tests only at the end (Task 11).** Until then, correctness rests on unit tests and fixtures. Fixture JSON must match real Riot shapes. Sources: Scout's `backend/sample_match.py` + `sample_data.py`, and the community endpoint docs at https://github.com/techchrism/valorant-api-docs. Never invent field names.
- **Execution method:** subagent-driven (the user asked for maximum accuracy). A fresh implementer + reviewer handles each task.

## Review Focus

1. **The game closes, restarts, or the token expires mid-match.**
   - Expected: the lockfile port/password changes, the client re-reads it, and the stage drops to Closed and then recovers. No crash, no frozen stale match.
   - Test: `riot::session` tests in Task 3.
2. **Riot returns 429 during Agent Select.**
   - Expected: cards fill progressively. Players already cached show immediately. Nobody is stuck on a skeleton forever. A failed field becomes `—` after the retry budget runs out.
   - Test: Task 3 throttle test and Task 5 `card_builds_with_partial_data`.
3. **New, unranked, or zero-death players.**
   - Expected: K/D with 0 deaths shows kills as K/D. Win rate with 0 games shows `—`. No peak shows `—`. No panic or NaN.
   - Test: Task 4 stats tests.
4. **Hidden players.**
   - `Incognito` + `HideAccountLevel` + agent not yet picked → `Hidden Player` and level `—`.
   - After the pick → the agent name.
   - A remembered name only comes from a match that includes the user's PUUID.
   - Test: Task 6 `display_name_*`.
5. **The longest Riot ID** (16 chars + 5-char tag, e.g. `WWWWWWWWWWWWWWWW#WWWWW`) on the smallest card (In Game).
   - Expected: it fits with no overlap and no font shrink.
   - Test: Task 8 Playwright fit check with a fixture containing that name.

---

## File Structure

```
Cargo.toml                         workspace
crates/core/src/
  lockfile.rs     parse Riot lockfile
  shards.rs       parse pd/glz from ShooterGame.log (latest entry wins)
  presence.rs     decode presence private blob → Stage, party_id, queue_id
  model.rs        PlayerCard, DisplayName, Stage, LiveState, PlayerDetails, MyStats (serde → UI)
  names.rs        display-name rules (hidden / remembered)
  stats.rs        K/D, HS%, winrate, peak, prev act, act label, session split
  party.rs        party detection from shared recent matches
  db/mod.rs       open + migrations (PRAGMA user_version)
  db/schema.sql
  db/repo.rs      typed queries (upserts, sync diff, encounters, known name)
  mystats.rs      My Stats aggregates + insights
crates/core/tests/fixtures/*.json  real-shaped Riot payloads (from Scout's sample_match.py and sample_data.py)
crates/riot/src/
  throttle.rs     token buckets
  session.rs      lockfile → local auth → entitlements → remote headers, auto-refresh
  endpoints.rs    typed calls (pregame, coregame, loadouts, mmr, compupdates, history, details, parties, names, presences)
  assets.rs       valorant-api JSON + image disk cache
src-tauri/
  tauri.conf.json, capabilities/, nsis/hooks.nsh
  src/main.rs     setup, window, state
  src/poll.rs     stage loop → builds LiveState → emit("live")
  src/commands.rs player_details(puuid), my_stats(), copy_log()
  src/demo.rs     #[cfg(feature="demo")] fake LiveState feed
ui/
  src/lib/tokens.css       colors/spacing/type scale
  src/lib/format.ts        timeAgo, actLabel, rrDelta, pct
  src/lib/stores.ts        live, tab, details state; listens to Tauri events or the demo feed
  src/lib/demo/            DEV-only fixtures + feed (tree-shaken in prod)
  src/components/PlayerCard.svelte, RankBadge.svelte, Last5.svelte, Skeleton.svelte, Tooltip.svelte
  src/screens/Live/{Lobby,AgentSelect,InGame}.svelte
  src/screens/Details.svelte
  src/screens/MyStats/*.svelte
  src/App.svelte           tabs + routing
  tests/fit.spec.ts        Playwright 1000×800 checks
.github/workflows/build.yml
```

---

### Task 1: Scaffold, private repo, CI → first installable Setup.exe

**Files:**
- Create: `Cargo.toml`, `crates/core/`, `crates/riot/`, `src-tauri/**`, `ui/**`, `.github/workflows/build.yml`, `.gitignore`
- Create: `src-tauri/nsis/hooks.nsh`

**Interfaces:**
- Produces:
  - An empty app window titled `OPD1 TRCKER` with the Global Constraints window config.
  - Tabs `Live` / `My Stats`, empty.
  - The CI artifact `OPD1-TRCKER-setup` (NSIS exe).

- [ ] **Step 1:** On the VPS, install rustup (stable) and the Tauri Linux deps (`libwebkit2gtk-4.1-dev libsoup-3.0-dev librsvg2-dev`) so `cargo check` works locally. Install `gh` and run `gh auth login --web`. Give the user the 8-char device code and have them enter it at `github.com/login/device`.
- [ ] **Step 2:** `git init`. Create the private repo `opd1-trcker` with `gh repo create --private`. Scaffold Tauri 2 + Svelte 5 + Vite (TS) with the frontend in `ui/`. Set `bundle.targets: ["nsis"]`, `nsis.installMode: "currentUser"`, the window config from Global Constraints, and `productName: "OPD1 TRCKER"`.
- [ ] **Step 3:** Write `hooks.nsh` with `NSIS_HOOK_PREINSTALL`.
  - It deletes the old `$INSTDIR\*` and `$LOCALAPPDATA\com.opd1.trcker`.
  - It deletes `$APPDATA\com.opd1.trcker\*` **except** the `data\` folder.
  - Wire it with `bundle.windows.nsis.installerHooks`.
- [ ] **Step 4:** Write `build.yml`.
  - Triggers: push to `main` + `workflow_dispatch`.
  - Runs on `windows-latest`: setup Rust + Node, `npm ci`, `cargo test -p core -p riot`, `npm --prefix ui run test`, `npx tauri build` (no `demo` feature).
  - Upload `src-tauri/target/release/bundle/nsis/*.exe` as the artifact.
- [ ] **Step 5:** Run `graft init --agents claude` and commit the `.claude/` wiring. Push. Expected: the CI run goes green and produces an artifact under ~10MB.
- [ ] **Step 6:** Deferred to Task 11 (window size, always on top, reinstall).
- [ ] **Step 7: Commit** `chore: scaffold tauri+svelte, nsis hooks, windows CI`.

### Task 2: Core parsers (lockfile, shards, presence)

**Files:**
- Create: `crates/core/src/{lockfile,shards,presence}.rs`, `crates/core/tests/parsers.rs`, fixtures

**Interfaces:**
- Produces:
  - `pub struct Lockfile { pub port: u16, pub password: String, pub protocol: String }`
  - `pub fn parse_lockfile(s: &str) -> Result<Lockfile, ParseError>`
  - `pub struct Shards { pub pd: String /* e.g. "eu" */, pub glz_region: String, pub glz_shard: String }`
  - `pub fn parse_shards(log: &str) -> Option<Shards>` (uses the **last** occurrence in the log)
  - `pub enum Stage { Closed, Menus, Pregame, Ingame }`
  - `pub struct SelfPresence { pub stage: Stage, pub party_id: Option<String>, pub queue_id: Option<String> }`
  - `pub fn decode_presence(private_b64: &str) -> Option<SelfPresence>`. It reads `sessionLoopState`, falling back to `matchPresenceData.sessionLoopState`, and `partyId`, falling back to `partyPresenceData.partyId`.

- [ ] **Step 1:** Write the tests:
  - `parse_lockfile("Riot Client:1234:54321:pw:https")` → port 54321, password "pw"
  - a malformed lockfile → Err
  - `parse_shards` on a log with an `eu` URL followed by a later `na` URL → `pd == "na"`
  - `decode_presence` on both the old and new presence shapes → `Stage::Pregame` with the party id
- [ ] **Step 2:** Run `cargo test -p core parsers`. Expected: FAIL (unresolved items).
- [ ] **Step 3:** Implement the three modules.
- [ ] **Step 4:** Run `cargo test -p core parsers`. Expected: PASS.
- [ ] **Step 5: Commit** `feat(core): lockfile, shard and presence parsers`.

### Task 3: Riot session, throttle, endpoints

**Files:**
- Create: `crates/riot/src/{throttle,session,endpoints}.rs`, `crates/riot/tests/*.rs`

**Interfaces:**
- Consumes: Task 2 parsers.
- Produces:
  - `pub struct RiotSession`
    - `RiotSession::connect(local_app_data: &Path) -> Result<Self, RiotError>`
    - `.self_puuid() -> &str`
    - `async fn local<T: DeserializeOwned>(&self, path) -> Result<T>`
    - `async fn pd<T>(&self, path) -> Result<T>`
    - `async fn glz<T>(&self, path) -> Result<T>`
  - Auto re-auth on 401/403. `RiotError::ClientGone` when the lockfile disappears or its port changes.
  - `pub struct Throttle`
    - `Throttle::new(burst: f64, rps: f64)`
    - `async fn take(&self)`
    - `fn hold_for(&self, secs: f64)` (called on 429)
  - `endpoints.rs` typed wrappers, one fn per endpoint:
    - `pregame_player`, `pregame_match`, `pregame_loadouts`
    - `coregame_player`, `coregame_match`, `coregame_loadouts`
    - `mmr(puuid)`, `comp_updates(puuid, start, end)`, `match_history(puuid, start, end, queue: Option<&str>)`, `match_details(id)`
    - `party_of(puuid)`, `party(id)`, `presences()`
    - `names(puuids: &[String])`, which **asserts** that no incognito puuid is passed (debug_assert + filtered by the caller)
  - Response structs cover only the fields we use: `PlayerIdentity { Incognito, HideAccountLevel, AccountLevel, PlayerCardID }`, `CharacterID`, `CharacterSelectionState`, `TeamID`, `SeasonalInfoBySeasonID`, `WinsByTier`, `RankedRatingEarned`, `TierAfterUpdate`, `MatchStartTime`, `MapID`, and match-details `players[].{subject,gameName,tagLine,teamId,partyId,characterId,stats}`, `teams[].won`, `roundResults[].playerStats[].damage[].{headshots,bodyshots,legshots}`.

- [ ] **Step 1:** Write the tests:
  - `throttle_limits_rate`: 20 takes at burst 10 / 10 rps take ≥ 0.9s
  - `throttle_hold_blocks`
  - `session_detects_port_change`: rewrite the temp lockfile, and the next call → `ClientGone`
  - endpoint deserialization from fixtures (`pregame_match.json`, `coregame_match.json`, `match_details.json`, `mmr.json`, `comp_updates.json`)
- [ ] **Step 2:** Run `cargo test -p riot`. Expected: FAIL.
- [ ] **Step 3:** Implement.
  - `reqwest` with `danger_accept_invalid_certs` **only** for the `127.0.0.1` client.
  - Client version from `/product-session/v1/external-sessions` (local), with a fallback to valorant-api `/v1/version`.
  - Client platform is the standard base64 blob from Scout.
- [ ] **Step 4:** Run `cargo test -p riot`. Expected: PASS.
- [ ] **Step 5: Commit** `feat(riot): session, throttling, typed endpoints`.

### Task 4: DB schema, migrations, sync repo, stats math

**Files:**
- Create: `crates/core/src/db/{mod.rs,schema.sql,repo.rs}`, `crates/core/src/stats.rs`, `crates/core/tests/{db,stats}.rs`

**Interfaces:**
- Produces:
  - `pub fn open_db(path: &Path) -> Result<Connection>`. It applies migrations to `user_version = 1`. Migrations are an ordered `&[&str]` and are idempotent on reopen.
  - Tables:
    - `players(puuid PK, game_name, tag_line, account_level, card_id, updated_ms)`
    - `matches(match_id PK, map_id, queue_id, start_ms, winning_team, is_ranked)`
    - `match_players(match_id, puuid, team_id, party_id, agent_id, kills, deaths, assists, score, rounds, hs, bs, ls, game_name, tag_line, PK(match_id,puuid))`
    - `history_index(puuid, match_id, queue_id, start_ms, PK(puuid,match_id))`
    - `comp_updates(puuid, match_id, season_id, tier_after, rr_after, rr_earned, map_id, start_ms, PK(puuid,match_id))`
    - `mmr_snapshots(puuid PK, json, last_comp_match_id, fetched_ms)`
  - `repo.rs`:
    - `upsert_match(&Match, &[MatchPlayer])`
    - `new_match_ids(puuid, &[String]) -> Vec<String>`: ids not in `matches`
    - `index_history(puuid, &[HistoryEntry])`
    - `last_n_comp(puuid, n) -> Vec<MatchPlayerRow>`
    - `needs_mmr_refresh(puuid, latest_comp_match_id) -> bool`
    - `encounters(self_puuid, other) -> Encounters { with: u32, against: u32, matches: Vec<EncounterRow> }`
    - `known_name(self_puuid, other) -> Option<(String,String)>`: only from `match_players` rows in matches where `self_puuid` also has a row, the most recent first
    - `top_teammates(puuid, season_start_ms, limit=3) -> Vec<(puuid, name, count)>`
  - `stats.rs`:
    - `kd(k,d) -> f32` (d=0 → k)
    - `hs_pct(hs,bs,ls) -> Option<f32>`
    - `winrate(w,g) -> Option<f32>`
    - `peak(seasons: &SeasonalMap) -> Option<Peak{tier, season_id}>` (max key of `WinsByTier` across seasons)
    - `prev_act_tier(seasons, ordered_acts) -> Option<u8>`
    - `act_label(season: &Act) -> String`: the official short form, e.g. `E8 A3` / `V25 A2`, derived from valorant-api `/seasons` names
    - `since_act_end(act, now_ms) -> Option<i64>`: `None` for the current act, which the UI shows as "This Act"
    - `split_sessions(starts_ms: &[i64], gap_ms = 6h) -> Vec<Range>`

- [ ] **Step 1:** Write the tests:
  - `migrations_idempotent`
  - `new_match_ids_filters_known`
  - `needs_mmr_refresh_false_when_same_latest`
  - `known_name_requires_shared_match`: a name seen only in someone else's match → None
  - `encounters_counts_with_against`
  - `top_teammates_orders_by_count`
  - `kd_zero_deaths`
  - `winrate_zero_games_none`
  - `peak_across_seasons`
  - `act_label_new_and_old_naming`
  - `sessions_split_on_6h_gap`
- [ ] **Step 2:** Run `cargo test -p core db stats`. Expected: FAIL.
- [ ] **Step 3:** Implement. Store only the compact columns above, never the raw JSON, except for the single `mmr_snapshots.json`.
- [ ] **Step 4:** Run `cargo test -p core db stats`. Expected: PASS.
- [ ] **Step 5: Commit** `feat(core): sqlite schema, sync repo, stats math`.

### Task 5: Card pipeline + assets + live loop

**Files:**
- Create: `crates/core/src/model.rs`, `crates/riot/src/assets.rs`, `src-tauri/src/{poll.rs,commands.rs}`, `crates/core/tests/card.rs`

**Interfaces:**
- Consumes: Tasks 2–4.
- Produces (serde camelCase to the UI):
  - `PlayerCard`:
    ```
    { puuid, name: DisplayName, level: Option<u32>, cardId, agentId: Option<String>,
      rank: Option<{tier, rr}>, prevActTier: Option<u8>,
      peak: Option<{tier, actLabel, actName, sinceActEndMs: Option<i64>}>,
      kd: Option<f32>, hsPct: Option<f32>, actWinrate: Option<f32>,
      last5: Vec<{win, rrEarned, startMs}>,
      party: Option<{group: u8, confirmed: bool}>, encounters: Option<{with, against}>,
      team: "ally"|"enemy", isSelfParty: bool }
    ```
    `None` always renders as `—`.
  - `LiveState { stage: Stage, mapId: Option<String>, players: Vec<PlayerCard> }`, emitted as the Tauri event `"live"` whenever it changes (a diff check on serialized bytes).
  - `pub fn build_card(ident, mmr, comp_updates, last5_details, …) -> PlayerCard`, a pure function.
  - Assets: `assets::ensure(kind, uuid) -> PathBuf`. It downloads once to `app_cache_dir/assets/` and is served to the UI via Tauri's asset protocol. Tables (agents, tiers, maps, seasons, chromas, cards) are refreshed when `/v1/version` changes.
  - The poll loop:
    - Closed: check the lockfile every 5s.
    - Client up: presence every 2s.
    - On a stage change, fetch the roster:
      - Lobby → party members
      - Pregame → ally team
      - Ingame → both teams + loadouts
    - Then **cached-first**: emit cards from the DB immediately, then sync each player in order (self first, then allies, then enemies).
    - Per player: history → diff → new details → MMR only if `needs_mmr_refresh`.
    - Re-emit as each card completes.
  - Commands:
    - `player_details(puuid) -> PlayerDetails { card, recent: Vec<MatchSummary /* up to 8 */>, topTeammates, loadout: {vandal, phantom, operator, sheriff: Option<ChromaId>}, encounters: Vec<EncounterRow> }`
    - Before computing `topTeammates`, `player_details` syncs that player's last 20 current-act Competitive matches (history → diff → only new details, throttled). Results that finish later re-emit via the event `"details"`.
    - `my_stats() -> MyStats` (Task 9)
    - `copy_log()`: puts the last 500 log lines on the clipboard. It is bound to Ctrl+Shift+L and has no visible UI.

- [ ] **Step 1:** Write the tests:
  - `card_builds_with_partial_data`: no MMR → rank None, other fields present
  - `card_hides_agent_until_selected`: `CharacterSelectionState == ""` → agentId None
  - `last5_newest_first`
  - `hide_account_level_gives_none`
- [ ] **Step 2:** Run `cargo test -p core card`. Expected: FAIL.
- [ ] **Step 3:** Implement `build_card`, assets, the poll loop, and the commands. The poll loop is not unit-tested; it's covered by Step 5.
- [ ] **Step 4:** Run `cargo test -p core card`. Expected: PASS.
- [ ] **Step 5:** Add the hidden command `dump_raw()` on Ctrl+Shift+D. It writes the last response of every endpoint, with tokens, JWTs and passwords redacted, to `%APPDATA%\com.opd1.trcker\debug\<endpoint>.json`. Task 11 turns those files into real fixtures. There is no visible UI for it.
- [ ] **Step 6: Commit** `feat: live pipeline with cached-first sync`.

### Task 6: Hidden names, party detection, encounters

**Files:**
- Create: `crates/core/src/{names,party}.rs`, `crates/core/tests/{names,party}.rs`
- Modify: `crates/core/src/model.rs` (fill `name`, `party`, `encounters`)

**Interfaces:**
- Produces:
  - `pub enum DisplayName { Visible{name,tag}, Remembered{name,tag}, HiddenPlayer, HiddenAgent{agentId} }`
  - `pub fn display_name(incognito: bool, visible: Option<(String,String)>, agent_selected: Option<&str>, remembered: Option<(String,String)>) -> DisplayName`
    - Precedence: not incognito → Visible
    - incognito + remembered → Remembered
    - incognito + agent → HiddenAgent
    - otherwise → HiddenPlayer
  - `pub fn detect_parties(players: &[(puuid, team)], recent: &HashMap<puuid, [Option<SharedRef>;2]>, self_party: &HashSet<puuid>) -> HashMap<puuid, PartyMark>`
    - `SharedRef { match_id, team_id: Option<String>, party_id: Option<String> }`, with team_id None for Deathmatch.
    - Two players link when their most recent match_id is equal and (team_id equal, or both None).
    - The link is confirmed when both of their last-2 ids are equal (pairwise on the same team), **or** when their most recent shared match has the same non-empty `party_id` for both.
    - Groups come from union-find. Group colors are 0..n in roster order. A group's `confirmed` is true only if all its links are confirmed.
    - Players in `self_party` are excluded entirely.
  - Encounters are set to None for self + self-party.

- [ ] **Step 1:** Write the tests:
  - `display_name_visible`
  - `display_name_hidden_before_pick`
  - `display_name_hidden_after_pick`
  - `display_name_remembered_beats_agent`
  - `party_one_shared_is_likely`
  - `party_two_shared_is_confirmed`
  - `party_same_party_id_one_game_is_confirmed`
  - `party_different_party_id_one_game_stays_likely`
  - `party_transitive_group`
  - `party_deathmatch_no_team`
  - `party_different_team_not_linked`
  - `self_party_never_marked`
- [ ] **Step 2:** Run `cargo test -p core names party`. Expected: FAIL.
- [ ] **Step 3:** Implement and wire into `build_card`. The poll loop filters incognito puuids out of every `names()` call.
- [ ] **Step 4:** Run `cargo test -p core`. Expected: PASS.
- [ ] **Step 5: Commit** `feat(core): hidden-name rules, party detection, encounters`.

### Task 7: Design tokens + PlayerCard + demo feed (DEV only)

**Files:**
- Create: `ui/src/lib/{tokens.css,format.ts,stores.ts}`, `ui/src/lib/demo/**`, `ui/src/components/*.svelte`, `ui/src/lib/format.test.ts`

**Interfaces:**
- Consumes: the Task 5/6 JSON shapes (mirror them as TS types in `ui/src/lib/types.ts`).
- Produces:
  - `<PlayerCard card={PlayerCard} size="lg"|"md"|"sm" on:open />`
  - The `live` store. In a Tauri build it listens to `listen("live")`. When `import.meta.env.DEV && !window.__TAURI__`, it plays the demo feed cycling Lobby(1–5) / Queue / Pregame / Ingame with keyboard 1–7. There is no on-screen demo button, so layout is unaffected.

- [ ] **Step 1:** Load the `interface-design` skill (craft-first product UI) plus `svelte-code-writer`, then set the tokens. Use `impeccable` afterwards to audit and polish, not to set direction.
  - Palette: a dark blue-grey base, one accent, rank colors from `/competitivetiers`, off-white text.
  - A type scale with nothing below 12px.
  - `font-variant-numeric: tabular-nums` for stats.
- [ ] **Step 2:** Write the tests in `format.test.ts`:
  - `timeAgo(now-3h) == "3h"`
  - `actLabel` passthrough
  - `rrDelta(+16) == "+16"`, `rrDelta(-19) == "−19"`
  - `pct(0.545) == "54.5%"`
  - `null` → `"—"` for every formatter
- [ ] **Step 3:** Run `npm --prefix ui run test`. Expected: FAIL. Implement `format.ts`. Run again. Expected: PASS.
- [ ] **Step 4:** Build `PlayerCard` and its subcomponents covering every field in CLAUDE.md "Player card fields".
  - The Tag is a separate, dimmer span.
  - `Remembered` names use the accent color.
  - The party stripe is dashed or solid by `confirmed`.
  - ⟲ encounters has a hover tooltip `N with · M against`.
  - The peak tooltip shows the full act name.
  - Skeletons are used for `undefined` (loading), distinct from `null` (`—`).
- [ ] **Step 5: Commit** `feat(ui): tokens, formatters, player card, dev demo feed`.

### Task 8: Live screens + fit verification

**Files:**
- Create: `ui/src/screens/Live/{Lobby,AgentSelect,InGame}.svelte`, `ui/src/App.svelte`, `ui/tests/fit.spec.ts`, `ui/playwright.config.ts`

**Interfaces:**
- Consumes: the `PlayerCard` and `live` store.
- Produces:
  - Tabs `Live` / `My Stats`. Auto-switch to Live when `stage` becomes `Pregame`. When Closed, the default tab is My Stats, and Live shows the self card only.
  - Lobby grid by count: 1 → a single large card, 2–3 → one row, 4–5 → 3+2. Average Party Rank shows when count ≥ 2.
  - AgentSelect: 5 cards, no scroll.
  - InGame: two columns of 5 (ally left, enemy right) with a map header, no scroll.

- [ ] **Step 1:** Write `fit.spec.ts`. It runs against `vite preview` with a DEV build at viewport 1000×800. For each demo scene (lobby1..5, pregame, ingame, ingame-longnames):
  - `document.scrollingElement.scrollHeight <= 800` for pregame/ingame
  - every `[data-text]` has `scrollWidth <= clientWidth` (no clipping)
  - no two `[data-stat]` bounding boxes intersect within a card
  - every text node's computed `font-size >= 12`
  - save a screenshot to `ui/tests/shots/<scene>.png`
- [ ] **Step 2:** Run `npx playwright test`. Expected: FAIL (screens missing).
- [ ] **Step 3:** Implement the screens.
- [ ] **Step 4:** Run `npx playwright test`. Expected: PASS. Look at every screenshot yourself and fix anything off. User design review happens in Task 11.
- [ ] **Step 5: Commit** `feat(ui): lobby, agent select, in-game screens with fit tests`.

### Task 9: Player Details + My Stats

**Files:**
- Create: `ui/src/screens/Details.svelte`, `ui/src/screens/MyStats/{RankCard,RrChart,Session,MatchRows,MapsTable,AgentsTable,TimeWinrate,Insights}.svelte`, `crates/core/src/mystats.rs`, `crates/core/tests/mystats.rs`

**Interfaces:**
- Consumes: `player_details`, and `my_stats` over `repo` + `stats`.
- Produces:
  - `MyStats`:
    ```
    { rank:{tier,rr,wins,losses,rrToNext,netRr,avgWin,avgLoss,changes:Vec<{tier,up,startMs}>},
      rrSeries: Vec<{startMs, absoluteRr}>, session:{netRr,wins,losses,startedMs,streak:Vec<'W'|'L'|'D'>},
      last5: Vec<MatchSummary>, maps: Vec<Row>, agents: Vec<Row>, byWeekday: [Cell;7], byDaypart: [Cell;4],
      insights: Vec<{kind, text, matches, positive: bool}> }
    ```
    All of it covers the current act only.
  - The RR chart is inline SVG with no chart library.
  - Details layout:
    - Back button.
    - Recent matches: 3, then Show More → up to 8.
    - Top 3 Teammates.
    - Loadout: 4 weapons with the chroma `fullRender`.
    - Encounters list.
    - Scrollable.
- [ ] **Step 1:** Write the tests:
  - `mystats_current_act_only`
  - `rr_to_next_immortal_edge` (no next tier → None)
  - `daypart_buckets` (Morning 5–12, Afternoon 12–17, Evening 17–22, Night 22–5, local time)
  - `insight_best_worst_map_needs_min_5_games`
  - `session_uses_6h_gap`
- [ ] **Step 2:** Run `cargo test -p core mystats`. Expected: FAIL. Implement. Run again. Expected: PASS.
- [ ] **Step 3:** Build the UI. Extend `fit.spec.ts` with the scenes `details`, `details-more` (8 rows) and `mystats`, using the clipping/overlap/font checks only (scroll allowed). Run `npx playwright test`. Expected: PASS.
- [ ] **Step 4:** Deferred to Task 11.
- [ ] **Step 5: Commit** `feat: player details and my stats`.

### Task 10: Release hardening

**Files:**
- Modify: `.github/workflows/build.yml`, `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`

- [ ] **Step 1:** Add a CI step that fails the build if `ui/dist` contains the string `demo-feed` or if the `demo` feature is enabled in the release build.
- [ ] **Step 2:** Set the release profile: `opt-level="s"`, `lto=true`, `codegen-units=1`, `strip=true`, `panic="abort"`. Expected: the installer is ≤ 10MB.
- [ ] **Step 3:** Deferred to Task 11.
- [ ] **Step 4: Commit + tag** `v0.1.0`.

### Task 11: Windows acceptance (the user) + design review

**Files:**
- Modify: whatever the findings require
- Create: `crates/core/tests/fixtures/real/*.json` from the user's `debug/` dumps

- [ ] **Step 1:** Give the user a short Arabic checklist and the `Setup.exe` from CI:
  1. The window is 1000×800, can't be resized, and stays on top. The close button quits.
  2. Game closed → the app opens on My Stats, and Live shows only their own card.
  3. Lobby solo, then with 2–5 friends → layouts and Average Party Rank.
  4. Queue → nothing changes. Match found → it jumps to Agent Select with 5 allies and no scroll.
  5. Load in → 5 v 5 with the map header and no scroll. Hidden players follow the name rules.
  6. Click a card in each stage → Details, Show More up to 8, Top 3 Teammates, Loadout, Encounters.
  7. Match end → Lobby. Play a second match with the same friend → encounters count up.
  8. Install the next build over the old one → encounters and My Stats history are still there.
  9. Idle RAM in Task Manager with the game closed.
  10. After a match, Ctrl+Shift+D + Ctrl+Shift+L, then send the `debug` folder and the log.
- [ ] **Step 2:** Turn each dump into a fixture under `fixtures/real/`. Add a deserialization test per endpoint. Run `cargo test`. Expected: PASS. Fix any shape mismatch it reveals.
- [ ] **Step 3:** Send the user every Task 8/9 screenshot and iterate on the design until they approve.
- [ ] **Step 4:** Fix every finding, rebuild, and have the user re-run only the failed checklist items.
- [ ] **Step 5: Commit + tag** `v1.0.0`.

---

## Resolved

- `partyId` upgrade approved by the user (2026-09-27) and folded into Task 6.
