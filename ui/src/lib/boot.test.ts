import { get } from "svelte/store";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  DETAILS_RETRY_MS,
  DETAILS_TIMEOUT_MS,
  META_RETRY_FIRST_MS,
  META_RETRY_MAX_MS,
  browserKeyBlocked,
  detailsSlot,
  installReleaseGuards,
  keyAction,
  loadMeta,
  matchSlot,
  retryOnce,
  retryWithDelays,
  runKeyAction,
  startLive,
  startMyStats,
  type Bridge,
} from "./boot";
import { EMPTY_META } from "./meta";
import type { LiveState, MatchDetailsView, Meta, MyStats, PlayerDetails } from "./types";

const live = (stage: LiveState["stage"]): LiveState => ({ stage, mapId: null, players: [], settled: true });

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

describe("startMyStats", () => {
  const stats = (marker: string) => ({ marker }) as unknown as MyStats;

  /** A bridge whose `my_stats` answer the test settles by hand (`resolve` / `reject`). */
  function fakeStats(opts: { listenFails?: boolean } = {}) {
    const calls: string[] = [];
    const handlers: Array<(e: { payload: MyStats }) => void> = [];
    let resolve!: (s: MyStats | null) => void;
    let reject!: (e: unknown) => void;
    const b: Bridge = {
      listen: (async (ev: string, cb: (e: { payload: MyStats }) => void) => {
        calls.push(`listen:${ev}`);
        if (opts.listenFails) throw new Error("no events");
        handlers.push(cb);
        return () => {};
      }) as Bridge["listen"],
      invoke: ((cmd: string) => {
        calls.push(`invoke:${cmd}`);
        return new Promise<MyStats | null>((res, rej) => ((resolve = res), (reject = rej)));
      }) as Bridge["invoke"],
    };
    return {
      b,
      calls,
      emit: (s: MyStats) => handlers.forEach((h) => h({ payload: s })),
      resolve: (s: MyStats | null) => resolve(s),
      reject: (e: unknown) => reject(e),
    };
  }

  it("listen('mystats') before invoke('my_stats'), then the answer is applied", async () => {
    const f = fakeStats();
    const applied: Array<MyStats | null> = [];
    const done = startMyStats(f.b, (s) => applied.push(s));
    await vi.waitFor(() => expect(f.calls).toContain("invoke:my_stats"));
    expect(f.calls).toEqual(["listen:mystats", "invoke:my_stats"]);
    f.resolve(stats("snapshot"));
    await done;
    expect(applied).toEqual([stats("snapshot")]);
  });

  it("a null answer (the user or the act is not known yet) is applied as null", async () => {
    const f = fakeStats();
    const applied: Array<MyStats | null> = [];
    const done = startMyStats(f.b, (s) => applied.push(s));
    await vi.waitFor(() => expect(f.calls).toContain("invoke:my_stats"));
    f.resolve(null);
    await done;
    expect(applied).toEqual([null]);
  });

  it("an event that arrives while the snapshot is in flight wins; later events keep applying", async () => {
    const f = fakeStats();
    const applied: string[] = [];
    const done = startMyStats(f.b, (s) => applied.push((s as unknown as { marker: string }).marker));
    await vi.waitFor(() => expect(f.calls).toContain("invoke:my_stats"));
    f.emit(stats("event"));
    f.resolve(stats("snapshot")); // older: must not overwrite
    await done;
    f.emit(stats("newer"));
    expect(applied).toEqual(["event", "newer"]);
  });

  it("a failed my_stats() settles on null (never an endless skeleton) and reports the error; an event still lands after it", async () => {
    const f = fakeStats();
    const applied: Array<MyStats | null> = [];
    const errors: unknown[] = [];
    const done = startMyStats(f.b, (s) => applied.push(s), (e) => errors.push(e));
    await vi.waitFor(() => expect(f.calls).toContain("invoke:my_stats"));
    f.reject(new Error("db busy"));
    await done; // never rejects
    expect(applied).toEqual([null]);
    expect(errors).toHaveLength(1);
    f.emit(stats("event"));
    expect(applied).toEqual([null, stats("event")]);
  });

  it("a failed my_stats() after an event keeps what the event brought", async () => {
    const f = fakeStats();
    const applied: Array<MyStats | null> = [];
    const done = startMyStats(f.b, (s) => applied.push(s), () => {});
    await vi.waitFor(() => expect(f.calls).toContain("invoke:my_stats"));
    f.emit(stats("event"));
    f.reject(new Error("db busy"));
    await done;
    expect(applied).toEqual([stats("event")]);
  });

  it("when listening fails the snapshot is still asked for (the page still gets its first answer)", async () => {
    const f = fakeStats({ listenFails: true });
    const applied: Array<MyStats | null> = [];
    const errors: unknown[] = [];
    const done = startMyStats(f.b, (s) => applied.push(s), (e) => errors.push(e));
    await vi.waitFor(() => expect(f.calls).toContain("invoke:my_stats"));
    f.resolve(stats("snapshot"));
    await done;
    expect(errors).toHaveLength(1);
    expect(applied).toEqual([stats("snapshot")]);
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

describe("browserKeyBlocked (release-only guards, v0.2.0 polish)", () => {
  const k = (key: string, mods: Partial<Record<"ctrlKey" | "shiftKey" | "altKey" | "metaKey", boolean>> = {}) => ({
    key,
    ctrlKey: false,
    shiftKey: false,
    altKey: false,
    metaKey: false,
    ...mods,
  });

  it("blocks reload, print, find, view-source and caret browsing: F5, Ctrl+R, Ctrl+Shift+R, Ctrl+P, Ctrl+F, Ctrl+U, F7", () => {
    expect(browserKeyBlocked(k("F5"))).toBe(true);
    expect(browserKeyBlocked(k("F5", { ctrlKey: true }))).toBe(true); // Ctrl+F5 is a hard reload
    expect(browserKeyBlocked(k("F5", { shiftKey: true }))).toBe(true);
    expect(browserKeyBlocked(k("r", { ctrlKey: true }))).toBe(true);
    expect(browserKeyBlocked(k("R", { ctrlKey: true, shiftKey: true }))).toBe(true);
    expect(browserKeyBlocked(k("p", { ctrlKey: true }))).toBe(true);
    expect(browserKeyBlocked(k("f", { ctrlKey: true }))).toBe(true);
    expect(browserKeyBlocked(k("u", { ctrlKey: true }))).toBe(true);
    expect(browserKeyBlocked(k("F7"))).toBe(true);
  });

  it("is case-insensitive (Caps Lock, Shift)", () => {
    expect(browserKeyBlocked(k("R", { ctrlKey: true }))).toBe(true);
    expect(browserKeyBlocked(k("P", { ctrlKey: true }))).toBe(true);
    expect(browserKeyBlocked(k("F", { ctrlKey: true }))).toBe(true);
    expect(browserKeyBlocked(k("U", { ctrlKey: true }))).toBe(true);
  });

  it("never blocks the app's own Ctrl+Shift+L (copy log) and Ctrl+Shift+D (dump raw)", () => {
    for (const key of ["L", "l", "D", "d"]) {
      const e = k(key, { ctrlKey: true, shiftKey: true });
      expect(keyAction(e), key).not.toBeNull();
      expect(browserKeyBlocked(e), key).toBe(false);
    }
  });

  it("leaves everything else alone: plain letters, other F keys, Tab/Enter/Space (keyboard use of the UI), AltGr chords", () => {
    for (const key of ["r", "p", "f", "u", "F", "Tab", "Enter", " ", "Escape", "F1", "F4", "F6", "F8", "F11", "F12", "ArrowDown"]) {
      expect(browserKeyBlocked(k(key)), key).toBe(false);
    }
    expect(browserKeyBlocked(k("r", { shiftKey: true }))).toBe(false);
    expect(browserKeyBlocked(k("s", { ctrlKey: true }))).toBe(false);
    expect(browserKeyBlocked(k("a", { ctrlKey: true }))).toBe(false);
    expect(browserKeyBlocked(k("c", { ctrlKey: true }))).toBe(false);
    // AltGr arrives as Ctrl+Alt on Windows: never swallow a layout's own character.
    expect(browserKeyBlocked(k("f", { ctrlKey: true, altKey: true }))).toBe(false);
    expect(browserKeyBlocked(k("r", { ctrlKey: true, metaKey: true }))).toBe(false);
  });
});

describe("installReleaseGuards", () => {
  function fakeTarget() {
    const handlers = new Map<string, (e: unknown) => void>();
    return {
      target: { addEventListener: (type: string, cb: (e: unknown) => void) => handlers.set(type, cb) } as unknown as Pick<Window, "addEventListener">,
      fire: (type: string, e: Record<string, unknown>) => {
        const preventDefault = vi.fn();
        handlers.get(type)?.({ ...e, preventDefault });
        return preventDefault;
      },
      types: () => [...handlers.keys()].sort(),
    };
  }
  const key = (k: string, mods: Record<string, boolean> = {}) => ({ key: k, ctrlKey: false, shiftKey: false, altKey: false, metaKey: false, ...mods });

  it("listens for contextmenu and keydown only", () => {
    const f = fakeTarget();
    installReleaseGuards(f.target);
    expect(f.types()).toEqual(["contextmenu", "keydown"]);
  });

  it("the context menu is always prevented", () => {
    const f = fakeTarget();
    installReleaseGuards(f.target);
    expect(f.fire("contextmenu", {})).toHaveBeenCalledOnce();
  });

  it("a blocked key is prevented, the copy-log and dump shortcuts and plain keys are not", () => {
    const f = fakeTarget();
    installReleaseGuards(f.target);
    expect(f.fire("keydown", key("F5"))).toHaveBeenCalledOnce();
    expect(f.fire("keydown", key("r", { ctrlKey: true }))).toHaveBeenCalledOnce();
    expect(f.fire("keydown", key("L", { ctrlKey: true, shiftKey: true }))).not.toHaveBeenCalled();
    expect(f.fire("keydown", key("D", { ctrlKey: true, shiftKey: true }))).not.toHaveBeenCalled();
    expect(f.fire("keydown", key("Enter"))).not.toHaveBeenCalled();
    expect(f.fire("keydown", key("Tab"))).not.toHaveBeenCalled();
  });
});

describe("retryOnce", () => {
  afterEach(() => vi.useRealTimers());

  it("a first success is returned with no retry and no delay", async () => {
    const call = vi.fn(async () => "ok");
    await expect(retryOnce(call, 1_000)).resolves.toBe("ok");
    expect(call).toHaveBeenCalledTimes(1);
  });

  it("a rejection is retried once after the delay and the retry's answer is returned", async () => {
    vi.useFakeTimers();
    let n = 0;
    const call = vi.fn(async () => {
      if (n++ === 0) throw new Error("busy");
      return "second";
    });
    const done = retryOnce(call, 1_000);
    await vi.advanceTimersByTimeAsync(999);
    expect(call).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(1);
    await expect(done).resolves.toBe("second");
    expect(call).toHaveBeenCalledTimes(2);
  });

  it("a second rejection propagates (the second error), with exactly two calls", async () => {
    vi.useFakeTimers();
    let n = 0;
    const call = vi.fn(async () => {
      throw new Error(`fail ${++n}`);
    });
    const done = retryOnce(call, 1_000);
    const seen = expect(done).rejects.toThrow("fail 2");
    await vi.advanceTimersByTimeAsync(1_000);
    await seen;
    await vi.advanceTimersByTimeAsync(10_000);
    expect(call).toHaveBeenCalledTimes(2);
  });

  it("no retry once nobody wants the answer any more: the first error propagates and the call is not repeated", async () => {
    vi.useFakeTimers();
    let wanted = true;
    const call = vi.fn(async () => {
      throw new Error("first");
    });
    const done = retryOnce(call, 1_000, () => wanted);
    const seen = expect(done).rejects.toThrow("first");
    wanted = false;
    await vi.advanceTimersByTimeAsync(1_000);
    await seen;
    expect(call).toHaveBeenCalledTimes(1);
  });
});

describe("retryWithDelays", () => {
  afterEach(() => vi.useRealTimers());

  it("asks again after each wait until an answer, and returns it", async () => {
    vi.useFakeTimers();
    let n = 0;
    const call = vi.fn(async () => {
      if (n++ < 2) throw new Error("not yet");
      return "found";
    });
    const done = retryWithDelays(call, [100, 200, 400]);
    await vi.advanceTimersByTimeAsync(100);
    expect(call).toHaveBeenCalledTimes(2);
    await vi.advanceTimersByTimeAsync(199);
    expect(call).toHaveBeenCalledTimes(2);
    await vi.advanceTimersByTimeAsync(1);
    await expect(done).resolves.toBe("found");
    expect(call).toHaveBeenCalledTimes(3);
  });

  it("gives up with the last error once the waits run out", async () => {
    vi.useFakeTimers();
    let n = 0;
    const call = vi.fn(async () => {
      throw new Error(`miss ${++n}`);
    });
    const done = retryWithDelays(call, [100, 100]);
    const seen = expect(done).rejects.toThrow("miss 3");
    await vi.advanceTimersByTimeAsync(200);
    await seen;
    await vi.advanceTimersByTimeAsync(10_000);
    expect(call).toHaveBeenCalledTimes(3);
  });

  it("stops when nobody wants the answer, and never repeats an invalid id", async () => {
    vi.useFakeTimers();
    let wanted = true;
    const miss = vi.fn(async () => {
      throw new Error("miss");
    });
    const left = retryWithDelays(miss, [100, 100], () => wanted);
    const leftSeen = expect(left).rejects.toThrow("miss");
    wanted = false;
    await vi.advanceTimersByTimeAsync(100);
    await leftSeen;
    expect(miss).toHaveBeenCalledTimes(1);

    const invalid = vi.fn(async () => {
      throw "invalid match id";
    });
    await expect(retryWithDelays(invalid, [100])).rejects.toBe("invalid match id");
    expect(invalid).toHaveBeenCalledTimes(1);
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

  describe("retry and settle (v0.2.0 polish: a rejected player_details retries once after 1s, then settles on the empty state)", () => {
    afterEach(() => vi.useRealTimers());
    const settle = (puuid: string) => details(puuid, "settled");

    it("the retry delay is 1s", () => {
      expect(DETAILS_RETRY_MS).toBe(1_000);
    });

    it("a rejection is retried once after the delay, and the retry's answer shows", async () => {
      vi.useFakeTimers();
      const slot = detailsSlot();
      let n = 0;
      const fetch = vi.fn(async (p: string) => {
        if (n++ === 0) throw new Error("db busy");
        return details(p, "retried");
      });
      const done = slot.open("p1", fetch, { retryMs: DETAILS_RETRY_MS, settle });
      await vi.advanceTimersByTimeAsync(DETAILS_RETRY_MS - 1);
      expect(fetch).toHaveBeenCalledTimes(1);
      expect(now(slot)).toBeNull(); // still the skeleton while waiting
      await vi.advanceTimersByTimeAsync(1);
      await done;
      expect(fetch).toHaveBeenCalledTimes(2);
      expect(now(slot)?.marker).toBe("retried");
    });

    it("two rejections settle the slot on the empty state (the skeleton ends) and still reject for the log", async () => {
      vi.useFakeTimers();
      const slot = detailsSlot();
      const fetch = vi.fn(async () => {
        throw new Error("unknown player");
      });
      const done = slot.open("p1", fetch, { retryMs: DETAILS_RETRY_MS, settle });
      const seen = expect(done).rejects.toThrow("unknown player");
      await vi.advanceTimersByTimeAsync(DETAILS_RETRY_MS);
      await seen;
      expect(fetch).toHaveBeenCalledTimes(2); // once, not forever
      expect(now(slot)?.marker).toBe("settled");
      expect(now(slot)?.card.puuid).toBe("p1");
    });

    it("a later details event replaces the settled state in place", async () => {
      vi.useFakeTimers();
      const slot = detailsSlot();
      const done = slot.open("p1", () => Promise.reject(new Error("x")), { retryMs: 10, settle });
      const seen = expect(done).rejects.toThrow();
      await vi.advanceTimersByTimeAsync(10);
      await seen;
      slot.event(details("p1", "real"));
      expect(now(slot)?.marker).toBe("real");
    });

    it("a details event during the wait supersedes the retry: no second call, nothing settled over it", async () => {
      vi.useFakeTimers();
      const slot = detailsSlot();
      const fetch = vi.fn(async () => {
        throw new Error("x");
      });
      const done = slot.open("p1", fetch, { retryMs: DETAILS_RETRY_MS, settle });
      const seen = expect(done).rejects.toThrow();
      await vi.advanceTimersByTimeAsync(10);
      slot.event(details("p1", "event"));
      await vi.advanceTimersByTimeAsync(DETAILS_RETRY_MS);
      await seen;
      expect(fetch).toHaveBeenCalledTimes(1);
      expect(now(slot)?.marker).toBe("event");
    });

    it("leaving (close) or opening another player during the wait drops the retry and settles nothing", async () => {
      vi.useFakeTimers();
      const slot = detailsSlot();
      const fetch = vi.fn(async () => {
        throw new Error("x");
      });
      const a = slot.open("p1", fetch, { retryMs: DETAILS_RETRY_MS, settle });
      const seenA = expect(a).rejects.toThrow();
      slot.close();
      await vi.advanceTimersByTimeAsync(DETAILS_RETRY_MS);
      await seenA;
      expect(fetch).toHaveBeenCalledTimes(1);
      expect(now(slot)).toBeNull();

      const b = slot.open("p2", fetch, { retryMs: DETAILS_RETRY_MS, settle });
      const seenB = expect(b).rejects.toThrow();
      const c = slot.open("p3", async (p) => details(p, "c"));
      await c;
      await vi.advanceTimersByTimeAsync(DETAILS_RETRY_MS);
      await seenB;
      expect(now(slot)?.marker).toBe("c");
    });

    it("the same player's payload already on screen (Back from Match) is kept when the refetch fails twice", async () => {
      vi.useFakeTimers();
      const slot = detailsSlot();
      await slot.open("p1", async () => details("p1", "first"));
      const done = slot.open("p1", () => Promise.reject(new Error("x")), { retryMs: 10, settle });
      const seen = expect(done).rejects.toThrow();
      await vi.advanceTimersByTimeAsync(10);
      await seen;
      expect(now(slot)?.marker).toBe("first");
    });
  });
});

describe("matchSlot (R150: the open Match Details, refreshed by the `match` event)", () => {
  const view = (matchId: string, marker: string) => ({ matchId, marker }) as unknown as MatchDetailsView;
  const now = (slot: ReturnType<typeof matchSlot>) => get(slot) as (MatchDetailsView & { marker: string }) | null;
  /** A `match_details` call held open until the test answers it. */
  function deferred() {
    let answer!: (v: MatchDetailsView) => void;
    const calls: string[] = [];
    const fetch = (matchId: string) => {
      calls.push(matchId);
      return new Promise<MatchDetailsView>((res) => (answer = res));
    };
    return { fetch, calls, answer: (v: MatchDetailsView) => answer(v) };
  }
  const settle = (matchId: string) => view(matchId, "settled");

  it("open fetches the match and shows the answer", async () => {
    const slot = matchSlot();
    const f = deferred();
    const done = slot.open("m1", f.fetch);
    expect(f.calls).toEqual(["m1"]);
    expect(now(slot)).toBeNull();
    f.answer(view("m1", "agent names"));
    await done;
    expect(now(slot)?.marker).toBe("agent names");
  });

  it("a match event for the open match replaces the view in place (the names arrived)", async () => {
    const slot = matchSlot();
    await slot.open("m1", async () => view("m1", "agent names"));
    slot.event(view("m1", "real names"));
    expect(now(slot)?.marker).toBe("real names");
  });

  it("an event for another match changes nothing", async () => {
    const slot = matchSlot();
    await slot.open("m1", async () => view("m1", "agent names"));
    slot.event(view("m2", "other match"));
    expect(now(slot)?.marker).toBe("agent names");
  });

  it("an event while nothing is open changes nothing, before the first open and after a close", async () => {
    const slot = matchSlot();
    slot.event(view("m1", "early"));
    expect(now(slot)).toBeNull();
    await slot.open("m1", async () => view("m1", "a"));
    slot.close();
    expect(now(slot)).toBeNull();
    slot.event(view("m1", "after leaving"));
    expect(now(slot)).toBeNull();
  });

  it("an event that lands before the older answer still in flight wins over that answer", async () => {
    const slot = matchSlot();
    const f = deferred();
    const done = slot.open("m1", f.fetch);
    slot.event(view("m1", "event"));
    expect(now(slot)?.marker).toBe("event");
    f.answer(view("m1", "snapshot"));
    await done;
    expect(now(slot)?.marker).toBe("event");
  });

  it("two events in a row: the later one stays, an older answer never comes back over it", async () => {
    const slot = matchSlot();
    const f = deferred();
    const done = slot.open("m1", f.fetch);
    slot.event(view("m1", "first"));
    slot.event(view("m1", "second"));
    f.answer(view("m1", "snapshot"));
    await done;
    expect(now(slot)?.marker).toBe("second");
  });

  it("leaving drops an answer still in flight, and a reopen starts from the skeleton", async () => {
    const slot = matchSlot();
    const f = deferred();
    const done = slot.open("m1", f.fetch);
    slot.close();
    f.answer(view("m1", "late"));
    await done;
    expect(now(slot)).toBeNull();
    await slot.open("m1", async () => view("m1", "b"));
    slot.close();
    void slot.open("m1", deferred().fetch);
    expect(now(slot)).toBeNull(); // never the old payload while the new answer is on its way
  });

  it("opening another match clears the old one at once (the skeleton), before the new answer comes", async () => {
    const slot = matchSlot();
    await slot.open("m1", async () => view("m1", "a")); // m1 is on screen, completely loaded
    expect(now(slot)?.marker).toBe("a");
    const f2 = deferred();
    const second = slot.open("m2", f2.fetch); // m2's answer is still pending
    expect(now(slot)).toBeNull();
    slot.event(view("m1", "old match")); // an event for the match that was left changes nothing
    expect(now(slot)).toBeNull();
    f2.answer(view("m2", "b"));
    await second;
    expect(now(slot)?.marker).toBe("b");
  });

  it("an answer still in flight for the match that was left never lands over the new one", async () => {
    const slot = matchSlot();
    const f1 = deferred();
    const first = slot.open("m1", f1.fetch);
    await slot.open("m2", async () => view("m2", "b"));
    f1.answer(view("m1", "late"));
    await first;
    expect(now(slot)?.marker).toBe("b");
    slot.event(view("m1", "old match"));
    expect(now(slot)?.marker).toBe("b");
  });

  it("a rejection is retried once; two rejections settle on the empty view, and a later event replaces it", async () => {
    vi.useFakeTimers();
    try {
      const slot = matchSlot();
      const fetch = vi.fn(async () => {
        throw new Error("unknown match");
      });
      const done = slot.open("m1", fetch, { retryMs: DETAILS_RETRY_MS, settle });
      const seen = expect(done).rejects.toThrow("unknown match");
      await vi.advanceTimersByTimeAsync(DETAILS_RETRY_MS);
      await seen;
      expect(fetch).toHaveBeenCalledTimes(2);
      expect(now(slot)?.marker).toBe("settled");
      slot.event(view("m1", "real"));
      expect(now(slot)?.marker).toBe("real");
    } finally {
      vi.useRealTimers();
    }
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

describe("R163: a page that never answers, a deterministic rejection, a settled page (M1, M5)", () => {
  afterEach(() => vi.useRealTimers());
  const details = (puuid: string, marker: string) => ({ card: { puuid }, marker }) as unknown as PlayerDetails;
  const marker = (slot: ReturnType<typeof detailsSlot>) => (get(slot) as { marker: string } | null)?.marker;
  const settle = (puuid: string) => details(puuid, "settled");

  it("the timeout is 15s", () => {
    expect(DETAILS_TIMEOUT_MS).toBe(15_000);
  });

  it("a call that never answers times out after 15s and feeds the retry, whose answer shows", async () => {
    vi.useFakeTimers();
    const slot = detailsSlot();
    let n = 0;
    const fetch = vi.fn((p: string) => (n++ === 0 ? new Promise<PlayerDetails>(() => {}) : Promise.resolve(details(p, "retried"))));
    const done = slot.open("p1", fetch, { retryMs: DETAILS_RETRY_MS, timeoutMs: DETAILS_TIMEOUT_MS, settle });
    await vi.advanceTimersByTimeAsync(DETAILS_TIMEOUT_MS - 1);
    expect(fetch).toHaveBeenCalledTimes(1);
    expect(marker(slot)).toBeUndefined(); // still the skeleton
    await vi.advanceTimersByTimeAsync(1 + DETAILS_RETRY_MS);
    await done;
    expect(fetch).toHaveBeenCalledTimes(2);
    expect(marker(slot)).toBe("retried");
  });

  it("two calls that never answer settle the page after 2 x 15s + 1s, and reject for the log", async () => {
    vi.useFakeTimers();
    const slot = detailsSlot();
    const fetch = vi.fn(() => new Promise<PlayerDetails>(() => {}));
    const done = slot.open("p1", fetch, { retryMs: DETAILS_RETRY_MS, timeoutMs: DETAILS_TIMEOUT_MS, settle });
    const seen = expect(done).rejects.toThrow(/timed out/);
    await vi.advanceTimersByTimeAsync(DETAILS_TIMEOUT_MS * 2 + DETAILS_RETRY_MS);
    await seen;
    expect(fetch).toHaveBeenCalledTimes(2);
    expect(marker(slot)).toBe("settled");
  });

  it("an answer inside the timeout leaves no timer behind", async () => {
    vi.useFakeTimers();
    const slot = detailsSlot();
    await slot.open("p1", async (p) => details(p, "a"), { retryMs: DETAILS_RETRY_MS, timeoutMs: DETAILS_TIMEOUT_MS, settle });
    expect(vi.getTimerCount()).toBe(0);
  });

  it("a rejection that says `invalid …` is deterministic: no retry, the slot settles at once", async () => {
    vi.useFakeTimers();
    const slot = detailsSlot();
    const fetch = vi.fn(async () => {
      throw "invalid player id"; // Tauri rejects with the bare string
    });
    const done = slot.open("p1", fetch, { retryMs: DETAILS_RETRY_MS, settle });
    await expect(done).rejects.toBe("invalid player id");
    expect(fetch).toHaveBeenCalledTimes(1);
    expect(marker(slot)).toBe("settled");
    expect(vi.getTimerCount()).toBe(0);
  });

  it("retryOnce: an Error whose message starts with `invalid ` is not retried either; other errors still are", async () => {
    vi.useFakeTimers();
    const bad = vi.fn(async () => {
      throw new Error("invalid match id");
    });
    await expect(retryOnce(bad, 1_000)).rejects.toThrow("invalid match id");
    expect(bad).toHaveBeenCalledTimes(1);
    const other = vi.fn(async () => {
      throw new Error("unknown match");
    });
    const p = expect(retryOnce(other, 1_000)).rejects.toThrow("unknown match");
    await vi.advanceTimersByTimeAsync(1_000);
    await p;
    expect(other).toHaveBeenCalledTimes(2);
  });

  describe("retrySettled (the next `live` event retries a settled page once)", () => {
    async function settled(fetch: (p: string) => Promise<PlayerDetails>) {
      const slot = detailsSlot();
      await slot.open("p1", fetch, { settle }).catch(() => {});
      expect(marker(slot)).toBe("settled");
      return slot;
    }

    it("a settled page asks again once; the answer replaces the empty state", async () => {
      let n = 0;
      const fetch = vi.fn(async (p: string) => {
        if (n++ === 0) throw new Error("db busy");
        return details(p, "late answer");
      });
      const slot = await settled(fetch);
      slot.retrySettled();
      await vi.waitFor(() => expect(marker(slot)).toBe("late answer"));
      expect(fetch).toHaveBeenCalledTimes(2);
      slot.retrySettled(); // nothing is settled any more
      expect(fetch).toHaveBeenCalledTimes(2);
    });

    it("when that fails too, the page stays settled and no later live event asks again", async () => {
      const fetch = vi.fn(async () => {
        throw new Error("still down");
      });
      const slot = await settled(fetch);
      slot.retrySettled();
      slot.retrySettled(); // one retry, even while it is in flight
      await vi.waitFor(() => expect(fetch).toHaveBeenCalledTimes(2));
      await Promise.resolve();
      slot.retrySettled();
      slot.retrySettled();
      expect(fetch).toHaveBeenCalledTimes(2);
      expect(marker(slot)).toBe("settled");
    });

    it("a page that was left, replaced or never settled does not retry", async () => {
      const fetch = vi.fn(async (p: string) => {
        if (fetch.mock.calls.length === 1) throw new Error("db busy");
        return details(p, "ok");
      });
      const slot = await settled(fetch);
      slot.close();
      slot.retrySettled();
      expect(fetch).toHaveBeenCalledTimes(1);

      const fresh = detailsSlot();
      const ok = vi.fn(async (p: string) => details(p, "ok"));
      await fresh.open("p2", ok);
      fresh.retrySettled();
      expect(ok).toHaveBeenCalledTimes(1);
    });

    it("a new open of a failing page gets its own one retry", async () => {
      const fetch = vi.fn(async () => {
        throw new Error("down");
      });
      const slot = await settled(fetch);
      slot.retrySettled();
      await vi.waitFor(() => expect(fetch).toHaveBeenCalledTimes(2));
      slot.close();
      await slot.open("p1", fetch, { settle }).catch(() => {});
      expect(fetch).toHaveBeenCalledTimes(3);
      slot.retrySettled();
      await vi.waitFor(() => expect(fetch).toHaveBeenCalledTimes(4));
    });
  });

  it("a timeout nobody waits for any more (the page was left) resolves quietly: nothing for the console", async () => {
    vi.useFakeTimers();
    const slot = detailsSlot();
    const fetch = vi.fn(() => new Promise<PlayerDetails>(() => {}));
    const done = slot.open("p1", fetch, { retryMs: DETAILS_RETRY_MS, timeoutMs: DETAILS_TIMEOUT_MS, settle });
    await vi.advanceTimersByTimeAsync(100);
    slot.close(); // Back, during the 15s
    await vi.advanceTimersByTimeAsync(DETAILS_TIMEOUT_MS + DETAILS_RETRY_MS);
    await expect(done).resolves.toBeUndefined();
    expect(fetch).toHaveBeenCalledTimes(1);
    expect(marker(slot)).toBeUndefined();
  });

  describe("loadMeta stops when told to (the `meta` event delivered the tables)", () => {
    it("no further invoke after `stopped()` turns true", async () => {
      vi.useFakeTimers();
      const invoke = vi.fn(async () => {
        throw new Error("asset tables unavailable");
      });
      let stop = false;
      const done = loadMeta({ invoke } as unknown as Bridge, vi.fn(), () => {}, () => stop);
      await vi.advanceTimersByTimeAsync(0);
      expect(invoke).toHaveBeenCalledTimes(1);
      stop = true;
      await vi.advanceTimersByTimeAsync(META_RETRY_FIRST_MS);
      await done;
      expect(invoke).toHaveBeenCalledTimes(1);
    });

    it("an answer that was in flight when the event arrived does not overwrite it", async () => {
      let answer!: (m: Meta) => void;
      const invoke = vi.fn(() => new Promise<Meta>((res) => (answer = res)));
      let stop = false;
      const set = vi.fn();
      const done = loadMeta({ invoke } as unknown as Bridge, set, () => {}, () => stop);
      stop = true; // the `meta` event came first
      answer({ agents: {}, maps: {}, tiers: {} });
      await done;
      expect(set).not.toHaveBeenCalled();
    });
  });
});
