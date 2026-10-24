// Competitive screen (design v2, TrackerApp.dc.html 658–792; logic `compData` / `rrSeries` / `extraVals2`). Pure: the
// screen and its cards render what these return, from the real `MyStats` snapshot. Nothing here reads the clock or the
// DOM (every "now" is passed in). The RR geometry rules for Immortal+ (`tierOfAbsolute`, `rrInTier`, `rrLines`) and the
// chart's keyboard (`keyTarget`, `valueIndex`) are the proven ones in `mystats.ts`.
import { DASH, dateShort, rrDelta, time12 } from "./format";
import { IMMORTAL_FLOOR, IMMORTAL_TIER, rrInTier, rrLines, tierOfAbsolute } from "./mystats";
import { tierColor, tierName, tierShort } from "./theme";
import type { MatchSummary, Meta, MyStats, RrPoint, SessionSummary, StatRow, StreakMark, WinCell } from "./types";

/** The chart shows at most this many of the act's points (the design's "LAST 40 OF 59 MATCHES"). */
export const CHART_WINDOW = 40;
/** Fewer games than this in a row or a cell: "limited evidence" (hatched tile, tooltip line). */
export const LOW_SAMPLE = 8;
export const LOW_SAMPLE_TIP = "Under 8 games: limited evidence";

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const DAYS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/** "Thu 22 Oct 2026 · 5:21 PM": the design's `stamp().exact` (en-GB date, 12-hour clock), local time. */
export function exactWhen(ms: number): string {
  const d = new Date(ms);
  return `${DAYS[d.getDay()]} ${d.getDate()} ${MONTHS[d.getMonth()]} ${d.getFullYear()} · ${time12(ms)}`;
}

/** Whether two timestamps fall on the same local calendar day. */
export function sameDay(a: number, b: number): boolean {
  const x = new Date(a);
  const y = new Date(b);
  return x.getFullYear() === y.getFullYear() && x.getMonth() === y.getMonth() && x.getDate() === y.getDate();
}

/** "Today", else "Oct 4" (", 2025" in another year). */
export function dayLabel(ms: number, nowMs: number): string {
  return sameDay(ms, nowMs) ? "Today" : dateShort(ms, nowMs);
}

/** "5 h 19 min", "38 min", "2 h": a session's length. */
export function hoursMinutes(ms: number): string {
  const total = Math.max(0, Math.round(ms / 60_000));
  const h = Math.floor(total / 60);
  const m = total % 60;
  if (h === 0) return `${m} min`;
  return m === 0 ? `${h} h` : `${h} h ${m} min`;
}

/** A session ends 6 h after its last match with no new one (the backend's rule). */
export const SESSION_GAP_MS = 6 * 3_600_000;

/**
 * Where a session's length is measured to: now, while it is still running; its last match's start once a snapshot
 * held since then is over 6 h old (the page can stay open long after the backend would have closed the session).
 */
export function sessionEnd(s: Pick<SessionSummary, "matches" | "startedMs">, nowMs: number): number {
  const lastStart = s.matches.reduce((a, m) => Math.max(a, m.startMs), s.startedMs);
  return nowMs - lastStart > SESSION_GAP_MS ? lastStart : nowMs;
}

/**
 * Whether the session is still going at `nowMs`: its last match started at most 6 h ago. Rust drops an ended session
 * only when it recomputes My Stats (a DB change), so a snapshot held overnight still carries yesterday's.
 */
export function sessionOpen(s: Pick<SessionSummary, "matches" | "startedMs">, nowMs: number): boolean {
  return sessionEnd(s, nowMs) === nowMs;
}

/** "1 game", "12 games". */
export const plural = (n: number, one: string, many = `${one}s`): string => `${n} ${n === 1 ? one : many}`;

// ── what the chart knows about a match ────────────────────────────────────────────────────────────────────────────
/** A match the snapshot describes beyond its RR point: from `last5` (score included) or the session's chips. */
export interface KnownMatch {
  matchId: string;
  mapId: string | null;
  result: StreakMark;
  /** "13–5" from the player's side; null when only the session knows the match. */
  score: string | null;
  rrEarned: number | null;
  startMs: number;
}

const LETTER = { win: "W", loss: "L", draw: "D" } as const;

/**
 * The matches the snapshot names, keyed by start time (an `RrPoint` carries no match id; its `startMs` is the match's
 * own, the same value `last5` and the session hold). `last5` wins where both know a match: it has the score.
 */
export function knownMatches(s: Pick<MyStats, "last5" | "session">): Map<number, KnownMatch> {
  const out = new Map<number, KnownMatch>();
  for (const m of s.session?.matches ?? []) {
    out.set(m.startMs, { matchId: m.matchId, mapId: m.mapId, result: m.result, score: null, rrEarned: m.rrEarned, startMs: m.startMs });
  }
  for (const m of s.last5) out.set(m.startMs, fromSummary(m));
  return out;
}

function fromSummary(m: MatchSummary): KnownMatch {
  const score = typeof m.roundsWon === "number" && typeof m.roundsLost === "number" ? `${m.roundsWon}–${m.roundsLost}` : null;
  return { matchId: m.matchId, mapId: m.mapId || null, result: LETTER[m.result], score, rrEarned: m.rrEarned, startMs: m.startMs };
}

// ── the RR progression chart ──────────────────────────────────────────────────────────────────────────────────────
export interface RrChartPoint {
  /** Index in the window (0 = oldest shown). */
  i: number;
  startMs: number;
  abs: number;
  /** The RR this match moved: this point less the one before it in the act; null for the act's first match. */
  delta: number | null;
  /** The tier the point stands in (24 for everything from Immortal 1's floor up) and the RR within it. */
  tier: number;
  rr: number;
  /** Centre of the point's column and its height, as percentages of the plot box. */
  xPct: number;
  yPct: number;
  match: KnownMatch | null;
  /** W / L / D from the match when known, else from the RR it moved (none for 0 or unknown). */
  result: StreakMark | null;
}

export interface RrBand {
  /** Line position, % from the plot's top. */
  topPct: number;
  label: string;
  color: string;
}

export interface RrChartView {
  /** Points shown and points in the act's series. */
  n: number;
  total: number;
  points: RrChartPoint[];
  /** Line and area paths in the SVG's 800 × 200 box (stretched to the plot, `preserveAspectRatio="none"`). */
  line: string;
  area: string;
  bands: RrBand[];
  /** Left edge of the shaded "Today" region (the first point inside the session), % of the plot; null with none. */
  todayLeftPct: number | null;
  /** RR at the last point less RR before the window's first match (or at it, when the act starts in the window). */
  net: number;
  /** Whether `net` starts before the first shown match (an earlier point exists). */
  fromBefore: boolean;
}

const VB_W = 800;
const VB_H = 200;
/** Air above and below the data, in RR (the design's ±25). */
const AIR = 25;
/** Nominal plot height for spacing the RR lines above Immortal 1's floor. */
const NOMINAL_PLOT_PX = 230;

const f1 = (n: number) => n.toFixed(1);

/** A point's tier name for the chart: "Ascendant 1"; from Immortal 1's floor up RR is cumulative, so "Immortal". */
export function chartTierName(abs: number, meta?: Meta | null): string {
  if (abs >= IMMORTAL_FLOOR) return tierName(IMMORTAL_TIER, meta).replace(/\s+\d+$/, "");
  return tierName(tierOfAbsolute(abs), meta);
}

/**
 * The chart for the newest `CHART_WINDOW` points of the act (oldest first). Columns are even: point i sits at the
 * centre of column i. The y range is the data plus 25 RR of air, the RR before the window included. A line every 100
 * RR is labelled with the tier that starts there (short name, tier colour) up to Immortal 1's floor; above it, where
 * RR accumulates, 3–4 round-number lines are labelled "N RR". null with no points.
 */
export function rrChart(
  series: readonly RrPoint[],
  known: Map<number, KnownMatch>,
  session: SessionSummary | null,
  meta?: Meta | null,
): RrChartView | null {
  if (series.length === 0) return null;
  const total = series.length;
  const from = Math.max(0, total - CHART_WINDOW);
  const win = series.slice(from);
  const n = win.length;
  const before = from > 0 ? series[from - 1]!.absoluteRr : null;
  const vals = win.map((p) => p.absoluteRr);
  const lo = Math.min(...vals, ...(before === null ? [] : [before])) - AIR;
  const hi = Math.max(...vals) + AIR;
  const yp = (v: number) => (hi - v) / (hi - lo);
  const xp = (i: number) => (i + 0.5) / n;

  const points = win.map((p, i): RrChartPoint => {
    const prev = from + i > 0 ? series[from + i - 1]!.absoluteRr : null;
    const delta = prev === null ? null : p.absoluteRr - prev;
    const match = known.get(p.startMs) ?? null;
    return {
      i,
      startMs: p.startMs,
      abs: p.absoluteRr,
      delta,
      tier: tierOfAbsolute(p.absoluteRr),
      rr: rrInTier(p.absoluteRr),
      xPct: xp(i) * 100,
      yPct: yp(p.absoluteRr) * 100,
      match,
      result: match?.result ?? (delta === null || delta === 0 ? null : delta > 0 ? "W" : "L"),
    };
  });

  const line = vals.map((v, i) => `${i ? "L" : "M"}${f1(xp(i) * VB_W)} ${f1(yp(v) * VB_H)}`).join(" ");
  const area = `${line} L${f1(xp(n - 1) * VB_W)} ${VB_H} L${f1(xp(0) * VB_W)} ${VB_H} Z`;

  const bands: RrBand[] = [];
  for (let b = Math.ceil(lo / 100) * 100; b <= hi && b <= IMMORTAL_FLOOR; b += 100) {
    const t = Math.max(0, b / 100);
    bands.push({ topPct: yp(b) * 100, label: tierShort(t, meta), color: tierColor(t) });
  }
  for (const abs of rrLines(lo, hi, NOMINAL_PLOT_PX / (hi - lo))) {
    bands.push({ topPct: yp(abs) * 100, label: `${abs - IMMORTAL_FLOOR} RR`, color: tierColor(IMMORTAL_TIER) });
  }

  const firstToday = session ? points.findIndex((p) => p.startMs >= session.startedMs) : -1;
  const startAbs = before ?? vals[0]!;
  return {
    n,
    total,
    points,
    line,
    area,
    bands,
    todayLeftPct: firstToday < 0 ? null : (firstToday / n) * 100,
    net: vals[n - 1]! - startAbs,
    fromBefore: before !== null,
  };
}

/**
 * The four dates under the chart: first, a third, two thirds and last point ("Today" for today). A label equal to the
 * next one is left blank (two marks on the same day would read "Today Today"); its slot stays, so the others keep
 * their place on the axis.
 */
export function axisLabels(points: readonly { startMs: number }[], nowMs: number): string[] {
  const n = points.length;
  if (n === 0) return [];
  const idx = [...new Set([0, Math.round((n - 1) / 3), Math.round((2 * (n - 1)) / 3), n - 1])];
  const labels = idx.map((i) => dayLabel(points[i]!.startMs, nowMs));
  return labels.map((l, i) => (l === labels[i + 1] ? "" : l));
}

/** The chart header's net: "+212 RR", "−40 RR", "0 RR". */
export const netLabel = (n: number): string => `${rrDelta(n)} RR`;

/** The net's tooltip: what it counts, and what the act's own total adds. */
export function netTip(view: RrChartView, actNet: number | null): string {
  const what = view.fromBefore
    ? "RR at the end of the chart minus RR before its first match."
    : "RR at the end of the chart minus RR after its first match.";
  const earlier = view.total - view.n;
  if (earlier <= 0 || actNet === null) return what;
  return `${what} The act total (${rrDelta(actNet)}) also includes the ${plural(earlier, "earlier match", "earlier matches")}.`;
}

/** The hover card's first line: "Match 12 of 40 · Thu 22 Oct 2026 · 5:21 PM". */
export const pointWhen = (p: RrChartPoint, n: number): string => `Match ${p.i + 1} of ${n} · ${exactWhen(p.startMs)}`;

/** The hover card's last line after the arrow: "Ascendant 1 · 21 RR", "Immortal · 143 RR". */
export const pointAfter = (p: RrChartPoint, meta?: Meta | null): string => `${chartTierName(p.abs, meta)} · ${p.rr} RR`;

/** What a screen reader hears for a point (the slider's value text). */
export function pointText(p: RrChartPoint, n: number, map: string | null, meta?: Meta | null): string {
  const res = p.result === "W" ? "Win" : p.result === "L" ? "Loss" : p.result === "D" ? "Draw" : null;
  const parts = [
    pointWhen(p, n),
    res,
    map,
    p.match?.score ?? null,
    p.delta === null ? null : `${rrDelta(p.delta)} RR`,
    pointAfter(p, meta),
  ];
  return parts.filter((x): x is string => typeof x === "string" && x !== "").join(" · ");
}

// ── the rank card ─────────────────────────────────────────────────────────────────────────────────────────────────
/** "21 / 100 RR · 79 to Ascendant 2"; from Immortal up "143 RR"; an unknown RR is "—". */
export function rankSubline(tier: number | null, rr: number | null, rrToNext: number | null, meta?: Meta | null): string {
  if (tier === null) return `${DASH} RR`;
  const v = typeof rr === "number" ? String(rr) : DASH;
  if (tier >= IMMORTAL_TIER) return `${v} RR`;
  if (tier < 3) return `${v} RR`;
  const base = `${v} / 100 RR`;
  return typeof rrToNext === "number" ? `${base} · ${rrToNext} to ${tierName(tier + 1, meta)}` : base;
}

/** The rank bar's fill, 0..100 (empty for Immortal+, unranked and an unknown RR). */
export function rankFill(tier: number | null, rr: number | null): number {
  if (tier === null || tier < 3 || tier >= IMMORTAL_TIER || typeof rr !== "number") return 0;
  return Math.max(0, Math.min(100, rr));
}

// ── maps / agents rows ────────────────────────────────────────────────────────────────────────────────────────────
/** The win% bar's colour: ≥ 55 green, ≤ 45 red, else grey (the design's `bar`). */
export function wrBarVar(wrPct: number): string {
  return wrPct >= 55 ? "var(--win)" : wrPct <= 45 ? "var(--loss)" : "var(--text-faint)";
}

export interface StatRowView {
  id: string;
  wl: string;
  wrPct: number;
  wr: string;
  bar: string;
  netRr: string;
  netColor: string;
  kd: string;
  /** Without the name (the card adds it once meta has it). */
  tipTail: string;
}

export function statRowView(r: StatRow): StatRowView {
  const wrPct = Math.round(r.winrate * 100);
  const net = r.netRr;
  return {
    id: r.id,
    wl: `${r.wins}–${r.losses}`,
    wrPct,
    wr: `${wrPct}%`,
    bar: wrBarVar(wrPct),
    netRr: typeof net === "number" ? rrDelta(net) : DASH,
    netColor: typeof net !== "number" ? "var(--text-muted)" : net >= 0 ? "var(--win)" : "var(--loss)",
    kd: Number.isFinite(r.kd) ? r.kd.toFixed(2) : DASH,
    tipTail: `${plural(r.games, "game")}, ${r.wins}W – ${r.losses}L` + (r.games < LOW_SAMPLE ? `\n${LOW_SAMPLE_TIP}` : ""),
  };
}

// ── best times ────────────────────────────────────────────────────────────────────────────────────────────────────
export const WEEKDAY_SHORT = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"] as const;
export const WEEKDAY_FULL = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"] as const;
/** The backend's buckets (Morning 05–12, Afternoon 12–17, Evening 17–22, Night 22–05) as the design writes them. */
export const DAYPART_VIEW = [
  { d: "Morning", h: "5 AM – 12 PM" },
  { d: "Afternoon", h: "12 – 5 PM" },
  { d: "Evening", h: "5 – 10 PM" },
  { d: "Night", h: "10 PM – 5 AM" },
] as const;

const HATCH = "repeating-linear-gradient(135deg,rgba(255,255,255,.07) 0 3px,transparent 3px 7px)";

export interface WeekdayTile {
  d: string;
  wr: string;
  g: string;
  bg: string;
  dashed: boolean;
  empty: boolean;
  tip: string;
}

/** A weekday tile: tinted green from 60%, red to 47%, neutral between; hatched and dashed under 8 games; dashed with none. */
export function weekdayTile(c: WinCell | undefined, i: number): WeekdayTile {
  const g = c?.games ?? 0;
  const w = c?.wins ?? 0;
  const wr = g > 0 ? Math.round((w / g) * 100) : null;
  const low = g > 0 && g < LOW_SAMPLE;
  const tint =
    wr === null
      ? "transparent"
      : wr >= 60
        ? `rgba(95,207,143,${(0.14 + Math.min(40, wr - 60) / 120).toFixed(2)})`
        : wr <= 47
          ? `rgba(239,106,91,${(0.12 + (47 - wr) / 90).toFixed(2)})`
          : "#1c1f24";
  const full = WEEKDAY_FULL[i] ?? "";
  return {
    d: WEEKDAY_SHORT[i] ?? "",
    wr: wr === null ? DASH : `${wr}%`,
    g: g ? `${g} g` : "none",
    bg: low ? `${HATCH},${tint === "transparent" ? "var(--card-head)" : tint}` : tint,
    dashed: low || wr === null,
    empty: wr === null,
    tip: `${full}: ` + (g ? `${wr}% · ${w}W – ${c?.losses ?? 0}L over ${plural(g, "game")}` : "no games played") + (low ? `\n${LOW_SAMPLE_TIP}` : ""),
  };
}

export interface DaypartRow {
  d: string;
  h: string;
  wr: string;
  w: number;
  bar: string;
  empty: boolean;
  g: string;
  tip: string;
}

export function daypartRow(c: WinCell | undefined, i: number): DaypartRow {
  const g = c?.games ?? 0;
  const w = c?.wins ?? 0;
  const wr = g > 0 ? Math.round((w / g) * 100) : null;
  const low = g > 0 && g < LOW_SAMPLE;
  const v = DAYPART_VIEW[i]!;
  return {
    d: v.d,
    h: v.h,
    wr: wr === null ? DASH : `${wr}%`,
    w: wr ?? 0,
    bar: wr === null ? "transparent" : wrBarVar(wr),
    empty: wr === null,
    g: g ? plural(g, "game") : "No games",
    tip: `${v.d} (${v.h}): ` + (g ? `${wr}% · ${w}W – ${c?.losses ?? 0}L` : "no games played, so no win rate") + (low ? `\n${LOW_SAMPLE_TIP}` : ""),
  };
}

// ── header ────────────────────────────────────────────────────────────────────────────────────────────────────────
/** Matches this act: every game of the maps table (draws included). */
export function actGames(s: Pick<MyStats, "maps">): number {
  return s.maps.reduce((a, r) => a + r.games, 0);
}

/** "Current act · Sep 2 – today · 59 matches. Each card names the period it covers." */
export function subtitle(s: MyStats | null | undefined, nowMs: number): string {
  const tail = "Each card names the period it covers.";
  if (!s) return `Current act. ${tail}`;
  const n = actGames(s);
  const first = s.rrSeries[0]?.startMs;
  const range = typeof first === "number" ? ` · ${dateShort(first, nowMs)} – today` : "";
  return `Current act${range} · ${plural(n, "match", "matches")}. ${tail}`;
}
