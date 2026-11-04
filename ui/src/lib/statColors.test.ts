import { describe, expect, it } from "vitest";
import {
  fractionBand,
  ratioBand,
  METRIC_COLORS,
  LIVE_KD_BANDS,
  LIVE_HS_BANDS,
  PROFILE_KD_BANDS,
  PROFILE_HS_BANDS,
  WR_BANDS,
  liveKdColor,
  liveHsColor,
  liveWrColor,
  profileKdColor,
  profileHsColor,
  profileWrColor,
} from "./statColors";

const colors = ["#D32F2F", "#FF5252", "#FF9100", "#FFFFFF", "#69E08A", "#00C853"] as const;
const unknown = [undefined, null, NaN, Infinity, -Infinity, -0.01];
const scopes = [
  {
    name: "Live K/D",
    bands: LIVE_KD_BANDS,
    expected: [0.72, 0.79, 0.87, 1.15, 1.39] as const,
    band: ratioBand,
    color: liveKdColor,
    neutral: "#d6d7da",
    off: "#d6d7da",
    ratio: true,
  },
  {
    name: "Live HS",
    bands: LIVE_HS_BANDS,
    expected: [0.15, 0.175, 0.205, 0.275, 0.33] as const,
    band: fractionBand,
    color: liveHsColor,
    neutral: "#d6d7da",
    off: "#d6d7da",
    ratio: false,
  },
  {
    name: "Profile K/D",
    bands: PROFILE_KD_BANDS,
    expected: [0.81, 0.86, 0.92, 1.10, 1.24] as const,
    band: ratioBand,
    color: profileKdColor,
    neutral: "var(--text-mid)",
    off: "var(--text-soft)",
    ratio: true,
  },
  {
    name: "Profile HS",
    bands: PROFILE_HS_BANDS,
    expected: [0.16, 0.185, 0.21, 0.27, 0.32] as const,
    band: fractionBand,
    color: profileHsColor,
    neutral: "var(--text-mid)",
    off: "var(--text-soft)",
    ratio: false,
  },
  {
    name: "Live WR",
    bands: WR_BANDS,
    expected: [0.36, 0.40, 0.44, 0.56, 0.64] as const,
    band: fractionBand,
    color: liveWrColor,
    neutral: "#d6d7da",
    off: "#d6d7da",
    ratio: false,
  },
  {
    name: "Profile WR",
    bands: WR_BANDS,
    expected: [0.36, 0.40, 0.44, 0.56, 0.64] as const,
    band: fractionBand,
    color: profileWrColor,
    neutral: "var(--text-mid)",
    off: "var(--text-soft)",
    ratio: false,
  },
];

describe("Requirement R2: Statistic Palette and Threshold Bands", () => {
  it("defines the exact six colors (0: #D32F2F, 1: #FF5252, 2: #FF9100, 3: #FFFFFF, 4: #69E08A, 5: #00C853)", () => {
    expect(METRIC_COLORS).toEqual(["#D32F2F", "#FF5252", "#FF9100", "#FFFFFF", "#69E08A", "#00C853"]);
    expect(METRIC_COLORS[0]).toBe("#D32F2F");
    expect(METRIC_COLORS[1]).toBe("#FF5252");
    expect(METRIC_COLORS[2]).toBe("#FF9100");
    expect(METRIC_COLORS[3]).toBe("#FFFFFF");
    expect(METRIC_COLORS[4]).toBe("#69E08A");
    expect(METRIC_COLORS[5]).toBe("#00C853");
  });

  for (const s of scopes) {
    describe(s.name, () => {
      it("uses the approved continuous inclusive lower-bound thresholds", () => {
        expect(s.bands).toEqual(s.expected);
      });

      it("tests every boundary below, exactly at, and above using raw continuous values", () => {
        s.expected.forEach((bound, i) => {
          expect(s.band(bound - 0.00001, s.bands)).toBe(i);
          expect(s.band(bound, s.bands)).toBe(i + 1);
          expect(s.band(bound + 0.00001, s.bands)).toBe(i + 1);
          expect(s.color(bound - 0.00001, true)).toBe(colors[i]);
          expect(s.color(bound, true)).toBe(colors[i + 1]);
          expect(s.color(bound + 0.00001, true)).toBe(colors[i + 1]);
        });
      });

      it("keeps real zero dark red (#D32F2F) and unknown/invalid/negative values neutral", () => {
        expect(s.band(0, s.bands)).toBe(0);
        expect(s.color(0, true)).toBe(colors[0]); // #D32F2F
        for (const v of unknown) {
          expect(s.band(v, s.bands)).toBeNull();
          expect(s.color(v, true)).toBe(s.neutral);
        }
        if (s.ratio) {
          // K/D ratio is unbounded above
          expect(s.band(1.5, s.bands)).toBe(5);
          expect(s.color(1.5, true)).toBe(colors[5]);
          expect(s.color(2.5, true)).toBe(colors[5]);
          expect(s.color(10.0, true)).toBe(colors[5]);
        } else {
          // Fraction (HS, WR) cannot exceed 1.0; 1.0 is valid max, > 1.0 is neutral
          expect(s.band(1.0, s.bands)).toBe(5);
          expect(s.color(1.0, true)).toBe(colors[5]);
          expect(s.band(1.0001, s.bands)).toBeNull();
          expect(s.color(1.0001, true)).toBe(s.neutral);
          expect(s.band(1.5, s.bands)).toBeNull();
          expect(s.color(1.5, true)).toBe(s.neutral);
        }
      });

      it("preserves highlight-off for all bands, zero, and invalid values", () => {
        for (const v of [...unknown, 0, ...s.expected, 1, 5]) {
          expect(s.color(v, false)).toBe(s.off);
        }
      });
    });
  }

  it("renders typical mid-range values white in both scopes", () => {
    // Overlapping white band for Live [0.87, 1.15) and Profile [0.92, 1.10) is [0.92, 1.10)
    for (const kd of [0.92, 0.95, 1.0, 1.05, 1.099]) {
      expect(liveKdColor(kd, true)).toBe("#FFFFFF");
      expect(profileKdColor(kd, true)).toBe("#FFFFFF");
    }
    // Overlapping white band for Live HS [0.205, 0.275) and Profile HS [0.21, 0.27) is [0.21, 0.27)
    for (const hs of [0.21, 0.22, 0.24, 0.269]) {
      expect(liveHsColor(hs, true)).toBe("#FFFFFF");
      expect(profileHsColor(hs, true)).toBe("#FFFFFF");
    }
    // Shared white band for WR [0.44, 0.56)
    for (const wr of [0.44, 0.48, 0.50, 0.559]) {
      expect(liveWrColor(wr, true)).toBe("#FFFFFF");
      expect(profileWrColor(wr, true)).toBe("#FFFFFF");
    }
  });

  it("preserves independent 5-match Live vs 20-match Profile scopes and shared full-act WR", () => {
    // KD 0.80: Live has [0.79, 0.87) -> band 2 (#FF9100), Profile has < 0.81 -> band 0 (#D32F2F)
    expect(liveKdColor(0.80, true)).toBe("#FF9100");
    expect(profileKdColor(0.80, true)).toBe("#D32F2F");

    // KD 0.87: Live has [0.87, 1.15) -> band 3 (#FFFFFF), Profile has [0.86, 0.92) -> band 2 (#FF9100)
    expect(liveKdColor(0.87, true)).toBe("#FFFFFF");
    expect(profileKdColor(0.87, true)).toBe("#FF9100");

    // KD 1.16: Live has [1.15, 1.39) -> band 4 (#69E08A), Profile has >= 1.24 -> band 5 (#00C853)
    expect(liveKdColor(1.16, true)).toBe("#69E08A");
    expect(profileKdColor(1.16, true)).toBe("#69E08A"); // Profile is [1.10, 1.24) -> band 4
    expect(liveKdColor(1.25, true)).toBe("#69E08A"); // Live is [1.15, 1.39) -> band 4
    expect(profileKdColor(1.25, true)).toBe("#00C853"); // Profile is >= 1.24 -> band 5

    // HS 0.155: Live has [0.15, 0.175) -> band 1 (#FF5252), Profile has < 0.16 -> band 0 (#D32F2F)
    expect(liveHsColor(0.155, true)).toBe("#FF5252");
    expect(profileHsColor(0.155, true)).toBe("#D32F2F");

    // HS 0.206: Live has [0.205, 0.275) -> band 3 (#FFFFFF), Profile has [0.185, 0.21) -> band 2 (#FF9100)
    expect(liveHsColor(0.206, true)).toBe("#FFFFFF");
    expect(profileHsColor(0.206, true)).toBe("#FF9100");

    // Shared WR bands: identical colors across all test points
    for (const wr of [0, 0.359, 0.36, 0.399, 0.40, 0.439, 0.44, 0.50, 0.559, 0.56, 0.639, 0.64, 1.0]) {
      expect(profileWrColor(wr, true)).toBe(liveWrColor(wr, true));
    }
  });

  describe("Raw vs Rounded value verification", () => {
    it("selects colors strictly from raw values without rounding to display precision", () => {
      // Live KD: raw 0.719 would round to "0.72", but must stay band 0 (#D32F2F), not band 1 (#FF5252)
      expect(liveKdColor(0.719, true)).toBe("#D32F2F");
      expect(liveKdColor(0.720, true)).toBe("#FF5252");

      // Live KD: raw 0.789 would round to "0.79", but must stay band 1 (#FF5252), not band 2 (#FF9100)
      expect(liveKdColor(0.789, true)).toBe("#FF5252");
      expect(liveKdColor(0.790, true)).toBe("#FF9100");

      // Live KD: raw 0.869 would round to "0.87", but must stay band 2 (#FF9100), not band 3 (#FFFFFF)
      expect(liveKdColor(0.869, true)).toBe("#FF9100");
      expect(liveKdColor(0.870, true)).toBe("#FFFFFF");

      // Live KD: raw 1.149 would round to "1.15", but must stay band 3 (#FFFFFF), not band 4 (#69E08A)
      expect(liveKdColor(1.149, true)).toBe("#FFFFFF");
      expect(liveKdColor(1.150, true)).toBe("#69E08A");

      // Live KD: raw 1.389 would round to "1.39", but must stay band 4 (#69E08A), not band 5 (#00C853)
      expect(liveKdColor(1.389, true)).toBe("#69E08A");
      expect(liveKdColor(1.390, true)).toBe("#00C853");

      // Profile KD: raw 0.809 would round to "0.81", but must stay band 0 (#D32F2F), not band 1 (#FF5252)
      expect(profileKdColor(0.809, true)).toBe("#D32F2F");
      expect(profileKdColor(0.810, true)).toBe("#FF5252");

      // Profile KD: raw 0.859 would round to "0.86", but must stay band 1 (#FF5252), not band 2 (#FF9100)
      expect(profileKdColor(0.859, true)).toBe("#FF5252");
      expect(profileKdColor(0.860, true)).toBe("#FF9100");

      // Profile KD: raw 0.919 would round to "0.92", but must stay band 2 (#FF9100), not band 3 (#FFFFFF)
      expect(profileKdColor(0.919, true)).toBe("#FF9100");
      expect(profileKdColor(0.920, true)).toBe("#FFFFFF");

      // Profile KD: raw 1.099 would round to "1.10", but must stay band 3 (#FFFFFF), not band 4 (#69E08A)
      expect(profileKdColor(1.099, true)).toBe("#FFFFFF");
      expect(profileKdColor(1.100, true)).toBe("#69E08A");

      // Profile KD: raw 1.239 would round to "1.24", but must stay band 4 (#69E08A), not band 5 (#00C853)
      expect(profileKdColor(1.239, true)).toBe("#69E08A");
      expect(profileKdColor(1.240, true)).toBe("#00C853");

      // WR: raw 0.359 would round to "36%", but must stay band 0 (#D32F2F), not band 1 (#FF5252)
      expect(liveWrColor(0.359, true)).toBe("#D32F2F");
      expect(liveWrColor(0.360, true)).toBe("#FF5252");

      // WR: raw 0.399 would round to "40%", but must stay band 1 (#FF5252), not band 2 (#FF9100)
      expect(liveWrColor(0.399, true)).toBe("#FF5252");
      expect(liveWrColor(0.400, true)).toBe("#FF9100");

      // WR: raw 0.439 would round to "44%", but must stay band 2 (#FF9100), not band 3 (#FFFFFF)
      expect(liveWrColor(0.439, true)).toBe("#FF9100");
      expect(liveWrColor(0.440, true)).toBe("#FFFFFF");

      // WR: raw 0.559 would round to "56%", but must stay band 3 (#FFFFFF), not band 4 (#69E08A)
      expect(liveWrColor(0.559, true)).toBe("#FFFFFF");
      expect(liveWrColor(0.560, true)).toBe("#69E08A");

      // WR: raw 0.639 would round to "64%", but must stay band 4 (#69E08A), not band 5 (#00C853)
      expect(liveWrColor(0.639, true)).toBe("#69E08A");
      expect(liveWrColor(0.640, true)).toBe("#00C853");
    });
  });
});
