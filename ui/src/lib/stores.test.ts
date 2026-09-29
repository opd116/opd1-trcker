import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { get } from "svelte/store";
import type { LiveState, Meta } from "./types";

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
      const listen = vi.fn(async () => () => {});
      vi.doMock("@tauri-apps/api/core", () => ({ isTauri: () => true }));
      vi.doMock("./api", () => ({ tauriBridge: { invoke, listen } }));
      vi.spyOn(console, "error").mockImplementation(() => {});
      return { ...(await load()), invoke, metaCalls: () => metaCalls };
    }

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
