import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { get } from "svelte/store";
import type { LiveState, MatchDetailsView, Meta, MyStats, PlayerDetails } from "./types";

const state = (stage: LiveState["stage"]): LiveState => ({ stage, mapId: null, players: [] });

// A fresh module graph per test: stores.ts keeps the previous stage in module state.
async function load() {
  vi.resetModules();
  const stores = await import("./stores");
  const router = await import("./router.svelte");
  return { ...stores, ...router };
}

describe("stores", () => {
  beforeEach(() => vi.resetModules());

  it("live and meta start null: nothing received yet, meta pending (spec §5.8)", async () => {
    const { live, meta, openDetails } = await load();
    expect(get(live)).toBeNull();
    expect(get(meta)).toBeNull();
    expect(get(openDetails)).toBeNull();
  });

  it("mystats starts undefined: nothing answered yet, the page draws its skeleton (spec §9.1)", async () => {
    const { mystats } = await load();
    expect(get(mystats)).toBeUndefined();
  });

  it("now is the wall clock when a subscription starts, not the module-load time (Minor 1)", async () => {
    vi.useFakeTimers();
    try {
      vi.setSystemTime(new Date("2026-09-29T10:00:00Z"));
      const { now } = await load();
      // Hours pass with nobody subscribed (VALORANT closed, no card mounted): no tick ever ran.
      vi.setSystemTime(new Date("2026-09-29T15:30:00Z"));
      const seen: number[] = [];
      const off = now.subscribe((v) => seen.push(v));
      expect(seen[seen.length - 1]).toBe(new Date("2026-09-29T15:30:00Z").getTime());
      // ...and the 30s tick still moves it on.
      await vi.advanceTimersByTimeAsync(30_000);
      expect(seen[seen.length - 1]).toBe(new Date("2026-09-29T15:30:30Z").getTime());
      off();
    } finally {
      vi.useRealTimers();
    }
  });

  it("applyLive stores the state", async () => {
    const { live, applyLive } = await load();
    applyLive(state("menus"));
    expect(get(live)?.stage).toBe("menus");
    applyLive(null);
    expect(get(live)).toBeNull();
  });

  it("entering Agent Select switches to Live; later pregame updates leave the route alone", async () => {
    const { applyLive, go, router } = await load();
    applyLive(state("menus"));
    go({ name: "details", puuid: "p1", more: false });
    applyLive(state("pregame"));
    expect(router.route).toEqual({ name: "live" });
    go({ name: "details", puuid: "p1", more: false });
    applyLive(state("pregame")); // the same stage again: the user may be browsing Details
    expect(router.route).toEqual({ name: "details", puuid: "p1", more: false });
    applyLive(state("ingame"));
    applyLive(state("menus"));
    applyLive(state("pregame")); // the next match
    expect(router.route).toEqual({ name: "live" });
  });

  it("a null LiveState forgets the stage, so the next pregame counts as entering it", async () => {
    const { applyLive, go, router } = await load();
    applyLive(state("pregame"));
    applyLive(null);
    go({ name: "details", puuid: "p1", more: false });
    applyLive(state("pregame"));
    expect(router.route).toEqual({ name: "live" });
  });

  describe("the start page (spec §2.4, R71 ended)", () => {
    it("the first LiveState picks it: closed opens My Stats, anything else stays on Live", async () => {
      for (const stage of ["menus", "pregame", "ingame"] as const) {
        const { applyLive, router } = await load();
        applyLive(state(stage));
        expect(router.route, stage).toEqual({ name: "live" });
      }
      const { applyLive, router } = await load();
      applyLive(state("closed"));
      expect(router.route).toEqual({ name: "mystats" });
    });

    it("only once: a later closed state (the game quits) leaves the user where they are", async () => {
      const { applyLive, router } = await load();
      applyLive(state("menus"));
      applyLive(state("closed"));
      expect(router.route).toEqual({ name: "live" });
    });

    it("a user who already navigated is not moved", async () => {
      const { applyLive, go, router } = await load();
      go({ name: "details", puuid: "p1", more: false });
      applyLive(state("closed"));
      expect(router.route).toEqual({ name: "details", puuid: "p1", more: false });
    });

    it("entering Agent Select still forces Live from My Stats", async () => {
      const { applyLive, go, router } = await load();
      applyLive(state("menus"));
      go({ name: "mystats" });
      applyLive(state("pregame"));
      expect(router.route).toEqual({ name: "live" });
    });
  });

  describe("startFeeds under Tauri", () => {
    const good: Meta = { agents: {}, maps: { m1: { name: "Ascent" } }, tiers: {} };

    afterEach(() => {
      vi.useRealTimers();
      vi.restoreAllMocks(); // the console.error spy from `underTauri`
      vi.doUnmock("@tauri-apps/api/core");
      vi.doUnmock("./api");
    });

    /** Stores wired to a fake Tauri bridge whose `meta` command rejects `failures` times before it resolves. */
    async function underTauri(failures: number, myStatsAnswer: () => Promise<MyStats | null> = async () => null) {
      let metaCalls = 0;
      const invoke = vi.fn(async (cmd: string) => {
        if (cmd === "live_state") return state("closed");
        if (cmd === "my_stats") return myStatsAnswer();
        if (cmd === "meta") {
          if (metaCalls++ < failures) throw new Error("asset tables unavailable");
          return good;
        }
        throw new Error(`unexpected ${cmd}`);
      });
      const handlers = new Map<string, (e: { payload: unknown }) => void>();
      const listen = vi.fn(async (ev: string, cb: (e: { payload: unknown }) => void) => {
        handlers.set(ev, cb);
        return () => {};
      });
      vi.doMock("@tauri-apps/api/core", () => ({ isTauri: () => true }));
      vi.doMock("./api", () => ({ tauriBridge: { invoke, listen }, myStats: () => invoke("my_stats") }));
      vi.spyOn(console, "error").mockImplementation(() => {});
      const emit = (ev: string, payload: unknown) => handlers.get(ev)!({ payload });
      return { ...(await load()), invoke, emit, metaCalls: () => metaCalls };
    }

    it("a details event reaches the open-details slot and wins over the answer in flight (Task 18 review, Minor 2)", async () => {
      const { startFeeds, openDetails, emit } = await underTauri(0);
      await startFeeds();
      const d = (marker: string) => ({ card: { puuid: "p1" }, marker }) as unknown as PlayerDetails;
      let answer!: (v: PlayerDetails) => void;
      const done = openDetails.open("p1", () => new Promise<PlayerDetails>((res) => (answer = res)));
      emit("details", d("event"));
      answer(d("snapshot"));
      await done;
      expect(get(openDetails)).toEqual(d("event"));
    });

    it("a match event reaches the open-match slot, wins over the answer in flight, and only for the open match (R150)", async () => {
      const { startFeeds, openMatch, emit } = await underTauri(0);
      await startFeeds();
      const v = (matchId: string, marker: string) => ({ matchId, marker }) as unknown as MatchDetailsView;
      let answer!: (v: MatchDetailsView) => void;
      const done = openMatch.open("m1", () => new Promise<MatchDetailsView>((res) => (answer = res)));
      emit("match", v("m2", "another match")); // ignored
      expect(get(openMatch)).toBeNull();
      emit("match", v("m1", "names"));
      answer(v("m1", "snapshot"));
      await done;
      expect(get(openMatch)).toEqual(v("m1", "names"));
      openMatch.close(); // the page was left: an event after that changes nothing
      emit("match", v("m1", "after leaving"));
      expect(get(openMatch)).toBeNull();
    });

    const stats = (marker: string) => ({ marker }) as unknown as MyStats;

    it("startFeeds asks my_stats once and applies the answer", async () => {
      const { startFeeds, mystats, invoke } = await underTauri(0, async () => stats("snapshot"));
      await startFeeds();
      expect(get(mystats)).toEqual(stats("snapshot"));
      expect(invoke.mock.calls.filter(([cmd]) => cmd === "my_stats")).toHaveLength(1);
    });

    it("a null my_stats answer is not the skeleton: the page settles on its frames", async () => {
      const { startFeeds, mystats } = await underTauri(0, async () => null);
      await startFeeds();
      expect(get(mystats)).toBeNull();
    });

    it("a mystats event replaces the data, and wins over the snapshot still in flight", async () => {
      let answer!: (v: MyStats | null) => void;
      const { startFeeds, mystats, emit } = await underTauri(0, () => new Promise<MyStats | null>((res) => (answer = res)));
      const started = startFeeds();
      await vi.waitFor(() => expect(answer).toBeTypeOf("function"));
      emit("mystats", stats("event"));
      answer(stats("snapshot"));
      await started;
      expect(get(mystats)).toEqual(stats("event"));
      emit("mystats", stats("newer")); // later events keep replacing it
      expect(get(mystats)).toEqual(stats("newer"));
    });

    it("a failed my_stats() does not leave the skeleton up forever: null now, a later event still fills it", async () => {
      const { startFeeds, mystats, emit } = await underTauri(0, async () => {
        throw new Error("db busy");
      });
      await startFeeds();
      expect(get(mystats)).toBeNull();
      expect(console.error).toHaveBeenCalledWith("my_stats failed", expect.any(Error));
      emit("mystats", stats("event"));
      expect(get(mystats)).toEqual(stats("event"));
    });

    it("refreshMyStats asks again only while the page has nothing, and never overwrites what landed meanwhile", async () => {
      let calls = 0;
      let answer!: (v: MyStats | null) => void;
      const { startFeeds, mystats, refreshMyStats, emit } = await underTauri(0, () => {
        calls++;
        return calls === 1 ? Promise.resolve(null) : new Promise<MyStats | null>((res) => (answer = res));
      });
      await startFeeds();
      expect(get(mystats)).toBeNull();
      const refreshing = refreshMyStats(); // the page opened with nothing to show
      emit("mystats", stats("event")); // an event lands first
      answer(stats("snapshot"));
      await refreshing;
      expect(get(mystats)).toEqual(stats("event"));
      await refreshMyStats(); // data on screen: no more calls
      expect(calls).toBe(2);
    });

    it("a rejected meta() does not leave meta pending: EMPTY_META now, the real tables on a retry", async () => {
      vi.useFakeTimers();
      const { startFeeds, meta, live, invoke } = await underTauri(1);
      await startFeeds(); // resolves without waiting for meta
      expect(get(live)?.stage).toBe("closed"); // the live feed is not held up by a failing meta()
      await vi.advanceTimersByTimeAsync(0);
      expect(get(meta)).toEqual({ agents: {}, maps: {}, tiers: {} }); // loaded-miss "—", never a skeleton forever
      expect(get(meta)).not.toBeNull();
      expect(console.error).toHaveBeenCalledTimes(1);

      await vi.advanceTimersByTimeAsync(2_000);
      expect(get(meta)).toEqual(good);
      expect(invoke.mock.calls.filter(([cmd]) => cmd === "meta")).toHaveLength(2);
    });

    it("a healthy meta() is set once with no retry", async () => {
      vi.useFakeTimers();
      const { startFeeds, meta, metaCalls } = await underTauri(0);
      await startFeeds();
      await vi.advanceTimersByTimeAsync(0);
      expect(get(meta)).toEqual(good);
      await vi.advanceTimersByTimeAsync(120_000);
      expect(metaCalls()).toBe(1);
      expect(console.error).not.toHaveBeenCalled();
    });
  });
});
