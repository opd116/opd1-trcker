# Next session prompt (after v0.3.1, 2026-10-04)

Paste this into a new session. It replaces the v0.3.0 brief; the parts of that brief that were done are listed in `docs/handoff.md`.

---

هلا. نكمل على OPD1 TRCKER من بعد نسخة 0.3.1. اقرأ `docs/handoff.md` و`CLAUDE.md` أول شي، وبعدها كمّل الشغل المتبقي تحت بالترتيب. كلّمني بالعربي وباختصار. لما تخلص ويكون كل شي أخضر، ارفع على main وانشر النسخة عن طريق `release.yml` وعطني رابط التنزيل.

---

# Brief for Claude (English)

You are continuing **OPD1 TRCKER** (Tauri 2 + Rust core + Svelte 5 SPA + SQLite; the name is spelled `OPD1 TRCKER` on purpose). The last release is **v0.3.1** (`main`, GitHub Release `v0.3.1`). Read `docs/handoff.md` (state, environment, what is done), `CLAUDE.md`, and `docs/DECISIONS.md` R177–R188 first.

## Ground rules (unchanged, binding)

- Short Gulf Arabic to the user, no jargon, progress updates, never "should I continue?".
- Lightness first; hidden-name rules; no status text; min 12 px; fixed 1000×800, no scroll on Live; no client writes; English UI.
- Claude orchestrates; Sonnet implementers; a fresh reviewer per step, a fix round, a scoped re-review. Commit each step with explicit paths and push at once. Rulings from **R189** up. User decisions get a dated line in `docs/requirements-raw.md`.
- Every push runs the Linux `checks` job (fmt, clippy, all tests, Vitest, svelte-check, Playwright with the visual baselines). Run `cargo fmt --check` on what you commit: a hand edit once turned `checks` red.
- The visual baselines (`ui/tests/baselines/*.png`) change only on purpose: run `Build` manually with `update_baselines: true` on the branch, then pull.

## Remaining work, in order

1. **B8, idle cost** (was in progress, nothing committed): with VALORANT closed only the 5 s lockfile check and the 15-minute My Stats tick may run. Tests (paused tokio time, wiremock counts) for: no lockfile → no HTTP; client up, game closed → only the local presence call; the 15-minute tick → DB only. Fix: the tables retry backs off (30 s → 60 s → 120 s → 5 min, reset on success) in `state.rs` `ensure_tables`, and the UI `loadMeta` retry cap rises to 5 minutes. Plus B7's leftover: `PRAGMA analysis_limit=400; PRAGMA optimize` once on `RunEvent::Exit` (`lib.rs`: `.build(ctx)` + `.run(cb)`).
2. **A flaky test to root-cause**: `pipeline_tests::sync_order::a_task_that_waited_on_a_download_that_failed_still_counts_it_as_missing` (`sync_order.rs` ~686) failed once under `--test-threads=48`. Look at the details-fetch dedup (`claim_details_fetch`, `DetailsFetchGuard`, `wait_for_details_fetch`, `note_details_404`). Fix the real race (production or test), with a stress loop showing 0 failures.
3. **C5 rest**: patch/minor bumps of Tauri, Svelte and Vite only if the suite stays green and sizes do not grow without reason (use `npx npm@11` for lock changes; Playwright stays pinned to the image tag in `build.yml`). The `audit.yml` workflow exists (monthly).
4. **Missing release tags**: after `release.yml` is on `main`, dispatch it for `0.2.0` with `ref: a1e3315`, then `0.3.0` with `ref: 493d4f9` (it builds those commits again and tags them through the API).
5. **Phase D, code health** (pure refactors, identical test counts, one file per commit, nothing else in flight):
   - D1: split `src-tauri/src/state.rs`, `poll.rs`, `sync.rs` into child modules; struct definitions stay in the parent file; helpers used across children become `pub(super)`; keep public paths with re-exports. The planned seams are in `/root/.claude/plans/` of the last session (not in the repo); re-derive them: `state/{card,names_queue,jobs,backfill,acts,publish,open_pages,details_claims}`, `poll/{supervisor,connection,stage,roster_fetch,post_match}` + `poll/tests.rs`, `sync/{rank,matches,details,anchor,backfill,comp_backfill,worker,names,self_info}`.
   - D2: (a) a reconnect because presence keeps failing gets its own counter and backs off 5 → 60 s, cleared only by a good presence (today `drop_session` zeroes `connect_failures`); (b) the "double sync" minor: close it with a ruling and no change (a Details sync makes no `match_history` call and match-details downloads are already deduplicated).
   - D3: share the backfill anchor code (`sync/anchor.rs`, a `RunCursor` in both run structs).
   - D4: one shared row component/CSS for Details' Recent Matches and My Stats' Recent Matches (and the table/encounter rows where identical); the pixel-alignment test and the visual baselines must not change.
   - D5: ARCHITECTURE file map; a dated pointer section at the top of `docs/plan.md`.
6. **Phase E, the user's five picks** (each its own step with a reviewer and 1000×800 screenshots in 3 fonts shown to the user; the release then becomes **v0.4.0**):
   - E6: richer ⟲ tooltip: `N with · M against` plus `Last · {Map} · {Win|Loss|Draw} · {ago}` (DB only).
   - E2: a **History** tab: the user's stored matches, newest first, filters for queue, map and agent, a windowed list, a click opens Match Details (`back: 'history'`); command `match_history` (paged) + facets; DB only.
   - E1: a **Settings** tab (fourth tab: `Live · My Stats · History · Settings`): the six design options of `ui/src/lib/theme.ts`; saved in `app_state` (`ui_settings`) and **kept across updates** (the user's exception to the installer rule: update CLAUDE.md's installer and Settings lines in the same change).
   - E4: private notes per PUUID (a new migration, `player_notes`), an icon on the card with the note as its tooltip, edited in Player Details; never logged.
   - E8: Ctrl+Shift+H hides and shows the window (`tauri-plugin-global-shortcut`; show without stealing focus) plus `tauri-plugin-single-instance` so launching again brings a hidden window back; a CLAUDE.md Window line.
7. **Parked, do not start without the user**: the act-change work (`docs/parked/act-change.patch`, `docs/parked/README.md`) and R188 (the rank carries over into a new act). The user said on 2026-10-04 to postpone anything about the new act. After 2026-10-14, ask the user for a Ctrl+Shift+L log of the act change and whether to pick it up.

## Release

v0.4.0 if any Phase E feature ships, else v0.3.2. Bump `tauri.conf.json`, `src-tauri/Cargo.toml`, `package.json`, `ui/package.json`, the three fields of `package-lock.json` by hand, `cargo check -p opd1-trcker`. CHANGELOG from `git log`; README; `docs/test-checklist.md` in Gulf Arabic. Fast-forward `main`, wait for `checks` and `build` green, dispatch `release.yml` with the version, send the user the Release link, the install steps and the checklist, then update `docs/handoff.md`.
