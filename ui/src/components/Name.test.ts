// Name markup facts (v0.2.0 polish): the player-chosen name spans are `dir="auto"`, so an Arabic or Hebrew Riot ID
// runs right to left inside its own span (and its ellipsis lands on the correct end) without touching the layout.
import { render } from "svelte/server";
import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";
import type { DisplayName } from "../lib/types";

let Name: typeof import("./Name.svelte").default;
beforeAll(async () => {
  vi.stubGlobal("location", { search: "" });
  vi.stubGlobal("document", { documentElement: { style: { setProperty: vi.fn() } }, getElementById: () => ({ setAttribute: vi.fn() }) });
  Name = (await import("./Name.svelte")).default;
});
afterAll(() => vi.unstubAllGlobals());

const html = (name: DisplayName | null | undefined, size: "card" | "hero" | "row" = "card", wrap = false) =>
  render(Name, { props: { name, size, wrap } }).body.replace(/<!--.*?-->/g, "");
/** The opening tag of every element whose class list has `cls`. */
const tags = (h: string, cls: string) => [...h.matchAll(new RegExp(`<span[^>]*class="[^"]*\\b${cls}\\b[^"]*"[^>]*>`, "g"))].map((m) => m[0]);

describe("Name dir=auto", () => {
  it("a visible name span has dir=auto, at every size", () => {
    for (const size of ["card", "hero", "row"] as const) {
      const t = tags(html({ kind: "visible", name: "مرحبا", tag: "1177" }, size), "n");
      expect(t, size).toHaveLength(1);
      expect(t[0], size).toContain('dir="auto"');
    }
  });

  it("a remembered name span has dir=auto (card and wrapping Top 3)", () => {
    const n: DisplayName = { kind: "remembered", name: "שלום", tag: "OPD1", source: "ownMatch", fromMapId: null, fromStartMs: null };
    for (const wrap of [false, true]) {
      const t = tags(html(n, "card", wrap), "n");
      expect(t, `wrap ${wrap}`).toHaveLength(1);
      expect(t[0], `wrap ${wrap}`).toContain('dir="auto"');
    }
  });

  it("the tag is not a name: it stays LTR and gets no dir", () => {
    const t = tags(html({ kind: "visible", name: "مرحبا", tag: "1177" }), "t");
    expect(t).toHaveLength(1);
    expect(t[0]).not.toContain("dir=");
  });
});
