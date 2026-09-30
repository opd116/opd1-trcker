// Theme (spec §1): the design's options, its colour math, and the CSS variables components read.
// Pure except `applyTheme`. Components read only the variables, never these tables.

export type ThemeOptions = {
  tone: "Neutral" | "Cool" | "Warm";
  accent: string;
  results: "Green / Red" | "Accent / Red" | "Accent / Gray";
  resultFill: "Tinted" | "Solid";
  font: "Barlow" | "IBM Plex Sans" | "Archivo";
  last5Style: "Arrow" | "Underline";
};

/** The values shipped until a Settings page exists. */
export const DEFAULT_THEME: ThemeOptions = {
  tone: "Neutral",
  accent: "#e3a54f",
  results: "Green / Red",
  resultFill: "Tinted",
  font: "Barlow",
  last5Style: "Arrow",
};

type ToneColors = {
  bg: string;
  card: string;
  raised: string;
  line: string;
  line2: string;
  text: string;
  textSoft: string;
  text2: string;
  text3: string;
  gray: string;
};

/** v8:442-446, verbatim. */
export const TONES: Record<ThemeOptions["tone"], ToneColors> = {
  Neutral: { bg: "#0e1013", card: "#171a1f", raised: "#20242a", line: "#23272e", line2: "#2c3139", text: "#e4e1db", textSoft: "#c9c9c7", text2: "#a3a7ae", text3: "#6f747c", gray: "#8d929a" },
  Cool: { bg: "#0b0f14", card: "#141920", raised: "#1d242e", line: "#212833", line2: "#2a323e", text: "#e0e4ea", textSoft: "#c3c9d1", text2: "#9ea7b3", text3: "#6a7380", gray: "#8b95a2" },
  Warm: { bg: "#100f0d", card: "#1a1815", raised: "#24211d", line: "#2a2722", line2: "#35312b", text: "#e7e2d9", textSoft: "#cbc5bb", text2: "#aaa396", text3: "#77716a", gray: "#978f84" },
};

/** v8:447. */
export const FONTS: Record<ThemeOptions["font"], string> = {
  Barlow: "'Barlow', sans-serif",
  "IBM Plex Sans": "'IBM Plex Sans', sans-serif",
  Archivo: "'Archivo', sans-serif",
};

export const ACCENTS = ["#e3a54f", "#4fc3ae", "#7fb1ef", "#e58a9a"] as const;

const GREEN = "#5fbf8b";
const LOSS = "#dc7a6e";
const ON_SOLID = "#14120e";

// ── colour math (spec §1.3, v8:503-505) ─────────────────────────────────────────
const rgb = (h: string) => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16));

export const mix = (a: string, b: string, t: number): string =>
  "#" +
  rgb(a)
    .map((v, i) => Math.round(v + (rgb(b)[i]! - v) * t).toString(16).padStart(2, "0"))
    .join("");

export const rgba = (h: string, a: number): string => `rgba(${rgb(h).join(",")},${a})`;

// ── party colours (spec §1.6) ───────────────────────────────────────────────────
/** 4 per accent. The first 2 are the design's own (v8:448, 554). */
export const PARTY_SETS4: Record<string, [string, string, string, string]> = {
  "#e3a54f": ["#7fa6e8", "#b596e0", "#b0c46c", "#db8fb4"],
  "#4fc3ae": ["#7fa6e8", "#b596e0", "#b0c46c", "#db8fb4"],
  "#7fb1ef": ["#5fbfa9", "#c99ad8", "#b0c46c", "#db8fb4"],
  "#e58a9a": ["#7fa6e8", "#5fbfa9", "#b0c46c", "#b596e0"],
};

const FALLBACK_PAIR = ["#7fa6e8", "#b596e0"];
/** lime 74°, rose 331°, lilac 265°, blue 218°: each 60–71% lightness, ≥ 7:1 contrast on `card`. */
const RESERVE = ["#b0c46c", "#db8fb4", "#b596e0", "#7fa6e8"];

function hue(h: string): number {
  const [r = 0, g = 0, b = 0] = rgb(h).map((v) => v / 255);
  const max = Math.max(r, g, b);
  const d = max - Math.min(r, g, b);
  if (!d || Number.isNaN(d)) return 0;
  const k = max === r ? ((g - b) / d + 6) % 6 : max === g ? (b - r) / d + 2 : (r - g) / d + 4;
  return k * 60;
}

const hueGap = (a: string, b: string) => {
  const d = Math.abs(hue(a) - hue(b));
  return Math.min(d, 360 - d);
};

/**
 * The 4 party colours for an accent. Listed accents use the table. For any other accent the design's fixed
 * fallback pair is party[0..1] unchanged (v8:554: `PARTY_SETS[accent] || ['#7fa6e8', '#b596e0']`); the reserve
 * walk runs only for party[2..3]: the first reserve colours that are ≥ 30° of hue from the accent, GREEN, LOSS
 * and every colour already taken, and if that leaves fewer than 4, fill from the reserve in order.
 */
export function partySet(accent: string): [string, string, string, string] {
  const known = PARTY_SETS4[accent.toLowerCase()];
  if (known) return [...known];

  const set = [...FALLBACK_PAIR];
  for (const c of RESERVE) {
    if (set.length >= 4) break;
    const clear = [accent, GREEN, LOSS, ...set].every((o) => hueGap(c, o) >= 30);
    if (!set.includes(c) && clear) set.push(c);
  }
  for (const c of RESERVE) if (set.length < 4 && !set.includes(c)) set.push(c);
  return set.slice(0, 4) as [string, string, string, string];
}

// ── options → CSS variables (spec §1.4, §1.5, §1.8) ─────────────────────────────
export function themeVars(o: ThemeOptions): Record<string, string> {
  const t = TONES[o.tone] ?? TONES.Neutral;
  const accent = o.accent;
  const win = o.results === "Green / Red" ? GREEN : accent;
  const loss = o.results === "Accent / Gray" ? t.gray : LOSS;
  const solid = o.resultFill === "Solid";
  const tint = (c: string) => mix(t.card, c, 0.24);
  const chip = (c: string) => (solid ? { bg: c, fg: ON_SOLID } : { bg: tint(c), fg: c });
  const W = chip(win);
  const L = chip(loss);
  const party = partySet(accent);

  // The newest last-5 tile in Arrow style (the Underline style uses the plain boxes).
  const newestBd = solid ? `2px solid ${t.text}` : "none";

  return {
    "--bg": t.bg,
    "--card": t.card,
    "--raised": t.raised,
    "--line": t.line,
    "--line2": t.line2,
    "--text": t.text,
    "--text-soft": t.textSoft,
    "--text2": t.text2,
    "--text3": t.text3,
    "--gray": t.gray,

    "--line-hover": t.line2,
    "--skel": t.raised,
    "--hover-row": mix(t.card, t.raised, 0.6),
    "--team-bg": mix(t.card, t.raised, 0.45),
    "--tip-bg": mix(t.card, "#ffffff", 0.07),
    "--arrow-col": rgba(t.text2, 0.6),
    "--self-frame": rgba(t.text, 0.55),
    "--scroll": t.line2,
    "--strip-fade": `linear-gradient(90deg, ${rgba(t.card, 0)} 0%, ${rgba(t.card, 0.25)} 55%, ${t.card} 100%)`,

    "--accent": accent,
    "--party-0": party[0],
    "--party-1": party[1],
    "--party-2": party[2],
    "--party-3": party[3],

    "--win": win,
    "--loss": loss,

    "--r-w-bg": W.bg,
    "--r-w-fg": W.fg,
    "--r-w-bd": "none",
    "--r-l-bg": L.bg,
    "--r-l-fg": L.fg,
    "--r-l-bd": "none",
    "--r-d-bg": "transparent",
    "--r-d-fg": t.text2,
    "--r-d-bd": `1px solid ${t.text3}`,
    "--r-e-bg": "transparent",
    "--r-e-fg": "transparent",
    "--r-e-bd": `1px dashed ${t.line2}`,

    "--rn-w-bg": win,
    "--rn-w-fg": ON_SOLID,
    "--rn-w-bd": newestBd,
    "--rn-l-bg": loss,
    "--rn-l-fg": ON_SOLID,
    "--rn-l-bd": newestBd,
    "--rn-d-bg": "transparent",
    "--rn-d-fg": t.text,
    "--rn-d-bd": `1px solid ${t.text}`,

    "--font": FONTS[o.font] ?? FONTS.Barlow,
    "--on-solid": ON_SOLID,
  };
}

/** Write the variables on :root and `data-last5` on #app. Call before the first paint. */
export function applyTheme(o: ThemeOptions): void {
  const style = document.documentElement.style;
  for (const [k, v] of Object.entries(themeVars(o))) style.setProperty(k, v);
  document.getElementById("app")?.setAttribute("data-last5", o.last5Style === "Arrow" ? "arrow" : "underline");
}

// ── DEV URL override (?tone=&accent=&results=&fill=&font=&last5=) ───────────────
function pick<T extends string>(v: string | null, allowed: readonly T[]): T | undefined {
  return allowed.find((a) => a === v);
}

export function themeFromSearch(search: string): Partial<ThemeOptions> {
  const q = new URLSearchParams(search);
  const out: Partial<ThemeOptions> = {};
  const tone = pick(q.get("tone"), ["Neutral", "Cool", "Warm"] as const);
  if (tone) out.tone = tone;
  const accent = /^#?([0-9a-f]{6})$/i.exec(q.get("accent") ?? "");
  if (accent) out.accent = `#${accent[1]!.toLowerCase()}`;
  const results = pick(q.get("results"), ["Green / Red", "Accent / Red", "Accent / Gray"] as const);
  if (results) out.results = results;
  const fill = pick(q.get("fill"), ["Tinted", "Solid"] as const);
  if (fill) out.resultFill = fill;
  const font = pick(q.get("font"), ["Barlow", "IBM Plex Sans", "Archivo"] as const);
  if (font) out.font = font;
  const last5 = pick(q.get("last5"), ["Arrow", "Underline"] as const);
  if (last5) out.last5Style = last5;
  return out;
}
