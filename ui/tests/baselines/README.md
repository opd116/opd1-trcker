# Visual baselines

These PNGs are the reference images for `tests/visual-baseline.spec.ts`: fourteen states of the v2 screens on the DEV demo feed (Live in game, Agent Select, waiting, the lobby in queue, the recap and glance mode; the profile drawer; Saved; History; a scoreboard; Competitive; Collection; ASCII; Settings), each one 1280×800 (glance mode 420×800) with the fixed page clock, compared with `toHaveScreenshot` (at most 0.2% of the pixels may differ).

Pixels depend on the exact browser build and its fonts, so they are generated only by the manual `Build` run with `update_baselines=true` (job `baselines-render`, in the pinned `mcr.microsoft.com/playwright` image that CI also compares them in) and committed by that run, never on a developer machine: locally the spec skips itself unless `VISUAL_BASELINES=1`, and a local render would not match CI.

**The v2 set has not been generated yet.** The v8 PNGs were deleted with the v2 redesign (R189), so start the `Build` workflow by hand on the branch once, with `update_baselines` ticked, and look at the PNGs in the commit it makes. Until then, and for any state added to the spec later, a state whose PNG is missing is skipped (not failed).

To refresh them on purpose after a change that is meant to alter the look, run the same workflow again, look at the new PNGs in its commit, and keep them only if the change is the one you intended.
