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
- **No-scroll stages:** Agent Select, In Game. Scroll is allowed in Player Details and My Stats. (Phase 2: no scrolling in any Live stage)
- **Agent image** is `displayIcon` from `/v1/agents?isPlayableCharacter=true`. Only show it after the agent is selected.
- **Never** call `name-service` or any endpoint for a player whose `PlayerIdentity.Incognito` is true.
- **No** instalock, dodge, or client automation.
- **Demo feed:** compiled only when `import.meta.env.DEV` (UI) or cargo feature `demo` (Rust). Release CI must build without it. (R40: no Rust demo feature; DEV-only UI feed)
- **Installer:** deletes old program files, `%LOCALAPPDATA%\com.opd1.trcker` (image cache + WebView2 data) and settings. It never deletes `%APPDATA%\com.opd1.trcker\data\opd1.sqlite`. (2026-09-29: the uninstaller's optional "Delete app data" may remove the DB — accepted)
- **Remote Riot calls:** a global token bucket (burst 10, 10 rps) and an MMR bucket (burst 24, 0.4 rps), with `429 Retry-After` honored. Values come from Scout's `riot_client.py`.
- **Idle cost:** when VALORANT is closed, the only work is one lockfile existence check every 5s. The presence poll (local only) runs every 2s while the client is up. Remote calls happen only on stage change or a user click.

- **The user tests only at the end (Task 11).** Until then, correctness rests on unit tests and fixtures. Fixture JSON must match real Riot shapes. Sources: Scout's `backend/sample_match.py` + `sample_data.py`, and the community endpoint docs at https://github.com/techchrism/valorant-api-docs. Never invent field names. (ملغي ← 2026-09-29: المستخدم يبي يستخدم In Game و Agent Select الحين)
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
5. **The longest Riot ID** (16 chars + 5-char tag, e.g. `WWWWWWWWWWWWWWWW#WWWWW`) on the smallest card (In Game). (Phase 2: the name part truncates with …, the tag stays whole — spec §3.4)
   - Expected: it fits with no overlap and no font shrink.
   - Test: Task 8 Playwright fit check with a fixture containing that name.

---

## File Structure (UI part superseded by Phase 2, Tasks 16a–18)

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
    - *Superseded 2026-09-30 (R159, `docs/DECISIONS.md`): the version now comes from the game's own presence, then valorant-api, then the `CI server version:` line of `ShooterGame.log`, then `release-09.00`, as Valorant-Scout does. external-sessions is no longer used. The plan text above is left as it was written.*
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
      last5: Vec<{win, rrEarned, startMs}>, // R32: {result, rrEarned|null, startMs}; full wire types in spec §7.1
      party: Option<{group: u8, confirmed: bool}>, encounters: Option<{with, against}>,
      team: "ally"|"enemy", isSelfParty: bool }
    ```
    `None` always renders as `—`.
  - `LiveState { stage: Stage, mapId: Option<String>, players: Vec<PlayerCard> }`, emitted as the Tauri event `"live"` whenever it changes (a diff check on serialized bytes).
  - `pub fn build_card(ident, mmr, comp_updates, last5_details, …) -> PlayerCard`, a pure function.
  - Assets: `assets::ensure(kind, uuid) -> PathBuf`. It downloads once to `app_cache_dir/assets/` and is served to the UI via Tauri's asset protocol. Tables (agents, tiers, maps, seasons, chromas, cards) are refreshed when `/v1/version` changes. (R11: served through the img:// scheme)
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

### Task 7: Design tokens + PlayerCard + demo feed (DEV only) (visuals superseded by Phase 2, Task 16 and Task 17)

**Files:**
- Create: `ui/src/lib/{tokens.css,format.ts,stores.ts}`, `ui/src/lib/demo/**`, `ui/src/components/*.svelte`, `ui/src/lib/format.test.ts`

**Interfaces:**
- Consumes: the Task 5/6 JSON shapes (mirror them as TS types in `ui/src/lib/types.ts`).
- Produces:
  - `<PlayerCard card={PlayerCard} size="lg"|"md"|"sm" on:open />` (superseded by Phase 2, Task 17)
  - The `live` store. In a Tauri build it listens to `listen("live")`. When `import.meta.env.DEV && !window.__TAURI__`, it plays the demo feed cycling Lobby(1–5) / Queue / Pregame / Ingame with keyboard 1–7. There is no on-screen demo button, so layout is unaffected.

- [ ] **Step 1:** Load the `interface-design` skill (craft-first product UI) plus `svelte-code-writer`, then set the tokens. Use `impeccable` afterwards to audit and polish, not to set direction. (superseded by Phase 2, Task 16)
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
- [ ] **Step 4:** Build `PlayerCard` and its subcomponents covering every field in CLAUDE.md "Player card fields". (superseded by Phase 2, Task 17)
  - The Tag is a separate, dimmer span.
  - `Remembered` names use the accent color.
  - The party stripe is dashed or solid by `confirmed`.
  - ⟲ encounters has a hover tooltip `N with · M against`.
  - The peak tooltip shows the full act name.
  - Skeletons are used for `undefined` (loading), distinct from `null` (`—`).
- [ ] **Step 5: Commit** `feat(ui): tokens, formatters, player card, dev demo feed`.

### Task 8: Live screens + fit verification (superseded by Phase 2, Task 17)

**Files:**
- Create: `ui/src/screens/Live/{Lobby,AgentSelect,InGame}.svelte`, `ui/src/App.svelte`, `ui/tests/fit.spec.ts`, `ui/playwright.config.ts`

**Interfaces:**
- Consumes: the `PlayerCard` and `live` store.
- Produces:
  - Tabs `Live` / `My Stats`. Auto-switch to Live when `stage` becomes `Pregame`. When Closed, the default tab is My Stats, and Live shows the self card only.
  - Lobby grid by count: 1 → a single large card, 2–3 → one row, 4–5 → 3+2. Average Party Rank shows when count ≥ 2.
  - AgentSelect: 5 cards, no scroll.
  - InGame: two columns of 5 (ally left, enemy right) with a map header, no scroll.

- [ ] **Step 1:** Write `fit.spec.ts`. It runs against `vite preview` with a DEV build at viewport 1000×800. For each demo scene (lobby1..5, pregame, ingame, ingame-longnames): (superseded by Phase 2, Task 17)
  - `document.scrollingElement.scrollHeight <= 800` for pregame/ingame
  - every `[data-text]` has `scrollWidth <= clientWidth` (no clipping)
  - no two `[data-stat]` bounding boxes intersect within a card
  - every text node's computed `font-size >= 12`
  - save a screenshot to `ui/tests/shots/<scene>.png`
- [ ] **Step 2:** Run `npx playwright test`. Expected: FAIL (screens missing). (superseded by Phase 2, Task 17)
- [ ] **Step 3:** Implement the screens. (superseded by Phase 2, Task 17)
- [ ] **Step 4:** Run `npx playwright test`. Expected: PASS. Look at every screenshot yourself and fix anything off. User design review happens in Task 11. (superseded by Phase 2, Task 17)
- [ ] **Step 5: Commit** `feat(ui): lobby, agent select, in-game screens with fit tests`. (superseded by Phase 2, Task 17)

### Task 9: Player Details + My Stats (UI half superseded by Phase 2, Task 18; My Stats UI waits until the user asks)

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
- [ ] **Step 3:** Build the UI. Extend `fit.spec.ts` with the scenes `details`, `details-more` (8 rows) and `mystats`, using the clipping/overlap/font checks only (scroll allowed). Run `npx playwright test`. Expected: PASS. (superseded by Phase 2, Task 18)
- [ ] **Step 4:** Deferred to Task 11.
- [ ] **Step 5: Commit** `feat: player details and my stats`.

### Task 10: Release hardening (superseded by Phase 2, Task 19)

**Files:**
- Modify: `.github/workflows/build.yml`, `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`

- [ ] **Step 1:** Add a CI step that fails the build if `ui/dist` contains the string `demo-feed` or if the `demo` feature is enabled in the release build. (superseded by Phase 2, Task 19)
- [ ] **Step 2:** Set the release profile: `opt-level="s"`, `lto=true`, `codegen-units=1`, `strip=true`, `panic="abort"`. Expected: the installer is ≤ 10MB. (superseded by Phase 2, Task 19)
- [ ] **Step 3:** Deferred to Task 11.
- [ ] **Step 4: Commit + tag** `v0.1.0`. (superseded by Phase 2, Task 19: tag v0.2.0)

### Task 11: Windows acceptance (the user) + design review (Phase 2: this is the user's test of the v0.2.0 build; superseded by Phase 2, Task 19 for the checklist)

**Files:**
- Modify: whatever the findings require
- Create: `crates/core/tests/fixtures/real/*.json` from the user's `debug/` dumps

- [ ] **Step 1:** Give the user a short Arabic checklist and the `Setup.exe` from CI: (superseded by Phase 2, Task 19: docs/test-checklist.md)
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
- [ ] **Step 3:** Send the user every Task 8/9 screenshot and iterate on the design until they approve. (superseded by Phase 2, Task 17 and Task 18: design approved 2026-09-29)
- [ ] **Step 4:** Fix every finding, rebuild, and have the user re-run only the failed checklist items.
- [ ] **Step 5: Commit + tag** `v1.0.0`.

---

## Resolved

- `partyId` upgrade approved by the user (2026-09-27) and folded into Task 6.

---

## Phase 2 — Claude Design v8 build (2026-09-29)

**Goal:** a usable `v0.2.0` Setup.exe. It has one Live page (Lobby / Agent Select / In Game / Closed), Player Details and Match Details, built from the approved Claude Design v8 file with all 6 design options wired at their defaults. The backend feeds every field the design shows.

**Inputs (read them, don't copy them):**
- `docs/requirements-raw.md`: the 2026-09-29 block is newest and wins.
- `.superpowers/audit-2026-09-29.md` ("audit §N").
- `docs/design-spec.md` ("spec §N"). Task 13 commits it from `.superpowers/design/cd-v8/spec.md` (text only; the 281MB design folder stays uncommitted). From Task 13 on, only the committed copy is edited. Task 16a runs in parallel with Task 13, so it reads the `.superpowers/` copy; nobody edits either copy while the parallel lanes run, so the two are identical. UI tasks cite its sections instead of repeating them.
- The design: `.superpowers/design/cd-v8/project/OPD1 Tracker v8.dc.html`, plus `support.js`, which is used only inside the test harness.
- Ledger rulings: `.superpowers/sdd/plan/progress.md`. R1–R73 (R72 = the current-act backfill deferral, R73 = the Task 9a fix round). R87–R98 = the pre-flight rulings on this plan. R99–R101 = the user's answers of 2026-09-29 (lock-only agent, own level, own-history backfill). R74–R86 are reserved for the spec's [C]/[bound] calls and this plan's review: Task 13 writes them to the ledger and mirrors R1–R101 in `docs/DECISIONS.md`.
- Code facts come from graft (`graft ask "<q>" --source`, `graft skeleton <file>`, `graft callers <sym>`, `graft grep "<literal>"`) before reading source.

**What this supersedes:**
- Task 7's visuals (tokens, 3 card sizes) → Tasks 16a–17.
- Task 8 → Task 17.
- Task 9b (the UI half) → Task 18 for Player Details. The My Stats UI waits until the user asks.
- Task 10 → Task 19.
- Task 11 becomes the user's test of the v0.2.0 build, using the checklist from Task 19.
- Tasks 1–6 and 9a stand. Task 12 (the 9a review) is already complete (R87).

**Order and lanes (R98):** three lanes run in parallel, then one sequence. Each task still gets its own review before the next task in its lane starts.
- **Lane A: Task 13** in `/home/project-1` on `main`. Pushes `main`.
- **Lane B: Task 14 → Task 15 → Task 15b** in the worktree `/home/project-1/.worktrees/p2-backend` on branch `p2-backend`. Task 14 Step 0 creates the worktree from `main`.
  - Each task commits and pushes the branch (`git push origin p2-backend`; the first push uses `-u`). CI builds only on a push to `main` or a manual dispatch (`build.yml`: `on: push: branches: [main]` + `workflow_dispatch`), so lane B's "watch CI" dispatches the workflow on its branch (Global "CI watch").
  - Task 13 must land on `origin/main` before Task 15's first commit. Task 15 Step 0 waits for that, then merges `origin/main` into `p2-backend`.
  - At the end of Task 15, `main` takes `p2-backend` in `/home/project-1` (fast-forward, else a merge commit) and pushes, and CI on `main` must be green (Task 15 Step 13). Task 16b starts only after that.
  - Task 15b then continues on `p2-backend`, in parallel with Tasks 16b → 18, and lands on `main` the same way at its end (Task 15b Step 6).
- **Lane C: Task 16a** in `/home/project-1/.worktrees/ui` on `ui-lane`. Pushes `ui-lane`. It merges the fixed commit `c9ad7f4` (main before any Phase 2 commit), not the moving `main`.
- **Then, one after the other:** Task 16b → Task 17 → Task 18 on `ui-lane`. Task 19 on `main` in `/home/project-1` starts after Task 18 and Task 15b have both landed.
- **Explicit directories:** agent shells reset to `/home/project-1` between calls. Every command in a worktree task starts with an explicit `cd <worktree> &&`: lane B uses `cd /home/project-1/.worktrees/p2-backend &&`, the UI lane uses `cd /home/project-1/.worktrees/ui &&`. Every worktree commit is guarded by the branch check (Global Git).
- **The ledger** (`.superpowers/sdd/plan/progress.md`, disk only) has one writer at a time. The controller holds its own ledger lines while Task 13 Step 3b edits the file.
- **File ownership.** No two lanes that run at the same time write the same file:

  | Lane / phase | Tasks | Writes only |
  |---|---|---|
  | A | 13 | `CLAUDE.md`, `README.md`, `CHANGELOG.md`, `AGENTS.md`, `LICENSE`, `THIRD-PARTY-NOTICES.md`, `.editorconfig`, `.gitattributes`, `docs/**` (incl. the 37 staged deletions) |
  | B | 14, 15, 15b | `crates/**`, `src-tauri/**`, `.github/workflows/build.yml` |
  | C | 16a | `ui/src/**`, `ui/vite.config.ts` |
  | then (UI) | 16b, 17, 18 | `ui/src/**`, `ui/vite.config.ts`, `ui/tests/**`, `ui/playwright.config.ts`, `.gitignore` (Task 17 only) |
  | then (main) | 19 | `Cargo.toml`, `Cargo.lock`, `.github/workflows/build.yml`, `src-tauri/{tauri.conf.json,Cargo.toml}`, `package.json`, `ui/package.json`, `package-lock.json`, `CHANGELOG.md`, `README.md`, `docs/ARCHITECTURE.md`, `docs/screenshots/**`, `docs/test-checklist.md` |

  Moved to keep the lanes apart: Task 15's `docs/ARCHITECTURE.md` refresh now runs in Task 19, because lane A owns `docs/` while lane B runs. `.gitignore`, `Cargo.toml`, `ui/package.json`, `CHANGELOG.md` and `README.md` have no writer in the parallel phase besides the one listed. Task 15b (lane B) and Tasks 16b–18 (UI) overlap in time but write disjoint trees.

**Split change from the brief:** the golden contract JSON and its Rust test land in Task 15 (on `p2-backend`, reaching `main` at the end of Task 15). Task 16b adds only the vitest half. Why: R22 keeps `ui-lane` out of `crates/`, and the Rust side owns the contract.

### Phase 2 Global Constraints

- **Window:** 1000×800, `resizable: false`, `maximizable: false`, `alwaysOnTop: true`. Close quits. No tray, no autostart.
- **Build scope:**
  - One Live page:
    - Lobby = me + my party in the left column.
    - Agent Select = my 5 in the left column + 5 dashed, empty, card-shaped slots on the right, with no text.
    - In Game = my team left, enemies right.
  - Plus Player Details and Match Details.
  - NOT built: the My Stats page, the Match History page, the Settings page.
- **Design options:** all 6 are built. They ship at the design defaults: tone `Neutral`, accent `#e3a54f`, results `Green / Red`, fill `Tinted`, font `Barlow`, last-5 `Arrow`. They become switchable later, from a Settings page that is not built now.
- **Last 5:** oldest → newest left to right. The newest is the RIGHTmost tile, with the arrow under it. Empty slots sit on the left.
- **Claude calls (R71):**
  - The start tab is `Live` until My Stats exists.
  - Average Party Rank goes in the top bar in the Lobby (2+ players).
  - The map shows in the top bar in Agent Select as soon as it's known.
  - R71's fourth call ("hover counts as selected") is reversed by the user (R99, below).
- **User decisions after the decisions audit (2026-09-29; ledger R99–R101; the last user line of `docs/requirements-raw.md`):**
  - **Lock-only selection (R99):** an agent counts as selected only when locked (`CharacterSelectionState == "locked"`). Before the lock the agent box is empty and a hidden player shows `Hidden Player`. After the lock the card shows the agent icon, and a hidden player shows the agent name. A hover changes nothing. (Task 14.)
  - **Own level (R100):** a hidden account level (`HideAccountLevel`) shows `Lv —` for everyone except the user's own card, which always shows the user's own level (the same logic as R49's own name). (Task 14.)
  - **Own-history backfill (R101):** on connect and after each match, the app pages the user's own match history (all queues) back to the last match it has confirmed as stored, and fetches match-details only for unknown ids. It is throttled and runs at the lowest priority, behind live cards. The first run stops at the current act start or 200 matches. (Task 15b.)
  - Lobby cards stay In Game size (R75 kept).
- **Text:**
  - Minimum 12px. Never shrink to fit.
  - Tabular numerals for stats.
  - No status text or badges.
  - Skeletons have no text.
  - A missing value is `—` (U+2014).
  - Only the name part of a Riot ID truncates with `…`. The tag is never cut.
  - Besides Riot-ID names, only the design's own ellipsis spans in Details may truncate: the encounters header name, the loadout skin line and the recent-row map name (spec §8.3 check 5). The Peak and Prev rows never clip.
  - Counts use singular/plural (`1 game`, `1 ranked player`; spec §4.3 `count`).
- **Scrolling:** none in any Live stage. Player Details and Match Details scroll vertically only.
- **Lightness:**
  - No new runtime dependency: the UI's runtime dependency stays `@tauri-apps/api` alone, and no new Rust crate.
  - Fonts are bundled as latin-subset woff2, only for weights 500/600/700 + 500 italic of Barlow, IBM Plex Sans and Archivo.
  - No Google Fonts and no CDN at runtime.
  - Every image goes through the Rust `img://` cache.
  - The CSP in `src-tauri/tauri.conf.json` stays unchanged.
- **Hidden names:** as in CLAUDE.md "Hidden names" (after Task 13 applies audit §2.11 with the R99/R100 wording and the R74 version of §2.12). Never request a hidden player's name. Remembered names come only from matches the user played. In every Match Details view, a row whose puuid is hidden on the live screen now follows its card's rule: the remembered style when the name is known, else the agent name (R93).
- **DB:**
  - Versioned migrations only (R89). Migration 1 (`crates/core/src/db/schema.sql`) is frozen as it stands at `c9ad7f4`. Phase 2's 4 columns ship as migration 2 (`crates/core/src/db/migration2.sql`: 4× `ALTER TABLE … ADD COLUMN`), applied by the existing `PRAGMA user_version` runner in `open_db`. A fresh DB runs 1 then 2; a `user_version` 1 DB from an earlier build runs only 2.
  - Every later schema change is a new migration appended to `MIGRATIONS`. There is no self-heal code and no in-place edit of a committed migration. This replaces R31's in-place permission and the planned R85 self-heal.
  - Uninstall "Delete app data" may wipe the DB. The Tauri default is accepted.
- **Git:**
  - Every step is committed with the exact message given and pushed right away: `git push origin main` on main, `git push origin p2-backend` in lane B, `git push origin ui-lane` on the UI lane.
  - Commit explicit paths only: `git add <paths> && git commit -m "<msg>" -- <paths>`. The trailing `-- <paths>` commits only those paths even if something else is staged. Never `git add -A`, `git add .` or `git add --renormalize .` (it implies `-u`).
  - Before every push, check that nothing the step touched was left out:
    - Main (`/home/project-1`: Task 13, the lane-B landings, Task 19): `git -C /home/project-1 status --porcelain -- crates src-tauri ui .github docs CLAUDE.md README.md CHANGELOG.md AGENTS.md LICENSE THIRD-PARTY-NOTICES.md .editorconfig .gitattributes Cargo.toml Cargo.lock package.json package-lock.json` prints nothing.
    - Lane B: `git -C /home/project-1/.worktrees/p2-backend status --porcelain -- crates src-tauri .github` prints nothing.
    - UI lane: `git -C /home/project-1/.worktrees/ui status --porcelain -- ui .gitignore` prints nothing.
  - Worktree commits are guarded by the branch: lane B with `test "$(git -C /home/project-1/.worktrees/p2-backend branch --show-current)" = p2-backend`, the UI lane with `test "$(git -C /home/project-1/.worktrees/ui branch --show-current)" = ui-lane`.
  - A rejected (non-fast-forward) push: `git pull --rebase --autostash origin <branch>`, re-run the step's tests, push again. Never force-push.
  - Never commit `.superpowers/` or root `*.zip`.
- **CI watch** (every step that says "watch CI"; picks the run by commit, so an older green run can't answer for the new push):
  ```bash
  cd <the task's directory> && sha=$(git rev-parse HEAD); id=""
  for i in $(seq 1 30); do id=$(gh run list -w Build -c "$sha" -L 1 --json databaseId -q '.[0].databaseId'); [ -n "$id" ] && break; sleep 10; done
  test -n "$id" && gh run watch "$id" --exit-status
  ```
  - On `main`, the push starts the run.
  - Lane B: a push to `p2-backend` starts nothing, so dispatch the run first: `cd /home/project-1/.worktrees/p2-backend && gh workflow run Build --ref p2-backend`, then the loop above from the same directory. A dispatched run uses the branch's own `build.yml` and carries the branch head as its commit.
  - If `sleep` is blocked in the shell, poll the same `gh run list -c` call with Monitor instead.
- **Skills (CLAUDE.md: load the best-in-class skill first):** each task's Step 0 loads the skills it names. Rust tasks: `test-driven-development`, plus `tauri-v2` for commands, events or `tauri.conf.json`. UI tasks: `svelte-code-writer` (mandatory for every `.svelte` / `.svelte.ts` edit), `svelte-core-bestpractices`, `test-driven-development`, `webapp-testing` for Playwright. Use graft for code facts. graft indexes `/home/project-1` (`main`); inside a worktree its answers describe `main`'s code, so confirm each line against the worktree's file before editing.
- **Precedence (R92):** requirements-raw.md 2026-09-29 > the design file > older CLAUDE.md visual rules. `docs/design-spec.md` transcribes the design. It outranks the design file only for its [C]/[bound] gap-fills (R74–R86). If the spec and the design disagree on something the design covers, the design wins and the spec is corrected. Any change to a visual the design covers goes to the user first: the user approved the design, not the spec.
- **Demo data:** DEV only (`import.meta.env.DEV`). It keeps the `demo-feed` marker (R15) and never ships. There is no Rust `demo` feature (R40).
  - The feed, `demo/cdv8.ts` and `demo/scenes.ts` are reached only through a dynamic import behind the DEV flag (`if (import.meta.env.DEV) await import('./demo/scenes')`), in `api.ts` and in boot. Never a static import: `vite build` must drop every mock roster.
  - The `ui/dist` guard (Task 16a Step 6, Task 16b Step 4, Task 19 CI) greps `demo-feed|VesperUnbreakabl|Vantablack|fonts.googleapis|fonts.gstatic|unpkg.com`. `VesperUnbreakabl` and `Vantablack` are fixture-only names from `cdv8.ts`, so a leaked scene bundle fails even without the marker. The last three catch a font CDN or the design prototype's runtime.
- **Test commands:**
  - Rust (lane B: prefix `cd /home/project-1/.worktrees/p2-backend &&`; on main: `cd /home/project-1 &&`): `cargo test -p opd1-core -p opd1-riot -p opd1-trcker`. `ui/dist` must exist, so run `npm --prefix ui run build` first. A new worktree needs `npm ci` once.
  - UI (UI lane: prefix `cd /home/project-1/.worktrees/ui &&`): `npm --prefix ui run test` + `npm --prefix ui run check`.
  - Playwright: `cd /home/project-1/.worktrees/ui/ui && npx playwright test`. Chromium, viewport 1000×800, `deviceScaleFactor: 1`, `timezoneId: 'UTC'`, `locale: 'en-US'`, page clock fixed at `2026-09-29T21:59:00Z` (spec §8.1).
    - `ui/tests/fit-dpr.spec.ts` sets `test.use({ deviceScaleFactor: 1.5 })` for spec §8.6's DPR checks. There is no second project, so no two projects write the same reference PNG.

### Phase 2 Review Focus

1. **Hidden names end to end.** No hidden player's real name may reach the DOM or any attribute: cards, Top 3, the encounters header, or Match Details for a match the user didn't play. A hidden player who only hovers an agent stays `Hidden Player` (R99). Remembered-name sources are map uuids, never `/Game/…` paths, on cards, in Top 3 and in the match view. Known risk (R74, extended to Top 3 names by R94): match-details has no incognito flag, so a match the user didn't play relies on "hidden on the live screen now, or no stored name". Tests: T14 `hovered_agent_is_not_selected`, `hidden_players_ignore_hover`, `remembered_name_source_is_map_uuid` + `top3_remembered_source_is_map_uuid`; T15 `user_absent_hides_live_incognito_players`, `hidden_now_rows_use_display_name_in_every_view` (R93) + `remembered_source_is_uuid_in_match_view`; T19 checklist item 12 (the user's DB compared with the core-game dump, R94). The spec §8.3.9 DOM scan stays in T17/T18 as a regression check only. It is not evidence here: the DEV scenes carry no real hidden names, so it can't fail (R96).
2. **Rust↔TS contract drift.** Tests: the T15 golden JSON (with a uuid `fromMapId` and one resolving loadout weapon); T16b `contract.test.ts` + `contract.test-d.ts`, where each descriptor is declared with `satisfies ShapeOf<X>`, key sets are asserted equal, and one `@ts-expect-error` case proves the guard can fail (R96); loading vs missing (key absent → skeleton, `null` → `—`) in `ingame-partial`, `details-partial` and the loadout weapons (R97).
3. **Fit at 1000×800 in all 3 fonts.** No scroll in any Live stage. Covers: longest names, the widest Peak row (`Ascendant 3` + `V26 A4 · 11mo`), a 3-digit ⟲ count, 4 party groups, the tier name vs the party tab (right edge ≤ 463), long Top 3 names, and no horizontal overflow in Details/Match. Also at `deviceScaleFactor: 1.5`. Tests: T17 `fit-live` + `fit-dpr`, T18 `fit-details`.
4. **Versioned migrations only (R89).** Migration 1 is frozen at `c9ad7f4`; Phase 2's 4 columns are migration 2; a `user_version` 1 DB upgrades to 2 at open. Tests: T15 `migration1_is_frozen`, `fresh_db_has_phase2_columns_at_user_version_2`, `v1_db_upgrades_to_user_version_2`, and `migrations_idempotent` (now expecting 2).
5. **Lightness.** Covers:
   - cardLarge 160px / chroma 256px / contentTier 32px downscales;
   - pregame loadouts polled only until valid (≤ 1 call per re-poll, 0 after a valid answer);
   - skin names fetched once and cached;
   - the user's own loadout: 1 pd call per connect (R90);
   - `comp_matches` limits the player's rows before any join, with no per-row subquery (R88);
   - the own-history backfill is bounded and runs behind live cards (R101);
   - installer ≤ 10MB; no demo code, font CDN or unpkg in `ui/dist`.

   Checks: T14/T15 asset tests; T15 `pregame_loadouts_polled_once_then_stops`, `weapon_skin_fetched_once_then_cached`, `self_loadout_fetched_once_per_connect`, `comp_matches_has_no_correlated_subqueries`; T15b `backfill_first_run_respects_bound`, `backfill_never_delays_live_card_sync`; the T16a/T16b/T19 `ui/dist` guards.

---

### Task 12: Review of Task 9a (My Stats, Rust half) (complete — commits 37b9631..c9ad7f4, ledger R73)

Done before Phase 2 started (R87): the review was Approved, the math review ran, and fix round 1 (R73) landed. `main` = `origin/main` = `c9ad7f4` (ledger progress.md:302–314). Nothing in this task runs, and it is in no lane.
- Its baseline check (ui build, 298 tests, `clippy -D warnings`) moved to Task 14 Step 0.
- The `comp_matches` per-row subqueries (ledger :303) are fixed in Task 15 Step 3 (R88).
- The `current_act()` rollover gap (ledger :307, :309) is re-deferred explicitly in Task 13 Step 3b (R88). The user said act-start handling comes later.

### Task 13: Docs, project rules and repo hygiene

**Files:**
- Modify:
  - `CLAUDE.md`
  - `docs/requirements-raw.md`: append the listed markers to existing lines, plus one new line at the end (R91). Never reword the user's lines.
  - `docs/plan.md` (stale-line notes outside Tasks 7–11, which this Phase 2 already marked)
  - `docs/handoff.md`: one "superseded" line at the top (audit §5.5). The rest stays as history; the next `/handoff` replaces the file (Task 19 Step 7).
- Create:
  - `README.md`, `CHANGELOG.md`
  - `docs/ARCHITECTURE.md`, `docs/DECISIONS.md`
  - `docs/design-spec.md`: a byte copy of `.superpowers/design/cd-v8/spec.md` (text only, ~70KB; the design folder stays uncommitted).
  - `AGENTS.md`, `LICENSE`, `THIRD-PARTY-NOTICES.md`
  - `.editorconfig`, `.gitattributes`
- Disk only, never committed: `.superpowers/sdd/plan/progress.md` (the ledger, Step 3b).
- Commit: the 37 deletions already staged (`docs/design-history/**`, `docs/architecture.visual-check.*`) go into the same docs commit. They are all byte-identical in `ارشيف/docs/`.

**Interfaces:**
- Consumes:
  - audit §2.1–§2.14 (the exact CLAUDE.md text blocks), except §2.12, which is replaced by R74's text (Step 3);
  - audit §2.15 (the marker table), plus the corrected §2.4/§2.14 markers in Step 3;
  - the ledger's `Ruling` lines R1–R73 and R87–R101, plus R74–R86 below;
  - `git log --reverse --format='%h %ad %s' --date=short`;
  - `gh repo view --json owner` → owner `opd116`.
- Produces: documentation only. There is no code change.
- New rulings. This task writes them to the ledger (`- Ruling Rn: <what> — <why> — cost if wrong: <cost>.`) and to `docs/DECISIONS.md`. R74–R86 are reserved for this table: the ledger already holds R87–R101, and those cite R74–R86 by number. Never renumber them.

  | # | What | Why | Cost if wrong |
  |---|---|---|---|
  | R74 | Match Details names: in a match the user did not play, a row shows as its agent name (or the remembered name) when its puuid is incognito on the live screen now or its stored name is empty; otherwise Visible. This replaces audit §2.12's "flagged incognito in that match". | match-details has no incognito flag (`endpoints.rs` `MatchPlayer`), so §2.12 can't be implemented. | If match-details returns real names for players who were incognito in that match, those names show for players not on screen now, and the same risk covers Top 3 names built from the details player's own history (R94). Task 19 item 12 checks this against the user's DB. The follow-up would hide every name in matches the user didn't play, and every Top 3 name learned only from such matches, except players visible on the live screen now. |
  | R75 | Live columns are fixed 5-slot tracks in every stage, and a card is always 485×139.8. In Agent Select the right column holds 5 dashed card-size slots. `closed` with no known self shows an empty grid and no skeleton. | The card's inner grid is tuned to ~140px, and CLAUDE.md wants one identical card. A skeleton that never resolves breaks Review Focus 2. | Lobby 1 leaves the rest of the column empty. |
  | R76 | 4 party colours per accent (spec §1.6 table and rule). | There can be up to 4 real groups (2 ally + 2 enemy), and the design has only 2 colours. | Two added colours. |
  | R77 | Top bar: an `Avg party rank` block (the mean of the ranked members' tiers, .5 rounds up; skeleton and `—` states; tip `From {count(n, 'ranked player')}`). The `My Stats` tab stays inert until the page exists. | R71 puts APR in the top bar, and the design has no APR. A clickable tab leading to a missing page would be a dead end. | Label wording. |
  | R78 | Tooltips the design leaves unbound or lacks: K/D and HS% `Last 5 competitive`; the Win% structured tip with row 2 `This act`; hero Games and W / L `This act`; the loadout well `{Tier} Edition`; the act name in the Peak tip; the Peak tip on the whole group. Tips hide on click, navigation and scroll. Counts use singular forms. | Scope lives only in tooltips (R67), and CLAUDE.md puts extra detail behind hover. | Extra tips. |
  | R79 | Type and assets: weight 400 is dropped (hero `Prev —` at 500); ⟲ was an inline SVG (Lucide `rotate-ccw`); R149 superseded that, so it is the design's Segoe UI Symbol glyph and there is no Lucide notice; `cardLarge` is 160px wide (not the audit's ~128, because 160 covers the 109px hero at 150% DPI); `contentTier` 32px; `chroma` 256px. | Lightness. (The "Segoe glyph exists only on Windows" reason was dropped by R149: the app is Windows-only.) | A few KB. |
  | R80 | States built from the design's parts: the rank `null` box; Details and Match loading skeletons; meta pending → skeleton bars, meta miss → `—` (spec §5.8). | The design has none of these states, and skeletons carry no text. | Small UI tweaks. |
  | R81 | Per-match rank = Riot's at-match `competitiveTier`, falling back to `comp_updates.tier_after`. One rule for recent rows, encounters and the scoreboard. `queueLabel` table (`''` → `Custom`). Perspective: the user's team first, else the focus player's team. | One scoreboard must not mix post-match and in-match ranks. | A promotion game shows the rank the player had during it. |
  | R82 | Text that can't fit in Details: Top 3 names wrap anywhere and never truncate. The encounters header name, the loadout skin line and the recent-row map name keep the design's ellipsis. The ⟲ slot is `min-width:36px` and grows for counts of 3+ digits. | CLAUDE.md asks for Top 3 with full names; the other ellipses are the design's own; the count must never overlap the tag. | A long Top 3 name takes 2 lines. |
  | R83 | Other players' loadouts are readable only In Game and for Agent Select allies; outside those stages their weapons are `—`. The user's own loadout comes from `personalization/v2/players/{puuid}/playerloadout` once per connect (pre-flight R90), so it shows everywhere. Skin names resolve from valorant-api when Details opens, independent of the 120s Riot details sync; while a name resolves, the weapon key is absent and the UI draws a skeleton well (R97). | Lightness: one extra call per connect, no endpoint for other players. A loadout that arrives after the first open must still get its names. | Other players' loadouts stay blank in the Lobby. |
  | R84 | Remembered-name map ids are resolved when the DisplayName is built (publish, Details, match view) through `AppState::map_uuid`, never when the DB is read. One helper does the conversion for all three call sites: `KnownName::with_map_uuid` (R95). | Same reason as `LiveState.map_id`: the asset tables can load after the DB read (first start, offline). | None. |
  | R85 | The Phase 2 columns ship as migration 2 (4× `ALTER TABLE … ADD COLUMN`). Migration 1 is frozen, and there is no self-heal (pre-flight R89 replaced the self-heal this plan first proposed). | CLAUDE.md requires versioned migrations (`PRAGMA user_version`). Each column is defined once, and an earlier build's DB upgrades through the normal runner. | None: no install exists yet. |
  | R86 | Process: the build spec is committed as `docs/design-spec.md`; every commit is path-limited (`git commit -m … -- <paths>`); CHANGELOG is filled at release from `git log`. Task 12 is closed as complete (R87), and the lanes run as R98 sets them. | CLAUDE.md wants every decision recorded, and the user wants every step pushed. CHANGELOG gets one writer across the lanes. | None. |

- [ ] **Step 0:** Load `engineering:documentation` (README / ARCHITECTURE structure). Take code facts for `docs/ARCHITECTURE.md` from graft (`graft ask … --source`, `graft skeleton`), not from memory.
- [ ] **Step 1:** Save this check to your scratchpad as `check_task13.sh`. Don't commit it.
  ```bash
  cd /home/project-1; fail=0
  chk(){ if bash -c "$2"; then echo "ok   $1"; else echo "FAIL $1"; fail=1; fi; }
  chk repo_files 'for f in README.md CHANGELOG.md docs/ARCHITECTURE.md docs/DECISIONS.md docs/design-spec.md AGENTS.md LICENSE THIRD-PARTY-NOTICES.md .editorconfig .gitattributes; do test -s $f || exit 1; done'
  chk design_spec 'grep -q "^## 8. Verification" docs/design-spec.md && grep -q "docs/design-spec.md" CLAUDE.md && grep -q "docs/design-spec.md" AGENTS.md'
  chk claude_design_source 'grep -q "OPD1 Tracker v8.dc.html" CLAUDE.md'
  chk claude_precedence 'grep -q "Precedence: a newer dated user decision" CLAUDE.md'
  chk claude_one_live_page 'grep -q "5 empty card-shaped slots" CLAUDE.md'
  chk claude_last5_right 'grep -q "oldest → newest left to right" CLAUDE.md'
  chk claude_match_details 'grep -q "opens \*\*Match Details\*\*" CLAUDE.md'
  chk claude_match_names 'grep -q "incognito on the live screen now" CLAUDE.md && ! grep -q "flagged incognito in that match" CLAUDE.md'
  chk claude_lock_only 'grep -q "hovering doesn.t count" CLAUDE.md && ! grep -q "hovered or locked" CLAUDE.md && ! grep -q "only after the agent is selected" CLAUDE.md'
  chk claude_own_level 'grep -q "except on the user.s own card" CLAUDE.md'
  chk claude_backfill 'grep -q "Own history backfill" CLAUDE.md'
  chk claude_spec_scope 'grep -q "goes to the user first (R92)" CLAUDE.md'
  chk claude_later_pages 'grep -q "Match History page, Settings page" CLAUDE.md'
  chk claude_indicators 'grep -q "^\*\*Card indicators\*\*" CLAUDE.md'
  chk claude_fonts 'grep -q "Fonts are bundled locally" CLAUDE.md'
  chk claude_defaults 'grep -q "tone Neutral, accent #e3a54f" CLAUDE.md'
  chk claude_demo_r40 'grep -q "no Rust feature, R40" CLAUDE.md'
  chk claude_start_live '[ "$(grep -c "on Live with the user" CLAUDE.md)" -eq 1 ]'
  chk claude_push_rule 'grep -q "pushed to the private GitHub repo" CLAUDE.md'
  chk claude_changelog 'grep -q "filled at release from" CLAUDE.md'
  chk claude_migration 'grep -q "existing migrations are frozen" CLAUDE.md && ! grep -q "self-heals" CLAUDE.md'
  chk claude_uninstall 'grep -q "Delete app data" CLAUDE.md'
  chk name_spelling '! grep -n "OPD1 TRACKER" CLAUDE.md README.md AGENTS.md CHANGELOG.md docs/ARCHITECTURE.md docs/DECISIONS.md'
  chk decisions_r1_r101 'for i in $(seq 1 101); do grep -Eq "^### R$i( |$)" docs/DECISIONS.md || exit 1; done'
  chk decisions_user_index 'grep -q "^## User decisions index" docs/DECISIONS.md'
  chk ledger_r74_r86 'for i in $(seq 74 86); do grep -q "Ruling R$i:" .superpowers/sdd/plan/progress.md || exit 1; done && [ "$(grep -n "Ruling R86:" .superpowers/sdd/plan/progress.md | cut -d: -f1)" -lt "$(grep -n "Ruling R87 " .superpowers/sdd/plan/progress.md | cut -d: -f1)" ]'
  chk ledger_redeferred 'grep -q "Re-deferred (R88): current_act() rollover" .superpowers/sdd/plan/progress.md'
  chk changelog 'grep -q "^## \[Unreleased\]" CHANGELOG.md && grep -q "keepachangelog.com" CHANGELOG.md'
  chk license 'grep -q "All rights reserved" LICENSE && grep -q "opd116" LICENSE && grep -q "THIRD-PARTY-NOTICES" LICENSE'
  chk third_party 'grep -q "OFL" THIRD-PARTY-NOTICES.md && ! grep -qi "lucide" THIRD-PARTY-NOTICES.md'
  chk readme 'grep -q "not endorsed by Riot Games" README.md && grep -q "More info" README.md && grep -q "Run anyway" README.md && grep -q "docs/screenshots/" README.md'
  chk agents 'grep -q "CLAUDE.md" AGENTS.md && grep -q "git push" AGENTS.md && grep -q "opd1-trcker" AGENTS.md'
  chk req_markers '[ "$(grep -c "(ملغي ←" docs/requirements-raw.md)" -ge 8 ] && [ "$(grep -c "(محسوم ←" docs/requirements-raw.md)" -ge 2 ]'
  chk req_marker_lines 'sed -n 5p docs/requirements-raw.md | grep -q "الأحدث يمين" && ! sed -n 9p docs/requirements-raw.md | grep -q "الأحدث يمين" && sed -n 9p docs/requirements-raw.md | grep -q "(ملغي ← سطر 95-97" && sed -n 44p docs/requirements-raw.md | grep -q "(مؤقتاً ← سطر 105)" && sed -n 105p docs/requirements-raw.md | grep -q "(الـ hover ملغي ← سطر 107)"'
  chk req_claude_calls 'sed -n 107p docs/requirements-raw.md | grep -q "قرارات المستخدم بعد مراجعة" && tail -n 1 docs/requirements-raw.md | grep -q "R74–R86" && [ "$(grep -c "R74–R86" docs/requirements-raw.md)" -eq 1 ]'
  chk plan_notes 'sed -n 39p docs/plan.md | grep -q "(ملغي ← 2026-09-29"'
  chk handoff_superseded 'head -3 docs/handoff.md | grep -q "Superseded (2026-09-29)"'
  chk gitattributes 'grep -q "text=auto eol=lf" .gitattributes && grep -q "\*.nsh" .gitattributes && grep -q "\*.woff2 binary" .gitattributes'
  chk staged_deletions '[ "$(git diff --cached --diff-filter=D --name-only | wc -l)" -eq 37 ]'
  exit $fail
  ```
- [ ] **Step 2:** Run `bash check_task13.sh`. Expected: FAIL. Only `staged_deletions` and `name_spelling` pass (`plan_notes` checks line 39 itself, because the marker string already appears elsewhere in this plan; `ledger_redeferred` looks for the exact Step 3b wording, because R88's own ledger line already says "current_act() rollover").
- [ ] **Step 3:** Write the docs.
  - **`CLAUDE.md`:**
    - Apply audit §2.1, §2.2, §2.3, §2.4 (Q1 answered: newest on the right), §2.5, §2.6, §2.7, §2.8, §2.9, §2.10, §2.11, §2.13 and §2.14's Demo line, verbatim, except for these three lines of §2.11, which the user's 2026-09-29 answers changed (R99, R100):
      - "- Until the player locks an agent (only possible in Agent Select): `Hidden Player` in the hidden-name style, empty agent slot (never a placeholder agent)."
      - "- Once the player locks an agent (hovering doesn't count — R99, user 2026-09-29): the agent name, no tag."
      - "- Account level shows `—` only when Riot itself withholds it (HideAccountLevel) — a separate player setting, not part of the hidden name — except on the user's own card, which always shows the user's own level (R100)."
    - CLAUDE.md:68 becomes: "- Agent **head icon** (`displayIcon`) only after the agent is locked (hovering doesn't count — R99). Never show a placeholder agent."
    - Instead of §2.12, add under Hidden names: "- Match Details: in every view, a player incognito on the live screen now shows as on their card: the remembered name when the user's own finished match knows it, else the agent name (R93). Otherwise, a match the user played shows names as match-details gives them; in a match the user did not play, a player with no stored name shows as the agent name (or the remembered name). Names from matches the user did not play are never stored as known names (R74 — match-details has no incognito flag; a check of the user's DB after the first real test confirms Riot hides those names, R94)."
    - §2.14's Closed line, once, at CLAUDE.md:48: "(Until My Stats is built, open on Live with the user's own card — R71.)"
    - After the §2.1 block: "- Build spec: `docs/design-spec.md` (sizes, states, tokens and tests derived from the design; its [C]/[bound] items are Claude calls R74–R86 in `docs/DECISIONS.md`). Keep it in step with the code. It outranks the design file only for those [C]/[bound] gap-fills; if it disagrees with the design on something the design covers, the design wins and the spec is fixed, and any change to a visual the design covers goes to the user first (R92)."
    - Then add today's decisions:
      - Under **Build and distribution**:
        - "- DB schema: every change is a new versioned migration appended to `MIGRATIONS` (`PRAGMA user_version`); existing migrations are frozen, migration 1 included (R89, 2026-09-29). No self-heal code, no in-place edits."
        - "- The uninstaller's optional "Delete app data" checkbox (off by default) removes everything, the DB included — accepted as the user's explicit choice (2026-09-29). A normal reinstall never touches the DB."
      - Under **Smart data (SQLite)**:
        - "- **Own history backfill (R101, user 2026-09-29):** on connect and after each match, page the user's own match history (all queues) back to the last match a finished backfill confirmed, and fetch match-details only for unknown ids. Throttled, lowest priority, behind live cards. The first run stops at the current act start or 200 matches."
      - Under **Working with the user**:
        - "- Every step is committed with a clear conventional message and pushed to the private GitHub repo right away (2026-09-29). Commit explicit paths only (`git commit -m "…" -- <paths>`); never commit `.superpowers/` or root `*.zip`."
        - "- Repo files: README.md, CHANGELOG.md (Keep a Changelog; the release section is filled at release from `git log`), docs/ARCHITECTURE.md, docs/DECISIONS.md, docs/design-spec.md, AGENTS.md, LICENSE (proprietary), THIRD-PARTY-NOTICES.md."
  - **`docs/requirements-raw.md`:** append the audit §2.15 markers to :5, :15, :16, :27, :66, :71, :76, :78 and :89 at the line ends.
    - Also add the §2.4 marker `(ملغي ← تصميم v8: الأحدث يمين)` to :5 only. It is the only line with the newest-first last-5 order; :7's newest-first is Recent Matches and still holds, and :9 has no last-5 order.
    - Add `(ملغي ← سطر 95-97: ترتيب Lobby = العمود اليسار بنفس تصميم In Game)` to :9. Only its "the cards adapt their order and size to the count" clause is superseded.
    - Add `(مؤقتاً ← سطر 105)` to :44 (closed opens on My Stats; Live until My Stats exists).
    - Add the §2.14 marker `(ملغي ← 2026-09-29: المستخدم يبي يستخدم In Game و Agent Select الحين)` to :55.
    - Add `(الـ hover ملغي ← سطر 107)` to :105. Line 107 (the user's answers of 2026-09-29) reverses only its hover clause (R99).
    - Don't edit any other line of the 2026-09-29 block (line 92 to the end). Append exactly one new line after the last line (:107), in Arabic and dated, listing the user-visible Claude calls R74–R86 (R91; it shifts no line number the check uses). Write it as: `- (2026-09-29، قرارات Claude في التصميم R74–R86، مسجلة في docs/DECISIONS.md) تفاصيل مباراة ما لعبتها: اللاعب المخفي الحين في الشاشة أو اللي ما له اسم محفوظ يطلع باسم الإيجنت. البطاقة نفس المقاس دايماً و5 خانات ثابتة في كل مرحلة (في Lobby الصغير الباقي فاضي). 4 ألوان للبارتيات بدل 2. "Avg party rank" في الشريط العلوي، وتبويب My Stats ما يسوي شي لين تنبني الصفحة. شروحات إضافية تطلع بالـ hover (مثل Last 5 competitive و This act). رانك اللاعب في كل مباراة = رانكه وقت المباراة نفسها. أسماء Top 3 تنزل سطر ثاني بدل ما تنقص. سكنات الأسلحة للاعبين الثانيين تطلع بس في Agent Select و In Game، وسكناتك انت تطلع دايماً. الداتا بيس تتحدث بخطوة جديدة (migration 2) بدل ما نعدل الأولى.`
  - **`docs/plan.md`:** append these notes at the line ends:
    - :30 `(Phase 2: no scrolling in any Live stage)`
    - :34 `(R40: no Rust demo feature; DEV-only UI feed)`
    - :35 `(2026-09-29: the uninstaller's optional "Delete app data" may remove the DB — accepted)`
    - :39 `(ملغي ← 2026-09-29: المستخدم يبي يستخدم In Game و Agent Select الحين)`
    - :58 `(Phase 2: the name part truncates with …, the tag stays whole — spec §3.4)`
    - :264, inside a code block: ` // R32: {result, rrEarned|null, startMs}; full wire types in spec §7.1`
    - :271 `(R11: served through the img:// scheme)`
    - the `## File Structure` heading: ` (UI part superseded by Phase 2, Tasks 16a–18)`
  - **`docs/design-spec.md`:** `cp .superpowers/design/cd-v8/spec.md docs/design-spec.md`. No edits in this task (R74–R86 are reserved, so there is nothing to renumber).
  - **`docs/handoff.md`:** insert as line 1: `> **Superseded (2026-09-29).** Current status lives in docs/plan.md (Phase 2), docs/DECISIONS.md and README.md. The next /handoff replaces this file.` followed by a blank line.
  - **`README.md`:**
    - What it is: one paragraph. Note the name is `OPD1 TRCKER` on purpose.
    - Screenshots from the placeholder paths `docs/screenshots/{live-ingame,live-agent-select,live-lobby,player-details,match-details}.png`. They resolve only after Task 19's merge, which creates the files; until then the README shows broken images, which is expected.
    - Features.
    - Install for friends:
      - download the `OPD1-TRCKER-setup` artifact from GitHub Actions and unzip it;
      - run it; on SmartScreen click **More info → Run anyway**;
      - it installs per user;
      - a reinstall keeps the history;
      - the uninstaller's "Delete app data" removes it.
    - Build from source:
      - Windows: Rust stable, Node 24, WebView2, then `npm ci`, `npx tauri build`.
      - Linux dev: `libwebkit2gtk-4.1-dev libsoup-3.0-dev librsvg2-dev`, then `npm ci`, `npm --prefix ui run dev` (DEV scenes `?scene=`), and the Phase 2 test commands.
    - Project structure (the crates, `src-tauri`, `ui`, `docs`).
    - Stack.
    - Status / roadmap: v0.2.0 scope; next up: My Stats page, Match History, Settings.
    - Privacy: local only; never requests hidden names.
    - Licence: proprietary (`LICENSE`); third-party notices in `THIRD-PARTY-NOTICES.md`.
    - Disclaimer: "OPD1 TRCKER is not endorsed by Riot Games and doesn't reflect the views or opinions of Riot Games or anyone officially involved in producing or managing Riot Games properties. Riot Games and all associated properties are trademarks or registered trademarks of Riot Games, Inc."
  - **`CHANGELOG.md`:**
    - The Keep a Changelog 1.1.0 header (link `https://keepachangelog.com/en/1.1.0/`) + SemVer.
    - `## [Unreleased]`, with `### Added` / `### Fixed` / `### Changed` backfilled from `git log` (6e9655f..HEAD, one line per user-visible change; skip `[skip ci]` doc-only commits). Later tasks don't edit it; Task 19 fills the release section from `git log` (R86).
  - **`docs/ARCHITECTURE.md`:**
    - Crates and responsibilities.
    - Data flow: the poll stage machine → cached-first sync → `emit("live")`; the `"details"` event.
    - Commands: `meta, live_state, player_details, my_stats, copy_log, dump_raw`. Task 19 adds `match_details` here once lane B has landed.
    - DB schema: migration 1, with `PRAGMA user_version`. Task 19 adds migration 2 (R89), the user's own loadout (R90) and the own-history backfill (R101), because lane A owns `docs/` while lane B runs.
    - Caching / sync ("sync, don't refetch", the MMR gate).
    - Party detection (R53/R55).
    - Hidden names (R16/R18/R43/R49/R74).
    - The `img://` scheme (R11/R21) and throttling.
    - Link to `docs/architecture.html`.
  - **`docs/DECISIONS.md`:**
    - One `### Rn — <short title>` per ruling R1–R101, with **What / Why / Cost / Status**. Status is one of `active`, `superseded by Rn`, `narrowed by Rn`, `reversed by the user (date)`, `dropped`, or `unused number` (R38).
    - Mark R61 → R68, R62 → R67, R65 → R69, R51 → R70, and R40 (no demo feature). R72 is `active` (deferred to the My Stats phase); R73 is the Task 9a fix round. R74–R86 come from the table above. R87–R101 come from their ledger lines.
    - Also mark: R31 `superseded by R89` (migration 1 is frozen); R71 `partly reversed by the user (2026-09-29, R99)` (hover no longer counts as selected; its other three calls stay active); R33 `narrowed by R101` (the user's own history is backfilled beyond the per-player window); R83 `narrowed by R90` (the user's own loadout shows everywhere). R88's entry names the `current_act()` rollover as re-deferred (user 2026-09-29: act-start handling later).
    - Then `## User decisions index`: date → the `docs/requirements-raw.md` heading → a one-line summary.
  - **`AGENTS.md`** (for non-Claude agents):
    - Read `CLAUDE.md` first; it is binding. The build spec is `docs/design-spec.md`.
    - Build/test commands: the Phase 2 Global Constraints commands, plus `npx tauri build`.
    - Commit+push rule, path-limited commits (`git commit -m … -- <paths>`), CI watched by commit sha.
    - Never commit `.superpowers/` or zips.
    - Spelling `OPD1 TRCKER`.
    - Lanes (Phase 2, R98): `main` in `/home/project-1` = docs and the release; `p2-backend` in `/home/project-1/.worktrees/p2-backend` = crates, src-tauri and CI; `ui-lane` in `/home/project-1/.worktrees/ui` = ui. Start every command in a worktree with an explicit `cd <worktree> &&`.
  - **`LICENSE`:** "Copyright (c) 2026 opd116. All rights reserved." Proprietary: no use, copy, modification or distribution without written permission. Third-party components keep their own licences; see `THIRD-PARTY-NOTICES.md`.
  - **`THIRD-PARTY-NOTICES.md`:**
    - No Lucide notice (R149): the ⟲ encounter badge is the design's own Segoe UI Symbol glyph, so no icon geometry is bundled. The notice this plan first listed was removed once nothing from Lucide remained in `ui/src`.
    - Barlow, IBM Plex Sans, Archivo: SIL Open Font License 1.1; full texts in `ui/src/assets/fonts/OFL.txt`. Task 16a adds that file on `ui-lane`, so the path resolves on `main` only after Task 19's merge; that is expected.
  - **`.editorconfig`:**
    - `root = true`;
    - `[*]` utf-8, lf, final newline, trim trailing whitespace;
    - `[*.{rs,toml}]` 4 spaces;
    - `[*.{ts,js,mjs,svelte,json,css,html,yml,yaml,md}]` 2 spaces;
    - `[*.md]` `trim_trailing_whitespace = false`;
    - `[*.{bat,cmd,ps1,nsh}]` `end_of_line = crlf`.
  - **`.gitattributes`:**
    - `* text=auto eol=lf`;
    - `*.bat *.cmd *.ps1 *.nsh text eol=crlf` (one pattern per line);
    - `binary` for `*.png *.ico *.icns *.woff2 *.sqlite *.zip`.
- [ ] **Step 3b (ledger, disk only):** edit `.superpowers/sdd/plan/progress.md` (the controller holds its own ledger writes until this step reports):
  - Insert `- Ruling R74: …` through `- Ruling R86: …`, one line each from the table above, directly above the `- Ruling R87 (pre-flight A)` line, so the ledger reads in number order.
  - Append `- Deferred minors re-deferred (2026-09-29): double sync (progress.md:188) and the connect-backoff reset (:209) stay deferred. Both cost lightness, not correctness: a double sync repeats idempotent upserts of immutable match data, and the backoff reset only reconnects more often while presence keeps failing. Neither can create a stripe once Task 14 gates party marks on a successful history fetch. Revisit with the poll.rs split.`
  - Append `- Re-deferred (R88): current_act() rollover — current_act() picks the act by valorant-api date (state.rs:537, roster.rs:446), so for a few hours around an act change the live card can show — for rank and Win% until Riot rolls over. The ledger pointed it at Task 14 (:307, :309); it is not in Phase 2 (user 2026-09-29: act-start handling later).`
  - Then fix the order (audit §5.4): split line :211 into the 5b note and the R52 ruling, and move the R54 line above R55.
- [ ] **Step 4:** Run `bash check_task13.sh`. Expected: PASS.
  - Then run the read-only line-ending check `git ls-files --eol | awk '$1 ~ /^i\/(crlf|mixed)/'`. Expected: no output (true on 2026-09-29). Any path it prints had CRLF in the index: report it as a concern, and don't renormalize it in this commit.
- [ ] **Step 5: Commit + push** `docs: phase 2 rules, repo files, design-history cleanup` →
  ```bash
  cd /home/project-1
  P="CLAUDE.md docs/requirements-raw.md docs/plan.md docs/handoff.md docs/design-spec.md README.md CHANGELOG.md docs/ARCHITECTURE.md docs/DECISIONS.md AGENTS.md LICENSE THIRD-PARTY-NOTICES.md .editorconfig .gitattributes"
  git add $P
  git commit -m "docs: phase 2 rules, repo files, design-history cleanup" -- $P docs/design-history 'docs/architecture.visual-check.*'
  ```
  - Check that `git show --name-only --format= HEAD | grep -cE "design-history|architecture.visual-check"` = 37 and `git diff --cached --name-only` is empty.
  - Then the main porcelain check (Global Git), then `cd /home/project-1 && git push origin main`.
  - Watch CI (Global "CI watch", `main`), because `.gitattributes` changes the Windows checkout. Expected: success. Task 15 Step 0 waits for this commit on `origin/main`.

### Task 14: Backend contract delta A (live card) + CI

**Lane B.** Directory: `/home/project-1/.worktrees/p2-backend`, branch `p2-backend` (created in Step 0). Every command starts with `cd /home/project-1/.worktrees/p2-backend &&`. Runs in parallel with Task 13 (lane A) and Task 16a (lane C); writes only `crates/**`, `src-tauri/**` and `.github/workflows/build.yml`.

**Files:**
- Modify:
  - `crates/core/src/model.rs`: `PlayerCard.{is_self, act_wins, act_games}`, `DisplayName::Remembered` source, `BuildCardInput.{is_self, remembered}`, `shown_agent` lock-only (R99).
  - `crates/core/src/names.rs`: `KnownName`, `KnownName::with_map_uuid` (R95), `display_name`.
  - `crates/core/src/db/repo.rs`: `known_name` returns `KnownName`.
  - `crates/riot/src/endpoints.rs`: `SeasonalInfo.number_of_wins_with_placements`.
  - `crates/riot/src/to_core.rs`
  - `crates/riot/src/assets.rs`: `AssetKind::CardLarge`, `largeArt`.
  - `src-tauri/src/imgproto.rs`: `cardLarge`.
  - `src-tauri/src/store.rs`: `trim_mmr`, `MMR_SNAPSHOT_VERSION`, `mmr_refresh_needed`, `PlayerDb.remembered: Option<KnownName>` (the stored Riot path). `load_player_db` keeps its signature, so its callers (`lib.rs:61`, `sync.rs:324`, `details.rs:132`, `state.rs:667` `reload_slot`, `poll.rs:686/736/764`) don't change (R84).
  - `src-tauri/src/sync.rs`: `SyncOutcome`, the forced MMR refresh, `party_ready`.
  - `src-tauri/src/state.rs`: `card_for` sets `is_self`, maps the remembered source with `KnownName::with_map_uuid`, and shows the user's own level even when hidden (R100); `LiveModel::build` and `card_for` take the `map_uuid` lookup; `publish()` passes `&|p| self.map_uuid(p)`; `LiveModel.party_ready`; the test module's `build(&[], 0)` calls and the `build(&[], card_clock(now_ms()))` call at state.rs:1203 gain `&|_| None`.
  - `src-tauri/src/details.rs`: `teammate_name` takes `&HashMap<String, KnownName>` plus the lookup and converts with `KnownName::with_map_uuid`; both the `card_for` call and the `teammate_name` call pass `&|p| st.map_uuid(p)`.
  - `src-tauri/src/poll.rs`: `party_ready` is cleared where `synced` is cleared (poll.rs:224 and :455) and removed where `synced` loses a puuid (poll.rs:818).
  - `.github/workflows/build.yml`
- Tests:
  - `crates/core/tests/{card,names,db}.rs`
  - `crates/riot/tests/{to_core,assets}.rs`
  - Create `crates/riot/tests/fixtures/mmr_placements.json`: a copy of `mmr.json` with `"NumberOfWins": 34, "NumberOfWinsWithPlacements": 39, "NumberOfGames": 61`.
  - Create `crates/riot/tests/fixtures/mmr_no_placements.json`: a copy of `mmr.json` with the `NumberOfWinsWithPlacements` key removed (`NumberOfWins` 34, `NumberOfGames` 61). `mmr.json` itself can't test the fallback (its two win counts are both 34), and `mmr_nulls.json` has `SeasonalInfoBySeasonID: null`; it stays for the empty-map case.
  - `src-tauri/src/{imgproto,store,state,details}.rs` (test modules)
  - `crates/core/tests/card.rs`'s `card_hides_agent_until_selected` (card.rs:127) is rewritten for R99: only `"locked"` shows the agent.
  - `src-tauri/src/pipeline_tests.rs`. The harness's valorant-api base becomes a second wiremock server instead of the unreachable `http://127.0.0.1:1` (pipeline_tests.rs:250). With no routes mounted it answers 404, which behaves like before for every existing test. A helper `mount_asset_tables(&server)` mounts `/v1/version`, `/v1/agents?isPlayableCharacter=true`, `/v1/maps`, `/v1/competitivetiers` and `/v1/seasons` with minimal data (reuse the shapes from `crates/riot/tests/assets.rs`). The maps entry is Split, uuid `d960549e-485c-e861-8d71-aa9d1aed12a2`, `mapUrl` `/Game/Maps/Bonsai/Bonsai`; the seasons entry has a current act spanning the stored matches. Tests that need tables mount them.

**Interfaces:**
- Consumes: spec §7.1 and §7.3 items 1–4; audit §3A 1–5 and §3D (false-stripe minor, ledger progress.md:219); R95, R99, R100.
- Produces (Rust):
  ```rust
  // crates/core/src/names.rs
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct KnownName { pub name: String, pub tag: String, pub map_id: Option<String>, pub start_ms: Option<i64> }
  impl KnownName {
      /// R84/R95: the one Riot-path → valorant-api-uuid conversion of a remembered source. `map_id` holds
      /// the stored Riot path (`/Game/Maps/…`) until this runs, and a uuid or None after it; it never holds
      /// a path once converted. Called exactly once, where the DisplayName is built: `card_for`,
      /// `teammate_name` (this task) and `build_match_view` (Task 15). No other code converts it.
      pub fn with_map_uuid(self, map_uuid: &dyn Fn(&str) -> Option<String>) -> KnownName
  }
  pub fn display_name(incognito: bool, visible: Option<(String, String)>, agent_selected: Option<&str>, remembered: Option<KnownName>) -> DisplayName
      // `remembered` arrives already converted (with_map_uuid)
  // crates/core/src/model.rs
  pub enum DisplayName { Visible { name: String, tag: String },
      Remembered { name: String, tag: String, from_map_id: Option<String>, from_start_ms: Option<i64> },
      HiddenPlayer, HiddenAgent { agent_id: String } }
  pub struct PlayerCard { /* … */ #[serde(skip_serializing_if = "Field::is_loading")] pub act_wins: Field<u32>,
      #[serde(skip_serializing_if = "Field::is_loading")] pub act_games: Field<u32>, /* … */ pub is_self: bool }
  pub struct BuildCardInput<'a> { /* … */ pub is_self: bool, pub remembered: Option<KnownName>, /* … */ }
  pub fn shown_agent<'a>(character_id: &'a str, selection_state: &str) -> Option<&'a str>
      // R99: Some only when selection_state is "locked" (ASCII case-insensitive); "" and "selected" (hover) → None.
      // Core-game rows pass "locked" (roster.rs:322), so In Game is unchanged.
  // crates/core/src/db/repo.rs — map_id is the stored Riot path; it stays a path in PlayerDb too,
  // and is converted only when a DisplayName is built (R84)
  pub fn known_name(&self, self_puuid: &str, other: &str) -> Result<Option<KnownName>>
  // crates/riot/src/endpoints.rs
  #[serde(rename = "NumberOfWinsWithPlacements", default)] pub number_of_wins_with_placements: Option<i64>
  // crates/riot/src/assets.rs
  AssetKind::CardLarge   // dir "cardLarge"; /v1/playercards/{uuid}.largeArt; downscaled to 160px wide (spec §7.3.4)
  // src-tauri/src/store.rs
  pub const MMR_SNAPSHOT_VERSION: i64 = 2;
  pub fn trim_mmr(resp: &PlayerMmrResponse) -> Value            // + per season "NumberOfWinsWithPlacements", top-level "SnapshotVersion": 2
  pub fn mmr_refresh_needed(repo: &Repo, puuid: &str, latest_comp_match_id: &str) -> Result<bool>  // needs_mmr_refresh || snapshot version < 2
  pub fn load_player_db(repo: &Repo, self_puuid: Option<&str>, puuid: &str) -> Result<PlayerDb>   // unchanged signature
  // src-tauri/src/sync.rs
  pub struct SyncOutcome { pub history_ok: bool }
  pub async fn sync_player(st: &Arc<AppState>, session: &Arc<RiotSession>, puuid: &str) -> Result<SyncOutcome, Gone>
  // src-tauri/src/state.rs
  pub struct LiveModel { /* … */ pub party_ready: HashSet<String>, /* … */ }  // party_marks() filters on it, not on `synced`
  pub fn build(&self, acts: &[Act], now_ms: i64, map_uuid: &dyn Fn(&str) -> Option<String>) -> LiveState
  pub fn card_for(&self, slot: &Slot, acts: &[Act], current_act_id: &str, now_ms: i64, party: Option<PartyMark>,
      map_uuid: &dyn Fn(&str) -> Option<String>) -> PlayerCard
      // remembered: slot.db.remembered.clone().map(|k| k.with_map_uuid(map_uuid))
      // R100: hide_account_level = (r.hide_account_level && !is_self) || r.account_level == 0
  // src-tauri/src/details.rs
  pub fn teammate_name(row: &TeammateRow, hidden_now: &HashMap<String, Option<String>>,
      remembered: &HashMap<String, KnownName>, map_uuid: &dyn Fn(&str) -> Option<String>) -> DisplayName
  ```
- Produces (JSON):
  - `PlayerCard` gains `"isSelf": bool`, `"actWins"?: number|null` and `"actGames"?: number|null`. They are absent while MMR is loading. They are `null` when MMR is missing, when there is no current-act entry, or when games is 0; `actWinrate` is then `null` too.
  - `actWinrate` = `actWins / actGames`, where `actWins` = `NumberOfWinsWithPlacements`, falling back to `NumberOfWins` when that key is absent.
  - `DisplayName` remembered = `{"kind":"remembered","name","tag","fromMapId":uuid|null,"fromStartMs":number|null}`. `fromMapId` is never a `/Game/…` path.
  - `img://localhost/cardLarge/<cardUuid>`.
  - `agentId` is set only after the lock (R99). A hidden player who only hovers stays `{"kind":"hiddenPlayer"}`.
  - `level` is `null` for a player with `HideAccountLevel`, except on the user's own card (`isSelf`), which carries the level (R100). A level of 0 (unknown) stays `null` for everyone.

- [ ] **Step 0:** Set up lane B and run the baseline (R87; this was Task 12's Step 1).
  - Load `test-driven-development` and `tauri-v2`.
  - Create the worktree and branch, then run the baseline:
    ```bash
    git -C /home/project-1 worktree add -b p2-backend /home/project-1/.worktrees/p2-backend main
    cd /home/project-1/.worktrees/p2-backend && test "$(git branch --show-current)" = p2-backend && npm ci && npm --prefix ui run build && cargo test -p opd1-core -p opd1-riot -p opd1-trcker && cargo clippy --workspace --all-targets -- -D warnings
    cd /home/project-1/.worktrees/p2-backend && git push -u origin p2-backend
    ```
    Expected: PASS (298 tests) and 0 warnings. A failure here is not this task's to fix: stop the lane and report it.
  - Confirm every caller before editing with graft (`graft callers card_for`, `graft callers teammate_name`, `graft callers build`, `graft callers shown_agent`, `graft skeleton src-tauri/src/state.rs`). graft reads `/home/project-1`; check each line in the worktree file.
- [ ] **Step 1:** Write the failing tests (contract):
  - `crates/core/tests/card.rs`:
    - `card_is_self_true_only_for_self`: `isSelf` JSON is `true` for `is_self: true`, `false` otherwise.
    - `act_wins_games_tri_state`: Loading → the keys `actWins`/`actGames` are absent. Missing → both `null`. A current act with wins 39 / games 61 → 39, 61, and `actWinrate` ≈ 0.639 (±1e-3).
    - `act_wins_games_null_when_no_games`: games 0 → all three `null`.
    - `remembered_name_serializes_source`: with a source → `fromMapId` + `fromStartMs` present. Without one → both keys present as `null`.
    - `hovered_agent_is_not_selected` (R99): `character_selection_state: "selected"` with a `character_id` → `agentId` is `null`; the same identity with `incognito: true` → name `{"kind":"hiddenPlayer"}`, not `hiddenAgent`. `"locked"` → the agent id, and `hiddenAgent` when incognito.
    - Rewrite `card_hides_agent_until_selected` as `card_shows_agent_only_when_locked`: `""` and `"selected"` → `None`, `"locked"` → `Some("agent-1")`.
  - `crates/core/tests/names.rs`: `display_name_remembered_carries_source`; `known_name_with_map_uuid_converts_path`: `/Game/Maps/Bonsai/Bonsai` with a lookup that knows it → `map_id == Some("uuid-split")`; an unknown path → `None`; `map_id: None` stays `None`; `name`, `tag` and `start_ms` are unchanged. Update the existing tests to use `KnownName`.
  - `crates/core/tests/db.rs`: `known_name_returns_source_match`.
    - Two shared matches (the older on `/Game/Maps/Duality/Duality`, the newer on `/Game/Maps/Bonsai/Bonsai`) → the name of the newer row, with `map_id == Some("/Game/Maps/Bonsai/Bonsai")` and the newer `start_ms`.
    - A name seen only in a stranger's match → `None`.
  - `crates/riot/tests/to_core.rs`:
    - `seasonal_map_counts_placement_wins`: `mmr_placements.json` → `number_of_wins == 39`.
    - `seasonal_map_falls_back_without_placements`: `mmr_no_placements.json` → `number_of_wins == 34`, `number_of_games == 61` (so `actWinrate` = 34/61 ≈ 0.557).
  - `crates/riot/tests/assets.rs`:
    - `card_large_uses_large_art_downscaled_to_160`: a mock `/v1/playercards/{uuid}` with a `largeArt` URL serving a 512×1024 PNG → the cached file decodes to width 160.
    - `playercard_cache_without_large_art_refetches`: a cached playercard JSON written before this change (no `largeArt`) → fetched once more.
  - `src-tauri/src/imgproto.rs`: add `("cardLarge", AssetKind::CardLarge)` to `parses_every_kind`.
  - `src-tauri/src/store.rs`:
    - `trim_mmr_keeps_wins_with_placements_and_version`
    - `old_snapshot_forces_mmr_refresh`: no `SnapshotVersion` → `true` even when `last_comp_match_id` matches; a current snapshot → `false`.
    - `load_player_db_keeps_remembered_riot_path`: `remembered.map_id == Some("/Game/Maps/Bonsai/Bonsai")` (conversion happens later, R84).
  - `src-tauri/src/state.rs`:
    - `card_for_maps_remembered_source`: a lookup mapping `/Game/Maps/Bonsai/Bonsai` → `"uuid-split"` gives `fromMapId == "uuid-split"`; a lookup returning `None` gives `fromMapId: null`; neither JSON contains `/Game/`.
    - `hidden_players_ignore_hover` (R99): an incognito slot with `selection_state: "selected"` and a `character_id` maps to `None` in `hidden_players()`; the same slot with `"locked"` maps to the agent.
    - `own_card_shows_level_even_when_hidden` (R100): the self slot with `hide_account_level: true`, level 87 → the card's `level` is 87; another slot with the same flags → `level: null`; the self slot with level 0 → `null`.
  - `src-tauri/src/details.rs`: the existing `teammate_name` tests take `KnownName` + a lookup; add `teammate_name_maps_remembered_source` (same two cases).
  - `src-tauri/src/pipeline_tests.rs`:
    - `self_card_is_self_and_others_not`
    - `remembered_name_source_is_map_uuid`: with `mount_asset_tables` → `fromMapId == "d960549e-485c-e861-8d71-aa9d1aed12a2"`; without it → `null`. Never a string starting with `/Game/`. `fromStartMs` = the shared match's start.
    - `top3_remembered_source_is_map_uuid`: `player_details` for a player whose Top 3 teammate is hidden on screen and remembered from the user's match → that entry's `fromMapId` is the Split uuid, and `serde_json::to_string(&details)` contains no `/Game/`.
    - `player_details_card_carries_act_wins_games` (moved here from Task 15, where it could not fail, R96): an off-screen player with a stored MMR snapshot → `card.actWins`/`card.actGames` (the hero's Games and W / L source) are set. It fails now because the fields don't exist; `offscreen_card` goes through `card_for` → `build_card`, which this task extends.
- [ ] **Step 2:** Run `cd /home/project-1/.worktrees/p2-backend && npm --prefix ui run build && cargo test -p opd1-core -p opd1-riot -p opd1-trcker`. Expected: FAIL (missing fields, variants and functions; hover still shows an agent; the hidden own level is `null`).
- [ ] **Step 3:** Implement Interfaces → Produces.
  - `card_for` passes `is_self`. It keeps `is_self_party: r.is_self_party || is_self`.
  - `shown_agent` returns the agent only for `"locked"` (R99). `hidden_players()` and `build_card` both use it, so a hover changes neither the card nor Top 3.
  - `card_for` sets `hide_account_level: (r.hide_account_level && !is_self) || r.account_level == 0` (R100).
  - `sync_player` uses `mmr_refresh_needed`.
  - The remembered source is converted where the DisplayName is built, never at DB read (R84), and only through `KnownName::with_map_uuid` (R95): `card_for` passes `slot.db.remembered.clone().map(|k| k.with_map_uuid(map_uuid))` to `build_card`, and `teammate_name` does the same for its entry. `publish()` passes `&|p| self.map_uuid(p)` (it already resolves `LiveState.map_id` there, state.rs:630-636).
  - `details.rs` passes `&|p| st.map_uuid(p)` to both `card_for` and `teammate_name`. Don't reuse the `lookup` closure: it lives inside `st.db(move |repo| …)` (details.rs:98-101), and `teammate_name` is called outside it (details.rs:155).
- [ ] **Step 4:** Run `cd /home/project-1/.worktrees/p2-backend && cargo test -p opd1-core -p opd1-riot -p opd1-trcker && cargo clippy --workspace --all-targets -- -D warnings`. Expected: PASS.
- [ ] **Step 5: Commit + push** `feat: live card contract for design v8 (isSelf, act W/L with placements, cardLarge, remembered source, lock-only agent, own level)` →
  ```bash
  cd /home/project-1/.worktrees/p2-backend && test "$(git branch --show-current)" = p2-backend && P="crates/core/src/model.rs crates/core/src/names.rs crates/core/src/db/repo.rs crates/riot/src/endpoints.rs crates/riot/src/to_core.rs crates/riot/src/assets.rs src-tauri/src/imgproto.rs src-tauri/src/store.rs src-tauri/src/sync.rs src-tauri/src/state.rs src-tauri/src/details.rs crates/core/tests/card.rs crates/core/tests/names.rs crates/core/tests/db.rs crates/riot/tests/to_core.rs crates/riot/tests/assets.rs crates/riot/tests/fixtures/mmr_placements.json crates/riot/tests/fixtures/mmr_no_placements.json src-tauri/src/pipeline_tests.rs" && git add $P && git commit -m "feat: live card contract for design v8 (isSelf, act W/L with placements, cardLarge, remembered source, lock-only agent, own level)" -- $P
  ```
  - If Step 3 had to touch any other file, add it to `P`. The lane-B porcelain check (Global Git) must print nothing (`build.yml` is untouched until Step 11). Then `cd /home/project-1/.worktrees/p2-backend && git push origin p2-backend`.
- [ ] **Step 6:** Write the failing test for the false stripe. In `src-tauri/src/pipeline_tests.rs`, `failed_history_never_feeds_party_marks`:
  - Setup: OTHER and a teammate share their last 2 matches in the DB from an earlier session. The mock `match-history` for OTHER returns 500.
  - After `run_worker`, OTHER is in `synced` but not in `party_ready`, and neither card has `party`.
  - A control run with 200 → both cards get the same `party.group`.
  - Run `cd /home/project-1/.worktrees/p2-backend && cargo test -p opd1-trcker failed_history`. Expected: FAIL.
- [ ] **Step 7:** Implement:
  - `sync_player` returns `SyncOutcome { history_ok }`.
  - `sync_step` inserts into `party_ready` only when `history_ok`.
  - `party_ready` is cleared wherever `synced` is cleared (`poll.rs:224` on an account switch, `poll.rs:455` on a stage change) and loses the puuid wherever `synced` does (`poll.rs:818`, the post-match re-sync).
  - `party_marks` filters on `party_ready`.
- [ ] **Step 8:** Run `cd /home/project-1/.worktrees/p2-backend && cargo test -p opd1-core -p opd1-riot -p opd1-trcker`. Expected: PASS.
- [ ] **Step 9: Commit + push** `fix(sync): a failed match history never feeds party marks` → `cd /home/project-1/.worktrees/p2-backend && test "$(git branch --show-current)" = p2-backend && P="src-tauri/src/sync.rs src-tauri/src/state.rs src-tauri/src/poll.rs src-tauri/src/pipeline_tests.rs" && git add $P && git commit -m "fix(sync): a failed match history never feeds party marks" -- $P`. Then the lane-B porcelain check and `cd /home/project-1/.worktrees/p2-backend && git push origin p2-backend`.
- [ ] **Step 10:** Run the failing CI checks. Each passes only after Step 11:
  - `cd /home/project-1/.worktrees/p2-backend && grep -q -- "-p opd1-trcker" .github/workflows/build.yml`
  - `cd /home/project-1/.worktrees/p2-backend && grep -q "Build UI" .github/workflows/build.yml`
  - `cd /home/project-1/.worktrees/p2-backend && gh run view "$(gh run list -w Build -b main -s completed -L 1 --json databaseId -q '.[0].databaseId')" --log | grep -ci "node.js 20"` returns 0. The run checked is the newest finished `main` run, because a `p2-backend` push starts no run; it still uses the old `build.yml`.

  Expected now: FAIL.
- [ ] **Step 11:** Edit `build.yml` (in the worktree):
  - Bump `actions/checkout`, `actions/setup-node` and `actions/upload-artifact` to the newest majors whose `action.yml` says `using: node24`. Check each with `gh api repos/<owner>/<repo>/contents/action.yml?ref=<tag> -q .content | base64 -d | grep using`; expected v5+.
  - Keep `Swatinem/rust-cache@v2` only if its latest v2 tag is node24; otherwise pin the newest node24 release.
  - Add `- name: Build UI` / `run: npm --prefix ui run build` before the Rust tests, because `generate_context!` needs `ui/dist`.
  - Change the Rust test step to `cargo test -p opd1-core -p opd1-riot -p opd1-trcker`.
  - If the Windows runner can't start the `opd1-trcker` test binary, add a separate `ubuntu-latest` job instead: install `libwebkit2gtk-4.1-dev libsoup-3.0-dev librsvg2-dev`, then `npm ci && npm --prefix ui run build` (`generate_context!` needs `ui/dist`), then `cargo test -p opd1-trcker`. No xvfb: the 68 src-tauri tests run headless on the VPS.
  - Keep the `on:` block as it is (`push` to `main` + `workflow_dispatch`).
- [ ] **Step 12: Commit + push** `ci: run src-tauri tests, build ui first, node24 actions` → `cd /home/project-1/.worktrees/p2-backend && test "$(git branch --show-current)" = p2-backend && git add .github/workflows/build.yml && git commit -m "ci: run src-tauri tests, build ui first, node24 actions" -- .github/workflows/build.yml`, the lane-B porcelain check, `cd /home/project-1/.worktrees/p2-backend && git push origin p2-backend`.
  - Then watch CI (Global "CI watch", lane B: dispatch on `p2-backend`, then select this commit's run as `$id`). Expected: success.
  - `cd /home/project-1/.worktrees/p2-backend && gh run view "$id" --log | grep -ci "node.js 20"` = 0.
  - `cd /home/project-1/.worktrees/p2-backend && gh api "repos/opd116/opd1-trcker/actions/runs/$id/artifacts" -q '.artifacts[].name'` = `OPD1-TRCKER-setup`.

### Task 15: Backend contract delta B (Player Details + Match Details)

**Lane B,** after Task 14, in `/home/project-1/.worktrees/p2-backend` on `p2-backend`. Every command starts with `cd /home/project-1/.worktrees/p2-backend &&`. Its first commit waits until Task 13 has landed on `origin/main` (Step 0). It ends by landing `p2-backend` on `main` (Step 13), which unblocks Task 16b.

**Files:**
- Modify:
  - `crates/core/src/db/mod.rs`: `MIGRATIONS` gains migration 2. Its doc comment says a committed migration is frozen (migration 1 froze at `c9ad7f4`, R89) and new ones are only appended. `open_db` itself is unchanged: it already applies the pending migrations in one transaction and sets `user_version`.
  - `crates/core/src/db/repo.rs`:
    - `Match`, `MatchTeam` and `MatchPlayer` gain columns;
    - `CompMatchRow.tier`, documented next to `tier_after` (Interfaces);
    - `comp_matches` rewritten so the player's rows are limited first and joined after (R88); its SQL is exposed as `COMP_MATCHES_SQL`;
    - `EncounterRow` extended;
    - new `encounter_counts` and `match_rows`.
  - `crates/core/src/model.rs`: `MatchSummary.tier`, `EncounterDetailRow`, `WeaponSkin`, the tri-state `Loadout` (R97), the `MatchDetailsView` family.
  - `crates/core/src/lib.rs`: `pub mod matchview`.
  - `crates/riot/src/endpoints.rs`: `MatchInfo.game_length_millis`, `MatchPlayer.competitive_tier`, `RoundResult.{round_num, winning_team}`; `PlayerLoadoutResponse`, `LoadoutGun` and `RiotSession::player_loadout` (R90).
  - `crates/riot/src/raw_capture.rs`: `endpoint_key` names `/personalization/v2/players/…/playerloadout` `player_loadout`, so `dump_raw` writes `player_loadout.json`.
  - `crates/riot/src/to_core.rs`: tier, damage, halves, length.
  - `crates/riot/src/assets.rs`:
    - `AssetKind::ContentTier`;
    - `Chroma` downscales to 256;
    - the `/v1/contenttiers` table;
    - the per-skin cache (an in-memory map, filled by `weapon_skin`);
    - `weapon_skin` and `weapon_skin_cached` (memory only).
  - `src-tauri/src/imgproto.rs`: `contentTier`.
  - `src-tauri/src/roster.rs`: `EquippedSkin`, `RawLoadout`, `extract_loadouts`, `extract_pregame_loadouts`, `extract_player_loadout` (R90), `want_pregame_loadouts`, `SKIN_SOCKET`.
  - `src-tauri/src/state.rs`: `LiveModel.loadouts: HashMap<String, RawLoadout>`; `LiveModel.self_loadout: Option<RawLoadout>` (R90); `AppState::skins_resolving` (R97).
  - `src-tauri/src/poll.rs`:
    - the pregame loadouts on the 3s re-poll, gated by `want_pregame_loadouts` (`Poller::new` / `pregame_repoll` become `pub(crate)` if the pipeline test needs them);
    - `on_connected` spawns `sync::fetch_self_loadout` once per connect (R90), and its account-switch branch (poll.rs:224) clears `self_loadout`.
  - `src-tauri/src/sync.rs`: `sync_details` stores `RawLoadout`s (it no longer resolves skin names); `fetch_self_loadout` (R90).
  - `src-tauri/src/details.rs`: extended encounter rows; the tri-state `WeaponSkin` loadout (R97); the user's own loadout source (R90); `skins_pending`, `resolve_skins`; the match view's remembered sources go through `KnownName::with_map_uuid` (R84, R95).
  - `src-tauri/src/store.rs`: `load_player_db` uses `encounter_counts`; `recent_matches` carries `tier`.
  - `src-tauri/src/commands.rs`: `match_details`, which validates both ids; `player_details` spawns `resolve_skins` whenever a skin name is missing, not gated by `claim_details_sync` (R83), and rebuilds once it ends (R97).
  - `src-tauri/src/lib.rs`: register `match_details`.
  - Not here: `docs/ARCHITECTURE.md`. Its DB schema and commands refresh runs in Task 19, because lane A owns `docs/` while this lane runs.
- Create:
  - `crates/core/src/db/migration2.sql`: the 4 `ALTER TABLE … ADD COLUMN` statements (R89).
  - `crates/core/src/matchview.rs`
  - `crates/core/tests/matchview.rs`
  - `crates/core/tests/contract.rs`
  - `crates/core/tests/fixtures/contract/{live_state,player_details,match_details_view}.json`
  - `src-tauri/src/match_details.rs`: DB read + fetch-once.
  - `crates/riot/tests/fixtures/weapon_skin_reaver_vandal.json`: captured from `https://valorant-api.com/v1/weapons/skins/<Reaver Vandal skin uuid>` and trimmed to `uuid, displayName, contentTierUuid, chromas[].{uuid, displayName}`. Its chroma `b2a065c0-4632-ccaa-f496-7681dc2a6185` is named `"Reaver Vandal Level 4\r\n(Variant 1 Red)"`.
  - `crates/riot/tests/fixtures/pregame_loadouts.json`: built from the real capture in techchrism `valorant-api-docs` (branch `trunk`), `valorant-api-types/tests/endpoint-responses/pre-game-loadouts/singleplayer.json` (`gh api "repos/techchrism/valorant-api-docs/contents/valorant-api-types/tests/endpoint-responses/pre-game-loadouts/singleplayer.json?ref=trunk" -q .content | base64 -d`). Each `Loadouts[]` entry carries `Subject` + `Items` at the top level (no `Loadout` wrapper, unlike core-game). Trim it to 2 entries (the capture's plus a copy with a second subject and other skins) and add a third entry without `Subject`, plus `"LoadoutsValid": true`.
  - `crates/riot/tests/fixtures/player_loadout.json` (R90): built from the same repo's `valorant-api-types/tests/endpoint-responses/player-loadout/response.json` (`gh api "repos/techchrism/valorant-api-docs/contents/valorant-api-types/tests/endpoint-responses/player-loadout/response.json?ref=trunk" -q .content | base64 -d`; a read-only clone at commit `f473c59` is at `/tmp/claude-1000/-home-project-1/5d122e31-a6c8-44a1-a1db-6d9b7e05f812/scratchpad/refs/valorant-api-docs`).
    - The field names come from `valorant-api-types/src/endpoints/pvp/PlayerLoadout.ts`. Top level: `Subject, Version, Guns, Sprays, Identity, Incognito`. Each `Guns[]` entry: `ID` (the weapon uuid), `SkinID`, `SkinLevelID`, `ChromaID`, `Attachments`, and optional `CharmInstanceID`/`CharmID`/`CharmLevelID`. The endpoint "only works for your own PUUID".
    - The capture's `Guns[].ID` values are anonymised; none is a real weapon uuid. Set four entries' `ID` to the Vandal, Phantom, Operator and Sheriff uuids (roster.rs:383-386), writing the Sheriff one in upper case to prove the match ignores case. Drop `Sprays`; keep the rest as captured.
  - `crates/core/tests/fixtures/schema_v1.sql`: `git show c9ad7f4:crates/core/src/db/schema.sql`, byte for byte (the frozen migration 1, R89).
- Modify (fixtures): `crates/riot/tests/fixtures/match_details.json` gains `matchInfo.gameLengthMillis: 2280000` (techchrism `MatchDetails.ts`).
- Tests:
  - `crates/core/tests/{db,card}.rs`. `db.rs`'s existing `migrations_idempotent` now expects `user_version` 2.
  - `crates/core/tests/mystats.rs`: its `CompMatchRow { … }` literal is in the `row()` helper at mystats.rs:61, has no `..Default`, and gains `tier: None`.
  - `crates/riot/tests/{to_core,assets,endpoints_fixtures}.rs`; the `crates/riot/src/raw_capture.rs` test module.
  - `src-tauri/src/{roster,imgproto,store,commands}.rs` (test modules; `store.rs`'s `Match`/`MatchTeam`/`MatchPlayer` literals gain the new fields).
  - `src-tauri/src/pipeline_tests.rs` (same for its literals).

**Interfaces:**
- Consumes: spec §7.1–§7.3 (items 5–9), §6.8 and §6.10 (perspective, sorting, no-team modes); R74 (in place of audit §2.12), R81, R83–R85, R88–R90, R93, R95, R97; audit §3C 15–18; Task 9a's code at `c9ad7f4` (reviewed, R73; Details builds on `recent_matches`); Task 14's `KnownName::with_map_uuid`.
- Produces (SQL, migration 2 in `crates/core/src/db/migration2.sql`, R89; `user_version` 1 → 2; `schema.sql` stays byte-identical to `c9ad7f4`):
  - `matches.game_length_ms INTEGER`
  - `match_teams.rounds_won_h1 INTEGER`: rounds with `roundNum` 0–11 won. Filled only for `competitive`, `unrated` and `premier`; otherwise NULL.
  - `match_players.competitive_tier INTEGER`
  - `match_players.damage INTEGER`: Σ `roundResults[].playerStats[subject].damage[].damage`.
- Produces (Rust):
  ```rust
  // db/mod.rs
  const MIGRATIONS: &[&str] = &[include_str!("schema.sql") /* frozen, R89 */, include_str!("migration2.sql")];
  // repo.rs
  pub struct Match { /* … */ pub game_length_ms: Option<i64> }
  pub struct MatchTeam { /* … */ pub rounds_won_h1: Option<i64> }
  pub struct MatchPlayer { /* … */ pub competitive_tier: Option<i64>, pub damage: Option<i64> }
  pub struct CompMatchRow { /* … */
      /// The player's rank DURING the match, for display (recent rows, encounters, scoreboard):
      /// COALESCE(mp.competitive_tier, cu.tier_after), Riot's at-match value first (R81).
      pub tier: Option<i64> }
      // Doc comment on the existing `tier_after`: the comp_updates row's rank AFTER the match (My Stats' RR math).
      // The two differ on a promotion or demotion game; never use one for the other.
  #[doc(hidden)] pub const COMP_MATCHES_SQL: &str;   // R88: the player's comp rows are limited first (ORDER BY start_ms DESC LIMIT ?4),
      // then joined to match_teams (both teams, aggregated per match) and comp_updates; no correlated subquery
  pub struct EncounterRow { pub match_id: String, pub map_id: String, pub start_ms: i64, pub with: bool,
      pub self_won: Option<bool>, pub rounds_won: Option<i64>, pub rounds_lost: Option<i64>, pub game_length_ms: Option<i64>,
      pub self_agent_id: Option<String>, pub other_agent_id: Option<String>,
      pub self_tier: Option<i64>, pub other_tier: Option<i64> }   // tiers: the same COALESCE(mp.competitive_tier, cu.tier_after) rule (R81)
  pub fn encounter_counts(&self, self_puuid: &str, other: &str) -> Result<(u32, u32)>   // (with, against), one COUNT query
  pub struct StoredMatch { pub info: Match, pub teams: Vec<MatchTeam>, pub players: Vec<MatchPlayer> } // players' competitive_tier = COALESCE(mp.competitive_tier, cu.tier_after) (R81)
  pub fn match_rows(&self, match_id: &str) -> Result<Option<StoredMatch>>
  // model.rs (serde camelCase; tiers < 3 serialize as null)
  pub struct MatchSummary { /* … */ pub tier: Option<u8> }
  pub struct EncounterDetailRow { pub match_id: String, pub map_id: String, pub start_ms: i64, pub with: bool,
      pub result: MatchResultKind, pub rounds_won: Option<i64>, pub rounds_lost: Option<i64>, pub duration_ms: Option<i64>,
      pub them_agent_id: Option<String>, pub them_tier: Option<u8>, pub self_agent_id: Option<String>, pub self_tier: Option<u8> }
  pub struct WeaponSkin { pub chroma_id: String, pub skin_name: String, pub chroma_name: Option<String>, pub tier_id: Option<String> }
  pub struct Loadout {   // R97: the R23 tri-state per weapon (absent = name resolving, null = none/unknown, value = skin)
      #[serde(skip_serializing_if = "Field::is_loading")] pub vandal: Field<WeaponSkin>,
      #[serde(skip_serializing_if = "Field::is_loading")] pub phantom: Field<WeaponSkin>,
      #[serde(skip_serializing_if = "Field::is_loading")] pub operator: Field<WeaponSkin>,
      #[serde(skip_serializing_if = "Field::is_loading")] pub sheriff: Field<WeaponSkin> }   // Default: all Field::Missing
  #[serde(rename_all = "lowercase")] pub enum Perspective { #[serde(rename = "self")] SelfView, Focus }
  pub struct MatchDetailsView { pub match_id: String, pub map_id: String, pub queue_id: String, pub start_ms: i64,
      pub duration_ms: Option<i64>, pub perspective: Perspective, pub teams: Vec<MatchTeamView> }
  pub struct MatchTeamView { pub team_id: String, pub result: Option<MatchResultKind>, pub rounds_won: Option<i64>,
      pub first_half: Option<i64>, pub players: Vec<ScoreRow> }
  pub struct ScoreRow { pub puuid: String, pub name: DisplayName, pub agent_id: Option<String>, pub tier: Option<u8>,
      pub kills: i64, pub deaths: i64, pub assists: i64, pub acs: Option<f32>, pub adr: Option<f32>, pub hs_pct: Option<f32>,
      pub is_self: bool, pub is_focus: bool }
  // matchview.rs (pure)
  pub struct MatchViewInput<'a> { pub stored: &'a StoredMatch,
      pub map_uuid: &'a dyn Fn(&str) -> Option<String>,     // Riot path → uuid: the match's map, and remembered sources via KnownName::with_map_uuid (R84, R95)
      pub self_puuid: Option<&'a str>, pub self_name: Option<(String, String)>, pub focus_puuid: &'a str,
      pub hidden_now: &'a HashMap<String, Option<String>>,   // LiveModel::hidden_players()
      pub remembered: &'a HashMap<String, KnownName> }       // Repo::known_name(self, p): the user's own matches only; map_id is still a Riot path here
  pub fn build_match_view(input: MatchViewInput) -> MatchDetailsView
  // assets.rs
  AssetKind::ContentTier  // dir "contentTier"; /v1/contenttiers[].displayIcon; downscale 32px; id = content-tier uuid
  pub fn weapon_skin_cached(&self, skin_id: &str, chroma_id: &str) -> Option<WeaponSkin>   // in-memory map only: no fs on the async runtime (R37)
  pub async fn weapon_skin(&self, skin_id: &str, chroma_id: &str) -> Result<WeaponSkin, AssetError>
      // reads the skin's cached JSON in spawn_blocking, else fetches /v1/weapons/skins/{skin_id} once and caches it; fills the memory map
  // endpoints.rs (R90; field names per techchrism PlayerLoadout.ts)
  pub struct PlayerLoadoutResponse { #[serde(rename = "Subject")] pub subject: String,
      #[serde(rename = "Guns", default)] pub guns: Vec<LoadoutGun> }             // other keys ignored
  pub struct LoadoutGun { #[serde(rename = "ID")] pub id: String, #[serde(rename = "SkinID")] pub skin_id: String,
      #[serde(rename = "ChromaID")] pub chroma_id: String }
  pub async fn player_loadout(&self, puuid: &str) -> Result<PlayerLoadoutResponse, RiotError>
      // pd GET /personalization/v2/players/{puuid}/playerloadout; read-only; called only with the user's own puuid
  // roster.rs
  pub const SKIN_SOCKET: &str = "bcef87d6-209b-46c6-8b19-fbe40bd95abc";   // Scout backend/valapi.py
  #[derive(Serialize, Deserialize)] pub struct EquippedSkin { pub skin_id: String, pub chroma_id: String }
  #[derive(Serialize, Deserialize)] pub struct RawLoadout { pub vandal: Option<EquippedSkin>, pub phantom: Option<EquippedSkin>, pub operator: Option<EquippedSkin>, pub sheriff: Option<EquippedSkin> }
  pub fn extract_loadouts(resp: &CoreGameLoadoutsResponse) -> HashMap<String, RawLoadout>
  pub fn extract_pregame_loadouts(resp: &PregameLoadoutsResponse) -> HashMap<String, RawLoadout> // keyed by each entry's `Subject`; an entry without one is skipped, never matched by position (as extract_loadouts, roster.rs:405-407); empty unless LoadoutsValid
  pub fn extract_player_loadout(resp: &PlayerLoadoutResponse) -> RawLoadout   // R90: Guns[].ID matched to the 4 weapon uuids ignoring case; ids stored lower-case
  pub fn want_pregame_loadouts(allies: &[String], have: &HashMap<String, RawLoadout>, seen_valid: bool) -> bool // !seen_valid && some ally has no RawLoadout
  // state.rs
  pub struct LiveModel { /* … */ pub self_loadout: Option<RawLoadout> }   // R90; cleared when the account changes
  impl AppState { pub fn skins_resolving(&self, puuid: &str) -> bool }    // R97: true only while a resolve_skins for puuid runs
  // sync.rs
  pub async fn fetch_self_loadout(st: &Arc<AppState>, session: &Arc<RiotSession>) -> Result<(), Gone>
      // R90: one player_loadout(self) call; sets live.self_loadout; writes app_state key "self_loadout" (RawLoadout JSON) when it changed;
      // a failure is logged and not retried until the next connect
  // details.rs
  pub fn skins_pending(st: &AppState, puuid: &str) -> bool          // the player has a RawLoadout skin whose name isn't in memory yet
  pub async fn resolve_skins(st: &Arc<AppState>, puuid: &str) -> bool // valorant-api only, ≤ 4 small JSON fetches, cached forever; true when a name became known
  // commands.rs / match_details.rs
  fn check_match_args(match_id: &str, focus_puuid: &str) -> Result<(), String>   // both must pass is_uuid (commands.rs:29), like player_details' puuid (commands.rs:52)
  #[tauri::command] pub async fn match_details(match_id: String, focus_puuid: String, state: AppStateRef<'_>) -> Result<MatchDetailsView, String>
  ```
- Produces (JSON):
  - `MatchSummary.tier`
  - `EncounterDetailRow`: `matchId, mapId, startMs, with, result, roundsWon, roundsLost, durationMs, themAgentId, themTier, selfAgentId, selfTier`. `selfWon` is removed.
  - `Loadout.{vandal,phantom,operator,sheriff}`: `{chromaId, skinName, chromaName, tierId}` for a known skin, `null` when there is none or it can't be known, and the key absent while the skin name is resolving (R97).
  - `MatchDetailsView`: `{matchId, mapId, queueId, startMs, durationMs, perspective: "self"|"focus", teams: [{teamId, result, roundsWon, firstHalf, players: [{puuid, name, agentId, tier, kills, deaths, assists, acs, adr, hsPct, isSelf, isFocus}]}]}`.
  - Invoked as `invoke('match_details', { matchId, focusPuuid })`.
- Rules:
  - **`skinName`:** the skin `displayName` minus the trailing weapon word (`"Reaver Vandal"` → `"Reaver"`; the text is kept as is when it doesn't end in the weapon name).
  - **`chromaName`:** the `X` in `(Variant N X)` of the chroma's `displayName`; `null` for the base chroma.
  - **`tierId`:** the skin's `contentTierUuid` (`null` for default/battle-pass skins).
  - **Loadout source (R83, R90):** another player: `live.loadouts` (core-game or pregame) only, else all four `null`. The user: `live.loadouts` first (the match's loadout is the freshest), else `live.self_loadout`, else the stored `app_state` `self_loadout` (read inside the same `st.db` call), else `null`.
  - **Loadout state (R97):** an equipped weapon whose skin name is in memory → the `WeaponSkin`. A name not in memory → `Field::Loading` (absent key) only while `st.skins_resolving(puuid)`; otherwise `Field::Missing` (`null`). So a skeleton never outlives the fetch attempt.
  - **Perspective:** `teams[0]` is the user's team when the user has a row in the match, else the focus player's team.
  - **Sorting:** ACS descending, nulls last, ties broken by puuid.
  - **No-team modes:** anything other than exactly 2 teams → one block, with `teamId ""` and `result`/`roundsWon`/`firstHalf` all `null`.
  - **Names:** in this order, for every row:
    1. Self → the user's name (Visible).
    2. A row whose puuid is in `hidden_now` → `display_name(true, None, row.agent, remembered.get(puuid).map(|k| k.with_map_uuid(map_uuid)))`, in every view, whether or not the user played the match (R93). The card, Top 3 and the scoreboard then show the same style: Remembered when the name is known, else the agent name.
    3. If the user played: Visible when the stored name is non-empty, else the rule-2 call.
    4. If the user did not play: a row with an empty stored name → the rule-2 call; else Visible (R74: match-details has no incognito flag; Task 19 item 12 checks the risk).
    - A remembered source's `map_id` goes through `KnownName::with_map_uuid(input.map_uuid)` (R84, R95), so `fromMapId` is a uuid or `null`, never `/Game/…`.
  - **Stored data:** `known_name` stays restricted to matches the user played.
  - **Units:** `acs` = score/rounds, `adr` = damage/rounds, `hsPct` = hs/(hs+bs+ls).
  - **Tier (R81):** everywhere (`MatchSummary.tier`, encounter rows, the scoreboard) = `COALESCE(mp.competitive_tier, cu.tier_after)`, i.e. Riot's at-match rank, the same source for all 10 scoreboard rows.
  - **`match_details` command:**
    - Rejects a non-uuid `matchId` (`Err("invalid match id")`) or a non-uuid `focusPuuid` (`Err("invalid player id")`), the same check `player_details` applies to its puuid.
    - Reads the DB. On a miss, and with a session, it fetches match-details once (`fetch_missing_details`) and re-reads.
    - Still missing → `Err("unknown match")`.

- [ ] **Step 0:** Load `test-driven-development` and `tauri-v2` (a new command and its registration).
  - Wait for Task 13: `cd /home/project-1/.worktrees/p2-backend && git fetch origin && git log origin/main --format=%s | grep -qx "docs: phase 2 rules, repo files, design-history cleanup"`. Until it succeeds, this task does not start; the controller holds it.
  - Merge it (R98: before this task's first commit):
    ```bash
    cd /home/project-1/.worktrees/p2-backend && test "$(git branch --show-current)" = p2-backend && test -z "$(git status --porcelain)" && git merge origin/main -m "chore: merge main into p2-backend" && git push origin p2-backend
    ```
    Expected: a clean merge (lane A wrote only docs and root files, lane B only `crates`, `src-tauri` and `.github`).
  - Save the frozen migration 1: `cd /home/project-1/.worktrees/p2-backend && git show c9ad7f4:crates/core/src/db/schema.sql > crates/core/tests/fixtures/schema_v1.sql`.
  - Find every struct literal the new fields break with graft (`graft grep "CompMatchRow {"`, `graft grep "MatchPlayer {"`, `graft grep "MatchTeam {"`, `graft grep "Match {"`, `graft grep "Loadout {"`), then confirm each in the worktree.
- [ ] **Step 1:** Write the failing tests (DB + match view):
  - `crates/core/tests/db.rs`:
    - `migration1_is_frozen` (R89): `include_str!("../src/db/schema.sql") == include_str!("fixtures/schema_v1.sql")`. It passes now and fails the day anyone edits migration 1.
    - `fresh_db_has_phase2_columns_at_user_version_2`: `PRAGMA table_info` lists the 4 new columns; `user_version` = 2.
    - `v1_db_upgrades_to_user_version_2` (R89): a DB built by running `fixtures/schema_v1.sql` and `PRAGMA user_version = 1`, holding one stored match → `open_db` sets `user_version` 2; the 4 columns exist and are NULL on the old rows; `upsert_match` round-trips the Phase 2 fields; a second `open_db` runs no SQL (`user_version` stays 2).
    - `migrations_idempotent`: update both assertions to 2.
    - `upsert_match_round_trips_phase2_columns`
    - `comp_matches_tier_prefers_match_tier_then_comp_update`: a match tier of 20 beats `tier_after` 21; with the match tier NULL → 21. `tier_after` stays 21 in both.
    - `comp_matches_has_no_correlated_subqueries` (R88): `EXPLAIN QUERY PLAN` + `repo::COMP_MATCHES_SQL` (bound with sample params, run on an `open_db` connection) has no detail row containing `CORRELATED`, and `match_teams` is only ever read with `SEARCH`, never `SCAN`. It fails now: the current query has two correlated scalar subqueries (repo.rs:329-333).
    - `comp_matches_limits_before_joining` (R88): 30 stored comp matches for the player, `limit` 8 → the 8 newest, newest first, with `rounds_won`/`rounds_lost` equal to the stored team rounds and `rounds_lost` `None` when the match has other than exactly 2 teams.
    - `encounters_rows_carry_agents_tiers_score_length`: self is Jett at tier 21 and other is Reyna at tier 24. Rounds are 10–13 from the user's side, with game length 2 460 000 → `self_won == Some(false)`.
    - `encounter_counts_equal_encounters_len`
    - `match_rows_returns_teams_players_with_tiers`
  - `crates/riot/tests/to_core.rs`:
    - `match_from_details_parses_tier_damage_halves_length`: `match_details.json` → tiers 15 and 18, damage = the fixture sums, `rounds_won_h1` counted from `roundNum < 12` × `winningTeam`, length 2 280 000.
    - `match_from_details_nulls_keep_phase2_fields_none`: `match_details_nulls.json`.
  - `crates/core/tests/matchview.rs`:
    - `perspective_self_puts_user_team_first`
    - `perspective_focus_when_user_absent`
    - `rows_sorted_by_acs_desc_nulls_last`
    - `deathmatch_is_one_block_without_scores`
    - `user_played_names_as_stored_and_empty_name_hidden`
    - `user_absent_hides_live_incognito_players`: the stored row has name `"RealName"` → `HiddenAgent{agentId}`, and `"RealName"` appears nowhere in `serde_json::to_string(&view)`.
    - `hidden_now_rows_use_display_name_in_every_view` (R93): in a match the user played, a row in `hidden_now` with a `remembered` entry → `{"kind":"remembered", …}` with a uuid `fromMapId`; a row in `hidden_now` without one (stored name `"RealName"`) → `HiddenAgent{agentId}`, and `"RealName"` appears nowhere in the view JSON. The same two rows in a match the user did not play give the same result.
    - `user_absent_remembered_only_from_users_own_matches`
    - `remembered_source_is_uuid_in_match_view`: a lookup maps the remembered row's `/Game/Maps/Bonsai/Bonsai` to the Split uuid → `fromMapId` is that uuid, and the view JSON contains no `/Game/`.
    - `first_half_and_result_per_team`
    - `acs_adr_hs_computed_per_round`
    - `self_and_focus_flags`
  - `crates/core/tests/card.rs`: `match_summary_tier_below_3_is_null`
  - `src-tauri/src/commands.rs` (test module): `match_details_args_must_be_uuids`: a non-uuid `matchId` → `Err("invalid match id")`; a valid `matchId` with `focusPuuid` `"../x"` → `Err("invalid player id")`; two uuids → `Ok`.
  - `src-tauri/src/pipeline_tests.rs`:
    - `match_details_reads_db_and_fetches_unknown_once`: the first call makes 1 `GET /match-details/v1/matches/{id}`; the second call makes 0 new requests; the result has 2 teams.
    - `player_details_encounter_rows_are_extended`
    - (`player_details_card_carries_act_wins_games` moved to Task 14 Step 1, R96.)
- [ ] **Step 2:** Run `cd /home/project-1/.worktrees/p2-backend && cargo test -p opd1-core -p opd1-riot -p opd1-trcker`. Expected: FAIL.
- [ ] **Step 3:** Implement the migration, parsing, repo, `matchview.rs`, `match_details.rs`, the command and its registration, and the `recent_matches` tier. `load_player_db` switches to `encounter_counts`.
  - Write `migration2.sql` and append it to `MIGRATIONS`. `schema.sql` stays byte-identical (R89). Update the `MIGRATIONS` doc comment: a committed migration is frozen, migration 1 froze at `c9ad7f4`, and a later task only appends to the list.
  - Rewrite `comp_matches` (R88): select the player's competitive rows in the range first, newest first, with `LIMIT` (a CTE or subquery over `match_players` ⨝ `matches`). Then join those rows to `match_teams` on `match_id` (aggregating both teams per match for `rounds_won` and the exactly-one-other-team `rounds_lost`) and to `comp_updates` on `(puuid, match_id)`. The read cost then grows with `limit`, not with the stored history. Expose the SQL as `COMP_MATCHES_SQL`.
  - Add the `tier` / `tier_after` doc comments exactly as in Interfaces.
  - `build_match_view` converts remembered sources only with `KnownName::with_map_uuid` (R95) and applies the Names rules in order (R93).
  - `match_details` calls `check_match_args` first.
  - Local dev DBs need no deleting: `open_db` upgrades them to `user_version` 2, as `v1_db_upgrades_to_user_version_2` proves.
- [ ] **Step 4:** Run `cd /home/project-1/.worktrees/p2-backend && cargo test -p opd1-core -p opd1-riot -p opd1-trcker && cargo clippy --workspace --all-targets -- -D warnings`. Expected: PASS.
- [ ] **Step 5: Commit + push** `feat: match details view, per-player tier/ADR/halves, richer encounters` →
  ```bash
  cd /home/project-1/.worktrees/p2-backend && test "$(git branch --show-current)" = p2-backend && P="crates/core/src/db/migration2.sql crates/core/src/db/mod.rs crates/core/src/db/repo.rs crates/core/src/model.rs crates/core/src/lib.rs crates/core/src/matchview.rs crates/core/tests/db.rs crates/core/tests/matchview.rs crates/core/tests/card.rs crates/core/tests/mystats.rs crates/core/tests/fixtures/schema_v1.sql crates/riot/src/endpoints.rs crates/riot/src/to_core.rs crates/riot/tests/to_core.rs crates/riot/tests/fixtures/match_details.json src-tauri/src/commands.rs src-tauri/src/match_details.rs src-tauri/src/details.rs src-tauri/src/store.rs src-tauri/src/lib.rs src-tauri/src/pipeline_tests.rs" && git add $P && git commit -m "feat: match details view, per-player tier/ADR/halves, richer encounters" -- $P
  ```
  - If Step 3 had to touch any other file, add it to `P`. The lane-B porcelain check must print nothing, then `cd /home/project-1/.worktrees/p2-backend && git push origin p2-backend`.
- [ ] **Step 6:** Write the failing tests (loadout):
  - `crates/riot/tests/assets.rs`:
    - `weapon_skin_names_and_tier`: the fixture → `skin_name "Reaver"`, `chroma_name Some("Red")`, the base chroma → `None`, `tier_id` = the fixture's `contentTierUuid`.
    - `weapon_skin_fetched_once_then_cached`: 1 mock hit over 2 calls, and `weapon_skin_cached` answers from memory after the first; a new `AssetCache` on the same dir → `weapon_skin_cached` is `None` until `weapon_skin` runs, which then makes 0 hits (disk JSON).
    - `weapon_skin_cached_is_none_before_fetch`
    - `content_tier_icon_downscaled_to_32`
    - `chroma_downscaled_to_256`
  - `crates/riot/tests/endpoints_fixtures.rs`: `player_loadout_fixture_parses` (R90): `player_loadout.json` → `subject` set, `guns.len()` = the fixture's count, and each gun has `id`, `skin_id` and `chroma_id`.
  - `crates/riot/src/raw_capture.rs` (test module): `endpoint_key("/personalization/v2/players/abc/playerloadout") == "player_loadout"`.
  - `src-tauri/src/roster.rs`:
    - `extract_loadouts_reads_skin_and_chroma` (the coregame fixture)
    - `pregame_loadouts_by_subject`: the capture-based fixture → one `RawLoadout` per `Subject`; the entry without `Subject` is skipped.
    - `pregame_loadouts_invalid_is_empty`
    - `pregame_loadouts_gate`: `want_pregame_loadouts` is false once a valid answer was seen, and false when every ally has a `RawLoadout`.
    - `extract_player_loadout_reads_four_guns` (R90): the four weapons map to their `SkinID`/`ChromaID` (lower-case), including the upper-case Sheriff `ID`; a response without one of the weapons → `None` for it.
  - `src-tauri/src/imgproto.rs`: `("contentTier", AssetKind::ContentTier)` in `parses_every_kind`.
  - `src-tauri/src/pipeline_tests.rs`:
    - `pregame_loadouts_polled_once_then_stops`: drive the Pregame re-poll 4 times against the mock glz. The first `/pregame/v1/matches/{id}/loadouts` answer is `LoadoutsValid: false`, the next valid. Expect exactly 2 GETs, and 0 on re-polls 3 and 4. A second run where every ally already has a `RawLoadout` makes 0 GETs.
    - `player_details_resolves_skins_without_riot_sync` (R83, R97): the asset mock serves `/v1/weapons/skins/{skin}`. Open Details before the loadouts arrive → the four weapon entries are `null`. Then the loadouts arrive in `live.loadouts`. Reopen within 120s (`claim_details_sync` refuses) → the response has those weapon keys absent (resolving), and then a `details` event carries `WeaponSkin{skinName: "Reaver", chromaName: Some("Red"), tierId: Some(<uuid>)}`. A third open returns the skins directly and makes 0 new skin requests.
    - `failed_skin_fetch_settles_to_null` (R97): the skin mock answers 500 → the response has the key absent, the following `details` event has `null` for it, and nothing stays absent once `resolve_skins` has returned. The next open tries the fetch once more (R83).
    - `self_loadout_fetched_once_per_connect` (R90): the pd mock serves `/personalization/v2/players/{self}/playerloadout` from `player_loadout.json`. After the connect: exactly 1 GET. `player_details(self)` in the Lobby stage (no match loadouts) → after its `details` event, the Vandal entry is the fixture's Vandal skin (the asset mock serves that `SkinID`). More presence ticks → 0 new GETs. A reconnect → 1 more. No `/personalization/` request ever carries another puuid.
    - `others_loadout_null_outside_match` (R83): `player_details(other)` in the Lobby stage → all four weapons `null` (not absent), and no skin request is made.
    - `self_loadout_kept_in_app_state` (R90): after the connect, `app_state` `self_loadout` holds the four skins. A fresh `AppState` on the same DB with no session (Closed) → `player_details(self)` resolves the same skins from the stored copy.
- [ ] **Step 7:** Run `cd /home/project-1/.worktrees/p2-backend && cargo test -p opd1-riot -p opd1-trcker`. Expected: FAIL.
- [ ] **Step 8:** Implement:
  - The asset table, cache and kinds.
  - `details::build` uses `weapon_skin_cached` only (memory, cached-first), the Loadout source rule (R83, R90) and the Loadout state rule (R97).
  - `player_details` (commands.rs): before building, if `details::skins_pending`, mark the puuid resolving, so the answer has the resolving keys absent. After building, spawn `details::resolve_skins` for it, whether or not `claim_details_sync` allows a Riot sync. When it ends (success or failure), unmark, rebuild, and emit `"details"` if the result changed. A failed fetch is retried only on the next open (R83).
  - `poll.rs`, during Pregame, on the existing 3s re-poll: call `pregame_loadouts(match_id)` only while `want_pregame_loadouts` is true (≤ 1 call per re-poll, 0 after a valid answer). Merge the result into `live.loadouts`.
  - `poll.rs` `on_connected`: spawn `sync::fetch_self_loadout` once, after the self name (R90). The account-switch branch clears `live.self_loadout`.
  - Run `cd /home/project-1/.worktrees/p2-backend && cargo test -p opd1-core -p opd1-riot -p opd1-trcker && cargo clippy --workspace --all-targets -- -D warnings`. Expected: PASS.
- [ ] **Step 9: Commit + push** `feat: loadout skin names and content tiers, pregame and own loadouts` →
  ```bash
  cd /home/project-1/.worktrees/p2-backend && test "$(git branch --show-current)" = p2-backend && P="crates/riot/src/assets.rs crates/riot/src/endpoints.rs crates/riot/src/raw_capture.rs crates/riot/tests/assets.rs crates/riot/tests/endpoints_fixtures.rs crates/riot/tests/fixtures/weapon_skin_reaver_vandal.json crates/riot/tests/fixtures/pregame_loadouts.json crates/riot/tests/fixtures/player_loadout.json crates/core/src/model.rs src-tauri/src/roster.rs src-tauri/src/state.rs src-tauri/src/poll.rs src-tauri/src/sync.rs src-tauri/src/details.rs src-tauri/src/commands.rs src-tauri/src/imgproto.rs src-tauri/src/pipeline_tests.rs" && git add $P && git commit -m "feat: loadout skin names and content tiers, pregame and own loadouts" -- $P
  ```
  - Then the lane-B porcelain check and `cd /home/project-1/.worktrees/p2-backend && git push origin p2-backend`.
- [ ] **Step 10:** Write the failing contract test. `crates/core/tests/contract.rs` covers:
  - `live_state_matches_golden`: 7 cards built with `build_card`:
    - self visible and full;
    - remembered with a source (`fromMapId` = the Split uuid, never `/Game/…`);
    - remembered without a source;
    - hiddenPlayer, no agent;
    - hiddenAgent;
    - all loading (tri-state keys absent);
    - all missing (`null`).
  - `player_details_matches_golden`: recent rows with a tier and one with a `null` tier; topTeammates; a loadout with 2 skins, 1 `null` and 1 absent weapon (resolving, R97); 2 extended encounter rows.
  - `match_details_view_matches_golden`: one self-perspective 2-team view + one deathmatch view, as a 2-element JSON array.

  Each test compares `serde_json::to_value(...)` to `tests/fixtures/contract/<name>.json`; `UPDATE_GOLDEN=1` rewrites the files.
  - Run `cd /home/project-1/.worktrees/p2-backend && cargo test -p opd1-core --test contract`. Expected: FAIL (files missing).
- [ ] **Step 11:** Run `cd /home/project-1/.worktrees/p2-backend && UPDATE_GOLDEN=1 cargo test -p opd1-core --test contract` once.
  - Diff the goldens against spec §7.1. Every key must be spelled as in the spec; absent keys stand for loading (including the resolving loadout weapon).
  - Run again without the env var. Expected: PASS.
- [ ] **Step 12: Commit + push** `test(core): golden UI contract` → `cd /home/project-1/.worktrees/p2-backend && test "$(git branch --show-current)" = p2-backend && P="crates/core/tests/contract.rs crates/core/tests/fixtures/contract" && git add $P && git commit -m "test(core): golden UI contract" -- $P`, the lane-B porcelain check, `cd /home/project-1/.worktrees/p2-backend && git push origin p2-backend`. Then watch CI (Global "CI watch", lane B: dispatch on `p2-backend`). Expected: success.
- [ ] **Step 13: Land lane B on `main`** (R98). Task 13 has landed (Step 0), so `main` is an ancestor of `p2-backend`:
  ```bash
  cd /home/project-1 && test "$(git branch --show-current)" = main && git pull --ff-only origin main && git merge --ff-only p2-backend
  cd /home/project-1 && npm --prefix ui run build && cargo test -p opd1-core -p opd1-riot -p opd1-trcker && cargo clippy --workspace --all-targets -- -D warnings
  ```
  - If `--ff-only` refuses (`main` moved), run `cd /home/project-1 && git merge --no-ff p2-backend -m "feat: merge p2-backend (backend contract for design v8)"` instead, then the same tests.
  - Expected: PASS. Then the main porcelain check, `cd /home/project-1 && git push origin main`, and watch CI (Global "CI watch", `main`). Expected: success.
  - Task 16b may start once this push is green. Task 15b continues on `p2-backend`.

### Task 15b: Own-history backfill (R101)

**Lane B,** after Task 15 has landed on `main`, in `/home/project-1/.worktrees/p2-backend` on `p2-backend`. Every command starts with `cd /home/project-1/.worktrees/p2-backend &&`. It runs in parallel with Tasks 16b → 18 (they write only `ui/**` and `.gitignore`) and lands on `main` before Task 19 starts.

**Why (user 2026-09-29, ledger R101; `docs/requirements-raw.md` line 107):** when the app opens, it fetches, in the background and quietly, all the matches the user played while the app was closed, so no encounters or remembered names are missed. This narrows R33's per-player window for the user's own history only; other players keep R33.

**Files:**
- Modify:
  - `src-tauri/src/state.rs`: `Job::Backfill`; `LiveModel.backfill: Option<BackfillRun>`; `next_job` returns `Backfill` last; `start_backfill`; `live_job_pending`. An account switch clears `backfill`.
  - `src-tauri/src/sync.rs`: `BACKFILL_PAGE`, `BACKFILL_FIRST_RUN_MAX`, `BACKFILL_ANCHOR_KEY`, the pure `backfill_take`, `backfill_step`; `run_worker` handles `Job::Backfill`.
  - `src-tauri/src/poll.rs`: `on_connected` and the match-end transition (In Game → Lobby) start a run and ensure the worker.
  - `src-tauri/src/pipeline_tests.rs`
- Stored state: the `app_state` key `self_history_anchor` = JSON `{"matchId": "…", "startMs": N}`, through the existing `Repo::get_state`/`set_state`. No schema change.

**Interfaces:**
- Consumes: R101, R33, R45 (the worker claim); `RiotSession::match_history` (all queues when `queue` is `None`) and `match_details`; `Repo::index_history`, `Repo::new_match_ids`, `Repo::upsert_match`; `AppState::current_act()`.
- Produces (Rust):
  ```rust
  // state.rs
  pub enum Job { Names(Vec<(String, bool)>), Sync(String), Backfill }   // Backfill only when no Names/Sync job is left (lowest priority)
  pub struct BackfillRun {
      pub anchor: Option<(String, i64)>,   // the last run's newest (match_id, start_ms); None on the first run ever
      pub act_start_ms: Option<i64>,       // the current act's start, first run only (None when the act tables aren't loaded)
      pub next_index: u32,                 // the next match-history startIndex
      pub scanned: u32,                    // history entries looked at so far (first-run cap)
      pub newest: Option<(String, i64)>,   // page 0's first entry; becomes the anchor when the run completes
      pub pending: VecDeque<String>,       // ids from the current page still to check, kept when the step yields
  }
  pub fn start_backfill(&mut self, anchor: Option<(String, i64)>, act_start_ms: Option<i64>)  // no-op while a run is active
  pub fn live_job_pending(&self) -> bool   // !names_needed().is_empty() || next_unsynced().is_some()
  // sync.rs
  pub const BACKFILL_PAGE: u32 = 20;               // match-history's default page (techchrism MatchHistory.ts: endIndex defaults to 20)
  pub const BACKFILL_FIRST_RUN_MAX: u32 = 200;
  pub const BACKFILL_ANCHOR_KEY: &str = "self_history_anchor";
  pub struct BackfillTake { pub entries: Vec<HistoryEntry>, pub done: bool }
  pub fn backfill_take(page: &[HistoryEntry], anchor: Option<&(String, i64)>, act_start_ms: Option<i64>, scanned: u32) -> BackfillTake
      // Pure. `page` is newest first. Takes entries until, exclusive:
      //   - the anchor's match id, or the first entry older than the anchor's start (the anchor match may have left the history); or
      //   - with no anchor (first run): the first entry older than act_start_ms, or once scanned + taken reaches BACKFILL_FIRST_RUN_MAX.
      // done = one of those stops was hit, or the page is shorter than BACKFILL_PAGE (the history ended).
  pub async fn backfill_step(st: &Arc<AppState>, session: &Arc<RiotSession>) -> Result<(), Gone>
      // One page. If `pending` is empty: match_history(me, Some(next_index), Some(next_index + BACKFILL_PAGE), None),
      // index_history, backfill_take, then new_match_ids → pending (ids already stored are skipped here).
      // Then match-details for `pending`, one at a time (concurrency 1, through the session's throttle), checking
      // live_job_pending() before each fetch and returning early, with the cursor kept, as soon as a live job exists.
      // When the run is done and `pending` is empty: set_state(BACKFILL_ANCHOR_KEY, newest) and clear the run.
  ```
- Behaviour:
  - **Triggers.** On connect (`on_connected`, after `enter_closed`) and at match end (the stage goes In Game → Lobby), read the anchor from `app_state` and the current act, call `start_backfill`, then `ensure_worker`. A run already in progress keeps going.
  - **Why an anchor and not "the newest stored match".** The live sync stores the user's newest matches first (history 0..2, comp 0..5). If the backfill stopped at the newest stored match, it would stop at index 0 and never fill the gap behind it. The anchor moves only when a run completes, so an interrupted run (client closed, `Gone`) starts over next time from the same anchor, and the ids it already stored are skipped by `new_match_ids`.
  - **Priority.** `next_job` hands out `Backfill` only when no `Names` or `Sync` job is left. Inside a step, the backfill fetches one match-details at a time and checks `live_job_pending()` before each, so a new roster waits for at most one in-flight backfill request.
  - **Bounds.** Later runs stop at the anchor. The first run stops at the current act start or after 200 history entries, whichever comes first (200 only, when the act tables aren't loaded).
  - Every call goes through `RiotSession` (token bucket + `429 Retry-After`). All queues are paged, Deathmatch included (party detection uses them). Match details stay immutable: an id already in the DB is never fetched again.

- [ ] **Step 0:** Load `test-driven-development`. Confirm Task 15 has landed: `cd /home/project-1/.worktrees/p2-backend && git fetch origin && git merge-base --is-ancestor HEAD origin/main`. Code facts with graft (`graft skeleton src-tauri/src/state.rs`, `graft callers next_job`, `graft callers on_connected`); confirm each line in the worktree.
- [ ] **Step 1:** Write the failing tests:
  - `src-tauri/src/sync.rs` (test module, pure `backfill_take`; entries built in the test, newest first):
    - `backfill_take_stops_at_anchor_id`: the anchor is entry 12 of a 20-entry page → 12 entries taken, `done`.
    - `backfill_take_stops_older_than_anchor`: the anchor id isn't in the page, and entries 15+ are older than the anchor's start → 15 taken, `done`.
    - `backfill_take_first_run_stops_at_act_start`: no anchor, the act starts between entries 7 and 8 → 8 taken, `done`.
    - `backfill_take_first_run_caps_at_200`: no anchor, `scanned` 190, a full page all inside the act → 10 taken, `done`.
    - `backfill_take_short_page_is_done`: 6 entries, no stop hit → 6 taken, `done`.
  - `src-tauri/src/state.rs` (test module):
    - `backfill_job_is_lowest_priority`: with names needed and an unsynced slot, `next_job` returns `Names`, then `Sync`, and `Backfill` only once both are empty; with no run, it returns `None` and releases the claim.
    - `start_backfill_is_noop_while_running`
  - `src-tauri/src/pipeline_tests.rs` (the pd mock serves `match-history` honouring `startIndex`/`endIndex` from a list of synthetic entries, newest first, one-minute apart; `match-details` answers a copy of `match_details.json` with the requested id; requests are counted per path):
    - `backfill_stops_at_first_known_id`: the anchor is entry 22 of 30 → exactly 2 history GETs (0–20, 20–40) and match-details only for entries 0–21.
    - `backfill_skips_known_ids`: as above, with 3 of entries 0–21 already stored → exactly 19 match-details GETs; the anchor becomes entry 0. A second run → 1 history GET and 0 match-details GETs.
    - `backfill_first_run_respects_bound`: no anchor, 230 entries all inside the act → 10 history GETs and at most 200 match-details GETs. A second setup with the act start between entries 30 and 31 → 2 history GETs and 31 match-details GETs.
    - `backfill_never_delays_live_card_sync`: a first-run backfill of 60 unknown matches is running when a roster with a new unsynced player arrives → that player's first sync request (`/mmr/v1/players/{puuid}/competitiveupdates`) reaches the mock before the backfill's next `match-details` request, with at most 1 backfill request in flight at that moment (order of the mock's received requests).
    - `backfill_interrupted_keeps_anchor`: the session drops (`ClientGone`) mid-run → the stored anchor is unchanged; the next connect's run fetches only the ids still missing and then moves the anchor to the newest entry.
- [ ] **Step 2:** Run `cd /home/project-1/.worktrees/p2-backend && npm --prefix ui run build && cargo test -p opd1-trcker`. Expected: FAIL (the job, the run and the helpers don't exist).
- [ ] **Step 3:** Implement Interfaces → Produces and the Behaviour list.
- [ ] **Step 4:** Run `cd /home/project-1/.worktrees/p2-backend && cargo test -p opd1-core -p opd1-riot -p opd1-trcker && cargo clippy --workspace --all-targets -- -D warnings`. Expected: PASS.
- [ ] **Step 5: Commit + push** `feat(sync): backfill the user's own match history in the background` →
  ```bash
  cd /home/project-1/.worktrees/p2-backend && test "$(git branch --show-current)" = p2-backend && P="src-tauri/src/state.rs src-tauri/src/sync.rs src-tauri/src/poll.rs src-tauri/src/pipeline_tests.rs" && git add $P && git commit -m "feat(sync): backfill the user's own match history in the background" -- $P
  ```
  - If Step 3 had to touch any other file under `crates/` or `src-tauri/`, add it to `P`. Then the lane-B porcelain check, `cd /home/project-1/.worktrees/p2-backend && git push origin p2-backend`, and watch CI (Global "CI watch", lane B: dispatch on `p2-backend`). Expected: success.
- [ ] **Step 6: Land on `main`** as in Task 15 Step 13 (the merge message, if a fast-forward is refused: `feat: merge p2-backend (own-history backfill)`), then the tests, the main porcelain check, `cd /home/project-1 && git push origin main`, and watch CI on `main`. Expected: success. Nothing else writes `main` between Task 15's landing and this step, so the fast-forward should apply.

### Task 16a: UI foundation, contract-independent (lane C, parallel with Tasks 13–15)

**Lane C.** Directory: `/home/project-1/.worktrees/ui`, branch `ui-lane`. Every command starts with `cd /home/project-1/.worktrees/ui &&` (Playwright: `cd /home/project-1/.worktrees/ui/ui &&`). Writes only `ui/src/**` and `ui/vite.config.ts`. It needs nothing from Tasks 13–15: the wire types come from spec §7.1 as TS-only types, and Task 16b reconciles them with the Rust goldens. Spec sections are read from `/home/project-1/.superpowers/design/cd-v8/spec.md` (Task 13 commits the identical text as `docs/design-spec.md`).

**Files (all under `/home/project-1/.worktrees/ui`):**
- Merge: commit `c9ad7f4` into `ui-lane`. That is `main` before any Phase 2 commit; not the moving `main`.
- Delete (own commit):
  - `ui/src/components/{PlayerCard,PlayerName,Last5,RankBadge,EncounterBadge}.svelte`
  - `ui/src/lib/demo/CardGallery.svelte`
  - `ui/src/lib/tokens.css`, together with the `@import "./lib/tokens.css";` line at `ui/src/app.css:1`
  - `ui/src/assets/fonts/barlow-semi-condensed-*.woff2`, with their `@font-face` rules
  - the gallery hook in `ui/src/App.svelte`. Its tab styles use the deleted tokens (`var(--fs-md)`, `var(--text-1)` …), so in the delete commit `App.svelte` becomes a bare `<main></main>` placeholder; Step 5 replaces it.
- Keep, then modify as below (audit §4.2): `ui/src/lib/{types,stores,format,meta,assets,cardTips}.ts`, `ui/src/lib/tooltip.svelte.ts`, `ui/src/components/{Tooltip,Skeleton}.svelte`, `ui/src/lib/demo/{feed.ts,fixtures.ts,meta.json}`, `ui/src/lib/{format,cardTips}.test.ts`.
- Modify:
  - `ui/src/lib/types.ts`: **provisional.** Exactly spec §7.1, as TS-only types, exported as the named types listed there: `Stage, Result, Team, DisplayName, Rank, Peak, Last5Entry, PartyMark, EncounterCount, PlayerCard, LiveState, MatchSummary, WeaponSkin, Loadout, EncounterDetailRow, TeammateSummary, PlayerDetails, MatchTeamView, ScoreRow, MatchDetailsView`, plus the unchanged `Meta`. `Loadout`'s four weapons are optional (`vandal?: WeaponSkin | null`, R97). New names match the Rust structs; ui-lane's `Encounters` becomes `EncounterCount`. First line: `// Provisional (Task 16a): written from spec §7.1. Task 16b syncs it with crates/core/tests/fixtures/contract/*.json.`
  - `ui/src/lib/format.ts`: spec §4.3, including `count(n, word)`.
  - `ui/src/lib/assets.ts`: kinds `agent | tier | mapList | chroma | cardLarge | contentTier` (plus the existing `cardSmall | cardWide | mapSplash`).
  - `ui/src/lib/meta.ts`: the one spec §5.8 lookup (R95), see Produces. `tips.ts` (here), `live.ts` (Task 17) and `details.ts` (Task 18) import it and never re-implement it. `tierLabel`, `agentName`, `mapName` and `tierColor` take `Meta | null`.
  - `ui/src/lib/stores.ts`: the `meta` store starts `null` (pending) instead of `EMPTY_META`. It keeps its DEV-only dynamic import of the demo (`if (import.meta.env.DEV) … await import(…)`), never a static one.
  - `ui/src/lib/tooltip.svelte.ts`: spec §4.1 engine + `use:tip` action + pure `tipPosition`.
  - `ui/src/lib/cardTips.ts` → rename to `ui/src/lib/tips.ts`, the spec §4.2 catalogue; same for its test.
  - `ui/src/components/Tooltip.svelte`: spec §4.1 node + the structured `wl:` rows.
  - `ui/src/lib/demo/fixtures.ts`: rewritten on the new wire types (R95) as the card and row **builders** that `cdv8.ts` and `scenes.ts` use (for example `card(…)`, `last5(…)`, `recent(…)`). It no longer exports a scene list, `buildScene`, `isScene` or `SceneName`, because `scenes.ts` is the only registry.
  - `ui/src/lib/demo/feed.ts`: imports `SCENES` and `SceneName` from `./scenes`, not `./fixtures`. It does URL scene/route/theme parsing and keys 0–8 (R12), and keeps the `demo-feed` marker (R15).
  - `ui/src/app.css`: spec §0 global CSS + 12 `@font-face` rules with `font-display: block`.
  - `ui/src/App.svelte`: the `#app` root per spec §2.1, the route switch, `<Tooltip/>`.
  - `ui/src/main.ts`
  - `ui/vite.config.ts`: set `process.env.TZ = 'UTC'` at the top. The `dateLong`, `dateShort` and `time12` tests depend on the local zone, and vitest workers inherit the variable.
- Create:
  - `ui/src/lib/theme.ts`: pure. `TONES, FONTS, ACCENTS, PARTY_SETS4, DEFAULT_THEME, mix, rgba, partySet, themeVars, applyTheme, themeFromSearch`.
  - `ui/src/lib/themeStore.svelte.ts`: a `$state` theme initialised with `DEFAULT_THEME`, the DEV URL override, and `applyTheme` on change.
  - `ui/src/lib/router.svelte.ts`
  - `ui/src/lib/boot.ts`
  - `ui/src/lib/api.ts`: Tauri `invoke`; in DEV without Tauri, the scene registry through `if (import.meta.env.DEV) { const { SCENES } = await import('./demo/scenes'); … }`. Never a static import of `demo/*` from `api.ts`, `boot.ts` or `stores.ts`.
  - `ui/src/lib/demo/cdv8.ts`: the design's mock data (v8:427–497 constants + v8:558–692 derivations), ported to the wire types, anchored at `DEMO_NOW = Date.parse('2026-09-29T21:59:00Z')`.
  - `ui/src/lib/demo/scenes.ts`: every spec §8.2 scene (including `closed-empty`, `lobby2-loading`, `lobby2-unranked`, `ingame-nometa`, `details-longnames`, `details-partial` and `match-longnames`), plus `boot` (no LiveState yet) and `match-loading`. Spec §8.2's `tips` row is not a scene: Task 17's `tips-live.spec.ts` and Task 18's `tips-details.spec.ts` carry it out on the existing scenes.
  - Stubs replaced later: `ui/src/components/TopBar.svelte` (wordmark + tabs per spec §2.2, right group empty), `ui/src/screens/{Live,Details,Match}.svelte`.
  - `ui/src/assets/fonts/{barlow,ibm-plex-sans,archivo}-latin-{500-normal,600-normal,700-normal,500-italic}.woff2` (12 files, from `npm pack @fontsource/<family>` in the scratchpad; the packages are not added as dependencies), with `ui/src/assets/fonts/OFL.txt` replaced by the three families' OFL 1.1 texts.
  - Tests: `ui/src/lib/{theme,assets,tooltip,meta,boot,router,scenes,fonts}.test.ts`. `fonts.test.ts` starts with `/// <reference types="node" />`, because `tsconfig.app.json` lists only the `svelte` and `vite/client` types (`@types/node` is already a devDependency).

**Interfaces:**
- Consumes: spec §1 (theme), §2.1–§2.4 (root, top-bar tabs, routes, startup, keys), §4 (tooltips), §4.3 (formatters), §5.8 (meta lookups), §7.1 (types, TS only), §8.2 (scenes); audit §4; R95, R97, R99.
- Produces (TS):
  ```ts
  // theme.ts
  export type ThemeOptions = { tone: 'Neutral' | 'Cool' | 'Warm'; accent: string;
    results: 'Green / Red' | 'Accent / Red' | 'Accent / Gray'; resultFill: 'Tinted' | 'Solid';
    font: 'Barlow' | 'IBM Plex Sans' | 'Archivo'; last5Style: 'Arrow' | 'Underline' };
  export const DEFAULT_THEME: ThemeOptions; // Neutral, #e3a54f, Green / Red, Tinted, Barlow, Arrow
  export function mix(a: string, b: string, t: number): string;  export function rgba(h: string, a: number): string;
  export function partySet(accent: string): [string, string, string, string];      // spec §1.6
  export function themeVars(o: ThemeOptions): Record<string, string>;             // spec §1.8 names (--bg … --party-3, --r-*, --rn-*, --font, --on-solid)
  export function applyTheme(o: ThemeOptions): void;                               // writes vars on :root, data-last5 on #app
  export function themeFromSearch(search: string): Partial<ThemeOptions>;         // ?tone=&accent=&results=&fill=&font=&last5=
  // assets.ts
  export type AssetKind = 'agent' | 'tier' | 'mapList' | 'chroma' | 'cardLarge' | 'contentTier' | 'cardSmall' | 'cardWide' | 'mapSplash';
  export function assetUrl(kind: AssetKind, id: string | number): string; // Tauri: img://localhost/<kind>/<id> (Windows: http://img.localhost/…); DEV browser: the design's CDN URLs
  // meta.ts (spec §5.8; the only copy of this rule, R95)
  export function lookup(meta: Meta | null, table: 'maps' | 'agents', id: string | null | undefined): string | null | undefined;
    // undefined: meta still pending (render a skeleton bar); null: no id, or the id is missing from a loaded table (render '—'); else the name
  export function agentName(meta: Meta | null, id: string | null | undefined): string | null | undefined;   // lookup(meta, 'agents', id)
  export function mapName(meta: Meta | null, id: string | null | undefined): string | null | undefined;     // lookup(meta, 'maps', id)
  export function tierLabel(meta: Meta | null, tier: number | null | undefined): string | null | undefined; // same rule; tier < 3 → null
  export function tierColor(meta: Meta | null, tier: number | null | undefined): string | undefined;
  // (EMPTY_META, isRankedTier, cssColor and FIRST_UNCAPPED_TIER stay)
  // tooltip.svelte.ts
  export function tip(node: HTMLElement, arg: string | { text: string; overflowOnly: true }): { update(a: typeof arg): void; destroy(): void }; // sets data-tip / data-tip-ov
  export function tipPosition(r: DOMRectReadOnly, R: DOMRectReadOnly): { x: number; top: number; transform: string }; // spec §4.1
  export function parseWl(text: string): { w: number; l: number; g: number } | null;
  export function hideTip(): void;
  // tips.ts (spec §4.2 catalogue, pure; meta parts come from meta.ts, and a pending or missing part is dropped, spec §5.8)
  export function nameTip(n: DisplayName, meta: Meta | null): string;  export function peakTip(p: Peak): string;
  export function winTip(c: PlayerCard): string;  export function last5Tip(e: Last5Entry, nowMs: number): string;
  export function encounterTip(e: EncounterCount): string;  export function partyTip(p: PartyMark): string;
  export function aprTip(n: number): string;       // 'Average party rank\nFrom 1 ranked player' / '…From 3 ranked players'
  // format.ts (addition)
  export function count(n: number, word: string): string;   // '1 game', '53 games' (spec §4.3)
  // router.svelte.ts
  export type Route = { name: 'live' } | { name: 'details'; puuid: string; more: boolean }
    | { name: 'match'; matchId: string; focusPuuid: string; from: 'recent' | 'encounter'; back: { puuid: string; more: boolean } };
  export const router: { route: Route };            // $state, starts { name: 'live' } (R71)
  export function go(r: Route): void;               // hides the tip, resets the scroll container
  export function onStage(prev: Stage | null, next: Stage): void; // → pregame: go({ name: 'live' })
  // boot.ts
  export interface Bridge { listen<T>(ev: string, cb: (e: { payload: T }) => void): Promise<() => void>;
    invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> }
  export async function startLive(b: Bridge, apply: (s: LiveState) => void): Promise<void>; // listen('live') then invoke('live_state'); invoke result applied only if no event came first
  export function keyAction(e: Pick<KeyboardEvent, 'key' | 'ctrlKey' | 'shiftKey' | 'altKey' | 'metaKey'>): 'copyLog' | 'dumpRaw' | null;
  export function applyDetailsEvent(open: PlayerDetails | null, payload: PlayerDetails): PlayerDetails | null; // replace only when card.puuid matches
  // api.ts (Tauri invoke, or the DEV scene registry via a DEV-guarded dynamic import)
  export function getPlayerDetails(puuid: string): Promise<PlayerDetails>;
  export function getMatchDetails(matchId: string, focusPuuid: string): Promise<MatchDetailsView>;   // invoke('match_details', { matchId, focusPuuid })
  // demo/scenes.ts (the only scene registry)
  export type SceneName = 'boot' | 'closed' | 'closed-empty' | 'lobby1' | 'lobby2' | 'lobby2-loading' | 'lobby2-unranked' | 'lobby3' | 'lobby4' | 'lobby5'
    | 'pregame' | 'pregame-nomap' | 'pregame-loading' | 'ingame' | 'ingame-longnames' | 'ingame-parties4' | 'ingame-partial' | 'ingame-unranked'
    | 'ingame-loading' | 'ingame-nometa'
    | 'details' | 'details-more' | 'details-self' | 'details-hidden' | 'details-empty' | 'details-loading' | 'details-longnames' | 'details-partial'
    | 'match' | 'match-enc' | 'match-hidden' | 'match-dm' | 'match-loading' | 'match-longnames';
  export const SCENES: Record<SceneName, { live: LiveState | null; route: Route;
    details?: Record<string, PlayerDetails | 'pending'>; matches?: Record<string, MatchDetailsView | 'pending'>;
    meta?: Meta | 'pending' }>;   // meta defaults to demo/meta.json; 'pending' never resolves (ingame-nometa)
  ```
- Produces (behaviour):
  - Startup per spec §2.4: route `live`, `startLive`, `invoke('meta')`, `listen('details')`.
  - Ctrl+Shift+L → `invoke('copy_log')` then `navigator.clipboard.writeText`. Ctrl+Shift+D → `invoke('dump_raw')`. No UI for either.
  - Auto-switch to Live on `pregame`.
  - Theme vars applied before the first paint.

- [ ] **Step 0:** Load `svelte-code-writer` (mandatory for every `.svelte` / `.svelte.ts` edit), `svelte-core-bestpractices`, `test-driven-development` and `webapp-testing`. Code facts from graft (`graft skeleton ui/src/lib/stores.ts`, `graft grep "demo/"`); graft indexes `/home/project-1`, so confirm each fact in the UI worktree.
- [ ] **Step 1:** Merge `c9ad7f4` and push the branch:
  ```bash
  cd /home/project-1/.worktrees/ui && test "$(git branch --show-current)" = ui-lane && git merge c9ad7f4 -m "chore(ui): merge main (c9ad7f4) into ui-lane" && git push -u origin ui-lane
  ```
  - Expected: a clean merge (audit §4.1). `docs/design-spec.md` does not exist yet on this branch; read the spec from `.superpowers/`.
  - `cd /home/project-1/.worktrees/ui && npm --prefix ui run test` passes the existing 27 tests.
- [ ] **Step 2:** Delete the v1 layer (Files → Delete), including the `app.css` `@import` of `tokens.css`, and reduce `App.svelte` to the placeholder.
  - Run `cd /home/project-1/.worktrees/ui && npm --prefix ui run test && npm --prefix ui run check && npm --prefix ui run build`. Expected: PASS.
  - **Commit + push** `chore(ui): remove v1 visual layer` → `cd /home/project-1/.worktrees/ui && test "$(git branch --show-current)" = ui-lane && git add -u -- ui/src && git commit -m "chore(ui): remove v1 visual layer" -- ui/src && git push origin ui-lane`.
- [ ] **Step 3:** Write the failing vitest tests:
  - `theme.test.ts`:
    - `default vars equal the spec §1.4/§1.5 Neutral column`: every token, incl. `--r-w-bg #284239` and `--rn-w-bg #5fbf8b`.
    - `computed tints per tone` (the §1.4 table)
    - `party sets for the 4 accents` (the §1.6 table)
    - `unknown accent uses the fallback rule`
    - `themeFromSearch parses all 6 params`
  - `format.test.ts`, updated to §4.3:
    - `span`;
    - `ago`;
    - `rrDelta(−15) === '−15'` (U+2212);
    - `pct0(0.639) === '64%'`;
    - `count(1, 'game') === '1 game'`, `count(0, 'game') === '0 games'`, `count(3, 'ranked player') === '3 ranked players'`;
    - `kd2`;
    - `score(13,9) === '13–9'`;
    - `actDates` for the 3 spec examples;
    - `actTitle('V26 ACT V') === 'VALORANT 2026 · Act 5'` and `actTitle('EPISODE 9 ACT III') === 'Episode 9 · Act 3'`, with unparseable input passed through;
    - `dateLong`, `dateShort` (year shown only when not the current year), `time12`, `durationMin` (in UTC, pinned by `vite.config.ts`);
    - every formatter maps `null` to `'—'`.
  - `assets.test.ts`:
    - Tauri URLs per kind;
    - the Windows UA base;
    - DEV URLs equal the design's patterns: `agents/<id>/displayicon.png`, `competitivetiers/03621f52-342b-cf4e-4f86-9350a49c6d04/<t>/smallicon.png`, `maps/<id>/listviewicon.png`, `weaponskinchromas/<id>/fullrender.png`, `playercards/<id>/largeart.png`, `contenttiers/<id>/displayicon.png`.
  - `meta.test.ts` (R95; moved here from Task 17's `live.test.ts`):
    - `lookup: pending meta (null) → undefined, null id → null, missing id → null, known id → name`
    - `tierLabel follows the same rule; tier < 3 → null`
    - `agentName and mapName are lookup on their table`
  - `tooltip.test.ts`:
    - `tipPosition` clamps x to 136–864 and flips above when `below > 720`;
    - `parseWl('wl:31:22:53')`;
    - `tips.ts` strings:
      - `V26 A5 · Current act\nAug 26 – Oct 20, 2026`
      - `VALORANT 2026 · Act 4\nJun 24 – Aug 25, 2026`
      - remembered with both, one, or none of map/date
      - `+18 RR · 45m`
      - `Win · 2h` when `rrEarned` is null
      - `1 with · 2 against`
      - `Party · likely`
      - `winTip` is `''` when `actGames` is null
      - `aprTip(1) === 'Average party rank\nFrom 1 ranked player'`, `aprTip(3)` ends `From 3 ranked players`
      - the structured W/L row reads `1W · 0L · 1 game` for `wl:1:0:1`
      - meta misses and pending meta (spec §5.8): a remembered name whose map id is not in meta, or with `meta` `null`, shows only the date line; no tip string contains `undefined` or `null`
  - `boot.test.ts`:
    - `listen before invoke`
    - `invoke result ignored after an event`
    - `keyAction` (Ctrl+Shift+L / D only)
    - `applyDetailsEvent replaces only the matching puuid`
  - `router.test.ts`:
    - `starts on live`
    - `pregame switches to live from details`
    - `match back returns to details with more kept`
    - `go hides the tip and resets the scroll container` (a tip shown via the engine, then `go(...)` → hidden; the container's `scrollTop` is 0)
  - `scenes.test.ts`:
    - every `SceneName` resolves;
    - `ingame` has 5+5 cards;
    - Fahad's `last5.map(r => r.result)` = `['win','loss','win','win','loss']` (newest first);
    - `Math.round(actWinrate*100)` equals the design's `win[0]` for all 10 players;
    - `lobby2` has exactly 1 ranked player; `ingame-longnames` peaks are `V26 A4` except one `E10 A3`, all `11mo`, and one card's `with + against` is 123;
    - `pregame` (R99): the hovered and the unpicked ally both have `agentId: null`; the hidden ally who hasn't locked is `{kind: 'hiddenPlayer'}`; only the 2 locked allies have an `agentId`;
    - `details` (Mara) has 8 recent rows (Lotus/Sunset/Split first; scores `13–9, 13–11, 7–13`), 3 encounters and Kilo as a live teammate;
    - `details-partial` has exactly one loadout weapon key absent (resolving, R97) and one `null`;
    - `match` has halves `7–4 · 6–5` and a header string of `Competitive · Sep 29, 2026 · 9:14 PM · 38 min`;
    - `fixtures.ts` exports no `buildScene`, `isScene` or scene list (one registry, R95);
    - the `demo-feed` marker is set by `startDemo`.
  - `fonts.test.ts`:
    - the 12 files exist and total ≤ 250 KB;
    - `app.css` has exactly 12 `@font-face` blocks with `font-display: block`, and no `@import`;
    - no `fonts.googleapis`, `fonts.gstatic` or `unpkg` string anywhere under `ui/src`.
- [ ] **Step 4:** Run `cd /home/project-1/.worktrees/ui && npm --prefix ui run test`. Expected: FAIL.
- [ ] **Step 5:** Implement the Files → Modify/Create.
- [ ] **Step 6:** Run `cd /home/project-1/.worktrees/ui && npm --prefix ui run test && npm --prefix ui run check && npm --prefix ui run build`, then `cd /home/project-1/.worktrees/ui && ! grep -rqE "demo-feed|VesperUnbreakabl|Vantablack|fonts.googleapis|fonts.gstatic|unpkg.com" ui/dist`. Expected: PASS. `VesperUnbreakabl` and `Vantablack` exist only in `demo/cdv8.ts`, so this catches a scene bundle that leaked without the marker.
  - Also run the dev server from the UI worktree, open `?scene=ingame` in Playwright's Chromium once, and confirm `getComputedStyle(document.documentElement).getPropertyValue('--bg') === '#0e1013'`.
- [ ] **Step 7: Commit + push** `feat(ui): design v8 foundation (theme, fonts, img kinds, tooltip, meta lookup, boot, scenes)` →
  ```bash
  cd /home/project-1/.worktrees/ui && test "$(git branch --show-current)" = ui-lane && git add ui/src ui/vite.config.ts && git commit -m "feat(ui): design v8 foundation (theme, fonts, img kinds, tooltip, meta lookup, boot, scenes)" -- ui/src ui/vite.config.ts
  ```
  - Then the UI-lane porcelain check (Global Git) and `cd /home/project-1/.worktrees/ui && git push origin ui-lane`.

### Task 16b: UI contract sync (after Task 15 has landed on `main`)

**UI lane,** first of the sequential tasks. Directory: `/home/project-1/.worktrees/ui`, branch `ui-lane`. Every command starts with `cd /home/project-1/.worktrees/ui &&`. It starts after Task 16a and Task 15 Step 13 (the lane-B landing on `main`, green CI).

**Files:**
- Merge: `origin/main` into `ui-lane`. It brings Task 13's docs (including `docs/design-spec.md`) and Tasks 14–15's code and goldens.
- Modify:
  - `ui/src/lib/types.ts`: reconciled with the goldens and the Rust struct names; the "Provisional" first line is removed.
  - `ui/vite.config.ts`: vitest `typecheck: { enabled: true, include: ['src/**/*.test-d.ts'], tsconfig: './tsconfig.app.json' }`. `ui/tsconfig.json` only lists references (`files: []`), so without the explicit `tsconfig` the type tests would run with default compiler options.
  - Any Task 16a file that used a provisional name or shape the goldens changed (for example `demo/fixtures.ts`, `demo/cdv8.ts`, `tips.ts`).
- Create: `ui/src/lib/contract.shape.ts` (test-only), `ui/src/lib/contract.test.ts`, `ui/src/lib/contract.test-d.ts`.

**Interfaces:**
- Consumes: `crates/core/tests/fixtures/contract/{live_state,player_details,match_details_view}.json` (Task 15), spec §7.1, R96, R97.
- Produces (TS, test-only):
  ```ts
  // contract.shape.ts
  export type Kind = 'string' | 'number' | 'boolean' | 'null' | 'object' | 'array';
  export type Desc = { kinds: Kind[]; optional?: true; shape?: string };   // `shape` names a nested descriptor ('PlayerCard', 'DisplayName', …)
  export type ShapeOf<T> = { [K in keyof T]-?: Desc };                      // every key of T, no extra key
  export const shapes = {
    PlayerCard: { puuid: { kinds: ['string'] }, name: { kinds: ['object', 'null'], optional: true, shape: 'DisplayName' }, /* … */ } satisfies ShapeOf<PlayerCard>,
    // … LiveState, MatchSummary, WeaponSkin, Loadout, EncounterDetailRow, TeammateSummary, PlayerDetails,
    //   MatchTeamView, ScoreRow, MatchDetailsView, and one entry per DisplayName variant (keyed by `kind`)
  };
  ```
  Each descriptor is written from the golden JSON (the Rust truth). `satisfies` keeps each literal's own type, so the key-set assertions below are real, not tautological (R96).

- [ ] **Step 0:** Load `test-driven-development` (and `svelte-code-writer` if a `.svelte` file must change).
- [ ] **Step 1:** Merge `main` and push:
  ```bash
  cd /home/project-1/.worktrees/ui && test "$(git branch --show-current)" = ui-lane && git fetch origin && git cat-file -e origin/main:crates/core/tests/fixtures/contract/player_details.json && git merge origin/main -m "chore(ui): merge main into ui-lane" && git push origin ui-lane
  ```
  - Expected: a clean merge (`ui-lane` wrote only `ui/src` and `ui/vite.config.ts`; `main` wrote no `ui/` file in Tasks 13–15).
  - `cd /home/project-1/.worktrees/ui && npm --prefix ui run test` passes Task 16a's tests.
  - `cd /home/project-1/.worktrees/ui && cargo test -p opd1-core --test contract` passes.
- [ ] **Step 2:** Write the failing tests:
  - `contract.shape.ts` as in Produces, one descriptor per named type.
  - `contract.test-d.ts`, for each named type `X`:
    - `expectTypeOf<keyof typeof shapes.X>().toEqualTypeOf<keyof X>()` (the same key set both ways);
    - one negative case that must stay an error: `// @ts-expect-error a missing key must fail` above `({ puuid: { kinds: ['string'] } }) satisfies ShapeOf<PlayerCard>`. It proves the guard can fail.
  - `contract.test.ts` (starts with `/// <reference types="node" />`): read each golden under `../../../crates/core/tests/fixtures/contract/` and walk it with the descriptors:
    - every object's keys ⊆ its descriptor's keys, and every non-`optional` descriptor key is present;
    - each value's kind is one of the descriptor's `kinds`; nested objects are checked against their `shape`; `DisplayName` variants are picked by `kind`;
    - the `player_details` golden has exactly one absent loadout weapon (R97), and every `fromMapId` is a uuid or `null`, never `/Game/…`.
  - The `vite.config.ts` typecheck block.
  - Run `cd /home/project-1/.worktrees/ui && npm --prefix ui run test && npm --prefix ui run check`. Expected: FAIL. The three files are new, and any key the provisional types spell differently from the goldens fails `satisfies` or the key-set assertions. If only the missing files fail, note "no provisional drift" in the report.
- [ ] **Step 3:** Reconcile `types.ts` with the goldens and the Rust struct names. Rust wins: a golden key that disagrees with spec §7.1 is a Task 15 bug, so stop and report it; don't bend the TS around it. Fix the Task 16a code that used a changed name. Remove the "Provisional" first line.
- [ ] **Step 4:** Run `cd /home/project-1/.worktrees/ui && npm --prefix ui run test && npm --prefix ui run check && npm --prefix ui run build`, then `cd /home/project-1/.worktrees/ui && ! grep -rqE "demo-feed|VesperUnbreakabl|Vantablack|fonts.googleapis|fonts.gstatic|unpkg.com" ui/dist`. Expected: PASS.
- [ ] **Step 5: Commit + push** `feat(ui): sync wire types with the backend contract (golden JSON + type tests)` →
  ```bash
  cd /home/project-1/.worktrees/ui && test "$(git branch --show-current)" = ui-lane && git add ui/src ui/vite.config.ts && git commit -m "feat(ui): sync wire types with the backend contract (golden JSON + type tests)" -- ui/src ui/vite.config.ts
  ```
  - Then the UI-lane porcelain check and `cd /home/project-1/.worktrees/ui && git push origin ui-lane`.

### Task 17: Live page (Lobby / Agent Select / In Game / Closed)

**UI lane,** after Task 16b. Directory: `/home/project-1/.worktrees/ui`, branch `ui-lane`.

**Files (ui-lane):**
- Create/replace:
  - `ui/src/screens/Live.svelte`: the grid, slot model, stages. Spec §3.1, §5.1–§5.7.
  - `ui/src/components/TopBar.svelte`: the right group per stage, spec §2.2–§2.3.
  - `ui/src/components/`:
    - `AvgPartyRank.svelte`, `MapGroup.svelte`
    - `PlayerCard.svelte`, `CardSkeleton.svelte`, `EmptySlot.svelte`, `AgentBox.svelte`
    - `Name.svelte` (sizes `card | hero | row`), `EncounterBadge.svelte`
    - `RankCurrent.svelte`, `PeakRow.svelte`, `PrevRow.svelte`
    - `StatTrio.svelte`, `Last5.svelte` (sizes `card | hero`), `PartyMark.svelte`, `Skel.svelte`
  - `ui/src/components/icons/{EyeOff,Rewind,PartyGlyph,ChevronRight}.svelte`: the spec §3.4/§3.5/§3.9/§6.4 SVGs.
  - `ui/src/lib/live.ts`: pure helpers.
  - `ui/src/lib/resultBox.ts`: W/L/D/E and newest → CSS var style, spec §1.5.
  - Tests:
    - `ui/src/lib/live.test.ts`
    - `ui/playwright.config.ts`
    - `ui/tests/harness.ts`: clock, routes, scene URL, font check.
    - `ui/tests/checks.ts`: spec §8.3 global assertions as functions.
    - `ui/tests/fit-live.spec.ts`
    - `ui/tests/fit-dpr.spec.ts`: `test.use({ deviceScaleFactor: 1.5 })`, spec §8.6's DPR checks.
    - `ui/tests/tips-live.spec.ts`
    - `ui/tests/interaction.spec.ts`: hover styles and inert targets (Task 18 extends it).
    - `ui/tests/design-ref.ts`
    - `ui/tests/visual-live.spec.ts`
- Modify: `.gitignore`: add `ui/tests/shots/`, `ui/tests/.cache/`, `ui/tests/ref/`, `ui/test-results/`, `ui/playwright-report/`. Remove the duplicate `/src-tauri/gen/schemas/` line and its `# Tauri` comment: `src-tauri/.gitignore:3` already covers it (audit §5.9, ledger Task 1 minor).

**Interfaces:**
- Consumes: Tasks 16a/16b's types, theme vars, `tip`, the `meta.ts` lookups (`lookup`, `tierLabel`, `agentName`, `mapName`; R95), router, scenes; spec §2.2–§2.3, §3, §4.2 (card and top-bar rows), §5, §8.1–§8.6 (Live parts); R99 (lock-only agent).
- Produces:
  ```ts
  // live.ts
  export type SlotItem = { kind: 'card'; card: PlayerCard } | { kind: 'skeleton' } | { kind: 'dashed' } | { kind: 'none' };
  export function liveColumns(s: LiveState | null): { left: SlotItem[]; right: SlotItem[] }; // always 5 + 5 items (spec §3.1, §5.7); self first on the left, else emitted order; closed + players [] → all 'none' (spec §5.3)
  export type Tile = { key: 'W' | 'L' | 'D' | 'E'; newest: boolean; entry: Last5Entry | null };
  export function last5Tiles(e: Last5Entry[] | null | undefined): Tile[] | 'skeleton'; // empties left, newest rightmost (spec §3.8)
  export function avgPartyRank(ps: PlayerCard[]): { tier: number | null; n: number; loading: boolean } | null; // null when < 2 players (spec §2.3)
  export const showEncounters: (c: PlayerCard) => boolean;   // !isSelf && !isSelfParty && with+against > 0
  export const showParty: (c: PlayerCard) => boolean;        // party && !isSelf && !isSelfParty
  // Meta names (map, agent, tier) come from meta.ts (Task 16a): undefined → skeleton bar, null → '—' (spec §5.8). live.ts has no lookup of its own (R95).
  // resultBox.ts
  export function boxStyle(key: 'W' | 'L' | 'D' | 'E', newest?: boolean): string; // 'background:var(--r-w-bg);color:var(--r-w-fg);border:var(--r-w-bd)'
  ```
  Svelte 5 props (callback props, R13):
  - `PlayerCard`: `{ card: PlayerCard; onopen?: (puuid: string) => void }`
  - `Name`: `{ name: DisplayName | null | undefined; size: 'card' | 'hero' | 'row'; tipMode?: 'card' | 'none' }`
  - `Last5`: `{ entries: Last5Entry[] | null | undefined; size: 'card' | 'hero' }`
  - `EncounterBadge`: `{ enc: EncounterCount; size: 'card' | 'hero' }`
  - `TopBar`: `{ route: Route; live: LiveState | null }`
  - `Live`: `{ live: LiveState | null }`

  Every tooltip trigger carries `data-tip` (spec §4.1).
- **Reference render of the design** (`ui/tests/design-ref.ts`; test harness only, never shipped):
  - `findDesignDir()` uses `OPD1_DESIGN_DIR`, else walks up from `ui/` to `.superpowers/design/cd-v8/project`. Absent → the visual specs `test.skip` with the message `design file not on this machine`.
  - `openDesign(page, props)`:
    - `page.route('http://design.local/**')` serves the design dir.
    - The `.dc.html` response has its `data-props` `default`s replaced by `props`: `screen`, `loading`, `tone`, `accent`, `results`, `resultFill`, `font`, `last5Style`.
    - `https://unpkg.com/**` is served from `ui/tests/.cache/cdn/` (git-ignored, test-only, never shipped). The first run fills it from the network byte for byte and keeps each response's `content-type`. Every fulfilled response adds `Access-Control-Allow-Origin: *`: `support.js` loads React and Babel with `integrity` + `crossOrigin='anonymous'`, so a response without that header fails the CORS check before SRI runs. The unchanged bytes keep the SRI hash valid, so never rewrite the body or drop the `integrity` attribute. With no network and an empty cache, the visual specs `test.skip` with `design CDN cache empty`.
    - `https://fonts.googleapis.com/css2**` returns generated `@font-face` CSS pointing at the 12 bundled woff2 files (served from `ui/src/assets/fonts/`), so both renders use identical font files. `fonts.gstatic.com` is aborted.
    - `https://media.valorant-api.com/**` uses the same cache as the app harness.
    - Then wait for `document.fonts.ready` and every `img.complete`.
    - Navigate like a user: click Mara's card, `Show more`, a recent row. After each click, move the pointer to (5,5) and wait 600ms.
  - The comparison screenshots both pages at 1000×800 with `style: '[style*="Segoe UI Symbol"], .enc .glyph{visibility:hidden!important}'` (`COMPARE_STYLE`). This hides only the ⟲ glyph, which the design draws as a Segoe UI Symbol character and we draw the same way (spec §3.5, R149), so that Linux, which has no such font, does not compare two fallback fonts; the count beside it stays in the comparison (R130).
  - The design PNG is written to `testInfo.snapshotPath(name)` in `beforeAll` on every run, with `snapshotPathTemplate: '{testDir}/ref/{arg}{ext}'` (git-ignored). Then `expect(ours).toMatchSnapshot(name, { maxDiffPixelRatio: 0.0002, threshold: 0.2 })`, i.e. at most 0.02% of the 800 000 pixels (160 px) may differ at a per-pixel YIQ threshold of 0.2. (The first draft said 1%, 8 000 px, set before the baseline was measured; the measured baseline is 20–40 px per In Game case, and R130 tightened the gate.)
  - The card's Prev row (R125, user-delegated) is masked in both renders as an approved deviation: the design draws `Prev` · icon · name, we draw icon · `Prev` · name. The visual spec asserts that the two masked rects are identical, so the mask cannot grow past the row.
  - A diff above 0.02% that comes from a documented intentional difference (a spec [C]/[bound] item, or an inconsistency in the design mock) is handled by narrowing the hide-style with a comment saying why. Never raise the threshold.

- [ ] **Step 0:** Load `svelte-code-writer` (mandatory for every `.svelte` edit), `svelte-core-bestpractices`, `test-driven-development` and `webapp-testing`. Code facts from graft. Every command runs as `cd /home/project-1/.worktrees/ui && …` (Playwright: `cd /home/project-1/.worktrees/ui/ui && npx playwright test …`).
- [ ] **Step 1:** Write the failing tests.
  - `live.test.ts`:
    - `closed → self only in slot 1`
    - `closed with players [] → no card and no skeleton`
    - `boot → 1 skeleton`
    - `menus keeps emitted order with self first`
    - `pregame → 5 dashed on the right`
    - `pregame-loading → 5 skeletons + 5 dashed`
    - `ingame-loading → 5 + 5 skeletons`
    - `last5Tiles puts empties left and the newest rightmost`
    - `last5Tiles(null) → 5 E`
    - `avgPartyRank rounds .5 up over tier ≥ 3 only`
    - `avgPartyRank with 0 ranked → tier null`
    - `avgPartyRank loading when none ranked and one absent`
    - `avgPartyRank lobby2 → n = 1`
    - `showEncounters / showParty suppress self and self-party`
    - (The `lookup` tests live in Task 16a's `meta.test.ts`.)
  - `ui/playwright.config.ts`:
    - project `chromium`, viewport 1000×800, `deviceScaleFactor: 1`, `timezoneId: 'UTC'`, `locale: 'en-US'`;
    - `webServer: { command: 'npm run dev -- --port 4173 --strictPort', url: 'http://127.0.0.1:4173', reuseExistingServer: false }` (R14);
    - the harness fixes the clock with `page.clock.setFixedTime(new Date('2026-09-29T21:59:00Z'))`.
  - `fit-live.spec.ts`:
    - `fit: <scene> [<font>]` for every scene in `closed, closed-empty, boot, lobby1…lobby5, lobby2-loading, lobby2-unranked, pregame, pregame-nomap, pregame-loading, ingame, ingame-longnames, ingame-parties4, ingame-partial, ingame-unranked, ingame-loading, ingame-nometa` × `Barlow, IBM Plex Sans, Archivo`. Each runs spec §8.3 checks 1, 2, 4, 5, 6, 7, 8, 9 (check 9 as a regression check only, R96) and saves `ui/tests/shots/<scene>__<font>.png`. Check 5 treats only the `data-ellipsis` spans as allowed to truncate; the Peak row is strict.
    - `geometry: ingame`: every spec §8.4 row, including the Underline row via `?last5=Underline`.
    - `pregame: 5 cards + 5 dashed slots` (spec §8.5); the hovered and the unpicked ally have no `img` in the agent box, and the hidden ally who hasn't locked shows `Hidden Player` (R99)
    - `lobby<n>: n cards, APR iff n ≥ 2, only self outlined`
    - `closed: one card, empty right side`; `closed-empty: no card, no skeleton`
    - `lobby2-loading / lobby2-unranked: APR skeleton / APR empty box + —` (spec §8.5)
    - `ingame-longnames: the 123 ⟲ count doesn't intersect its tag; no Peak row clips` (3 fonts)
    - `ingame-nometa: skeleton bars for the map name, tier names and hidden-agent name; no "undefined"/"null" text` (spec §5.8)
    - `last5 order: Fahad renders L W W L W and the rightmost is the newest style; 2-match player has 3 dashed tiles first`
    - `parties4: 4 distinct stripe colours = --party-0…3, likely = gradient, confirmed = solid, none on self/self-party`
    - `party colours: ingame-parties4 [<accent>]` for the 4 accents: the rendered stripe colours equal that accent's spec §1.6 row (DOM, not `themeVars`)
    - `options: ingame [<option>]` for each spec §8.6 `ingame` variant (checks 1–8 + screenshot)
    - `tier name right edge ≤ 463 for Ascendant 3 in all 3 fonts`
    - `no request to fonts.googleapis.com / fonts.gstatic.com`
  - `fit-dpr.spec.ts`: `dpr1.5: <scene> [<font>]` for `ingame-longnames` and `pregame` × 3 fonts, spec §8.3 checks 2, 5 and 6.
  - `tips-live.spec.ts`:
    - Every card and top-bar row of spec §4.2 hovered in turn on `ingame`, `ingame-longnames`, `lobby3` and `pregame` (`Hidden Player` → `Hidden name`).
    - `lobby2`: the APR tip is `Average party rank\nFrom 1 ranked player`.
    - The text matches exactly.
    - The tip rect is inside `#app`.
    - Its centre is `clamp(136, 864, triggerCentre)`.
    - The bottom-row trigger flips above.
    - The name tip appears at ≥ 450ms and not at 400ms. This test installs Playwright's fake clock before loading the page (`page.clock.install({ time: new Date('2026-09-29T21:59:00Z') })`) and moves it with `page.clock.runFor(400)`, then `runFor(50)`, so it never depends on real timers.
    - The Win% tip shows `31W · 22L · 53 games` / `This act` in `--win`/`--loss`.
    - Moving off a trigger clears the tip immediately.
  - `interaction.spec.ts` (Live part, spec §8.5 "Hover and click"):
    - hovering a card gives `box-shadow: inset 0 0 0 1px` in `--line-hover`;
    - clicking a dashed slot (`pregame`), a skeleton card (`ingame-loading`) or the `My Stats` tab leaves the route unchanged, and each has `cursor: default`;
    - a click while a tip is open hides it.
  - `visual-live.spec.ts` (`test.describe.configure({ mode: 'serial' })`, so no two workers write the same reference PNG):
    - `design-ref: ingame [default]`
    - `design-ref: ingame [<option>]` for Underline, Solid, Accent / Red, Accent / Gray, Cool, Warm, the 3 non-default accents, IBM Plex Sans and Archivo
    - `design-ref: ingame-loading` (design `loading: true`)
- [ ] **Step 2:** Run `cd /home/project-1/.worktrees/ui && npm --prefix ui run test` and `cd /home/project-1/.worktrees/ui/ui && npx playwright test fit-live fit-dpr tips-live interaction visual-live`. Expected: FAIL (Live is a stub).
- [ ] **Step 3:** Implement the Files → Create/replace per the cited spec sections.
  - Svelte 5 runes; `onclick` callback props.
  - No new dependency.
  - Every size and colour comes from the spec + CSS variables.
  - No text inside skeletons or dashed slots.
  - Every allowed-ellipsis span carries `data-ellipsis` (spec §8.3 check 5). The ⟲ slot is `min-width:36px` (spec §3.3).
- [ ] **Step 4:** Run `cd /home/project-1/.worktrees/ui && npm --prefix ui run test && npm --prefix ui run check && cd ui && npx playwright test fit-live fit-dpr tips-live interaction visual-live`. Expected: PASS.
  - Open every `ui/tests/shots/*.png`, and every diff image the visual spec wrote under `ui/test-results/`, yourself. Fix anything off before committing.
- [ ] **Step 5: Commit + push** `feat(ui): live page (lobby, agent select, in game) with fit and design tests` →
  ```bash
  cd /home/project-1/.worktrees/ui && test "$(git branch --show-current)" = ui-lane && P="ui/src ui/tests ui/playwright.config.ts .gitignore" && git add $P && git commit -m "feat(ui): live page (lobby, agent select, in game) with fit and design tests" -- $P
  ```
  - Then the UI-lane porcelain check and `cd /home/project-1/.worktrees/ui && git push origin ui-lane`.

### Task 18: Player Details + Match Details UI

**UI lane,** after Task 17. Directory: `/home/project-1/.worktrees/ui`, branch `ui-lane`.

**Files (ui-lane):**
- Create/replace:
  - `ui/src/screens/Details.svelte`: spec §6.1–§6.9.
  - `ui/src/screens/Match.svelte`: spec §6.10.
  - `ui/src/components/details/`:
    - `BackButton.svelte`, `Hero.svelte`
    - `RecentMatches.svelte`, `ShowMore.svelte`
    - `Encounters.svelte`, `TopTeammates.svelte`, `Loadout.svelte`
    - `DetailsSkeleton.svelte`
  - `ui/src/components/match/{MatchHeader,Scoreboard,MatchSkeleton}.svelte`
  - `ui/src/components/ResultChip.svelte` (20/28px chips via `boxStyle`)
  - `ui/src/lib/details.ts`
  - Tests: `ui/src/lib/details.test.ts`, `ui/tests/fit-details.spec.ts`, `ui/tests/tips-details.spec.ts`, `ui/tests/visual-details.spec.ts`
- Modify (only if the hero needs it): `ui/src/components/{Name,Last5,EncounterBadge}.svelte` for the `hero`/`row` sizes.
- Modify: `ui/tests/interaction.spec.ts` (the Details part).

**Interfaces:**
- Consumes:
  - `getPlayerDetails`, `getMatchDetails`, `applyDetailsEvent`, the router, the `meta.ts` lookups (Tasks 16a/16b);
  - `Name`, `Last5`, `EncounterBadge`, `boxStyle`, `harness.ts`, `checks.ts`, `design-ref.ts` (Task 17);
  - spec §4.2 (details/match rows), §5.5–§5.6, §6, §8.2–§8.6 (details/match parts); R97 (loadout tri-state).
- Produces:
  ```ts
  // details.ts
  export function recentShown(recent: MatchSummary[], more: boolean): { rows: MatchSummary[]; showMore: boolean }; // 3, then ≤ 8; no button once more
  export function shortName(n: DisplayName, meta: Meta | null): string | undefined;
    // visible/remembered → name; hiddenPlayer → 'Hidden Player'; hiddenAgent → agentName(meta, agentId) from meta.ts (R95):
    // undefined while meta is pending (render the 60×10 bar, spec §5.8), '—' when the agent is missing from loaded meta
  export function weaponState(w: WeaponSkin | null | undefined): 'loading' | 'none' | WeaponSkin; // R97: absent → 'loading', null → 'none', value → the skin
  export function liveTeammate(puuid: string, live: LiveState | null): { agentId: string | null; tier: number | null } | null; // only visible live cards
  export function queueLabel(queueId: string): string;                     // spec §6.10 table
  export function whenText(v: MatchDetailsView, from: MatchFrom, nowMs: number): string; // '{queueLabel} · {date} · {time12} · {durationMin}'; date = dateShort from an encounter row, dateLong from a recent row (R140)
  export function halvesText(v: MatchDetailsView): string | null;          // null when firstHalf null or no-team
  export function teamLabel(v: MatchDetailsView, i: number, focusShort: string): string; // 'Your team' | "{name}'s team" | 'Opponents' | 'Players'
  export function pmText(k: number, d: number): { text: string; tone: 'win' | 'loss' | 'flat' }; // '+N' / '−N' / '0'
  export const CONTENT_TIERS: Record<string, { name: string; color: string }>; // spec §6.8 table
  ```
  - `Details.svelte` props: `{ puuid: string; more: boolean }`.
  - `Match.svelte` props: `{ matchId: string; focusPuuid: string; from: 'recent' | 'encounter' }`.
  - A recent row click → `go({ name: 'match', …, from: 'recent' })`. An encounter row click → `from: 'encounter'`. Back per spec §2.4.

- [ ] **Step 0:** Load `svelte-code-writer` (mandatory for every `.svelte` edit), `svelte-core-bestpractices`, `test-driven-development` and `webapp-testing`. Code facts from graft. Every command runs as `cd /home/project-1/.worktrees/ui && …` (Playwright: `cd /home/project-1/.worktrees/ui/ui && npx playwright test …`).
- [ ] **Step 1:** Write the failing tests.
  - `details.test.ts`:
    - `recentShown 3 then 8 and hides the button`
    - `shortName per variant` (meta `null` → `undefined` for `hiddenAgent`, miss → `'—'`, spec §5.8; visible, remembered and hiddenPlayer never wait for meta)
    - `weaponState: absent → loading, null → none, value → the skin` (R97)
    - `liveTeammate ignores hidden live cards`
    - `queueLabel table incl. '' → Custom and unknown → title case`
    - `whenText drops the duration when unknown`
    - `halvesText a1–b1 · a2–b2 incl. overtime in the second`
    - `teamLabel self / focus / dm`
    - `pmText signs with U+2212`
  - `fit-details.spec.ts`:
    - `fit: <scene> [<font>]` for `details, details-more, details-self, details-hidden, details-empty, details-loading, details-longnames, details-partial, match, match-enc, match-hidden, match-dm, match-loading, match-longnames` × 3 fonts. Each runs spec §8.3 checks 1, 3, 4, 5, 7, 8, 9 (check 9 as a regression check only, R96) and saves `ui/tests/shots/<scene>__<font>.png` (the full scroll height, via a scrolled shot of the container). Check 5 allows truncation only on `data-ellipsis` spans (Riot-ID names, the encounters header name, the loadout skin line, the recent-row map name) and asserts `text-overflow: ellipsis` on the truncated ones.
    - The spec §8.5 `details`, `details-self`, `details-longnames`, `details-partial` (including its resolving loadout weapon: a skeleton well and bar with no text, R97), `match`, `match-enc`, `match-dm` and `match-longnames` assertions.
    - `details event updates in place keeping more and scroll`
    - `Back paths: match → details (more kept) → live`
    - `scroll container resets on navigation`
  - `tips-details.spec.ts`: every details/match row of spec §4.2. The text is exact, and the tip hides on scroll. The rows:
    - the hero: name (`remembered` and `hiddenAgent` on `details-hidden`, overflow-only on `details-longnames`), ⟲ `1 with · 2 against`, the Peak group, K/D and HS% `Last 5 competitive`, Win% `41W · 24L · 65 games` / `This act` (Mara: the design's `win: [63, 41, 24]`), Games and W / L `This act`, the last-5 tiles;
    - recent time `9:14 PM`, rank icon;
    - the encounter map cell `Sep 12 · 11:18 PM · 41 min`;
    - the Top-3 live icons `In this match · Breach`;
    - the loadout well `Premium Edition`;
    - halves, `ACS`, `+/−`, `ADR`, the scoreboard rank.
  - `interaction.spec.ts` (Details part): Back, recent rows, encounter rows and Show more change background on hover (spec §6.1, §6.4, §6.5); Top 3 rows and scoreboard rows are not clickable.
  - `visual-details.spec.ts` (`test.describe.configure({ mode: 'serial' })`):
    - `design-ref: details (top)` and `details (bottom)`: the design after clicking Mara, ours at `?scene=details`. The bottom shot sets the scroll container's `scrollTop = scrollHeight` in both.
    - `design-ref: details-more (top/bottom)`: adds `[data-tip$=" AM"],[data-tip$=" PM"]{visibility:hidden!important}`, because the design mock's rows 4–8 have `ago` labels that don't match their own dates (v8:451 vs v8:472).
    - `design-ref: match (top/bottom)`: from recent row 0.
    - `match-enc` is pixel-compared too (top only, it fits the window). The design writes that header as `Sep 12 · 11:18 PM · 41 min` with no year (v8:642, v8:685), and the Match header opened from an encounter row uses `dateShort` (R140), which prints no year in the current year. (An earlier draft of this step said spec §6.10 uses `dateLong` and left `match-enc` out of the comparison; R140 corrected that.) A match opened from a recent row keeps `dateLong`.
    - Threshold and hide-style rule as in Task 17.
- [ ] **Step 2:** Run `cd /home/project-1/.worktrees/ui && npm --prefix ui run test` and `cd /home/project-1/.worktrees/ui/ui && npx playwright test fit-details tips-details interaction visual-details`. Expected: FAIL (stubs).
- [ ] **Step 3:** Implement per the spec sections.
  - `Details` loads with `getPlayerDetails` and shows `DetailsSkeleton` until it resolves.
  - A `details` event updates it in place (`applyDetailsEvent`).
  - `Match` loads with `getMatchDetails`, showing `MatchSkeleton` meanwhile.
  - Loadout (R97, via `weaponState`): an absent weapon (name resolving) → the spec §6.9 loading row: a 124×48 `--skel` well (radius 5) and a 90×13 bar, no text; a `null` weapon → an empty raised well, the weapon name and `—`; a skin → spec §6.8. `tierId` null → no tint and no icon.
  - Top 3 name spans get `overflow-wrap:anywhere` (spec §6.7); the count uses `count()` (`1 game`).
  - `data-ellipsis` on the encounters header name, the loadout skin line, the recent-row map name and the Riot-ID name spans (spec §8.3 check 5).
- [ ] **Step 4:** Run `cd /home/project-1/.worktrees/ui && npm --prefix ui run test && npm --prefix ui run check && cd ui && npx playwright test`. The full suite, Live included, must pass. Expected: PASS.
  - Look at every new shot and diff image yourself.
- [ ] **Step 5: Commit + push** `feat(ui): player details and match details with fit and design tests` →
  ```bash
  cd /home/project-1/.worktrees/ui && test "$(git branch --show-current)" = ui-lane && git add ui/src ui/tests && git commit -m "feat(ui): player details and match details with fit and design tests" -- ui/src ui/tests
  ```
  - Then the UI-lane porcelain check and `cd /home/project-1/.worktrees/ui && git push origin ui-lane`.

### Task 19: Release hardening, merge, v0.2.0

**Main,** in `/home/project-1`. Starts after Task 18 (on `ui-lane`) and Task 15b (landed on `main`). Every command starts with `cd /home/project-1 &&`.

**Files:**
- Merge: `ui-lane` into `main` (in `/home/project-1`).
- Modify:
  - `Cargo.toml` (root, R2): `[profile.release]`.
  - `.github/workflows/build.yml`: the `npm --prefix ui run check` step, the `ui/dist` guard (R15 marker, fixture-only names, font CDN and unpkg), the installer size check.
  - Versions → `0.2.0`: `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`, `package.json`, `ui/package.json`, `package-lock.json` (root and `ui` workspace entries), `Cargo.lock`.
  - `CHANGELOG.md`: `## [0.2.0] - <release date>` from `Unreleased`, plus Tasks 13–18 and 15b, filled from `git log` (R86: the only CHANGELOG writer after Task 13).
  - `README.md`: the screenshots now exist.
  - `docs/ARCHITECTURE.md` (moved here from Task 15, because lane A owned `docs/` while lane B ran): the DB schema section (migration 1 frozen + migration 2, R89) and the commands section (`match_details`); the user's own loadout, fetched once per connect (R90); the own-history backfill (R101).
- Create:
  - `docs/screenshots/{live-ingame,live-agent-select,live-lobby,player-details,match-details}.png`, copied from `ui/tests/shots/{ingame,pregame,lobby3,details,match}__Barlow.png`.
  - `docs/test-checklist.md`: in Arabic, for the user.

**Interfaces:**
- Consumes: Tasks 13–18 and 15b landed, all green (Task 12 was complete before Phase 2, R87).
- Produces:
  - Tag `v0.2.0` on the green commit.
  - The CI artifact `OPD1-TRCKER-setup` ≤ 10 MB.
  - Schema changes after this tag are migration 3 and up (R89: migrations are versioned and frozen once committed).
  - Cargo profile:
    ```toml
    [profile.release]
    opt-level = "s"
    lto = true
    codegen-units = 1
    strip = true
    panic = "abort"
    ```
  - CI steps, all `shell: bash`, after `npx tauri build`:
    ```bash
    if grep -rqE "demo-feed|VesperUnbreakabl|Vantablack|fonts.googleapis|fonts.gstatic|unpkg.com" ui/dist; then echo "demo code or a CDN reference in ui/dist"; exit 1; fi
    f=$(ls target/release/bundle/nsis/*.exe); s=$(stat -c %s "$f"); echo "installer $s bytes"; [ "$s" -le 10485760 ]
    ```

- [ ] **Step 0:** Load `tauri-v2` (release profile, NSIS bundle) and `verification-before-completion`. Code facts from graft.
- [ ] **Step 1:** Run the failing checks. Each must fail now:
  - `grep -q '^\[profile.release\]' Cargo.toml`
  - `grep -q "demo-feed" .github/workflows/build.yml && grep -q "VesperUnbreakabl" .github/workflows/build.yml && grep -q "fonts.gstatic" .github/workflows/build.yml && grep -q "unpkg.com" .github/workflows/build.yml`
  - `grep -q "match_details" docs/ARCHITECTURE.md && grep -q "migration 2" docs/ARCHITECTURE.md`
  - `grep -q "10485760" .github/workflows/build.yml`
  - `grep -q '"version": "0.2.0"' src-tauri/tauri.conf.json`
  - `grep -q '"version": "0.2.0"' package-lock.json`
  - `test -s docs/test-checklist.md`
  - `git tag -l v0.2.0 | grep -q v0.2.0`

  Expected: FAIL.
- [ ] **Step 2:** Merge:
  ```bash
  cd /home/project-1 && git checkout main && git pull --ff-only origin main && git merge-base --is-ancestor p2-backend main && git merge --no-ff ui-lane -m "feat: merge ui-lane (design v8 live, details, match details)"
  ```
  - `git merge-base --is-ancestor p2-backend main` confirms Task 15b has landed; if it fails, stop and wait for it.
  - Then run `cd /home/project-1 && npm ci && npm --prefix ui run build && cargo test -p opd1-core -p opd1-riot -p opd1-trcker && npm --prefix ui run test && npm --prefix ui run check && (cd ui && npx playwright test)`. Expected: PASS.
  - `cd /home/project-1 && git push origin main`.
- [ ] **Step 3:** Implement:
  - the profile;
  - the CI steps, with `npm --prefix ui run check` added after `Test UI`;
  - the version bumps: `npm version 0.2.0 --no-git-tag-version --workspaces --include-workspace-root` (updates `package.json`, `ui/package.json` and `package-lock.json` together, so `npm ci` stays valid), then `src-tauri/tauri.conf.json` and `src-tauri/Cargo.toml` by hand, then `cargo check -p opd1-trcker` to refresh `Cargo.lock`;
  - the CHANGELOG release section;
  - the screenshots + README paths;
  - the `docs/ARCHITECTURE.md` refresh (Files);
  - `docs/test-checklist.md`, with short Gulf Arabic items, one line each:
    1. Download Setup.exe from GitHub and run it. If Windows warns: More info → Run anyway.
    2. The window is 1000×800, can't be enlarged, stays on top, and close quits.
    3. With the game closed it opens on Live with only your card. (On the very first start, before the game ever ran, the page is empty; that's expected.)
    4. Lobby solo, then a 2–5 party: the cards stack on the left and Average Party Rank shows top right. Your own card shows your level even if you hid it in the game; other players who hid theirs show `Lv —` (R100).
    5. Searching for a match changes nothing.
    6. Match found:
       - it jumps to Live by itself;
       - your 5 on the left and 5 dashed empty slots on the right;
       - the map at the top;
       - a player's agent image (and, for a hidden player, the agent name) appears only after they lock; a hover shows nothing (R99).
    7. In game:
       - 5 against 5 with no scrolling;
       - a hidden player shows the agent name only;
       - a hidden player you've played with shows their name with a dotted underline.
    8. Hover the last 5, Win%, the party mark and ⟲: the tooltips appear.
    9. Click any card:
       - Back;
       - Show more up to 8;
       - Top 3 Teammates;
       - Loadout: your own skins show everywhere, the Lobby included (R90); other players' skins show in game, and in Agent Select for your team; elsewhere they show `—`, by design (R83);
       - Encounters.
    10. Click a match: the Match Details scoreboard, and Back returns to the player.
    11. After the match it returns to the Lobby.
    12. Once in Agent Select, and once after a match where at least one card showed Hidden Player or an agent name instead of a name: Ctrl+Shift+D, then Ctrl+Shift+L. Send the folder `%APPDATA%\com.opd1.trcker\debug`, the copied text, and the file `%APPDATA%\com.opd1.trcker\data\opd1.sqlite` (close the app first) (R94).
    13. Play 2 matches with the same friend, then click their card: the Encounters list has the new match. (Friends in your party get no ⟲ on the card, by design.)
    14. Party stripes: if you know two players in the match queued together (for example friends of yours in another party), they share one colour stripe, and no stripe links players who clearly aren't together. Tell us any wrong stripe.
    15. Run the same Setup.exe again over the installed app, then open it: the ⟲ counts and the Details history are still there.
    16. With the game closed, open Task Manager and send the memory (RAM) number shown for OPD1 TRCKER.
    17. Close the app, play 1–2 matches, then open the app: within a few minutes those matches show in your Recent matches, and a friend you played with has them in Encounters (R101).
    18. A screenshot of anything that looks wrong.
  - Note in the file which dumps the controller will check, before friends get the build:
    - the pregame `CharacterSelectionState` values: a lock reads `locked` (R99);
    - the user's own `AccountLevel` in the pregame/core-game dump when `HideAccountLevel` is true: non-zero, so R100 can show it;
    - the pregame loadouts shape (`Loadouts[].Subject`, spec §7.3.6);
    - `player_loadout.json`: `Guns[].ID/SkinID/ChromaID` as in techchrism `PlayerLoadout.ts` (R90);
    - whether match-details returns names for hidden players (R74, R94). The dumped `match_details.json` is only the last match-details fetched (one body per endpoint, raw_capture.rs:248-256; a body over 2MB becomes a marker), so use the DB from item 12 instead: for each puuid whose `PlayerIdentity.Incognito` was true in `coregame_match.json`, read `SELECT game_name, tag_line FROM match_players WHERE match_id = '<that match>' AND puuid = '<puuid>'`. If a real name is there, do the R74 follow-up before friends get the build: hide every name in matches the user didn't play, and every Top 3 name learned only from such matches, except players visible on the live screen now;
    - `NumberOfWinsWithPlacements`;
    - the RAM number (item 16) against the lightness goal.
- [ ] **Step 4:** Run `cd /home/project-1 && cargo build --release -p opd1-trcker` locally (the profile compiles) and Step 1's checks, except the tag. Expected: PASS.
- [ ] **Step 5: Commit + push** `build: release profile, CI demo and size guards, v0.2.0` →
  ```bash
  cd /home/project-1 && P="Cargo.toml Cargo.lock .github/workflows/build.yml src-tauri/tauri.conf.json src-tauri/Cargo.toml package.json ui/package.json package-lock.json CHANGELOG.md README.md docs/ARCHITECTURE.md docs/screenshots docs/test-checklist.md" && git add $P && git commit -m "build: release profile, CI demo and size guards, v0.2.0" -- $P
  ```
  - Then the main porcelain check and `cd /home/project-1 && git push origin main`.
  - Watch CI (Global "CI watch"). Expected: success.
  - `gh run view "$id" --log` shows `installer N bytes` with N ≤ 10 485 760.
  - The artifact `OPD1-TRCKER-setup` is listed via `gh api "repos/opd116/opd1-trcker/actions/runs/$id/artifacts"`.
- [ ] **Step 6: Tag + push** `cd /home/project-1 && git tag -a v0.2.0 -m "OPD1 TRCKER v0.2.0" && git push origin v0.2.0`, only after Step 5's run is green.
  - The controller then sends the user the artifact link and `docs/test-checklist.md` (this is Task 11, Step 1).
- [ ] **Step 7: Phase end.** Write the Phase 2 close-out to the ledger (disk only). Then ask the user, in Arabic, to run `/handoff` (it is user-invoked only; it replaces the superseded `docs/handoff.md`): «خلصنا المرحلة. اكتب ‎/handoff عشان نحفظ كل شي للمحادثة الجاية.»

## v0.3.0 round (2026-10-03)

Fixes plus the My Stats page, at the user's request (R168). What was built is in `docs/DECISIONS.md` R168–R176 and `CHANGELOG.md` `[0.3.0]`; where it stands and what is next is in `docs/handoff.md`. The Task 14+ checkboxes above are historical: that work landed in v0.2.0 and v0.3.0.
