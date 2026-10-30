// The History list's module cache must not outlive a Riot account switch: a remount (Back from a scoreboard, or the
// screen reopened) draws it before its refresh answers, so the old account's rows would show for the new one.
import { describe, expect, it, vi } from "vitest";
import type { HistoryPage, LiveState, PlayerCard } from "../../lib/types";

const page = (matchId: string): HistoryPage => ({
  rows: [
    {
      matchId,
      mapId: null,
      queueId: "competitive",
      startMs: 1,
      durationMs: null,
      result: "win",
      roundsWon: 13,
      roundsLost: 5,
      agentId: null,
      kills: 1,
      deaths: 1,
      assists: 1,
      kd: 1,
      acs: 100,
      hsPct: null,
      rrEarned: 10,
      tierAfter: 21,
    },
  ],
  total: 1,
  competitiveTotal: 1,
  allTotal: 1,
});

const liveWith = (puuid: string, stage: LiveState["stage"] = "menus"): LiveState => ({
  stage,
  mapId: null,
  players: [{ puuid, isSelf: true } as PlayerCard],
  settled: true,
});

// A fresh module graph per test: the cache and the account signal are module state.
async function load() {
  vi.resetModules();
  const stores = await import("../../lib/stores");
  const { historyCache } = await import("./cache");
  return { ...stores, historyCache };
}

describe("historyCache", () => {
  it("is cleared when the Riot account changes", async () => {
    const { applyLive, historyCache } = await load();
    applyLive(liveWith("account-a"));
    historyCache.pages.competitive = page("a-match");
    expect(historyCache.pages.competitive?.rows[0]?.matchId).toBe("a-match");
    applyLive(liveWith("account-b"));
    expect(historyCache.pages).toEqual({});
  });

  it("keeps its pages across a stage change of the same account", async () => {
    const { applyLive, historyCache } = await load();
    applyLive(liveWith("account-a", "ingame"));
    historyCache.pages.competitive = page("a-match");
    applyLive(liveWith("account-a", "menus"));
    expect(historyCache.pages.competitive?.rows[0]?.matchId).toBe("a-match");
  });
});
