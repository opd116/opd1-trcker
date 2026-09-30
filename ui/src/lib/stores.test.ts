import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { get } from "svelte/store";
import type { LiveState, Meta, PlayerDetails } from "./types";

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

  describe("startFeeds under Tauri", () => {
    const good: Meta = { agents: {}, maps: { m1: { name: "Ascent" } }, tiers: {} };

    afterEach(() => {
      vi.useRealTimers();
      vi.restoreAllMocks(); // the console.error spy from `underTauri`
      vi.doUnmock("@tauri-apps/api/core");
      vi.doUnmock("./api");
    });

    /** Stores wired to a fake Tauri bridge whose `meta` command rejects `failures` times before it resolves. */
    async function underTauri(failures: number) {
      let metaCalls = 0;
      const invoke = vi.fn(async (cmd: string) => {
        if (cmd === "live_state") return state("closed");
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
      vi.doMock("./api", () => ({ tauriBridge: { invoke, listen } }));
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
