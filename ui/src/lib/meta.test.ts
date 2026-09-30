import { describe, expect, it } from "vitest";
import { agentName, cssColor, isRankedTier, lookup, mapName, tierColor, tierLabel } from "./meta";
import type { Meta } from "./types";

const JETT = "add6443a-41bd-e414-f6ad-e58d267f4e95";
const HAVEN = "2bee0dc9-4ffe-519b-1cbd-7fbe763a6047";

const meta: Meta = {
  agents: { [JETT]: { name: "Jett" } },
  maps: { [HAVEN]: { name: "Haven" } },
  tiers: {
    "21": { name: "ASCENDANT 1", color: "2a9d8fff" },
    "24": { name: "IMMORTAL 1", color: "bb3d65ff" },
  },
};

describe("lookup (spec §5.8, the one copy of this rule)", () => {
  it("lookup: pending meta (null) → undefined, null id → null, missing id → null, known id → name", () => {
    expect(lookup(null, "agents", JETT)).toBeUndefined();
    expect(lookup(meta, "agents", null)).toBeNull();
    expect(lookup(meta, "agents", undefined)).toBeNull();
    expect(lookup(meta, "agents", "")).toBeNull();
    expect(lookup(meta, "agents", "not-in-meta")).toBeNull();
    expect(lookup(meta, "agents", JETT)).toBe("Jett");
    expect(lookup(meta, "maps", HAVEN)).toBe("Haven");
    expect(lookup(meta, "maps", JETT)).toBeNull();
  });

  it("no id is null even while meta is pending (there is nothing to wait for)", () => {
    expect(lookup(null, "maps", null)).toBeNull();
    expect(lookup(null, "maps", undefined)).toBeNull();
  });

  it("agentName and mapName are lookup on their table", () => {
    expect(agentName(null, JETT)).toBeUndefined();
    expect(agentName(meta, JETT)).toBe("Jett");
    expect(agentName(meta, "x")).toBeNull();
    expect(agentName(meta, null)).toBeNull();
    expect(mapName(null, HAVEN)).toBeUndefined();
    expect(mapName(meta, HAVEN)).toBe("Haven");
    expect(mapName(meta, "x")).toBeNull();
    expect(mapName(meta, undefined)).toBeNull();
  });
});

describe("tierLabel and tierColor", () => {
  it("tierLabel follows the same rule; tier < 3 → null", () => {
    expect(tierLabel(null, 21)).toBeUndefined();
    expect(tierLabel(meta, 21)).toBe("Ascendant 1");
    expect(tierLabel(meta, 99)).toBeNull();
    expect(tierLabel(meta, 2)).toBeNull();
    expect(tierLabel(meta, 0)).toBeNull();
    expect(tierLabel(meta, null)).toBeNull();
    expect(tierLabel(meta, undefined)).toBeNull();
    // unranked never waits for meta
    expect(tierLabel(null, 0)).toBeNull();
    expect(tierLabel(null, null)).toBeNull();
  });

  it("tierColor is a css hex, or undefined while pending/unknown", () => {
    expect(tierColor(meta, 24)).toBe("#bb3d65");
    expect(tierColor(null, 24)).toBeUndefined();
    expect(tierColor(meta, 99)).toBeUndefined();
    expect(tierColor(meta, 2)).toBeUndefined();
  });
});

describe("helpers that stay", () => {
  it("isRankedTier starts at 3", () => {
    expect(isRankedTier(2)).toBe(false);
    expect(isRankedTier(3)).toBe(true);
    expect(isRankedTier(null)).toBe(false);
  });

  it("cssColor turns rrggbbaa into #rrggbb", () => {
    expect(cssColor("bb3d65ff")).toBe("#bb3d65");
    expect(cssColor("#bb3d65")).toBe("#bb3d65");
    expect(cssColor(undefined)).toBeUndefined();
  });
});
