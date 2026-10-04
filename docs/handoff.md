# Handoff: OPD1 TRCKER after v0.3.1 (2026-10-04)

Written by hand at the end of the v0.3.1 session (the user's Claude limit was running out, so the round was cut short and shipped as v0.3.1). It replaces the v0.3.0 note.

**Next session's job:** follow `docs/next-session-prompt.md` (ready to paste). In short:
1. Finish B8 (idle cost, PRAGMA optimize on exit).
2. Root-cause one rare test flake.
3. Dependency bumps.
4. The missing v0.2.0 and v0.3.0 releases.
5. Phase D (file split and code health).
6. The user's five features (History, Settings, notes, the hide hotkey, a richer encounter tooltip), then v0.4.0.

The act-change work is parked by the user.

## Release status

- **`main` = the v0.3.1 release commit** (fast-forwarded from `claude/amazing-carson-gse33k`).
- **CI:**
  - `checks` (Linux, in the pinned Playwright image) runs on every push.
  - `build` (Windows installer) runs on `main` and on manual runs.
  - `release.yml` publishes a GitHub Release with `OPD1-TRCKER-<version>-setup.exe`.
- **The user's last word:** v0.3.0 tested "everything fine" (R177).

## What this session did (details in `docs/DECISIONS.md`)

| Ruling | What |
|---|---|
| R177 | The user's scope: the brief plus five picked features; Settings as a fourth tab whose choices survive updates |
| R178 | Linux `checks` on every branch, Windows installer only on `main` |
| R179 | Tests that don't need the sync worker never start it (the flaky CI test) |
| R180 | GitHub Releases from `release.yml`, a versioned installer name, monthly audits, the visual-baseline workflow |
| R181 | The map list picture is stored at 384 px wide |
| R182 | UI failures reach the log (`ui_log`); a header line on the copied log and the dump |
| R183 | Session chips show map, RR change and time, and open their match |
| R184 | Tooltips reach the keyboard; the RR chart is one keyboard stop |
| R185 | Request holds survive a new session |
| R186 | The My Stats push checks the account and emits under the model lock |
| R187 | Scale and plan tests for the DB; the encounter queries start from the other player |
| R188 | The rank carries over into a new act (user); then everything about the new act is **parked** |

Not done (in the next-session prompt):
- B8.
- The sync_order flake.
- The dependency bumps.
- The old tags.
- Phase D and Phase E.

## Where the truth lives

| What | Path |
|---|---|
| Project rules | `CLAUDE.md` |
| The user's words (newest lines 2026-10-04) | `docs/requirements-raw.md` |
| Every ruling, R1–R188 | `docs/DECISIONS.md` |
| UI build spec (§9 My Stats, §8.7 visual baselines) | `docs/design-spec.md` |
| How the parts fit | `docs/ARCHITECTURE.md` |
| Release notes | `CHANGELOG.md` `[0.3.1]` |
| The user's test list (Arabic) + controller checks | `docs/test-checklist.md` |
| Parked act-change work | `docs/parked/` |
| Ready-to-paste brief for the next session | `docs/next-session-prompt.md` |

## Open items

- **Still unverified on the user's PC** (checklist controller section):
  - the R99 lock value, the R100 level, pregame `Loadouts[].Subject`, the v3 `Guns[]` shape;
  - hidden names in match-details (R74, R94), the MMR 500 (R159), R173 paging;
  - the first content-service answer after 2026-10-14 (R162), whether MMR has the new act's entry before a first match (R188);
  - the RAM figure, and the new header line (R182).
- **The act change on 2026-10-14:**
  - Without the parked work, a player whose rank snapshot lacks the new act shows `—` for the current rank until they play.
  - The switch reaches the screen at the next publish or the 15-minute My Stats tick.
  - The user knows (checklist item 10).
- **Visual baselines:**
  - 7 PNGs in `ui/tests/baselines/`, rendered in CI.
  - Any intended visual change needs a `Build` run with `update_baselines: true` on the branch, then a pull.

## Environment notes (cloud container)

- **Tauri Linux deps:** run `apt-get update` first (the package lists go stale), then `apt-get install -y libwebkit2gtk-4.1-dev libsoup-3.0-dev librsvg2-dev build-essential pkg-config libssl-dev`, then `npm ci` and `npm --prefix ui run build` before any cargo command.
- **Playwright:** 1.63 expects Chromium build 1243 and `/opt/pw-browsers` has 1194. Never run `playwright install`. Workaround:
  1. `mkdir /opt/pw-browsers/chromium_headless_shell-1243`.
  2. Link `chrome-headless-shell-linux64` in it to `/opt/pw-browsers/chromium_headless_shell-1194/chrome-linux`.
  3. Inside that folder, link `chrome-headless-shell` → `headless_shell`.
  4. Touch `INSTALLATION_COMPLETE` and `DEPENDENCIES_VALIDATED`.
- **Port 4173:** Playwright's dev server uses it, so two agents running Playwright at once collide. Run them one at a time, or with a temporary config on another port.
- **Node:** the container has Node 22 / npm 10. Bump the three version fields in `package-lock.json` by hand (npm 10 rewrites the `libc` fields).
- **Missing here:** `graft` (MCP fails to connect) and `.superpowers/` (the design file), so the two design-compare specs skip.
- **Worktree agents:** branch from a stale ref. Have them `git fetch` and merge the working branch first; fast-forward or cherry-pick their commits back.
- **GitHub:** pushes by the `baselines-commit` job land on the branch without a new run. Pull (`git rebase --autostash`) before the next push.

## Notes about the user

- Short Gulf Arabic, blunt. Show pictures, not jargon.
- Wants progress updates, not "should I continue?" questions.
- Doesn't code; tests on Windows with VALORANT and sends the log or a dump when something is wrong.
- Their Claude limit can run out mid-round: keep the branch releasable at every step.
