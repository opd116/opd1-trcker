import { describe, expect, it } from "vitest";
import { smurfSignal } from "./smurf";
import type { PlayerCard } from "./types";

const card = (p: Partial<PlayerCard>): PlayerCard => ({
  puuid: "p",
  cardId: "c",
  agentId: null,
  party: null,
  encounters: null,
  team: "enemy",
  isSelfParty: false,
  isSelf: false,
  level: 300,
  hsPct: 0.22,
  actWinrate: 0.5,
  actGames: 80,
  ...p,
});

const lobby = [card({ puuid: "a" }), card({ puuid: "b" }), card({ puuid: "c", hsPct: 0.24 })];

describe("smurfSignal", () => {
  const wisp = card({ puuid: "wisp", level: 41, hsPct: 0.38, actWinrate: 0.81, actGames: 16 });

  it("flags the design's sample (all four signals) at Cautious", () => {
    const s = smurfSignal(wisp, [...lobby, wisp], "cautious");
    expect(s?.reasons).toHaveLength(4);
    expect(s?.reasons[0]).toMatch(/^Account level 41; this lobby averages \d+$/);
  });

  it("needs 3 signals at Cautious and 2 at Eager", () => {
    const two = card({ puuid: "x", level: 41, actGames: 16 });
    expect(smurfSignal(two, [...lobby, two], "cautious")).toBeNull();
    expect(smurfSignal(two, [...lobby, two], "eager")?.reasons).toHaveLength(2);
  });

  it("never flags with Off, the user, or a dismissed player", () => {
    expect(smurfSignal(wisp, lobby, "off")).toBeNull();
    expect(smurfSignal({ ...wisp, isSelf: true }, lobby, "eager")).toBeNull();
    expect(smurfSignal(wisp, lobby, "eager", ["wisp"])).toBeNull();
  });

  it("ignores inputs that are still loading or unknown", () => {
    const loading = card({ puuid: "y", level: undefined, hsPct: undefined, actWinrate: null, actGames: undefined });
    expect(smurfSignal(loading, lobby, "eager")).toBeNull();
  });
});
