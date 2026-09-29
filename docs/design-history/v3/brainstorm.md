# OPD1 TRCKER: A (Night Drive) and B (Case File), round 3 brainstorm

Scope: In Game at 1000×800 (5 ally cards left, 5 enemy right, map header), plus Lobby with 3 players.
Binding rules kept: one accent, off-white text, min 12px, tabular numerals, no status text, every card field in every stage, last-5 row = W/L/D markers only (RR change + time since go in the tooltip).

Method note: critique run single-context (the brief said no subagents), using the impeccable critique heuristics, design:design-critique framework, interface-design checks (swap/squint/signature/token), and design-taste-frontend's anti-default list. The ui-ux-pro-max search offered "Russo One + Chakra Petch", "Orbitron + JetBrains Mono" and a neon-purple gaming palette; all rejected as category clichés. Hubot Sans (A) and Geologica + Martian Mono (B) were kept, since the complaint was how stats were set, not which family.

---

## 1. Critique of what exists

### A-current (Night Drive)
Works:
- The tach dial makes the current rank the clear focal point, and the wide Hubot RR numerals are the best "stat font" in either direction.
- Calm cabin-black surfaces and off-white text.

Problems:
- **Amber is spent everywhere.** RR sweep, every win light, remembered names, the tab and the wordmark all use it: about 45 amber marks on the In Game screen. That is the "repeated colours" complaint again.
- **The last-5 "shift lights" look like a password field.** Filled vs outlined 14×10 pills in a tray. Win vs loss is fill-only, and in the lobby they turn into a progress bar.
- **Peak and previous act look alike.** Both sit on the footer line at 18px/12px, so the peak (the key scouting fact, e.g. Kaito's Radiant) reads as a footnote.
- **Team identity comes from colour only.** Cyan and red labels bring in two extra hues, and the cards themselves are identical on both sides.
- The party stripe sits on top of colourful card art and gets lost.
- **Lobby:** a 224px decorative banner, stat chips that look like buttons, and a thin "console" strip with one fact in it, which reads as empty space.

### B-current (Case File)
Works:
- The folder tab gives every name its own zone. The warmer enemy folder is a colourless team cue.
- The highlighter idea and the redaction bar for hidden names are strong, on-theme details.
- W/L/D letters are the most explicit last-5 marker, and colour-blind safe.
- **Lobby:** the tall portrait and dotted-leader form rows are the best-composed screen of all six.

Problems:
- **Weak hierarchy.** The RR (20px mono) is barely bigger than K/D (15px), so nothing leads.
- **Too many small boxes.** 50 separate keycaps for the last-5 make a noisy grid.
- **The highlighter reads as a muddy olive underline** at this size. The remembered-name block fill (dark text on yellow) is the loudest thing on screen.
- **The map is a 150×44 thumbnail**, the weakest header of the set.
- **Mono is used for everything** (tags, act codes, stats), so the screen reads a little like code.

---

## 2. Improvement ideas (★ = top pick)

### A: Night Drive
1. ★ **Momentum stems for the last 5.** A win rises above a baseline (lit), a loss drops below it (dim), and a draw is a dot on the line. Direction encodes the result, so it is colour-blind safe and reads as form at a glance, with no pills.
2. ★ **Amber diet: one meaning only.** Amber marks the RR sweep plus the active tab. Wins become lit off-white. This kills the repeated-colour problem.
3. ★ **Give the peak its own slot next to the tach.** It gets its own label, a 28px icon and the act on its own line. The previous act drops to a muted greyscale footnote, so each rank has a different container.
4. ★ **Mirrored face-off.** The enemy column is mirrored (art on the outer edge), so the ranks face each other across the centre line and team is shown by side, not by hue.
5. ★ **Width is the hierarchy.** Hubot Sans uses its width axis for hierarchy: measured values wide (112–125%), names normal, labels narrow (90%). Scale: 30/24/17/15/13/12.
6. **RR inside the dial on the lobby.** It becomes a real speedometer readout, and the emblem sits in the dial.
7. ★ **Party bracket in the outer gutter.** Party members are sorted next to each other, and one coloured bracket joins them. Dashed means likely, solid means confirmed, and ticks point at each card. It shows the group shape without sitting on top of the art.
8. **Windshield header.** A taller map band with a radial scrim shows more of the map, and the name sits on the centre line.
9. **Gear-selector letters as an alternative last-5.** The letters W L W W L sit in a recessed tray, with wins lit in amber, like a car's PRNDL display. It is explicit, but it spends the accent.
10. **Tooltip as a cluster readout:** a result glyph matching the marker, then +RR, then time, separated by hairlines instead of dots.

### B: Case File
1. ★ **Current rank becomes the headline.** A 46px emblem, the RR typed at 25px in wide mono, and the tier name demoted to a caption.
2. ★ **A real comb field for the last 5.** One ruled baseline with tick dividers and five character cells. It carries the same letters with a fraction of the noise.
3. ★ **Photocopy map.** A full-width greyscale, high-contrast strip: calm, on-theme, and finally large.
4. ★ **Mirrored enemy folders.** The tab hangs from the right and the photo sits on the right, so teams are filed from opposite sides.
5. ★ **Labelled rank sheet: Now / Peak / Prev.** Three typed lines with the labels in one column. It is the most direct fix for "can't tell the ranks apart".
6. **Highlighter as a translucent marker swipe.** It covers the full x-height at 30% instead of a thick lower band, so it reads as "marked", not olive.
7. **Ink-only results.** W in bold ink, L in light pencil grey, no red. That leaves one accent (marker yellow as text for "on file" facts).
8. **Stats on dotted leaders in the card too,** so In Game and Lobby read as the same document.
9. **Mono diet.** Mono is kept for measured numbers and tags only; act codes and labels go back to Geologica.
10. **ID-photo mount.** The card art is mounted like a file photo with a 1px paper edge, and the agent is stamped on the corner.

### Wild cards (either direction)
1. ★ **A×B "Night File".** B's structure (folder tab name zone, mirrored filing, labelled rank sheet) with A's instruments (cabin black, Hubot width hierarchy, tach on "Now", stems, one amber light).
2. **Sort by party, then rank.** Keeping party members adjacent makes any stripe or bracket read as a group, and rank order makes the threat readable top-to-bottom.
3. **"You are here" surface.** The user's own card is one elevation step lighter, with no badge or text. Used in A2.
4. **Motion (spec only; static artboards).** On entering In Game, the cards deal outward from the centre line, 30ms apart. Each tach sweeps once from 0 to its RR in about 400ms. Nothing loops, and prefers-reduced-motion keeps a fade only.
5. **Team tint (rejected for now).** The user's own sketch draws allies cyan and enemies red. If structure alone is not enough in real use, add a 2px cyan/red top hairline on each card, as a data encoding rather than decoration.

---

## 3. What each artboard applies

| Artboard | Applies |
|---|---|
| **A-current** | v2 as shipped, with the tooltip drawn open on Falcon's newest result. |
| **A2 · Cluster** | A1 stems, A2 amber = RR sweep only, A3 peak slot + muted prev act, A5 width hierarchy, A10 cluster tooltip, "you are here" surface. Lobby adds A6 (RR inside a 200px dial) and a peak/prev split, with even vertical rhythm and no empty band. |
| **A3 · Face-off** | A4 mirrored enemies, A7 gutter party brackets, A8 windshield header, A9 gear letters (amber = lit = wins + remembered names), peak with a max glyph and act chip, RR sweep in off-white. |
| **B-current** | v2 as shipped, with the tooltip drawn open. |
| **B2 · Dossier** | B1 hierarchy, B2 comb field, B3 photocopy map, B6 marker swipe, B10 ID-photo mount, keeping the red ribbon for L. Lobby adds a 40px RR, the comb field, taller portraits and balanced spacing. |
| **B3 · Cross-file** | B4 mirrored folders, B5 Now/Peak/Prev sheet, B7 ink-only letters typed on one rule, B8 dotted-leader stats, B9 mono diet, marker yellow as text, and a header with folder-divider tabs plus a photocopy label. |
| **AxB · Night File** | Wild card 1: folder tabs mirrored for enemies, a Now/Peak/Prev sheet with the tach on Now, Hubot width hierarchy, stems, gutter party brackets, and one amber light (the RR sweep). |

All boards were verified at exactly 1000×800 in headless Chromium: no scrollbar, no text under 12px, no text-on-text or text-on-image overlap, no clipping. The longest name `WWWWWWWWWWWWWWWW#WWWWW` fits on both sides, including remembered and mirrored cases. The canvas exports were re-rendered and pixel-diffed against the live pages (mean difference 0.04–0.23 per channel).

---

## 4. Honest ranking

1. **AxB · Night File: the recommendation.** It fixes all three big complaints at once.
   - The ranks can't be confused: each gets a labelled line, and "Now" has the dial.
   - The stats get the best numerals (wide Hubot, not mono).
   - Amber has exactly one job.
   - The folder-tab silhouettes make the teams readable without any colour.

   Weak spot: the stems are less explicit than letters on first sight. **Best next step:** try AxB with B3's typed W/L letters in place of the stems, and keep everything else.
2. **B3 · Cross-file.** The clearest information design of all: the rank sheet plus explicit letters, calm and ink-only. It ranks below AxB only because mono numerals may repeat the "stat font" complaint, and the small map label is modest.
3. **A2 · Cluster.** The calmest and most "premium" screen, with strong hierarchy. The peak slot works well. **A2-lobby is the best lobby of the round** (RR inside the tach). The stems are an acquired taste.
4. **B2 · Dossier.** A solid, low-risk refinement of B: the comb field and photocopy map are clear upgrades, but the card still has four competing zones. B2-lobby is very good and close to final.
5. **A3 · Face-off.** The mirrored layout and gutter brackets are worth keeping (they carry into AxB). But lit amber wins bring back about 30 accent marks, which is exactly the "repeated colours" problem.

**Lobby pick:** A2-lobby if AxB/A wins, B2-lobby if B wins. Either can take AxB's Now/Peak/Prev labels unchanged.

---

## 5. Files
- Sources: `scratchpad/design-v3/<A2|A3|B2|B3|AxB|A-current|B-current>/index.html` (`?s=ingame`, `?s=lobby3`, `&tip=1`)
- Screenshots: `scratchpad/design-v3/<name>.png`, `<name>-lobby.png`, `<name>-tip.png`
- Scripts: `scratchpad/design-v3/shared/capture.mjs` (screens + checks), `shared/export.mjs` (canvas export), `shared/validate.mjs` (format rules + pixel diff)
- Canvas: `scratchpad/design-canvas/project/*.dc.html`, `scratchpad/design-canvas/assets/` (+ `manifest.json`)
