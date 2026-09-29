import { beforeEach, describe, expect, it, vi } from "vitest";
import { get } from "svelte/store";
import type { LiveState } from "./types";

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
});
