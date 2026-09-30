// The ⟲ encounter badge (v0.2.0 polish, R92): the design's own Segoe UI Symbol glyph in a span, no SVG. The app is
// Windows-only, so the system font is always there and the design wins over the earlier inline Lucide icon.
import { render } from "svelte/server";
import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";

let EncounterBadge: typeof import("./EncounterBadge.svelte").default;
beforeAll(async () => {
  vi.stubGlobal("location", { search: "" });
  vi.stubGlobal("document", { documentElement: { style: { setProperty: vi.fn() } }, getElementById: () => ({ setAttribute: vi.fn() }) });
  EncounterBadge = (await import("./EncounterBadge.svelte")).default;
});
afterAll(() => vi.unstubAllGlobals());

const html = (size: "card" | "hero", enc = { with: 2, against: 3 }) =>
  render(EncounterBadge, { props: { enc, size } }).body.replace(/<!--.*?-->/g, "");

describe("EncounterBadge", () => {
  it("draws the ⟲ character in a glyph span, followed by the total count, and no <svg>", () => {
    for (const size of ["card", "hero"] as const) {
      const h = html(size);
      expect(h, size).not.toContain("<svg");
      expect(h, size).toMatch(/<span class="glyph[^"]*"[^>]*>⟲<\/span>5</);
    }
  });

  it("the count is with + against", () => {
    expect(html("card", { with: 70, against: 53 })).toMatch(/⟲<\/span>123</);
  });
});
