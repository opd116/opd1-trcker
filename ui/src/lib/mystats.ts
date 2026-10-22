// The Competitive screen's RR helpers (spec §9.2), kept from v8 My Stats: tiers from absolute RR, the RR gridlines above
// Immortal 1's floor, and the chart pick's keyboard model. Pure: nothing here reads the clock or the DOM.

/** From Immortal 1 up the backend adds RR to 2400 (Immortal 1's floor); no fixed tier lines exist above it. */
export const IMMORTAL_FLOOR = 2400;
/** Immortal 1's tier number (the first tier whose RR accumulates). */
export const IMMORTAL_TIER = 24;

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
