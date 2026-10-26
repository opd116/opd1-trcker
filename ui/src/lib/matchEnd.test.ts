import { writable } from "svelte/store";
import { afterEach, describe, expect, it, vi } from "vitest";
import { AFTER_MATCH_MS, followMatchEnds } from "./matchEnd";
import type { LiveState } from "./types";

const at = (stage: LiveState["stage"]): LiveState => ({ stage, mapId: null, players: [] }) as unknown as LiveState;

describe("followMatchEnds", () => {
  afterEach(() => vi.useRealTimers());

  it("asks on the schedule after In Game ends, and only then", async () => {
    vi.useFakeTimers();
    const live = writable<LiveState | null>(at("menus"));
    const ask = vi.fn(async () => false);
    const stop = followMatchEnds(live, () => ask);
    live.set(at("pregame"));
    live.set(at("menus")); // a dodge: no match was played
    await vi.advanceTimersByTimeAsync(200_000);
    expect(ask).not.toHaveBeenCalled();

    live.set(at("ingame"));
    live.set(at("menus"));
    await vi.advanceTimersByTimeAsync(AFTER_MATCH_MS[0]! - 1);
    expect(ask).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1);
    expect(ask).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(AFTER_MATCH_MS.at(-1)!);
    expect(ask).toHaveBeenCalledTimes(AFTER_MATCH_MS.length);
    await vi.advanceTimersByTimeAsync(500_000);
    expect(ask).toHaveBeenCalledTimes(AFTER_MATCH_MS.length);
    stop();
  });

  it("stops once the asker answers true; a newer match end starts a new run", async () => {
    vi.useFakeTimers();
    const live = writable<LiveState | null>(at("ingame"));
    let calls = 0;
    const start = vi.fn(() => async () => ++calls >= 2);
    const stop = followMatchEnds(live, start);
    live.set(at("closed"));
    await vi.advanceTimersByTimeAsync(AFTER_MATCH_MS.at(-1)!);
    expect(calls).toBe(2);
    expect(start).toHaveBeenCalledTimes(1);

    live.set(at("ingame"));
    live.set(at("menus"));
    expect(start).toHaveBeenCalledTimes(2);
    await vi.advanceTimersByTimeAsync(AFTER_MATCH_MS[0]!);
    expect(calls).toBe(3);
    stop();
  });

  it("stopped: no more asks", async () => {
    vi.useFakeTimers();
    const live = writable<LiveState | null>(at("ingame"));
    const ask = vi.fn(async () => false);
    const stop = followMatchEnds(live, () => ask);
    live.set(at("menus"));
    stop();
    await vi.advanceTimersByTimeAsync(500_000);
    expect(ask).not.toHaveBeenCalled();
  });
});
