// The ASCII studio's model (design v2, TrackerApp.dc.html: the 5×5 font 1849–1868, painting / trim / copy 1870–1916,
// the view model in `extraVals3` 1972–2010). Pure: Ascii.svelte holds the state and draws it.
//
// The editor is a fixed canvas, W × 15 cells (W = max(64, art width + 8)), with the text art centred in it. Paint is a
// map keyed by the offset from the canvas centre ("dx,dy"), so strokes stay where they were drawn relative to the
// art when the text (and so W) changes. What gets copied or saved is the canvas trimmed to its content.

export type Effect = "flat" | "wide" | "shadow";
export type Tool = "draw" | "erase";
/** Paint strokes: "dx,dy" from the canvas centre → the character there (" " for an erased cell). */
export type PaintMap = Record<string, string>;

/** The input's limits: letters, digits, space and `! ? . -`, at most 10. */
export const MAX_TEXT = 10;
export const CANVAS_H = 15;
export const CANVAS_MIN_W = 64;

/** The editor's state when the page opens (the prototype's defaults). */
export const INITIAL = { text: "CLUTCH", char: "█", effect: "shadow" as Effect, tool: "draw" as Tool, brush: "█" };

/** Every character's name: the radio buttons' labels and tooltips. */
export const CHAR_NAMES: Record<string, string> = {
  "█": "Full block",
  "▓": "Dark shade",
  "▒": "Medium shade",
  "░": "Light shade",
  "#": "Hash",
  "@": "At sign",
  "*": "Asterisk",
  "+": "Plus",
  "·": "Middle dot",
  "─": "Line",
};

/** The text's fill characters. */
export const FILL_CHARS = ["█", "▓", "▒", "#", "@", "*"] as const;
/** The brush grid (2 rows of 5). */
export const BRUSHES = ["█", "▓", "▒", "░", "#", "@", "*", "+", "·", "─"] as const;

export const EFFECTS: readonly { value: Effect; label: string; tip: string }[] = [
  { value: "flat", label: "Flat", tip: "One character per pixel" },
  { value: "wide", label: "Wide", tip: "Two characters per pixel, closer to square in most fonts" },
  { value: "shadow", label: "Shadow", tip: "Wide, plus a ░ drop shadow" },
];

/** The 5×5 font: rows joined by "|", `#` = ink. Narrow glyphs (I, 1, !, ., space) are narrower than 5. */
export const FONT: Readonly<Record<string, string>> = {
  A: " ### |#   #|#####|#   #|#   #",
  B: "#### |#   #|#### |#   #|#### ",
  C: " ####|#    |#    |#    | ####",
  D: "#### |#   #|#   #|#   #|#### ",
  E: "#####|#    |#### |#    |#####",
  F: "#####|#    |#### |#    |#    ",
  G: " ####|#    |#  ##|#   #| ####",
  H: "#   #|#   #|#####|#   #|#   #",
  I: "###| # | # | # |###",
  J: "  ###|   # |   # |#  # | ##  ",
  K: "#   #|#  # |###  |#  # |#   #",
  L: "#    |#    |#    |#    |#####",
  M: "#   #|## ##|# # #|#   #|#   #",
  N: "#   #|##  #|# # #|#  ##|#   #",
  O: " ### |#   #|#   #|#   #| ### ",
  P: "#### |#   #|#### |#    |#    ",
  Q: " ### |#   #|# # #|#  # | ## #",
  R: "#### |#   #|#### |#  # |#   #",
  S: " ####|#    | ### |    #|#### ",
  T: "#####|  #  |  #  |  #  |  #  ",
  U: "#   #|#   #|#   #|#   #| ### ",
  V: "#   #|#   #|#   #| # # |  #  ",
  W: "#   #|#   #|# # #|## ##|#   #",
  X: "#   #| # # |  #  | # # |#   #",
  Y: "#   #| # # |  #  |  #  |  #  ",
  Z: "#####|   # |  #  | #   |#####",
  "0": " ### |#  ##|# # #|##  #| ### ",
  "1": " # |## | # | # |###",
  "2": "#### |    #| ### |#    |#####",
  "3": "#### |    #| ### |    #|#### ",
  "4": "#   #|#   #|#####|    #|    #",
  "5": "#####|#    |#### |    #|#### ",
  "6": " ### |#    |#### |#   #| ### ",
  "7": "#####|    #|   # |  #  |  #  ",
  "8": " ### |#   #| ### |#   #| ### ",
  "9": " ### |#   #| ####|    #| ### ",
  " ": "  |  |  |  |  ",
  "!": "#|#|#| |#",
  "?": "### |   #| ## |    | #  ",
  "-": "    |    |####|    |    ",
  ".": " | | | |#",
};

/** What the text input keeps of a typed value: the allowed characters, at most 10. */
export function filterText(raw: string): string {
  return raw.replace(/[^a-z0-9 !?.\-]/gi, "").slice(0, MAX_TEXT);
}

/**
 * The text drawn in the 5×5 font, as a grid of characters. Glyphs are one blank column apart. `wide` doubles every
 * column; `shadow` is `wide` plus a ░ copy one cell down and right, under the ink. Empty text → [].
 */
export function asciiGrid(text: string, ch: string, fx: Effect): string[][] {
  const glyphs = [...(text || "").toUpperCase()].map((c) => (FONT[c] ?? FONT["?"]!).split("|"));
  if (!glyphs.length) return [];
  let g = [0, 1, 2, 3, 4].map((r) => [...glyphs.map((x) => x[r]!).join(" ")].map((c) => c === "#"));
  if (fx !== "flat") g = g.map((r) => r.flatMap((v) => [v, v]));
  if (fx === "shadow") {
    const h = g.length + 1;
    const w = g[0]!.length + 1;
    return Array.from({ length: h }, (_, y) =>
      Array.from({ length: w }, (_, x) => (g[y]?.[x] ? ch : g[y - 1]?.[x - 1] ? "░" : " ")),
    );
  }
  return g.map((r) => r.map((v) => (v ? ch : " ")));
}

/** The text art as copyable text: rows joined by "\n", trailing spaces dropped. */
export function asciiText(text: string, ch: string, fx: Effect): string {
  return asciiGrid(text, ch, fx)
    .map((r) => r.join("").replace(/\s+$/, ""))
    .join("\n");
}

/** A grid cut to its content: blank rows above and below dropped, the common left indent and trailing spaces removed. */
export function trimArt(grid: readonly (readonly string[])[]): string {
  const rows = grid.map((r) => r.join("").replace(/\s+$/, ""));
  while (rows.length && !rows[0]) rows.shift();
  while (rows.length && !rows[rows.length - 1]) rows.pop();
  const indents = rows.filter(Boolean).map((r) => r.match(/^ */)![0].length);
  const ind = indents.length ? Math.min(...indents) : 0;
  return rows.map((r) => r.slice(ind)).join("\n");
}

export interface Canvas {
  /** The cells, H rows of W. */
  grid: string[][];
  W: number;
  H: number;
  /** The centre cell paint offsets are measured from. */
  cx: number;
  cy: number;
  /** The whole canvas as text (what the editor shows). */
  out: string;
  /** The canvas trimmed to its content (what Copy / Save take); "" when blank. */
  art: string;
  /** The editor's font size in px. */
  fontPx: number;
}

/** The editor font: the canvas fills about 770px, clamped to 7–20px. */
export const canvasFontPx = (W: number): number => Math.max(7, Math.min(20, Math.floor(770 / (W * 0.6))));

/** The text art centred on the canvas, with the paint strokes on top. */
export function composeCanvas(text: string, ch: string, fx: Effect, paint: PaintMap): Canvas {
  const ag = asciiGrid(text, ch, fx);
  const aw = ag.length ? ag[0]!.length : 0;
  const ah = ag.length;
  const W = Math.max(CANVAS_MIN_W, aw + 8);
  const H = CANVAS_H;
  const cx = Math.floor(W / 2);
  const cy = Math.floor(H / 2);
  const grid = Array.from({ length: H }, () => Array<string>(W).fill(" "));
  const x0 = Math.floor((W - aw) / 2);
  const y0 = Math.floor((H - ah) / 2);
  ag.forEach((r, y) => r.forEach((c, x) => (grid[y0 + y]![x0 + x] = c)));
  for (const [k, v] of Object.entries(paint)) {
    const [dx, dy] = k.split(",").map(Number) as [number, number];
    const x = cx + dx;
    const y = cy + dy;
    if (y >= 0 && y < H && x >= 0 && x < W) grid[y]![x] = v;
  }
  return { grid, W, H, cx, cy, out: grid.map((r) => r.join("")).join("\n"), art: trimArt(grid), fontPx: canvasFontPx(W) };
}

/** The paint key of canvas cell (x, y). */
export const paintKey = (x: number, y: number, c: Pick<Canvas, "cx" | "cy">): string => `${x - c.cx},${y - c.cy}`;

/** The canvas cell under a pointer, from the canvas element's box. */
export function cellAt(
  clientX: number,
  clientY: number,
  rect: { left: number; top: number; width: number; height: number },
  c: Pick<Canvas, "W" | "H">,
): [number, number] {
  return [Math.floor(((clientX - rect.left) / rect.width) * c.W), Math.floor(((clientY - rect.top) / rect.height) * c.H)];
}

/** The paint map with cell (x, y) set to `value` (" " erases); the same map when the cell is off the canvas. */
export function paintAt(map: PaintMap, x: number, y: number, c: Pick<Canvas, "W" | "H" | "cx" | "cy">, value: string): PaintMap {
  if (x < 0 || y < 0 || x >= c.W || y >= c.H) return map;
  const k = paintKey(x, y, c);
  if (map[k] === value) return map;
  return { ...map, [k]: value };
}

/**
 * A drawing loaded into the editor as paint (the text goes empty): centred on the 64 × 15 canvas the empty text
 * gives, so it shows where it was drawn.
 */
export function loadRaw(raw: string): PaintMap {
  const ls = raw.split("\n");
  const w = Math.max(...ls.map((l) => l.length));
  const h = ls.length;
  const W = CANVAS_MIN_W;
  const H = CANVAS_H;
  const cx = Math.floor(W / 2);
  const cy = Math.floor(H / 2);
  const x0 = Math.floor((W - w) / 2);
  const y0 = Math.floor((H - h) / 2);
  const m: PaintMap = {};
  ls.forEach((l, y) =>
    [...l].forEach((c, x) => {
      if (c !== " ") m[`${x0 + x - cx},${y0 + y - cy}`] = c;
    }),
  );
  return m;
}

/** A gallery tile's font: the art fits about 190 × 96px, clamped to 4–13px. */
export function tileFontPx(raw: string): number {
  const ls = raw.split("\n");
  const cw = Math.max(1, ...ls.map((l) => l.length));
  return Math.max(4, Math.min(13, Math.floor(190 / (cw * 0.6)), Math.floor(96 / ls.length)));
}

/** "W × H" of a piece of art (columns × rows), for the status line. */
export function artSize(art: string): { cols: number; rows: number } {
  const lines = art ? art.split("\n") : [];
  return { cols: lines.length ? Math.max(...lines.map((l) => l.length)) : 0, rows: lines.length };
}

/** A built-in gallery item: text art (loads into the text controls) or a raw drawing (loads as paint). */
export type Preset =
  | { title: string; text: string; char: string; effect: Effect; raw: string }
  | { title: string; raw: string; text?: undefined };

const CROSSHAIR = [
  "       ┃       ",
  "       ┃       ",
  "               ",
  "━━━━   •   ━━━━",
  "               ",
  "       ┃       ",
  "       ┃       ",
].join("\n");

/** The prototype's built-in gallery, in its order. */
export const PRESETS: readonly Preset[] = [
  ...([
    ["ACE", "█", "shadow"],
    ["GG", "▓", "wide"],
    ["1V5", "@", "wide"],
    ["DEFUSED", "#", "flat"],
    ["OPD1", "█", "shadow"],
  ] as const).map(([text, char, effect]) => ({ title: text, text, char, effect, raw: asciiText(text, char, effect) })),
  { title: "Crosshair", raw: CROSSHAIR },
];
