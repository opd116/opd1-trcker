// Historical rank hierarchy: label, 22px icon column, then tier name. An absent tier keeps the icon column.
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

  it("a prev tier: Prev, then icon column, then tier name", () => {
    const h = html(22);
    expect(h).toMatch(/^<div class="prev"><span class="lbl">Prev<\/span><div class="icol"><img class="pi" src="[^"]+" alt=""\/?><\/div><span class="pname">Ascendant 2<\/span><\/div>$/);
  });

  it("no prev tier: the empty icon column stays between Prev and —", () => {
    expect(html(null)).toBe('<div class="prev"><span class="lbl">Prev</span><div class="icol"></div><span class="dash">—</span></div>');
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
    expect(h).toMatch(/<span class="lbl">Prev<\/span><div class="icol"><img class="pi"[^>]*><\/div><span class="pname"><span class="sk"/);
  });
});
