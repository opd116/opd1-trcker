// The History list's module cache must not outlive a Riot account switch: a remount (Back from a scoreboard, or the
// screen reopened) draws it before its refresh answers, so the old account's rows would show for the new one — and an
// instance the switch unmounted can still answer a request it made for the previous account.
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
  const cache = await import("./cache");
  return { ...stores, ...cache };
}

describe("historyCache", () => {
  it("is cleared when the Riot account changes, and starts a new epoch", async () => {
    const { applyLive, historyCache, putHistoryPage } = await load();
    applyLive(liveWith("account-a"));
    const epoch = historyCache.epoch;
    expect(putHistoryPage(epoch, "competitive", page("a-match"))).toBe(true);
    expect(historyCache.pages.competitive?.rows[0]?.matchId).toBe("a-match");
    applyLive(liveWith("account-b"));
    expect(historyCache.pages).toEqual({});
    expect(historyCache.epoch).not.toBe(epoch);
  });

  it("keeps its pages across a stage change of the same account", async () => {
    const { applyLive, historyCache, putHistoryPage } = await load();
    applyLive(liveWith("account-a", "ingame"));
    putHistoryPage(historyCache.epoch, "competitive", page("a-match"));
    applyLive(liveWith("account-a", "menus"));
    expect(historyCache.pages.competitive?.rows[0]?.matchId).toBe("a-match");
  });

  it("a page asked for under the old account's epoch cannot repopulate the cache after a switch", async () => {
    const { applyLive, historyCache, putHistoryPage } = await load();
    applyLive(liveWith("account-a"));
    const oldEpoch = historyCache.epoch;
    expect(putHistoryPage(oldEpoch, "competitive", page("a-match"))).toBe(true);
    applyLive(liveWith("account-b")); // the switch: the module handler clears the cache and starts a new epoch
    // The unmounted list's request for account A answers late: it must not write A's page back.
    expect(putHistoryPage(oldEpoch, "competitive", page("a-late"))).toBe(false);
    expect(historyCache.pages).toEqual({});
    // The new account's request writes normally.
    expect(putHistoryPage(historyCache.epoch, "competitive", page("b-match"))).toBe(true);
    expect(historyCache.pages.competitive?.rows[0]?.matchId).toBe("b-match");
  });

  it("starts one epoch per account (a second handler call does not bump again)", async () => {
    const { applyLive, historyCache, historyCacheEpoch } = await load();
    applyLive(liveWith("account-a"));
    expect(historyCache.epoch).toBe(0);
    applyLive(liveWith("account-b")); // this module's own account handler starts B's epoch
    const now = historyCache.epoch;
    expect(now).toBe(1);
    // The mounted list's handler calls it too: same account, so no second bump.
    expect(historyCacheEpoch()).toBe(now);
    expect(historyCache.epoch).toBe(now);
  });
});
