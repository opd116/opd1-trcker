// `closeDetails` (R145's `close_details` command): the Rust side forgets the open Player Details once the page is left.
// Under Tauri it is one `invoke('close_details')` with no arguments; in the DEV browser there is no Rust, so the demo
// module only counts it (`data-details-closed` on <html>, for the Playwright checks).
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
