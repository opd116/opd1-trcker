import { describe, expect, it, vi } from "vitest";
import { applyDetailsEvent, keyAction, runKeyAction, startLive, type Bridge } from "./boot";
import type { LiveState, PlayerDetails } from "./types";

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

describe("applyDetailsEvent", () => {
  const details = (puuid: string, marker: string) => ({ card: { puuid }, marker }) as unknown as PlayerDetails;

  it("applyDetailsEvent replaces only the matching puuid", () => {
    const open = details("p1", "old");
    const same = details("p1", "new");
    expect(applyDetailsEvent(open, same)).toBe(same);
    const other = details("p2", "other");
    expect(applyDetailsEvent(open, other)).toBe(open);
  });

  it("ignores events while nothing is open", () => {
    expect(applyDetailsEvent(null, details("p1", "x"))).toBeNull();
  });
});
