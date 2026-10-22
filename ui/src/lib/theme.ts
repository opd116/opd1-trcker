// Theme (design v2, `.superpowers/design/v2/TrackerApp.dc.html`): one fixed dark palette with a violet accent. Every
// colour the prototype uses is a CSS custom property on :root (`themeVars`, written by `applyTheme` before the first
// paint); components read the variables, never the hex values. The TS helpers below cover what a variable cannot:
// rank tiers, skin editions, party colours and the stat highlight (`hl`), all transcribed from the prototype.
// Pure except `applyTheme`.
import { tierName as titleCase } from "./format";
import type { Meta } from "./types";

// ── colour math ──────────────────────────────────────────────────────────────────
const rgb = (h: string) => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16));

/** `a` moved `t` (0–1) of the way to `b`, as #rrggbb. */
export const mix = (a: string, b: string, t: number): string =>
  "#" +
  rgb(a)
    .map((v, i) => Math.round(v + (rgb(b)[i]! - v) * t).toString(16).padStart(2, "0"))
    .join("");

/** `#rrggbb` at alpha `a`, written the way the prototype writes it (`rgba(157,140,255,.12)`). */
export const rgba = (h: string, a: number): string => `rgba(${rgb(h).join(",")},${String(a).replace(/^0\./, ".")})`;

// ── the palette (every hex the prototype uses, by role) ──────────────────────────
export const PALETTE = {
  // surfaces, darkest first
  bgDeep: "#050607", // glance mode's backdrop around the 420px column
  chrome: "#08090b", // title bar
  rail: "#0a0b0d", // nav rail
  bg: "#0c0d10", // app background
  input: "#0e1013", // text fields, code wells
  sunken: "#101215", // table head strips
  drawer: "#111317", // profile drawer
  surface: "#121418", // search box, the waiting status pill
  card: "#131519", // cards / panels
  navActive: "#15171b", // the current nav item
  cardHead: "#16181c", // card header strip; title-bar button hover
  rowHover: "#1a1d22", // table row hover; secondary button
  skel: "#1c1f25", // skeleton bars, agent/avatar wells
  tipBg: "#1d2026", // tooltip
  chipHover: "#1f2228", // small chip/button hover
  buttonHover: "#22252b", // secondary button hover
  // lines, faintest first
  lineChrome: "#1a1c20", // title bar / nav rail / glance edges
  lineSoft: "#1d2025", // list row separators
  line: "#1f2227", // card borders, inner separators
  lineCard: "#23262c", // raised card borders
  track: "#24272d", // progress tracks; waiting pill border
  lineMedia: "#262a30", // agent / avatar frames
  lineControl: "#2b2f36", // buttons, inputs, scrollbar thumb
  lineTip: "#33373e", // tooltip border
  lineHover: "#3a3e46", // button border on hover
  lineStrong: "#4a4e56", // dashed empty slots, unchecked toggles
  // text, brightest first
  white: "#ffffff", // a highlighted (standout) stat
  text: "#ecebe7",
  textSoft: "#d6d7da", // stats with highlight off
  textMid: "#d0d2d6", // a stat in the normal band
  text2: "#c9cbd0", // secondary copy, tooltip sub-lines
  text3: "#b4b7bd", // tertiary copy
  textMuted: "#a6a9b0", // muted labels (the most used grey)
  hlLow: "#a3a7ae", // a stat well below average (dimmed, still readable)
  textLabel: "#9ba0a8", // small caps labels under numbers
  textTag: "#8f949c", // Riot tag, disabled state
  textDim: "#8a8f97", // idle icons, Unranked
  textFaint: "#7d828b", // placeholders
  textGhost: "#6b7079", // chevrons
  textDisabled: "#5b6068", // unknown values ("?")
  // accent (violet)
  accent: "#9d8cff",
  accentHover: "#b2a4ff",
  accentText: "#b7abff", // links, focus ring
  accentDeep: "#8b7cf0", // focused field border
  accentPale: "#d6cfff", // text on accent tints, link hover
  // results and signals
  win: "#5fcf8f",
  winText: "#8fe3b2",
  loss: "#ef6a5b",
  lossText: "#ff8b7e",
  warn: "#ff9a3c",
  warnText: "#ffb36b",
  mvp: "#f0c75e",
  ally: "#56c8d8",
  enemy: "#ff6b7d",
  // status pill label colours (prototype 2055–2060)
  okText: "#cdeedb",
  lostText: "#ffc690",
  errorText: "#ffb1a8",
  // window close button hover
  danger: "#c42b1c",
} as const;

/** Party colours A–D (prototype `PARTY`). Index = the party group on screen. */
export const PARTY_COLORS = ["#6fa8ff", "#f0c75e", "#e88fbf", "#9fd36b"] as const;

/** The design's three families (bundled in app.css). */
export const FONT_BODY = "'Barlow', system-ui, sans-serif";
export const FONT_DISPLAY = "'Saira Condensed', sans-serif";
export const FONT_MONO = "'Geist Mono', monospace";

const P = PALETTE;

/** Every CSS custom property, name → value. */
export function themeVars(): Record<string, string> {
  return {
    // surfaces
    "--bg-deep": P.bgDeep,
    "--chrome": P.chrome,
    "--rail": P.rail,
    "--bg": P.bg,
    "--input": P.input,
    "--sunken": P.sunken,
    "--drawer": P.drawer,
    "--surface": P.surface,
    "--card": P.card,
    "--nav-active": P.navActive,
    "--card-head": P.cardHead,
    "--chrome-hover": P.cardHead,
    "--row-hover": P.rowHover,
    "--button": P.rowHover,
    "--skel": P.skel,
    "--tip-bg": P.tipBg,
    "--chip-hover": P.chipHover,
    "--button-hover": P.buttonHover,
    "--scrim": "rgba(5,6,8,.5)",
    // lines
    "--line-chrome": P.lineChrome,
    "--line-soft": P.lineSoft,
    "--line": P.line,
    "--line-card": P.lineCard,
    "--track": P.track,
    "--line-media": P.lineMedia,
    "--line-control": P.lineControl,
    "--line-tip": P.lineTip,
    "--line-hover": P.lineHover,
    "--line-strong": P.lineStrong,
    "--scroll-thumb": P.lineControl,
    // text
    "--text-hi": P.white,
    "--text": P.text,
    "--text-soft": P.textSoft,
    "--text-mid": P.textMid,
    "--text-2": P.text2,
    "--text-3": P.text3,
    "--text-muted": P.textMuted,
    "--text-lo": P.hlLow,
    "--text-label": P.textLabel,
    "--text-tag": P.textTag,
    "--text-dim": P.textDim,
    "--text-faint": P.textFaint,
    "--text-ghost": P.textGhost,
    "--text-disabled": P.textDisabled,
    // accent
    "--accent": P.accent,
    "--accent-hover": P.accentHover,
    "--accent-text": P.accentText,
    "--accent-deep": P.accentDeep,
    "--accent-pale": P.accentPale,
    "--accent-soft": rgba(P.accent, 0.12),
    "--accent-faint": rgba(P.accent, 0.07),
    "--accent-line": rgba(P.accent, 0.26),
    "--focus": P.accentText,
    "--on-accent": P.bg,
    // results and signals
    "--win": P.win,
    "--win-text": P.winText,
    "--win-soft": rgba(P.win, 0.15),
    "--win-faint": rgba(P.win, 0.07),
    "--win-line": rgba(P.win, 0.26),
    "--loss": P.loss,
    "--loss-text": P.lossText,
    "--loss-soft": rgba(P.loss, 0.15),
    "--loss-faint": rgba(P.loss, 0.07),
    "--loss-line": rgba(P.loss, 0.3),
    "--warn": P.warn,
    "--warn-text": P.warnText,
    "--warn-faint": rgba(P.warn, 0.07),
    "--warn-line": rgba(P.warn, 0.3),
    "--mvp": P.mvp,
    "--ally": P.ally,
    "--enemy": P.enemy,
    "--ok-text": P.okText,
    "--lost-text": P.lostText,
    "--error-text": P.errorText,
    "--danger": P.danger,
    // party A–D
    "--party-0": PARTY_COLORS[0],
    "--party-1": PARTY_COLORS[1],
    "--party-2": PARTY_COLORS[2],
    "--party-3": PARTY_COLORS[3],
    // type
    "--font": FONT_BODY,
    "--font-display": FONT_DISPLAY,
    "--font-mono": FONT_MONO,
  };
}

/** Write every variable on :root. main.ts runs it (through themeStore) before the first paint. */
export function applyTheme(root: HTMLElement = document.documentElement): void {
  for (const [k, v] of Object.entries(themeVars())) root.style.setProperty(k, v);
}

// ── rank tiers (prototype `TG`, `SHORT`, `tier()`) ───────────────────────────────
/** [first tier of the group, group name, colour]. Tiers 0–2 are Unranked; 27 is Radiant. */
export const TIER_GROUPS: readonly (readonly [number, string, string])[] = [
  [0, "Unranked", "#8a8f97"],
  [3, "Iron", "#a4a7a4"],
  [6, "Bronze", "#c79f6e"],
  [9, "Silver", "#cfd5d5"],
  [12, "Gold", "#eccf56"],
  [15, "Platinum", "#5fc6d4"],
  [18, "Diamond", "#cfa6ea"],
  [21, "Ascendant", "#6ae2af"],
  [24, "Immortal", "#f2678f"],
  [27, "Radiant", "#fff1a1"],
];

const SHORT: Record<string, string> = { Platinum: "Plat", Diamond: "Dia", Ascendant: "Asc", Immortal: "Imm" };

/** A tier number the helpers accept: anything else (null, NaN, negative) reads as Unranked. */
const norm = (t: number | null | undefined): number => (typeof t === "number" && Number.isFinite(t) && t > 0 ? Math.floor(t) : 0);

function group(t: number): readonly [number, string, string] {
  let g = TIER_GROUPS[0]!;
  for (const x of TIER_GROUPS) if (t >= x[0]) g = x;
  return g;
}

/** The division suffix (" 1"–" 3"); none for Unranked and Radiant. */
const division = (t: number) => (t >= 27 || t < 3 ? "" : ` ${t - group(t)[0] + 1}`);

/** The tier's name from meta (valorant-api's "DIAMOND 2" → "Diamond 2"), when meta has it; ranked tiers only. */
function metaName(t: number, meta: Meta | null | undefined): string | null {
  if (t < 3 || !meta) return null;
  const raw = meta.tiers[String(t)]?.name;
  return raw ? titleCase(raw) : null;
}

/** "Diamond 2", "Radiant", "Unranked". The name meta carries wins (it follows the game's own tables). */
export function tierName(t: number | null | undefined, meta?: Meta | null): string {
  const n = norm(t);
  return metaName(n, meta) ?? group(n)[1] + division(n);
}

/** "Dia 2", "Plat 1", "Asc 3", "Imm 1", "Gold 2", "Radiant", "Unranked". */
export function tierShort(t: number | null | undefined, meta?: Meta | null): string {
  const n = norm(t);
  const name = metaName(n, meta);
  if (name) {
    const [first = "", ...rest] = name.split(" ");
    return [SHORT[first] ?? first, ...rest].join(" ");
  }
  const g = group(n)[1];
  return (SHORT[g] ?? g) + division(n);
}

/** The design's colour for the tier's group (fixed palette, never valorant-api's colours). */
export function tierColor(t: number | null | undefined): string {
  return group(norm(t))[2];
}

// ── skin editions (prototype `TIERC`) ────────────────────────────────────────────
export const EDITION_COLORS = {
  Select: "#5a9fe2",
  Deluxe: "#2fbfa4",
  Premium: "#d1548d",
  Exclusive: "#f5955b",
  Ultra: "#fad663",
} as const;

export type Edition = keyof typeof EDITION_COLORS;

// ── stat highlight (prototype `hl`) ──────────────────────────────────────────────
/**
 * The colour of a stat value: ≥ `hi` white, ≤ `lo` dimmed (still readable), else the normal band. With the
 * "Highlight standout stats" setting off every value is the plain stat colour.
 */
export function hl(v: number | null | undefined, hi: number, lo: number, on = true): string {
  if (!on || typeof v !== "number" || !Number.isFinite(v)) return P.textSoft;
  return v >= hi ? P.white : v <= lo ? P.hlLow : P.textMid;
}

/** Win%: ≥ 60 white, ≤ 47 dim. */
export const hlWinPct = (v: number | null | undefined, on = true) => hl(v, 60, 47, on);
/** K/D: ≥ 1.3 white, ≤ 0.95 dim. */
export const hlKd = (v: number | null | undefined, on = true) => hl(v, 1.3, 0.95, on);
/** HS%: ≥ 28 white, ≤ 19 dim. */
export const hlHs = (v: number | null | undefined, on = true) => hl(v, 28, 19, on);
