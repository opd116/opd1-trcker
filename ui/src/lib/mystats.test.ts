// My Stats helpers (spec §9): minus signs, tones, the weekday / time-of-day cells, band names and the RR chart's
// geometry. Pure functions, no DOM.
import { describe, expect, it } from "vitest";
import {
  bandName,
  cellViews,
  chartModel,
  DAYPARTS,
  minusSigns,
  rateTone,
  signedRr,
  signTone,
  tierOfAbsolute,
  WEEKDAYS,
  WEEKDAYS_LONG,
  type ChartBox,
} from "./mystats";
import type { Meta, RrPoint, WinCell } from "./types";

const meta: Meta = {
  agents: {},
  maps: {},
  tiers: {
    "21": { name: "ASCENDANT 1", color: "" },
    "22": { name: "ASCENDANT 2", color: "" },
    "23": { name: "ASCENDANT 3", color: "" },
    "24": { name: "IMMORTAL 1", color: "" },
  },
};

describe("minusSigns", () => {
  it("writes the spec's U+2212 for the hyphen the backend puts before a number", () => {
    expect(minusSigns("Average +19 RR on wins and -17 RR on losses.")).toBe("Average +19 RR on wins and −17 RR on losses.");
    expect(minusSigns("-5 RR over 12 games in the last 7 days.")).toBe("−5 RR over 12 games in the last 7 days.");
    expect(minusSigns("Net (-12 RR)")).toBe("Net (−12 RR)");
  });

  it("leaves every other hyphen alone", () => {
    expect(minusSigns("A well-known map: 60% wins")).toBe("A well-known map: 60% wins");
    expect(minusSigns("+30 RR")).toBe("+30 RR");
    expect(minusSigns("W-L")).toBe("W-L");
  });
});

describe("signTone and signedRr", () => {
  it("gains win, losses lose, zero and unknown are flat", () => {
    expect(signTone(18)).toBe("win");
    expect(signTone(-1)).toBe("loss");
    expect(signTone(0)).toBe("flat");
    expect(signTone(null)).toBe("flat");
    expect(signTone(undefined)).toBe("flat");
    expect(signTone(0.4)).toBe("flat"); // an average that rounds to 0 reads 0
  });

  it("signedRr rounds whole and writes +, U+2212 and 0; unknown is the dash", () => {
    expect(signedRr(19.75)).toBe("+20");
    expect(signedRr(-15)).toBe("−15");
    expect(signedRr(0)).toBe("0");
    expect(signedRr(null)).toBe("—");
  });
});

describe("rateTone", () => {
  it("above 50% wins, below loses, the shown 50% and no games are flat", () => {
    expect(rateTone(0.53)).toBe("win");
    expect(rateTone(0.38)).toBe("loss");
    expect(rateTone(0.5)).toBe("flat");
    expect(rateTone(0.504)).toBe("flat"); // shown as 50%
    expect(rateTone(null)).toBe("flat");
  });
});

describe("cellViews", () => {
  const cells: WinCell[] = [
    { games: 8, wins: 5, winrate: 0.625 },
    { games: 0, wins: 0, winrate: null },
    { games: 4, wins: 1, winrate: 0.25 },
    { games: 2, wins: 1, winrate: 0.5 },
  ];
  const labels = ["A", "B", "C", "D"];
  const scopes = ["a", "b", "c", "d"];

  it("a cell is its label, rate text, bar fill, tone, losses and tip slice", () => {
    const v = cellViews(cells, labels, scopes);
    expect(v.map((c) => c.text)).toEqual(["63%", "—", "25%", "50%"]);
    expect(v.map((c) => c.fill)).toEqual([0.625, 0, 0.25, 0.5]);
    expect(v.map((c) => c.tone)).toEqual(["win", "flat", "loss", "flat"]);
    expect(v.map((c) => c.losses)).toEqual([3, 0, 3, 1]);
    expect(v.map((c) => c.scope)).toEqual(["a", "b", "c", "d"]);
    expect(v[1]).toMatchObject({ rate: null, games: 0, fill: 0 });
  });

  it("nothing known is one empty cell per label", () => {
    const v = cellViews(null, labels, scopes);
    expect(v).toHaveLength(4);
    for (const c of v) expect(c).toMatchObject({ games: 0, rate: null, text: "—", fill: 0, tone: "flat" });
  });

  it("a rate is never trusted without games", () => {
    expect(cellViews([{ games: 0, wins: 0, winrate: 0.9 }], ["A"], ["a"])[0]!.rate).toBeNull();
  });

  it("the labels: Monday first, the four dayparts with their hour ranges", () => {
    expect(WEEKDAYS).toEqual(["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]);
    expect(WEEKDAYS_LONG[0]).toBe("Monday");
    expect(WEEKDAYS_LONG).toHaveLength(7);
    expect(DAYPARTS.map((d) => d.label)).toEqual(["Morning", "Afternoon", "Evening", "Night"]);
    expect(DAYPARTS.map((d) => d.hours)).toEqual(["5 AM – 12 PM", "12 PM – 5 PM", "5 PM – 10 PM", "10 PM – 5 AM"]);
  });
});

describe("tierOfAbsolute and bandName", () => {
  it("tier × 100 + RR below Immortal, one tier from 2400 up", () => {
    expect(tierOfAbsolute(2150)).toBe(21);
    expect(tierOfAbsolute(2199)).toBe(21);
    expect(tierOfAbsolute(2200)).toBe(22);
    expect(tierOfAbsolute(2399)).toBe(23);
    expect(tierOfAbsolute(2400)).toBe(24);
    expect(tierOfAbsolute(2655)).toBe(24);
  });

  it("a band is named by the tier table; from Immortal up it is the family alone", () => {
    expect(bandName(meta, 22)).toBe("Ascendant 2");
    expect(bandName(meta, 24)).toBe("Immortal");
    expect(bandName(meta, 24, true)).toBe("Immortal+");
    expect(bandName(meta, 30, true)).toBe("Immortal+");
  });

  it("meta pending is undefined (a skeleton), a tier the table lacks is null", () => {
    expect(bandName(null, 22)).toBeUndefined();
    expect(bandName(null, 24, true)).toBeUndefined();
    expect(bandName(meta, 9)).toBeNull();
    expect(bandName(meta, 1)).toBeNull();
  });
});

describe("chartModel", () => {
  const box: ChartBox = { w: 700, h: 300, padL: 96, padR: 18, padT: 16, padB: 30 };
  const series = (abs: number[]): RrPoint[] => abs.map((absoluteRr, i) => ({ startMs: 1000 + i * 100, absoluteRr }));

  it("fewer than 2 points, or a box with no room, has no chart", () => {
    expect(chartModel([], box)).toBeNull();
    expect(chartModel(series([2100]), box)).toBeNull();
    expect(chartModel(series([2100, 2120]), { ...box, w: 100 })).toBeNull();
    expect(chartModel(series([2100, 2120]), { ...box, h: 40 })).toBeNull();
  });

  it("points are evenly spaced over the plot, oldest left, higher RR higher up", () => {
    const m = chartModel(series([2100, 2150, 2130, 2180]), box)!;
    expect(m.plot).toEqual({ x0: 96, x1: 682, y0: 16, y1: 270 });
    expect(m.points.map((p) => p.x)).toEqual([96, 96 + 586 / 3, 96 + (2 * 586) / 3, 682]);
    expect(m.points[0]!.y).toBeGreaterThan(m.points[1]!.y); // 2150 above 2100
    expect(m.points[1]!.y).toBeLessThan(m.points[2]!.y); // 2130 below 2150
    for (const p of m.points) {
      expect(p.y).toBeGreaterThanOrEqual(m.plot.y0);
      expect(p.y).toBeLessThanOrEqual(m.plot.y1);
    }
  });

  it("a point's change is the step from the one before; the first has none", () => {
    const m = chartModel(series([2100, 2118, 2103, 2103]), box)!;
    expect(m.points.map((p) => p.delta)).toEqual([null, 18, -15, 0]);
  });

  it("the path is the polyline, and the area is the same closed down to the baseline", () => {
    const m = chartModel(series([2100, 2150, 2130]), box)!;
    expect(m.line.startsWith("M96 ")).toBe(true);
    expect(m.line.match(/L/g)).toHaveLength(2);
    expect(m.area.startsWith(m.line)).toBe(true);
    expect(m.area.endsWith(`L682 270 L96 270 Z`)).toBe(true);
  });

  it("a flat stretch still spans at least 50 RR (it never fills the chart)", () => {
    const m = chartModel(series([2100, 2100, 2101]), box)!;
    const ys = m.points.map((p) => p.y);
    expect(Math.max(...ys) - Math.min(...ys)).toBeLessThan((m.plot.y1 - m.plot.y0) * 0.1);
  });

  it("tier boundaries inside the range are the gridlines, each at its own height; the bands between them are named by tier", () => {
    const m = chartModel(series([2050, 2130, 2260]), box)!;
    expect(m.grid.map((g) => g.abs)).toEqual([2100, 2200]);
    const y = (abs: number) => m.grid.find((g) => g.abs === abs)!.y;
    expect(y(2200)).toBeLessThan(y(2100));
    expect(m.bands.map((b) => b.tier)).toEqual([20, 21, 22]); // lowest first
    // The bands tile the plot from top to bottom with no gap.
    const sorted = [...m.bands].sort((a, b) => a.y0 - b.y0);
    expect(sorted[0]!.y0).toBeCloseTo(m.plot.y0);
    expect(sorted[sorted.length - 1]!.y1).toBeCloseTo(m.plot.y1);
    for (let i = 1; i < sorted.length; i++) expect(sorted[i]!.y0).toBeCloseTo(sorted[i - 1]!.y1);
  });

  it("one band when the data stays inside a tier: no gridline, one label", () => {
    const m = chartModel(series([2110, 2150, 2135]), box)!;
    expect(m.grid).toEqual([]);
    expect(m.bands).toHaveLength(1);
    expect(m.bands[0]).toMatchObject({ tier: 21 });
    expect(m.bands[0]!.labelY).not.toBeNull();
  });

  it("a band too thin for a 12px label goes unnamed", () => {
    // 2199 → 2201: the range is padded to 50 RR; the sliver below / above 2200 is wide enough, so squeeze the box.
    const m = chartModel(series([2090, 2300]), { ...box, h: 80 })!;
    expect(m.bands.some((b) => b.labelY === null)).toBe(true);
    for (const b of m.bands) if (b.labelY !== null) expect(b.y1 - b.y0).toBeGreaterThanOrEqual(16);
  });

  it("from Immortal 1 up there are no more tier lines: the 2400 floor is the last", () => {
    const m = chartModel(series([2350, 2450, 2600, 2700]), box)!;
    expect(m.grid.map((g) => g.abs)).toEqual([2400]);
    expect(m.bands.map((b) => b.tier)).toEqual([23, 24]);
    const above = chartModel(series([2450, 2520, 2480]), box)!;
    expect(above.grid).toEqual([]); // the 2400 floor is below the range
    expect(above.bands.map((b) => b.tier)).toEqual([24]);
  });
});
