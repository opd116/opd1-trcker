import { afterEach, describe, expect, it, vi } from "vitest";
import { DEFAULT_THEME, themeVars } from "./theme";

afterEach(() => vi.unstubAllGlobals());

// Vitest compiles `.svelte.ts` for the server here (no DOM, `$effect` is a no-op), so only the synchronous, pre-paint
// half of themeStore is testable in this suite. The effect's skipped first run was checked under a client-mode
// config (see the task 16a report, "Minor batch" item 9).
describe("themeStore (load time)", () => {
  function stubDom() {
    const setProperty = vi.fn();
    const setAttribute = vi.fn();
    vi.stubGlobal("document", {
      documentElement: { style: { setProperty } },
      getElementById: () => ({ setAttribute }),
    });
    return { setProperty, setAttribute };
  }

  it("applies the whole theme synchronously, exactly once, before anything can paint", async () => {
    vi.resetModules();
    const dom = stubDom();
    vi.stubGlobal("location", { search: "" });
    const { theme } = await import("./themeStore.svelte");
    const vars = themeVars(DEFAULT_THEME);
    expect(dom.setProperty).toHaveBeenCalledTimes(Object.keys(vars).length);
    for (const [k, v] of Object.entries(vars)) expect(dom.setProperty).toHaveBeenCalledWith(k, v);
    expect(dom.setAttribute).toHaveBeenCalledTimes(1);
    expect(dom.setAttribute).toHaveBeenCalledWith("data-last5", "arrow");
    expect({ ...theme }).toEqual(DEFAULT_THEME);
  });

  it("in DEV the URL params override the defaults before the first apply", async () => {
    vi.resetModules();
    const dom = stubDom();
    vi.stubGlobal("location", { search: "?tone=Cool&accent=%234fc3ae&last5=Underline" });
    const { theme } = await import("./themeStore.svelte");
    expect(theme).toMatchObject({ tone: "Cool", accent: "#4fc3ae", last5Style: "Underline" });
    expect(dom.setProperty).toHaveBeenCalledWith("--accent", "#4fc3ae");
    expect(dom.setAttribute).toHaveBeenCalledWith("data-last5", "underline");
  });
});
