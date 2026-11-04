# Visual references

## Current: approved Compact design (R211)

`compact/` holds the active references for `tests/visual-baseline.spec.ts`: 16 states at both **700×860** and **680×760**, plus **420×420 Glance** (33 PNGs). This covers every retained screen, both profile encounter variants, Current/Peak Live, pregame, waiting, lobby, recap and scoreboard. The page clock and DPR 1 are fixed. The screenshot tolerance remains **0.2%**, unchanged.

The user explicitly approved the whole new Compact design on 2026-11-04 after choosing it over the historical v2 comparison. R211 authorizes establishing these new references; it does not authorize hiding functional, privacy, clipping, image-loading or pixel regressions. Views intentionally lacking raster art (ASCII, Settings, About) have content assertions instead of expecting the removed own-rank shell image.

References are rendered only by the manual `Build` workflow with `update_baselines=true`, using the same pinned **Playwright v1.63.0** image as CI. The renderer writes only `compact/`; the commit job stages the 33 explicit expected paths and refuses a moved source branch. Do not generate platform-dependent reference pixels on Windows. A normal run (`update_baselines=false`) must then verify the final commit; the reference-generation run alone is not passing CI.

Local runs skip pixel comparison unless `VISUAL_BASELINES=1`. In CI, **a missing required Compact reference fails rather than silently skipping**. Review rendered images and source provenance before accepting the resulting candidate.

## Historical: v2

The 14 root-level PNGs are the previous v2 design at 1280×800 (Glance 420×800). They remain untouched for historical comparison and no longer define acceptance for Compact. Do not restore the old interface to match them or overwrite them as part of the Compact migration.
