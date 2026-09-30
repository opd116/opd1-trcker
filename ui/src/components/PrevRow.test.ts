// Spec §3.6 Prev (R125/R126): the card row reads icon · `Prev` · name. The greyed icon sits in its own 40px column
// (the current-rank icon's width), which stays, empty, when there is no prev tier; the label follows the column.
import { render } from "svelte/server";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import demoMeta from "../lib/demo/meta.json";
import { meta } from "../lib/stores";
import type { Meta } from "../lib/types";
import PrevRow from "./PrevRow.svelte";

/**
 * The markup with Svelte's scope hashes, hydration comments and the whitespace between tags stripped (the row is
 * a flex container, so that whitespace never renders), so two renders compare.
 */
const shape = (s: string) => s.replace(/ svelte-[a-z0-9]+/g, "").replace(/<!--.*?-->/g, "").replace(/>\s+</g, "><");
const html = (tier: number | null | undefined) => shape(render(PrevRow, { props: { tier } }).body);

describe("PrevRow (R125/R126)", () => {
  beforeEach(() => meta.set(demoMeta as Meta));
  afterEach(() => meta.set(null));

  it("a prev tier: icon column, then Prev, then the tier name", () => {
    const h = html(22);
    expect(h).toMatch(/^<div class="prev"><div class="icol"><img class="pi" src="[^"]+" alt=""\/?><\/div><span class="lbl">Prev<\/span><span class="pname">Ascendant 2<\/span><\/div>$/);
  });

  it("no prev tier: the icon column stays, empty, then Prev —", () => {
    expect(html(null)).toBe('<div class="prev"><div class="icol"></div><span class="lbl">Prev</span><span class="dash">—</span></div>');
  });

  for (const tier of [0, 1, 2]) {
    it(`tier ${tier} renders exactly like no prev tier`, () => {
      expect(html(tier)).toBe(html(null));
    });
  }

  it("loading (absent): the whole row is one bar, no label", () => {
    const h = html(undefined);
    expect(h).toMatch(/^<div class="prev"><span class="sk block"[^>]*><\/span><\/div>$/);
    expect(h).not.toContain("Prev");
  });

  it("meta pending: the name is a bar, the icon and label stay", () => {
    meta.set(null);
    const h = html(22);
    expect(h).toMatch(/<div class="icol"><img class="pi"[^>]*><\/div><span class="lbl">Prev<\/span><span class="pname"><span class="sk"/);
  });
});
