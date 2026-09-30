// The catalogue's exact strings are in tooltip.test.ts. This file checks the rules
// every entry must obey: hidden names never leak, and no missing part prints as text.
import { describe, expect, it } from "vitest";
import { aprTip, encounterMapTip, encounterTip, heroStatTip, last5Tip, liveTip, loadoutTip, nameTip, partyTip, peakTip, rankTip, winTip } from "./tips";
import type { DisplayName, Meta, PlayerCard } from "./types";

const meta: Meta = { agents: { a1: { name: "Jett" } }, maps: { m1: { name: "Haven" } }, tiers: {} };
const NOW = Date.parse("2026-09-29T21:59:00Z");

const NAMES: DisplayName[] = [
  { kind: "visible", name: "Fahad", tag: "OPD1" },
  { kind: "remembered", name: "Rook", tag: "4413", fromMapId: "m1", fromStartMs: NOW },
  { kind: "remembered", name: "Rook", tag: "4413", fromMapId: "gone", fromStartMs: NOW },
  { kind: "remembered", name: "Rook", tag: "4413", fromMapId: null, fromStartMs: null },
  { kind: "hiddenPlayer" },
  { kind: "hiddenAgent", agentId: "a1" },
  { kind: "hiddenAgent", agentId: "gone" },
];

describe("nameTip", () => {
  it("never contains undefined or null, with or without meta", () => {
    for (const n of NAMES) for (const m of [meta, null]) expect(nameTip(n, m)).not.toMatch(/undefined|null/);
  });

  it("never carries a hidden player's name or tag", () => {
    for (const m of [meta, null]) {
      expect(nameTip({ kind: "hiddenPlayer" }, m)).toBe("Hidden name");
      expect(nameTip({ kind: "hiddenAgent", agentId: "a1" }, m)).not.toContain("Jett");
    }
  });

  it("a remembered tip is two lines at most", () => {
    for (const n of NAMES) expect(nameTip(n, meta).split("\n").length).toBeLessThanOrEqual(2);
  });
});

describe("other entries stay clean", () => {
  it("no entry prints undefined or NaN for odd input", () => {
    const out = [
      encounterTip({ with: 0, against: 0 }),
      partyTip({ group: 3, confirmed: false }),
      aprTip(0),
      last5Tip({ result: "win", rrEarned: null, startMs: NOW }, NOW),
      last5Tip({ result: "draw", rrEarned: 0, startMs: NOW + 5 * 60_000 }, NOW),
      winTip({ actWins: 0, actGames: 0 } as PlayerCard),
      peakTip({ tier: 3, actLabel: "X", actName: "unparseable", sinceActEndMs: 1, actStartMs: 0, actEndMs: 86_400_000 }),
      heroStatTip(null, 0),
      heroStatTip(1, 0),
      rankTip(null, 21),
      rankTip(meta, 21),
      encounterMapTip({ startMs: NOW, durationMs: null }, NOW),
      liveTip(null, null),
      liveTip(meta, "gone"),
      loadoutTip(null),
    ];
    for (const s of out) expect(s).not.toMatch(/undefined|null|NaN/);
  });

  it("an unparseable act name is passed through in the peak tip", () => {
    const tip = peakTip({
      tier: 3,
      actLabel: "X",
      actName: "SOME ACT",
      sinceActEndMs: 1,
      actStartMs: Date.UTC(2026, 0, 7),
      actEndMs: Date.UTC(2026, 2, 4),
    });
    expect(tip).toBe("SOME ACT\nJan 7 – Mar 3, 2026");
  });
});
