// The RR chart's keyboard model (spec §9.2) and its frames. The keys and the pick are plain functions (`keyTarget`,
// `valueIndex`, in lib/mystats.ts), so they are tested directly; the wiring (focus, blur, preventDefault on the handled
// keys only, the bubble, the ring) needs a real browser and lives in `tests/tips-mystats.spec.ts` and
// `tests/focus-ring.spec.ts`. A server render has no size, so the plot itself never draws here: the render tests cover
// the frames (skeleton, `—`) and that none of them is a tab stop.
import { render } from "svelte/server";
import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";
import { myStatsFull } from "../../lib/demo/mystats";
import demoMeta from "../../lib/demo/meta.json";
import { keyTarget, valueIndex } from "../../lib/mystats";
import type { Meta, RrPoint } from "../../lib/types";

// The chart pulls in the theme store, which reads `location` and paints CSS variables when it loads (see Hero.test.ts).
let RrChart: typeof import("./RrChart.svelte").default;
let setMeta: (m: Meta | null) => void;
beforeAll(async () => {
  vi.stubGlobal("location", { search: "" });
  vi.stubGlobal("document", { documentElement: { style: { setProperty: vi.fn() } }, getElementById: () => ({ setAttribute: vi.fn() }) });
  RrChart = (await import("./RrChart.svelte")).default;
  const { meta } = await import("../../lib/stores");
  setMeta = (m) => meta.set(m);
  setMeta(demoMeta as Meta);
});
afterAll(() => {
  setMeta(null);
  vi.unstubAllGlobals();
});

const key = (k: string, mods: Partial<Record<"ctrlKey" | "altKey" | "metaKey" | "shiftKey", boolean>> = {}) => ({
  key: k,
  ctrlKey: false,
  altKey: false,
  metaKey: false,
  shiftKey: false,
  ...mods,
});

describe("keyTarget: ←/→, Home and End move the pick; every other key is left alone", () => {
  const END = 9;

  it("← and → move one point", () => {
    expect(keyTarget(key("ArrowLeft"), 5, END)).toBe(4);
    expect(keyTarget(key("ArrowRight"), 5, END)).toBe(6);
  });

  it("clamps at both ends", () => {
    expect(keyTarget(key("ArrowLeft"), 0, END)).toBe(0);
    expect(keyTarget(key("ArrowRight"), END, END)).toBe(END);
    expect(keyTarget(key("ArrowLeft"), 1, END)).toBe(0);
    expect(keyTarget(key("ArrowRight"), END - 1, END)).toBe(END);
  });

  it("Home is the first point and End the newest, from anywhere", () => {
    for (const from of [0, 4, END]) {
      expect(keyTarget(key("Home"), from, END), `Home from ${from}`).toBe(0);
      expect(keyTarget(key("End"), from, END), `End from ${from}`).toBe(END);
    }
  });

  it("a key with Ctrl, Alt, Meta or Shift held is not the chart's (null: not prevented, the browser keeps it)", () => {
    for (const k of ["ArrowLeft", "ArrowRight", "Home", "End"]) {
      for (const mod of ["ctrlKey", "altKey", "metaKey", "shiftKey"] as const) {
        expect(keyTarget(key(k, { [mod]: true }), 5, END), `${mod}+${k}`).toBeNull();
      }
    }
  });

  it("every other key is not the chart's: ↑ ↓ PageUp PageDown (the page scrolls), Tab (focus moves), Escape (the tip's), Enter, Space, letters", () => {
    for (const k of ["ArrowUp", "ArrowDown", "PageUp", "PageDown", "Tab", "Escape", "Enter", " ", "a", "1", "Shift", "Control"]) {
      expect(keyTarget(key(k), 5, END), k).toBeNull();
    }
  });
});

describe("valueIndex: where the pick stands", () => {
  /** Points oldest first, one per match, `startMs` 1000 apart. */
  const points = (n: number, from = 0) => Array.from({ length: n }, (_, i) => ({ point: { startMs: (from + i) * 1000 } }));

  it("with no pick it is the newest point (the slider's value while nothing is picked)", () => {
    expect(valueIndex(points(10), null)).toBe(9);
    expect(valueIndex(points(2), null)).toBe(1);
  });

  it("a pick stands at its match's index", () => {
    expect(valueIndex(points(10), { ms: 3000, idx: 3 })).toBe(3);
    expect(valueIndex(points(10), { ms: 0, idx: 0 })).toBe(0);
    expect(valueIndex(points(10), { ms: 9000, idx: 9 })).toBe(9);
  });

  it("a push that inserts older points keeps the pick on its own match, not on the same index", () => {
    const pick = { ms: 3000, idx: 3 };
    // Four older matches arrive (startMs −4000 … −1000): the picked match is now the 8th point.
    const longer = points(14, -4);
    expect(longer[7]!.point.startMs).toBe(3000);
    expect(valueIndex(longer, pick)).toBe(7);
    // Dropping older points moves it the other way.
    expect(valueIndex(points(7, 2), pick)).toBe(1);
    // A new newest match changes nothing for it.
    expect(valueIndex(points(11), pick)).toBe(3);
  });

  it("the remembered index is tried first: nothing moved, nothing searched (two matches with one start time keep their own pick)", () => {
    const twins = [{ point: { startMs: 7 } }, { point: { startMs: 7 } }, { point: { startMs: 9 } }];
    expect(valueIndex(twins, { ms: 7, idx: 1 })).toBe(1);
    expect(valueIndex(twins, { ms: 7, idx: 0 })).toBe(0);
    expect(valueIndex(twins, { ms: 7, idx: 2 }), "the index holds another match: found by start time").toBe(0);
  });

  it("a pick whose match is gone falls back to its old index, clamped into the new series", () => {
    expect(valueIndex(points(10, 100), { ms: 3000, idx: 3 }), "series replaced: the old index").toBe(3);
    expect(valueIndex(points(3, 100), { ms: 3000, idx: 7 }), "shorter series: clamped").toBe(2);
    expect(valueIndex(points(5, 100), { ms: 3000, idx: -2 }), "never below the first").toBe(0);
  });

  it("works on the chart's own points (the demo series)", () => {
    const series = myStatsFull().rrSeries.map((p) => ({ point: p }));
    const mid = series[17]!.point;
    expect(valueIndex(series, { ms: mid.startMs, idx: 17 })).toBe(17);
    expect(valueIndex(series, null)).toBe(series.length - 1);
  });
});

describe("the frames: no plot, no tab stop", () => {
  const html = (props: { series: RrPoint[] | undefined }) =>
    render(RrChart, { props }).body.replace(/ svelte-[a-z0-9]+/g, "").replace(/<!--.*?-->/g, "");
  const point = myStatsFull().rrSeries[0]!;

  it("loading (the skeleton): neither a slider nor a tabindex", () => {
    const h = html({ series: undefined });
    expect(h).toContain('class="skel"');
    expect(h).not.toContain("slider");
    expect(h).not.toContain("tabindex");
  });

  it("no points, or one (nothing to draw a line through): the empty frame with —, no tab stop", () => {
    for (const series of [[], [point]]) {
      const h = html({ series });
      expect(h, `${series.length} points`).toContain('class="none"');
      expect(h).toContain("—");
      expect(h).not.toContain("slider");
      expect(h).not.toContain("tabindex");
      expect(h).not.toContain("<svg");
    }
  });

  it("a box too small for a plot (a server render has none) is the empty frame too, not a stop over nothing", () => {
    const h = html({ series: myStatsFull().rrSeries });
    expect(h).toContain('class="none"');
    expect(h).not.toContain("slider");
    expect(h).not.toContain("tabindex");
  });
});
