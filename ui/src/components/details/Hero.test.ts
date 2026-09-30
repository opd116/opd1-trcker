// R154: the Details hero with an unknown RR (rank.rr null): the tier icon and name as usual, — where the number goes.
import { render } from "svelte/server";
import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";
import { detailsFor } from "../../lib/demo/cdv8";
import demoMeta from "../../lib/demo/meta.json";
import type { Meta, PlayerDetails, Rank } from "../../lib/types";

// The hero pulls in the theme store, which reads `location` and paints CSS variables when it loads (see PlayerCard.test.ts).
let Hero: typeof import("./Hero.svelte").default;
let setMeta: (m: Meta | null) => void;
beforeAll(async () => {
  vi.stubGlobal("location", { search: "" });
  vi.stubGlobal("document", { documentElement: { style: { setProperty: vi.fn() } }, getElementById: () => ({ setAttribute: vi.fn() }) });
  Hero = (await import("./Hero.svelte")).default;
  const { meta } = await import("../../lib/stores");
  setMeta = (m) => meta.set(m);
  setMeta(demoMeta as Meta);
});
afterAll(() => {
  setMeta(null);
  vi.unstubAllGlobals();
});

const withRank = (rank: Rank): PlayerDetails => {
  const d = detailsFor("mara");
  d.card.rank = rank;
  return d;
};
const html = (rank: Rank) => render(Hero, { props: { details: withRank(rank) } }).body.replace(/ svelte-[a-z0-9]+/g, "").replace(/<!--.*?-->/g, "");
const rrValue = (h: string) => /<span class="rr"><span class="v">(.*?)<\/span>/.exec(h)![1];

describe("Hero current rank with an unknown RR (R154)", () => {
  it("the tier icon and name as usual, and — where the number goes", () => {
    expect(rrValue(html({ tier: 18, rr: 40 }))).toBe("40");
    const h = html({ tier: 18, rr: null });
    expect(rrValue(h)).toBe("—");
    expect(h).toContain('class="ri"');
    expect(h).toContain("Diamond");
  });

  it("nothing else changes: the same markup with only the number replaced", () => {
    expect(html({ tier: 18, rr: null })).toBe(html({ tier: 18, rr: 40 }).replace(">40<", ">—<"));
  });
});
