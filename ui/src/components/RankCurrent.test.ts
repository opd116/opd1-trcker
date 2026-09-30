// Spec §3.6 current rank: a rank object below tier 3 (Unrated / the unused tiers 1–2) renders exactly like
// `rank: null` (an empty 40×40 box, then —), never a tier icon or an RR line (task 17 review, F6/F14).
import { render } from "svelte/server";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import demoMeta from "../lib/demo/meta.json";
import { meta } from "../lib/stores";
import type { Meta } from "../lib/types";
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

// R154: when the MMR request failed, the tier comes from the stored matches and the exact RR is unknown (null).
describe("RankCurrent with an unknown RR (R154)", () => {
  beforeAll(() => meta.set(demoMeta as Meta));
  afterAll(() => meta.set(null));

  /** The text of the RR number's own span. */
  const rrValue = (h: string) => /<span class="v[^"]*">(.*?)<\/span>/.exec(shape(h))![1];

  it("the tier icon and the tier name draw exactly as with a number", () => {
    const h = shape(html({ tier: 18, rr: null }));
    expect(h).toContain("<img");
    expect(h).toContain("Diamond");
    expect(rrValue(html({ tier: 18, rr: 40 }))).toBe("40");
    expect(h.match(/<img[^>]*>/)![0]).toBe(shape(html({ tier: 18, rr: 40 })).match(/<img[^>]*>/)![0]);
  });

  it("— stands where the number goes: the RR slot keeps its markup, so its width and alignment do", () => {
    expect(rrValue(html({ tier: 18, rr: null }))).toBe("—");
    // The same markup as a known RR with only the number replaced: no extra text, no badge, no other element.
    expect(shape(html({ tier: 18, rr: null }))).toBe(shape(html({ tier: 18, rr: 40 })).replace(">40<", ">—<"));
  });

  it("rr 0 is a real RR, not an unknown one", () => {
    expect(rrValue(html({ tier: 18, rr: 0 }))).toBe("0");
  });
});
