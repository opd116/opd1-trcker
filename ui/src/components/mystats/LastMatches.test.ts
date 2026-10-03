// Recent Matches rows on My Stats: a row that opens a match has the chevron and the button role; a plain row (nothing to
// open it with) has neither, and its last column stays empty. Unknown stat values take the one dim class.
import { render } from "svelte/server";
import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";
import { myStatsFull } from "../../lib/demo/mystats";
import demoMeta from "../../lib/demo/meta.json";
import type { MatchSummary, Meta } from "../../lib/types";

// The row pulls in the theme store, which reads `location` and paints CSS variables when it loads (see Hero.test.ts).
let LastMatches: typeof import("./LastMatches.svelte").default;
let setMeta: (m: Meta | null) => void;
beforeAll(async () => {
  vi.stubGlobal("location", { search: "" });
  vi.stubGlobal("document", { documentElement: { style: { setProperty: vi.fn() } }, getElementById: () => ({ setAttribute: vi.fn() }) });
  LastMatches = (await import("./LastMatches.svelte")).default;
  const { meta } = await import("../../lib/stores");
  setMeta = (m) => meta.set(m);
  setMeta(demoMeta as Meta);
});
afterAll(() => {
  setMeta(null);
  vi.unstubAllGlobals();
});

const clean = (h: string) => h.replace(/ svelte-[a-z0-9]+/g, "").replace(/<!--.*?-->/g, "");
const html = (rows: MatchSummary[], onopen?: (id: string) => void) => clean(render(LastMatches, { props: { rows, onopen } }).body);
const CHEVRON = 'viewBox="0 0 7 10"';
const count = (h: string, needle: string) => h.split(needle).length - 1;

describe("Recent Matches rows", () => {
  const rows = myStatsFull().last5;

  it("a row that opens a match is a button with the chevron", () => {
    const h = html(rows, () => {});
    expect(count(h, 'role="button"')).toBe(rows.length);
    expect(count(h, CHEVRON)).toBe(rows.length);
    expect(h).not.toContain("rrow plain");
  });

  it("a plain row has no chevron and no button role, and keeps its grid column", () => {
    const h = html(rows);
    expect(count(h, 'role="button"')).toBe(0);
    expect(count(h, CHEVRON)).toBe(0);
    expect(count(h, 'class="rrow plain"')).toBe(rows.length);
    // Nine children per row, the last (the chevron's column) empty: the columns line up with a clickable row's.
    const first = /<div class="rrow plain">(.*?)<\/div><div class="rrow plain">/s.exec(h) ?? /<div class="rrow plain">(.*)$/s.exec(h);
    expect(first![1]).toMatch(/<span><\/span>$/);
  });

  it("an unknown stat is dim, a known one is not", () => {
    const unknown: MatchSummary = { ...rows[0]!, kills: null, deaths: null, assists: null, kd: null, acs: null, hsPct: null, rrEarned: null };
    const h = html([unknown], () => {});
    for (const cls of ["rkd", "racs", "rhs", "rrr"]) expect(h, cls).toMatch(new RegExp(`class="${cls}[^"]*\\bdim\\b[^"]*"`));
    expect(h).toMatch(/class="r dim"/);
    expect(h).toMatch(/class="c dim"/);
    const known = html([rows[0]!], () => {});
    expect(known).not.toMatch(/class="rkd[^"]*\bdim\b/);
    expect(known).not.toMatch(/class="racs[^"]*\bdim\b/);
  });
});
