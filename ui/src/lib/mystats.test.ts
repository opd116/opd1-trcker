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
  rrInTier,
  rrLines,
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
  it("gains win, losses lose, zero is flat, and an unknown value is the one dim tone", () => {
    expect(signTone(18)).toBe("win");
    expect(signTone(-1)).toBe("loss");
    expect(signTone(0)).toBe("flat");
    expect(signTone(0.4)).toBe("flat"); // an average that rounds to 0 reads 0
    expect(signTone(null)).toBe("dim");
    expect(signTone(undefined)).toBe("dim");
    expect(signTone(Number.NaN)).toBe("dim");
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

describe("rrInTier", () => {
  it("within the tier below Immortal; counted from the 2400 floor from Immortal 1 up", () => {
    expect(rrInTier(2147)).toBe(47);
    expect(rrInTier(2100)).toBe(0);
    expect(rrInTier(2399)).toBe(99);
    expect(rrInTier(2400)).toBe(0);
    expect(rrInTier(2487)).toBe(87);
    expect(rrInTier(2755)).toBe(355); // Immortal's RR has no ceiling
  });
});

describe("rrLines", () => {
  it("none while the range is entirely below the 2400 floor", () => {
    expect(rrLines(2100, 2399, 2)).toEqual([]);
    expect(rrLines(2300, 2400, 2)).toEqual([]);
  });

  it("3 or 4 round-number lines above the floor, the smallest round step that fits", () => {
    // 187 RR above the floor at ~1.07 px per RR: 50 is the step that gives 3 lines.
    expect(rrLines(2400, 2587, 1.07)).toEqual([2450, 2500, 2550]);
    // From inside the range upward: lines are multiples of the step counted from 2400, strictly inside.
    expect(rrLines(2432, 2618, 1.07)).toEqual([2450, 2500, 2550, 2600]);
    // A short range takes a small step; a long one a big step.
    expect(rrLines(2395, 2445, 4.3)).toEqual([2410, 2420, 2430, 2440]);
    expect(rrLines(2400, 3000, 0.35)).toEqual([2600, 2800]);
  });

  it("never more than 4 lines, and never closer than 24px", () => {
    for (const [lo, hi, px] of [[2400, 2480, 3], [2400, 2900, 0.5], [2600, 2650, 5], [2380, 2640, 0.8]] as const) {
      const lines = rrLines(lo, hi, px);
      expect(lines.length).toBeLessThanOrEqual(4);
      for (let i = 1; i < lines.length; i++) expect((lines[i]! - lines[i - 1]!) * px).toBeGreaterThanOrEqual(24);
      for (const v of lines) {
        expect(v).toBeGreaterThan(Math.max(lo, 2400));
        expect(v).toBeLessThan(hi);
      }
    }
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

  it("from Immortal 1 up there are no more tier lines: the 2400 floor is the last, then RR lines", () => {
    const m = chartModel(series([2350, 2450, 2600, 2700]), box)!;
    const tiers = m.grid.filter((g) => g.label === null);
    expect(tiers.map((g) => g.abs)).toEqual([2400]);
    expect(m.bands.map((b) => b.tier)).toEqual([23, 24]);
    const above = chartModel(series([2450, 2520, 2480]), box)!;
    expect(above.grid.filter((g) => g.label === null)).toEqual([]); // the 2400 floor is below the range
    expect(above.bands.map((b) => b.tier)).toEqual([24]);
  });

  it("above the floor 3–4 round-number lines are labelled `N RR`, counted from the floor, each at its own height", () => {
    for (const abs of [[2450, 2520, 2480], [2350, 2450, 2600, 2700], [2410, 2430, 2420], [2500, 2790, 2650]]) {
      const m = chartModel(series(abs), box)!;
      const marks = m.grid.filter((g) => g.label !== null);
      expect(marks.length, abs.join()).toBeGreaterThanOrEqual(3);
      expect(marks.length, abs.join()).toBeLessThanOrEqual(4);
      for (const g of marks) {
        expect(g.label).toBe(`${g.abs - 2400} RR`);
        expect((g.abs - 2400) % 5).toBe(0);
        expect(g.y).toBeGreaterThan(m.plot.y0);
        expect(g.y).toBeLessThan(m.plot.y1);
      }
      // Higher RR, higher up; spaced so the 12px labels never touch.
      for (let i = 1; i < marks.length; i++) {
        expect(marks[i]!.y).toBeLessThan(marks[i - 1]!.y);
        expect(marks[i - 1]!.y - marks[i]!.y).toBeGreaterThanOrEqual(24);
      }
    }
    // Below the floor nothing is labelled: the bands name the tiers.
    expect(chartModel(series([2050, 2130, 2260]), box)!.grid.every((g) => g.label === null)).toBe(true);
  });

  it("the Immortal+ name sits in the widest gap between the RR labels and clears them", () => {
    const m = chartModel(series([2450, 2520, 2480]), box)!;
    const band = m.bands.find((b) => b.tier === 24)!;
    expect(band.labelY).not.toBeNull();
    for (const g of m.grid.filter((x) => x.label !== null)) expect(Math.abs(g.y - band.labelY!)).toBeGreaterThanOrEqual(16);
    // A short chart: whatever name is drawn still clears every RR label.
    const short = chartModel(series([2450, 2520, 2480]), { ...box, h: 110 })!;
    for (const b of short.bands) {
      if (b.tier !== 24 || b.labelY === null) continue;
      for (const g of short.grid.filter((x) => x.label !== null)) expect(Math.abs(g.y - b.labelY)).toBeGreaterThanOrEqual(16);
    }
  });

  it("each point carries the RR it stands at", () => {
    const m = chartModel(series([2147, 2165, 2399, 2400, 2455]), box)!;
    expect(m.points.map((p) => p.rr)).toEqual([47, 65, 99, 0, 55]);
  });
});
