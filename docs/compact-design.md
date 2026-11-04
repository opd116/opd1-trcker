# Compact competition roster — 2026-11-03

Status: implementation for review, not a merged release. R205 and the newest block in `requirements-raw.md` supersede earlier visual specifications only.

## Visual contract

- Preserve the approved prototype, not frozen Design 1, Design 2, or rejected B. Its final near-black/red trial is the starting point. No frozen archive is copied back over it.
- Local Barlow Semi Condensed 500/600/700 unifies names, captions and tabular metrics. Retained mono/code faces stay local. No new runtime dependencies or font CDN.
- Canvas `#0C0B0C` lifts the preceding `#020202` slightly. Alternate rows `#191718`, roster/panels `#101010`, brand `#D63743`. Semantic rank/party/W/L colors are not repurposed.
- Two five-player teams stack in one column. Alternate shading restarts at each team. Portraits 36px; current/peak icons equally 32px. Win/KD/HS share fixed column centers and baselines.
- Flexible 48–80px rows consume excess bottom space, leaving roughly 16px at supported window heights. Default 700×860, minimum 680×760; large windows remain usable. Lists/profiles may scroll, normal ten-player Live does not need to.
- Image saturation 1.22 affects artwork, not portrait borders, text or page backgrounds. Self ownership rail stays red; detected parties retain confirmed/likely distinction.

## Navigation and information

Live is immediate. Records discloses History, Saved players and Competitive. Tools discloses Skins and ASCII. Escape closes disclosures and restores their trigger focus; normal tab navigation works. Settings intentionally contains only its page title. About remains available without another full-width text tab. No duplicate own-rank control in the shell. Glance enters a 420×420 logical-pixel window, preserving its origin and restoring the previous normal geometry on exit.

Live notes and game counts stay off the roster. Saved notes remain in the profile. Hidden-name style/source, real tri-state loading/missing values, exact RR, account guards, encounter scope, links and keyboard return behavior remain product rules.

## Peak view

Current retreats in perspective while Peak settles forward. The persistent layers reverse during rapid toggles. R206 adds the visible full peak rank name and a downward numeric badge toward Current when Current is lower (e.g. ↓2); Current retains its upward badge toward Peak. R207 replaces the former three-cell bar with the actual act/episode label and age since act end (e.g. `E9 A3 · 1y ago`; current act is labeled `This act`). Full act name and dates remain in the tooltip/accessibility text. No invented episode labels or fake peak data. OS reduced motion disables movement without disabling the control.

R206 removes the pin and always-on-top setting/runtime entirely, in both normal and Glance windows; other window controls, tray/autostart and geometry restore remain.

## Party connectors and statistic colors (R207, R210)

**R208 reference correction; R209 connected strokes:** the user's supplied crop (Drake/Nova in blue; Mara/Kilo in yellow) is authoritative: a rounded vertical spine with small right-pointing arrows into each portrait, not the assistant's mistaken R207 no-arrow restoration. The first/last branch curves into the spine; a trio's middle member has a horizontal branch at portrait center. Only active stroke edges are painted, with no stray rectangles. Distinct groups retain distinct colors (including enemy duo+duo / trio+duo). **R209 explicitly makes all connectors and portrait frames solid**, including likely parties; real confidence remains in data/tooltips, not falsified as confirmed. Keep the existing transitive/team-aware detection algorithm and all other approved layout/geometry.

**R210 exact palette and continuous thresholds:** Live K/D/HS use the **5-match** continuous lower-bound tables; profile K/D/HS use the **20-current-act-match** tables; WR uses the same **full-act** table on both. R210 replaces both the metric palette and threshold bands with the exact six colors (0: `#D32F2F` dark red, 1: `#FF5252` red, 2: `#FF9100` orange, 3: `#FFFFFF` white, 4: `#69E08A` light green, 5: `#00C853` strong green) and five continuous lower-bound arrays:
- `LIVE_KD_BANDS [0.72, 0.79, 0.87, 1.15, 1.39]`
- `LIVE_HS_BANDS [0.15, 0.175, 0.205, 0.275, 0.33]`
- `PROFILE_KD_BANDS [0.81, 0.86, 0.92, 1.10, 1.24]`
- `PROFILE_HS_BANDS [0.16, 0.185, 0.21, 0.27, 0.32]`
- `WR_BANDS [0.36, 0.40, 0.44, 0.56, 0.64]` (shared for Live and profile contexts).

Colors are evaluated strictly from RAW numerical values before rounding or display formatting. Threshold bands are continuous with inclusive lower bounds and exclusive upper bounds (`[lower, upper)`). Real zero maps to color 0 (`#D32F2F`), negative/null/undefined/NaN/Infinity/out-of-bounds HS or WR (> 1) and highlight-off map to context-neutral, while KD values > 1 remain valid without an upper ceiling. Calculation scopes (Live KD/HS last 5 Competitive, Profile KD/HS last 20 current-act Competitive, WR full current-act) are strictly preserved.

## Compact profile sections (R209, R210)

After rank/stats, show Encounters first when any are stored/expected, then Top teammates. With no encounters, omit the empty encounter panel and start with Top teammates.

**Lightweight measured layout & section placement:**
- **Deterministic column placement (>= 600px):**
  - **With encounters:** Left column renders Encounters then Recent matches; Right column renders Top teammates then Map performance directly beneath it with an exact 12–20px vertical gap (nominally 16px).
  - **Without encounters:** Left column renders Top teammates then Map performance directly beneath it with an exact 12–20px vertical gap (nominally 16px); Right column renders Recent matches.
  - **Loadout:** Spans full width at the bottom below both columns.
- **Priority DOM, reading & keyboard order:** Sections are direct sequential children of `.profile-content` in priority sequence: Encounters (if present) → Top teammates → Recent matches → Map performance → Loadout. Screen readers and Tab key navigation follow this sequence naturally without tabindex overrides.
- **Single vertical scroll flow:** Eliminate internal Recent matches scrolling and the 340px height cap in `RecentTable.svelte`. The entire profile drawer content flows naturally within one main vertical scroll container (`ProfileDrawer.svelte`), and all six recent matches are fully rendered and reachable via profile vertical scrolling.
- **No grid-row gaps:** Lightweight measured positioning (`ResizeObserver`, zero new dependencies) eliminates artificial height gaps below Top teammates in both variants.
- **Unified section headings:** Section headings across profile components (`Encounters`, `Mates`, `RecentTable`, `Maps`) use unified placement, `.mlabel` typography, and spacing.
- **Header headroom:** RecentTable map area retains at least 48px headroom with no clipped descenders.
- **Responsive fallback:** Below 600px drawer width the single-column fallback stacks sections in identical priority sequence, preserving focus across resize without node remounting. Personal notes, loadout, all data, keyboard access, and scoreboard navigation rules are preserved.

## Verification boundaries

`ui/playwright.compact.config.ts` isolates the compact acceptance suite on port 4187. Existing functional tests protect account changes, navigation, lists and actions. **R211:** the user approved the entire Compact design and chose it over the historical v2 comparison. Active visual references move to `ui/tests/baselines/compact/`, covering both normal sizes and square Glance in the pinned Playwright image; root-level v2 files remain unchanged as history. Keep screenshot tolerance, image-readiness checks, and functional/privacy assertions strong; missing required Compact references fail CI. This authorized migration is not permission to suppress real failures.

Windows is the application target. Linux CI is a tool environment, not promised game support, and does not replace Windows build/Rust checks or manual native/Riot verification. Browser/demo tests do not establish real VALORANT correctness. The Rust request/DB implementation is otherwise unchanged: beyond the normal-window minimum, the window module carries the compact default **700×860** and the **420×420 square Glance**, with restore/round-trip tests. Native window geometry still needs interactive Windows acceptance.
