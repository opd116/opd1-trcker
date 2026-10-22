import { afterEach, describe, expect, it, vi } from "vitest";
import { themeVars } from "./theme";

afterEach(() => vi.unstubAllGlobals());

describe("themeStore (load time)", () => {
  it("applies the whole theme synchronously, exactly once, before anything can paint", async () => {
    vi.resetModules();
    const setProperty = vi.fn();
    vi.stubGlobal("document", { documentElement: { style: { setProperty } } });
    await import("./themeStore.svelte");
    const vars = themeVars();
    expect(setProperty).toHaveBeenCalledTimes(Object.keys(vars).length);
    for (const [k, v] of Object.entries(vars)) expect(setProperty).toHaveBeenCalledWith(k, v);
  });
});
