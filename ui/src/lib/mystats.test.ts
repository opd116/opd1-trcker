// The Competitive screen's RR helpers (spec §9.2): tiers from absolute RR and the RR gridlines above the floor.
import { describe, expect, it } from "vitest";
import { rrInTier, rrLines, tierOfAbsolute } from "./mystats";

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

describe("tierOfAbsolute", () => {
  it("tier × 100 + RR below Immortal, one tier from 2400 up", () => {
    expect(tierOfAbsolute(2150)).toBe(21);
    expect(tierOfAbsolute(2199)).toBe(21);
    expect(tierOfAbsolute(2200)).toBe(22);
    expect(tierOfAbsolute(2399)).toBe(23);
    expect(tierOfAbsolute(2400)).toBe(24);
    expect(tierOfAbsolute(2655)).toBe(24);
  });
});
