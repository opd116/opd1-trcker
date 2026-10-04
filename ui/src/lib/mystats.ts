// My Stats helpers (spec §9). Pure: the page renders what these return. Names from meta come through meta.ts (R95),
// tooltip strings live in tips.ts. Nothing here reads the clock or the DOM.
import { DASH, rrDelta } from "./format";
import { tierLabel } from "./meta";
import type { Meta, RrPoint, WinCell } from "./types";

/** "−" instead of the hyphen the backend writes before a number in an insight (`-15 RR`): the spec's U+2212. */
export function minusSigns(text: string): string {
  return text.replace(/(^|[\s(])-(?=\d)/g, "$1−");
}

/**
 * The colour a signed number takes: gains `win`, losses `loss`, zero `flat` (the text colour), and an unknown value
 * (`—`) `dim`: the one quiet tone every unknown on My Stats takes (`--text2`), never green, red or full text.
 */
export type Tone = "win" | "loss" | "flat" | "dim";

export function signTone(n: number | null | undefined): Tone {
  if (typeof n !== "number" || !Number.isFinite(n)) return "dim";
  if (Math.round(n) === 0) return "flat";
  return n > 0 ? "win" : "loss";
}

/** A signed RR number as the page writes it (`+18`, `−15`, `0`); `—` when unknown. Averages are rounded whole. */
export const signedRr = rrDelta;

/** A win rate against the even mark: above 50% `win`, below `loss`, exactly half or no games `flat`. */
export function rateTone(rate: number | null | undefined): Tone {
  if (typeof rate !== "number" || !Number.isFinite(rate)) return "flat";
  const pct = Math.round(rate * 100);
  return pct > 50 ? "win" : pct < 50 ? "loss" : "flat";
}

export const WEEKDAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"] as const;
export const WEEKDAYS_LONG = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"] as const;

/** The four time-of-day buckets, in the backend's order, with their local hour range (Morning 05–12 … Night 22–05). */
export const DAYPARTS = [
  { label: "Morning", hours: "5 AM – 12 PM" },
  { label: "Afternoon", hours: "12 PM – 5 PM" },
  { label: "Evening", hours: "5 PM – 10 PM" },
  { label: "Night", hours: "10 PM – 5 AM" },
] as const;

/** One cell of the weekday / time-of-day panels: its label, rate text and bar. `games` 0 → `—` and no bar. */
export interface CellView {
  label: string;
  games: number;
  wins: number;
  losses: number;
  rate: number | null;
  /** `64%`, or `—` with no games. */
  text: string;
  /** The bar's fill, 0..1 (0 with no games). */
  fill: number;
  tone: Tone;
  /** What the second tip row says next to "This act": the long weekday or the hour range. */
  scope: string;
}

/** The cells of a WinCell list under their labels; `cells` null (nothing known) → empty cells, one per label. */
export function cellViews(
  cells: readonly WinCell[] | null,
  labels: readonly string[],
  scopes: readonly string[],
): CellView[] {
  return labels.map((label, i) => {
    const c = cells?.[i];
    const games = c?.games ?? 0;
    const rate = games > 0 && typeof c?.winrate === "number" ? c.winrate : null;
    return {
      label,
      games,
      wins: c?.wins ?? 0,
      losses: Math.max(0, games - (c?.wins ?? 0)),
      rate,
      text: rate === null ? DASH : `${Math.round(rate * 100)}%`,
      fill: rate === null ? 0 : Math.max(0, Math.min(1, rate)),
      tone: rateTone(rate),
      scope: scopes[i] ?? "",
    };
  });
}

// ── the RR chart ──────────────────────────────────────────────────────────────────────────────────────────────────
/** From Immortal 1 up the backend adds RR to 2400 (Immortal 1's floor); no fixed tier lines exist above it. */
export const IMMORTAL_FLOOR = 2400;
/** Immortal 1's tier number (the first tier whose RR accumulates). */
export const IMMORTAL_TIER = 24;

/**
 * A chart band's name (`Ascendant 2`), by the same rule as `tierLabel` (undefined: meta pending; null: unknown). From
 * Immortal 1 up the chart's RR is cumulative and no longer says Immortal 1, 2 or 3, so that band is the family name
 * alone: `Immortal`, or `Immortal+` as a band label when `plus` is set.
 */
export function bandName(meta: Meta | null, tier: number, plus = false): string | null | undefined {
  const label = tierLabel(meta, Math.min(tier, IMMORTAL_TIER));
  if (tier < IMMORTAL_TIER || typeof label !== "string") return label;
  const family = label.replace(/\s+\d+$/, "");
  return plus ? `${family}+` : family;
}
/** A tier band's label needs this much height (a 12px line with room around it). */
const MIN_BAND_LABEL_PX = 16;
/** The least RR the y range spans, so a flat stretch of a few points does not fill the chart. */
const MIN_SPAN = 50;

/** The tier an absolute RR value sits in: `floor(abs / 100)` below Immortal, tier 24 for everything from 2400 up. */
export function tierOfAbsolute(abs: number): number {
  return abs >= IMMORTAL_FLOOR ? IMMORTAL_TIER : Math.max(0, Math.floor(abs / 100));
}

/**
 * The RR a point stands at: within its tier below Immortal (`abs % 100`, the 0–99 the rank card shows), and counted
 * from Immortal 1's floor from 2400 up (`abs − 2400`, which has no ceiling).
 */
export function rrInTier(abs: number): number {
  return abs >= IMMORTAL_FLOOR ? abs - IMMORTAL_FLOOR : Math.max(0, abs % 100);
}

/** The round steps the gridlines above Immortal 1's floor may take, smallest first. */
const RR_STEPS = [5, 10, 20, 25, 50, 100, 200, 250, 500, 1000, 2000, 5000];
/** At most this many RR gridlines above the floor (3 or 4 read as a scale; more is a ruler). */
const MAX_RR_LINES = 4;
/** Two RR gridlines are at least this far apart, so their 12px labels never touch. */
const MIN_RR_LINE_PX = 24;

/**
 * The round-number gridlines (absolute values) strictly inside `(max(lo, 2400), hi)`: the smallest round step that
 * gives at most 4 lines at least 24px apart. None when the range is entirely below the floor.
 */
export function rrLines(lo: number, hi: number, pxPerRr: number): number[] {
  const from = Math.max(lo, IMMORTAL_FLOOR);
  if (hi <= from) return [];
  for (const step of RR_STEPS) {
    if (step * pxPerRr < MIN_RR_LINE_PX) continue;
    const lines: number[] = [];
    for (let v = IMMORTAL_FLOOR + (Math.floor((from - IMMORTAL_FLOOR) / step) + 1) * step; v < hi; v += step) lines.push(v);
    if (lines.length <= MAX_RR_LINES) return lines;
  }
  return [];
}

export interface ChartBox {
  w: number;
  h: number;
  /** Room for the tier labels on the left, and for the end dates under the plot. */
  padL: number;
  padR: number;
  padT: number;
  padB: number;
}

export interface ChartPoint {
  x: number;
  y: number;
  point: RrPoint;
  /** RR gained or lost by that match: this point's `absoluteRr` less the one before; null for the first point. */
  delta: number | null;
  /** The RR it stands at: within its tier, or counted from Immortal 1's floor (`rrInTier`). */
  rr: number;
}

/** A tier band's visible part: `tier` ≥ 24 stands for "Immortal and up". `labelY` is its vertical middle, or null when too thin to label. */
export interface ChartBand {
  tier: number;
  y0: number;
  y1: number;
  labelY: number | null;
}

export interface ChartModel {
  points: ChartPoint[];
  /** The polyline's path, and the same closed down to the baseline (the area wash). */
  line: string;
  area: string;
  /**
   * Gridlines inside the range: the tier boundaries (every 100 RR, up to Immortal 1's floor; no `label`, the bands
   * name the tiers) and, above the floor, 3–4 round-number lines labelled `N RR` (counted from the floor).
   */
  grid: Array<{ y: number; abs: number; label: string | null }>;
  bands: ChartBand[];
  plot: { x0: number; x1: number; y0: number; y1: number };
}

const fix = (n: number) => (Math.round(n * 10) / 10).toString();

/**
 * Chart geometry for a series (oldest first): even spacing by match, a y range of the data plus a little air (at
 * least `MIN_SPAN` RR), tier boundaries as gridlines, and the tier bands between them for the left labels.
 * null with fewer than 2 points (nothing to draw a line through) or a box too small for a plot.
 */
export function chartModel(series: readonly RrPoint[], box: ChartBox): ChartModel | null {
  if (series.length < 2) return null;
  const plot = { x0: box.padL, x1: box.w - box.padR, y0: box.padT, y1: box.h - box.padB };
  if (plot.x1 - plot.x0 < 10 || plot.y1 - plot.y0 < 10) return null;

  const values = series.map((p) => p.absoluteRr);
  const min = Math.min(...values);
  const max = Math.max(...values);
  const air = Math.max(10, (max - min) * 0.12);
  let lo = Math.max(0, min - air);
  let hi = max + air;
  if (hi - lo < MIN_SPAN) {
    const mid = (hi + lo) / 2;
    lo = Math.max(0, mid - MIN_SPAN / 2);
    hi = lo + MIN_SPAN;
  }
  const y = (v: number) => plot.y1 - ((v - lo) / (hi - lo)) * (plot.y1 - plot.y0);
  const pxPerRr = (plot.y1 - plot.y0) / (hi - lo);

  const points = series.map((p, i): ChartPoint => ({
    x: plot.x0 + (i * (plot.x1 - plot.x0)) / (series.length - 1),
    y: y(p.absoluteRr),
    point: p,
    delta: i === 0 ? null : p.absoluteRr - series[i - 1]!.absoluteRr,
    rr: rrInTier(p.absoluteRr),
  }));
  const line = points.map((p, i) => `${i === 0 ? "M" : "L"}${fix(p.x)} ${fix(p.y)}`).join(" ");
  const area = `${line} L${fix(points[points.length - 1]!.x)} ${fix(plot.y1)} L${fix(plot.x0)} ${fix(plot.y1)} Z`;

  // Boundaries strictly inside the range (one on the plot's own edge would only double that edge).
  const marks: number[] = [];
  for (let v = Math.floor(lo / 100) * 100 + 100; v < hi && v <= IMMORTAL_FLOOR; v += 100) marks.push(v);
  // Above Immortal 1's floor there are no tiers to draw, so the scale is RR itself: a few round-number lines.
  const rrMarks = rrLines(lo, hi, pxPerRr);
  const grid = [
    ...marks.map((abs) => ({ y: y(abs), abs, label: null as string | null })),
    ...rrMarks.map((abs) => ({ y: y(abs), abs, label: `${abs - IMMORTAL_FLOOR} RR` as string | null })),
  ];

  const edges = [lo, ...marks, hi];
  const bands: ChartBand[] = [];
  for (let i = 0; i + 1 < edges.length; i++) {
    const [a, b] = [edges[i]!, edges[i + 1]!];
    const top = y(b);
    const bottom = y(a);
    let labelY: number | null = (b - a) * pxPerRr >= MIN_BAND_LABEL_PX ? (top + bottom) / 2 : null;
    if (tierOfAbsolute(a) >= IMMORTAL_TIER && rrMarks.length > 0) {
      // The RR lines carry their own labels in the same gutter: the band's name goes in the widest gap between them
      // (lowest first), and only where it clears both neighbours' labels.
      const cuts = [a, ...rrMarks, b];
      let best = 0;
      for (let k = 1; k + 1 < cuts.length; k++) if (cuts[k + 1]! - cuts[k]! > cuts[best + 1]! - cuts[best]!) best = k;
      const [ga, gb] = [cuts[best]!, cuts[best + 1]!];
      labelY = (gb - ga) * pxPerRr >= 2 * MIN_BAND_LABEL_PX ? y((ga + gb) / 2) : null;
    }
    bands.push({ tier: tierOfAbsolute(a), y0: top, y1: bottom, labelY });
  }
  return { points, line, area, grid, bands, plot };
}

/** The chart's picked point (spec §9.2): its match (`ms`, the start time) and the index it last stood at. */
export interface ChartPick {
  ms: number;
  idx: number;
}

/**
 * Where the pick stands in `points` (oldest first): at its match's own index, which moves when a `mystats` push
 * inserts older points or drops some. The remembered index is tried first (nothing moved: the common case), then a
 * search by start time; a match no longer in the series falls back to its old index, clamped. The newest with no pick
 * (the slider's value while nothing is picked).
 */
export function valueIndex(points: readonly { point: { startMs: number } }[], pick: ChartPick | null): number {
  const end = points.length - 1;
  if (pick === null) return end;
  if (points[pick.idx]?.point.startMs === pick.ms) return pick.idx;
  const i = points.findIndex((p) => p.point.startMs === pick.ms);
  return i >= 0 ? i : Math.max(0, Math.min(end, pick.idx));
}

/** The keys `keyTarget` reads (a `KeyboardEvent` has them all). */
export interface PickKey {
  key: string;
  ctrlKey: boolean;
  altKey: boolean;
  metaKey: boolean;
  shiftKey: boolean;
}

/**
 * The index a key moves the pick to (`from` is where it stands, `end` the newest): ←/→ one point, Home/End the ends,
 * clamped. null for any other key and for any with a modifier held (Ctrl+←, Alt+←, Shift+Home …): those are not the
 * chart's and are left alone, so the page's own keys (scrolling, browser shortcuts) keep working.
 */
export function keyTarget(e: PickKey, from: number, end: number): number | null {
  if (e.ctrlKey || e.altKey || e.metaKey || e.shiftKey) return null;
  const next = e.key === "ArrowLeft" ? from - 1 : e.key === "ArrowRight" ? from + 1 : e.key === "Home" ? 0 : e.key === "End" ? end : null;
  return next === null ? null : Math.max(0, Math.min(end, next));
}
