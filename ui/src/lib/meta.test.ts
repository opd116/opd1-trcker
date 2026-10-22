import { describe, expect, it } from "vitest";
import { agentName, isRankedTier, lookup, mapName } from "./meta";
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

describe("helpers that stay", () => {
  it("isRankedTier starts at 3", () => {
    expect(isRankedTier(2)).toBe(false);
    expect(isRankedTier(3)).toBe(true);
    expect(isRankedTier(null)).toBe(false);
  });
});
