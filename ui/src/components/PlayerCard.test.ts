// Card markup facts that spec §3 states and that pixels alone do not prove (task 17 review, F3/F5/F11/F12).
import { render } from "svelte/server";
import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";
import { SCENES } from "../lib/demo/scenes";
import type { PlayerCard as Card } from "../lib/types";

// The card pulls in the theme store, which reads `location` and paints CSS variables when it loads: give it a
// DOM stub before the component is imported (the server-side render below needs nothing else).
let PlayerCard: typeof import("./PlayerCard.svelte").default;
beforeAll(async () => {
  vi.stubGlobal("location", { search: "" });
  vi.stubGlobal("document", { documentElement: { style: { setProperty: vi.fn() } }, getElementById: () => ({ setAttribute: vi.fn() }) });
  PlayerCard = (await import("./PlayerCard.svelte")).default;
});
afterAll(() => vi.unstubAllGlobals());

const roster = SCENES.ingame.live!.players;
const byName = (n: string): Card => roster.find((c) => c.name?.kind === "visible" && c.name.name === n)!;
const html = (card: Card) => render(PlayerCard, { props: { card } }).body.replace(/<!--.*?-->/g, "");
/** Offset of the first element whose class list starts with `cls`. */
const at = (h: string, cls: string) => h.search(new RegExp(`class="${cls}[ "]`));

describe("PlayerCard DOM order", () => {
  it("the party stripe and tab come before the grid, as in the design (v8:44-49); z-index paints them above", () => {
    const h = html(byName("Mara")); // a likely party member
    for (const part of ["stripe", "ptab"]) {
      expect(at(h, part), part).toBeGreaterThan(-1);
      expect(at(h, part), `${part} before .grid`).toBeLessThan(at(h, "grid"));
    }
    // The self outline is last: it paints over everything and takes no pointer events.
    const self = html({ ...byName("Fahad") });
    expect(at(self, "self")).toBeGreaterThan(at(self, "grid"));
  });

  it("a card without a party has neither part", () => {
    const h = html({ ...byName("Mara"), party: null });
    expect(at(h, "stripe")).toBe(-1);
    expect(at(h, "ptab")).toBe(-1);
  });
});

describe("PlayerCard level line", () => {
  const lv = (card: Card) => /<div class="lv[^"]*">(.*?)<\/div>/.exec(html(card))![1]!;

  it("absent (still loading): a bar in the Lv line, no text", () => {
    const { level: _, ...noLevel } = byName("Mara");
    const l = lv(noLevel as Card);
    expect(l).toContain('class="sk');
    expect(l).toContain("width: 48px");
    expect(l).toContain("height: 10px");
    expect(l).not.toMatch(/Lv|—/);
  });

  it("withheld (null): Lv —", () => {
    expect(lv({ ...byName("Mara"), level: null }).replace(/<[^>]+>/g, "").trim()).toBe("Lv —");
  });

  it("known: Lv and the number", () => {
    expect(lv({ ...byName("Mara"), level: 221 }).replace(/<[^>]+>/g, "").trim()).toBe("Lv 221");
  });
});
