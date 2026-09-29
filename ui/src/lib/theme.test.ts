import { afterEach, describe, expect, it, vi } from "vitest";
import {
  ACCENTS,
  applyTheme,
  DEFAULT_THEME,
  FONTS,
  mix,
  PARTY_SETS4,
  partySet,
  rgba,
  themeFromSearch,
  themeVars,
  TONES,
  type ThemeOptions,
} from "./theme";

const NEUTRAL = {
  "--bg": "#0e1013",
  "--card": "#171a1f",
  "--raised": "#20242a",
  "--line": "#23272e",
  "--line2": "#2c3139",
  "--text": "#e4e1db",
  "--text-soft": "#c9c9c7",
  "--text2": "#a3a7ae",
  "--text3": "#6f747c",
  "--gray": "#8d929a",
  "--line-hover": "#2c3139",
  "--skel": "#20242a",
  "--hover-row": "#1c2026",
  "--team-bg": "#1b1f24",
  "--tip-bg": "#272a2f",
  "--arrow-col": "rgba(163,167,174,0.6)",
  "--self-frame": "rgba(228,225,219,0.55)",
  "--scroll": "#2c3139",
  "--strip-fade": "linear-gradient(90deg, rgba(23,26,31,0) 0%, rgba(23,26,31,0.25) 55%, #171a1f 100%)",
  "--accent": "#e3a54f",
  "--party-0": "#7fa6e8",
  "--party-1": "#b596e0",
  "--party-2": "#b0c46c",
  "--party-3": "#db8fb4",
  "--win": "#5fbf8b",
  "--loss": "#dc7a6e",
  "--r-w-bg": "#284239",
  "--r-w-fg": "#5fbf8b",
  "--r-w-bd": "none",
  "--r-l-bg": "#463132",
  "--r-l-fg": "#dc7a6e",
  "--r-l-bd": "none",
  "--r-d-bg": "transparent",
  "--r-d-fg": "#a3a7ae",
  "--r-d-bd": "1px solid #6f747c",
  "--r-e-bg": "transparent",
  "--r-e-fg": "transparent",
  "--r-e-bd": "1px dashed #2c3139",
  "--rn-w-bg": "#5fbf8b",
  "--rn-w-fg": "#14120e",
  "--rn-w-bd": "none",
  "--rn-l-bg": "#dc7a6e",
  "--rn-l-fg": "#14120e",
  "--rn-l-bd": "none",
  "--rn-d-bg": "transparent",
  "--rn-d-fg": "#e4e1db",
  "--rn-d-bd": "1px solid #e4e1db",
  "--font": "'Barlow', sans-serif",
  "--on-solid": "#14120e",
};

describe("colour math (spec §1.3)", () => {
  it("mixes per channel with rounding", () => {
    expect(mix("#171a1f", "#20242a", 0.6)).toBe("#1c2026");
    expect(mix("#000000", "#ffffff", 0.5)).toBe("#808080");
    expect(mix("#123456", "#123456", 0.3)).toBe("#123456");
  });

  it("writes rgba() with integer channels", () => {
    expect(rgba("#a3a7ae", 0.6)).toBe("rgba(163,167,174,0.6)");
    expect(rgba("#171a1f", 0)).toBe("rgba(23,26,31,0)");
  });
});

describe("themeVars", () => {
  it("default vars equal the spec §1.4/§1.5 Neutral column", () => {
    expect(DEFAULT_THEME).toEqual({
      tone: "Neutral",
      accent: "#e3a54f",
      results: "Green / Red",
      resultFill: "Tinted",
      font: "Barlow",
      last5Style: "Arrow",
    });
    const vars = themeVars(DEFAULT_THEME);
    expect(vars).toEqual(NEUTRAL);
    expect(vars["--r-w-bg"]).toBe("#284239");
    expect(vars["--rn-w-bg"]).toBe("#5fbf8b");
  });

  const CASES: Array<[ThemeOptions["tone"], Record<string, string>]> = [
    [
      "Neutral",
      {
        hover: "#1c2026",
        team: "#1b1f24",
        tip: "#272a2f",
        arrow: "rgba(163,167,174,0.6)",
        self: "rgba(228,225,219,0.55)",
        green: "#284239",
        loss: "#463132",
        gray: "#33373d",
        "#e3a54f": "#483b2b",
        "#4fc3ae": "#244341",
        "#7fb1ef": "#303e51",
        "#e58a9a": "#48353d",
      },
    ],
    [
      "Cool",
      {
        hover: "#192028",
        team: "#181e26",
        tip: "#242930",
        arrow: "rgba(158,167,179,0.6)",
        self: "rgba(224,228,234,0.55)",
        green: "#26413a",
        loss: "#443033",
        gray: "#31373f",
        "#e3a54f": "#463b2b",
        "#4fc3ae": "#224242",
        "#7fb1ef": "#2e3d52",
        "#e58a9a": "#46343d",
      },
    ],
    [
      "Warm",
      {
        hover: "#201d1a",
        team: "#1f1c19",
        tip: "#2a2825",
        arrow: "rgba(170,163,150,0.6)",
        self: "rgba(231,226,217,0.55)",
        green: "#2b4031",
        loss: "#49302a",
        gray: "#383530",
        "#e3a54f": "#4a3a23",
        "#4fc3ae": "#27413a",
        "#7fb1ef": "#323d49",
        "#e58a9a": "#4b3335",
      },
    ],
  ];

  it.each(CASES)("computed tints per tone: %s", (tone, want) => {
    const base = { ...DEFAULT_THEME, tone };
    const v = themeVars(base);
    expect(v["--hover-row"]).toBe(want.hover);
    expect(v["--team-bg"]).toBe(want.team);
    expect(v["--tip-bg"]).toBe(want.tip);
    expect(v["--arrow-col"]).toBe(want.arrow);
    expect(v["--self-frame"]).toBe(want.self);
    // tint(GREEN) and tint(LOSS) are the W and L chips of the default results mode.
    expect(v["--r-w-bg"]).toBe(want.green);
    expect(v["--r-l-bg"]).toBe(want.loss);
    // tint(gray): the loss chip in "Accent / Gray".
    expect(themeVars({ ...base, results: "Accent / Gray" })["--r-l-bg"]).toBe(want.gray);
    // tint(accent): the win chip in "Accent / Red".
    for (const accent of ACCENTS) {
      expect(themeVars({ ...base, accent, results: "Accent / Red" })["--r-w-bg"]).toBe(want[accent]);
    }
  });

  it("follows the tone table for every base colour", () => {
    for (const tone of ["Neutral", "Cool", "Warm"] as const) {
      const t = TONES[tone];
      const v = themeVars({ ...DEFAULT_THEME, tone });
      expect([v["--bg"], v["--card"], v["--raised"], v["--line"], v["--line2"]]).toEqual([
        t.bg,
        t.card,
        t.raised,
        t.line,
        t.line2,
      ]);
      expect([v["--text"], v["--text-soft"], v["--text2"], v["--text3"], v["--gray"]]).toEqual([
        t.text,
        t.textSoft,
        t.text2,
        t.text3,
        t.gray,
      ]);
    }
  });

  it("results modes pick the win and loss colours", () => {
    const acc = "#4fc3ae";
    const accentRed = themeVars({ ...DEFAULT_THEME, accent: acc, results: "Accent / Red" });
    expect([accentRed["--win"], accentRed["--loss"]]).toEqual([acc, "#dc7a6e"]);
    const accentGray = themeVars({ ...DEFAULT_THEME, accent: acc, results: "Accent / Gray" });
    expect([accentGray["--win"], accentGray["--loss"]]).toEqual([acc, TONES.Neutral.gray]);
    expect(accentGray["--rn-l-bg"]).toBe(TONES.Neutral.gray);
  });

  it("Solid fill paints the chips and the newest tile with the result colour", () => {
    const v = themeVars({ ...DEFAULT_THEME, resultFill: "Solid" });
    expect([v["--r-w-bg"], v["--r-w-fg"], v["--r-w-bd"]]).toEqual(["#5fbf8b", "#14120e", "none"]);
    expect([v["--r-l-bg"], v["--r-l-fg"], v["--r-l-bd"]]).toEqual(["#dc7a6e", "#14120e", "none"]);
    expect([v["--rn-w-bg"], v["--rn-w-fg"], v["--rn-w-bd"]]).toEqual(["#5fbf8b", "#14120e", "2px solid #e4e1db"]);
    expect([v["--rn-l-bd"], v["--rn-d-bd"]]).toEqual(["2px solid #e4e1db", "1px solid #e4e1db"]);
    // Draw and empty boxes do not depend on the fill.
    expect(v["--r-d-bg"]).toBe("transparent");
    expect(v["--r-e-bd"]).toBe("1px dashed #2c3139");
  });

  it("font option picks the family", () => {
    expect(themeVars({ ...DEFAULT_THEME, font: "IBM Plex Sans" })["--font"]).toBe("'IBM Plex Sans', sans-serif");
    expect(themeVars({ ...DEFAULT_THEME, font: "Archivo" })["--font"]).toBe(FONTS.Archivo);
  });
});

describe("party colours (spec §1.6)", () => {
  const TABLE: Record<string, [string, string, string, string]> = {
    "#e3a54f": ["#7fa6e8", "#b596e0", "#b0c46c", "#db8fb4"],
    "#4fc3ae": ["#7fa6e8", "#b596e0", "#b0c46c", "#db8fb4"],
    "#7fb1ef": ["#5fbfa9", "#c99ad8", "#b0c46c", "#db8fb4"],
    "#e58a9a": ["#7fa6e8", "#5fbfa9", "#b0c46c", "#b596e0"],
  };

  it("party sets for the 4 accents", () => {
    expect([...ACCENTS]).toEqual(Object.keys(TABLE));
    for (const [accent, want] of Object.entries(TABLE)) {
      expect(partySet(accent)).toEqual(want);
      expect(PARTY_SETS4[accent]).toEqual(want);
      const v = themeVars({ ...DEFAULT_THEME, accent });
      expect([v["--party-0"], v["--party-1"], v["--party-2"], v["--party-3"]]).toEqual(want);
    }
  });

  it("unknown accent uses the fallback rule", () => {
    // Far from every reserve colour: the fallback pair plus the first two reserve colours that pass.
    expect(partySet("#ff00ff")).toEqual(TABLE["#e3a54f"]);
    // A lime accent (hue 74°) rejects lime itself: the pair, rose, then the remaining reserve in order.
    const lime = partySet("#a6c94a");
    expect(lime.slice(0, 3)).toEqual(["#7fa6e8", "#b596e0", "#db8fb4"]);
    expect(lime[3]).toBe("#b0c46c");
    // Always 4 distinct colours.
    for (const accent of ["#000000", "#ffffff", "#123456", "#a6c94a", "#f00"]) {
      const set = partySet(accent);
      expect(set).toHaveLength(4);
      expect(new Set(set).size).toBe(4);
    }
    expect(themeVars({ ...DEFAULT_THEME, accent: "#ff00ff" })["--party-2"]).toBe("#b0c46c");
  });
});

describe("themeFromSearch", () => {
  it("themeFromSearch parses all 6 params", () => {
    const q =
      "?scene=ingame&tone=Cool&accent=%234fc3ae&results=Accent%20%2F%20Gray&fill=Solid&font=IBM%20Plex%20Sans&last5=Underline";
    expect(themeFromSearch(q)).toEqual({
      tone: "Cool",
      accent: "#4fc3ae",
      results: "Accent / Gray",
      resultFill: "Solid",
      font: "IBM Plex Sans",
      last5Style: "Underline",
    });
  });

  it("ignores absent and invalid values", () => {
    expect(themeFromSearch("")).toEqual({});
    expect(themeFromSearch("?scene=ingame")).toEqual({});
    expect(themeFromSearch("?tone=Pink&accent=red&results=x&fill=y&font=Comic&last5=z")).toEqual({});
    expect(themeFromSearch("?accent=E3A54F")).toEqual({ accent: "#e3a54f" });
    expect(themeFromSearch("?accent=4fc3ae")).toEqual({ accent: "#4fc3ae" });
  });
});

describe("applyTheme", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("writes the vars on :root and data-last5 on #app", () => {
    const set: Record<string, string> = {};
    const attrs: Record<string, string> = {};
    vi.stubGlobal("document", {
      documentElement: { style: { setProperty: (k: string, v: string) => (set[k] = v) } },
      getElementById: (id: string) => (id === "app" ? { setAttribute: (k: string, v: string) => (attrs[k] = v) } : null),
    });
    applyTheme(DEFAULT_THEME);
    expect(set).toEqual(NEUTRAL);
    expect(attrs["data-last5"]).toBe("arrow");
    applyTheme({ ...DEFAULT_THEME, last5Style: "Underline" });
    expect(attrs["data-last5"]).toBe("underline");
  });
});
