# OPD1 TRCKER — redesign, 2026-10-15

This is the current visual specification under the user's explicit autonomous redesign request (R189). It replaces v8 geometry, not data, privacy or navigation contracts. The priority order is unchanged: lightness, then visual comfort.

## Visual system

- Existing local Barlow font; IBM Plex Sans and Archivo remain alternatives. No new font or runtime dependency.
- Neutral canvas `#0e1013`, surface `#171a1f`, raised `#20242a`, separators `#23272e`, body `#e4e1db`, secondary `#a3a7ae`. Accent `#e3a54f`; green/red reserved for results. The existing six theme options still drive CSS variables.
- Text floor 12px; tabular numerals. Current rank/identity dominate secondary ranks. Small square result tiles keep W/L/D visible independent of color. No blur, glow, shadow animation or running animation loop.
- Header 64px including border, 20px horizontal inset, 16px wordmark, 14px navigation. Active tab uses accent text and a 2px underline.

## Live

1000×800, no scrolling. Content padding 14px top / 16px sides and bottom. Two equal 476px columns, 16px gutter. A 20px team-label track and 10px gap precede five 127.2px cards, 10px apart. Labels are `Your team` / `Opponents` in game; `Your party` outside a match. Empty pregame slots stay text-free.

Each card has a 38px player-art strip at 0.5 opacity and a grid inset 48px left, 14px right, 8px vertically. Rows: 28px identity, remaining space for ranks, 32px performance, with 6px gaps.

1. Agent 28×28; full-width identity lane, reserved encounter slot, level at the right with 16px clearance for party tab. Only the Riot name may truncate; its tag stays whole.
2. Current rank in a 164px column. Historical ranks share one column after a 12px gutter, 1px rule and 12px inset. Both read label → icon → rank; Peak includes act/elapsed time. Secondary labels/names are 12px.
3. Performance divided from ranks by a fine rule: 17px values with 12px inline labels; last-five results remain 22px, oldest to newest, with existing arrow/underline behavior.

Party tab sits at y12, clear of the level. Self outline, remembered-name decoration and hidden identity rules are unchanged. Full and partial skeletons use the same lanes; performance skeletons are 54×18, avoiding the 30px bar overflow discovered in review.

## Player Details and Match

Details keeps the established two-column structure: recent matches/encounters left, teammates/loadout right. Hero has a 132px identity/rank band and separated metric strip, total 209px including border. Current Rank has the strongest size; previous and peak occupy separate aligned blocks. Values sit above labels. Section titles sit within flat bordered panels. Recent rows are 62px; encounter and teammate rows 60px; weapon rows 70px. Column widths and navigation are retained.

Match uses the same panel system with a 104px header and 46px scoreboard rows. Vertical scroll is intentional and permitted; no column or name is hidden. Live itself never scrolls.

## My Stats

Rank remains 330px beside the SVG RR chart. Four rank changes use two columns; minimum chart height 220px. Full normal top group is 260px in Barlow. Session is a single horizontal strip with its title at the left, 48px minimum content-cell height and 12px vertical padding. Existing chips still open their actual sample matches. Section gaps are 16px. Five recent matches remain 52px rows and are visible in the first normal viewport; Maps, Agents, weekday/time and insights follow in the same scroll container. Long sessions wrap.

## Sample-data review

Run `npm ci`, then `npm --prefix ui run dev -- --port 4173 --strictPort`. Open `http://127.0.0.1:4173/design.html`. The separate review page exposes screen/edge-state, font, tone, accent, result palette/fill and newest-result options. It scales a real interactive 1000×800 iframe to the available review space; the app's own layout is never resized. The screen link opens the full app directly. `design-screen.html` anchors its sample clock to the fixtures so recent matches and sessions do not appear weeks old. Neither review HTML file is a production build entry and the workbench module is not imported by the app; mock feeds and review controls do not reach `dist`. The review defaults to Underline after independent critique of ten repeated arrows; the established product Arrow default remains available and unchanged.

## Critique and iteration ledger

| Round | Evidence / issue | Correction |
|---|---|---|
| Structure | Independent critic found zigzag reading order and name squeezed beside current rank | Three full-width lanes; current rank before grouped history |
| Live render | Party tab overlapped the level; rank/performance lanes crowded | Reserved identity clearance, moved party tab, revised vertical padding |
| Stress scenes | Partial-data stat skeleton exceeded performance lane by 3px | Smaller neutral skeleton, preserving font floor and lane size |
| My Stats | Initial summary pushed several recent matches below first viewport | Two-column rank changes, compact horizontal session, shorter SVG minimum |
| Stability | Loading session was 4px shorter than loaded state | Identical 48px minimum cell height |
| Regression | Old tests pinned obsolete v8 positions | Replaced deliberate geometry pins; retained strict clipping, hidden-name, field, tooltip and navigation checks |

## Concept comparison and intentional differences

Three built-in Image Gen concepts were inspected with `view_image` alongside actual Chromium screenshots at 1000×800. Concepts are design exploration, not shipped raster UI. Prompts specified the entire screen, existing required fields, restrained dark palette and no new services; full prompt text is retained in the chat's tool history.

| Comparison point | Concept → rendered result |
|---|---|
| Information hierarchy | Identity, current rank, grouped history and performance order preserved on all ten cards |
| Container/layout | Two-column Live and Details; rank beside chart; native tables preserved |
| Typography | Consistent existing Barlow; 12px minimum, deliberate number/label scale; alternatives stress-tested |
| Palette | Exact existing charcoal/off-white theme, restrained accent; no concept's illustrative glow/shadows shipped |
| Art | Actual cached VALORANT assets retained, narrower/less prominent than generated portraits to reduce distraction |
| Copy/data | Existing consistent sample roster/rank/history data retained instead of invented concept figures; hidden players never gain the concept's erroneous tag |
| Density | Real 1000×800 sizing replaces the concept's expanded illustrations; current rank uses 40px icon; full first five My Stats matches prioritized over showing Maps rows too |
| Interactions | Real Svelte cards, match rows, tabs, tooltips and back/scroll restoration; no screenshot-based controls |

Copy audit: new visible app labels are the two team column headings and `Current Rank` in Details; both clarify grouping. Existing field labels, scopes in tooltips, result semantics and screen navigation stay. No status copy added. The review-page explanatory copy is outside the product. Generated chevrons on non-clickable map/stat rows were rejected as false affordances.

## Validation boundary

The in-app browser was used for live inspection and interactions. Its screenshot surface remained cropped to the narrow sidebar despite a 1000×800 layout viewport; repository Playwright Chromium supplied exact-size whole-screen screenshots and automated checks. A narrow review viewport is supported by the separate scaled workbench; a mobile product layout is intentionally outside this fixed Windows application's scope.

Backend polling, Riot API, SQLite, installers and native window behavior were not changed. Mock testing cannot verify a live Riot session or real game integration. Performance claims are limited to dependency/build inspection, not a measured Windows RAM/CPU benchmark.

Final local validation: **584 Vitest tests passed; 400 Chromium tests passed in one clean full-suite run**; Svelte/TypeScript check with no errors or warnings; production build; `cargo fmt --check`. The Chromium suite has 26 existing environment-gated skips: 19 historical-design comparisons and seven baseline comparisons that only run in CI's pinned Linux browser. Four obsolete geometry/name-truncation assertions discovered in the first full run were corrected for the replacement layout. No clipping, hidden-name or interaction assertion was disabled. Workbench scene/font changes and a recent-match click were verified separately, including a 390×844 review viewport with no horizontal overflow.

The seven current baseline PNGs were regenerated successfully through [the repository's baseline workflow](https://github.com/opd116/opd1-trcker/actions/runs/38655206457) and pulled into this branch (`c91449d`). The independent final screenshot critic found no P1, clipping or overlap in the four main screens; its remaining P2 preference for less repetition led to the workbench's Underline default. Live still offers both result treatments. This is a mock-data design handoff, not a new installer release.

Build comparison (same local toolchain): JS 146.62 → 147.20 kB raw / 50.04 → 50.17 kB gzip; CSS 39.09 → 41.10 kB raw / 6.47 → 6.88 kB gzip. Total compressed JS+CSS increase about 0.54 kB. Runtime dependencies remain unchanged; `dist` contains no mock roster, demo-feed marker, review controls or review HTML entries. The three concept PNGs are local-only under `.superpowers/redesign-2026-10-15/` (not committed). Current actual screenshots replace `docs/screenshots/*.png`.
