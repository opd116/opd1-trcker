import { describe, expect, it } from "vitest";
import { cardDescription, displayNameText, peakTip, prevActTip, rankTip } from "./cardTips";
import type { Meta, PlayerCard } from "./types";

const meta: Meta = {
  agents: { "add6443a-41bd-e414-f6ad-e58d267f4e95": { name: "Jett" } },
  maps: {},
  tiers: {
    "15": { name: "PLATINUM 1", color: "59a9b6ff" },
    "18": { name: "DIAMOND 1", color: "b489c4ff" },
    "25": { name: "IMMORTAL 2", color: "bb3d65ff" },
  },
};

const NOW = Date.parse("2026-09-27T12:00:00Z");

const card: PlayerCard = {
  puuid: "p1",
  name: { kind: "visible", name: "Falcon", tag: "2211" },
  level: 214,
  cardId: "c1",
  agentId: "add6443a-41bd-e414-f6ad-e58d267f4e95",
  rank: { tier: 18, rr: 64 },
  prevActTier: 15,
  peak: {
    tier: 18,
    actLabel: "V26 A5",
    actName: "V26 ACT V",
    sinceActEndMs: null,
    actStartMs: Date.parse("2026-08-19T00:00:00Z"),
    actEndMs: Date.parse("2026-10-14T00:00:00Z"),
  },
  kd: 1.18,
  hsPct: 0.231,
  actWinrate: 0.545,
  last5: [{ win: true, rrEarned: 18, startMs: NOW - 2 * 3_600_000 }],
  party: { group: 0, confirmed: false },
  encounters: { with: 2, against: 1 },
  team: "ally",
  isSelfParty: false,
};

describe("peakTip", () => {
  it("shows the full act name with its dates", () => {
    expect(peakTip(meta, card.peak)).toEqual(["Peak · Diamond 1", "V26 ACT V · 19 Aug – 14 Oct 2026", "This act"]);
  });

  it("says how long ago a past act ended", () => {
    const past = {
      tier: 15,
      actLabel: "E9 A3",
      actName: "EPISODE 9 ACT III",
      sinceActEndMs: NOW - Date.parse("2025-01-08T00:00:00Z"),
      actStartMs: Date.parse("2024-10-23T00:00:00Z"),
      actEndMs: Date.parse("2025-01-08T00:00:00Z"),
    };
    expect(peakTip(meta, past)).toEqual([
      "Peak · Platinum 1",
      "EPISODE 9 ACT III · 23 Oct 2024 – 8 Jan 2025",
      "Act ended 1y ago",
    ]);
  });

  it("has no tooltip while loading and a plain one when missing", () => {
    expect(peakTip(meta, undefined)).toBeNull();
    expect(peakTip(meta, null)).toEqual(["Peak rank", "No ranked history"]);
  });
});

describe("rank tips", () => {
  it("names the tier and the RR to the next one", () => {
    expect(rankTip(meta, { tier: 18, rr: 64 })).toEqual(["Diamond 1", "64 RR · 36 RR to next tier"]);
    expect(rankTip(meta, { tier: 25, rr: 187 })).toEqual(["Immortal 2", "187 RR"]);
    expect(rankTip(meta, null)).toEqual(["Unranked"]);
    expect(prevActTip(meta, 15)).toEqual(["Previous act", "Platinum 1"]);
  });
});

describe("displayNameText", () => {
  it("follows the hidden-name rules", () => {
    expect(displayNameText(meta, card.name)).toBe("Falcon#2211");
    expect(displayNameText(meta, { kind: "hiddenPlayer" })).toBe("Hidden Player");
    expect(displayNameText(meta, { kind: "hiddenAgent", agentId: "add6443a-41bd-e414-f6ad-e58d267f4e95" })).toBe(
      "Jett",
    );
  });
});

describe("cardDescription", () => {
  it("carries every hover-only detail for screen readers", () => {
    const d = cardDescription(meta, card, NOW);
    expect(d).toContain("Rank Diamond 1, 64 RR · 36 RR to next tier");
    expect(d).toContain("Previous act Platinum 1");
    expect(d).toContain("V26 ACT V · 19 Aug – 14 Oct 2026");
    expect(d).toContain("Account level 214");
    expect(d).toContain("Win +18 RR 2h ago");
    expect(d).toContain("Likely party");
    expect(d).toContain("Encounters 2 with, 1 against");
  });

  it("leaves out loading fields and never marks the user's own party", () => {
    const loading: PlayerCard = { ...card, rank: undefined, peak: undefined, last5: undefined, isSelfParty: true };
    const d = cardDescription(meta, loading, NOW);
    expect(d).not.toContain("Rank");
    expect(d).not.toContain("Peak");
    expect(d).not.toContain("Last 5");
    expect(d).not.toContain("party");
    expect(d).not.toContain("Encounters");
  });
});
