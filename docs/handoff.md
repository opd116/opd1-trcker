# Handoff: OPD1 TRCKER, session 2 → session 3 (2026-09-27)

**Next session's job, in order:**
1. **Finish the backend:** Task 5 is closed. Next are Task 6, Task 9's Rust half (`mystats.rs` + the `my_stats` command), and Task 10.
2. **Restart the UI design from scratch.** Rounds v1, v2 and v3 were all rejected, and a v4 attempt was stopped before it produced anything. Get the user's approval, then redo Task 7's visuals, then Task 8, then Task 9's UI half.
3. **Task 11 is the user's Windows test.** Don't ask them to test before it.

Keep using `subagent-driven-development`. The user wants Claude as **orchestrator only**: subagents implement and review, and Claude decides, keeps the ledger and talks to the user.

## Build status (checked at handoff)
- **CI: green.**
  - GitHub Actions `Build` succeeded on the last pushed commit, `286ce71` (run 36336476484, windows-latest, 6m47s).
  - It produced the `OPD1-TRCKER-setup` artifact, ≈1.9 MB zipped.
  - The earlier `startup_failure` runs were an account setting, which the user has fixed.
- **Local: green at the last commit, `ca0b5fe`.**
  - `cargo test --workspace` gives 234 passed, 0 failed.
  - It ran in a clean worktree at `ca0b5fe`, with a separate target dir and a copy of `ui/dist`.
- **Not pushed:** 5 local commits on `main`, `209ade9..ca0b5fe`. The user has authorized pushing to the private repo (`git remote -v`).
- **Update after this file was first written:** Task 5b's fix round landed as `2a16055`, and its scoped re-review is clean (7 of 7 addressed).
  - The implementer reports `cargo test --workspace` at 245 passed, 0 failed.
  - `main` was then pushed to trigger CI. The result is recorded at the bottom of this file.

## Where the truth lives (read these, not repeated here)
| What | Path |
|---|---|
| **SDD ledger:** every task status, rulings R1–R48, deferred minors, design-gate history | `.superpowers/sdd/plan/progress.md` (git-ignored, on disk) |
| Task briefs, implementer reports, review packages | `.superpowers/sdd/plan/task-*-brief.md`, `task-*-report.md`, `review-*.diff` |
| Project rules (last-5 rule updated this session) | `CLAUDE.md` |
| The user's decisions, including the **new design direction** in the last two bullets | `docs/requirements-raw.md` |
| Plan | `docs/plan.md` (Task 7 Step 1 was edited by the user: `interface-design` sets direction, `impeccable` audits) |
| Rejected designs + the v3 brainstorm | `docs/design-history/{v1,v2,v3}/` (the user put these there) |
| SDD helper scripts | `~/.claude/skills/subagent-driven-development/scripts/{task-brief,review-package,sdd-workspace}` |

## Task status
| Task | State |
|---|---|
| 1 Scaffold + CI | done (`6e9655f..61b5989`) |
| 2 Parsers | done (`..ab47bd3`) |
| 3 Riot session / throttle / endpoints | done (`..78b47d7`) |
| 4 DB + stats | done (`..209ade9`) |
| 5a Card model, riot→core conversions, asset cache, raw capture | done (`..f745ee9`) |
| 5b Tauri poll loop, commands, `img` URI scheme, logging | **done** (`..2a16055`, fix round 1 re-review clean) |
| 6 Hidden names, party detection, encounters | not started (5a/5b left hooks, see rulings R9 and R18) |
| 7 Tokens + PlayerCard + demo feed | code done and reviewed on branch `ui-lane` (`ab47bd3..89d30b6`), but **the design was rejected** |
| 8, 9, 10 | not started |
| 11 | the user |

### Task 5b
Closed: fix round 1 re-review clean (`ca0b5fe..2a16055`). Its deferred minors are in the ledger.

## Worktrees and lanes
- `/home/project-1` on `main` is the **backend lane**.
- `/home/project-1/.worktrees/ui` on `ui-lane` is the **UI lane**.
  - It holds Task 7's reusable plumbing: `types.ts`, stores, the DEV-only demo feed, `format.ts`, `assets.ts`, tooltip action, `format`/`cardTips` tests.
  - Its visuals get replaced by the new design.
  - **Contract changes since Task 7 that the UI must adopt:**
    - R32: `last5[].result` is `"win" | "loss" | "draw"`.
    - R34: `name` is tri-state (absent while loading).
    - The last-5 row shows markers only, with RR and time in a tooltip.
    - The UI must call `invoke("live_state")` on start.
    - Ctrl+Shift+L runs `copy_log()` and writes the result with `navigator.clipboard`.
    - Ctrl+Shift+D runs `dump_raw()`.
  - Merge `main` into `ui-lane` before Task 8.
- `.worktrees/` is excluded through `.git/info/exclude`.

## Design restart: what the next session must know
- **Rejected:** v1 (Barlow condensed cards), v2 (A Night Drive / B Case File / C Sadu), v3 (A2, A3, B2, B3, A×B).
  - The user's complaints are recorded in the ledger and `requirements-raw.md`: stat fonts, cramped, dead space, repeated colours, last-5 shapes, rank confusion (current vs peak vs previous act), "not cohesive or professional".
  - They **liked** v3's colours (cabin black, off-white text, one amber accent) and its "shapes".
- **The user's newest words (paraphrased, from `requirements-raw.md`):**
  - They want something easy on the eyes.
  - The problem is not odd fonts as such: the type doesn't harmonize with the numbers and results, and the spacing and element placement aren't orderly.
  - They want **one consistent system for type, numbers, spacing and alignment**.
  - Claude should think like an engineer and UX expert, and report back when done.
- **Recommended method** (planned for v4, stopped before any output):
  1. Research real companion apps and VALORANT's own UI.
  2. Build the design system first: grid, a type scale of 5 sizes or fewer, colour roles, components.
  3. Build every screen only from that system: In Game, Agent Select, Lobby with 1/3/5 players, Closed, Details, My Stats.
  4. Run automated fit checks plus a written alignment/type/colour audit.
  5. Get an independent critic subagent's review, then fix.
  6. Only then show the user.
- **Hard constraints:**
  - An In Game card is about 480×125 (5 stacked per team column), and every card field is required.
  - Minimum 12px, tabular numerals, no status text.
  - Verify at exactly 1000×800 with Playwright Chromium (`~/.cache/ms-playwright/chromium-1243/chrome-linux64/chrome`; `playwright` is in the UI worktree's `node_modules`).
- **Claude Design canvas** (the user asked to use it): https://claude.ai/artifact/1BWA8jKBVEwzBSaYUTyD86. It is private to the user.
  - Artboards are `.dc.html` files.
  - Images and fonts must first be uploaded as assets (`Artifact publish asset:true, file_paths ≤25`) and referenced by the `/_blob/<id>` URL the upload returns. External URLs are not allowed.
  - Read `project/canvas.json` before republishing, and add new rounds as a new page or row.
  - Comment thread `a0db9069-d4c6-470c-a28a-1a4d8d23d564` (the user's "none of them, rethink") is still **open**. Resolve it after presenting the new design.
- **Design gate:** Task 8 does not start until the user approves the card and screens.

## Environment
- **Rust:** 1.98.1 via rustup (`source ~/.cargo/env`), plus the Tauri Linux deps.
- **Node:** 24.
- **`gh`:** logged in as the repo owner.
- **Reference repos** (read-only; the old scratchpad may be gone, so re-clone):
  - https://github.com/kryotrades/Valorant-Scout
  - https://github.com/techchrism/valorant-api-docs (authoritative field names are in `valorant-api-types/src/endpoints/**`)
- **Mock-only so far** (settle them from the user's `dump_raw` output in Task 11): core-game `Loadout.Subject`, presence `partySize`, Deathmatch `TeamID` shape, and the match-publish delay after a game.

## Non-obvious notes about the user
- They reply in short Gulf Arabic and are blunt.
- Show visuals, not jargon.
- They hate generic "AI" design.
- They accidentally stopped an agent once; if an agent is killed, ask before assuming intent.
- They want progress updates, but not "should I continue?" questions.
- At the end of the SDD run, list every `Ruling:` line from the ledger (R1–R48 and later) in the final message, per the skill's Finish section.

## Housekeeping for the start of the next session
- **Uncommitted docs from the user:** `docs/requirements-raw.md` (new design direction), `docs/plan.md` (Task 7 Step 1), `docs/design-history/` and this file.
  - Commit them in a separate `docs:` commit **after** Task 5b's fix commit lands, so it doesn't fall inside 5b's review range.

## Suggested skills (call via the Skill tool)
- **Always:**
  - `subagent-driven-development` (required driver)
  - `verification-before-completion`
  - `graft` (codebase context; the repo is indexed)
- **Backend (implementers):** `test-driven-development`, `tauri-v2`
- **Design restart:**
  - `impeccable`, `interface-design`, `ui-craft-dense-dashboard`, `game-ui-ux`
  - `ui-ux-pro-max`, `design-taste-frontend`, `frontend-design`, `web-design-guidelines`
  - `design:design-critique`, `design:design-system`
  - `svelte-code-writer` when implementing in Svelte
- **Explaining to the user:** `archify` (diagrams in English, explanation in Arabic).
- **`handoff`** is user-invoked only: ask the user to run `/handoff` at phase ends.

## CI result after the final push
- **Green.** GitHub Actions run 36346114001 succeeded on `7855f45`, which contains everything through Task 5b (`2a16055`) plus these docs.
- Every step passed on windows-latest: Rust tests, UI tests and the NSIS build.
- The `OPD1-TRCKER-setup` artifact is 4.2 MB zipped. It grew from 1.9 MB with the real Rust core (rustls/ring, rusqlite, image).
- Task 10 applies the release profile (size optimization). Only then check the ≤10 MB installer target.
- `.superpowers/design/` appeared during handoff and wasn't created by this session. It is probably the user's parallel design work. Leave it alone and don't commit it.
