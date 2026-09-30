// Spec §3.6 current rank: a rank object below tier 3 (Unrated / the unused tiers 1–2) renders exactly like
// `rank: null` (an empty 40×40 box, then —), never a tier icon or an RR line (task 17 review, F6/F14).
import { render } from "svelte/server";
import { describe, expect, it } from "vitest";
import RankCurrent from "./RankCurrent.svelte";

const html = (rank: Parameters<typeof RankCurrent>[1]["rank"]) => render(RankCurrent, { props: { rank } }).body;

/** The markup with Svelte's scope hashes and hydration comments stripped, so two renders compare. */
const shape = (s: string) => s.replace(/ svelte-[a-z0-9]+/g, "").replace(/<!--.*?-->/g, "");

describe("RankCurrent below tier 3", () => {
  it("rank null: an empty box and —", () => {
    const h = shape(html(null));
    expect(h).toContain('class="icon"');
    expect(h).not.toContain("<img");
    expect(h).toContain("—");
    expect(h).not.toContain("RR");
  });

  for (const tier of [0, 1, 2]) {
    it(`tier ${tier} renders exactly like rank null`, () => {
      expect(shape(html({ tier, rr: 40 }))).toBe(shape(html(null)));
    });
  }

  it("tier 3 is the first ranked tier: icon and RR", () => {
    const h = html({ tier: 3, rr: 40 });
    expect(h).toContain("<img");
    expect(h).toContain("RR");
    expect(h).not.toContain("—");
  });
});
