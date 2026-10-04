# Decisions

Every ruling made while building OPD1 TRCKER, in number order (R1 to R176). Each one has what was decided, why, what it costs if it turns out wrong, and its current status.

- **Status** is one of `active`, `superseded by Rn`, `narrowed by Rn`, `reversed by the user (date)`, `dropped`, or `unused number`. A note in parentheses adds detail.
- Rulings come from the working ledger (R1 to R73 and R87 to R164) and from Phase 2 planning (R74 to R86, the gap-fills of `docs/design-spec.md`, which carry the tags [C] and [bound] there).
- Precedence when rules disagree (R92): `docs/requirements-raw.md` (newest dated block) > the design file > older `CLAUDE.md` visual rules. `docs/design-spec.md` outranks the design file only for the R74 to R86 gap-fills.
- The user's own words are in `docs/requirements-raw.md`; the [user decisions index](#user-decisions-index) at the end maps each dated block to a summary.

### R1 — Package names opd1-core and opd1-riot

- **What:** The packages are `opd1-core` (lib `opd1_core`) and `opd1-riot` (lib `opd1_riot`), in `crates/core` and `crates/riot`. Every plan command `-p core` / `-p riot` means `-p opd1-core` / `-p opd1-riot`.
- **Why:** A crate named `core` breaks derive macros that expand to `::core::`.
- **Cost:** A rename if wrong.
- **Status:** active

### R2 — Release profile lives in the root Cargo.toml

- **What:** `[profile.release]` goes in the root `Cargo.toml` (Task 10).
- **Why:** Cargo ignores profiles in member manifests.
- **Cost:** None.
- **Status:** active

### R3 — vitest passes with no tests

- **What:** The vitest config sets `passWithNoTests: true` until the UI has tests.
- **Why:** Avoids a red CI with zero tests.
- **Cost:** None.
- **Status:** active

### R4 — Installer deletes old files only in a real install dir

- **What:** `hooks.nsh` deletes the old `$INSTDIR` contents only when `$INSTDIR\<main exe>` exists (a previous install). It never recursive-deletes an arbitrary folder.
- **Why:** Protects a user who points the installer at a shared folder.
- **Cost:** If wrong, a stale file could survive in an odd install dir.
- **Status:** active

### R5 — Every rate crosses the wire as a fraction

- **What:** `hsPct`, `actWinrate` and the My Stats winrates are fractions 0..1. `kd` is a plain ratio. The UI's `pct()` formats fractions.
- **Why:** Matches `pct(0.545) == "54.5%"`.
- **Cost:** A 100x display error if wrong (caught by tests).
- **Status:** active

### R6 — Peak normalises pre-Episode-5 tiers

- **What:** `peak` normalises tiers from acts that started before Episode 5 Act 1 (2022-06-22): tier >= 21 becomes tier + 3 (as VALORANT-rank-yoinker and Scout do), then takes the max. The act is decided from valorant-api `/seasons` start times.
- **Why:** Correct icon for old Immortal and Radiant peaks.
- **Cost:** If wrong, old peaks show one tier band off.
- **Status:** active

### R7 — Peak in several acts reports the most recent

- **What:** When the peak tier was reached in several acts, the most recent such act is reported.
- **Why:** "Time since act end" is most meaningful for recent skill.
- **Cost:** Trivially reversible.
- **Status:** active

### R8 — Test commands run per test file

- **What:** Plan test commands map to `cargo test -p opd1-core --test <file>` per test file (for example `--test db --test stats`).
- **Why:** Cargo takes one name filter.
- **Cost:** None.
- **Status:** active

### R9 — Wire types live in model.rs

- **What:** Task 5 defines the serialized `DisplayName` enum, `PartyMark` and `EncounterCount` in `model.rs`, filled with simple values or `None`. Task 6 adds `names.rs::display_name()` and `party.rs::detect_parties()` returning those types.
- **Why:** Keeps the UI contract in one file.
- **Cost:** None.
- **Status:** active

### R10 — JSON contract shape

- **What:** serde `rename_all = "camelCase"`. `Stage` is `"closed" | "menus" | "pregame" | "ingame"`. `DisplayName` is internally tagged: `{"kind":"visible"|"remembered","name","tag"}`, `{"kind":"hiddenPlayer"}`, `{"kind":"hiddenAgent","agentId"}`. `team` is `"ally" | "enemy"`.
- **Why:** One shape for Rust and TypeScript.
- **Cost:** A rename only.
- **Status:** active

### R11 — Images through an img:// scheme, names through meta()

- **What:** Images are served through a Tauri custom URI scheme `img` (`img://localhost/<kind>/<uuid>`, on Windows `http://img.localhost/<kind>/<uuid>`) that returns the disk-cached file or downloads it once. Names, colours and act metadata come from one `meta()` command, fetched once per app start. The backend translates Riot map paths to map uuids before emitting. In a DEV browser the UI builds valorant-api CDN URLs instead.
- **Why:** Lazy per-image caching, and no absolute paths in the UI.
- **Cost:** If wrong, the URL helper changes in one place.
- **Status:** active

### R12 — Demo feed scenes by URL and key

- **What:** The DEV demo feed is selected by URL `?scene=<name>` (for Playwright) and by keys: 1-5 lobby(n), 6 pregame, 7 ingame, 8 ingame-longnames, 0 closed. Queue is not a separate scene, because queueing changes nothing.
- **Why:** Deterministic tests.
- **Cost:** None.
- **Status:** active

### R13 — Svelte 5 callback props

- **What:** Components use callback props (`onopen`) instead of `on:open`.
- **Why:** `on:` component events are deprecated in Svelte 5.
- **Cost:** None.
- **Status:** active

### R14 — Playwright runs against vite dev

- **What:** The Playwright `webServer` runs the Vite dev server (`vite dev --port 4173 --strictPort`), not `vite preview`.
- **Why:** The DEV flag must be true for the demo feed.
- **Cost:** None.
- **Status:** active

### R15 — demo-feed marker

- **What:** The demo module sets `document.documentElement.dataset.feed = "demo-feed"` on start (invisible, DEV only).
- **Why:** A leaked demo bundle keeps the marker after minification, and the CI guard greps for it.
- **Cost:** If wrong, the CI check is vacuous.
- **Status:** active

### R16 — names() takes only visible puuids

- **What:** `names()` takes `&[VisiblePuuid]`, a newtype constructible only from `(puuid, incognito = false)`.
- **Why:** Makes "never request a hidden player's name" a type-level guarantee instead of a `debug_assert`.
- **Cost:** A small signature change if wrong.
- **Status:** active

### R17 — Client version source

- **What:** The client version comes from the local `/product-session/v1/external-sessions`, with valorant-api `/v1/version` as the fallback. Scout's presence `partyClientVersion` is an acceptable middle fallback.
- **Why:** The plan text wins, and Scout shows presence also works.
- **Cost:** None.
- **Status:** superseded by R159 (the source order is now Scout's: presence, valorant-api, `ShooterGame.log`, `release-09.00`; external-sessions is no longer asked)

### R18 — Live hidden-name protection covers Top 3 Teammates

- **What:** A teammate whose puuid is incognito in the current live roster is shown by the display_name rules (remembered from a match the user played, or hidden) in Player Details, never with a name from another player's history.
- **Why:** Closes an indirect leak.
- **Cost:** A hidden duo partner shows as hidden in someone else's details.
- **Status:** narrowed by R145 (the remembered name now comes from any source the DB knows, not only a match the user played; the never-from-another-player's-history part stands for live incognito puuids)

### R19 — new_match_ids drops the puuid parameter

- **What:** `new_match_ids` no longer takes a puuid.
- **Why:** Match details are global per match, so the plan's parameter was dead.
- **Cost:** None.
- **Status:** active

### R20 — Core owns the stats input types

- **What:** `opd1-core` owns plain stats input types (`SeasonInfo`, `Act`). The riot response structs are converted in Task 5.
- **Why:** Core must not depend on riot.
- **Cost:** None.
- **Status:** active

### R21 — Image kinds for img:// (R11 addendum)

- **What:** The kinds are agent (displayIcon), cardSmall, cardWide, tier (id = tier number, latest competitivetiers set), mapList, mapSplash and chroma (fullRender). `meta()` returns `{agents: {uuid: {name}}, maps: {uuid: {name}}, tiers: {n: {name, color}}}`. DEV uses a static `meta.json` snapshot from valorant-api.
- **Why:** Fixes the scheme's contract for both lanes.
- **Cost:** One helper in each lane.
- **Status:** active

### R22 — Two parallel lanes in the first phase

- **What:** One controller ran two lanes: the backend lane on `main` (Tasks 3 to 9a) and the UI lane on branch `ui-lane` in `/home/project-1/.worktrees/ui`. They touch disjoint directories (`crates/`, `src-tauri/` versus `ui/`), and the controller merges `ui-lane` into `main`.
- **Why:** There was no start_session tool, and parallel lanes are faster.
- **Cost:** Merge friction on `types.ts` if the backend JSON drifts from the plan (reconciled at merge). R98 sets the Phase 2 lanes.
- **Status:** superseded by R98 (Phase 2 lanes)

### R23 — Tri-state card fields on the wire

- **What:** Loadable `PlayerCard` fields are tri-state: loading is the key ABSENT (UI skeleton), missing is `null` (UI `—`), otherwise the value. Implemented with `skip_serializing_if`.
- **Why:** The plan requires `undefined` to differ from `null` in the UI.
- **Cost:** Serde attributes in `model.rs`.
- **Status:** active

### R24 — Demo detection uses isTauri()

- **What:** Demo detection uses `isTauri()` from `@tauri-apps/api/core`.
- **Why:** Tauri 2 has no `window.__TAURI__` without `withGlobalTauri`.
- **Cost:** None.
- **Status:** active

### R25 — comp_updates always sends queue=competitive

- **What:** `comp_updates` keeps the plan signature and always sends `queue=competitive`.
- **Why:** The function exists only for competitive RR, and without the parameter rows mix queues.
- **Cost:** None.
- **Status:** active

### R26 — Retry-After is clamped, rps is validated

- **What:** The `Retry-After` hold is clamped to 0-30s and `rps` is validated (a reviewer Minor upgraded to Important).
- **Why:** Release builds use `panic = "abort"`, so any panic kills the app.
- **Cost:** None.
- **Status:** partly superseded by R160 (the clamp is now 0-120 s; the `rps` validation stands)

### R27 — ring TLS provider, no http3

- **What:** reqwest uses the `ring` rustls provider and no http3 (a reviewer Minor upgraded to Important).
- **Why:** Lightness is the number one tie-breaker, and `aws-lc-sys` is a Windows CI build risk.
- **Cost:** None, if reqwest exposes it.
- **Status:** active

### R28 — Peak carries act start and end dates

- **What:** The contract adds `peak.actStartMs` and `peak.actEndMs` (epoch ms). The UI shows a `V26 ACT V · 19 Aug – 14 Oct 2026` style tip.
- **Why:** CLAUDE.md requires the hover to show the full act name and dates.
- **Cost:** Two fields in both lanes.
- **Status:** active

### R29 — Schema additions in migration 1

- **What:** Beyond the plan's list: `matches.rounds_played`; table `match_teams(match_id, team_id, won, rounds_won, PK(match_id, team_id))`; `comp_updates.tier_before`; `matches.winning_team` NULL means draw or no team.
- **Why:** Needed downstream: My Stats "result + score" and "recent rank changes".
- **Cost:** One migration's worth of columns, and it avoids a version 2 migration later.
- **Status:** active

### R30 — copy_log returns the last 500 lines

- **What:** `copy_log()` returns the last 500 log lines. The UI writes them with `navigator.clipboard` on the Ctrl+Shift+L keydown (a user gesture). The log is an in-memory ring buffer in Rust.
- **Why:** No clipboard plugin (lightness).
- **Cost:** None.
- **Status:** active

### R31 — Migration 1 may be amended before v0.1.0

- **What:** Until tag v0.1.0 no user DB exists, so migration 1 (`schema.sql`) may still be amended. After v0.1.0 every schema change is a new migration. Task 5 adds `app_state(key TEXT PRIMARY KEY, value TEXT)` (`self_puuid` for the Closed stage).
- **Why:** No installer has been distributed yet.
- **Cost:** None.
- **Status:** superseded by R89 (migration 1 is frozen)

### R32 — last5 entries carry a result

- **What:** `last5` entries carry `result: "win" | "loss" | "draw"` instead of `win: bool`.
- **Why:** Draws exist, and version 1 rendered them as losses.
- **Cost:** One field in both lanes.
- **Status:** active

### R33 — Task 5 split and per-player fetch window

- **What:** Task 5 was split into 5a (core `model.rs`, `build_card`, riot to core conversion including match-details extraction, riot `assets.rs`, raw-response capture) and 5b (`src-tauri`: DB open, poll loop, commands, `img` scheme, log ring buffer). The per-player fetch is `comp_updates(0..5)` plus `match_history(any queue, 0..2)` plus details only for unknown ids plus `mmr` only if needed. UI keybindings and the initial `live_state` invoke belong to the UI lane.
- **Why:** Fewer calls for the same data.
- **Cost:** An extra review cycle.
- **Status:** narrowed by R101 (the user's own history is backfilled beyond the per-player window)

### R34 — PlayerCard.name is tri-state too

- **What:** `PlayerCard.name` is absent while a visible name is still loading (UI skeleton), so a blank `Visible{"",""}` never appears (a reviewer Minor upgraded).
- **Why:** The UI contract requires loading to differ from missing.
- **Cost:** `Field<DisplayName>` in the model and the UI types.
- **Status:** active

### R35 — Future acts are clipped

- **What:** valorant-api lists future acts (observed: V26 ACT VI through 2027-01-06 while ACT V is current), so acts are clipped to started acts (start <= now) before the previous-act and current-act logic. `last5` is capped at 5 after sorting.
- **Why:** The previous-act rank would otherwise show the current act.
- **Cost:** None.
- **Status:** active

### R36 — Raw capture keeps only valid 2xx bodies

- **What:** Raw capture records only 2xx bodies and never byte-truncates into invalid JSON (over the cap it keeps the previous one or stores a marker).
- **Why:** `dump_raw`'s whole purpose is valid fixtures.
- **Cost:** None.
- **Status:** active

### R37 — Image work off the runtime, longer image timeout

- **What:** Image decode/resize and file work run in `spawn_blocking`. Image downloads get a longer total timeout (about 30s) than JSON calls.
- **Why:** A blocked runtime stalls the presence poll, and a 3 MB splash must be cacheable on slow links.
- **Cost:** None.
- **Status:** active

### R38 — Number never used

- **What:** No ruling was recorded under this number.
- **Why:** The ledger skipped it while numbering.
- **Cost:** None.
- **Status:** unused number

### R39 — Pregame repoll every 3 seconds

- **What:** During Pregame, `pregame_match` is polled about every 3s (glz, cheap) only to pick up agent selections. The party roster is re-fetched when the presence `partyId` or `partySize` changes.
- **Why:** Agent icons and lobby membership change without a stage change, and the plan's "remote calls only on stage change" would freeze them.
- **Cost:** About 25 glz calls per agent select.
- **Status:** active

### R40 — No Rust demo cargo feature

- **What:** There is no Rust `demo` cargo feature. The DEV-only UI demo feed covers screen work, and Task 10's "demo feature not enabled" check stays trivially true.
- **Why:** YAGNI.
- **Cost:** None.
- **Status:** active

### R41 — Deathmatch and FFA keep the Lobby view

- **What:** In Deathmatch / FFA the app keeps the Lobby (party) view and does not fetch or sync the 13 opponents. Party detection still uses DM matches from history.
- **Why:** No 5v5 layout exists for FFA, and about 140 cold calls violates lightness.
- **Cost:** No scouting in DM (low value).
- **Status:** active

### R42 — Local presence poll while VALORANT is closed

- **What:** When the Riot Client runs but VALORANT is not in presence, a local presence call every 5s is accepted as the equivalent of the idle lockfile check (no remote calls).
- **Why:** Needed to notice VALORANT starting.
- **Cost:** Negligible.
- **Status:** active

### R43 — Own name from the local chat session

- **What:** The user's own name comes from the local `/chat/v1/session` (`game_name` / `game_tag`), and self is never sent to `names()`.
- **Why:** Keeps "never call name-service for an incognito player" literally true with no self-exemption.
- **Cost:** One local call per connect.
- **Status:** active

### R44 — CSP accepted as implemented

- **What:** The CSP adds `data:` for img/font and `'unsafe-inline'` style with `dangerousDisableAssetCspModification` for `style-src`, for Svelte style attributes. Nothing remote.
- **Why:** Needed by the UI.
- **Cost:** None.
- **Status:** active

### R45 — worker_running flag

- **What:** A `worker_running` flag under the same lock fixes the worker-exit race that left a new roster player loading until the next roster event (a reviewer Minor upgraded).
- **Why:** A card must never be stuck on a skeleton.
- **Cost:** None.
- **Status:** active

### R46 — Rebuild the session when VALORANT relaunches

- **What:** The session is rebuilt when VALORANT relaunches under a still-running Riot Client (a reviewer Minor upgraded).
- **Why:** After a patch the cached client version and shards go stale and remote calls fail.
- **Cost:** None.
- **Status:** active

### R47 — Back off repeated connect failures

- **What:** Repeated connect failures back off (5s, then up to 30-60s) (a reviewer Minor upgraded).
- **Why:** Idle lightness: a failing connect re-reads `ShooterGame.log` every 5s forever.
- **Cost:** Slower reconnect after a Riot Client crash.
- **Status:** active

### R48 — Post-match follow-up checks comp_updates

- **What:** The post-match follow-up checks `comp_updates` (not matches) for competitive queues (a reviewer Minor upgraded).
- **Why:** Otherwise rank and RR stay stale in the Lobby after a competitive match.
- **Cost:** None.
- **Status:** active

### R49 — The user's own card keeps their own name

- **What:** The user's own card keeps showing their own name even with Riot's hide-name setting (sourced locally from `/chat/v1/session`, never name-service).
- **Why:** The user knows their own name, and hiding it from themselves helps nobody.
- **Cost:** One line to change in `card_for`.
- **Status:** active

### R50 — Design v4 first gate

- **What:** The design v4 first gate showed the design system plus PlayerCard (3 sizes) and Live screens (In Game, Agent Select, Lobby 1/3/5 including VALORANT-closed) with edge states. Details and My Stats came after approval.
- **Why:** The user was impatient and had rejected 3 rounds, so a smaller, faster gate on the screens that define the card beat a long all-screens round.
- **Cost:** Details and My Stats could force a small system tweak later.
- **Status:** dropped (the local design rounds were retired on 2026-09-29 when the user approved Claude Design v8)

### R51 — Design work lives in .superpowers/design/

- **What:** Design work lives in `.superpowers/design/`, kept on disk and out of git (persistent; the old /tmp scratchpad lost track once).
- **Why:** Persistence between sessions.
- **Cost:** None.
- **Status:** superseded by R70 (the original claim that it was ignored was wrong)

### R52 — Encounter count only above zero

- **What:** `encounters` on the card is `Some` only when with + against > 0 (`None` for strangers, self and self-party). There is no badge for zero.
- **Why:** The brief was silent for other players.
- **Cost:** The UI cannot tell "not loaded" from zero (both show nothing).
- **Status:** active

### R53 — Party marks over the shown players each publish

- **What:** Party marks are computed over the currently shown players on each publish. A player without synced history has no link yet and appears on a later publish. It never blocks a card.
- **Why:** Cards must not wait for history.
- **Cost:** A stripe may pop in a few seconds late.
- **Status:** active

### R54 — Task 9 split

- **What:** Task 9 was split: 9a is the Rust half (`mystats.rs`, the `my_stats` command, tests), which landed on main; 9b (the UI half: Details and My Stats screens) waits for the approved design.
- **Why:** The UI was blocked on the design gate, the Rust half was not.
- **Cost:** None.
- **Status:** narrowed by R98 (Phase 2 plan: 9b is now Task 18, Player Details; the My Stats UI waits until the user asks)

### R55 — Task 6 implementer calls accepted

- **What:** Accepted: opposite live teams are never linked; party marks come only from this cycle's fresh sync; no stored latest details means no link; Deathmatch is detected by queue id only.
- **Why:** All follow "same match on the same team" literally and avoid stale false stripes.
- **Cost:** A split custom-game party, or a player with a failed details fetch, shows no stripe.
- **Status:** active

### R56 — Design v5 samples

- **What:** Design v5 was 3 parallel samples (A scoreboard table / Geist, B v4-refined / Inter, C banner art / soft face), each covering ingame, lobby3 and lobby1 at 1000x800.
- **Why:** The user picks by eye, so distinct options beat one iteration.
- **Cost:** One round of designer time.
- **Status:** dropped (local design rounds retired 2026-09-29)

### R57 — Critic writes the rubric first

- **What:** The design critic writes its rubric before seeing any design and may only tighten it later.
- **Why:** Stops the bar from bending to the result.
- **Cost:** None.
- **Status:** dropped (local design rounds retired 2026-09-29)

### R58 — Design folder ownership

- **What:** Design folders had one owner each (system = architect, screens = designer, review = critic). Cross-role requests went through the controller.
- **Why:** Parallel agents must not overwrite each other.
- **Cost:** One relay hop per request.
- **Status:** dropped (local design rounds retired 2026-09-29)

### R59 — Merge the three v7 passes

- **What:** After the three refinement passes the architect and the designer merged the best changes into one design, the critic re-gated it against the same rubric, and the user then saw the result.
- **Why:** Three parallel edits of the same files cannot be combined automatically.
- **Cost:** One merge step.
- **Status:** dropped (local design rounds retired 2026-09-29)

### R60 — Names truncate with an ellipsis

- **What:** Long names truncate with an ellipsis, the full name shows in a tooltip, the tag is never truncated, and the badge slot is reserved. This replaced the rubric's "no ellipsis" gate for the name text only.
- **Why:** It is the user's own point, and a user decision outranks the critic's rubric.
- **Cost:** None.
- **Status:** active

### R61 — Information text at least 14px

- **What:** Information-bearing text (tags, times, act labels, rank names, Peak/Prev role words) is at least 14px with strong contrast.
- **Why:** The user called the 12px faint labels hard to read.
- **Cost:** Less room on the card.
- **Status:** superseded by R68

### R62 — Stat scope printed once per team

- **What:** Stat scope was printed once per team (In Game) and once in the Details hero, with a definition tooltip.
- **Why:** The user asked that each statistic's scope be visible.
- **Cost:** One heading row.
- **Status:** reversed by the user (2026-09-28), see R67

### R63 — Hidden name clarification

- **What:** The agent name replaces the Riot ID and everything else renders normally. `Hidden Player` shows only before an agent is picked.
- **Why:** The user's point covers the case where the agent is known.
- **Cost:** One string in Agent Select.
- **Status:** narrowed by R99 (an agent counts as picked only when locked)

### R64 — Design v8 process

- **What:** Design v8 kept the three-agent process (architect, designer, independent critic) and added a hidden-player Details state and one shot per indicator tooltip.
- **Why:** So every one of the user's points is checkable.
- **Cost:** A slightly longer round.
- **Status:** dropped (local design rounds retired 2026-09-29)

### R65 — encounters.last

- **What:** `encounters` would gain `last {startMs, result, score}` (the user's side) for the tooltip and the hero's `Your result`.
- **Why:** The user's point needed the last shared result.
- **Cost:** A small DB query and a field.
- **Status:** dropped (R69)

### R66 — Reconcile Q1-Q9 into system v8.1

- **What:** The design system's open questions Q1-Q9 were reconciled into system v8.1 before critic round 1.
- **Why:** Avoids a certain gate failure, as v6 showed.
- **Cost:** One short architect and designer pass.
- **Status:** dropped (local design rounds retired 2026-09-29)

### R67 — Stat scope lives only in tooltips

- **What:** R62 (printed scope headings) is reversed. The scope of each statistic shows only in its tooltip.
- **Why:** The user's words (2026-09-28).
- **Cost:** None.
- **Status:** active

### R68 — Type scale follows the design file

- **What:** R61 (information text at least 14px) is superseded by the design file's type scale (12-28px, minimum 12).
- **Why:** The user approved that scale in the design.
- **Cost:** Small labels at 12px, which the user approved.
- **Status:** active

### R69 — encounters.last dropped

- **What:** R65 (`encounters.last`) is dropped.
- **Why:** cd-v8 does not use it.
- **Cost:** One small query later.
- **Status:** active

### R70 — R51 wording fixed

- **What:** `.superpowers/` was not ignored. It is now excluded through `.git/info/exclude` (`/.superpowers/`, `/*.zip`).
- **Why:** The earlier ruling claimed it was already ignored.
- **Cost:** None.
- **Status:** active

### R71 — Claude calls of 2026-09-29

- **What:** Recorded in `docs/requirements-raw.md`: the start tab is Live until My Stats exists; Average Party Rank goes in the top bar in the Lobby; the map shows in Agent Select once known; hover counts as selected (to be verified with the first pregame `dump_raw`).
- **Why:** Neither the user nor the design settled them.
- **Cost:** Small UI tweaks.
- **Status:** partly reversed by the user (2026-09-29, R99): hover no longer counts as selected; the start-tab call ended with My Stats (R175, 2026-10-03); the APR and map calls stay active

### R72 — Current-act backfill deferred

- **What:** The current-act backfill (My Stats undercounts because sync keeps only the last 5/20 competitive updates) is deferred to the My Stats UI phase.
- **Why:** My Stats is not in the 2026-09-29 build scope.
- **Cost:** None now, because My Stats is not shown yet.
- **Status:** done by R173 (2026-10-03)

### R73 — Task 9a fix round

- **What:** Fix the 9a review findings M1 (placement games with 0 RR counted in the average win/loss and RR economy) and M2 (act membership: matches by valorant-api date versus RR rows by season id) on main before the Phase 2 lanes, plus cheap minors m2, m4, m7 and m8. Deferred: m1, m3, m5, m6. Done in `c9ad7f4`.
- **Why:** Main was idle while planning ran, and `mystats.rs` is not touched by Phase 2.
- **Cost:** Small: My Stats UI is not shipped yet.
- **Status:** active

### R74 — Match Details names

- **What:** Match Details names: in a match the user did not play, a row shows as its agent name (or the remembered name) when its puuid is incognito on the live screen now or its stored name is empty; otherwise Visible. This replaces audit §2.12's "flagged incognito in that match".
- **Why:** match-details has no incognito flag (`endpoints.rs` `MatchPlayer`), so §2.12 can't be implemented.
- **Cost:** If match-details returns real names for players who were incognito in that match, those names show for players not on screen now, and the same risk covers Top 3 names built from the details player's own history (R94). Task 19 item 12 checks this against the user's DB. The follow-up would hide every name in matches the user didn't play, and every Top 3 name learned only from such matches, except players visible on the live screen now.
- **Status:** narrowed by R145 and R150 (a name from a match the user did not play may now be the remembered name; blank rows fall back to the players-table name; the live incognito rule stands)

### R75 — Fixed 5-slot Live columns and one card size

- **What:** Live columns are fixed 5-slot tracks in every stage, and a card is always 485×139.8. In Agent Select the right column holds 5 dashed card-size slots. `closed` with no known self shows an empty grid and no skeleton.
- **Why:** The card's inner grid is tuned to ~140px, and CLAUDE.md wants one identical card. A skeleton that never resolves breaks Review Focus 2.
- **Cost:** Lobby 1 leaves the rest of the column empty.
- **Status:** active

### R76 — Four party colours per accent

- **What:** 4 party colours per accent (spec §1.6 table and rule).
- **Why:** There can be up to 4 real groups (2 ally + 2 enemy), and the design has only 2 colours.
- **Cost:** Two added colours.
- **Status:** active

### R77 — Avg party rank in the top bar

- **What:** Top bar: an `Avg party rank` block (the mean of the ranked members' tiers, .5 rounds up; skeleton and `—` states; tip `From {count(n, 'ranked player')}`). The `My Stats` tab stays inert until the page exists.
- **Why:** R71 puts APR in the top bar, and the design has no APR. A clickable tab leading to a missing page would be a dead end.
- **Cost:** Label wording.
- **Status:** active for APR; the inert `My Stats` tab is superseded by R175 (the tab is a real button now)

### R78 — Tooltips the design leaves unbound

- **What:** Tooltips the design leaves unbound or lacks: K/D and HS% `Last 5 competitive`; the Win% structured tip with row 2 `This act`; hero Games and W / L `This act`; the loadout well `{Tier} Edition`; the act name in the Peak tip; the Peak tip on the whole group. Tips hide on click, navigation and scroll. Counts use singular forms.
- **Why:** Scope lives only in tooltips (R67), and CLAUDE.md puts extra detail behind hover.
- **Cost:** Extra tips.
- **Status:** active

### R79 — Type and asset calls

- **What:** Type and assets: weight 400 is dropped (hero `Prev —` at 500); ⟲ is an inline SVG (Lucide `rotate-ccw`, ISC notice in `THIRD-PARTY-NOTICES.md`); `cardLarge` is 160px wide (not the audit's ~128, because 160 covers the 109px hero at 150% DPI); `contentTier` 32px; `chroma` 256px.
- **Why:** Lightness. The Segoe glyph exists only on Windows.
- **Cost:** A few KB.
- **Status:** narrowed by R149 (the ⟲ part is superseded: R149 put the design's own Segoe UI Symbol glyph back, so the Lucide `rotate-ccw` swap and its ISC notice are gone; the rest stands)

### R80 — States built from the design's parts

- **What:** States built from the design's parts: the rank `null` box; Details and Match loading skeletons; meta pending → skeleton bars, meta miss → `—` (spec §5.8).
- **Why:** The design has none of these states, and skeletons carry no text.
- **Cost:** Small UI tweaks.
- **Status:** active

### R81 — Per-match rank rule

- **What:** Per-match rank = Riot's at-match `competitiveTier`, falling back to `comp_updates.tier_after`. One rule for recent rows, encounters and the scoreboard. `queueLabel` table (`''` → `Custom`). Perspective: the user's team first, else the focus player's team.
- **Why:** One scoreboard must not mix post-match and in-match ranks.
- **Cost:** A promotion game shows the rank the player had during it.
- **Status:** active

### R82 — Text that can't fit in Details

- **What:** Text that can't fit in Details: Top 3 names wrap anywhere and never truncate. The encounters header name, the loadout skin line and the recent-row map name keep the design's ellipsis. The ⟲ slot is `min-width:36px` and grows for counts of 3+ digits.
- **Why:** CLAUDE.md asks for Top 3 with full names; the other ellipses are the design's own; the count must never overlap the tag.
- **Cost:** A long Top 3 name takes 2 lines.
- **Status:** active

### R83 — Other players' loadouts

- **What:** Other players' loadouts are readable only In Game and for Agent Select allies; outside those stages their weapons are `—`. The user's own loadout comes from `personalization/v2/players/{puuid}/playerloadout` once per connect (pre-flight R90), so it shows everywhere. Skin names resolve from valorant-api when Details opens, independent of the 120s Riot details sync; while a name resolves, the weapon key is absent and the UI draws a skeleton well (R97).
- **Why:** Lightness: one extra call per connect, no endpoint for other players. A loadout that arrives after the first open must still get its names.
- **Cost:** Other players' loadouts stay blank in the Lobby.
- **Status:** narrowed by R90 (the user's own loadout shows everywhere)

### R84 — Remembered-name map ids resolve at display time

- **What:** Remembered-name map ids are resolved when the DisplayName is built (publish, Details, match view) through `AppState::map_uuid`, never when the DB is read. One helper does the conversion for all three call sites: `KnownName::with_map_uuid` (R95).
- **Why:** Same reason as `LiveState.map_id`: the asset tables can load after the DB read (first start, offline).
- **Cost:** None.
- **Status:** active

### R85 — Phase 2 columns ship as migration 2

- **What:** The Phase 2 columns ship as migration 2 (4× `ALTER TABLE … ADD COLUMN`). Migration 1 is frozen, and there is no self-heal (pre-flight R89 replaced the self-heal this plan first proposed).
- **Why:** CLAUDE.md requires versioned migrations (`PRAGMA user_version`). Each column is defined once, and an earlier build's DB upgrades through the normal runner.
- **Cost:** None: no install exists yet.
- **Status:** active

### R86 — Process: spec, path-limited commits, changelog

- **What:** Process: the build spec is committed as `docs/design-spec.md`; every commit is path-limited (`git commit -m … -- <paths>`); CHANGELOG is filled at release from `git log`. Task 12 is closed as complete (R87), and the lanes run as R98 sets them.
- **Why:** CLAUDE.md wants every decision recorded, and the user wants every step pushed. CHANGELOG gets one writer across the lanes.
- **Cost:** None.
- **Status:** active

### R87 — Task 12 is complete

- **What:** Task 12 is complete (`37b9631..c9ad7f4`, R73). Mark it done in `docs/plan.md`. The 298-test / clippy baseline moves to Task 14 Step 0.
- **Why:** Task 12 was the Task 9a fix round and it landed.
- **Cost:** None.
- **Status:** active

### R88 — comp_matches fix folded into Task 15; current_act() rollover re-deferred

- **What:** Fold the `comp_matches` per-row-subquery fix into Task 15 (LIMIT the player's rows first, then join). Re-defer the `current_act()` rollover explicitly (user 2026-09-29: act-start handling later). `current_act()` picks the act by valorant-api date, so around an act change the live card can show `—` for rank and Win% until Riot rolls over.
- **Why:** Task 15 rewrites that query anyway, and the user said act-start handling comes later.
- **Cost:** A few hours of `—` around an act change.
- **Status:** active for the `comp_matches` fix; the rollover half is superseded by R162 (the current act comes from Riot's content-service)

### R89 — Migration 2, migration 1 frozen, no self-heal

- **What:** The Phase 2 columns go in a versioned migration 2 (`ALTER TABLE ADD COLUMN` ×4). Migration 1 is frozen and there is no PRAGMA self-heal.
- **Why:** Satisfies CLAUDE.md's versioned-migration rule with one definition per column.
- **Cost:** None (no installs exist).
- **Status:** active

### R90 — Own loadout fetched once per connect

- **What:** Fetch the user's own loadout once per connect via pd `personalization/v2/players/{self}/playerloadout` (read-only, 1 call), so the user's own Details shows skins outside a match. Others keep R83 (`—` outside Agent Select and In Game).
- **Why:** The user's own Details should not be blank outside a match.
- **Cost:** 1 call per connect.
- **Status:** narrowed by R148 (the endpoint is `personalization/v3`; v2 returns 404 on every shard)

### R91 — Arabic line for the Claude calls

- **What:** Append one dated Arabic line to `docs/requirements-raw.md` (after the last line) listing the user-visible Claude calls R74–R86.
- **Why:** CLAUDE.md requires every decision to be recorded there.
- **Cost:** None.
- **Status:** active

### R92 — Spec outranks the design only for gap-fills

- **What:** `docs/design-spec.md` outranks the design file only for its [C]/[bound] gap-fills (R74–R86). Any change to a design-covered visual goes to the user.
- **Why:** The user approved the design, not Claude's spec.
- **Cost:** None.
- **Status:** active

### R93 — Hidden players in every Match Details view

- **What:** In every Match Details view, a row whose puuid is hidden on the live screen now goes through `display_name` (remembered style if known).
- **Why:** The card, Top 3 and the scoreboard stay consistent.
- **Cost:** One branch and one test.
- **Status:** active

### R94 — Follow-up check of R74 for Top 3 names

- **What:** Extend the R74 known-risk follow-up to Top 3 teammate names. The Task 19 checklist asks the user for `opd1.sqlite` and compares `match_players` names for puuids marked Incognito in `coregame_match.json`.
- **Why:** R74 rests on an unverified assumption about what match-details returns.
- **Cost:** None.
- **Status:** active

### R95 — De-duplicate lookup helpers

- **What:** T16 `meta.ts` owns `lookup` / `tierLabel` / `agentName` (`Meta|null → string|null|undefined`) used by `tips.ts`, `live.ts` and `details.ts`. `names.rs` gets one `KnownName::with_map_uuid` used by all three Rust call sites (`map_id` is always the uuid after conversion). T16 rewrites `demo/fixtures.ts` to the new wire types.
- **Why:** One definition per helper.
- **Cost:** None.
- **Status:** active

### R96 — Make the three non-failing tests real

- **What:** The contract test uses `satisfies ShapeOf<X>` plus key-set equality. Drop the DEV DOM scan from Review Focus evidence (keep it as a regression check). Task 15's act-wins test moves to Task 14, where it goes red.
- **Why:** A test that cannot fail proves nothing.
- **Cost:** None.
- **Status:** active

### R97 — Loadout weapons are tri-state

- **What:** Loadout weapons are tri-state: absent means resolving (skeleton well), `null` means none (`—`), a value is the skin. Same serde pattern as R23.
- **Why:** CLAUDE.md's skeleton rule.
- **Cost:** One more state in goldens and the UI.
- **Status:** active

### R98 — Phase 2 split into parallel lanes

- **What:** Split Task 16 into 16a (independent of the new contracts: merge main@c9ad7f4, delete v1 visuals and the tokens.css import, fonts, theme store, tooltip action, `meta.ts` lookup, shortcuts/init/auto-switch, demo registry with new wire types) and 16b (`types.ts` sync and contract vitest after Task 15). Lanes: [13 on main] ∥ [14→15 in `.worktrees/p2-backend`, branch `p2-backend`] ∥ [16a on `ui-lane`]; then 16b → 17 → 18 → 19. Every task still gets its own review, and branches are pushed after each task (CI builds main only).
- **Why:** Speed: the user (2026-09-29) approves more parallel agents at higher token cost, with no quality loss.
- **Cost:** 2 extra merges, each reviewed.
- **Status:** active

### R99 — An agent counts as selected only when locked

- **What:** User decision: 'selected' means locked only (`CharacterSelectionState == "locked"`). Before the lock: empty agent slot, and `Hidden Player` for hidden players.
- **Why:** The user's answer to the decisions audit (2026-09-29).
- **Cost:** None.
- **Status:** active

### R100 — The user's own card shows their own level

- **What:** The user's own card shows their own level even when `HideAccountLevel` is set (same logic as R49's own name). Everyone else with a hidden level shows `Lv —`.
- **Why:** The user said to hide it when hidden, and the own card can't be hidden from yourself.
- **Cost:** One line if wrong.
- **Status:** active

### R101 — Own-history backfill

- **What:** User decision: on connect (and after each match), page the user's own match history (all queues) back to the anchor, which is the newest match of the last completed backfill run (`app_state` key `self_history_anchor`), then fetch match-details only for unknown ids. The anchor moves only when a run completes. Throttled, lowest priority behind live cards. The first run, with no anchor yet, stops at the current act start or 200 matches (Claude's lightness bound, not the user's).
- **Why:** The user wants matches played while the app was closed never to be forgotten; the bounds keep it light. The anchor is not "the newest stored match id": the live sync stores the user's newest matches first, so that rule would stop on page 0 and miss the matches played while the app was closed (plan Task 15b, "Why an anchor and not the newest stored match").
- **Cost:** Extra calls on the first run only.
- **Status:** active

### R102 — A match with no stats row renders dashes

- **What:** The deferred 9a minor (a no-stats match shows K/D/A 0/0/0) is carried into Task 15: a match with no stats row renders `—` for K/D/A, K/D, ACS and HS% in the Details recent rows and in the My Stats last 5.
- **Why:** The design rule "missing values show `—`".
- **Cost:** One small UI/format revert.
- **Status:** active

### R103 — Model policy for Phase 2

- **What:** Implementers, fix rounds 1–3 and scoped re-reviews run on Sonnet 5.5. Every first task review, diverse-lens review and the final whole-branch review run on Opus 5.5. Fix rounds 4–5 run on Opus. If a Sonnet-built task draws a Critical in its first review, that task type switches back to Opus implementers.
- **Why:** The user asked to try Sonnet 5.5 only if quality is untouched, and the Opus review gate keeps quality equal.
- **Cost:** Extra fix rounds.
- **Status:** superseded by R147

### R104 — Ultrareview finding on requirements-raw.md:66

- **What:** The cloud ultrareview (main, 1 nit) found that `docs/requirements-raw.md:66` still points at `docs/design-history/`, which the staged deletions remove. Task 13's §2.15 marker for :66 covers it (the path is now `ارشيف/docs/design-history`, a local archive outside git).
- **Why:** No separate fix is needed.
- **Cost:** One stale line.
- **Status:** active

### R105 — Lane B branches from the fixed commit c9ad7f4

- **What:** Lane B creates `p2-backend` from the fixed commit `c9ad7f4`, not the moving `main`. Task 15 Step 0 merges `origin/main` into it once Task 13 has landed.
- **Why:** Its review diff stays lane-B only.
- **Cost:** None.
- **Status:** active

### R106 — Keyboard focus ring

- **What:** The design has no focus ring, so the UI adds a subtle `:focus-visible` outline in the accent colour. It shows only for keyboard focus, so mouse visuals match the design exactly.
- **Why:** Accessibility at zero visual cost.
- **Cost:** One CSS rule.
- **Status:** active

### R107 — large_art is optional in the playercard cache

- **What:** The playercard cache field `large_art` is optional (`Option` + `serde(default)`). A missing `largeArt` never breaks the small or wide art. When CardLarge is requested and a cached entry lacks it, the playercard table is refetched once.
- **Why:** Robustness, and old caches stay valid.
- **Cost:** A little extra code.
- **Status:** active

### R108 — Ledger line references are corrected after the split

- **What:** After the line-211 split in the ledger, line references (for example the R88 re-deferred `:307, :309`) are corrected to their new numbers instead of being copied verbatim.
- **Why:** Accurate pointers.
- **Cost:** None.
- **Status:** active

### R109 — THIRD-PARTY-NOTICES.md ships inside the installer

- **What:** `THIRD-PARTY-NOTICES.md` ships inside the installer. Task 19 bundles it as a resource, plus generated crate and npm notices if that is cheap.
- **Why:** Licence hygiene for the binary friends receive.
- **Cost:** One resource entry.
- **Status:** active

### R110 — Task 13 doc minors are fixed now, in one batch on main

- **What:** Task 13's documentation Minors are fixed now, in one Sonnet batch on `main` (docs only; lane A is idle). Left out on purpose: the THIRD-PARTY bundling (R109, Task 19), the post-commit `staged_deletions` check noise (expected, no fix) and the CI Node-20 warning (already bumped in lane B, `0cb68c1`).
- **Why:** Stale statuses and wrong process facts mislead later agents.
- **Cost:** One small docs commit.
- **Status:** active

### R111 — The §0 update for the --bg fallback and the R106 ring is done on main

- **What:** The `docs/design-spec.md` §0 update for the `--bg` fallback and the R106 focus ring is done on `main`, inside the Task 13 minors batch (lane A owns `docs/**`), not on `ui-lane`.
- **Why:** It keeps lane ownership.
- **Cost:** None.
- **Status:** active

### R112 — Task 16a minors fixed in one batch on ui-lane

- **What:** Task 16a's Minors are fixed in one Sonnet batch on `ui-lane`: the design's party fallback per R92, the tooltip reading `data-tip` inside the 450ms timeout, a tautology test, DEV self `isSelfParty` true, DEV fallback rows honouring the hidden kind, the node types ref, a duplicate `expect`, a one-line Tooltip comment, the theme store's double apply, and the `imgBase` memo. Deferred to the Task 17/18 briefs: the TopBar right group (spec §2.2) and the rest of the §4.2 tips catalogue, which lands in `tips.ts`.
- **Why:** Not recorded in the ledger.
- **Cost:** Not recorded in the ledger.
- **Status:** active

### R113 — Every task reads the committed spec

- **What:** From now on every task reads the spec from the committed `docs/design-spec.md` on `main`, never from the `.superpowers/design/cd-v8/spec.md` copy. That copy is now stale: only the committed copy is edited after Task 13 (plan line 476).
- **Why:** A stale copy would send implementers to outdated rules.
- **Cost:** None.
- **Status:** active

### R114 — The Task 19 checklist adds four real-client checks

- **What:** The Task 19 Arabic test checklist adds the real-client checks the VPS cannot do: (a) the user's own level shows with HideAccountLevel on (the roster's AccountLevel is real for self); (b) the act Win% matches the client (`NumberOfWinsWithPlacements` is present); (c) the agent appears only after Lock (the state string is `locked`); (d) the card art is sharp at 160px (`largeArt`).
- **Why:** The VPS has no game client, so only the user can check these.
- **Cost:** Extra checklist lines.
- **Status:** active

### R115 — Task 14 minors fixed in one batch before Task 15

- **What:** Task 14's Minors are fixed in one Sonnet batch on `p2-backend` before Task 15 starts: `map_uuid_for_path` searches under the read lock without cloning the tables (lightness first); `history_ok` and `party_ready` are set only when the `index_history` write and the reload succeed; the MMR snapshot is loaded once; the R99 doc fixes (`model.rs:558`, `state.rs:329`); the `SeasonInfo.number_of_wins` field doc (`stats.rs`); the test `closed_stage_never_shows_skeletons` lists `actWins` and `actGames`; the assets test helpers are de-duplicated; and `display_name` drops a `map_id` that starts with `/` (defence in depth for hidden names).
- **Why:** Task 15 edits the same files, so the Minors go first.
- **Cost:** Not recorded in the ledger.
- **Status:** active

### R116 — Player Details hero K/D and HS% use the current act

- **What:** User decision (2026-09-29). In Player Details, the hero stat strip (K/D, HS%, Win%, Games, W / L) takes its sources as follows. `Win%`, `Games` and `W / L` stay act-wide, from the MMR data (the design already works this way). `K/D` and `HS%` switch from the last 5 competitive matches to the player's most recent up to 20 competitive matches of the current act, the same match set Top 3 Teammates already fetches. The hero tooltip becomes `Last {N} competitive · this act`, with N the number of act matches used. With no current-act competitive match, both values show `—`. The Live card is unchanged: K/D and HS% from the last 5 competitive, tooltip `Last 5 competitive`. Backend (Task 15): `PlayerDetails` gains `heroKd` (number|null), `heroHs` (number|null) and `heroMatches` (number).
- **Why:** The user wants the hero to describe the act, like Win%, Games and W / L beside it. It costs zero extra requests because Top 3 Teammates already fetches those matches.
- **Cost:** The hero K/D and HS% can differ from the same player's card values, because the sets differ (up to 20 act matches vs. the last 5). Three new fields in Task 15.
- **Status:** active

### R117 — DECISIONS.md is re-synced with every ledger ruling

- **What:** `docs/DECISIONS.md` is re-synced with every ledger ruling from R109 on. This was planned for Task 19's docs pass. The other half, spec §7.2's hero K/D and HS% source (`heroKd`, `heroHs`, `heroMatches`), was fixed in the docs catch-up commit `39baf13`.
- **Why:** One docs writer at a time.
- **Cost:** A stale mirror for a few hours.
- **Status:** active (fulfilled by the 2026-09-30 docs pass that added R109 to R125)

### R118 — Task 17 starts now, in parallel with Task 15

- **What:** Task 17 starts now on `ui-lane`, in parallel with Task 15, built against the provisional `types.ts` (spec §7.1 already carries Task 14's live-card fields). Task 16b later reconciles the types for the 16a and 17 files and re-runs the fit specs. New order: [T15 ∥ T17] → land T15 → [T15b ∥ T16b] → T18 → T19. The files stay disjoint (`ui/**` vs `crates/**`). R121 sets the lane B order.
- **Why:** Speed. On 2026-09-29 the user asked for faster work without any quality loss and approved more parallel agents at a higher token cost.
- **Cost:** Small type renames in 16b.
- **Status:** active

### R119 — Opus implementers for the UI screen tasks

- **What:** The UI screen tasks (17, 18) use Opus implementers. Sonnet stays for backend, docs and fix rounds (R103).
- **Why:** Design fidelity is the user's top concern, and these are design-judgment tasks (the skill's Model Selection).
- **Cost:** More tokens, same or better quality.
- **Status:** superseded by R147

### R120 — heroHs is a 0..1 fraction

- **What:** `heroHs` is an `f32` between 0 and 1, like every HS% in spec §7.1. The R116 dispatch had said `u32`; the spec convention wins.
- **Why:** One representation for every rate (R5).
- **Cost:** None.
- **Status:** active

### R121 — Lane B runs Task 15 review, then Task 15b

- **What:** Lane B is sequenced as the Task 15 review, then Task 15b. They do not overlap. The docs catch-up on `main` runs right after the Task 15 review lands, before 16b: spec §7.1 `kills`/`deaths`/`assists`/`kd` become `| null` for R102, and spec §7.2 hero K/D and HS% come from `heroKd`/`heroHs`/`heroMatches` (R120).
- **Why:** The fix rounds and 15b would edit the same files on the same branch. The UI lane (T17, then 16b, then T18) is the critical path, so this costs no wall-clock time.
- **Cost:** None in wall-clock time.
- **Status:** active

### R122 — Own-loadout mechanism accepted

- **What:** The own-loadout mechanism stays as built: the stored `app_state` key `self_loadout` is restored once in `AppState::new` into `live.self_loadout`, kept in step on change and blanked on account switch. The brief had asked for a per-build read instead.
- **Why:** `skins_pending` and `resolve_skins` are synchronous over memory and need the loadout before the build. It is lighter per build, and the observable order is the same, pinned by `own_loadout_prefers_the_matches_over_the_stored_copy`.
- **Cost:** None.
- **Status:** active

### R123 — Task 15 minors are fixed before 16b

- **What:** Task 15's Minors (14, two of them duplicates) are fixed in one Sonnet batch on `p2-backend` and then landed on `main`, before 16b. The batch: `player_details.recent` is newest first; the `hidden_player` live card carries rank and stats like any card (only the name is hidden); `first_half_wins` is `None` when the rounds lack `round_num` or `winning_team`; one shared `needs_known_name` predicate holds the hidden-name rule in one place; the R88 plan test asserts the CO-ROUTINE/MATERIALIZE mine row; the hero-stats doc wording; the contenttiers-only refetch; `log::warn` on a `self_loadout` read error; the key is deleted instead of written as `""`; an RAII guard for the skin resolve (R97); the Task 15 pipeline tests move to their own module; `sync_details`'s fallback loadout triggers `resolve_skins`; and `player_loadout` takes no puuid (self only, R90).
- **Why:** Two of the items change the golden contract that 16b builds against (`player_details.recent` newest first, and the `hidden_player` card carrying rank and stats).
- **Cost:** Not recorded in the ledger.
- **Status:** active

### R124 — Scoreboard rows are nullable

- **What:** `ScoreRow` `kills`, `deaths` and `assists` (and any derived +/− or K/D) are `number | null`. They are null when the player has no stats row, never 0/0/0. The row shows `—`. The golden `crates/core/tests/fixtures/contract/match_details_view.json` (row `enemy-3`) is the truth. The backend fix was item 13 of the Task 15 minors batch. Spec §7.1's scoreboard comment ("R102 does not apply") is corrected, and §6.10 and §7.2 say `—` for null, in the 2026-09-30 docs pass.
- **Why:** The golden showed a no-stats scoreboard row as 0/0/0, which breaks the rule that a missing value shows `—`.
- **Cost:** One `Option` field.
- **Status:** active

### R125 — The card's Prev row reads icon, Prev, name

- **What:** The user delegated the call to Claude (choose what is best for the user experience), so this is a user-delegated change to the design (R92). The card's Prev row becomes icon · `Prev` · name (variant V4 in `.superpowers/sdd/plan/shots-prev/`): the greyed 18px prev icon is centred in the 40px rank-icon column, then comes the 12px `Prev` label, starting right after that icon column (R126, spec §3.6), then the tier name right after the label. With no prev tier: an empty icon column, then `Prev —`. The Peak row is unchanged (50px label), and so is the hero Prev row. Applied on `ui-lane` after the Task 17 review loop; the design-ref visual test accepts the Prev row as an approved deviation. Spec §3.6 (Prev) and the §8.4 geometry table record it.
- **Why:** The user found the 50px label gap untidy. V4 mirrors the rank block above it (icon column, then text column), puts the word next to its value, and matches the design's own Details hero Prev order (icon · Prev · name, v8:214-218).
- **Cost:** One CSS change.
- **Status:** active

### R126 — The Prev label starts right after the 40px icon column

- **What:** The R125 Prev label starts right after the 40px icon column (x = 40 in the row, as spec §3.6 says), not at the rank-name column's x = 50. The ui-lane change verifies it with the fit specs in all 3 fonts.
- **Why:** At x = 50 the widest `Prev` + `Ascendant 3` overflows the 158px column in the wider fonts, and the Prev row must never clip.
- **Cost:** A 10px nudge.
- **Status:** active

### R127 — The empty agent box stays in every stage

- **What:** The empty 40×40 `--raised` agent box stays in every stage: Agent Select before a lock, Lobby and Closed. It is never a placeholder agent and never the skeleton shape. Spec §3.3, §5.2 and §5.3 say so.
- **Why:** The card is identical everywhere, R99 names it the empty agent slot, and a transparent slot would leave a hole beside the name. The user saw the Lobby and Closed screenshots and objected only to Prev.
- **Cost:** One CSS rule.
- **Status:** active

### R128 — The remembered name's dotted underline gets clip room

- **What:** Accept the remembered-name underline fix: the name span (`Name.svelte` `.rem`) uses `overflow:clip; overflow-clip-margin:6px`, so the dotted underline (offset 4px, hero 5px) is not clipped at the card, hero and row sizes. Spec §3.4 says so.
- **Why:** CLAUDE.md "Card indicators" requires the dotted underline and the design's CSS asks for it; `overflow:hidden` (needed for the ellipsis) cut it away. The user delegated decisions.
- **Cost:** Revert one block.
- **Status:** active

### R129 — Spec text catch-up for Task 17's calls

- **What:** The spec text on `main` catches up with Task 17's calls: §3.3 and §3.5, the card ⟲ badge gets `margin-left:5px`, which only shows for 3+ digit counts (a new [C]); §8.3 check 2 exempts the Underline option's bar `.uline`, with the geometry test that keeps it inside the card; §3.3 and §5.4, an absent level is a 48×10 bar and a withheld level is `Lv —`; §2.3, the Avg party rank tooltip is none while loading and `From 0 ranked players` when 2+ players are present and none is ranked; §3.2, the party stripe and tab come before the grid in the markup; §3.6, a current rank with tier < 3 renders like `null`; §5.2 and §5.3, the R127 agent box; §3.4, the R128 underline clip margin.
- **Why:** The code on `ui-lane` already behaves this way (Task 17 review, fix rounds 1 and 2), and the spec must stay in step with the code.
- **Cost:** Not recorded in the ledger.
- **Status:** active

### R130 — Task 17 follow-up batch on ui-lane

- **What:** One Opus (visual) batch on `ui-lane`: the R125/R126 Prev row; Last5 keyed by index (a duplicate `startMs` would drop tiles); `live.ts` Closed shows self only; loading K/D and HS% carry no tooltip; check 9 covers meta-pending hidden names; `varRgb` deduplicated; dead test code removed; the clock pause is relative; the card-click test asserts the right player; a map skeleton assertion; the design gate tightened (threshold near the measured baseline, ⟲ unmasked where stable); check 5 adds a vertical and underline ink probe; the AgentBox test regex; the fit-live deep-equal covers shadow, opacity and border. The tests tsconfig moves to Task 16b.
- **Why:** The Task 17 review left 10 spec-review Minors and 4 design-review Minors outside fix rounds 1 and 2, and R125/R126 still had to land in code.
- **Cost:** Not recorded in the ledger.
- **Status:** active

### R131 — Task 16b may write the UI tsconfig files

- **What:** Task 16b may also write `ui/tsconfig*.json`: a tests tsconfig, so `npm run check` type-checks `ui/tests/**` and `playwright.config.ts`. R130 moved it here. The UI lane is the only `ui/` config writer until Task 19. The plan and spec text that quotes the 1% design gate is updated to 0.02% in Task 19's docs pass (done in the 2026-09-30 docs drift pass).
- **Why:** R130 moved the tests tsconfig from the Task 17 follow-up batch to Task 16b, and the UI lane is the only `ui/` config writer until Task 19.
- **Cost:** None.
- **Status:** active

### R132 — opd1-riot may depend on `log`

- **What:** `opd1-riot` may depend on `log`. `RiotSession::with_global_rate` must be test-only.
- **Why:** `log` is already in `Cargo.lock` through `src-tauri`, so no new crate enters the build.
- **Cost:** None.
- **Status:** active

### R133 — ui/tsconfig.node.json is widened to cover the tests

- **What:** Widening `ui/tsconfig.node.json` to cover `tests/**` and `playwright.config.ts`, instead of a separate `tsconfig.tests.json`, is accepted. Task 18 adds the `details-nostats` and `match-nostats` DEV scenes (the `—` cells).
- **Why:** `package.json` is outside the lane and strictness holds.
- **Cost:** None.
- **Status:** active

### R134 — Task 16b's test-strength Minors go into Task 18

- **What:** Task 16b's test-strength Minors are folded into Task 18 as its first commit: the test that ties `heroHs` null-ness only to `heroKd` is corrected (`heroHs` is also null when no shot landed); the hidden-card stats check runs over every non-visible name, including `hiddenAgent` and `remembered`, and matches `agentId`; the invariant `isSelf` ⇒ `isSelfParty` ⇒ no party and no encounters; a `KindOf` type test for the base kinds; a narrower demo error regex; a fixed overclaiming test title; the report's 15b note corrected.
- **Why:** It is the same lane, and it avoids two agents in one worktree.
- **Cost:** None.
- **Status:** active

### R135 — Task 19 splits into 19a and 19b

- **What:** Task 19 splits. 19a runs now on main, in parallel with Task 18: the release profile, CI check and size guards, the dist guard switched on only if main's pre-merge `ui/dist` passes it, versions 0.2.0, the ARCHITECTURE refresh, the Arabic test checklist (including the R114, R116 and R125 items), THIRD-PARTY-NOTICES as a bundle resource (R109), and the CHANGELOG backend lines. 19b runs after Task 18: merge `ui-lane`, full suites, screenshots and README, CHANGELOG `[0.2.0]`, the dist guard on, tag `v0.2.0`.
- **Why:** Speed. The user OK'd more tokens for speed, and quality stays the same. The 19a files are disjoint from `ui-lane`.
- **Cost:** A rebase.
- **Status:** active

### R136 — The final whole-branch review splits too

- **What:** The final whole-branch review splits like Task 19. The backend whole-branch review (`c9ad7f4..main`, over `crates/`, `src-tauri/` and `.github/`) runs as soon as the Task 15b review lands, in parallel with Task 18. The final review after 19b then covers the UI, the integration and the backend delta only. Both use the most capable model.
- **Why:** Speed. Same coverage, earlier.
- **Cost:** None.
- **Status:** active

### R137 — Task 15b Minors batch

- **What:** A Sonnet batch of the Task 15b Minors on `p2-backend`, then land. `Job::Backfill` is handed out only in Menus and Closed: it never downloads during Pregame or In Game, and never holds the shared bucket with a 429 while live cards need it. Decode errors end the run and keep the anchor (transient), and a per-id failure count in `app_state` skips an id only after 3 consecutive runs. The anchor is tied to its owner, and another owner's anchor counts as none (no delete on a switch); implemented as one key per account, `self_history_anchor:{puuid}`, which is cleaner than an owner field. `begin_backfill` starts the worker only once VALORANT is running, so a Closed start wastes no calls. The `store_details` helper is shared with `fetch_missing_details`, there is a `connect_mock_session` test helper, and the found act start is stored in the run.
- **Why:** Lightness is priority #1, and R101 says the backfill runs behind live cards.
- **Cost:** None recorded.
- **Status:** active (R139 refines which failures count toward the 3-run skip)

### R138 — The release profile keeps unwinding panics

- **What:** Drop `panic = "abort"` from `[profile.release]` and keep unwind. Done in Task 19b, which also updates the ARCHITECTURE line and re-checks the ARCHITECTURE Ctrl+Shift+L/D line after the merge.
- **Why:** The code's panic containment (the JoinSet "task failed" logging, the R97 skin-resolve drop guard) assumes unwinding. With abort, any stray panic in a background task kills the tracker mid-match. The size gain is a few hundred KB against a 2.6 MB installer and a 10 MB cap, so robustness wins over a negligible lightness gain.
- **Cost:** About 200 KB.
- **Status:** active (landed in Task 19b; `[profile.release]` has no `panic` key, and the ARCHITECTURE line says unwind)

### R139 — The 3-strike skip counts only per-id failures

- **What:** The 3-strike skip of the backfill (R137) counts only per-id failures: a Status other than 404 or 429, and Decode. Rate-limit, network and auth errors end the run without counting. Fixed inside the backend whole-branch review's fix dispatch; the ARCHITECTURE and DECISIONS anchor-key wording is updated in 19b.
- **Why:** Otherwise a 3-run network outage would skip ids for good and leave the gap R101 forbids.
- **Cost:** One match arm.
- **Status:** active

### R140 — The encounter Match header date follows the design

- **What:** The Match header opened from an encounter row prints the date without a year for a current-year match (`Sep 12`, the design's own form) and adds the year only for a previous year (`dateShort`). The header opened from a recent row keeps `dateLong`. Spec §6.10 and §4.2.
- **Why:** The design wins where it covers the case, and older encounters need the year.
- **Cost:** One format branch.
- **Status:** active

### R141 — Merge, build, review, fix, then tag

- **What:** The order stays: merge `ui-lane`, build, the UI whole-branch review, fixes, then the `v0.2.0` tag. The merge only joins the branches so the full app can be built and tested. The tag is the release and comes last. A preview build for the user was dispatched on `ui-lane` in the meantime (CI run 36661819503, backend as of `d0f9fc6` plus the new UI).
- **Why:** The user asked to test the current version even with known issues, and a preview build does that without releasing.
- **Cost:** None recorded.
- **Status:** active

### R142 — After the user's own match, names show as match-details gives them

- **What:** User correction 2026-09-30: after a match the user played ends, names show as match-details gives them (CLAUDE.md, "After a match ends"). No hiding. The only thing to prevent on the off-screen Details path was a hidden player's name learned solely from matches the user did not play (R74). During live stages the card rule holds: the remembered style comes only from the user's own finished matches. The backend whole-branch fix follows this exactly, and the controller checks the fix diff for over-hiding.
- **Why:** The user's correction of an over-hiding reading of the hidden-name rules.
- **Cost:** None recorded.
- **Status:** narrowed by R143 (partly superseded: the "prevent names learned solely from matches the user did not play" part no longer applies off the live screen; the after-match rule and the live-stage rule stand)

### R143 — Off the live screen, names Riot returns may be shown

- **What:** User decision 2026-09-30. Names that Riot returns in match-details of matches the user did not play may be shown in the Player Details and Match Details views (off the live screen). Mid-match rules are unchanged (`requirements-raw.md` lines 16 and 47): during live stages a hidden player shows `Hidden Player` or the agent name, or the remembered style only from the user's own finished matches. This supersedes R142's "prevent names learned solely from matches the user did not play" for the off-live views, and CLAUDE.md's "Never take names from other players' match histories" for those views (CLAUDE.md updated). Whether Riot actually returns them is R94's check on the user's PC.
- **Why:** The user's words: «ما منعته، دايم يمدي — بس هل يمدي أساساً؟» (he never forbade it, it's always possible, but is it possible at all?).
- **Cost:** One filter.
- **Status:** narrowed by R145 (the off-screen part stands; the live-stage clause "the remembered style only from the user's own finished matches" no longer applies: R145 allows any known source)

### R144 — One UI batch: the Windows path bug and the Task 18 minors

- **What:** One Opus batch on `ui-lane` now, then a preview CI on `ui-lane`. The Windows path bug: `scenes.test.ts:711` builds a file path from `URL.pathname`; use `fileURLToPath` and sweep every test for the same pattern. The Task 18 minors: clear `openDetails` on leaving Details and drop stale payloads (no old-name flash for a now-hidden player); a details event newer than the in-flight `getPlayerDetails` answer wins; the skeleton hero height equals the loaded one (no 15px jump); the Top 3 remembered-name wrap keeps the eye with the name; wrapped `TopTeammates` names get vertical padding; the unknown-map `—` in the value's own colour; a Hero stat `kind` instead of the label sentinel; a `plural(n, word)` export; the `scrolls()` check matches the real scroll host; `details-nostats` fixture consistency; the Nova check-9 test uses shapes the backend sends; the visual bottom cases really scroll or are removed, plus a hidden-name Details design-ref state; the shared `.sec` and `.panel` styles deduplicated.
- **Why:** The preview CI 36661819503 failed on `windows-latest` on a Windows-only test bug that the Linux runs could not see (`'/D:/a/...'` became `'D:\D:\a\...'`).
- **Cost:** None recorded.
- **Status:** active

### R145 — A hidden player's name is remembered from any source the DB knows

- **What:** User decision 2026-09-30 (the question «من أي مكان نعرفه»). During live stages a hidden player's name shows in the remembered style (eye-off icon and dotted underline) whenever the DB knows it from any source: the user's own finished matches, name-service answers from a time the player was visible, or match-details of matches the user did not play (if Riot returns names there). Still never request a hidden player's name. The tooltip is `Hidden name · known from your match` + map · date when the source match is the user's, `Hidden name · known from a past match` + map · date otherwise, and `Hidden name · seen before` when the name came from a name-service answer with no match. Implemented after the backend whole-branch fix lands: the backend known-name lookup, the contract and its tests, and the UI tips. The docs (CLAUDE.md Hidden names, requirements-raw, DECISIONS, spec §3.4 and §4.2) change in the same pass. Supersedes the source restriction of the line-47 exception and R74's store-only-own-match rule.
- **Why:** The user's answer when asked where a remembered name may come from (`من أي مكان نعرفه`, from any place we know it).
- **Cost:** One query filter.
- **Status:** active (pulled forward from v0.2.1 into v0.2.0 by R148)

### R146 — The fast path: v0.2.0 tags before the UI whole-branch review ends

- **What:** User decision 2026-09-30, the fast path. (1) The preview `Setup.exe` link goes to the user as soon as the `ui-lane` Windows CI is green. (2) v0.2.0 is Task 19b, right after the backend whole-branch fix lands and the `ui-lane` minors batch is green: merge `ui-lane`, full suites, CI, tag. The UI whole-branch review runs in parallel and does not block the tag; its findings ship in v0.2.1. (3) R145 ships in v0.2.1 right after.
- **Why:** The user asked for speed. Per-task Opus reviews and the user's own testing cover the gap.
- **Cost:** Residual UI risk for a few hours.
- **Status:** narrowed by R148 (item 3: R145 shipped in v0.2.0, not v0.2.1; the v0.2.0 tag is held until R148's fixes land)

### R147 — Every subagent runs on Sonnet

- **What:** User decision 2026-09-30. From now on every subagent runs on Sonnet (effort high where a workflow allows it), and only the controller is Opus. The two agents already running on Opus (the backend whole-branch fix phase and the `ui-lane` minors batch) finish rather than restart. Supersedes R103 and R119, and the skill's most-capable-final-review rule.
- **Why:** The user's call; no reason is recorded. The two running Opus agents finish because restarting them would lose their progress and the speed the user asked for.
- **Cost:** None recorded.
- **Status:** active

### R148 — Fix the loadout endpoint and the names in v0.2.0; the tag stays held

- **What:** Fix both in v0.2.0, and the tag stays held. (1) The loadout uses the personalization v3 endpoint. (2) R145 is pulled forward from v0.2.1: known names from any source (own match, other match, name-service `seen`); `DisplayName::Remembered` gains `source` (`ownMatch`, `otherMatch` or `seen`); the off-live views fall back to the players-table name when `match_players` is blank. The backend half landed on `p2-backend` (d32e6d6, not landed on `main` until the UI half was ready); the UI half (types and the tooltip texts per R145) followed on `ui-lane`.
- **Why:** The user's test of the preview (2026-09-30): Match Details of the user's own match showed every other player as hidden and Top 3 showed 3× Hidden Player, and the own loadout rows were all `—`. Research confirmed that personalization v2 `playerloadout` returns 404 on every shard, so v3 (`Guns[]` unchanged) is the fix. Why the stored match-details names are blank was unconfirmed; the legitimate names are name-service answers for visible players (the players table) and the user's own name. Shipping names-all-hidden would be a broken release.
- **Cost:** None recorded.
- **Status:** active

### R149 — The UI batch for v0.2.0

- **What:** One UI batch for v0.2.0 on `ui-lane`, then merged. Details calls `close_details` on leave. The ⟲ badge goes back to the design's Segoe UI Symbol glyph (R92): the app is Windows-only, so the "glyph only on Windows" reason for the Lucide swap (R79) does not hold. `tauri.conf` window `backgroundColor` is `#0e1013` (no white flash). Release-only guards: no context menu, no F5 or Ctrl+R reload, no Ctrl+P or Ctrl+F. The focus ring is not clipped (inset offset on panel rows and the tab). A rejected `player_details` or `match_details` retries once, then shows the `—` state (no skeleton forever). The `now` store sets `Date.now()` on start. A truncated remembered name's tooltip includes the full `Name#TAG`. `dir="auto"` on name spans. A `<svelte:boundary>` around the routes, with the `liveColumns` default. A double-click guard on opening a card. The R145 UI half: `Remembered.source` and the three tooltips `Hidden name · known from your match`, `Hidden name · known from a past match` and `Hidden name · seen before`, with map · date (date only for `seen`).
- **Why:** The UI whole-branch review's findings (an open Details was not refreshed on live changes; the focus ring was clipped by the panel), the user's preview test, and R92 for the glyph.
- **Cost:** None recorded. Parked, not done: Playwright in CI (the harness is Linux-only) and shipping all three fonts (the six options are built now per the user; Settings comes later).
- **Status:** active (its parked Playwright-in-CI item is done by R176)

### R150 — Off the live stages, name-service may be asked for match participants

- **What:** Outside the live stages (Match Details, Top 3, encounters, backfilled own matches) the app may ask name-service for match participants: batched, throttled, only for puuids with no known name, and the results cached in `players`. Riot itself blanks anyone who currently hides their name, so no hidden name is revealed. During the live stages the `VisiblePuuid` rule stays: a live incognito player's name is never requested (the rule is about names only; rank, stats and history are fetched for a hidden player like anyone's). The user's dump is no longer needed to proceed; it remains a check after the next build. Built as a `FillNames` worker job (behind the live cards, ahead of the backfill) plus a `match` event: when a fill stores names, the open Player Details and the open Match Details are rebuilt and emitted (`details`, `match`). Leaving Match Details calls `close_match` (no arguments, idempotent, mirrors `close_details`), so the backend stops rebuilding a page nobody sees.
- **Why:** Scout's evidence (`backend/live_match.py` `reveal_names` and `diagnose_reveal`, 5e0cd0a): Scout PUTs name-service for all match participants and keeps only the non-empty `GameName`. Match-details names are dynamic on the player's current status (blank while they hide now), so history alone cannot reveal hidden names, and post-match names come from name-service with Riot enforcing the privacy.
- **Cost:** Extra name-service calls (bounded, cached).
- **Status:** active (built in v0.2.0; refined by R151 and R153)

### R151 — Names are captured at each match end

- **What:** User decision 2026-09-30, refining R150. Names are captured right at each match end: one batched name-service PUT (at most 100 puuids) for the players of the match that just ended who have no stored name, kept by PUUID forever. A blank answer is asked again only at a later match end where that PUUID appears, with no time-based retry. The marker is `players.name_asked_ms` (migration 3): the start of the newest finished match whose question came back blank, or 0 for an on-demand blank. A name learned this way shows later in the remembered style (source `seen`, R145) when that PUUID appears hidden. Recognition is by PUUID, exact and surviving renames, so no stat-similarity matching is needed.
- **Why:** The user wants a hidden player recognisable the next time they are met. Scout does the same (name-service, encounters keyed by puuid) and has no other post-match route; its only reveal path is a Riot developer key through account-v1, which this app does not use.
- **Cost:** One name-service PUT per match end, bounded and cached.
- **Status:** active

### R152 — The game's own Match History shows real names after a match

- **What:** User confirmation 2026-09-30: the game's Match History shows every player's real name after the match, including players who were shown as their agent during it. So a post-match source exists, and it is name-service (the client and VRY use it; VRY added a streamer-mode filter because name-service returns incognito names). R151's match-end capture asks it. This matches `requirements-raw.md` line 16 (after the match, show what the client shows). During live stages the rule stays: a live-incognito player's name is never requested and only stored names show, in the remembered style.
- **Why:** The user's own observation of the game client, given as the reason a post-match name is possible at all.
- **Cost:** If name-service answers blank for a player who hid during the match, the fill stores only the marker for them and nothing is lost. Whether it does is settled by the user's next test.
- **Status:** active

### R153 — The names fill includes players hidden during the match

- **What:** Ruling 2026-09-30. The between-matches fill asks name-service about players who were hidden in the match that just ended, or earlier this run, like anyone else. R150's first build skipped them, which defeats the fill. Only two things are never asked: the user (the own name comes from the chat session, R43) and an empty id. The one hard guard is the stage: nothing is sent while a match is running (Agent Select, In Game, a Deathmatch). It is checked where asks leave the queue (`take_fill`) and again under the `live` lock right before the request, and asks queued during a match wait and go out after it. A step that is dropped (the poll loop aborts the worker on a stage change) or that loses the Riot client puts its asks back in the queue; a failed request drops them and the next trigger asks again. Player Details (the hero and the Top 3 Teammates shown as Hidden Player, also after the details sync), Match Details and the backfill queue the names they lack, hidden players included. The live-stage lookup (`Job::Names`) is unchanged.
- **Why:** The players who most need a post-match name are the ones hidden during the match (R152). Riot blanks anyone who currently hides, so nothing hidden is revealed.
- **Cost:** Between matches, name-service receives the puuids of players who were incognito in the match that just ended.
- **Status:** active

### R154 — The rank comes from stored data when MMR has no answer

- **What:** User decision 2026-09-30. When a player has no MMR snapshot (the `mmr` request failed or was skipped), the card's Current Rank is the newest current-act competitive-updates row in the DB (tier and exact RR), else the tier of the newest stored current-act competitive match (no RR), else it stays missing (`—`). MMR stays the authority whenever a snapshot exists, including a snapshot that says "no rank this act". Only the current rank falls back: Peak, Prev, Win%, Games and W / L stay loading, then `—`. `rank.rr` becomes nullable (`RankInfo.rr: Option<u16>`, on the wire `"rr": <n>` or `"rr": null`, the key always present); the UI shows `— RR` under the tier name when it is null, same markup and positions. The fallback is one indexed query (`Repo::newest_ranked_match_tier`, asserted by a plan test) and costs no request. The Player Details hero, the off-screen card and the Closed-stage own card go through the same `card_for`, so they get it too.
- **Why:** The user's test of the preview: the own card and the Details hero showed `—` for Rank, Peak, Prev, Win%, Games and W / L while K/D, HS%, last 5 and level showed. The matches and competitive updates were already stored, so the rank could be shown from them.
- **Cost:** One contract field (`rr` nullable, the golden `live_state.json` carries one card with an `rr` number and one with `rr: null`).
- **Status:** active

### R155 — A failed rank sync is retried, bounded

- **What:** Ruling 2026-09-30. A per-player sync whose `comp_updates` or `mmr` call failed (any error but `ClientGone`) is run again: at most twice, the first no sooner than 20 s after the failure and the second no sooner than 60 s after the first retry failed, so a round is at most 3 syncs per player. Only for a player still on the live roster (the user's own Lobby card counts), never while the stage is Closed, and the worker never waits for one: `next_job` hands a due retry out after the first-pass syncs and before the names fill and the backfill, and the poll loop's 2-second tick starts a worker when one comes due. A retry is counted when it is handed out, so one cut short by a stage change still uses its turn, and a retry never starts a round of its own. A later MMR answer replaces the fallback of R154. The schedule is `src-tauri/src/rank_retry.rs` (a pure module that takes the time as an argument); tests move time with a `#[cfg(test)]` clock offset on `LiveModel`, never with real waits.
- **Why:** The cause of the failures was unknown when this was ruled (a 429 or a 4xx); bounded retries help the first and cost little in the second. The user's later log showed a 500 on every `mmr` call (R159 targets that), which a retry does not fix.
- **Cost:** At most 2 extra syncs per player over about 80 s (20 for a full In Game roster that keeps failing), through the same throttle. A retry runs the whole `sync_player` again (comp updates, history, unknown match details, then `mmr`). Retries also run in Agent Select and In Game.
- **Status:** active (counting amended by R160: a rank part held back locally by the mmr hold uses no retry)

### R156 — Encounters count competitive matches only

- **What:** User decision 2026-09-30. The ⟲ badge and the Player Details encounter list count only shared competitive matches, with or against. Deathmatch, unrated, spike rush, swiftplay and every other queue do not count. This supersedes CLAUDE.md's "Count every past match shared with the user". Party detection (any mode, Deathmatch included), the known-name lookup (any match) and Top 3 Teammates (already competitive) are unchanged. `Repo::encounter_counts` and `Repo::encounters` join `matches` on `queue_id = 'competitive'`, and a plan test asserts they stay index-backed.
- **Why:** The user's call; no reason is recorded.
- **Cost:** One SQL condition.
- **Status:** active

### R157 — Friends' names from the friends list (withdrawn)

- **What:** Proposed 2026-09-30, withdrawn by the user the same day: take a friend's name from the local `/chat/v4/presences` answer. Friends get their names the same way as every other player (the R151 to R153 fill), so no separate friends-list source exists and none is built.
- **Why:** The user: "same way as the other players".
- **Cost:** None.
- **Status:** dropped (withdrawn by the user, 2026-09-30)

### R158 — The diagnostics explain themselves

- **What:** Ruling 2026-09-30. (1) `dump_raw` (Ctrl+Shift+D) also writes `log.txt`, exactly what `copy_log` returns (Ctrl+Shift+L), even with no session or nothing captured, since the log is what explains why. (2) The client version and where it came from are logged once per session (`client version X (from Y)`, with `Y` one of `presence`, `valorant-api`, `log`, `fallback`); an empty, whitespace-only or unsendable version from a source is no answer and falls through to the next. (3) Every failed remote call logs its status and Riot's `errorCode` in the warn line: `RiotError::Status` is now `Status(u16, String)` carrying a log-safe excerpt of the answer (redacted, one line, full uuids shortened to 8 characters, at most 200 characters), and a transport or decode error names its kind (`timeout`, `connect`, `body`, `decode`, `request`, `builder`, `other`). (4) `dump_raw` writes the last non-2xx answer of each endpoint as `<key>.error.json` or `<key>.error.txt` (status and redacted body, at most 4 KB, full puuids shortened, a JWT quoted inside a longer string redacted too), from a store apart from the 2xx captures; the error files of an earlier dump are removed on each dump.
- **Why:** The user could not deliver the clipboard log, and the first dumps had no `mmr.json` because only 2xx bodies were captured, so the cause of the failing `mmr` calls was invisible. The user's log then showed HTTP 500 for every `mmr` call.
- **Cost:** None. `dump_raw` no longer errors without a session; it writes the log alone.
- **Status:** active

### R159 — Client version and requests as Scout sends them

- **What:** User decision 2026-09-30: «سو مثل سكوت» (do it like Scout). (1) The client version comes from, in order: the user's own VALORANT presence `partyClientVersion`, valorant-api `/v1/version` `riotClientVersion`, the `CI server version:` line of `ShooterGame.log`, then the literal `release-09.00`. The external-sessions source is dropped (supersedes R17). (2) A session built before the game's presence existed is corrected once, at the first VALORANT presence, from the blob the poll tick already read (no request); a presence with no usable version leaves the lower value in place and is tried again on a later tick. (3) Every remote (pd/glz) call sends Scout's `User-Agent: ShooterGame/13 Windows/10.0.19043.1.256.64bit`; local calls send none. (4) gzip is on for the remote client and the two valorant-api asset clients, and off for the local client (`127.0.0.1`, where compression only costs CPU). reqwest decodes before the body is buffered, so captures and error excerpts see plain JSON.
- **Why:** The user's log showed HTTP 500 on `mmr` for every player while `competitiveupdates`, history and match-details worked. Our version source and the missing User-Agent were the two differences from Scout that could explain a per-endpoint failure, but that cause is an inference until the user's next test: R158 logs the version, its source and each failure's `errorCode`.
- **Cost:** If `mmr` still fails, R154 still shows the rank and the next log says more. gzip adds `async-compression` and friends to `Cargo.lock` (all on the existing tokio/reqwest stack).
- **Status:** active

### R160 — Requests are held per family, in full, and mmr never blocks the worker

- **What:** User report 2026-10-01 after testing 7943aff: "there is still a delay or a bug in the requests"; ruling made on the Scout comparison (`riot_client.py`, `live_match.py`). (1) `Retry-After` is honoured in full, no 30 s cap; a 120 s ceiling remains only against a garbage header (`throttle::MAX_HOLD_SECS`, `session::MAX_RETRY_AFTER_SECS`; supersedes R26's 0-30 s clamp). (2) Two hold families, `mmr` (a pd path starting `/mmr/`: the mmr, competitive-updates paths) and `other` (every other pd/glz call). A 429 holds only its own family; the token buckets (global 10 rps burst 10, mmr 0.4 rps burst 24) stay shared. An mmr call never waits on the `other` hold, nor an `other` call on the mmr hold. (3) While the mmr family is held an mmr call returns `RiotError::Held(remaining)` at once, nothing sent; an mmr call that gets a 429 sets the hold and returns `RateLimited` at once, with no in-call retry (Scout: 0), so the single sync worker is never parked for Riot's ~60 s block. The `other` family keeps its two in-call retries, with one bound (fix round 1): an `other` call waits in the call only for a hold of at most 10 s (`OTHER_MAX_WAIT`); with more left it returns `Held` at once, nothing sent, and a 429 whose `Retry-After` brings the hold over 10 s sets it and returns `RateLimited` at once instead of sleeping. The hold itself is still kept in full. The callers handle `Held` as "not now", never as a failure of the id: the roster fetches retry on the poll loop's backoff, the pregame re-poll and loadouts try again on the next tick, a held history or match-details download makes the player's sync due again after the hold (as a held rank part does), the names step and the names fill are deferred until the hold is over (nobody is marked as asked, the asks stay queued, no job is handed out meanwhile), the own-loadout read waits for the hold (3 times at most), and Player Details' sync gives its 120 s claim back and runs again once the hold is over while that player's page is still open (3 times at most; a failed competitive-updates call gives the claim back too and is not run again by itself). (4) `sync_player` no longer ties `mmr` to `comp_updates`: when comp updates failed it asks `mmr` for a player with no stored snapshot (or when the snapshot check cannot run) and keeps skipping it when a snapshot exists; the snapshot is then stored with no latest-match id, so the next sync that sees the match refreshes it. When comp updates answered, `mmr_refresh_needed` decides as before. (5) Amends R155's counting: a rank part that did not run only because the mmr family was held (`SyncOutcome.rank_held`, nothing sent) uses up no retry. The player is due 1.5 s after the hold ends (`rank_retry::HOLD_SLACK`) and is synced again, as often as the hold is still there. A real failure (5xx, 4xx, a 429 that was sent) keeps R155's 20 s / 60 s bound; a hold ends or is extended only by a real 429, so this cannot loop. A retry that finds the hold gives its turn back (`RankRetries::retry_held`); a first pass that finds it starts a round with both retries unused. The same roster, stage and account guards apply. (6) One `warn` line per hold started: `429 on <path, ids shortened>: holding the <mmr|other> requests for Ns`.
- **Why:** The Scout comparison found four causes in our code: `Retry-After` capped at 30 s while Riot's `/mmr/` block is about 60 s (the next call 429'd again); any non-mmr 429 held the global bucket and so delayed mmr too; the worker slept inside the throttle while held; `sync_player` never asked mmr when comp updates had failed.
- **Cost:** The bound on the `other` family is Claude's call (review of the first round): without it a 120 s hold would park the poll loop's live fetches, the backfill and the worker for minutes inside one call. A Match Details page opened during an `other` hold longer than 10 s answers "unknown match" for a match not yet stored, and the user opens it again. An mmr call during a hold fails fast and its card waits (the fallback of R154 shows meanwhile); a held player is synced again later (history and details are already stored, so the repeat costs the two mmr-family calls and one history call). A no-header 429 still holds only 3 s (Scout's fallback). The worker order was unchanged here: `mmr` was still asked after the match-details (R165 moved the rank phase first).
- **Status:** active

### R161 — The live tier from the roster answers fills the rank at once

- **What:** Ruling 2026-10-01 (extends R154's chain). Riot's roster answers already carry each player's current tier and the app dropped it: `pregame/v1/matches/{id}` players carry `CompetitiveTier` for both teams, `parties` members carry `CompetitiveTier`. `core-game/v1/matches/{id}` players carry none (their `SeasonalBadgeInfo.Rank` is the badge the player picked, an earlier act's: 21 where pregame and party said 22 for the same player, so it is not read). The rank of a card with no MMR snapshot is now, in order: the newest current-act `comp_updates` row (tier and exact RR, R154), then the live tier (tier, RR `—`), then the tier of the newest stored current-act match (RR `—`, R154), else missing. A tier of 0 (Unranked), 1 or 2, a missing field or a non-number is no tier. The live tier is a statement about now, so it needs no act tables. It shows as soon as the roster is on screen, before any rank call returns, and a later MMR snapshot replaces it. The tiers of an Agent Select answer (both teams, enemies included) are kept in `LiveModel::match_tiers`, bound to that match's id, and given to the In Game roster, whose core-game answer has none (`roster::fill_tiers`), only when the core-game match id equals the stored one (pregame `ID` is the core-game `MatchID`); a re-poll of the same match merges by presence, a new match replaces, and the map is emptied on every Lobby fetch, on the transition to Menus or Closed, on an account switch and when VALORANT closes, so a failed fetch can never pass one match's tiers to the next. A `CompetitiveTier` that is present but neither null nor an integer is logged once per run at debug. `BuildCardInput.live_tier` and `RosterPlayer.live_tier` carry it; `PregamePlayer.competitive_tier` and `PartyMember.competitive_tier` read it leniently (absent, `null` or a stray type is no tier, never a decode error). The wire contract is unchanged: `rank.rr` is already nullable.
- **Why:** The user reported delays: every card's rank waited for `comp_updates` and `mmr`, the slow, 429-prone family, while Riot had already sent the tier with the roster. Zero extra requests; a hidden player's rank shows like anyone's, since rank is not part of the hidden name; no name is requested.
- **Cost:** A tier from the roster can be one match stale right after a rank change (until the sync lands an exact row or a snapshot, which take priority). A stored MMR snapshot or stored current-act `comp_updates` row of a returning player still outranks the live tier until the sync lands, even when it is older: R161 speeds up players the DB does not know, not every card. Core-game rosters of a match whose Agent Select the app did not see (started mid-match) still wait for the rank calls.
- **Status:** active

### R162 — The current act comes from Riot's content-service

- **What:** Claude's call, from the Scout comparison. (1) One `GET https://shared.{pd shard}.a.pvp.net/content-service/v3/content` per session, made at the first presence that shows VALORANT (never while the game is closed: the session is stale then, R46/R137), with the same remote headers as pd/glz and the global token bucket (it is not mmr-family). (2) The current act is the `Seasons` entry with `Type == "act"` (case-insensitive) and `IsActive`; the previous act is the act whose `EndTime` equals its `StartTime` (Scout's rule). Riot's act id is the same uuid valorant-api uses; valorant-api only supplies display names, and an id it does not know yet is named from Riot's own `Name` fields (`ACT 3` becomes `ACT III`). The real entries have exactly `ID`, `Name`, `Type`, `StartTime`, `EndTime` and `IsActive` (no parent id), so the act's episode is the `episode` entry that contains its start (`episode.start <= act.start < episode.end`, the shortest span if several); `EPISODE n` is kept as is (the label shortens it to `En`) and any other episode name is used as given. A label made from Riot's names may therefore differ from valorant-api's (`E9 A5` against `V26 A5`) once valorant-api catches up; the act id is the same, so nothing else changes. With no episode found the label is the act's own name, and as a last resort `—`, never blank. (3) The answer is held in memory. It is used until the act's end time passes; then the date rule over valorant-api answers and Riot is asked again at once (at most once a minute). A Riot that still calls the ended act active is believed for 10 minutes at a time, so that is also the repeat rate in that case. A failed or empty answer logs one line with the reason; an earlier answer whose act has not ended stays in charge, otherwise valorant-api's date rule is; it is retried every 5 minutes (only while VALORANT runs) and at each new session. The chosen act and its source are logged once. (4) Decoding is tolerant: no shape of body fails the call, and an entry or field that does not parse is left out. (5) Everything reads the one resolution (`AppState::acts()`): the table with Riot's act last, the previous act just before it, and acts valorant-api dates after the current one removed, so `current_act`, the previous-act tier, the peak's "This Act", win rate, My Stats and Details all agree. Cards are built with the clock floored to the hour, so for the first hour of an act the card list says it began at that floored hour and is rebuilt at the next hour; readers of dates (backfill bound, My Stats, hero stats) get the exact start through `AppState::current_act()`. A change of the current act republishes the live cards and rebuilds an open Player Details; My Stats has no push path and takes the new act at its next call. (6) Same rollover, other failure (backend review finding 10): when `/v1/version` answers a new version but one valorant-api table fails, `AssetCache::refresh_tables` loads the old disk tables and returns `Ok(false)` instead of an error, marks them stale (`tables_stale`, the manifest keeps the old version, said once in the log) and `ensure_tables` asks again every 30 s of use until it succeeds (a heal republishes), so a patch never leaves the app without act or agent tables.
- **Why:** The current act ends 2026-10-14. At a rollover Riot's switch and valorant-api's dates can disagree for hours (or valorant-api can be late), and then every act-scoped field is wrong or `—`. Scout asks Riot itself. There is no content-service dump from the user's PC on disk: the fixtures are invented, but their shape (entries with exactly `ID`, `Name`, `Type`, `StartTime`, `EndTime`, `IsActive`) follows a real answer in the techchrism/valorant-api-docs community sample and Scout's parser, so only the first answer from this user's PC is unverified.
- **Cost:** One request per session, plus one every 10 minutes only while Riot's named act has ended and Riot still names it. If Riot's real shape differs (for instance `IsActive` missing), the date rule answers as before and the log line says why. The `shared` host is derived from the pd shard (`shared.eu.a.pvp.net`), as Scout does.
- **Status:** active

### R163 — UI review fixes: keyed close, late opens, `meta` event, image retry

- **What:** Claude's call after the session-3 UI review (I1, I2, M1 to M5), no visual change. (1) `close_details(puuid?)` and `close_match(matchId?)` carry the key of the page that is left; Rust ignores a close for any other page than the one open or being opened. Every `player_details` / `match_details` call takes a ticket from a per-page gate before its build (`AppState::begin_open_details` / `begin_open_match`) and may set the open page only while that ticket is still the latest (`set_open_details` / `set_open_match`, checked under the gate lock); a close, or a newer open, voids it. A build that ends after Back, or after a newer open, no longer re-opens the old page or overwrites the new one. The unkeyed close still closes whatever is open. (2) Rust emits a `meta` event (the same `Meta` the command returns) when the asset tables first load and whenever `ensure_tables` downloaded them again (a new patch), not on a plain version re-check. The UI replaces its `meta` store on it, listening under the same `started` guard as the other events, and stops the `meta` invoke retries once one arrived (the invoke with its backoff stays the startup path). (3) Failed `img://` loads are retried by two Svelte attachments (`ui/src/lib/imgRetry.ts`): `retryImg` on every `<img>`, `retryBg` on the card-art `background-image` (preloaded with `new Image()`); three retries after 2 s, 5 s, 15 s, each with `?r=n` added (`imgproto::parse_path` reads the path only). (4) A Details or Match call with no answer after 15 s counts as rejected and feeds the existing retry-then-settle path; a page that settled empty asks once more on the next `live` event. (5) A rejection starting `invalid ` (a malformed id) is never retried. (6) `PlayerCard` draws no card art when `cardId` is empty, as the hero already did.
- **Why:** The review found a stale `set_open_details` after Back that kept rebuilding and emitting a page nobody sees (and, with a quick second open, froze the new page at its first snapshot), a blank tier icon or agent head for the whole session after one 503, names reading `—` for up to 30 s after the tables loaded by another route (and for the rest of the session after a patch), and pages that stayed on the skeleton or empty forever.
- **Cost:** `close_details` and `close_match` take an optional argument (old callers that send none still work). A page that settled empty shows the empty state until the next `live` event (at most one retry, so a permanently failing page costs one extra call). The card art appears when its image has loaded instead of at first paint; the attachment is keyed on the art's URL string, so the new card objects every `live` / `details` event brings do not clear or reload it. An open that lost (page left, or a newer open) queues no name asks, skin fetch or sync. The `?r=n` retry URLs are cached separately from the original, which costs nothing for a miss.
- **Status:** active

### R164 — In Game roster first, faster roster retry and post-match checks, Peak never below the current rank

- **What:** (1) In Game, the roster is applied (cards published, worker started) before the core-game loadouts are requested; the loadouts follow and are merged into `live.loadouts`, and an open Details is rebuilt, as Agent Select does (R83). A loadouts failure leaves the roster alone. (2) When presence says PREGAME or INGAME but the player endpoint is not ready, the roster retry is a flat 2 s for the first five retries, then the previous backoff (3, 6, 12, 24, 30 s); it still stops when the stage moves on. (3) The post-match rank check runs at about +10, +20, +40, +80 and +160 s instead of +30 and +90 s: each one DB lookup plus the existing re-sync, stopping at the first success. (4) Peak is never below the current rank: when the MMR snapshot's current tier is ranked (3 or more) and is at least the wins-based peak, or there is no wins-based peak, Peak is that tier in the current act (`This Act`); a tie goes to the current act (R7's most-recent rule). `stats::peak` takes the current tier and act id as a new parameter.
- **Why:** Backend review of main 7943aff (findings 2, 7, 6, 4). The loadouts only feed Player Details, yet the ten cards waited for a 290 KB answer (up to 8 s on a timeout). Scout polls every 1 to 3.5 s, and a retry at +3, +9, +21 s left the screen with one card. Riot can publish the `comp_updates` row 35 s after a match, so +30/+90 left the Lobby rank stale for a minute or more. `WinsByTier` only holds wins at the tier a game was won at, so after a promotion or placements the Peak lagged the shown rank; Scout seeds the peak with the current tier (`live_match.py:485-496`).
- **Cost:** The timing numbers are Claude's latency and lightness calls (at most 4 more cheap DB lookups per match end, one extra roster request every 2 s for at most 10 s). The Peak floor follows Scout. Cards of players who have never been seen at a ranked win now show a Peak equal to their current rank instead of `—`.
- **Status:** active

### R165 — Rank first for the whole roster; failed names and partial syncs are retried

- **What:** Ruling 2026-10-01 (controller, on the backend review's findings 1, 3, 5 and 11; findings 8 and 9 are not part of it, see Cost). (1) A player's sync is two phases. The *rank phase* is `comp_updates(0..5)` then `mmr` when needed; its card is stored and published at once (rank, peak, previous act, win rate settle; K/D, HS% and last 5 stay skeletons). The *matches phase* is `match_history(0..2)` then match-details for the missing ids (the history's and the newest five stored competitive updates), then the card is published and the player counts as synced. The worker hands out `Job::Rank` for the whole roster first (display order, three players at once: `RANK_CONCURRENCY`, one `JoinSet` inside one job, so the worker is still one job loop), then `Job::Sync` (the matches phase) one player at a time. Names still come first. Party marks wait for the fresh history as before (`party_ready`). The request count is unchanged; the order is the change, so no card waits for another player's match-details downloads. With three at once, the 429 that holds the mmr family can land while the other two players of the batch are already in flight. (2) A live names lookup that failed is not an answer: only a 2xx (blank names included) marks puuids attempted. After an error they are asked again after 2 s and then 6 s (`NAMES_RETRY_DELAYS`), three tries a stage at most, then they count as attempted (the card shows `—`). The delay is `defer_names` plus the poll tick's `names_wakeup_due` (R160's own mechanism); a `Held` stays R160's path (nobody marked, no try counted). The visible-only gate (`names_needed`, `VisiblePuuid`) is unchanged. (3) A matches phase that failed for real (the history call, a match-details download, a failed write) is *partial* and joins R155's 20 s / 60 s schedule (`LiveModel::note_partial_sync`; a round already pending for the player is left alone). The retry (`Job::RankRetry`) runs both phases again and fetches only the ids still missing. A history or details call refused only by the `other` hold (`Held`) uses no retry and waits for the hold (`note_matches_held`), as R160 ruled. A real failure covers a hold seen in the same sync. (4) `LiveModel::last_match_end_ms` keeps the last match end of the current account; a new match no longer clears what `AppState::match_end_asks` reads. It is copied from `match_end_ms` by `observe_match_end` (called on every worker job and every publish, which always run between the end and the next match), because the poll loop only sets and clears `match_end_ms`.
- **Why:** The backend review (2026-10-01): a cold roster showed skeletons for rank, peak and win rate until the last enemy's match-details had downloaded, and one failed lookup or download left a card without a name or with partial stats for the whole stage.
- **Cost:** Two worker passes over a roster (the card is published twice per player). A retry after a partial sync repeats the cheap `comp_updates` call and, when the snapshot is current, not `mmr`. Concurrency 3 sends up to two more rank requests before a 429 is seen. A history 429 that Riot actually sent is a real failure, so it counts toward R155's bound (R160's test of it was adjusted from "due after the hold" to "due at 20 s"). **Not done, needs a decision:** finding 8 (Player Details claim) is Task E's (R160). Finding 9 (the names fill must skip a puuid who is hidden on the current roster) contradicts R153, which deliberately asks a party member who is still hidden in the Lobby after the match (the user's requirement; `a_player_hidden_in_the_match_that_just_ended_is_asked_and_remembered_next_time` tests it with `party_hidden = true`); its other half (a fill that runs in the 2 s before the poll sees Agent Select) cannot be fixed in the model, which has no roster for the new stage yet. A Lobby that counts as a live stage for the fill needs the user's word. Fix round 1: a 404 on match-details is terminal and not partial (Riot no longer has the match; the backfill treats it the same way), so an inactive stranger's old ids cost no retry rounds. The staggered retries are woken by a separate instant, `LiveModel::arm_names_retry_wake` (the earliest due time among players on the roster who still need a name), armed after every names step and read only by the tick's `names_wakeup_due`; it blocks nothing, so a player who joins while an older retry waits is asked at once. R160's global `defer_names` stays only for `Held`. A task that waited for another task's match-details download that answered 404 does not count the id as missing (`AppState::note_details_404`). "Three tries a stage" means until the next `names_attempted.clear()` (Pregame to In Game keeps the counts). The failure, partial and hold notes are recorded under the same lock as the `ranked`/`synced` mark (`rank_step`, `matches_step`), so an aborted worker cannot lose a retry.
- **Status:** active (amends R155 and R160's worker order; R160's "mmr is still asked after the match-details" is superseded)

### R166 — Tolerant decoding, token refresh, one more name-service try

- **What:** Claude's call from the Scout comparison (§7 items 4, 6, 7) and the backend review's BAD_CLAIMS note. (1) **Tolerant decoding** (Scout: `dict.get(...) or default`). No missing or `null` field fails a whole Riot answer unless it is the identity of the thing decoded. Lists (`Matches`, `History`, `Teams`, `Players`, `Members`, `Guns`, `Loadouts`, `presences`, the name-service answer) decode element by element: a missing, `null` or non-array list is empty, an element without its identity (`Subject`, `puuid`, a gun `ID`) is dropped, never invented. **Fix round 1 (controller ruling):** where dropping would shift what the rest means or store wrong data for good, the answer fails as a whole instead (the existing retry paths ask again): `match-history` and `competitiveupdates` fail if any entry lacks its id or start time (or, for a comp row, its tier or RR; the consumer stores RR as a number, so a missing one cannot be "unknown"), because the newest entry decides party detection and the stored-rank fallback; a missing or `null` list is still empty. A match-details answer fails if any player lacks `subject` or `teamId` or any team lacks `teamId` or `won`, or (competitive, unrated, premier; not a `VoteDraw`) if its `teams` list is empty or more than one team has `won: true` (`check_winner` in `RiotSession::match_details`; fix round 2 ruling: both teams `won: false` is a draw, R29, and is stored as one): a stored match is never refetched, and a `""` team id would make strangers teammates (party detection, encounters) and a lost winner entry would be a draw for good. Soft fields (map, season, queue, names, card, agent, team id, `State`, `IsOwner`, ...) default to empty. A default that would be wrong data does not default: a competitive-update row needs its id, start time, tier after, RR after and RR earned (without one the answer fails: no 1970 match, unranked tier or "+0 RR"); a history entry needs its start time; a season record whose `NumberOfWins` (with placements) or `NumberOfGames` is missing is unknown as a whole (0 games, which the card shows as `—`), never "0 wins of N games" (both are `Option`); `TierBeforeUpdate` is optional (`CompUpdateRow.tier_before` was already optional); a `SeasonalInfo` without `RankedRating` gives no current rank from MMR (`RankedRating` is `Option`), so the stored competitive updates answer (R154), never "0 RR"; a season entry keeps its map key as the act id (the entry's own `SeasonID` is not read); a `null` season or queue entry is dropped; a player's `stats` block with a hole is no stats; match-details keeps its required match id, start, queue and `players` list. **Fail closed for identities:** a roster entry whose `PlayerIdentity`, or `Incognito` inside it, is missing, `null` or not a bool is hidden (no name is ever asked for it), and a level that did not arrive is not shown (hide flag set, level 0). A core-game player without a `TeamID` is never an ally of another team-less player (only the user is placed by themselves). (2) **Token refresh like Scout.** The cached token is re-fetched from the local `/entitlements/v1/token` (a) once per call when a remote call answers 401, 403 or 400 with `errorCode: "BAD_CLAIMS"` (Riot's answer for an expired RSO token), then the call is repeated, never in a loop; (b) before a call when the cached token is older than 50 minutes (`TOKEN_MAX_AGE`; Riot tokens last about an hour). One refresh at a time: a generation counter plus a gate, so eight calls rejected together cost one fetch and the others just retry with the new token. A failed proactive refresh lets the call go on with the cached token and waits a minute before the next proactive try; a failed reactive one is the call's error, as before. Any other 400 is not a token problem and is not retried. (3) **name-service** (`RiotSession::names`): one more try after 250 ms for a 5xx or a transport error, same body (`VisiblePuuid`s only: the privacy gate is the argument type, not the loop); a 4xx, 429 and `Held` are not retried here.
- **Why:** Scout decodes everything leniently and re-reads the token every poll; ours failed a whole roster, history or rank answer on one missing field and kept one token per connect, so the first call after about an hour could be the one that learns the token expired (and Riot says that with a 400, which the old 401/403 rule ignored).
- **Cost:** An element dropped by the lenient decoder is logged once per run at debug (count only). A history or comp-update answer with one half-empty entry fails whole instead of being shortened; a match-details answer without a team id or a winner is retried later (the backfill's existing failure counting skips it after 3 runs). A failing name-service call now sends two requests (250 ms apart) instead of one; the caller's own retry rules are unchanged. One extra local call about every 50 minutes of use. `ConnectOverrides` gained a hidden `token_max_age` test knob.
- **Status:** active

### R167 — Names refresh the card on screen, rosters' matches queue names, My Stats rank falls back, an honest `VisiblePuuid` door

- **What:** Ruling 2026-10-01 (controller, the parked v0.2.1 items of Tasks A and B), all four checked against the code first. (1) When a names fill stores names, the cards of those players who are on the roster right now are rebuilt from the DB and published **once for the whole fill** (`AppState::reload_slots`), so a hidden Lobby party member (R153) shows the remembered name at once instead of at the next roster change. A fill that names nobody on screen publishes nothing. (2) The matches phase of a roster player's sync (R165) queues the players of the matches it just stored whose names match-details blanked, one ask per puuid with its newest such match (queued as each match is stored, inside the match-details join loop, so a client that goes away mid-call loses none), the way the backfill queues its matches' players. **Two priorities (fix round 1):** match-end, Player/Match Details, backfill and put-back asks are high and go first (`queue_name_fill`; when the 300-entry queue is full they evict the oldest low entries); a roster matches-phase ask is low (`queue_name_fill_low`), behind all high ones, never evicts, and the low asks hold at most 100 places (`NAME_FILL_LOW_MAX`), so a mid-match burst of strangers' histories cannot crowd out the just-ended match (R153). A high ask for a puuid queued low promotes it. Queued at any stage, sent between matches only (`take_fill` and the `fill_visible` check); `Repo::names_to_ask` still drops the known and the already-asked. (3) My Stats' rank card uses the live card's fallback (R154) when no MMR snapshot of the user is stored: the newest current-act `comp_updates` row (tier and exact RR), else the tier of the newest stored current-act competitive match (`rr` is `null`, `rrToNext` is `null`). A stored snapshot is the authority, "no rank this act" included. No request: one indexed read (`Repo::newest_ranked_match_tier`) only without a snapshot, and no `live_tier` (the user's own roster tier is not read). The chain is `model::rank_without_mmr`, now `pub` and taking `Option<&[CompUpdateRow]>`. `MyStatsInput` gains `has_mmr` and `stored_tier`. (4) The names fill builds its `VisiblePuuid`s with `VisiblePuuid::between_matches(puuid)` (no flag), reachable only through `LiveModel::fill_visible`, which returns nothing while Agent Select, In Game or a free-for-all match is on; `VisiblePuuid::new(puuid, incognito)` stays the live-stage door. `lacks_stored_name` has a direct unit test.
- **Why:** The first (A m4) and second (A concern) left a hidden player's name missing on screen, or never asked for, after a stored match; the third (B minor) left My Stats' rank card blank for a user whose `mmr` call failed while the live card showed the rank; the fourth (A m2) was a false claim in the code: `new(_, false)` on the fill's door said "visible" for players who were hidden.
- **Cost:** One DB read per roster player named by a fill, and one more publish per such fill. Queued asks: the low ones are capped at 100 and the whole queue at 300; the fill stays one request per 100. A dropped fill step puts its asks back as high priority. My Stats' rank card shows `rr: null` for the tier-only fallback; **the My Stats screen is not built yet, so its UI must render `—` for a null RR (the live card already does)**.
- **Status:** active (amends R153's "shown at the next sighting" and R154, which now also covers My Stats; supersedes the `(puuid, false)` wording of R16's later notes)

### R168 — v0.3.0 scope: fixes plus the My Stats page, released on `main`

- **What:** User decision 2026-10-03. Asked for a plan of fixes, development and improvements, then a new version to use. Answers: the new version is fixes plus the My Stats page (v0.3.0); the 2026-10-01 build (`44ae56d`) was tested and works; the release goes to `main` as before. The plan: robustness fixes (poll-loop supervisor, poison-tolerant locks in `opd1-riot`, Player Details retry after a failed match-details download, the names fill re-checks presence before it sends), the own competitive-updates backfill (R72), a `"mystats"` push event, the My Stats page, a Playwright CI job (R149's parked item), then the v0.3.0 release.
- **Why:** The user's words (`docs/requirements-raw.md`, 2026-10-03).
- **Cost:** None recorded.
- **Status:** active

### R169 — The poll loop restarts after a panic

- **What:** Claude's call (v0.3.0 plan, A1). `poll::start` runs the poller under a generic supervisor (`supervise(make, pacing, on_panic)`, `tokio::spawn`). A panic is logged at error level, the worker claim is revoked, the backfill runs and the session are dropped, and a fresh `Poller` starts after 1 s, then 5 s, then every 30 s while it keeps panicking; a poller that ran at least 60 s starts the backoff over. A poller that returns normally is not restarted. Review round 1: dropping a `Poller` aborts its worker task, and after a panic the supervisor shows the Closed view at once (`show_closed`, shared with `enter_closed`) and publishes the user's own card, so no stale cards stay up during the wait; `start()` goes through `spawn_supervised`. Review round 2: the supervisor awaits the Closed view before its backoff sleep, so it can never land after the new poller's roster, and `LiveState.settled` (R175) stays true across a restart.
- **Why:** The backend sweep of 2026-10-03: the loop was spawned once with nothing watching it, so one panic in the loop (or in core/riot code it calls directly) stopped polling for the session while the UI kept showing old data.
- **Cost:** A crash loop costs one reconnect every 30 s at most.
- **Status:** active

### R170 — `opd1-riot` takes its locks poison-tolerantly

- **What:** Claude's call (A2). Every std `Mutex`/`RwLock` in `opd1-riot` (session, throttle, assets, raw capture) is taken through `crates/riot/src/locks.rs` (`locked`, `read_lock`, `write_lock`: `unwrap_or_else(PoisonError::into_inner)`), as `src-tauri` already did (`state.rs` `lock()`).
- **Why:** With `.lock().unwrap()`, one panic while a session or throttle lock was held made every later Riot call panic, which reached the poll loop.
- **Cost:** None.
- **Status:** active

### R171 — Player Details retries after a failed match-details download

- **What:** Claude's call (A3, the leftover of R165 finding 8). `sync_details` reads `DetailsFetch.incomplete`: a match-details download that failed for real makes the Details sync `Failed`, so the page gives its 120 s claim back and the existing retry runs. A hold stays `Held`; a failure outranks a hold. `fetch_missing_details_checked` is gone. Review round 1: a download that Riot answered with a 429 counts as held for the rest of the `other` hold (`DetailsFetch.rate_limited`, from `RiotSession::other_hold_remaining`), so R160's automatic rerun after the hold (3 at most) still happens; only other failures are `Failed`. The live matches phase keeps R165: there a sent 429 is a real failure.
- **Why:** The helper dropped the flag, so a failed download reported `Done` and the page kept its claim with matches missing.
- **Cost:** None beyond the existing retry's requests.
- **Status:** active

### R172 — The names fill re-reads the presence before it sends; finding 9 closed

- **What:** Claude's call (A4, R165 finding 9). Right before a names fill request goes out, the user's own local presence is read again (one local call, only when a request is about to be sent; the lookup is shared with the poll loop as `roster::own_valorant_private`). If it says Agent Select, In Game or a free-for-all match, nothing is sent: the asks stay queued as high priority, nobody is marked asked, and the fill stands down for 3 s so the worker does not spin. Review round 1: the stand-down is a fill-only instant (`fill_stand_down_until`, checked in `fill_ready()`), so the live roster's names step never waits on it; the poll tick's `names_wakeup_due` wakes the fill when it ends. A failed presence read keeps the old behaviour (send). The other half of finding 9 (skip a puuid hidden on the current Lobby roster) is closed with no change: asking a party member still hidden after the match is R153, the user's decision, and the Lobby is between matches.
- **Why:** The fill could go out in the ~2 s between a match starting and the poll loop seeing it.
- **Cost:** One local call per fill request.
- **Status:** active (closes R165's open finding 9)

### R173 — The user's own competitive updates are backfilled for the act

- **What:** Claude's call (B1), building what R72 deferred. A lowest-priority worker job for the user's own account pages `competitiveupdates` (`queue=competitive`, 20 per page, newest first) and stores every row of each page in `comp_updates` (no schema change; the primary key dedups). A run stops at the first of: a page with a row older than the current act's start minus `ACT_EDGE_MS` (one day; My Stats takes rows that close to the act by Riot's season id) (R162), a page holding the anchor's match id or a row older than the anchor's start, a short or empty page (the endpoint has no total), or 10 pages (Claude's lightness bound). The anchor is `self_comp_anchor:<puuid>` in `app_state`, the match backfill's JSON shape, written only by a run that finished. It is triggered with the match backfill on connect and after each match end (`sync::start_own_backfills`), runs one page per worker turn with at least 2.5 s between pages (`COMP_BACKFILL_PAGE_GAP`: the run is not ready until then, the poll tick wakes it), never during Agent Select, In Game or a free-for-all match, and pauses on an `mmr` hold (`Held`, nothing sent); a real failure, a sent 429 included (R160), ends the run until the next trigger. Page 0 is skipped when the user's newest rows from the last own `comp_updates` call (`LiveModel::own_comp_head`) already contain the anchor's match id; the anchor then moves to the newest of those rows, and a match that ended in between is picked up by the next run. Review round 2: a new match end drops `own_comp_head`, so a head from before the match can never skip page 0.
- **Why:** My Stats' RR chart, net RR and rank changes only saw the last 5/20 competitive updates the live sync fetches.
- **Cost:** At most 10 `mmr`-family requests on the first run of an act, at least 2.5 s apart, and usually none or one page afterwards, all behind the live cards.
- **Status:** active (R72 done)

### R174 — A `"mystats"` event pushes My Stats when the user's data changes

- **What:** Claude's call (B2). Rust emits `"mystats"` (payload: the `MyStats` the `my_stats` command returns, never null) after the user's own rank or matches step, a match-backfill pass that stored matches, a competitive-updates page that stored rows, the user's own Details sync, an act change, and when the asset tables load. Recomputes are coalesced to one task at a time, run off the presence tick, and are emitted only when the serialized bytes differ from the last emission. The UI attaches the listener before its first `my_stats` call, and an event that lands while that call is pending wins over its answer. Review round 1: the push task's guard clears its running flag only when the task dies (no second concurrent recompute); a match backfill notifies right after each stored match, so an aborted pass still pushes; an account switch notifies at once, a read that ends after a switch is dropped and re-read, and the emitted bytes are recorded only after the emit. `self_puuid` is read from the DB at startup (`build_state`), so My Stats is available while VALORANT is closed. Review round 2: the push task waits 750 ms before each recompute, so a burst (a first backfill stores up to 200 matches) becomes a few pushes, not hundreds; an account switch skips the wait (`notify_now`). A 15-minute tick, started once in setup and never on the presence tick, recomputes too, so a session past its 6 h gap and the last-7-days insight age out on screen (the byte dedup makes an unchanged tick free). `MyStats` carries the user's own `puuid` (R175).
- **Why:** Rust pushes, the UI never polls (CLAUDE.md stack rule).
- **Cost:** One DB-only recompute per trigger (a few per match) and one every 15 minutes.
- **Status:** active

### R175 — The My Stats page

- **What:** Claude's design call (CLAUDE.md: a screen the design file doesn't cover is built only from the design's own parts and tokens; the design file has no My Stats screen). One scrolling page in the Player Details vocabulary (14px/600 section titles, `--card` panels, the Details table rows and result chips): the rank card (tier, RR or `— RR`, W / L, a bar to the next tier with its RR in the tooltip, Net RR, average win and loss, the last 4 rank changes) beside the RR chart (inline SVG, one accent line, evenly spaced by match, tier names in the gutter; above the Immortal floor 3 to 4 `N RR` gridlines; one hover overlay picks the nearest point, its tip names the RR change, tier, RR and date), Session (net RR, W / L, start, the session's results as chips), Recent Matches (the Details row plus ACS and RR; a row opens Match Details and Back returns to My Stats at the same scroll position), the Maps and Agents tables, win rate by weekday and by time of day, and Insights (green dot for a strength, neutral for a weak spot). Scopes live in the section header tooltips only; every `—` is the dim `--text2`; skeletons while the first answer is pending, frames with `—` when Rust has no data. Navigation: `My Stats` is a real tab; a Match opened from it sits under it. Start page: the first LiveState with `settled: true` (a new wire field: false until the poll loop's first verdict on whether VALORANT runs, then true for the app's life) picks it once: closed → My Stats, anything else → Live, and a user who already navigated is not moved; Agent Select still forces Live from anywhere. `MyStats` carries the user's `puuid`, so Recent Matches rows open even before the user's live card exists. Back from a Match restores the scroll position of My Stats and of Player Details. Full spec: `docs/design-spec.md` §9.
- **Why:** User decision R168 (fixes plus the My Stats page). The `settled` field comes from the UI review: Rust publishes Closed before it has looked at the game, so a first-state rule alone would open My Stats for a user whose game was already in the lobby.
- **Cost:** About 26 kB of JS (8 kB gzipped). The design calls are [C] in the spec, and the user sees them in the v0.3.0 test.
- **Status:** active (ends R71's start-tab call; supersedes R77's inert tab)

### R176 — Playwright runs in CI on Linux

- **What:** Claude's call, R149's parked item. The `Build` workflow has a second job, `ui-e2e` on `ubuntu-latest`, that runs the whole Playwright suite against the DEV demo scenes (Chromium, 1000×800): layout fit in all three fonts, tooltips, navigation. It does not gate the Windows installer job. The two specs that compare with the design file skip without `.superpowers/`. It caches the Playwright browsers and the test image cache, has a 30-minute limit, and uploads `ui/test-results` and `ui/tests/shots` when it fails.
- **Why:** The fit checks ran only on Claude's machine; a layout regression could reach the installer unseen.
- **Cost:** A few CI minutes per push to `main`.
- **Status:** superseded by R178 (the job became `checks`, which runs on every branch)

### R177 — v0.4.0 scope: the brief's fixes, hardening, CI and code health, plus five features the user picked

- **What:** User decision 2026-10-04. The v0.3.0 test went fine ("everything fine"). Continue with the brief (`docs/next-session-prompt.md`): fixes, hardening, build and distribution, code health, then features, each feature asked before it is built. Picked features: a richer encounter tooltip (E6), a Match History page (E2), a Settings page (E1), a quick hide hotkey Ctrl+Shift+H (E8) and private player notes (E4); not picked: My Stats for past acts (E3), the team average rank In Game (E5), the remembered window position (E7). Settings is a fourth tab (`Live · My Stats · History · Settings`). The Settings choices survive an update (stored with the DB), an exception to the installer's "settings are removed" rule. The release is v0.4.0, on `main` as before, with a download link.
- **Why:** The user's words (`docs/requirements-raw.md`, 2026-10-04).
- **Cost:** None recorded.
- **Status:** active

### R178 — Linux `checks` on every branch; the Windows installer only on `main`

- **What:** Claude's call (brief C3, C4). The `Build` workflow runs on a push to any branch, on pull requests and on a manual run. Its Linux job `checks` (replacing R176's `ui-e2e`) runs everything that does not need Windows, inside the pinned `mcr.microsoft.com/playwright:v1.63.0-jammy` image: the UI build, the dist guard, the gzipped JS and CSS sizes in the run summary (C6), `cargo fmt --check`, clippy `-D warnings`, the Rust tests, Vitest, svelte-check and Playwright. A guard step fails the job when `@playwright/test` and the image tag drift apart, so a dependency bump must move the image too. The Windows `build` job runs only for a push to `main` and a manual run, keeps the Rust tests (Windows coverage), drops Vitest and svelte-check (now in `checks`), and writes the installer size to the summary. The jobs do not wait for each other. A newer push cancels a branch's older run; `main` is never cancelled.
- **Why:** Lint and the UI suite only ran on `main` (or on Claude's machine), so a branch could carry a red check for hours; Windows minutes cost double in a private repo, so the installer is built only where it is shipped from. The pinned image gives every run the same browser and fonts, which the visual baselines need.
- **Cost:** One Linux job per branch push (about 15 minutes cold, less with the caches). A branch that touches Windows-only code is checked on Windows by a manual run of `Build` before `main` moves.
- **Status:** active (supersedes R176's `ui-e2e` job)

### R179 — Tests that do not need the sync worker never start it

- **What:** Claude's call (the red CI run 43 on `main`, 2026-10-04). `pipeline_tests::details_loadouts::a_hidden_player_who_left_the_screen_keeps_the_card_rule_in_details` failed once on Windows (expected `Hidden Player`, got the remembered `SecretName`) and passed on the same code in run 44. Root cause: its helper applied a roster through the real poll path, whose `ensure_worker` spawns the sync worker on the app's multi-thread runtime; the worker stored a match naming the hidden player before the test's `stop_worker()` aborted it, and an abort cannot undo a DB write already started. A `#[cfg(test)]` flag on `Poller` (`hold_worker`, set by `Poller::held`) makes `ensure_worker` claim and spawn nothing. Every test that applies or fetches a roster and does not look at the worker's output uses it (24 sites in `poll.rs`, `details_loadouts.rs`, `live_tier.rs`); tests that need the worker are unchanged. The 120 s delayed-500 rank mocks of `live_tier.rs` that only parked a racing worker are gone, and that file's "before any rank call" test now asserts that the mock saw no `/mmr/` request. A proving test (`a_held_poller_claims_no_worker_for_a_roster_that_needs_syncing`) fails on the old code every time.
- **Why:** "Flake" is not a root cause; a race in the test made CI red at random.
- **Cost:** None in the app: the flag and its check exist only in test builds.
- **Status:** active

### R180 — GitHub Releases from a workflow, a versioned installer, audits, visual-baseline plumbing

- **What:** Claude's call (brief C1, C2, C5, B10's CI half). (1) The Windows build moved into a reusable `installer.yml` (read-only token, no persisted git credentials) that renames the NSIS output to `OPD1-TRCKER-<version>-setup.exe`, checks the 10 MB limit on that file and uploads the artifact `OPD1-TRCKER-<version>-setup`; `Build` calls it for `main` and manual runs. (2) `release.yml` (manual: `version`, optional `ref`) publishes a GitHub Release: a `preflight` job resolves `ref` to a full SHA (a short SHA works), checks the version in that commit's `tauri.conf.json`, that the tag and any release or draft for it do not exist, and that `CHANGELOG.md` on the default branch has the version's section; then the installer is built and a `release` job (the only one with `contents: write`) runs `gh release create v<version> --target <sha>` with the installer and the section as notes. The tag is created through the API, so no git tag push is needed; a rebuild of an older commit gets `--latest=false`. (3) `audit.yml` runs `cargo audit` and `npm audit --omit=dev` monthly and on demand; a finding fails the run, which is how the owner hears of it. (4) For the visual baselines: `checks` sets `VISUAL_BASELINES=1`; a manual `Build` run with `update_baselines` renders the baselines in the pinned image (read-only) and a second job with write access, which runs no npm and no browser, commits them to the branch. (5) Every checkout that does not need git afterwards has `persist-credentials: false`; workflow inputs reach shell steps only through environment variables, validated before they are echoed.
- **Why:** Actions artifacts expire after about 90 days and the session cannot push tags; a Release keeps each installer. A version in the file name tells the user which build they have. The audits catch a vulnerable dependency without anyone looking. The write token never meets third-party code.
- **Cost:** A release builds the installer once more on Windows (about 10 minutes, Windows minutes count double in a private repo). The rebuilt v0.2.0 and v0.3.0 installers come from the same commits but are not the original files.
- **Status:** active

### R181 — The map list picture is stored at 384 px wide

- **What:** Claude's call (brief B9). The map `listViewIcon` (`AssetKind::MapList`, the row and top-bar map picture) is downscaled to 384 px wide on its first download, aspect kept; CSS still cover-crops it. A file already in the image cache is served as it is (the installer wipes that cache on an upgrade). Every kind's target is pinned by a test: agent 128, tier 128, `largeArt` 160, splash 1000, skin render 256, content tier 32, map list 384; `smallArt`, `wideArt` and `splash` are not shown anywhere and are left alone.
- **Why:** A decoded image costs width × height × 4 bytes of WebView RAM, whatever its file size. The real `listViewIcon` is 456×100 for most maps and 3840×2160 (33 MB decoded) for Basic Training and The Range. Its tallest use, the Match Details header's 150×56 box, needs 84 device px of height at 150% Windows scaling, which 384 px wide keeps exactly, so nothing looks different.
- **Cost:** One resize per map on first download. Savings: about 53 KB per map picture on the Live page, about 0.4 MB on a Details page with 8 maps, and about 33 MB for each of the two oversized maps. The Live In Game page's decoded images total about 5.2 MB, almost half of it the ten card arts (160 px wide, needed at that size by the Details hero).
- **Status:** active

### R182 — UI failures reach the log; the log starts with a header line

- **What:** Claude's call (brief B1, B2). (1) A crashed page, a `window` error and an unhandled promise rejection are reported by `ui/src/lib/uilog.ts` (installed before any other module) through a new command `ui_log(level, message)`: one line `kind: Name: message @ path:line:col`, at most 500 characters, never an object's field values (an object becomes its `message` or `[object Object] [key names]`), a repeat within 10 s dropped, at most 20 sends per 10 s, only inside Tauri, never throwing; the console sinks are dev-only and compiled out. Rust logs the line under the `ui` module (target `opd1_trcker::ui`), after `raw_capture::log_excerpt` (secret keys and JWTs redacted, one line, every full uuid shortened to 8 characters, 500 characters at most) and a process-wide limiter of 20 ring lines per rolling minute (the `N lines dropped` notice included). The command takes no app state, so an error before `setup()` is logged too. (2) `copy_log` (Ctrl+Shift+L, now run off the UI thread) and the dump's `log.txt` start with one line: `OPD1 TRCKER <version> · Windows <major.minor.build> · stage <s> · settled <b> · shard <pd>/<glz region>/<glz shard> · client <version> (<source>) · act <first 8 of the id> <label> (<riot|valorant-api>)`, `—` for an unknown part. `RiotSession` keeps the parsed shards; the client version and its source are read together; `AppState::act_source()` says which source chose the act; the Windows build comes from `windows-version` (already in the dependency tree; Windows builds only).
- **Why:** UI failures were invisible in a release build. The header answers the first questions about a user's log (which build, which Windows, which stage, which shard and act) without a round trip.
- **Cost:** No request. One IPC call per reported UI failure, bounded on both sides. Rust cannot recognise a player's name in free text; the UI is that barrier.
- **Status:** active

### R183 — Each Session chip has a tooltip and opens its match

- **What:** Claude's design call (brief B5; amends §9.3 and §9.6 of the build spec, where the chips had no tooltip). `SessionSummary.streak` (results only) became `matches`: per match its id, map (a valorant-api uuid, or `null`, never `""` or a Riot path), RR change (`null` when no competitive update is stored, never 0) and start, oldest first. Each chip keeps its plain 20×20 look and gets the card's last-5 tooltip with the map in front, `Ascent · +18 RR · 2h` (the map dropped while unknown; `Win` / `Loss` / `Draw` in place of an unknown RR). It opens Match Details exactly as a Recent Matches row does (the user as focus player, Back returns to My Stats at its scroll position): `role="button"`, Enter / Space, the global focus ring at its 2px outside offset (R106), an `aria-label` with the result word in front of the tip, no hover colour. The golden contract carries a draw chip with no map and no RR.
- **Why:** The v0.3.0 review left the chips as the only My Stats figures with nothing behind them; the data was already in the DB.
- **Cost:** A few bytes per session match in the `mystats` payload. A long session adds one tab stop per match.
- **Status:** active

## User decisions index

Each row is a dated block of `docs/requirements-raw.md`, in file order. That file is the source of truth for the user's wishes; never reword it, only append supersession markers.

| Date | Heading in `docs/requirements-raw.md` | Summary |
|---|---|---|
| 2026-09-27 | `متطلبات التراكر — نص المستخدم الأصلي (2026-09-27)` | The user's original text: fixed 1000×800 window; the player card fields; Details with Back, Recent Matches, Top 3 Teammates and Loadout; what differs per stage; hidden names hide only the Riot ID and tag; images by UUID from valorant-api. |
| 2026-09-27 (undated heading) | `قرارات سابقة من المحادثة` | Tauri 2 + Rust + Svelte 5, no Python; Setup.exe (NSIS) built by GitHub Actions; no instalock, no dodge, no revealing hidden names during a match; the user tests on Windows. |
| 2026-09-27 | `قرارات جولة الأسئلة 1` | Both teams' ranks In Game, and Agent Select shows my team only; match end returns to Lobby; English UI, always on top; demo button is temporary; Peak shows time since its act ended; My Stats depth as an extra; the name `OPD1 TRCKER`; local storage; the In Game sketch. |
| 2026-09-27 | `قرارات جولة الأسئلة 2` | No status text or badges, skeleton loading; calm dark theme; close quits, no autostart; Live and My Stats tabs with auto-switch to Live; no scroll in In Game and Agent Select; the eight My Stats sections; smart permanent SQLite storage; automatic party detection (likely, then confirmed). |
| 2026-09-27 | `قرارات جولة الأسئلة 3` | Closed game opens My Stats with saved data (temporary until My Stats is built, see the marker); party detection includes Deathmatch; the user's own party gets no marks; a hidden player's name may show if learned from a finished match the user played; Top 3 Teammates window. |
| 2026-09-27 | `قرارات جولة الأسئلة 4` | Private repo only, no auto-updater; the user sends Setup.exe by hand; the installer wipes program files, settings and image cache but never the DB, which migrates automatically. |
| 2026-09-27 | `قرارات بعد الخطة` | The user tests on Windows only at the end (Task 11; cancelled 2026-09-29, see the marker); subagent-driven execution; a shared `partyId` confirms a party from one match. |
| 2026-09-27 to 2026-09-28 | `قرارات أثناء التنفيذ` | GitHub Actions works; the user must approve the design before the screens; Claude orchestrates and agents implement; the Peak tip shows the full act name and dates; design v1 rejected; last 5 shows W/L/draw only with RR and time on hover; design rounds 2 and 3 not liked; new direction "easy on the eye" (one system for type, numbers, spacing); party-detection details; a trial round on two screens; three refinement passes; obsolete files archived to `ارشيف/`. |
| 2026-09-28 | `ملاحظات المستخدم على تصميم v7.1` | Nine points: the card is crowded, secondary text is too faint, each statistic's scope must be clear (shown without hovering: superseded, see the marker), long names truncate but the tag never does, one meaning per indicator, last 5 must be clearer, Peak names the tier, shared-match results read `Your result`, hidden means the name only. |
| 2026-09-28 | `ملاحظات المستخدم على v8` | Remove the printed scope headings (tooltip only); numbers and last-5 results look off; the palette looks AI-made; the user said the design pleases them (to confirm). |
| 2026-09-29 | `التصميم المعتمد + قرارات 2026-09-29` | Claude Design v8 is the approved design and outranks older visual rules; build scope is Live (one page), Player Details and Match Details; the six options ship at their defaults; smart party detection unchanged; migration 1 could be amended (R31, later frozen by R89); "Delete app data" in the uninstaller may wipe the DB; last 5 newest on the right; commit and push every step; the standard repo files. Claude's calls (R71, My Stats maths) and the user's answers (`Lv —`, lock-only, own-history backfill, Lobby card size) are on the last lines, and the final line lists the design calls R74 to R86. Later 2026-09-29 line: Player Details hero K/D and HS% from the last 20 act matches (R116). Later 2026-09-30 lines: names Riot returns for matches the user did not play may show off the live screen (R143); a hidden player's name shows in the remembered style from any source the DB knows (R145); name-service may be asked off the live stages (R150); names are captured after each match because the game's own Match History shows them; Encounters count competitive matches only (R156); friends' names come like everyone else's (R157, withdrawn); when the rank request fails the rank shows from the player's matches (R154); build the requests the way Scout does (R159). Later line, 2026-10-01: after testing, "there is still a delay or a bug in the requests"; holds per family, full Retry-After, mmr never blocks (R160). Later line, 2026-10-03: v0.3.0 = fixes plus My Stats (R168). Later line, 2026-10-04: v0.3.0 tested fine; v0.4.0 = the brief plus five picked features (encounter tooltip, Match History, Settings as a fourth tab whose choices survive updates, hide hotkey, notes) (R177). |
