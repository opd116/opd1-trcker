# Parked work

Work that was built and reviewed but held back on the user's word. Each entry says what it is, why it waits, and how to pick it up.

## `act-change.patch` (parked 2026-10-04)

- **What:** the act-change hardening (brief B6), applies on top of commit `fa6517d`:
  - one `AppState::note_act_change()` that republishes the cards, pushes My Stats and rebuilds an open Player Details once, whatever changed the current act (Riot's answer, valorant-api's date rule, a tables load);
  - both own-history backfills re-read the act start on every page;
  - `roster::resolve_acts` ends every earlier act where the next one begins, so an old act's peak never reads `This Act`;
  - a test-only clock seam (`AppState::clock_ms`, `advance_wall_clock`) and the pipeline tests for all of it.
- **State:** reviewed once with no must-fix item. The review round was half applied when it was parked: the exactly-once test still has to open a Details page, the act-id read has to move inside its lock, `ensure_tables` should call the reaction itself, the backfill bound should never loosen, and the six date-rule tests should pin the clock.
- **Not started:** R188, the user's answer that the rank carries over into a new act (`docs/requirements-raw.md`, 2026-10-04). The card and My Stats should show the carried rank (the newest stored competitive update of an earlier act, exact RR) instead of `—` until the first match of the new act.
- **Why it waits:** the user, 2026-10-04: «اذا بقى موضوع يخص الاكت الجديد اجله حاليا» (anything left about the new act: postpone it for now).
- **Pick up:** `git apply --3way docs/parked/act-change.patch` on a branch. The code has moved since (the file split of the big Rust files), so expect to re-apply by hand where a hunk no longer fits; the tests in the patch say what must hold.
