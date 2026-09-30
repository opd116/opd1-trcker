# Decisions

Every ruling made while building OPD1 TRCKER, in number order (R1 to R125). Each one has what was decided, why, what it costs if it turns out wrong, and its current status.

- **Status** is one of `active`, `superseded by Rn`, `narrowed by Rn`, `reversed by the user (date)`, `dropped`, or `unused number`. A note in parentheses adds detail.
- Rulings come from the working ledger (R1 to R73 and R87 to R125) and from Phase 2 planning (R74 to R86, the gap-fills of `docs/design-spec.md`, which carry the tags [C] and [bound] there).
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
- **Status:** active

### R18 — Live hidden-name protection covers Top 3 Teammates

- **What:** A teammate whose puuid is incognito in the current live roster is shown by the display_name rules (remembered from a match the user played, or hidden) in Player Details, never with a name from another player's history.
- **Why:** Closes an indirect leak.
- **Cost:** A hidden duo partner shows as hidden in someone else's details.
- **Status:** active

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
- **Status:** active

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
- **Status:** partly reversed by the user (2026-09-29, R99): hover no longer counts as selected; its other three calls stay active

### R72 — Current-act backfill deferred

- **What:** The current-act backfill (My Stats undercounts because sync keeps only the last 5/20 competitive updates) is deferred to the My Stats UI phase.
- **Why:** My Stats is not in the 2026-09-29 build scope.
- **Cost:** None now, because My Stats is not shown yet.
- **Status:** active (deferred to the My Stats phase)

### R73 — Task 9a fix round

- **What:** Fix the 9a review findings M1 (placement games with 0 RR counted in the average win/loss and RR economy) and M2 (act membership: matches by valorant-api date versus RR rows by season id) on main before the Phase 2 lanes, plus cheap minors m2, m4, m7 and m8. Deferred: m1, m3, m5, m6. Done in `c9ad7f4`.
- **Why:** Main was idle while planning ran, and `mystats.rs` is not touched by Phase 2.
- **Cost:** Small: My Stats UI is not shipped yet.
- **Status:** active

### R74 — Match Details names

- **What:** Match Details names: in a match the user did not play, a row shows as its agent name (or the remembered name) when its puuid is incognito on the live screen now or its stored name is empty; otherwise Visible. This replaces audit §2.12's "flagged incognito in that match".
- **Why:** match-details has no incognito flag (`endpoints.rs` `MatchPlayer`), so §2.12 can't be implemented.
- **Cost:** If match-details returns real names for players who were incognito in that match, those names show for players not on screen now, and the same risk covers Top 3 names built from the details player's own history (R94). Task 19 item 12 checks this against the user's DB. The follow-up would hide every name in matches the user didn't play, and every Top 3 name learned only from such matches, except players visible on the live screen now.
- **Status:** active

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
- **Status:** active

### R78 — Tooltips the design leaves unbound

- **What:** Tooltips the design leaves unbound or lacks: K/D and HS% `Last 5 competitive`; the Win% structured tip with row 2 `This act`; hero Games and W / L `This act`; the loadout well `{Tier} Edition`; the act name in the Peak tip; the Peak tip on the whole group. Tips hide on click, navigation and scroll. Counts use singular forms.
- **Why:** Scope lives only in tooltips (R67), and CLAUDE.md puts extra detail behind hover.
- **Cost:** Extra tips.
- **Status:** active

### R79 — Type and asset calls

- **What:** Type and assets: weight 400 is dropped (hero `Prev —` at 500); ⟲ is an inline SVG (Lucide `rotate-ccw`, ISC notice in `THIRD-PARTY-NOTICES.md`); `cardLarge` is 160px wide (not the audit's ~128, because 160 covers the 109px hero at 150% DPI); `contentTier` 32px; `chroma` 256px.
- **Why:** Lightness. The Segoe glyph exists only on Windows.
- **Cost:** A few KB.
- **Status:** active

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
- **Status:** active

### R89 — Migration 2, migration 1 frozen, no self-heal

- **What:** The Phase 2 columns go in a versioned migration 2 (`ALTER TABLE ADD COLUMN` ×4). Migration 1 is frozen and there is no PRAGMA self-heal.
- **Why:** Satisfies CLAUDE.md's versioned-migration rule with one definition per column.
- **Cost:** None (no installs exist).
- **Status:** active

### R90 — Own loadout fetched once per connect

- **What:** Fetch the user's own loadout once per connect via pd `personalization/v2/players/{self}/playerloadout` (read-only, 1 call), so the user's own Details shows skins outside a match. Others keep R83 (`—` outside Agent Select and In Game).
- **Why:** The user's own Details should not be blank outside a match.
- **Cost:** 1 call per connect.
- **Status:** active

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
- **Status:** active

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
- **Status:** active

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

- **What:** The user delegated the call to Claude (choose what is best for the user experience), so this is a user-delegated change to the design (R92). The card's Prev row becomes icon · `Prev` · name (variant V4 in `.superpowers/sdd/plan/shots-prev/`): the greyed 18px prev icon is centred in the 40px rank-icon column, then comes the 12px `Prev` label, then the tier name in the rank-name column. With no prev tier: an empty icon column, then `Prev —`. The Peak row is unchanged (50px label), and so is the hero Prev row. Applied on `ui-lane` after the Task 17 review loop; the design-ref visual test accepts the Prev row as an approved deviation. Spec §3.6 (Prev) and the §8.4 geometry table record it.
- **Why:** The user found the 50px label gap untidy. V4 mirrors the rank block above it (icon column, then text column), puts the word next to its value, and matches the design's own Details hero Prev order (icon · Prev · name, v8:214-218).
- **Cost:** One CSS change.
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
| 2026-09-29 | `التصميم المعتمد + قرارات 2026-09-29` | Claude Design v8 is the approved design and outranks older visual rules; build scope is Live (one page), Player Details and Match Details; the six options ship at their defaults; smart party detection unchanged; migration 1 could be amended (R31, later frozen by R89); "Delete app data" in the uninstaller may wipe the DB; last 5 newest on the right; commit and push every step; the standard repo files. Claude's calls (R71, My Stats maths) and the user's answers (`Lv —`, lock-only, own-history backfill, Lobby card size) are on the last lines, and the final line lists the design calls R74 to R86. Later 2026-09-29 line: Player Details hero K/D and HS% from the last 20 act matches (R116). |
