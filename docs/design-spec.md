# OPD1 TRCKER: cd-v8 build spec (Svelte 5)

Build target: one Live page (Lobby / Agent Select / In Game / Closed), Player Details and Match Details, all 6 design options wired at their defaults. The Settings page and My Stats page are NOT built.

- Source of truth: `.superpowers/design/cd-v8/project/OPD1 Tracker v8.dc.html`. `v8:N` below means line N of that file. `screens/*.png` are stale; ignore them.
- Precedence (R92): dated user decisions in `docs/requirements-raw.md` (newest block 2026-09-29) > the design file > older CLAUDE.md visual rules. This spec transcribes the design; it outranks the design file only for its **[C]**/**[bound]** gap-fills (R74–R86). Where this spec and the design disagree on something the design covers, the design wins and this spec is corrected. Any change to a visual the design covers goes to the user first: the user approved the design, not this spec.
- Anything marked **[C]** is a Claude call that fills a state or gap the design does not cover. It is built only from the design's own parts and tokens. Every [C] and [bound] call is recorded as a ruling (R74–R86) in `docs/DECISIONS.md` and the ledger (plan Task 13).
- The prototype runtime (`support.js`, React, Babel, Google Fonts, `media.valorant-api.com`) is never shipped.
- Location: plan Task 13 commits this file as `docs/design-spec.md`. From then on only the committed copy is edited; this `.superpowers/` copy is history.

---

## 0. Ground rules for the implementer

| Rule | Value |
|---|---|
| Global CSS | Only `html,body{margin:0;height:100%;background:var(--bg,#0e1013)}` (the fallback equals the Neutral `--bg`; it keeps the window dark until `main.ts` writes the theme variables, and is never used afterwards), the R106 keyboard-only ring `:focus-visible{outline:1px solid color-mix(in srgb,var(--accent) 60%,transparent);outline-offset:2px}` (no mouse-visible change), and the `@font-face` rules. **No CSS reset**: no global `box-sizing`, no global `line-height`, no `img{display:block}`. The design relies on browser defaults. |
| box-sizing | `content-box` everywhere except the elements where this spec says `border-box` (it copies the design's own `box-sizing:border-box`). Consequence: the top bar is 48px + 1px border = **49px**. |
| line-height | `normal` (font-dependent) unless this spec gives one. It is given only for stat labels (14px), stat values (24px card / 27px hero), the Match header score (32px) and tooltips (18px). |
| Units | px only. Heights with `1fr`/`flex:1` resolve to fractions (for example card height 139.8px). Do not round them. |
| Numerals | `font-variant-numeric: tabular-nums` only where this spec says `tab`. |
| Minus / dashes | RR and +/− use U+2212 `−`. Scores use U+2013 `–` (`13–9`). The missing value is U+2014 `—`. The date-range separator is ` – ` (U+2013 with spaces). |
| Images | Every image goes through `assetUrl(kind, id)` → `img://` (Tauri) or the DEV CDN helper. Kinds: `agent`, `tier`, `mapList`, `chroma` (all existing) plus **NEW** `cardLarge` and `contentTier` (§7.3). |
| Missing image | Keep the box (same size, same `background`). Never collapse the layout. |
| Status text | None anywhere. No placeholder words in skeletons. |
| Font floor | 12px. Never shrink to fit. Only the name part of a Riot ID truncates with `…`, plus the design's own ellipsis spans in Details: the encounters header name (§6.6), the loadout skin line (§6.8) and the recent-row map name (§6.4). Nothing else may clip; the Peak and Prev rows included (§8.3 check 5). |
| Scroll | None in any Live stage. Player Details and Match Details scroll vertically only. |
| Hover target | `cursor:pointer` only on: the Live tab, cards, Back, recent rows, encounter rows, Show more. |

Glyph and text constants (exact strings): `OPD1 TRCKER`, `Live`, `My Stats`, `Lv`, `RR`, `Peak`, `Prev`, `This Act`, `K/D`, `HS%`, `Win%`, `Games`, `W / L`, `Back`, `Recent Competitive Matches`, `Map`, `Score`, `K / D / A`, `Show more`, `Encounters`, `You`, `Your result`, `With`, `Against`, `Top 3 Teammates`, `games`, `Loadout`, `Vandal`, `Phantom`, `Operator`, `Sheriff`, `Player`, `ACS`, `+/−`, `ADR`, `Your team`, `Opponents`, `Hidden Player`, `Avg party rank` **[C]**.

---

## 1. Theme

### 1.1 Options (all built; the values below are the only ones shipped until Settings exists)

| Option | Values | Default |
|---|---|---|
| `tone` | `Neutral` · `Cool` · `Warm` | `Neutral` |
| `accent` | `#e3a54f` · `#4fc3ae` · `#7fb1ef` · `#e58a9a` | `#e3a54f` |
| `results` | `Green / Red` · `Accent / Red` · `Accent / Gray` | `Green / Red` |
| `resultFill` | `Tinted` · `Solid` | `Tinted` |
| `font` | `Barlow` · `IBM Plex Sans` · `Archivo` | `Barlow` |
| `last5Style` | `Arrow` · `Underline` | `Arrow` |

### 1.2 TONES (v8:442-446, verbatim)

| key | Neutral | Cool | Warm |
|---|---|---|---|
| bg | `#0e1013` | `#0b0f14` | `#100f0d` |
| card | `#171a1f` | `#141920` | `#1a1815` |
| raised | `#20242a` | `#1d242e` | `#24211d` |
| line | `#23272e` | `#212833` | `#2a2722` |
| line2 | `#2c3139` | `#2a323e` | `#35312b` |
| text | `#e4e1db` | `#e0e4ea` | `#e7e2d9` |
| textSoft | `#c9c9c7` | `#c3c9d1` | `#cbc5bb` |
| text2 | `#a3a7ae` | `#9ea7b3` | `#aaa396` |
| text3 | `#6f747c` | `#6a7380` | `#77716a` |
| gray | `#8d929a` | `#8b95a2` | `#978f84` |

Constants: `GREEN = #5fbf8b`, `LOSS = #dc7a6e`, `ON_SOLID = #14120e` (text on a solid chip).

### 1.3 Colour math (v8:503-505, port exactly, including rounding)

```ts
const rgb = (h: string) => [1, 3, 5].map(i => parseInt(h.slice(i, i + 2), 16));
export const mix = (a: string, b: string, t: number) =>
  '#' + rgb(a).map((v, i) => Math.round(v + (rgb(b)[i] - v) * t).toString(16).padStart(2, '0')).join('');
export const rgba = (h: string, a: number) => `rgba(${rgb(h).join(',')},${a})`;
```

### 1.4 theme() derivations (v8:535-556)

`t = TONES[tone]`.

| Token | Formula | Neutral + defaults |
|---|---|---|
| accent | option | `#e3a54f` |
| win | `results==='Green / Red' ? GREEN : accent` | `#5fbf8b` |
| loss | `results==='Accent / Gray' ? t.gray : LOSS` | `#dc7a6e` |
| solid | `resultFill==='Solid'` | false |
| lineHover | `t.line2` | `#2c3139` |
| skel | `t.raised` | `#20242a` |
| hoverRow | `mix(t.card, t.raised, 0.6)` | `#1c2026` |
| teamBg | `mix(t.card, t.raised, 0.45)` | `#1b1f24` |
| tipBg | `mix(t.card, '#ffffff', 0.07)` | `#272a2f` |
| arrowCol | `rgba(t.text2, 0.6)` | `rgba(163,167,174,0.6)` |
| selfFrame | `rgba(t.text, 0.55)` | `rgba(228,225,219,0.55)` |
| scroll | `t.line2` | `#2c3139` |
| stripFade | `linear-gradient(90deg, rgba(card,0) 0%, rgba(card,0.25) 55%, card 100%)` | `linear-gradient(90deg, rgba(23,26,31,0) 0%, rgba(23,26,31,0.25) 55%, #171a1f 100%)` |
| arrow | `last5Style==='Arrow'` | true |
| (skelStrip) | `mix(card, raised, 0.5)`, unused by the design | not needed |

Computed per tone (for tests and snapshots):

| Token | Neutral | Cool | Warm |
|---|---|---|---|
| hoverRow | `#1c2026` | `#192028` | `#201d1a` |
| teamBg | `#1b1f24` | `#181e26` | `#1f1c19` |
| tipBg | `#272a2f` | `#242930` | `#2a2825` |
| arrowCol | `rgba(163,167,174,0.6)` | `rgba(158,167,179,0.6)` | `rgba(170,163,150,0.6)` |
| selfFrame | `rgba(228,225,219,0.55)` | `rgba(224,228,234,0.55)` | `rgba(231,226,217,0.55)` |
| tint(GREEN) | `#284239` | `#26413a` | `#2b4031` |
| tint(LOSS) | `#463132` | `#443033` | `#49302a` |
| tint(gray) | `#33373d` | `#31373f` | `#383530` |
| tint(#e3a54f) | `#483b2b` | `#463b2b` | `#4a3a23` |
| tint(#4fc3ae) | `#244341` | `#224242` | `#27413a` |
| tint(#7fb1ef) | `#303e51` | `#2e3d52` | `#323d49` |
| tint(#e58a9a) | `#48353d` | `#46343d` | `#4b3335` |

`tint(col) = mix(t.card, col, 0.24)`.

### 1.5 Result boxes (v8:542, 549-553, 568)

`chip(col) = solid ? {bg: col, fg: ON_SOLID, border: 'none'} : {bg: tint(col), fg: col, border: 'none'}`

| Key | bg | fg | border |
|---|---|---|---|
| W | `chip(win)` | | |
| L | `chip(loss)` | | |
| D | `transparent` | `t.text2` | `1px solid t.text3` |
| E (empty slot) | `transparent` | `transparent` | `1px dashed t.line2` |

Newest tile (last-5 only), **Arrow** style:

| Key | Tinted | Solid |
|---|---|---|
| W | `{bg: win, fg: ON_SOLID, border: none}` | `{bg: win, fg: ON_SOLID, border: 2px solid t.text}` |
| L | `{bg: loss, fg: ON_SOLID, border: none}` | `{bg: loss, fg: ON_SOLID, border: 2px solid t.text}` |
| D | `{bg: transparent, fg: t.text, border: 1px solid t.text}` | same |

Newest tile, **Underline** style: the same box as the older tiles, plus an underline bar (§3.8).

Everywhere else (recent rows, encounter rows, Match header, team headers) uses the plain W/L/D boxes, never the newest variant.

### 1.6 Party colours: 4 per accent **[C]**

The design ships 2 per accent (v8:448, 554). Extended sets (the first 2 are the design's, unchanged):

| accent | party[0] | party[1] | party[2] NEW | party[3] NEW |
|---|---|---|---|---|
| `#e3a54f` (default; also the fallback for any other accent) | `#7fa6e8` | `#b596e0` | `#b0c46c` | `#db8fb4` |
| `#4fc3ae` | `#7fa6e8` | `#b596e0` | `#b0c46c` | `#db8fb4` |
| `#7fb1ef` | `#5fbfa9` | `#c99ad8` | `#b0c46c` | `#db8fb4` |
| `#e58a9a` | `#7fa6e8` | `#5fbfa9` | `#b0c46c` | `#b596e0` |

Rule for the added colours:
- Walk the reserve list `[#b0c46c lime (HSL hue 74°), #db8fb4 rose (331°), #b596e0 lilac (265°), #7fa6e8 blue (218°)]` and take the first 2 that are:
  - ≥30° HSL hue from the accent,
  - ≥30° from `GREEN` (147°),
  - ≥30° from `LOSS` (7°),
  - and ≥30° from every colour already in the set.
- Every reserve colour has lightness 60–71%, saturation 43–54% and contrast ≥7.0:1 on `card` (lime 9.10, rose 7.14, lilac 6.98, blue 7.08), matching the design's pair.
- The rule was checked for all 4 accents: amber → lime 39°/rose 36° clear; teal → same; blue → lime, then rose 46° from `#c99ad8`; pink → rose rejected (18° from the accent), lilac taken.
- For a non-listed accent, apply the rule to the fallback pair. If fewer than 2 pass, fill from the reserve in order.
- Index: `party[group % 4]`. Groups are numbered by the backend in roster order across the whole screen. The maximum number of real groups is 4 (2 ally + 2 enemy).

### 1.7 Fonts

`FONTS = {Barlow: "'Barlow', sans-serif", 'IBM Plex Sans': "'IBM Plex Sans', sans-serif", Archivo: "'Archivo', sans-serif"}` (v8:447).

- Bundle latin-subset woff2 for all 3 families in weights **500, 600, 700 and 500 italic** (12 files, `font-display: block`), plus OFL.txt. Use Barlow regular width, not Semi Condensed. Delete the old `barlow-semi-condensed-*` files.
- Weight 400 is dropped. The only place the design inherits 400 is the hero `Prev —` (v8:219). This spec sets it to 500 (§6.2).
- @font-face files load only when a family is used, so the 2 non-default families cost bundle bytes (about 150KB) but no runtime work.

### 1.8 Options → CSS custom properties

`applyTheme(opts)` writes these on `document.documentElement.style` and sets `data-last5="arrow|underline"` on `#app`. Components read only the variables, never the tables.

| Option(s) | Variables |
|---|---|
| tone | `--bg --card --raised --line --line2 --text --text-soft --text2 --text3 --gray` plus the derived `--line-hover --skel --hover-row --team-bg --tip-bg --arrow-col --self-frame --scroll --strip-fade` |
| accent | `--accent`, `--party-0 … --party-3` |
| results | `--win --loss` |
| tone + results + resultFill | `--r-w-bg --r-w-fg --r-w-bd`, `--r-l-*`, `--r-d-*`, `--r-e-*`; newest (Arrow): `--rn-w-* --rn-l-* --rn-d-*` |
| font | `--font` |
| last5Style | `data-last5` attribute and a `theme.last5Style` store value (it decides whether the arrow row is rendered) |
| (constant) | `--on-solid: #14120e` |

`ui/src/lib/theme.ts` exports `TONES`, `FONTS`, `ACCENTS`, `PARTY_SETS4`, `DEFAULT_THEME`, `themeVars(opts): Record<string,string>` (a pure function, unit-tested against §1.4) and `applyTheme(opts)`. The theme store is a `$state` object initialised with `DEFAULT_THEME`. It is not persisted. In DEV only, URL params `?tone=&accent=&results=&fill=&font=&last5=` override it (for Playwright).

---

## 2. Global chrome

### 2.1 Root `#app` (v8:16)

`position:relative; width:1000px; height:800px; overflow:hidden; background:var(--bg); color:var(--text); font-family:var(--font); display:flex; flex-direction:column; -webkit-font-smoothing:antialiased`.

### 2.2 Top bar (v8:18-32)

- Container: `height:48px; flex:none; display:flex; align-items:center; gap:30px; padding:0 18px; border-bottom:1px solid var(--line)`. Content-box, so the outer height is 49px.
- Wordmark: `OPD1 TRCKER`, 14px/700, `letter-spacing:0.18em`, `nowrap`.
- Tabs wrapper: `display:flex; gap:24px; align-self:stretch`.
  - Each tab: `position:relative; display:flex; align-items:center; font-size:14px`.
  - Active: weight 600, colour `--text`, plus an indicator `position:absolute; left:0; right:0; bottom:-1px; height:2px; border-radius:1px; background:var(--accent)`.
  - Inactive: weight 500, colour `--text2`.
- `Live` is always the active tab, including in Details and Match Details (as in the design). Clicking it goes to the Live grid.
- `My Stats` renders inactive and inert: no handler, `cursor:default` **[C]** until the page exists.
- Spacer: `flex:1`.
- Right group (only on the Live grid route; empty on Details and Match): `display:flex; align-items:center; gap:12px`. Content per §2.3.

### 2.3 Right side per stage (Live route only)

| Stage | Content |
|---|---|
| `closed` | empty |
| `menus` (Lobby), 1 player | empty |
| `menus`, ≥2 players | **Average Party Rank [C]**, see below |
| `pregame` | map group once `mapId` is known; skeleton while null |
| `ingame` | map group; skeleton while `mapId` is null |

- **Map group** (v8:26-29): the map name (`meta.maps[mapId].name`) 14px/600, then `<img mapList>` `display:block; width:116px; height:32px; object-fit:cover; border-radius:4px`. Gap 12. No tooltip.
- **Map skeleton** (v8:31): `52×12` radius 3 `--skel`, then `116×32` radius 4 `--skel`. Gap 12.
- **Average Party Rank [C]** (built from the card's label/value pattern and the rank block):
  - Content, gap 12: label `Avg party rank` 12px/500 `--text2`, then a group with gap 8: `<img tier>` 32×32, tier name 14px/600 `--text`.
  - Value: `round(mean(tier))` over party cards whose `rank.tier ≥ 3`, with .5 rounding up. Name = `tierLabel(meta, tier)`.
  - Tooltip on the whole group: `Average party rank\nFrom {count(n, 'ranked player')}` (`n` = ranked members counted; `From 1 ranked player`, `From 3 ranked players`, §4.3). **(R129)** No tooltip while the group is loading (there is no count to explain yet). With 2+ players present and none ranked, the tip reads `Average party rank\nFrom 0 ranked players` (the formula applied literally; it explains the empty box and `—`).
  - Loading: if any member's `rank` is still absent AND no member is ranked yet, show the label, then a 32×32 circle `--skel`, then a 52×12 bar radius 3 `--skel`.
  - ≥2 players but 0 ranked: label, empty 32×32 box, then `—` 14px/600 `--text2`.

### 2.4 Routes, navigation and startup

- Routes: `live` | `details(puuid)` | `match(matchId, focusPuuid, from: 'recent'|'encounter')`.
- Card click → `details(card.puuid)`, and `more=false`.
- Recent or encounter row click → `match(…)`.
- Match `Back` → `details` (same puuid, keeps `more`). Details `Back` → `live`.
- Every navigation clears the tooltip and resets the scroll container to top.
- Auto-switch: when `live.stage` changes from anything to `pregame`, set the route to `live`.
- Startup:
  - Route `live`.
  - `await listen('live')`, then `invoke('live_state')`. Apply the invoke result only if no event has arrived.
  - `invoke('meta')`.
  - `listen('details')`: replace the open details when `payload.card.puuid` is the player open or opening. An event always wins over a `player_details` answer still in flight. Navigating to `live` forgets the open details (Match and Back keep them).
- Keys (no UI): Ctrl+Shift+L → `invoke('copy_log')` then `navigator.clipboard.writeText`. Ctrl+Shift+D → `invoke('dump_raw')`.

---

## 3. Live grid and PlayerCard

### 3.1 Grid and slot model

- Container (v8:35): `flex:1; min-height:0; display:grid; grid-template-columns:minmax(0,1fr) minmax(0,1fr); gap:10px; padding:10px`.
- Each column: `display:grid; grid-template-rows:repeat(5,minmax(0,1fr)); gap:8px; min-height:0`. The design uses a flex column with 5 `flex:1` cards; this is geometrically identical.
- **Decision [C]: every Live column is a fixed 5-slot track in every stage. A card is always 485 × 139.8px.**
  - Why: the design's `flex:1` derives card height from 5 cards per column. The card's inner grid (rows 40/18/1fr, stats bottom-aligned) is tuned to about 140px.
  - With 1 card, `flex:1` would stretch it to 731px and leave a ~590px hole between Peak/Prev and the stats.
  - CLAUDE.md requires the card to be identical in all stages.
- Cards fill slots top-down. Unused slots render nothing, except the Agent Select right column (§5.1).
- Order: in the left column, the `isSelf` card comes first, then the others in emitted order. The right column keeps emitted order. Never reorder when party marks arrive (R53: they can pop in late).
- At most 5 per column.

Geometry at 1000×800 (default font; geometry does not depend on the font):

| Item | Value |
|---|---|
| Grid top | y = 49 |
| Card left x | 10 (left), 505 (right) |
| Card top y | `59 + i × 147.8` (i = 0…4) |
| Card size | 485 × 139.8 |
| Card content box | x 56…471, y 12…127.8 (relative to the card) |
| Column tracks | left 241px, gap 16, right 158px (x 297…313 is the gap) |
| Row tracks | 40 (y 12–52), gap 10, 18 (y 62–80), gap 10, 1fr = 37.8 (y 90–127.8) |

### 3.2 Card box (v8:39-50)

- Outer: `position:relative; background:var(--card); border-radius:6px; overflow:hidden; cursor:pointer`. On hover: `box-shadow: inset 0 0 0 1px var(--line-hover)`.
- Art strip (v8:40-43): `position:absolute; left:0; top:0; bottom:0; width:44px; overflow:hidden`. Two layers, both `inset:0`:
  1. `background-image:url(assetUrl('cardLarge', cardId)); background-size:150% auto; background-position:50% 0; background-repeat:no-repeat; opacity:0.8`
  2. `background:var(--strip-fade)`
- Inner grid (v8:50): `position:relative; height:100%; box-sizing:border-box; display:grid; grid-template-columns:minmax(0,1fr) 158px; grid-template-rows:40px 18px 1fr; column-gap:16px; row-gap:10px; padding:12px 14px 12px 56px`.

Grid children, in DOM order:

| # | Cell | Content |
|---|---|---|
| 1 | r1 c1 | agent + name block (§3.3) |
| 2 | r1 c2 | current rank (§3.6) |
| 3 | r2 c1 | Peak row |
| 4 | r2 c2 | Prev row |
| 5 | r3 c1 | stat trio (§3.7) |
| 6 | r3 c2 | last 5 (§3.8) |

The party stripe and tab (§3.9) come **before** the grid in the markup, as in the design (v8:44-49); their `z-index` 2 and 3 paint them above the grid (a hit test at the tab and the stripe finds them, not the grid). The self outline (§3.10) is the last child, after the grid.

### 3.3 Agent + name block (v8:52-79)

- Wrapper: `display:flex; align-items:center; gap:10px; min-width:0`.
- Agent: 40×40, `flex:none; border-radius:6px; background:var(--raised)`.
  - `agentId` set → `<img assetUrl('agent', agentId)>` (displayIcon).
  - `agentId` null → the same box as an empty `<div>`. Never a placeholder agent. **(R127)** This empty box is one look in every stage: Agent Select before a lock, Lobby and Closed (where no agent can exist) all keep it, because the card is identical everywhere and R99 names it the empty agent slot. It is not the skeleton shape (`.sk`; a loading card is all grey blocks with no text, this card has a name, rank and stats beside the box), and it is not made transparent in any stage (that would leave a 40px hole beside the name).
- Name column: `flex:1; min-width:0; display:flex; flex-direction:column; gap:2px`.
  - Line 1: `display:flex; align-items:center; min-width:0`.
    - Name wrapper (the tooltip trigger): `flex:1; min-width:0; display:flex; align-items:baseline; gap:5px`. Variants per §3.4.
    - Encounter slot: `min-width:36px; flex:none; display:flex; justify-content:flex-end` (§3.5). The slot is always rendered, even empty. **[C]** The design has `width:36px`; `min-width` renders identically for counts up to 2 digits and lets a 3+ digit count widen the slot leftwards (measured: `123` is 36px in Barlow, 39px in IBM Plex Sans and Archivo), so the name ellipsizes earlier and the count never overlaps the tag. No `99+` cap (it is as wide as 3 digits). **[C] (R129)** The card's ⟲ badge itself has `margin-left:5px` (the name–tag gap, §3.5). The slot is right-aligned, so for 1–2 digit counts the margin is absorbed inside the design's 36px slot (geometry unchanged, in all 3 fonts); it only takes effect once the count has 3 or more digits, where it keeps a 5px gap so the tag never touches the icon. (The widths above were measured before this margin.)
  - Line 2: `align-self:flex-start; font-size:12px; font-weight:500; color:var(--text2)`. Text: `Lv ` + `<span style="color:var(--text)" tab>{level}</span>`. `level === null` (Riot withholds it, HideAccountLevel, R100; never on the user's own card) → `Lv —`. **(R129)** `level` absent (still loading, R23) → the skeleton card's level bar drawn inline in the Lv line: an inline-block 48×10 bar, radius 3, `--skel`, `vertical-align:middle` (§5.4), replacing the whole `Lv …` text. The line box keeps its height, so the name column does not move.
- Name wrapper width = 241 − 40 − 10 − 36 = 155px (less when a 3+ digit count widens the slot).

### 3.4 Name variants (card 16px; hero 22px §6.2; row 14px §6.7/§6.10)

| Variant (`card.name.kind`) | Nodes inside the wrapper | Tooltip |
|---|---|---|
| `visible` (includes self) | name: `min-width:0; font-size:16px; font-weight:600; white-space:nowrap; overflow:hidden; text-overflow:ellipsis`. Tag: `flex:none; font-size:13px; font-weight:500; color:var(--text2); nowrap`. Tag text = `'#' + tag`. | `{name}#{tag}`, overflow-only |
| `remembered` | eye-off 14×14 (`flex:none; align-self:center; color:var(--text2)`), name as `visible` plus `text-decoration:underline dotted var(--text2); text-underline-offset:4px` (hero 5px), then the tag. **[C] (R128)** The name span uses `overflow:clip; overflow-clip-margin:6px` instead of `overflow:hidden`, so the underline is not clipped (see below). **(R145)** One look for every `source`; only the tooltip differs | by `source`, always (§4.2): `Hidden name · known from your match` / `… known from a past match` + `\n{Map} · {Mon D, YYYY}`; `Hidden name · seen before` + `\n{Mon D, YYYY}` |
| `hiddenAgent` | eye-off 14, then the agent name (`meta.agents[agentId].name`): `min-width:0; font-size:16px; font-weight:500; font-style:italic; color:var(--text-soft); nowrap; overflow:hidden; text-overflow:ellipsis`. No tag. | `Hidden name`, always |
| `hiddenPlayer` (Agent Select before the lock, R99) | eye-off 14, then `Hidden Player` in the `hiddenAgent` style | `Hidden name`, always |
| name `null` (missing) | `—` 16px/600 `--text2` | none |
| name absent (loading) | an inline-block 120×13 bar, radius 3, `--skel`, `vertical-align:middle`. The line box stays. No tag. | none |

- **The remembered underline needs clip room (R128).** The dotted underline is painted 2–4px below the name's box (font and size dependent; fully outside at the card, hero and row sizes in Barlow and Archivo, half outside at card size in IBM Plex Sans). The name span clips for its ellipsis, and `overflow:hidden` would cut the underline away (the design's own mock does, by accident). So the remembered-name span uses `overflow:clip; overflow-clip-margin:6px` (covers the worst measured case, 4px): it only widens what is painted, adds no layout or scrollable overflow (§8.3 check 2 stays clean) and the ellipsis still applies. Same rule at the card (16px), hero (22px, offset 5px) and row (14px) sizes. CLAUDE.md "Card indicators" defines the style as including the underline (R92, R128). Covered by a pixel probe in `fit-live.spec.ts`.
- The remembered tooltip's second line is dropped when the parts it would show are all null (`fromStartMs`, and `fromMapId` for the two match sources). If one is null, only the other part is shown. A `seen` name has no map, so its second line is the date alone.
- Eye-off SVG (v8:62): `<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M1.5 8s2.4-4.5 6.5-4.5S14.5 8 14.5 8 12.1 12.5 8 12.5 1.5 8 1.5 8z"/><circle cx="8" cy="8" r="2"/><path d="M2.5 13.5l11-11"/></svg>`.

### 3.5 Encounter slot (v8:71-75)

- Shown only when `!card.isSelf && !card.isSelfParty && encounters && (with+against) > 0`.
- Content: `display:flex; align-items:center; gap:3px; font-size:13px; font-weight:600; color:var(--text2); tab`. It holds the icon, then the count `with+against`. Tooltip `{with} with · {against} against`.
- **(R149)** The glyph is the design's own (v8:73, v8:196): the U+27F2 character `⟲` in a `<span class="glyph">` set in `font-family:"Segoe UI Symbol","Segoe UI",sans-serif; font-weight:400`, at the badge's own size (13px in the card, 14px in the hero) and colour. No SVG, no icon library. The app is Windows-only, so the system font is always there and nothing is downloaded. This supersedes the inline Lucide `rotate-ccw` SVG that R79 had chosen (that reason, the glyph existing only on Windows, does not hold for a Windows-only app). Covered by `EncounterBadge.test.ts`.
- **[C] (R129)** The card's badge has `margin-left:5px`, and only 3+ digit counts show it (1–2 digits keep the design's 36px slot, §3.3). The hero badge (§6.2) has no such margin.
- 3+ digit counts: see the slot's `min-width` in §3.3. `ingame-longnames` carries a `123` count (§8.2).

### 3.6 Rank blocks

**Current** (r1 c2, v8:81-87). No tooltip (the design's `tipCur` is `''`).
- Wrapper: `display:flex; align-items:center; gap:10px; min-width:0`.
- Icon: `<img tier rank.tier>` 40×40 `flex:none`.
- Text column: `flex-direction:column; gap:2px; min-width:0`.
  - Tier name 15px/600 `nowrap`.
  - Line 2: 12px/500 `--text2` `nowrap`: `<span 13px/600 --text tab>{rr}</span> RR`.
- `rank === null` **[C]**, or `rank.tier < 3` (Unrated and the unused tiers 1–2; **R129**): an empty 40×40 box (layout kept), then `—` 15px/600 `--text2`, with no line 2. A tier below 3 renders exactly like `null`, the same rule as the Prev row.
- The tier name's right edge must stay ≤ x 463 in card coordinates, because the party tab occupies x 464–485, y 22–42.

**Peak** (r2 c1, v8:89-94).
- Row: `display:flex; align-items:center; min-width:0; white-space:nowrap; font-size:12px; font-weight:500; color:var(--text2)`.
- `Peak` label: `width:50px; flex:none`.
- Then a tooltip group **[C]** wrapping, in order:
  - `<img tier peak.tier>` 18×18 `flex:none; margin-right:6px`;
  - tier name 13px/600 `--text` `margin-right:8px`;
  - the "when" span `tab; overflow:hidden; text-overflow:ellipsis`.
  - The design puts the tip on the icon only. Widening it to the group changes no pixels. The group is a real element: `display:flex; align-items:center; min-width:0`.
- When text: `peak.sinceActEndMs === null` → `This Act`; else `{peak.actLabel} · {span(sinceActEndMs)}` (`V26 A4 · 1mo`, `E9 A3 · 1y`).
- The when span keeps the design's ellipsis only as a safety net; it must never trigger. Widest real row: `Ascendant 3` + `V26 A4 · 11mo` (or `E10 A3 · 11mo`) needs 224 / 240 / 235px of the 241px column in Barlow / IBM Plex Sans / Archivo (measured with the bundled 600/500 files). `ingame-longnames` holds that row and §8.3 check 5 asserts it does not clip.
- `peak === null` → after the label, `—` 13px (weight 500 and `--text2` inherited).

**Prev** (r2 c2, v8:96-105). No tooltip. **[R125, user-delegated change to the design; matches the design's own Details hero Prev order, v8:214-218]** The design's card row is `Prev` (50px label) · icon · name. The card row is now icon · `Prev` · name, so the word sits next to its value. The Peak row is unchanged (50px label). The hero Prev row (§6.2) is unchanged.
- Row style as Peak, minus the 50px label: `display:flex; align-items:center; min-width:0; white-space:nowrap; font-size:12px; font-weight:500; color:var(--text2)`.
- Icon column: `width:40px; flex:none; display:flex; justify-content:center`. That is the width of the current-rank icon above it (§3.6 Current), so the two icons share one centre line.
  - `<img tier prevActTier>` 18×18 `flex:none; filter:grayscale(0.7); opacity:0.75` (unchanged), centred in the column: 11px + 18px + 11px.
- Label: `Prev` 12px/500 `--text2` `flex:none; margin-right:6px`. **(R126)** It starts right after the 40px icon column (x = 40 in the row), not at the rank-name column's x = 50: at x = 50 the widest `Prev` + `Ascendant 3` overflows the 158px column in the wider fonts.
- Name: tier name 13px/500 `--text2` `min-width:0; overflow:hidden; text-overflow:ellipsis`, right after the label. The ellipsis is a safety net only. The widest row (`Prev` + `Ascendant 3`) must still fit the 158px column unclipped in all three fonts (§8.3 check 5).
- `prevActTier === null` (or < 3) → the icon column stays, empty (40px, layout kept), then `Prev`, then `—` 13px (weight 500 and `--text2` inherited). The label is never hidden (§5.5).

### 3.7 Stat trio (r3 c1, v8:107-120)

- `align-self:end; display:flex`. 3 blocks, each `width:76px; display:flex; flex-direction:column`:
  - label 12px, line-height 14px, weight 500, `--text2`;
  - value 20px, line-height 24px, weight 600, `tab`.

| Label | Value | Tooltip |
|---|---|---|
| `K/D` | `kd.toFixed(2)` | `Last 5 competitive` **[bound, design defines `tipKd` but never binds it]** |
| `HS%` | `Math.round(hsPct×100) + '%'` | `Last 5 competitive` **[bound]** |
| `Win%` | `Math.round(actWinrate×100) + '%'` | structured W·L·games tip (§4.2) |

- `null` → `—`. Absent → 44×30 bar, radius 3, `--skel`, bottom-aligned in its 76px block.
- The trio is 38px tall in a 37.8px row. The 0.2px overflow matches the design and is clipped by the card.

### 3.8 Last 5 (r3 c2, v8:122-131, 562-570)

- Wrapper: `align-self:end; justify-self:start`.
- Inner column: `display:flex; flex-direction:column; gap:3px`.
  - Tiles row: `display:flex; gap:5px`. Width 130px.
  - Arrow row: only when `last5Style==='Arrow'`.
- **Order:** `card.last5` arrives newest first. Render `5 − n` empty tiles, then the entries **reversed**. The oldest real entry sits left of the others; the newest is the **rightmost** tile. Empty tiles are always on the LEFT.
- Tile: `position:relative; width:22px; height:22px; box-sizing:border-box; border-radius:4px; display:flex; align-items:center; justify-content:center; font-size:12px; font-weight:700`. `background`, `color` and `border` come from §1.5 (W, L, D, E; the newest uses the newest variant in Arrow style). The letter is `W`, `L` or `D`; empty tiles have no text.
- Underline style: the newest tile gets a child `position:absolute; left:5px; right:5px; bottom:-6px; height:2px; border-radius:1px; background:var(--text)`. There is no arrow row, so the block is 22px tall.
- Arrow row (v8:129):
  - `position:relative; height:7px; color:var(--arrow-col)`.
  - Line: `position:absolute; left:0; right:2px; top:3px; height:1px; background:currentColor`.
  - Chevron: `<svg width="5" height="7" viewBox="0 0 5 7" fill="none" stroke="currentColor" stroke-width="1.2" style="position:absolute;right:0;top:0;display:block"><path d="M0.6 0.6L4.2 3.5L0.6 6.4"/></svg>`.
  - Block height: 22 + 3 + 7 = 32px.
- `last5` null or `[]` → 5 empty tiles plus the arrow row. Absent → one 130×22 bar, radius 4, `--skel`, and no arrow row.
- Tile tooltip: §4.2. Empty tiles have no tooltip.

### 3.9 Party stripe and tab (v8:44-49, 572, 599-601)

Shown only when `card.party && !card.isSelf && !card.isSelfParty`. `col = --party-{group%4}`.

- Hit area: `position:absolute; right:0; top:0; bottom:0; width:10px; z-index:2`. Child: `position:absolute; right:0; top:0; bottom:0; width:3px`.
  - Background: confirmed → `col`; likely → `repeating-linear-gradient(180deg, col 0 6px, transparent 6px 10px)`.
- Tab:
  - `position:absolute; right:0; top:22px; z-index:3; width:21px; height:20px; box-sizing:border-box; padding-right:1px; border-radius:5px 0 0 5px; display:flex; align-items:center; justify-content:center`.
  - `background: linear-gradient(F,F), var(--card); box-shadow: inset 0 0 0 1.5px col; color: G`.
  - Confirmed: `F = col`, `G = var(--on-solid)`. Likely: `F = transparent`, `G = col`.
- Tab glyph (v8:48): `<svg width="12" height="12" viewBox="0 0 12 12" fill="currentColor" style="display:block"><circle cx="4.2" cy="3.8" r="2"/><circle cx="8.6" cy="4.4" r="1.6"/><path d="M0.9 10.8c0-2 1.4-3.3 3.3-3.3s3.3 1.3 3.3 3.3z"/><path d="M8 10.8c0-1.2-.3-2.2-.9-2.9.4-.3.9-.4 1.5-.4 1.5 0 2.5 1.2 2.5 3.3z"/></svg>`.
- Both the hit area and the tab carry the tooltip `Party · confirmed` / `Party · likely`.

### 3.10 Self outline, hover, click

- `card.isSelf` → last child: `position:absolute; inset:0; border:1px solid var(--self-frame); border-radius:6px; pointer-events:none`.
- The self card never shows a stripe or ⟲. Self-party cards show neither either, and get no outline.
- Hover: the inset `--line-hover` shadow from §3.2. There is no other hover change.
- Click anywhere on the card → details (tooltip triggers inside it do not stop the click). Skeleton cards and empty or dashed slots are not clickable.

---

## 4. Tooltips

### 4.1 Engine (v8:421-423, 516-532)

- One tooltip node inside `#app`: `position:absolute; z-index:50; width:max-content; max-width:260px; padding:7px 10px; background:var(--tip-bg); border:1px solid var(--line2); border-radius:6px; box-shadow:0 6px 18px rgba(0,0,0,0.45); font-size:13px; line-height:18px; font-weight:500; color:var(--text); white-space:pre-line; pointer-events:none`.
- Implement as a Svelte action `use:tip={text}` plus `use:tip={{text, overflowOnly:true}}`. It uses delegated `mouseover` on `#app` with `closest('[data-tip]')`, as the design does.
- Delay: **450ms**. The tip shows only if the pointer is still on the same trigger. Moving to another element (or to no trigger) clears it immediately. Leaving `#app`, clicking, navigating, or scrolling the Details/Match container (**[C]**, the design has no scrolling tip case) hides it.
- Overflow-only: for a `visible` name, check the name span (the trigger's first `span`) at hover time. Show only if `scrollWidth > clientWidth`.
- Position, relative to `#app` (`r` = trigger rect, `R` = app rect):
  - `x = clamp(136, 864, r.left + r.width/2 − R.left)`;
  - `below = r.bottom − R.top + 8`.
  - If `below > 720`: `top = r.top − R.top − 8` and `transform: translate(-50%,-100%)`.
  - Otherwise: `top = below` and `transform: translate(-50%,0)`.
- An empty text means no tooltip.
- Structured W/L tip (a text starting with `wl:`), row 1: `display:flex; gap:6px; align-items:baseline; white-space:nowrap; tab`. Spans in order:
  1. `{W}W` in `--win`, weight 600;
  2. `·` in `--text3`;
  3. `{L}L` in `--loss`, weight 600;
  4. `·` in `--text3`;
  5. `count(G, 'game')` (`1 game`, `53 games`, §4.3).
- **[C]** row 2: `This act` in `--text2` (13px/18px). Scope lives in the tooltip (requirements 2026-09-28; audit §2.8).

### 4.2 Catalogue

| Trigger | Screen | Text |
|---|---|---|
| Name wrapper, `visible` | card, hero | `{name}#{tag}`, only when the name is truncated |
| Name wrapper, `remembered`, `source: ownMatch` | card, hero | `Hidden name · known from your match\n{Map} · {Mon D, YYYY}` |
| Name wrapper, `remembered`, `source: otherMatch` | card, hero | `Hidden name · known from a past match\n{Map} · {Mon D, YYYY}` |
| Name wrapper, `remembered`, `source: seen` | card, hero | `Hidden name · seen before\n{Mon D, YYYY}` (the date alone; a name-service answer has no map) |
| Name wrapper, `hiddenAgent` / `hiddenPlayer` | card, hero | `Hidden name` |
| ⟲ badge | card, hero | `{with} with · {against} against` |
| Party hit area + tab | card | `Party · confirmed` / `Party · likely` |
| Peak group (icon + name + when) | card, hero | past act: `{actTitle}\n{actDates}`; current act: `{actLabel} · Current act\n{actDates}`. Examples: `VALORANT 2026 · Act 4\nJun 24 – Aug 25, 2026`, `V26 A5 · Current act\nAug 26 – Oct 20, 2026`. The design shows the dates only; the act name is added. |
| K/D block, HS% block | card | `Last 5 competitive`. A value that is still loading (`kd` / `hsPct` absent, the skeleton bar) carries no tip, the same as the APR group and Win% while unknown; a known-empty value (`null`, shown `—`) keeps it. |
| K/D block, HS% block | hero | `Last {N} competitive · this act`, `N = details.heroMatches` (R116) |
| Win% block | card, hero | `wl:{actWins}:{actGames−actWins}:{actGames}` rendered structured (§4.1). No tip if `actGames` is null. |
| Games block, W / L block | hero | `This act` **[bound]** (the design's `tipGames`; `tipWL` would repeat Games, so it uses the scope word too) |
| Last-5 tile (real) | card, hero | `{rrDelta} RR · {span}`, e.g. `+18 RR · 45m`, `−15 RR · 2h`, `0 RR · 4h`. `rrEarned === null` → `{Win|Loss|Draw} · {span}` (the design's unused `WORD` map). |
| Current rank, Prev | card, hero | none (the design's `tipCur` and `tipPrev` are `''`) |
| Avg party rank group | top bar | `Average party rank\nFrom {count(n, 'ranked player')}` |
| Recent row "time ago" text | details | `{h:mm AM/PM}` local, e.g. `9:14 PM` (v8:627) |
| Recent row rank icon | details | tier name, e.g. `Ascendant 1` |
| Encounter row map cell | details | `{Mon D} · {h:mm AM/PM} · {N min}`, with `, YYYY` after the day when the year is not the current year. Drop ` · {N min}` when the duration is unknown (v8:642). |
| Top-3 live icons | details | `In this match · {Agent}`; `In this match` when the live card has no agent yet |
| Loadout well | details | `{Tier} Edition` (e.g. `Premium Edition`) **[bound]** (the design computes `tierName` but never binds it); no tip for a default skin |
| Halves text | match | `First half · Second half` |
| `ACS` header | match | `Average combat score` |
| `+/−` header | match | `Kills minus deaths` |
| `ADR` header | match | `Average damage per round` |
| Scoreboard rank icon | match | tier name |

### 4.3 String formatters (`ui/src/lib/format.ts`)

- `span(ms)`: `Nm` (<1h, minimum 1m), `Nh` (<1d), `Nd` (<7d), `Nw` (<30d), `Nmo` (<365d), `Ny`. This is the existing function; keep it.
- `ago(ts)` = `span(now − ts) + ' ago'`.
- `rrDelta`: `+18`, `−15`, `0`. This is the existing function.
- `pct0(f)` **NEW** = `Math.round(f×100) + '%'`, or `—`. The card, hero, recent rows and scoreboard all use integer percentages. The existing 1-decimal `pct` is not used on these screens.
- `kd2(x)` = `x.toFixed(2)`, or `—` when `x` is null (R102).
- `count(n, word)` **NEW** = `${n} ${word}` when `n === 1`, else `${n} ${word}s` (`1 game`, `0 games`, `1 ranked player`). Used by the APR tip, the Win% tip and the Top 3 count (§6.7).
- `score(a,b)` = `${a}–${b}`, or `—` if either is null.
- `actDates(start,end)` **CHANGED**:
  - Format in UTC, as `Mon D – Mon D, YYYY`, or `Mon D, YYYY – Mon D, YYYY` across a new year.
  - The end date is the date of `actEndMs − 1`.
  - Examples: `Aug 26 – Oct 20, 2026`, `Nov 12, 2025 – Jan 6, 2026`, `Oct 22, 2024 – Jan 8, 2025`.
- `actTitle(actName)` **NEW**: `V26 ACT V` → `VALORANT 2026 · Act 5`; `EPISODE 9 ACT III` → `Episode 9 · Act 3` (roman I–VI → 1–6; `V{yy}` → `VALORANT 20{yy}`). If it can't parse, return the input.
- Dates in local time, `en-US`:
  - `dateLong` = `Sep 29, 2026`;
  - `dateShort` = `Sep 12` (plus `, YYYY` when not the current year). Used by the encounter rows, the encounter map tip and the Match header opened from an encounter row (R140, §6.10); a Match header opened from a recent row uses `dateLong`;
  - `time12` = `9:14 PM`;
  - `durationMin(ms)` = `Math.round(ms/60000) + ' min'`.
- `tierLabel(meta, t)`: the existing function (title-cases `DIAMOND 2` → `Diamond 2`). Tier < 3 → null. Meta pending or a missing entry: §5.8.

---

## 5. States built from the design's parts

### 5.1 Agent Select (`stage: 'pregame'`)

- Left column: the 5 ally cards (self first).
- Right column: always 5 **dashed empty slots [C]**. Each is `box-sizing:border-box; border:1px dashed var(--line2); border-radius:6px; background:transparent`. It fills its grid slot (485 × 139.8), has no children and no text, is not clickable and has no hover. This is the design's `E` box language (v8:552) at card size.
- No agent locked (`agentId === null`): the empty 40×40 raised square (§3.3).
- "Selected" means locked only (R99, user 2026-09-29; it reverses R71's hover call). The backend sets `agentId` only after the lock (`CharacterSelectionState == "locked"`). A hovered agent looks exactly like no pick: the empty square.
- Hidden player before the lock (hovering or not): `Hidden Player` in the hidden style. After the lock: the agent name in the hidden style. Everything else on the card renders normally.
- Top bar: the map group once `mapId` is known, the skeleton before that.
- Roster not yet received (`players.length === 0`): the left column shows 5 skeleton cards (§5.4). The right column still shows 5 dashed slots.

### 5.2 Lobby (`stage: 'menus'`), 1–5 players

- The left column holds self plus the party in slots 1…n. Cards are the fixed size (§3.1). The right column is empty.
- Self-party members have no stripe, no ⟲ and no outline. Only self has the outline.
- Average Party Rank in the top bar when n ≥ 2 (§2.3).
- Queueing changes nothing.
- **(R127)** No card has an agent here (none can exist in the Lobby): every card keeps the empty 40×40 `--raised` agent box that Agent Select draws before a lock (§5.1, §3.3). It is the same box in every stage, not a placeholder agent, and not the skeleton shape.
- Deathmatch or FFA in progress: the backend reports `menus` (R41), so the Lobby view shows.

### 5.3 Closed (`stage: 'closed'`)

- The route opens on `live` (R71: until My Stats exists).
- Only the user's own card, in slot 1 of the left column. Its values come from the DB; unknown values show `—`. It has the self outline.
- The top-bar right side is empty. Nothing else is on screen.
- **(R127)** The user's card keeps the empty 40×40 `--raised` agent box, as in Agent Select before a lock and in the Lobby (§3.3, §5.2): no agent exists while VALORANT is closed.
- Before the first `LiveState` arrives, slot 1 shows one skeleton card.
- **[C]** `closed` with `players: []` (first launch with VALORANT closed, before the user's PUUID was ever learned; the backend publishes no card): no card and **no skeleton**, both columns empty. A skeleton here would never resolve (plan Review Focus 2). The user's card appears once the client connects.

### 5.4 Loading

- **Full skeleton card** (v8:143-160; used when there is no card object yet), exact:
  - Box: `background:linear-gradient(90deg, var(--raised) 0 44px, var(--card) 44px); border-radius:6px; overflow:hidden`. Same grid, paddings and gaps as §3.2 (`box-sizing:border-box`).
  - r1 c1: flex, center, gap 10: a 40×40 radius-6 box, then a column with gap 7: a 120×13 bar and a 48×10 bar.
  - r1 c2: flex, center, gap 10: a 40×40 circle, then a column with gap 7: an 84×13 bar and a 40×10 bar.
  - r2 c1: a 176×10 bar, vertically centred.
  - r2 c2: a 124×10 bar.
  - r3 c1: `align-self:end`, flex, gap 28: three 44×30 bars.
  - r3 c2: `align-self:end`, one 130×22 bar with radius 4.
  - All bars are `--skel` with radius 3 unless stated.
- Where full skeleton cards appear:
  - `ingame` with `players.length===0` → 5 + 5.
  - `pregame` with `players.length===0` → 5 on the left, dashed slots on the right.
  - `menus` with `players.length===0` → 1 in slot 1.
  - Before the first LiveState → 1 in slot 1.
- **Per-field skeletons** (a real card with absent keys, R23): absent name, level, rank, stats and last5 as described in §3.3–3.8.
  - Absent `level` (R129) → a 48×10 bar (the skeleton card's level bar, radius 3, `--skel`) inline in the Lv line, replacing the whole `Lv …` text; the name column keeps its height (§3.3). A withheld level (`null`, Riot's HideAccountLevel, not the user's own card) is not loading: it shows `Lv —` (R100, §5.5).
  - Absent `peak` → the whole Peak row becomes a 176×10 bar, including the label, as in the design's skeleton.
  - Absent `prevActTier` → the whole Prev row becomes a 124×10 bar.
  - The encounter slot and party stripe are not tri-state; they simply appear later.
- The top-bar map skeleton is described in §2.3. There is no text in any skeleton.

### 5.5 Missing values

- `null` → `—` in the value's own font, size and colour slot.
- Exceptions:
  - Rank null → §3.6.
  - Hero `W / L` null → `—` in `--win`, ` / ` in `--text3`, `—` in `--loss` (as the design renders it, v8:595).
- Never hide the label of a missing value.

### 5.6 Unranked or no-history player

- Current rank `—`, Peak `—`, Prev `—`, K/D, HS% and Win% `—`, last-5: 5 empty tiles plus the arrow. No Win% tooltip.
- Details: hero stats `—`; Recent shows a single 52px row with `—` in the Map column (14px/500 `--text2`) and no Show more. Top 3 and Loadout are described in §6.7 and §6.8.

### 5.7 Stage → layout

| stage | Left column | Right column | Top-bar right |
|---|---|---|---|
| closed | self (nothing when `players: []`, §5.3) | — | — |
| menus | self + party (1–5) | — | APR if ≥2 |
| pregame | 5 allies | 5 dashed slots | map (or skeleton) |
| ingame | 5 allies | 5 enemies | map (or skeleton) |

### 5.8 Meta lookups (names from `invoke('meta')`) **[C]**

Map names, agent names (hidden-agent cards, Details `shortName`, Top-3 and tooltips) and tier names come from `Meta`. `invoke('meta')` can still be pending, and on a first start with valorant-api unreachable an id can be missing from a loaded table.

- **Meta pending:** each looked-up text renders as a skeleton bar at that text's size, radius 3, `--skel`, no text:
  - top-bar map name: 52×12 (the §2.3 map skeleton bar; the image still loads by id);
  - card tier name 84×13, Peak/Prev tier names 60×10, hero tier names 90×13, recent/encounter/scoreboard tier names 60×10;
  - hidden-agent name (card, hero, Top 3, scoreboard): the §3.4 120×13 name bar, eye-off icon kept;
  - Details `shortName` and team label `{name}'s team`: 60×10.
- **Loaded, id missing:** `—` in that text's own style (for `hiddenAgent` the eye-off icon and italic style stay, so the name is still never shown).
- **Tooltips:** a missing part is dropped (`In this match` without the agent; a remembered tip shows only the date (always so for `seen`); the rank-icon tip is empty, i.e. no tip). No tooltip ever contains `undefined` or `null`.
- Images are keyed by id and never wait for meta.
- Scene `ingame-nometa` (§8.2) covers the pending state; the miss state is unit-tested (`live.test.ts`, `tooltip.test.ts`).

---

## 6. Player Details and Match Details

### 6.1 Scroll container and Back (v8:163-168, 359-364)

- Container: `flex:1; min-height:0; overflow-y:auto; scrollbar-width:thin; scrollbar-color:var(--scroll) transparent; scrollbar-gutter:stable; padding:14px 8px 22px 16px`. No horizontal overflow.
- Back row: `display:flex; margin-bottom:12px`.
- Back button:
  - `display:flex; align-items:center; gap:7px; height:32px; padding:0 13px 0 10px; box-sizing:border-box; border:1px solid var(--line2); border-radius:5px; background:var(--card); font-size:13px; font-weight:500; cursor:pointer`. Hover: `background:var(--raised)`.
  - Icon: `<svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.6"><path d="M7.5 2.5L4 6l3.5 3.5"/></svg>`, then `Back`.

### 6.2 Hero (v8:170-240)

- Box: `position:relative; background:var(--card); border-radius:8px; overflow:hidden; margin-bottom:20px`.
- Art (v8:171-174): `position:absolute; left:0; top:0; bottom:0; width:84px; overflow:hidden`. Two layers:
  1. `background-image:cardLarge; background-size:130% auto; background-position:50% 22%; background-repeat:no-repeat; opacity:0.6; filter:saturate(0.65)`
  2. `--strip-fade`
- Agent: `position:absolute; left:12px; top:18px; width:56px; height:56px; border-radius:7px; background:var(--raised); box-shadow:0 0 0 2px var(--card)`. No agent → an empty box with the same style.
- Top grid: `position:relative; display:grid; grid-template-columns:minmax(0,1fr) auto auto auto; align-items:center; column-gap:30px; min-height:56px; padding:18px 22px 0 96px`.

| Col | Content |
|---|---|
| 1 | Column, gap 4, `min-width:0`.<br>**Name wrapper:** `min-width:0; display:flex; align-items:baseline; gap:6px`. Variants as §3.4 at name 22px/600 (hidden: 22px/500 italic `--text-soft`), tag 15px/500 `--text2`, eye-off 16×16, underline offset 5px. Same tooltips.<br>**Meta row:** `display:flex; align-items:center; font-size:12px; font-weight:500; color:var(--text2); nowrap`. It holds `<span width:64px flex:none>Lv <span 14px --text tab>{level}</span></span>`, then the ⟲ badge: `display:flex; align-items:center; gap:3px; font-size:14px; font-weight:600; tab`, colour inherited `--text2`, SVG 13×13, count; tooltip as the card. The badge follows the §3.5 visibility rule. |
| 2 | Current rank: flex, center, gap 12. `<img tier>` 48×48, then a column with gap 2, `nowrap`: tier name 18px/600; line 2 12px/500 `--text2` with `<span 14px/600 --text tab>{rr}</span> RR`. Null: empty 48 box, then `—` 18px/600 `--text2`. No tooltip. |
| 3 | Peak: flex, center, gap 10 (tooltip group = the whole block **[C]**). `<img tier>` 28×28, then a column with gap 2, `nowrap`. Row: baseline, gap 6: `Peak` 12px/500 `--text2`, tier name 15px/600. Below: when-text 12px/500 `--text2` `tab`. Null: `Peak` then `—` 15px/600. |
| 4 | Prev: flex, center, gap 10. `<img tier>` 24×24 (`grayscale(0.7)`, `opacity 0.75`), present only if a prev tier exists. Then a baseline row, gap 6, `nowrap`: `Prev` 12px/500 `--text2`, name 15px/500 `--text2`; or `—` **15px/500** `--text2` (weight pinned to 500, see §1.7). No tooltip. |

- Stats strip (v8:223-239): `position:relative; display:grid; grid-template-columns:repeat(5,minmax(0,1fr)) 196px; margin:18px 0 0 96px; border-top:1px solid var(--line)`.
  - Cells 1–5: cell 1 `padding:12px 16px 14px 0`; cells 2–5 `padding:12px 16px 14px 18px; border-left:1px solid var(--line)`.
  - Each cell is a column: label 12px/14px 500 `--text2`; value 22px/27px 600 `tab`.
  - Order:
    1. `K/D` (`kd2`), from `details.heroKd` (R116);
    2. `HS%` (`pct0`), from `details.heroHs` (R116);
    3. `Win%` (`pct0 actWinrate`);
    4. `Games` (`actGames`);
    5. `W / L`: `<W in --win>` + `<' / ' --text3 weight 500>` + `<L in --loss>`, `nowrap`.
  - Sources (R116, user 2026-09-29): `Win%`, `Games` and `W / L` stay act-wide, taken from the MMR data (`card.actWinrate`, `card.actWins`, `card.actGames`). `K/D` and `HS%` are the player's most recent up to 20 competitive matches of the current act, the same match set Top 3 Teammates fetches (zero extra requests). `details.heroMatches` is the number of act matches used. With no current-act competitive match, `heroKd` and `heroHs` are `null` and both cells show `—`. The Live card is unchanged: its K/D and HS% stay on the last 5 competitive (§3.7).
  - Tooltips per §4.2. Absent values → a 44×27 bar, radius 3, `--skel`, under the label.
- Last-5 cell: `display:flex; align-items:center; padding:12px 20px 14px 20px; border-left:1px solid var(--line)`.
  - Same component as §3.8 at hero size: tiles 26×26, radius 4, 13px/700; tile gap 6; column gap 4; underline `left/right:6px; bottom:-6px`; the arrow row is unchanged (7px).
  - The tiles row is 154px wide.

### 6.3 Body grid (v8:242)

- `display:grid; grid-template-columns:minmax(0,1fr) 296px; gap:16px; align-items:start`.
- Left: `display:flex; flex-direction:column; gap:20px; min-width:0` → Recent, then Encounters.
- Right: the same → Top 3 Teammates, then Loadout.
- Section: `display:flex; flex-direction:column; gap:8px`. Header 14px/600. Panel: `background:var(--card); border-radius:8px; overflow:hidden`.
- Row separators: row 0 `border-top:none`; others `border-top:1px solid var(--line)`.

### 6.4 Recent Competitive Matches (v8:244-275)

- Shared grid `G = 176px 28px 24px 84px minmax(0,1fr) 48px 44px 10px`, `column-gap:12px`.
- Header row: `display:grid; grid-template-columns:G; align-items:center; height:30px; padding:0 14px 0 8px; font-size:12px; font-weight:500; color:var(--text2); border-bottom:1px solid var(--line); nowrap`. Cells: `Map` | `` | `` | `Score` | `K / D / A` (center) | `K/D` (right) | `HS%` (right) | ``.
- Row: `display:grid; grid-template-columns:G; column-gap:12px; align-items:center; height:52px; padding:0 14px 0 8px; cursor:pointer`. Hover: `background:var(--hover-row)`. Click → `match(matchId, puuid, 'recent')`.

| Cell | Spec |
|---|---|
| Map | flex, center, gap 10, `min-width:0`. `<img mapList>` 64×36 `object-fit:cover; border-radius:4px; flex:none`. Column with gap 2, `min-width:0`: map name 14px/600 `nowrap` ellipsis; `ago(startMs)` 12px/500 `--text2` `nowrap; align-self:flex-start` (tooltip `time12`). |
| Agent | `<img agent>` 28×28, `border-radius:5px; background:var(--raised)` (empty box if null) |
| Rank | `<img tier>` 24×24 (tooltip tier name). Tier null or <3 → empty 24 box, no tooltip. |
| Score | flex, center, gap 8. Chip: 20×20 `flex:none; box-sizing:border-box; border-radius:4px`, centred, 12px/700, box from §1.5 (W/L/D). Then `score(roundsWon, roundsLost)` 14px/600 `tab` `nowrap`. |
| K / D / A | inner grid `24px 14px 18px 14px 24px`, `justify-content:center; align-items:center; font-size:14px; font-weight:500; tab`. Cells: k (right), `/` (center, `--text3`), d (center), `/` (center, `--text3`), a (left). |
| K/D | right, 14px/600 `tab`, `kd2` |
| HS% | right, 14px/500 `tab`, `pct0` |
| › | `<svg width="7" height="10" viewBox="0 0 7 10" fill="none" stroke="currentColor" stroke-width="1.5" style="display:block;justify-self:end;color:var(--text3)"><path d="M1.5 1l4 4-4 4"/></svg>` |

- Rows are newest first. Show 3 at first.
- The RR change is not shown (the design computes it but doesn't render it).

### 6.5 Show more (v8:272-274)

- Shown when `!more && recent.length > 3`: `height:34px; display:flex; align-items:center; justify-content:center; border:1px solid var(--line2); border-radius:6px; font-size:13px; font-weight:500; color:var(--text-soft); cursor:pointer`. Hover: `background:var(--card)`. Text `Show more`.
- Click → `more=true`. Show `recent.slice(0,8)` and remove the button. There is no "show less" and no count picker.

### 6.6 Encounters (v8:277-313)

- Shown when `!card.isSelf && encounters.length > 0`.
- Grid `E = minmax(0,1fr) 144px 58px 144px 80px 10px`, `column-gap:12px`.
- Header row: same style as §6.4. Cells: `Map` | `{shortName}` (`overflow:hidden; text-overflow:ellipsis`) | `` | `You` | `Your result` | ``.
- `shortName`:
  - `visible` / `remembered` → name;
  - `hiddenAgent` → agent name;
  - `hiddenPlayer` → `Hidden Player`.
- Rows: `height:50px`, other styles as §6.4. Click → `match(matchId, puuid, 'encounter')`. All shared matches, newest first, no cap.

| Cell | Spec |
|---|---|
| Map (tooltip §4.2) | flex, center, gap 10, `min-width:0`. `<img mapList>` 56×32 cover, radius 4. Column with gap 2: map 14px/600 `nowrap`; `dateShort` 12px/500 `--text2` `nowrap` `tab`. |
| Them | flex, center, gap 6, `min-width:0`. `<img agent themAgentId>` 26×26 radius 5 `--raised`; `<img tier themTier>` 20×20; tier name 13px/500 `nowrap` ellipsis. Null tier → empty 20 box and `—`. |
| Side | `With` / `Against`, 13px/600 `--text2` `nowrap` |
| You | the same as Them, using `selfAgentId` and `selfTier` |
| Your result | flex, center, gap 8: 20×20 chip (`result`, user side), then `score(roundsWon, roundsLost)` 14px/600 `tab` (user side) |
| › | as §6.4 |

### 6.7 Top 3 Teammates (v8:317-335)

- Row: `display:flex; align-items:center; gap:10px; min-height:30px; padding:8px 14px` **[C]** (a one-line row is the design's 46px), with the §6.3 separators. Rows are not clickable.
- Name block: `flex:1; min-width:0; display:flex; align-items:baseline; gap:5px; flex-wrap:wrap`. Full names wrap; they never truncate.
  - **[C]** The name span also gets `overflow-wrap:anywhere` (the design has none). A 16-char name with no spaces is 192–208px at 14/600, and the block is only ~132px wide beside the live icons (54) and the count (62); without it the name overflows under them. Short names render exactly as the design.
  - `visible`: name 14px/600, tag `#TAG` 13px/500 `--text2`.
  - `remembered`: eye-off 14, name 14px/600 dotted underline (offset 4), tag.
  - `hiddenAgent` / `hiddenPlayer`: eye-off 14, then the agent name or `Hidden Player` at 14px/500 italic `--text-soft`.
  - In the wrapping block the eye opens the name's first line (the first line is indented 19px and the eye sits in the indent), and a wrapped `remembered` name has line-height 1.3 so its underline clears the next line **[C]**.
  - The backend already applies R18. No tooltips on names here.
- Live indicator (v8:325-330):
  - Shown when the teammate's puuid is a card in the current `LiveState.players` whose `name.kind === 'visible'` (self counts as visible).
  - `flex:none; display:flex; align-items:center; gap:6px`: `<img agent>` 26×26 radius 5 `--raised` (an empty box if the live card has no agent), then `<img tier>` 22×22 (live `rank.tier`; empty box if null).
  - Tooltip per §4.2.
- Count: `flex:none; width:62px; text-align:right; font-size:12px; font-weight:500; color:var(--text2); nowrap`. It holds `<span 15px/600 --text tab margin-right:4px>{count}</span>games` (`game` when the count is 1 **[C]**, §4.3 `count`).
- Empty list → one 46px row with `—` 14px/500 `--text2`.

### 6.8 Loadout (v8:336-352)

- Row: `display:flex; align-items:center; gap:14px; height:62px; padding:0 14px 0 8px`, with separators. Fixed order: Vandal, Phantom, Operator, Sheriff.
- Well:
  - `position:relative; width:124px; height:48px; flex:none; display:flex; align-items:center; justify-content:center; border-radius:5px; overflow:hidden`.
  - `background: linear-gradient(0deg, rgba(T,0.26) 0%, rgba(T,0.07) 55%, rgba(T,0) 100%), var(--raised)`.
  - `box-shadow: inset 0 -2px 0 rgba(T,0.85)`.
  - Tier icon: `<img contentTier tierId>` `position:absolute; left:5px; top:5px; width:13px; height:13px; opacity:0.9`.
  - Skin image: `<img chroma chromaId>` `position:relative; display:block; max-width:108px; max-height:38px; object-fit:contain`.
- Text column: gap 2, `min-width:0`. Weapon name 12px/500 `--text2`. Then `{skinName}` 14px/600 `nowrap` ellipsis, containing `<span 13px/500 --text2> · {chromaName}</span>`. The span is omitted when `chromaName` is null (the base chroma).
- Content tiers, a fixed UI table keyed by tier uuid:

| Tier | uuid | T |
|---|---|---|
| Select | `12683d76-48d7-84a3-4e09-6985794f0445` | `#5a9fe2` |
| Deluxe | `0cebb8be-46d7-c12a-d306-e9907bfc5a25` | `#009587` |
| Premium | `60bca009-4182-7998-dee7-b8a2558dc369` | `#d1548d` |
| Exclusive | `e046854e-406c-37f4-6607-19a9ba8426fc` | `#f5955b` |
| Ultra | `411e4a55-4e59-7757-41f0-86a53f101bb5` | `#fad663` |

- `tierId` null (default or battle-pass skin): `background:var(--raised)`, no shadow, no icon.
- Weapon entry tri-state (R97, the R23 pattern):
  - key absent (the skin name is still resolving): the §6.9 loading row, i.e. a 124×48 `--skel` well (radius 5) and a 90×13 bar in the text column, with no text;
  - `null` (no skin, or it can't be known): an empty raised well, the weapon name, and `—` 14px/600 `--text2`;
  - a `WeaponSkin`: as above.
  - The backend keeps a key absent only while its name fetch runs, so the skeleton always resolves to a skin or `—`.
- **[C] (R83, R90)** Other players' loadouts are readable only In Game (core-game) and for Agent Select allies (pre-game); their Details opened in Lobby or Closed shows all 4 weapons as `—`. The user's own loadout is read once per connect from `pd …/personalization/v2/players/{puuid}/playerloadout` (R90; read-only, one call; the last answer is kept for Closed), so the user's own Details shows their skins outside a match too.
- Skin names resolve from valorant-api independently of the 120s Riot details sync, so a loadout that arrives after the first open still gets its names on the next open (plan Task 15).

### 6.9 Details states

- Self: no Encounters section and no ⟲. Top 3 is shown.
- Hidden or remembered player: the hero name uses the §3.4 variants. Everything else is shown normally.
- Loading (after the click, before `player_details` resolves) **[C]**:
  - Back, plus the hero box with: a 56×56 `--skel` radius-7 agent box, a 160×18 bar, a 48×10 bar, a 48-circle and 90×13 bar, a 28-circle and 90×13 bar, a 24-circle and 90×13 bar for Prev **[C]**, a 154×26 bar for the last-5 row **[C]** (both keep the hero grid's columns), and five 44×27 bars in the stats strip, each under an empty 14px label line, so the skeleton hero is 160px like the loaded one **[C]**;
  - Recent: the header row plus 3 rows, each with a 64×36 map box and a 90×13 bar;
  - Top 3: 3 rows with a 120×13 bar;
  - Loadout: 4 rows with a 124×48 `--skel` well and a 90×13 bar.
  - Section headers are rendered; they are labels, not status text.
- A loaded Details whose loadout has an absent weapon key draws that one row as the loading row above (§6.8, R97).
- Card fields inside the hero that are still absent use per-field bars (§6.2).
- A `details` event for the same puuid replaces the data in place and keeps `more` and the scroll position.

### 6.10 Match Details (v8:358-419)

Container and Back as §6.1. Back → details.

- **Header** (v8:365-378):
  - Box: `display:flex; align-items:center; gap:16px; background:var(--card); border-radius:8px; padding:12px 22px 12px 12px; margin-bottom:14px`.
  - Map: `<img mapList>` 150×56 cover, radius 5, `flex:none`.
  - Text column: `flex:1; min-width:0`, gap 3. Map name 20px/600. `when` 12px/500 `--text2` `nowrap` `tab` = `{queueLabel} · {date} · {time12} · {durationMin}` (the duration part is dropped when unknown). **[R140]** The date follows the entry: a match opened from an encounter row uses `dateShort(startMs, now)` (`Sep 12`, plus `, YYYY` only for a previous year; the design strips the year, v8:642 and v8:685), e.g. `Competitive · Sep 12 · 11:18 PM · 41 min`; a match opened from a recent row keeps `dateLong` (v8:53), e.g. `Competitive · Sep 29, 2026 · 9:14 PM · 38 min`. Code: `whenText(v, from, nowMs)` in `ui/src/lib/details.ts`.
  - Right: flex, center, gap 14.
    - Chip 28×28, `box-sizing:border-box; border-radius:5px`, 14px/700; box = `teams[0].result`.
    - Column `align-items:flex-end`: score 28px/32px 600 `tab` = `score(teams[0].roundsWon, teams[1].roundsWon)`; halves 12px/500 `--text2` `tab` = `{a.firstHalf}–{b.firstHalf} · {a.roundsWon−a.firstHalf}–{b.roundsWon−b.firstHalf}` (tooltip `First half · Second half`; the second half includes overtime, as in v8:659). Halves are hidden when `firstHalf` is null.
- `queueLabel` (a UI table):

| queueId | Label |
|---|---|
| `competitive` | `Competitive` |
| `unrated` | `Unrated` |
| `swiftplay` | `Swiftplay` |
| `spikerush` | `Spike Rush` |
| `deathmatch` | `Deathmatch` |
| `hurm` | `Team Deathmatch` |
| `ggteam` | `Escalation` |
| `onefa` | `Replication` |
| `premier` | `Premier` |
| `''` | `Custom` |
| other | the raw id, title-cased |

- **Scoreboard panel**: `background:var(--card); border-radius:8px; overflow:hidden`.
  - Grid `S = minmax(0,1fr) 52px 104px 48px 52px 48px`, `column-gap:14px`.
  - Header row: `display:grid; grid-template-columns:S; align-items:center; height:30px; padding:0 18px 0 12px; font-size:12px; font-weight:500; color:var(--text2); border-bottom:1px solid var(--line); nowrap`. Cells: `Player` | `ACS` (right, tooltip) | `K / D / A` (center) | `+/−` (right, tooltip) | `ADR` (right, tooltip) | `HS%` (right).
  - Team header (one per team): `display:flex; align-items:center; gap:10px; height:36px; padding:0 12px; background:var(--team-bg)`. `border-top`: none for team 0, `1px solid var(--line)` for team 1.
    - A 20×20 chip (the team's result box);
    - `roundsWon` 15px/600 `tab` (`—` if null);
    - label 12px/500 `--text2`: team 0 → `Your team` when `perspective==='self'`, else `{focusShortName}'s team`; team 1 → `Opponents`.
  - Player row: `position:relative; display:grid; grid-template-columns:S; column-gap:14px; align-items:center; height:42px; padding:0 18px 0 12px; border-top:1px solid var(--line)`. Not clickable.
    - `isFocus && !isSelf` → a child `position:absolute; left:0; top:0; bottom:0; width:3px; background:var(--accent)`.
    - `isSelf` → a child `position:absolute; inset:0; border:1px solid var(--self-frame); pointer-events:none`.
    - Player cell: flex, center, gap 10, `min-width:0`. `<img agent>` 28×28 radius 5 `--raised`; `<img tier>` 22×22 `flex:none` (tooltip tier name; empty box if null). Then a name box: `min-width:0; display:flex; align-items:baseline; gap:5px`.
      - `visible`: name 14px/600 `nowrap` ellipsis + tag 13px/500 `--text2` `flex:none`.
      - `hiddenAgent` / `hiddenPlayer`: eye-off 14 (`align-self:center; --text2`) + name 14px/500 italic `--text-soft` `nowrap`.
      - `remembered`: eye-off 14 + name 14px/600 dotted underline (offset 4) + tag.
    - ACS: right, 14px/600 `tab`, integer.
    - K / D / A: the §6.4 inner grid. Each of k, d and a shows `—` when null (R124); the two `/` separators stay.
    - +/−: right, 14px/600 `tab`, `k−d` as `+N` / `−N` / `0`, coloured `--win` if >0, `--loss` if <0, `--text2` if 0. `—` in `--text2` when `kills` or `deaths` is null (R124), never `0`.
    - ADR: right, 14px/500 `tab`, integer.
    - HS%: right, 14px/500 `tab`, `pct0`. Null values → `—` in every numeric cell (R124: a player with no stats row shows `—` in ACS, K / D / A, +/−, ADR and HS%, never 0; ADR is also `—` when only the damage is unknown). The rank icon does not depend on the stats row: `tier` comes from the player's competitive_tier (R81), so a ranked player with no stats row keeps the icon beside the `—` cells.
  - Sort: each team's rows by ACS descending, with null ACS last (v8:668).
- **Perspective** (backend sets it; the UI only renders):
  - `teams[0]` is the user's team when the user played in the match (`perspective:'self'`). Otherwise it is the focus player's team (`perspective:'focus'`).
  - The header chip, score and halves are from `teams[0]`'s side. This generalises v8:688 and follows the "Your result = user's side" rule.
- **Hidden names (R74, R93, R145, R150):** each row's `name` is a `DisplayName` computed by the backend:
  - in every view, a player hidden on the live screen now follows the same rule as their card: `remembered` when the DB knows the name from any source (the user's own finished matches, other stored matches, or a name-service answer from when the player was visible, R145), else `hiddenAgent` (R93), so the card, Top 3 and the scoreboard agree. The tooltip names the source (§4.2);
  - otherwise, a match the user played → names as match-details gives them;
  - otherwise (a match the user didn't play) → names as Riot returns them (R143). A row Riot left blank and whose player is not hidden on the live screen shows the name stored in the players table as a plain name; with none, the remembered rule above, else the agent name;
  - This replaces audit §2.12's "any player flagged incognito in that match": match-details carries no incognito flag (`endpoints.rs` `MatchPlayer`), so that rule can't be implemented.
  - **Name-service (R150):** off the live stages (Match Details, Top 3, encounter rows, backfilled own matches) the app may ask name-service, batched, throttled and cached in `players`, for the participants of a match that have no known name. Riot itself blanks anyone who currently hides their name, so nothing hidden is revealed. During the live stages a live incognito puuid is never sent to any endpoint. Decided, not built yet in v0.2.0 (the code reads only the players table for such rows).
- **No-team modes** (Deathmatch): a single team block. The team header shows no chip, `—` for rounds, and the label `Players`. The header chip is omitted, the score is `—`, and halves are hidden.
- **Loading [C]:** the header box with a 150×56 `--skel` radius-5 box and a 120×18 bar; the scoreboard header row; 2 team headers with 5 rows each (28-box, 22-circle, 120×13 bar, and 30×13 bars aligned as each column's numbers are; the K / D / A bar is centred, as that column is in the design).

---

## 7. Data needs

### 7.1 Wire types (TS mirror of model.rs; camelCase; R23 tri-state: absent = loading, `null` = missing)

The block below writes nested types inline for reading. `ui/src/lib/types.ts` exports each of them as a named type: `Stage`, `Result`, `Team`, `DisplayName`, `Rank`, `Peak`, `Last5Entry`, `PartyMark`, `EncounterCount`, `PlayerCard`, `LiveState`, `MatchSummary`, `WeaponSkin`, `Loadout`, `EncounterDetailRow`, `TeammateSummary`, `PlayerDetails`, `MatchTeamView`, `ScoreRow`, `MatchDetailsView`, `Meta`. New names match the Rust structs (`EncounterCount`, `TeammateSummary`, `WeaponSkin`, `MatchTeamView`, `ScoreRow`; ui-lane's `Encounters` is renamed `EncounterCount`). The existing UI names `Rank`, `Peak` and `Last5Entry` stay (Rust `RankInfo`, `PeakInfo`, `LastFiveEntry`).

```ts
type Stage = 'closed' | 'menus' | 'pregame' | 'ingame';
type Result = 'win' | 'loss' | 'draw';
type DisplayName =
  | { kind: 'visible'; name: string; tag: string }
  | { kind: 'remembered'; name: string; tag: string;
      source: 'ownMatch' | 'otherMatch' | 'seen';                        // R145
      fromMapId: string | null;                                          // null for 'seen'
      fromStartMs: number | null }                                       // 'seen': when last confirmed
  | { kind: 'hiddenPlayer' }
  | { kind: 'hiddenAgent'; agentId: string };

interface LiveState { stage: Stage; mapId: string | null; players: PlayerCard[] }

interface PlayerCard {
  puuid: string;
  name?: DisplayName | null;            // tri-state (R34) — ui-lane types.ts has it required: FIX
  level?: number | null;                // tri-state — FIX. null when the player hides it (HideAccountLevel), except on the user's own card (isSelf), which always carries the level (R100)
  cardId: string;
  agentId: string | null;               // set only after the lock (R99; a hover leaves it null)
  rank?: { tier: number; rr: number } | null;
  prevActTier?: number | null;
  peak?: { tier: number; actLabel: string; actName: string; sinceActEndMs: number | null;
           actStartMs: number; actEndMs: number } | null;
  kd?: number | null;
  hsPct?: number | null;                // 0..1
  actWinrate?: number | null;           // 0..1, MUST use NumberOfWinsWithPlacements — FIX
  actWins?: number | null;              // NEW tri-state
  actGames?: number | null;             // NEW tri-state (losses = games − wins)
  last5?: { result: Result; rrEarned: number | null; startMs: number }[] | null; // newest first — ui-lane has `win:boolean`: FIX
  party: { group: number; confirmed: boolean } | null;
  encounters: { with: number; against: number } | null;
  team: 'ally' | 'enemy';
  isSelfParty: boolean;
  isSelf: boolean;                      // NEW
}

interface MatchSummary {                // PlayerDetails.recent; the My Stats last-5 row is the same Rust struct (that page is not built in this spec)
  matchId: string; mapId: string;       // mapId is '' when the map is unknown
  startMs: number; result: Result;
  roundsWon: number | null; roundsLost: number | null; agentId: string | null;
  kills: number | null; deaths: number | null; assists: number | null; kd: number | null;   // R102: null (never a counted 0) when the match has no stats row; the row shows —
  acs: number | null;                   // R102: null without a stats row
  hsPct: number | null;                 // 0..1; R102: null without a stats row (also null when no shot landed)
  rrEarned: number | null;
  tier: number | null;                  // NEW: player's rank DURING that match: match_players.competitive_tier (Riot's at-match value), else comp_updates.tier_after (R81; one rule for recent rows, encounters and the scoreboard)
}

interface WeaponSkin {                  // NEW (replaces the bare chroma uuid)
  chromaId: string; skinName: string; chromaName: string | null; tierId: string | null;
}
interface Loadout { vandal?: WeaponSkin | null; phantom?: WeaponSkin | null;
                    operator?: WeaponSkin | null; sheriff?: WeaponSkin | null }   // CHANGED; tri-state per weapon (R97): absent = name resolving, null = none/unknown

interface EncounterDetailRow {
  matchId: string; mapId: string /* '' when unknown */; startMs: number; with: boolean;
  result: Result;                       // NEW, user side (replaces selfWon)
  roundsWon: number | null; roundsLost: number | null;   // NEW, user side
  durationMs: number | null;            // NEW
  themAgentId: string | null; themTier: number | null;   // NEW
  selfAgentId: string | null; selfTier: number | null;   // NEW
}

interface PlayerDetails {
  card: PlayerCard; recent: MatchSummary[] /* ≤8 newest first */;
  topTeammates: { puuid: string; name: DisplayName; count: number }[];
  loadout: Loadout; encounters: EncounterDetailRow[] /* newest first */;
  heroKd: number | null;                // (R116) K/D over the most recent ≤20 current-act competitive matches; null when there are none
  heroHs: number | null;                // (R116, R120) HS% over the same set, a 0..1 fraction like every other HS% field (card.hsPct, MatchSummary.hsPct, ScoreRow.hsPct), never an integer percent; `pct0` renders it; null when there are none
  heroMatches: number;                  // (R116) how many act matches heroKd / heroHs use (0..20); feeds the hero tooltip
}

interface MatchDetailsView {            // NEW — invoke('match_details', { matchId, focusPuuid })
  matchId: string; mapId: string /* '' when unknown */; queueId: string; startMs: number;
  durationMs: number | null;
  perspective: 'self' | 'focus';
  teams: {                              // [0] = perspective team; 1 entry for no-team modes
    teamId: string; result: Result | null; roundsWon: number | null; firstHalf: number | null;
    players: { puuid: string; name: DisplayName; agentId: string | null; tier: number | null;
               kills: number | null; deaths: number | null; assists: number | null;   // R124: null (never a counted 0/0/0) when the player has no stats row; the row shows — (golden match_details_view.json, row enemy-3: kills, deaths, assists, acs, adr and hsPct all null). `tier` is independent of the stats (R81): it comes from the player's competitive_tier, so a ranked player with no stats row still has one and the rank icon draws next to the — cells
               acs: number | null; adr: number | null; hsPct: number | null;   // hsPct 0..1; null without a stats row; adr is also null when only the damage is unknown
               isSelf: boolean; isFocus: boolean }[];   // sorted by ACS desc
  }[];
}

interface Meta { agents: Record<string,{name:string}>; maps: Record<string,{name:string}>;
                 tiers: Record<string,{name:string;color:string}> }   // unchanged
```

### 7.2 Field → source map

| Screen element | JSON read | Status |
|---|---|---|
| Top bar map name / image | `live.mapId` → `meta.maps[id].name`, `assetUrl('mapList', id)` | existing |
| Avg party rank | `live.players[].rank.tier` (stage `menus`), `meta.tiers` | existing (UI computes) |
| Card art strip / hero art | `card.cardId` → `assetUrl('cardLarge', id)` | **NEW kind** |
| Agent icon | `card.agentId` → `assetUrl('agent', id)` | existing |
| Name / tag / variants | `card.name` | existing, plus **NEW** `fromMapId`/`fromStartMs` |
| Hidden agent name | `card.name.agentId` → `meta.agents` | existing |
| ⟲ count + tip | `card.encounters.{with,against}` | existing |
| Lv | `card.level` | existing |
| Current rank | `card.rank.{tier,rr}`, `meta.tiers` | existing |
| Peak row + tip | `card.peak.{tier,actLabel,actName,sinceActEndMs,actStartMs,actEndMs}` | existing |
| Prev row | `card.prevActTier` | existing |
| K/D, HS% (Live card) | `card.kd`, `card.hsPct` (last 5 competitive) | existing |
| K/D, HS% (Details hero) | `details.heroKd`, `details.heroHs` over `details.heroMatches` current-act competitive matches, at most 20 (R116; `heroHs` is a 0..1 fraction, R120); tooltip `N = heroMatches` | **NEW** |
| Win% | `card.actWinrate` | existing, **FIX** placement wins |
| Win% tip, hero Games, W / L | `card.actWins`, `card.actGames` | **NEW** |
| Last 5 tiles + tips | `card.last5[].{result,rrEarned,startMs}` | existing |
| Party stripe / tab | `card.party.{group,confirmed}` | existing |
| Self outline | `card.isSelf` | **NEW** |
| Suppression of stripe / ⟲ | `card.isSelf`, `card.isSelfParty` | existing + NEW |
| Recent row | `details.recent[]`: `mapId, startMs, agentId, tier, result, roundsWon, roundsLost, kills, deaths, assists, kd, hsPct, matchId` (`kills, deaths, assists, kd, hsPct` are `null` for a match with no stats row and render `—`, R102) | existing, plus **NEW** `tier` |
| Encounters row | `details.encounters[]`: `mapId, startMs, durationMs, themAgentId, themTier, with, selfAgentId, selfTier, result, roundsWon, roundsLost, matchId` | **NEW** (all but `matchId`/`mapId`/`startMs`/`with`) |
| Top 3 | `details.topTeammates[].{puuid,name,count}` plus a live join on `live.players[].{puuid,name.kind,agentId,rank.tier}` | existing |
| Loadout | `details.loadout.{vandal,phantom,operator,sheriff}.{chromaId,skinName,chromaName,tierId}` (each key tri-state, R97) → `assetUrl('chroma')`, `assetUrl('contentTier')` | **NEW** shape + kind |
| Match header | `mdv.{mapId,queueId,startMs,durationMs}`, `teams[0/1].{result,roundsWon,firstHalf}` | **NEW** command |
| Scoreboard | `mdv.teams[].players[].{name,agentId,tier,kills,deaths,assists,acs,adr,hsPct,isSelf,isFocus}`, `mdv.perspective` (`kills, deaths, assists, acs, adr, hsPct` are `null` for a player with no stats row and render `—`, R124; `adr` is also `null` when the damage is unknown; `tier` is independent of the stats, R81, so the rank icon still draws) | **NEW** |

### 7.3 Backend deltas this spec depends on (for the plan; no code here)

1. `PlayerCard.isSelf` from `card_for` (state.rs:224 already computes it).
2. `actWins`/`actGames`, and Win% from `NumberOfWinsWithPlacements` (endpoints.rs `SeasonalInfo`, `trim_mmr`, force an MMR refetch for old snapshots).
3. `DisplayName::Remembered { fromMapId, fromStartMs }` (repo `known_name` also returns the map and start).
4. Asset kinds:
   - `cardLarge`: playercard `largeArt`, downscaled to 160px wide (the strip shows 66px at 1×, the hero 109px; 160 covers 150% DPI).
   - `contentTier`: `displayIcon`, downscaled to 32px.
   - `chroma` downscale target: 256px wide (shown at ≤108×38).
5. `MatchSummary.tier`.
6. `Loadout` → `WeaponSkin` (a skins, chromas and content-tier table in assets.rs). `skinName` = the skin display name minus the trailing weapon word. `chromaName` = the text inside `(Variant N …)`, or null for the base chroma. Also: fetch `pregame_loadouts` during Agent Select, keyed only by each entry's `Subject` (real capture: techchrism `valorant-api-docs`, `valorant-api-types/tests/endpoint-responses/pre-game-loadouts/singleplayer.json`; entries without a subject are skipped, never matched by position). Skin names resolve from valorant-api on Details open, independent of the 120s Riot sync (§6.8). The user's own loadout comes from `pd …/personalization/v2/players/{self}/playerloadout` once per connect (R90; `Guns[].ID/SkinID/ChromaID`, techchrism `PlayerLoadout.ts`). Each weapon serializes tri-state (R97).
7. `EncounterDetailRow` extensions (agents are already stored; ranks, duration and scores need item 8).
8. Migration 2 (R89; migration 1 is frozen, and there is no self-heal):
   - `matches.game_length_ms INTEGER`;
   - `match_teams.rounds_won_h1 INTEGER` (rounds 1–12 won);
   - `match_players.competitive_tier INTEGER`;
   - `match_players.damage INTEGER` (the sum of `roundResults[].playerStats[].damage[].damage`).
   - Parse `matchInfo.gameLengthMillis`, `players[].competitiveTier`, `roundResults[].winningTeam`/`roundNum`.
9. New command `match_details(matchId, focusPuuid) -> MatchDetailsView` (perspective, sorting, hidden-name rule, no-team modes).

### 7.4 Contract test descriptors (`ui/src/lib/contract.shape.ts`, Task 16b; test-only)

The descriptors are written from the Rust goldens under `crates/core/tests/fixtures/contract/`, not from `types.ts`, and `contract.test-d.ts` holds them against `types.ts`. Two differences in form from the plan's interface text were accepted in the Task 16b fix round (`d478473`):
- The four `DisplayName` variants live in a separate export, `displayNameShapes`, keyed by the `kind` tag, not in `shapes`. `DisplayName` is a union, so it has no single `ShapeOf<DisplayName>`. The walker picks the variant by `kind`, and a type test asserts the variant set equals `DisplayName['kind']`.
- `Desc` has two optional fields beyond `{ kinds, optional?, shape? }`: `enum` (a closed string set, keyed into the exported `enums` table) and `fraction` (a 0..1 number, R5 and R120). Both are additive and pinned by the type tests.

The contract tests pin a no-stats `ScoreRow` as `kills`, `deaths`, `assists`, `acs`, `adr` and `hsPct` null, and leave `tier` free of that rule (R81, §7.1).

---

## 8. Verification (Playwright, Chromium, viewport exactly 1000×800, DEV demo feed)

### 8.1 Harness

- Scenes are selected by `?scene=` (R12). Routes: `&route=details&puuid=…`, `&route=match&match=…&from=recent|encounter`. Theme: §1.8 URL params.
- Freeze the page clock at `2026-09-29T21:59:00Z` with `timezoneId: 'UTC'` and `locale: 'en-US'` (`page.clock.setFixedTime`). Only at 21:59 do the design mock's `45m / 2h / 4h ago` rows agree with its `9:14 / 7:02 / 5:20 PM` times (v8:451, v8:472).
- Viewport 1000×800 at `deviceScaleFactor: 1`. A second file runs checks 2, 5 and 6 at `deviceScaleFactor: 1.5` on `ingame-longnames` and `pregame` in all 3 fonts, because the user's Windows display is likely scaled 125–150% and the tight edges (tier name ≤ 463, the Peak row, fractional 139.8px cards) must hold there too.
- `page.route('https://media.valorant-api.com/**')` serves from a local cache dir when the file exists, otherwise passes through.
- Screenshot every scene to `ui/tests/shots/<scene>[__<variant>].png`. Look at each one before reporting.
- Fonts come from bundled files. Assert that no request goes to `fonts.googleapis.com` or `fonts.gstatic.com`.
- Design-ref visual gate (plan Task 17, `ui/tests/visual-live.spec.ts`): at most 0.02% of the 800 000 pixels (160 px) may differ at a per-pixel YIQ threshold of 0.2. The card's Prev row (R125) is masked in both renders as an approved deviation, and the two masked rects must be identical, so the mask can never grow past the row. The contract test descriptors are §7.4.

### 8.2 Scenes

| Scene | Content |
|---|---|
| `closed` | self card only |
| `closed-empty` | `stage: 'closed'`, `players: []` (first launch, self unknown): no card, no skeleton (§5.3) |
| `lobby1` … `lobby5` | self + party; `lobby2` has 1 unranked member (APR from 1: `From 1 ranked player`); `lobby3` has 1 unranked member (APR from 2); `lobby4` has one member with rank still loading |
| `lobby2-loading` | 2 players, none ranked yet, one `rank` absent (APR skeleton, §2.3) |
| `lobby2-unranked` | 2 players, both `rank: null` (APR empty box + `—`) |
| `pregame` | allies: 2 locked, 1 hovered and 1 unpicked (both `agentId: null` on the wire, R99), 1 hidden player who hasn't locked (`Hidden Player`); map known |
| `pregame-nomap` | as `pregame`, but `mapId` null |
| `pregame-loading` | `players: []` |
| `ingame` | the design's roster (Fahad self; Drake + Nova party with each other, confirmed; Mara + Kilo likely; Rook remembered; Cypher hidden agent; encounters on Vesper, Mara, Kilo, Rook) |
| `ingame-longnames` | every visible name `WWWWWWWWWWWWWWWW#WWWWW`; every tier and peak tier `Ascendant 3`; every peak `V26 A4 · 11mo` (the design's V-year form, 13 chars), one peak `E10 A3 · 11mo`; one non-self, non-party card with a 3-digit ⟲ (`70 with · 53 against` → `123`) |
| `ingame-parties4` | 4 groups (ally 2 × likely/confirmed, enemy 2) |
| `ingame-partial` | per-field loading mix (name, level, rank, peak, prev, stats and last5 each absent on some card) |
| `ingame-unranked` | a card with every value null and `last5: []`; one with 2 matches (3 empty tiles) |
| `ingame-loading` | `players: []`, `mapId: null` |
| `ingame-nometa` | the `ingame` roster with `meta` still pending (§5.8: skeleton bars for the map name, tier names and the hidden agent name) |
| `ingame-remembered` | the `ingame` roster with three remembered cards, one per source (R145): Rook `ownMatch`, Nyx `otherMatch`, Kilo `seen` (no map). Drives the three remembered tooltips of §4.2 (`tips-live.spec.ts`) |
| `details` | Mara (Show more visible, 3 encounters, 1 live teammate) |
| `details-longnames` | a focus player named `WWWWWWWWWWWWWWWW#WWWWW` (hero name truncates, tag whole; encounters header ellipsizes); Top 3 = three 16-char `W` names, one of them live (they wrap, never overlap the icons or the count); loadout skin lines `Protocol 781-A · Sunrise`-length (ellipsis) |
| `details-partial` | the hero card with per-field loading: name, level, rank, peak, stats and last5 each absent (§6.2 bars), recent rows present; one loadout weapon absent (resolving) and one `null` (R97) |
| `details-more` | after clicking Show more (8 rows) |
| `details-self` | Fahad (no Encounters) |
| `details-hidden` | Cypher (hiddenAgent), Rook (remembered) |
| `details-empty` | a no-history player |
| `details-loading` | details skeleton |
| `match` | from a recent row (focus perspective) |
| `match-enc` | from an encounter row (self perspective, focus bar + self frame) |
| `match-hidden` | contains `hiddenAgent` and `remembered` rows |
| `match-dm` | a no-team mode |
| `match-longnames` | 10 scoreboard rows with 16-char `W` names (visible and remembered) and the focus player's team label `WWWWWWWWWWWWWWWW's team` |
| `tips` | each tooltip in §4.2 hovered in turn (a screenshot per tip); the Live part runs on `ingame`, `ingame-longnames`, `lobby3` and `pregame` (for `Hidden Player` → `Hidden name`) |

### 8.3 Global assertions (every scene)

1. `#app` bounding box = 0,0,1000,800. `document.scrollingElement.scrollWidth ≤ 1000` and `scrollHeight ≤ 800`.
2. **Live scenes:** no element inside the Live grid has `scrollHeight > clientHeight + 1` or `scrollWidth > clientWidth + 1`, except name spans and, **(R129)**, the Underline option's bar `.uline`, which the design hangs 6px below the newest tile (v8:125; §3.8), so it overflows its tile by construction. It is decoration inside the card and is never clipped: the geometry test `geometry: ingame [Underline]` (§8.4) asserts the bar's bottom stays inside the card. No scrollbar anywhere.
3. **Details / Match:** the scroll container's `scrollWidth ≤ clientWidth` (vertical scroll allowed).
4. Every rendered text node has computed `font-size ≥ 12px`.
5. Clipping: every text element has `scrollWidth ≤ clientWidth`, except the allowed ellipsis spans (each tagged `data-ellipsis` in the markup so the check can find them):
   - Riot-ID name spans (card, hero, scoreboard);
   - the encounters header name (§6.6), the loadout skin line (§6.8) and the recent-row map name (§6.4), which keep the design's own ellipsis.

   An allowed span that is truncated must have computed `text-overflow: ellipsis`; a truncated Riot-ID name's tag sibling must be fully visible. Everything else is strict, including the Peak row's when span and the Prev name (they have a CSS ellipsis only as a safety net; `ingame-longnames` in all 3 fonts proves it never triggers).
6. No overlap within a card: the rects of the name wrapper, ⟲ slot, Lv line, rank text, Peak row, Prev row, the 3 stat blocks and the last-5 block are pairwise disjoint (a 3-digit ⟲ count included). The party tab rect does not intersect any text rect.
7. `innerText` of `#app` does not match `/offline|last updated|beta|saved data|loading|unavailable|searching|queue/i`.
8. Computed `font-family` of `#app` starts with the selected family, and `document.fonts.check('600 16px <family>')` is true.
9. Hidden-name integrity: no `hiddenAgent` or `hiddenPlayer` card contains the fixture's real name string anywhere in the DOM, including attributes. This is a regression check only (R96): the DEV scenes carry no real hidden names, so the real guard is the backend's tests (plan Tasks 14–15).

### 8.4 Geometry assertions (Live, default theme; tolerance ±0.5px)

| Item | Expected |
|---|---|
| Top bar height (incl. border) | 49 |
| Active tab indicator | 2px tall, bottom edge at y 49, colour `--accent` |
| Map image | 116×32, right edge at x 982 |
| Card i, left column | x 10, y `59 + 147.8i`, 485 × 139.8 (all stages, including `lobby1` and `closed`) |
| Right column / dashed slot | x 505, same y / size |
| Agent icon (card-relative) | (56,12) 40×40 |
| Rank icon | (313,12) 40×40 |
| Peak row / Prev row | y 62, h 18; x 56 / 313 |
| Prev icon (R125) | 18×18 (absent when there is no prev tier), centre x 333 (313 + 20), the same centre line as the rank icon |
| Stat trio | bottom at 127.8, blocks 76px wide starting x 56 |
| Last-5 tiles (Arrow) | 5 × 22×22, x 313…443, y 95.8…117.8; arrow row y 120.8…127.8 |
| Last-5 tiles (Underline) | y 105.8…127.8, underline under the rightmost tile only; the bar is 2px tall, its top at tile bottom + 4 (`bottom:-6px`), and its bottom edge stays inside the card (R129) |
| ⟲ slot | x 261…297 |
| Party tab | x 464…485, y 22…42; stripe x 482…485 |
| Tier name right edge | ≤ 463 for `Ascendant 3` in Barlow, IBM Plex Sans and Archivo |

### 8.5 Per-scene assertions

- `pregame`:
  - exactly 5 cards on the left and 5 dashed slots on the right;
  - each slot has `innerText === ''`, `border-style: dashed`, and the same rect size as a card;
  - the hovered and the unpicked cards have no `img` in the agent box (R99);
  - the hidden card that hasn't locked shows `Hidden Player`, italic.
- `lobby1`–`lobby5`:
  - n cards on the left, nothing on the right;
  - APR is present iff n ≥ 2;
  - self-party cards have no stripe and no ⟲; only self has the outline.
- `lobby2`: the APR tip is `Average party rank\nFrom 1 ranked player`.
- `lobby2-loading`: APR = label + 32×32 `--skel` circle + 52×12 bar, no text besides the label. `lobby2-unranked`: label + empty 32×32 box + `—`.
- `closed`: exactly 1 card; the top-bar right side is empty. `closed-empty`: no card, no skeleton, nothing in the grid.
- `ingame-nometa`: no text node contains `undefined` or `null`; the top-bar map name, card tier names and the hidden-agent name are skeleton bars (§5.8).
- Last-5 order: in `ingame`, Fahad `last5 = [W,L,W,W,L]` (newest first) renders left→right `L W W L W`, and the rightmost tile uses the newest style. A 2-match player renders 3 dashed tiles first.
- `ingame-parties4`: 4 distinct stripe colours equal to `--party-0…3`. Likely stripes are gradients, confirmed ones solid. No stripe on self or self-party.
- `ingame-longnames`: every name span is truncated with an ellipsis, every tag is intact, the ⟲ slot is not overlapped, the `123` count's rect does not intersect its tag's rect, no Peak row clips, and hovering a name shows the full `Name#TAG` after ≥450ms (none at 400ms).
- `details-longnames`: the hero name truncates with the tag whole; Top 3 names are never cut and the name rect doesn't intersect the live icons or the count, and the live teammate's long name wraps inside its name block; the encounters header name and the skin lines end in an ellipsis.
- `match-longnames`: at the design's widths the scoreboard name box is about 490px (v8:381) and the longest Riot ID about 290px, so no name truncates: every name shows whole with its tag on one 42px line, and no row is taller than 42px. A name longer than the box would still take the ellipsis with the tag whole (§8.3 check 5).
- `details-partial`: each absent hero field is a bar (§6.2), with no text inside. The absent loadout weapon is a `--skel` well and a 90×13 bar with no text; the `null` one is an empty raised well with the weapon name and `—` (R97).
- Hover and click (interaction tests): a card's hover shadow is `inset 0 0 0 1px` in `--line-hover`; Back, recent rows, encounter rows and Show more change background on hover (§6.1, §6.4, §6.5); clicking a dashed slot, a skeleton card or the `My Stats` tab changes nothing (route unchanged, `cursor: default`); a click hides an open tooltip.
- `tips`:
  - each tip's text matches §4.2 exactly;
  - the tip rect lies inside `#app`;
  - its horizontal centre is `clamp(136, 864, triggerCentre)`;
  - a trigger whose bottom is > 712 (so `bottom + 8 > 720`) flips the tip above;
  - the Win% tip shows `31W · 22L · 53 games` / `This act` with the win and loss colours.
- `details`:
  - 3 recent rows and a visible Show more; after the click, 8 rows and no button;
  - Encounters headers are exactly `Map | Mara | | You | Your result`;
  - the live teammate shows agent + rank icons.
- `details-self`: no Encounters section.
- `match`:
  - rows in each team are sorted by ACS descending;
  - team 0's label is `Mara's team`, team 1's `Opponents`;
  - 1 focus bar (3px `--accent`);
  - halves text = `a1–b1 · a2–b2`.
- `match-enc`: team 0 is `Your team`; 1 self frame and 1 focus bar in different rows.
- `match-dm`: 1 team block, header score `—`, no halves.

### 8.6 Option matrix (screenshot + §8.3 checks)

- `ingame-longnames` and `details`, each in `font=Barlow`, `IBM Plex Sans` and `Archivo`.
- `ingame` in:
  - `last5=Underline`;
  - `fill=Solid`;
  - `results=Accent / Red`;
  - `results=Accent / Gray`;
  - `tone=Cool`;
  - `tone=Warm`;
  - each of the 4 accents.
- `ingame-parties4` in each of the 4 accents: the 4 rendered stripe colours (computed `background-color` of confirmed stripes, gradient colour of likely ones) equal that accent's §1.6 row, in the DOM, not just in `themeVars`.
- Unit test (`theme.test.ts`): `themeVars(DEFAULT_THEME)` equals the Neutral column of §1.4 and §1.5, and the §1.6 table holds for all 4 accents.
- `deviceScaleFactor: 1.5` (§8.1): checks 2, 5 and 6 on `ingame-longnames` and `pregame` in all 3 fonts.
