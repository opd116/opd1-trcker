/// <reference types="node" />
import { existsSync, readFileSync } from "node:fs";
import { describe, expect, it, vi } from "vitest";
import {
  applyTheme,
  EDITION_COLORS,
  hl,
  hlHs,
  hlKd,
  hlWinPct,
  mix,
  PALETTE,
  PARTY_COLORS,
  rgba,
  themeVars,
  TIER_GROUPS,
  tierColor,
  tierName,
  tierShort,
} from "./theme";
import type { Meta } from "./types";

// The prototype is not committed (.superpowers/): this cross-check runs only in a checkout that has it.
const PROTO_URL = new URL("../../../.superpowers/design/v2/TrackerApp.dc.html", import.meta.url);
const PROTOTYPE = existsSync(PROTO_URL) ? readFileSync(PROTO_URL, "utf8") : "";
const hasPrototype = PROTOTYPE.length > 0;

describe("palette (design v2)", () => {
  it("the core semantic variables carry the prototype's values", () => {
    const v = themeVars();
    expect(v).toMatchObject({
      "--bg": "#0c0d10",
      "--chrome": "#08090b",
      "--rail": "#0a0b0d",
      "--card": "#131519",
      "--card-head": "#16181c",
      "--row-hover": "#1a1d22",
      "--tip-bg": "#1d2026",
      "--line": "#1f2227",
      "--line-chrome": "#1a1c20",
      "--line-control": "#2b2f36",
      "--line-tip": "#33373e",
      "--text": "#ecebe7",
      "--text-muted": "#a6a9b0",
      "--accent": "#9d8cff",
      "--accent-soft": "rgba(157,140,255,.12)",
      "--focus": "#b7abff",
      "--win": "#5fcf8f",
      "--loss": "#ef6a5b",
      "--warn": "#ff9a3c",
      "--mvp": "#f0c75e",
      "--ally": "#56c8d8",
      "--enemy": "#ff6b7d",
      "--party-0": "#6fa8ff",
      "--party-1": "#f0c75e",
      "--party-2": "#e88fbf",
      "--party-3": "#9fd36b",
    });
  });

  it("every value is a colour or a font stack; names are CSS custom properties", () => {
    for (const [k, val] of Object.entries(themeVars())) {
      expect(k, k).toMatch(/^--[a-z0-9-]+$/);
      expect(val, k).toMatch(/^(#[0-9a-f]{6}|rgba\(|'[A-Z])/);
    }
  });

  it("covers every hex the lead listed from the prototype", () => {
    const listed = [
      "#050607", "#08090b", "#0a0b0d", "#0c0d10", "#0e1013", "#111317", "#121418", "#131519", "#15171b", "#16181c", "#1a1d22",
      "#1c1f25", "#1d2026", "#1f2228", "#22252b", "#1a1c20", "#1f2227", "#23262c", "#262a30", "#2b2f36", "#33373e", "#3a3e46",
      "#4a4e56", "#ecebe7", "#d6d7da", "#d0d2d6", "#c9cbd0", "#b4b7bd", "#a6a9b0", "#9ba0a8", "#8f949c", "#8a8f97", "#7d828b",
      "#6b7079", "#5b6068", "#9d8cff", "#b2a4ff", "#b7abff", "#8b7cf0", "#d6cfff", "#5fcf8f", "#8fe3b2", "#ef6a5b", "#ff8b7e",
      "#ff9a3c", "#ffb36b", "#f0c75e", "#56c8d8", "#ff6b7d",
    ];
    const values = new Set(Object.values(themeVars()));
    for (const h of listed) expect(values.has(h), h).toBe(true);
  });

  it.runIf(hasPrototype)("every palette hex appears in the prototype", () => {
    for (const [k, h] of Object.entries(PALETTE)) expect(PROTOTYPE.toLowerCase(), k).toContain(h);
    for (const h of PARTY_COLORS) expect(PROTOTYPE.toLowerCase()).toContain(h);
  });

  it("applyTheme writes every variable on the root it is given", () => {
    const setProperty = vi.fn();
    applyTheme({ style: { setProperty } } as unknown as HTMLElement);
    const vars = themeVars();
    expect(setProperty).toHaveBeenCalledTimes(Object.keys(vars).length);
    for (const [k, v] of Object.entries(vars)) expect(setProperty).toHaveBeenCalledWith(k, v);
  });

  it("mix and rgba", () => {
    expect(mix("#000000", "#ffffff", 0.5)).toBe("#808080");
    expect(rgba("#9d8cff", 0.12)).toBe("rgba(157,140,255,.12)");
    expect(rgba("#9d8cff", 1)).toBe("rgba(157,140,255,1)");
  });
});

describe("tiers (prototype TG / tier())", () => {
  it("TIER_GROUPS is the prototype's table", () => {
    expect(TIER_GROUPS.map((g) => g[0])).toEqual([0, 3, 6, 9, 12, 15, 18, 21, 24, 27]);
    expect(TIER_GROUPS[7]).toEqual([21, "Ascendant", "#6ae2af"]);
  });

  it("names, short names and colours", () => {
    expect(tierName(0)).toBe("Unranked");
    expect(tierName(2)).toBe("Unranked");
    expect(tierName(3)).toBe("Iron 1");
    expect(tierName(21)).toBe("Ascendant 1");
    expect(tierName(23)).toBe("Ascendant 3");
    expect(tierName(27)).toBe("Radiant");
    expect(tierShort(16)).toBe("Plat 2");
    expect(tierShort(20)).toBe("Dia 3");
    expect(tierShort(22)).toBe("Asc 2");
    expect(tierShort(24)).toBe("Imm 1");
    expect(tierShort(13)).toBe("Gold 2");
    expect(tierShort(27)).toBe("Radiant");
    expect(tierShort(null)).toBe("Unranked");
    expect(tierColor(22)).toBe("#6ae2af");
    expect(tierColor(null)).toBe("#8a8f97");
    expect(tierColor(27)).toBe("#fff1a1");
  });

  it("prefers meta's names when it has them (ranked tiers only)", () => {
    const meta: Meta = { agents: {}, maps: {}, tiers: { "22": { name: "ASCENDANT 2", color: "x" }, "0": { name: "UNRANKED", color: "x" } } };
    expect(tierName(22, meta)).toBe("Ascendant 2");
    expect(tierShort(22, meta)).toBe("Asc 2");
    expect(tierName(0, meta)).toBe("Unranked");
    expect(tierName(16, meta)).toBe("Platinum 2"); // missing from meta: the table
  });
});

describe("editions and the stat highlight", () => {
  it("EDITION_COLORS", () => {
    expect(EDITION_COLORS).toEqual({ Select: "#5a9fe2", Deluxe: "#2fbfa4", Premium: "#d1548d", Exclusive: "#f5955b", Ultra: "#fad663" });
  });

  it("hl: ≥ hi white, ≤ lo dim, else the normal band; off → plain", () => {
    expect(hl(60, 60, 47)).toBe("#ffffff");
    expect(hl(47, 60, 47)).toBe("#a3a7ae");
    expect(hl(55, 60, 47)).toBe("#d0d2d6");
    expect(hl(99, 60, 47, false)).toBe("#d6d7da");
    expect(hl(null, 60, 47)).toBe("#d6d7da");
    expect(hlWinPct(61)).toBe("#ffffff");
    expect(hlKd(1.3)).toBe("#ffffff");
    expect(hlKd(0.95)).toBe("#a3a7ae");
    expect(hlHs(28)).toBe("#ffffff");
    expect(hlHs(19)).toBe("#a3a7ae");
    expect(hlHs(23)).toBe("#d0d2d6");
  });
});
