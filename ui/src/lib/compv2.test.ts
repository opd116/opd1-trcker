// Competitive (design v2) helpers: the RR chart window and geometry, matching points to known matches, the rank card's
// lines, the table rows and the best-times cells. Pure functions, no DOM.
import { describe, expect, it } from "vitest";
import {
  actGames,
  axisLabels,
  CHART_WINDOW,
  chartTierName,
  daypartRow,
  exactWhen,
  hoursMinutes,
  knownMatches,
  netTip,
  pointAfter,
  rankFill,
  rankSubline,
  rrChart,
  sessionEnd,
  sessionOpen,
  statRowView,
  subtitle,
  weekdayTile,
  wrBarVar,
} from "./compv2";
import type { MatchSummary, MyStats, RrPoint, SessionSummary, StatRow } from "./types";

const H = 3_600_000;
const T0 = Date.UTC(2026, 8, 1, 12);
const series = (vals: number[], step = H): RrPoint[] => vals.map((v, i) => ({ startMs: T0 + i * step, absoluteRr: v }));

const summary = (startMs: number, over: Partial<MatchSummary> = {}): MatchSummary => ({
  matchId: `m${startMs}`,
  mapId: "map-a",
  startMs,
  result: "win",
  roundsWon: 13,
  roundsLost: 7,
  agentId: null,
  kills: null,
  deaths: null,
  assists: null,
  kd: null,
  acs: null,
  hsPct: null,
  rrEarned: 18,
  tier: 21,
  ...over,
});

describe("knownMatches", () => {
  it("keys last5 and the session by start time; last5 wins (it has the score)", () => {
    const session: SessionSummary = {
      netRr: 4,
      wins: 1,
      losses: 1,
      startedMs: T0,
      matches: [
        { matchId: "s1", mapId: "map-b", rrEarned: -14, startMs: T0, result: "L" },
        { matchId: "s2", mapId: null, rrEarned: 18, startMs: T0 + H, result: "W" },
      ],
    };
    const k = knownMatches({ last5: [summary(T0 + H)], session });
    expect(k.get(T0)).toMatchObject({ matchId: "s1", result: "L", score: null });
    expect(k.get(T0 + H)).toMatchObject({ matchId: `m${T0 + H}`, result: "W", score: "13–7", mapId: "map-a" });
  });
});

describe("rrChart", () => {
  it("is null with no points", () => {
    expect(rrChart([], new Map(), null)).toBeNull();
  });

  it("shows the last 40 of a longer act, with deltas from the full series and the net from before the window", () => {
    const vals = Array.from({ length: 59 }, (_, i) => 1800 + i * 5);
    const v = rrChart(series(vals), new Map(), null)!;
    expect(v.n).toBe(CHART_WINDOW);
    expect(v.total).toBe(59);
    expect(v.points[0]!.abs).toBe(vals[19]);
    expect(v.points[0]!.delta).toBe(5);
    expect(v.fromBefore).toBe(true);
    expect(v.net).toBe(vals[58]! - vals[18]!);
    // even columns: point i at the centre of column i
    expect(v.points[0]!.xPct).toBeCloseTo(100 * (0.5 / 40));
    expect(v.points[39]!.xPct).toBeCloseTo(100 * (39.5 / 40));
  });

  it("whole act in the window: the first point has no delta and the net starts at it", () => {
    const v = rrChart(series([2050, 2068, 2054]), new Map(), null)!;
    expect(v.points[0]!.delta).toBeNull();
    expect(v.points[0]!.result).toBeNull();
    expect(v.points[1]!.result).toBe("W");
    expect(v.points[2]!.result).toBe("L");
    expect(v.net).toBe(4);
    expect(v.fromBefore).toBe(false);
  });

  it("labels each 100-RR line with the tier that starts there, inside the ±25 RR range", () => {
    const v = rrChart(series([1990, 2030, 2121]), new Map(), null)!;
    // range 1965..2146: lines at 2000 (tier 20, Diamond 3) and 2100 (tier 21, Ascendant 1)
    expect(v.bands.map((b) => b.label)).toEqual(["Dia 3", "Asc 1"]);
    expect(v.bands[1]!.color).toBe("#6ae2af");
    for (const b of v.bands) expect(b.topPct).toBeGreaterThanOrEqual(0);
  });

  it("above Immortal 1's floor the lines are RR counts, not tiers", () => {
    const v = rrChart(series([2480, 2560, 2620, 2700]), new Map(), null)!;
    expect(v.bands.every((b) => /^\d+ RR$/.test(b.label))).toBe(true);
    expect(v.points[3]!.rr).toBe(300);
    expect(pointAfter(v.points[3]!)).toBe("Immortal · 300 RR");
  });

  it("shades the session from its first point, and takes results and matches from known matches", () => {
    const s = series([2000, 2018, 2004, 2022]);
    const session: SessionSummary = {
      netRr: 4,
      wins: 1,
      losses: 1,
      startedMs: s[2]!.startMs,
      matches: [
        { matchId: "a", mapId: "x", rrEarned: -14, startMs: s[2]!.startMs, result: "L" },
        { matchId: "b", mapId: "x", rrEarned: 18, startMs: s[3]!.startMs, result: "W" },
      ],
    };
    const v = rrChart(s, knownMatches({ last5: [], session }), session)!;
    expect(v.todayLeftPct).toBe(50);
    expect(v.points[2]!.match?.matchId).toBe("a");
    expect(v.points[1]!.match).toBeNull();
  });

  it("net tip mentions the earlier matches only when the window cuts some off", () => {
    const long = rrChart(series(Array.from({ length: 50 }, (_, i) => 2000 + i)), new Map(), null)!;
    expect(netTip(long, 293)).toContain("The act total (+293) also includes the 10 earlier matches.");
    const short = rrChart(series([2000, 2010]), new Map(), null)!;
    expect(netTip(short, 10)).not.toContain("act total");
  });
});

describe("axis, dates and durations", () => {
  it("four dates under the chart, Today for today", () => {
    const pts = series(Array.from({ length: 10 }, () => 2000), 24 * H);
    const labels = axisLabels(pts, pts[9]!.startMs + H);
    expect(labels).toHaveLength(4);
    expect(labels[3]).toBe("Today");
  });

  it("a date that repeats the next one is left blank, keeping its slot", () => {
    // 40 points a minute apart, all today (T0 is noon): every mark is "Today", and only the last one says it.
    const pts = series(Array.from({ length: 40 }, () => 2000), 60_000);
    const labels = axisLabels(pts, pts[39]!.startMs + 60_000);
    expect(labels).toHaveLength(4);
    expect(labels.filter((l) => l === "Today")).toHaveLength(1);
    expect(labels[3]).toBe("Today");
    expect(labels[2]).toBe("");
  });

  it("exactWhen writes the design's en-GB date with the 12-hour clock", () => {
    expect(exactWhen(new Date(2026, 9, 22, 17, 21).getTime())).toBe("Thu 22 Oct 2026 · 5:21 PM");
  });

  it("hoursMinutes", () => {
    expect(hoursMinutes(38 * 60_000)).toBe("38 min");
    expect(hoursMinutes(5 * H + 19 * 60_000)).toBe("5 h 19 min");
    expect(hoursMinutes(2 * H)).toBe("2 h");
  });

  it("a session over 6 h old ends at its last match", () => {
    const s = { startedMs: T0, matches: [{ matchId: "a", mapId: null, rrEarned: null, startMs: T0 + H, result: "W" as const }] };
    expect(sessionEnd(s, T0 + 2 * H)).toBe(T0 + 2 * H);
    expect(sessionEnd(s, T0 + 30 * H)).toBe(T0 + H);
    expect(sessionOpen(s, T0 + 2 * H)).toBe(true);
    expect(sessionOpen(s, T0 + 30 * H)).toBe(false);
  });
});

describe("rank card", () => {
  it("writes RR out of 100 and the RR to the next tier", () => {
    expect(rankSubline(21, 21, 79)).toBe("21 / 100 RR · 79 to Ascendant 2");
    expect(rankSubline(21, null, null)).toBe("— / 100 RR");
    expect(rankSubline(24, 143, null)).toBe("143 RR");
    expect(rankSubline(null, null, null)).toBe("— RR");
  });

  it("fills the bar below Immortal only", () => {
    expect(rankFill(21, 21)).toBe(21);
    expect(rankFill(25, 300)).toBe(0);
    expect(rankFill(21, null)).toBe(0);
    expect(rankFill(0, 0)).toBe(0);
  });

  it("chartTierName says Immortal from the floor up", () => {
    expect(chartTierName(2121)).toBe("Ascendant 1");
    expect(chartTierName(2550)).toBe("Immortal");
  });
});

describe("tables and best times", () => {
  const row = (games: number, wins: number, netRr: number | null): StatRow => ({
    id: "x",
    games,
    wins,
    losses: games - wins,
    winrate: wins / games,
    netRr,
    kd: 1.234,
    acs: 200,
  });

  it("bar colour thresholds", () => {
    expect(wrBarVar(55)).toBe("var(--win)");
    expect(wrBarVar(45)).toBe("var(--loss)");
    expect(wrBarVar(50)).toBe("var(--text-faint)");
  });

  it("a row view: W–L, win%, signed net RR, K/D, limited evidence under 8 games", () => {
    const v = statRowView(row(5, 3, -9));
    expect(v).toMatchObject({ wl: "3–2", wr: "60%", netRr: "−9", netColor: "var(--loss)", kd: "1.23" });
    expect(v.tipTail).toBe("5 games, 3W – 2L\nUnder 8 games: limited evidence");
    expect(statRowView(row(10, 6, null)).netRr).toBe("—");
    expect(statRowView(row(10, 6, null)).tipTail).toBe("10 games, 6W – 4L");
  });

  it("weekday tiles: tint, hatch under 8 games, dashed with none", () => {
    expect(weekdayTile({ games: 0, wins: 0, losses: 0, winrate: null }, 0)).toMatchObject({ wr: "—", g: "none", dashed: true, empty: true });
    const low = weekdayTile({ games: 6, wins: 6, losses: 0, winrate: 1 }, 6);
    expect(low.dashed).toBe(true);
    expect(low.bg).toContain("repeating-linear-gradient");
    expect(low.tip).toBe("Sunday: 100% · 6W – 0L over 6 games\nUnder 8 games: limited evidence");
    // A draw is neither a win nor a loss.
    expect(weekdayTile({ games: 9, wins: 5, losses: 3, winrate: 5 / 9 }, 0).tip).toContain("5W – 3L over 9 games");
    expect(daypartRow({ games: 9, wins: 5, losses: 3, winrate: 5 / 9 }, 0).tip).toContain("5W – 3L");
    expect(weekdayTile({ games: 11, wins: 5, losses: 6, winrate: 5 / 11 }, 3).bg).toMatch(/^rgba\(239,106,91/);
    expect(weekdayTile({ games: 10, wins: 5, losses: 5, winrate: 0.5 }, 1).bg).toBe("#1c1f24");
  });

  it("daypart rows", () => {
    expect(daypartRow({ games: 0, wins: 0, losses: 0, winrate: null }, 0)).toMatchObject({ d: "Morning", wr: "—", g: "No games", empty: true });
    expect(daypartRow({ games: 34, wins: 23, losses: 11, winrate: 23 / 34 }, 2)).toMatchObject({ wr: "68%", g: "34 games", bar: "var(--win)" });
  });
});

describe("header", () => {
  it("counts the act's matches from the maps table", () => {
    const s = { maps: [{ games: 12 }, { games: 9 }] } as unknown as MyStats;
    expect(actGames(s)).toBe(21);
  });

  it("subtitle with and without data", () => {
    expect(subtitle(null, T0)).toBe("Current act. Each card names the period it covers.");
    const s = { maps: [{ games: 1 }], rrSeries: [{ startMs: Date.UTC(2026, 8, 2, 12), absoluteRr: 0 }] } as unknown as MyStats;
    expect(subtitle(s, Date.UTC(2026, 9, 1))).toBe("Current act · Sep 2 – today · 1 match. Each card names the period it covers.");
  });
});
