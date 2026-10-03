// My Stats helpers (spec §9). Pure: the page renders what these return. Names from meta come through meta.ts (R95),
// tooltip strings live in tips.ts. Nothing here reads the clock or the DOM.
import { DASH, rrDelta } from "./format";
import { tierLabel } from "./meta";
import type { Meta, RrPoint, WinCell } from "./types";

/** "−" instead of the hyphen the backend writes before a number in an insight (`-15 RR`): the spec's U+2212. */
export function minusSigns(text: string): string {
  return text.replace(/(^|[\s(])-(?=\d)/g, "$1−");
}

/** The colour a signed number takes: gains `win`, losses `loss`, zero and unknown `flat` (the text colour). */
export type Tone = "win" | "loss" | "flat";

export function signTone(n: number | null | undefined): Tone {
  if (typeof n !== "number" || !Number.isFinite(n) || Math.round(n) === 0) return "flat";
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
  /** Tier boundaries (every 100 RR, up to Immortal 1's floor) inside the range. */
  grid: Array<{ y: number; abs: number }>;
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
  }));
  const line = points.map((p, i) => `${i === 0 ? "M" : "L"}${fix(p.x)} ${fix(p.y)}`).join(" ");
  const area = `${line} L${fix(points[points.length - 1]!.x)} ${fix(plot.y1)} L${fix(plot.x0)} ${fix(plot.y1)} Z`;

  // Boundaries strictly inside the range (one on the plot's own edge would only double that edge).
  const marks: number[] = [];
  for (let v = Math.floor(lo / 100) * 100 + 100; v < hi && v <= IMMORTAL_FLOOR; v += 100) marks.push(v);
  const grid = marks.map((abs) => ({ y: y(abs), abs }));

  const edges = [lo, ...marks, hi];
  const bands: ChartBand[] = [];
  for (let i = 0; i + 1 < edges.length; i++) {
    const [a, b] = [edges[i]!, edges[i + 1]!];
    const top = y(b);
    const bottom = y(a);
    bands.push({
      tier: tierOfAbsolute(a),
      y0: top,
      y1: bottom,
      labelY: (b - a) * pxPerRr >= MIN_BAND_LABEL_PX ? (top + bottom) / 2 : null,
    });
  }
  return { points, line, area, grid, bands, plot };
}
