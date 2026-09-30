import { get } from "svelte/store";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  META_RETRY_FIRST_MS,
  META_RETRY_MAX_MS,
  detailsSlot,
  keyAction,
  loadMeta,
  runKeyAction,
  startLive,
  type Bridge,
} from "./boot";
import { EMPTY_META } from "./meta";
import type { LiveState, Meta, PlayerDetails } from "./types";

const live = (stage: LiveState["stage"]): LiveState => ({ stage, mapId: null, players: [] });

function fake() {
  const calls: string[] = [];
  const handlers: Array<(e: { payload: LiveState }) => void> = [];
  let resolveInvoke!: (s: LiveState) => void;
  const b: Bridge = {
    listen: (async (ev: string, cb: (e: { payload: LiveState }) => void) => {
      calls.push(`listen:${ev}`);
      handlers.push(cb);
      return () => {};
    }) as Bridge["listen"],
    invoke: ((cmd: string) => {
      calls.push(`invoke:${cmd}`);
      return new Promise<LiveState>((res) => (resolveInvoke = res));
    }) as Bridge["invoke"],
  };
  return { b, calls, emit: (s: LiveState) => handlers.forEach((h) => h({ payload: s })), resolve: (s: LiveState) => resolveInvoke(s) };
}

describe("startLive", () => {
  it("listen before invoke", async () => {
    const f = fake();
    const applied: LiveState[] = [];
    const done = startLive(f.b, (s) => applied.push(s));
    await vi.waitFor(() => expect(f.calls).toContain("invoke:live_state"));
    expect(f.calls).toEqual(["listen:live", "invoke:live_state"]);
    f.resolve(live("menus"));
    await done;
    expect(applied).toEqual([live("menus")]);
  });

  it("invoke result ignored after an event", async () => {
    const f = fake();
    const applied: LiveState[] = [];
    const done = startLive(f.b, (s) => applied.push(s));
    await vi.waitFor(() => expect(f.calls).toContain("invoke:live_state"));
    f.emit(live("ingame")); // an event arrives while live_state is in flight
    f.resolve(live("menus")); // the older snapshot must not overwrite it
    await done;
    expect(applied).toEqual([live("ingame")]);
  });

  it("later events keep applying", async () => {
    const f = fake();
    const applied: string[] = [];
    const done = startLive(f.b, (s) => applied.push(s.stage));
    await vi.waitFor(() => expect(f.calls).toContain("invoke:live_state"));
    f.resolve(live("menus"));
    await done;
    f.emit(live("pregame"));
    f.emit(live("ingame"));
    expect(applied).toEqual(["menus", "pregame", "ingame"]);
  });
});

describe("keyAction", () => {
  const k = (key: string, mods: Partial<Record<"ctrlKey" | "shiftKey" | "altKey" | "metaKey", boolean>> = {}) => ({
    key,
    ctrlKey: false,
    shiftKey: false,
    altKey: false,
    metaKey: false,
    ...mods,
  });

  it("Ctrl+Shift+L copies the log and Ctrl+Shift+D dumps the raw captures", () => {
    expect(keyAction(k("L", { ctrlKey: true, shiftKey: true }))).toBe("copyLog");
    expect(keyAction(k("l", { ctrlKey: true, shiftKey: true }))).toBe("copyLog");
    expect(keyAction(k("D", { ctrlKey: true, shiftKey: true }))).toBe("dumpRaw");
  });

  it("nothing else does", () => {
    expect(keyAction(k("L", { ctrlKey: true }))).toBeNull();
    expect(keyAction(k("L", { shiftKey: true }))).toBeNull();
    expect(keyAction(k("L"))).toBeNull();
    expect(keyAction(k("L", { ctrlKey: true, shiftKey: true, altKey: true }))).toBeNull();
    expect(keyAction(k("L", { ctrlKey: true, shiftKey: true, metaKey: true }))).toBeNull();
    expect(keyAction(k("K", { ctrlKey: true, shiftKey: true }))).toBeNull();
    expect(keyAction(k("1"))).toBeNull();
  });
});

describe("runKeyAction", () => {
  it("copyLog invokes copy_log and writes the text to the clipboard", async () => {
    const invoke = vi.fn(async () => "log text");
    const write = vi.fn(async () => {});
    await runKeyAction("copyLog", { invoke } as unknown as Bridge, write);
    expect(invoke).toHaveBeenCalledWith("copy_log");
    expect(write).toHaveBeenCalledWith("log text");
  });

  it("dumpRaw invokes dump_raw and touches no clipboard", async () => {
    const invoke = vi.fn(async () => ({ dir: "x", files: 3 }));
    const write = vi.fn(async () => {});
    await runKeyAction("dumpRaw", { invoke } as unknown as Bridge, write);
    expect(invoke).toHaveBeenCalledWith("dump_raw");
    expect(write).not.toHaveBeenCalled();
  });
});

describe("detailsSlot (spec §2.4, §6.9: the open Player Details)", () => {
  const details = (puuid: string, marker: string) => ({ card: { puuid }, marker }) as unknown as PlayerDetails;
  const now = (slot: ReturnType<typeof detailsSlot>) => get(slot) as (PlayerDetails & { marker: string }) | null;
  /** A `player_details` call held open until the test answers it. */
  function deferred() {
    let answer!: (d: PlayerDetails) => void;
    const calls: string[] = [];
    const fetch = (puuid: string) => {
      calls.push(puuid);
      return new Promise<PlayerDetails>((res) => (answer = res));
    };
    return { fetch, calls, answer: (d: PlayerDetails) => answer(d) };
  }

  it("open fetches the player's details and shows the answer", async () => {
    const slot = detailsSlot();
    const f = deferred();
    const done = slot.open("p1", f.fetch);
    expect(f.calls).toEqual(["p1"]);
    expect(now(slot)).toBeNull();
    f.answer(details("p1", "a"));
    await done;
    expect(now(slot)?.marker).toBe("a");
  });

  it("a details event for the player being opened wins over the older answer still in flight", async () => {
    const slot = detailsSlot();
    const f = deferred();
    const done = slot.open("p1", f.fetch);
    slot.event(details("p1", "event")); // Rust's sync finished before the invoke answer landed
    expect(now(slot)?.marker).toBe("event");
    f.answer(details("p1", "snapshot"));
    await done;
    expect(now(slot)?.marker).toBe("event");
  });

  it("Back from Match: the same player's details stay on screen, and an event during the refetch still wins", async () => {
    const slot = detailsSlot();
    await slot.open("p1", async () => details("p1", "first"));
    const f = deferred();
    const done = slot.open("p1", f.fetch);
    expect(now(slot)?.marker).toBe("first"); // no skeleton flash on Back
    slot.event(details("p1", "event"));
    f.answer(details("p1", "refetch"));
    await done;
    expect(now(slot)?.marker).toBe("event");
  });

  it("without an event, the refetch answer replaces the details in place", async () => {
    const slot = detailsSlot();
    await slot.open("p1", async () => details("p1", "first"));
    await slot.open("p1", async () => details("p1", "second"));
    expect(now(slot)?.marker).toBe("second");
  });

  it("an event for another player, or while nothing is open, changes nothing", async () => {
    const slot = detailsSlot();
    slot.event(details("p1", "early"));
    expect(now(slot)).toBeNull();
    await slot.open("p1", async () => details("p1", "a"));
    slot.event(details("p2", "other"));
    expect(now(slot)?.marker).toBe("a");
  });

  it("opening another player clears the old details at once, and the old answer never lands", async () => {
    const slot = detailsSlot();
    const f1 = deferred();
    const first = slot.open("p1", f1.fetch);
    await slot.open("p2", async () => details("p2", "b"));
    f1.answer(details("p1", "late"));
    await first;
    expect(now(slot)?.marker).toBe("b");
    const f3 = deferred();
    void slot.open("p3", f3.fetch);
    expect(now(slot)).toBeNull();
  });

  it("close forgets the details (a reopen starts from the skeleton) and drops an answer still in flight", async () => {
    const slot = detailsSlot();
    await slot.open("p1", async () => details("p1", "a"));
    slot.close();
    expect(now(slot)).toBeNull();
    const f = deferred();
    const done = slot.open("p1", f.fetch);
    expect(now(slot)).toBeNull(); // not the old payload: a now-hidden player's old name never flashes
    slot.close();
    f.answer(details("p1", "late"));
    await done;
    expect(now(slot)).toBeNull();
    slot.event(details("p1", "after close"));
    expect(now(slot)).toBeNull();
  });

  it("a failed call rejects and leaves the slot as it was", async () => {
    const slot = detailsSlot();
    await expect(slot.open("p1", async () => Promise.reject(new Error("offline")))).rejects.toThrow("offline");
    expect(now(slot)).toBeNull();
  });
});

describe("loadMeta (spec §5.8: a rejected meta() must not stay pending)", () => {
  const good: Meta = { agents: { a1: { name: "Jett" } }, maps: {}, tiers: {} };

  afterEach(() => vi.useRealTimers());

  /** A bridge whose `meta` rejects `failures` times, then resolves `good`. */
  function flaky(failures: number) {
    let calls = 0;
    const invoke = vi.fn(async (cmd: string) => {
      if (cmd !== "meta") throw new Error(`unexpected ${cmd}`);
      if (calls++ < failures) throw new Error("asset tables unavailable");
      return good;
    });
    return { b: { invoke } as unknown as Bridge, invoke };
  }

  it("success on the first try: sets the tables once, no retry", async () => {
    vi.useFakeTimers();
    const { b, invoke } = flaky(0);
    const set = vi.fn();
    await loadMeta(b, set);
    expect(set.mock.calls).toEqual([[good]]);
    await vi.advanceTimersByTimeAsync(META_RETRY_MAX_MS * 2);
    expect(invoke).toHaveBeenCalledTimes(1);
  });

  it("a rejection sets EMPTY_META at once, then retries with a doubling backoff until it succeeds", async () => {
    vi.useFakeTimers();
    const { b, invoke } = flaky(2);
    const set = vi.fn();
    const errors: number[] = [];
    const done = loadMeta(b, set, (_err, attempt) => errors.push(attempt));

    await vi.advanceTimersByTimeAsync(0);
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(set.mock.calls).toEqual([[EMPTY_META]]); // loaded-miss "—", not a skeleton that never resolves

    await vi.advanceTimersByTimeAsync(META_RETRY_FIRST_MS - 1);
    expect(invoke).toHaveBeenCalledTimes(1); // not before the first delay
    await vi.advanceTimersByTimeAsync(1);
    expect(invoke).toHaveBeenCalledTimes(2); // second try fails too
    expect(set).toHaveBeenCalledTimes(1); // EMPTY_META is set only once

    await vi.advanceTimersByTimeAsync(META_RETRY_FIRST_MS * 2 - 1);
    expect(invoke).toHaveBeenCalledTimes(2);
    await vi.advanceTimersByTimeAsync(1);
    await done;
    expect(invoke).toHaveBeenCalledTimes(3);
    expect(set.mock.calls).toEqual([[EMPTY_META], [good]]); // the real tables replace the loaded-miss
    expect(errors).toEqual([0, 1]);

    await vi.advanceTimersByTimeAsync(META_RETRY_MAX_MS * 2);
    expect(invoke).toHaveBeenCalledTimes(3); // and it stops
  });

  it("the backoff is capped", async () => {
    vi.useFakeTimers();
    const { b, invoke } = flaky(Number.POSITIVE_INFINITY);
    void loadMeta(b, vi.fn());
    // 2s, 4s, 8s, 16s, then 30s (the cap) every time: 5 tries by 30s, 6th at 60s, 7th at 90s.
    await vi.advanceTimersByTimeAsync(META_RETRY_FIRST_MS * (1 + 2 + 4 + 8));
    expect(invoke).toHaveBeenCalledTimes(5);
    await vi.advanceTimersByTimeAsync(META_RETRY_MAX_MS - 1);
    expect(invoke).toHaveBeenCalledTimes(5);
    await vi.advanceTimersByTimeAsync(1);
    expect(invoke).toHaveBeenCalledTimes(6);
    await vi.advanceTimersByTimeAsync(META_RETRY_MAX_MS);
    expect(invoke).toHaveBeenCalledTimes(7);
  });
});
