// `closeDetails` (R145's `close_details` command): the Rust side forgets the open Player Details once the page is left.
// Under Tauri it is one `invoke('close_details')` with no arguments; in the DEV browser there is no Rust, so the demo
// module only counts it (`data-details-closed` on <html>, for the Playwright checks). `closeMatch` (R150's
// `close_match`) is the same for the open Match Details (`data-match-closed`).
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn(async (_cmd: string, _args?: unknown) => undefined);
let tauri = true;

beforeEach(() => {
  vi.resetModules();
  invoke.mockClear();
  tauri = true;
  vi.doMock("@tauri-apps/api/core", () => ({ invoke, isTauri: () => tauri }));
  vi.doMock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
});
afterEach(() => {
  vi.doUnmock("@tauri-apps/api/core");
  vi.doUnmock("@tauri-apps/api/event");
  vi.unstubAllGlobals();
});

describe("closeDetails", () => {
  it("under Tauri: invoke('close_details') with no arguments", async () => {
    const { closeDetails } = await import("./api");
    await closeDetails();
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenCalledWith("close_details");
  });

  it("R163: with a puuid the close is keyed, so Rust ignores a late close of a page already replaced", async () => {
    const { closeDetails } = await import("./api");
    await closeDetails("p1");
    expect(invoke).toHaveBeenCalledWith("close_details", { puuid: "p1" });
  });

  it("a rejected close_details rejects (the caller logs it)", async () => {
    invoke.mockRejectedValueOnce(new Error("no state"));
    const { closeDetails } = await import("./api");
    await expect(closeDetails()).rejects.toThrow("no state");
  });

  it("in the DEV browser (no Tauri): no invoke, the demo module counts the close", async () => {
    tauri = false;
    const dataset: Record<string, string> = {};
    vi.stubGlobal("document", { documentElement: { dataset } });
    const { closeDetails } = await import("./api");
    await closeDetails();
    await closeDetails();
    expect(invoke).not.toHaveBeenCalled();
    expect(dataset.detailsClosed).toBe("2");
  });
});

describe("closeMatch", () => {
  it("under Tauri: invoke('close_match') with no arguments", async () => {
    const { closeMatch } = await import("./api");
    await closeMatch();
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenCalledWith("close_match");
  });

  it("R163: with a match id the close is keyed (Rust ignores a close for another match)", async () => {
    const { closeMatch } = await import("./api");
    await closeMatch("m1");
    expect(invoke).toHaveBeenCalledWith("close_match", { matchId: "m1" });
  });

  it("a rejected close_match rejects (the caller logs it)", async () => {
    invoke.mockRejectedValueOnce(new Error("no state"));
    const { closeMatch } = await import("./api");
    await expect(closeMatch()).rejects.toThrow("no state");
  });

  it("in the DEV browser (no Tauri): no invoke, the demo module counts the close", async () => {
    tauri = false;
    const dataset: Record<string, string> = {};
    vi.stubGlobal("document", { documentElement: { dataset } });
    const { closeMatch } = await import("./api");
    await closeMatch();
    await closeMatch();
    expect(invoke).not.toHaveBeenCalled();
    expect(dataset.matchClosed).toBe("2");
    expect(dataset.detailsClosed).toBeUndefined();
  });

  it("the command goes out in the order it is called: the old page's close_match, then the new page's match_details (R150)", async () => {
    const { closeMatch, getMatchDetails } = await import("./api");
    invoke.mockImplementation(async (cmd: string) => (cmd === "match_details" ? ({ matchId: "b" } as never) : undefined));
    const closing = closeMatch();
    const opening = getMatchDetails("b", "p1");
    await Promise.all([closing, opening]);
    expect(invoke.mock.calls.map((c) => c[0])).toEqual(["close_match", "match_details"]);
    invoke.mockImplementation(async () => undefined);
  });
});

describe("myStats", () => {
  it("under Tauri: invoke('my_stats') with no arguments; its answer (data or null) comes through as is", async () => {
    invoke.mockResolvedValueOnce({ marker: "stats" } as never);
    const { myStats } = await import("./api");
    expect(await myStats()).toEqual({ marker: "stats" });
    expect(invoke).toHaveBeenCalledWith("my_stats");
    invoke.mockResolvedValueOnce(null as never);
    expect(await myStats()).toBeNull();
  });

  it("a rejected my_stats rejects (the caller logs it)", async () => {
    invoke.mockRejectedValueOnce(new Error("db busy"));
    const { myStats } = await import("./api");
    await expect(myStats()).rejects.toThrow("db busy");
  });

  it("in the DEV browser (no Tauri): no invoke, the scene named in the URL answers", async () => {
    tauri = false;
    vi.stubGlobal("location", { search: "?scene=mystats-new" });
    const { myStats } = await import("./api");
    expect(await myStats()).toBeNull();
    expect(invoke).not.toHaveBeenCalled();
    vi.stubGlobal("location", { search: "?scene=mystats" });
    expect((await myStats())?.rrSeries).toHaveLength(60);
  });
});
