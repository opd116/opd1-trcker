import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { attachTooltips, hideTip, parseWl, retip, tip, tipOver, tipPosition, tipState, wlRow } from "./tooltip.svelte";
import {
  ACS_TIP,
  ACT_SCOPE,
  ADR_TIP,
  actScopeTip,
  aprTip,
  encounterMapTip,
  encounterTip,
  HALVES_TIP,
  heroStatTip,
  insightTip,
  last5Tip,
  liveTip,
  loadoutTip,
  nameTip,
  partyTip,
  peakTip,
  PM_TIP,
  RECENT_SCOPE,
  rrPointTip,
  rrToNextTip,
  SESSION_SCOPE,
  sessionChipLabel,
  sessionChipTip,
  whenTip,
  winCellTip,
  rankTip,
  recentTimeTip,
  winTip,
} from "./tips";
import type { Meta, Peak, PlayerCard, SessionMatch } from "./types";

const rect = (left: number, top: number, width: number, height: number): DOMRectReadOnly =>
  ({ left, top, width, height, right: left + width, bottom: top + height, x: left, y: top }) as DOMRectReadOnly;

describe("tipPosition (spec §4.1)", () => {
  const R = rect(0, 0, 1000, 800);

  it("centres on the trigger", () => {
    expect(tipPosition(rect(400, 100, 100, 20), R)).toEqual({ x: 450, top: 128, transform: "translate(-50%,0)" });
  });

  it("clamps x to 136–864", () => {
    expect(tipPosition(rect(0, 100, 20, 20), R).x).toBe(136);
    expect(tipPosition(rect(980, 100, 20, 20), R).x).toBe(864);
    expect(tipPosition(rect(136 - 5, 100, 10, 20), R).x).toBe(136);
    expect(tipPosition(rect(500, 100, 0, 20), R).x).toBe(500);
  });

  it("flips above when below > 720", () => {
    // bottom 712 → below 720: still below
    expect(tipPosition(rect(400, 692, 100, 20), R)).toEqual({ x: 450, top: 720, transform: "translate(-50%,0)" });
    // bottom 713 → below 721: above the trigger
    expect(tipPosition(rect(400, 693, 100, 20), R)).toEqual({ x: 450, top: 685, transform: "translate(-50%,-100%)" });
  });

  it("is relative to the app rect", () => {
    const off = rect(20, 30, 1000, 800);
    expect(tipPosition(rect(420, 130, 100, 20), off)).toEqual({ x: 450, top: 128, transform: "translate(-50%,0)" });
  });
});

describe("parseWl and wlRow", () => {
  it("parseWl('wl:31:22:53')", () => {
    expect(parseWl("wl:31:22:53")).toEqual({ w: 31, l: 22, g: 53 });
    expect(parseWl("wl:1:0:1")).toEqual({ w: 1, l: 0, g: 1 });
  });

  it("an optional `|text` names the slice in the second row (My Stats' weekday and time-of-day cells)", () => {
    expect(parseWl("wl:5:3:8|This act · Monday")).toEqual({ w: 5, l: 3, g: 8, scope: "This act · Monday" });
    expect(parseWl("wl:5:3:8|This act · 5 PM – 10 PM")?.scope).toBe("This act · 5 PM – 10 PM");
    expect(parseWl("wl:5:3:8")).not.toHaveProperty("scope");
    expect(parseWl("wl:5:3:8|")).toBeNull(); // an empty slice is malformed, never an empty row
    expect(parseWl("wl:5:3:8|a\nb")).toBeNull(); // one line
  });

  it("rejects anything else", () => {
    expect(parseWl("31:22:53")).toBeNull();
    expect(parseWl("wl:31:22")).toBeNull();
    expect(parseWl("wl:a:b:c")).toBeNull();
    expect(parseWl("Last 5 competitive")).toBeNull();
    expect(parseWl("")).toBeNull();
  });

  it("the structured W/L row reads `1W · 0L · 1 game` for `wl:1:0:1`", () => {
    expect(wlRow(parseWl("wl:1:0:1")!).join(" ")).toBe("1W · 0L · 1 game");
    expect(wlRow(parseWl("wl:31:22:53")!).join(" ")).toBe("31W · 22L · 53 games");
  });
});

describe("tip action", () => {
  function node() {
    const attrs = new Map<string, string>();
    return {
      attrs,
      setAttribute: (k: string, v: string) => void attrs.set(k, v),
      removeAttribute: (k: string) => void attrs.delete(k),
      getAttribute: (k: string) => attrs.get(k) ?? null,
    };
  }

  it("sets data-tip, and data-tip-ov for overflow-only", () => {
    const n = node();
    const a = tip(n as unknown as HTMLElement, "Party · likely");
    expect(n.attrs.get("data-tip")).toBe("Party · likely");
    expect(n.attrs.has("data-tip-ov")).toBe(false);

    a.update({ text: "Fahad#OPD1", overflowOnly: true });
    expect(n.attrs.get("data-tip")).toBe("Fahad#OPD1");
    expect(n.attrs.get("data-tip-ov")).toBe("1");

    a.update("Hidden name");
    expect(n.attrs.get("data-tip")).toBe("Hidden name");
    expect(n.attrs.has("data-tip-ov")).toBe(false);

    a.destroy();
    expect(n.attrs.size).toBe(0);
  });

  it("`ifCut` sets data-tip-cut (never data-tip-ov: the tip itself always shows) and is cleared by a plain update and by destroy", () => {
    const n = node();
    const a = tip(n as unknown as HTMLElement, { text: "Hidden name · known from your match", ifCut: "Fahad#OPD1" });
    expect(n.attrs.get("data-tip")).toBe("Hidden name · known from your match");
    expect(n.attrs.get("data-tip-cut")).toBe("Fahad#OPD1");
    expect(n.attrs.has("data-tip-ov")).toBe(false);

    a.update("Hidden name");
    expect(n.attrs.has("data-tip-cut")).toBe(false);

    a.update({ text: "T", ifCut: "X#Y" });
    expect(n.attrs.get("data-tip-cut")).toBe("X#Y");
    a.destroy();
    expect(n.attrs.size).toBe(0);
  });

  it("an empty text is an empty data-tip, which shows nothing", () => {
    const n = node();
    tip(n as unknown as HTMLElement, "");
    expect(n.attrs.get("data-tip")).toBe("");
  });
});

describe("tooltip engine (delegated mouseover, 450 ms)", () => {
  const root = { getBoundingClientRect: () => rect(0, 0, 1000, 800) } as unknown as Element;

  function trigger(attrs: Record<string, string>, r: DOMRectReadOnly, span?: { scrollWidth: number; clientWidth: number }) {
    return {
      getAttribute: (k: string) => attrs[k] ?? null,
      getBoundingClientRect: () => r,
      querySelector: (sel: string) => (sel === "span" ? (span ?? null) : null),
    } as unknown as Element;
  }
  const over = (el: Element | null) =>
    tipOver({ closest: (sel: string) => (sel === "[data-tip]" ? el : null) } as unknown as Element, root);

  beforeEach(() => {
    vi.useFakeTimers();
    hideTip();
  });
  afterEach(() => {
    hideTip();
    vi.useRealTimers();
  });

  it("shows after 450 ms, not before, at the computed position", () => {
    const el = trigger({ "data-tip": "Party · likely" }, rect(400, 100, 100, 20));
    over(el);
    vi.advanceTimersByTime(449);
    expect(tipState.tip).toBeNull();
    vi.advanceTimersByTime(1);
    expect(tipState.tip).toEqual({ text: "Party · likely", x: 450, top: 128, transform: "translate(-50%,0)" });
  });

  it("moving to another trigger or to none clears it immediately", () => {
    const a = trigger({ "data-tip": "A" }, rect(100, 100, 40, 20));
    const b = trigger({ "data-tip": "B" }, rect(300, 100, 40, 20));
    over(a);
    vi.advanceTimersByTime(450);
    expect(tipState.tip?.text).toBe("A");
    over(b);
    expect(tipState.tip).toBeNull();
    vi.advanceTimersByTime(450);
    expect(tipState.tip?.text).toBe("B");
    over(null);
    expect(tipState.tip).toBeNull();
    vi.advanceTimersByTime(1000);
    expect(tipState.tip).toBeNull();
  });

  it("moving within the same trigger keeps it and does not restart the timer", () => {
    const a = trigger({ "data-tip": "A" }, rect(100, 100, 40, 20));
    over(a);
    vi.advanceTimersByTime(300);
    over(a);
    vi.advanceTimersByTime(150);
    expect(tipState.tip?.text).toBe("A");
  });

  it("an empty data-tip shadows nothing and shows nothing", () => {
    over(trigger({ "data-tip": "" }, rect(100, 100, 40, 20)));
    vi.advanceTimersByTime(1000);
    expect(tipState.tip).toBeNull();
  });

  it("overflow-only shows only when the name span is truncated", () => {
    const fits = trigger({ "data-tip": "Fahad#OPD1", "data-tip-ov": "1" }, rect(100, 100, 100, 20), {
      scrollWidth: 60,
      clientWidth: 60,
    });
    over(fits);
    vi.advanceTimersByTime(1000);
    expect(tipState.tip).toBeNull();

    const cut = trigger({ "data-tip": "WWWWWWWWWWWWWWWW#WWWWW", "data-tip-ov": "1" }, rect(100, 100, 100, 20), {
      scrollWidth: 190,
      clientWidth: 150,
    });
    over(cut);
    vi.advanceTimersByTime(449);
    expect(tipState.tip).toBeNull();
    vi.advanceTimersByTime(1);
    expect(tipState.tip?.text).toBe("WWWWWWWWWWWWWWWW#WWWWW");
  });

  it("`data-tip-cut` adds its line under the tip only while the name span is cut off", () => {
    const attrs = { "data-tip": "Hidden name · known from your match\nBind · Sep 12, 2026", "data-tip-cut": "WWWWWWWWWWWWWWWW#WWWWW" };
    over(trigger(attrs, rect(100, 100, 100, 20), { scrollWidth: 60, clientWidth: 60 }));
    vi.advanceTimersByTime(450);
    expect(tipState.tip?.text).toBe("Hidden name · known from your match\nBind · Sep 12, 2026"); // fits: the plain tip
    hideTip();

    over(trigger(attrs, rect(100, 100, 100, 20), { scrollWidth: 190, clientWidth: 150 }));
    vi.advanceTimersByTime(450);
    expect(tipState.tip?.text).toBe("Hidden name · known from your match\nBind · Sep 12, 2026\nWWWWWWWWWWWWWWWW#WWWWW");
  });

  it("`data-tip-cut` with no name span (nothing to measure) shows the plain tip", () => {
    over(trigger({ "data-tip": "Hidden name", "data-tip-cut": "X#Y" }, rect(100, 100, 100, 20)));
    vi.advanceTimersByTime(450);
    expect(tipState.tip?.text).toBe("Hidden name");
  });

  describe("retip: one hover surface, the tip follows its target (the RR chart)", () => {
    const fakeRoot = {
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      getBoundingClientRect: () => rect(0, 0, 1000, 800),
    } as unknown as HTMLElement;
    /** A trigger that finds itself for `closest`, as the real element does. */
    const self = (attrs: Record<string, string>, box: { r: DOMRectReadOnly }) => {
      const el: Record<string, unknown> = {
        getAttribute: (k: string) => attrs[k] ?? null,
        getBoundingClientRect: () => box.r,
        querySelector: () => null,
      };
      el.closest = () => el;
      return el as unknown as Element;
    };

    it("restarts the delay for the same trigger: the old bubble goes, the new text shows after 450 ms, placed by its new box", () => {
      const detach = attachTooltips(fakeRoot);
      const attrs = { "data-tip": "+18 RR · Ascendant 2 · 47 RR · Sep 28" };
      const box = { r: rect(300, 100, 20, 28) };
      const el = self(attrs, box);
      tipOver(el, fakeRoot);
      vi.advanceTimersByTime(450);
      expect(tipState.tip?.text).toBe("+18 RR · Ascendant 2 · 47 RR · Sep 28");

      // The pointer moved to the next point: the trigger's text and box changed, no mouseover came.
      attrs["data-tip"] = "−15 RR · Ascendant 2 · 32 RR · Sep 29";
      box.r = rect(500, 120, 20, 28);
      retip(el);
      expect(tipState.tip, "the old bubble is gone at once").toBeNull();
      vi.advanceTimersByTime(449);
      expect(tipState.tip).toBeNull();
      vi.advanceTimersByTime(1);
      expect(tipState.tip).toEqual({ text: "−15 RR · Ascendant 2 · 32 RR · Sep 29", x: 510, top: 156, transform: "translate(-50%,0)" });
      detach();
    });

    it("a trigger with an empty tip shows nothing, and a plain mouseover on it afterwards still works", () => {
      const detach = attachTooltips(fakeRoot);
      const attrs = { "data-tip": "" };
      const el = self(attrs, { r: rect(300, 100, 20, 28) });
      retip(el);
      vi.advanceTimersByTime(1000);
      expect(tipState.tip).toBeNull();
      attrs["data-tip"] = "A";
      tipOver(el, fakeRoot);
      vi.advanceTimersByTime(450);
      expect(tipState.tip?.text).toBe("A");
      detach();
    });

    it("before the engine is attached, or after it is detached, there is no root to place a bubble in: nothing happens", () => {
      const el = self({ "data-tip": "A" }, { r: rect(300, 100, 20, 28) });
      retip(el);
      vi.advanceTimersByTime(1000);
      expect(tipState.tip).toBeNull();
      const detach = attachTooltips(fakeRoot);
      detach();
      retip(el);
      vi.advanceTimersByTime(1000);
      expect(tipState.tip).toBeNull();
    });
  });

  it("a text starting with wl: that does not parse shows nothing", () => {
    over(trigger({ "data-tip": "wl:oops" }, rect(100, 100, 40, 20)));
    vi.advanceTimersByTime(1000);
    expect(tipState.tip).toBeNull();
  });

  it("reads data-tip at fire time: text changed during the delay shows the new string (v8:525)", () => {
    const attrs = { "data-tip": "2h" };
    over(trigger(attrs, rect(400, 100, 100, 20)));
    vi.advanceTimersByTime(200);
    attrs["data-tip"] = "3h";
    vi.advanceTimersByTime(250);
    expect(tipState.tip?.text).toBe("3h");
  });

  it("skips the tip when data-tip went empty or malformed during the delay", () => {
    const attrs = { "data-tip": "2h" };
    over(trigger(attrs, rect(400, 100, 100, 20)));
    vi.advanceTimersByTime(200);
    attrs["data-tip"] = "";
    vi.advanceTimersByTime(1000);
    expect(tipState.tip).toBeNull();

    const wl = { "data-tip": "wl:31:22:53" };
    over(trigger(wl, rect(100, 100, 40, 20)));
    vi.advanceTimersByTime(200);
    wl["data-tip"] = "wl:oops";
    vi.advanceTimersByTime(1000);
    expect(tipState.tip).toBeNull();
  });

  it("hideTip cancels a pending tip", () => {
    over(trigger({ "data-tip": "A" }, rect(100, 100, 40, 20)));
    vi.advanceTimersByTime(200);
    hideTip();
    vi.advanceTimersByTime(1000);
    expect(tipState.tip).toBeNull();
  });
});

// ── tips.ts catalogue (spec §4.2) ───────────────────────────────────────────────
const NOW = Date.parse("2026-09-29T21:59:00Z");
const HAVEN = "2bee0dc9-4ffe-519b-1cbd-7fbe763a6047";
const SPLIT = "d960549e-485c-e861-8d71-aa9d1aed12a2";
const meta: Meta = {
  agents: {},
  maps: { [HAVEN]: { name: "Haven" }, [SPLIT]: { name: "Split" } },
  tiers: {},
};
const FROM = Date.parse("2026-08-30T21:00:00Z");

const peak = (over: Partial<Peak>): Peak => ({
  tier: 24,
  actLabel: "V26 A5",
  actName: "V26 ACT V",
  sinceActEndMs: null,
  actStartMs: Date.UTC(2026, 7, 26),
  actEndMs: Date.UTC(2026, 9, 21),
  ...over,
});

describe("peakTip", () => {
  it("current act: `{label} · Current act` plus the dates", () => {
    expect(peakTip(peak({}))).toBe("V26 A5 · Current act\nAug 26 – Oct 20, 2026");
  });

  it("past act: the full act title plus the dates", () => {
    const a4 = peak({
      actLabel: "V26 A4",
      actName: "V26 ACT IV",
      sinceActEndMs: 34 * 86_400_000,
      actStartMs: Date.UTC(2026, 5, 24),
      actEndMs: Date.UTC(2026, 7, 26),
    });
    expect(peakTip(a4)).toBe("VALORANT 2026 · Act 4\nJun 24 – Aug 25, 2026");
    const e9 = peak({
      actLabel: "E9 A3",
      actName: "EPISODE 9 ACT III",
      sinceActEndMs: 600 * 86_400_000,
      actStartMs: Date.UTC(2024, 9, 22),
      actEndMs: Date.UTC(2025, 0, 9),
    });
    expect(peakTip(e9)).toBe("Episode 9 · Act 3\nOct 22, 2024 – Jan 8, 2025");
  });
});

describe("nameTip", () => {
  it("visible: the full name and tag", () => {
    expect(nameTip({ kind: "visible", name: "Fahad", tag: "OPD1" }, meta)).toBe("Fahad#OPD1");
  });

  it("hidden kinds: `Hidden name`", () => {
    expect(nameTip({ kind: "hiddenPlayer" }, meta)).toBe("Hidden name");
    expect(nameTip({ kind: "hiddenAgent", agentId: "x" }, meta)).toBe("Hidden name");
    expect(nameTip({ kind: "hiddenAgent", agentId: "x" }, null)).toBe("Hidden name");
  });

  it("remembered from your own match (ownMatch): map and date, one of them, or none", () => {
    const base = { kind: "remembered", name: "Rook", tag: "4413", source: "ownMatch" } as const;
    const head = "Hidden name · known from your match";
    expect(nameTip({ ...base, fromMapId: SPLIT, fromStartMs: FROM }, meta)).toBe(`${head}\nSplit · Aug 30, 2026`);
    expect(nameTip({ ...base, fromMapId: SPLIT, fromStartMs: null }, meta)).toBe(`${head}\nSplit`);
    expect(nameTip({ ...base, fromMapId: null, fromStartMs: FROM }, meta)).toBe(`${head}\nAug 30, 2026`);
    expect(nameTip({ ...base, fromMapId: null, fromStartMs: null }, meta)).toBe(head);
  });

  it("remembered from someone else's match (otherMatch, R145): `known from a past match`, then map · date", () => {
    const base = { kind: "remembered", name: "Rook", tag: "4413", source: "otherMatch" } as const;
    const head = "Hidden name · known from a past match";
    expect(nameTip({ ...base, fromMapId: SPLIT, fromStartMs: FROM }, meta)).toBe(`${head}\nSplit · Aug 30, 2026`);
    expect(nameTip({ ...base, fromMapId: SPLIT, fromStartMs: null }, meta)).toBe(`${head}\nSplit`);
    expect(nameTip({ ...base, fromMapId: null, fromStartMs: FROM }, meta)).toBe(`${head}\nAug 30, 2026`);
    expect(nameTip({ ...base, fromMapId: null, fromStartMs: null }, meta)).toBe(head);
  });

  it("remembered from a name-service answer (seen, R145): `seen before`, then the date only (there is no map)", () => {
    const base = { kind: "remembered", name: "Rook", tag: "4413", source: "seen" } as const;
    const head = "Hidden name · seen before";
    expect(nameTip({ ...base, fromMapId: null, fromStartMs: FROM }, meta)).toBe(`${head}\nAug 30, 2026`);
    expect(nameTip({ ...base, fromMapId: null, fromStartMs: null }, meta)).toBe(head);
    // Even if a map id ever came with it, `seen` never prints a map.
    expect(nameTip({ ...base, fromMapId: SPLIT, fromStartMs: FROM }, meta)).toBe(`${head}\nAug 30, 2026`);
    expect(nameTip({ ...base, fromMapId: SPLIT, fromStartMs: null }, meta)).toBe(head);
  });

  it("meta misses and pending meta drop only the map (spec §5.8), for every source", () => {
    for (const [source, head] of [
      ["ownMatch", "Hidden name · known from your match"],
      ["otherMatch", "Hidden name · known from a past match"],
    ] as const) {
      const r = { kind: "remembered", name: "Rook", tag: "4413", source, fromMapId: "not-in-meta", fromStartMs: FROM } as const;
      expect(nameTip(r, meta)).toBe(`${head}\nAug 30, 2026`);
      expect(nameTip({ ...r, fromMapId: SPLIT }, null)).toBe(`${head}\nAug 30, 2026`);
      expect(nameTip({ ...r, fromStartMs: null }, meta)).toBe(head);
      for (const m of [null, meta]) {
        for (const s of [nameTip(r, m), nameTip({ ...r, fromStartMs: null }, m), nameTip({ ...r, fromMapId: null }, m)]) {
          expect(s).not.toMatch(/undefined|null/);
        }
      }
    }
  });
});

describe("last5Tip", () => {
  const at = (msAgo: number) => NOW - msAgo;
  it("`{rr} RR · {span}`", () => {
    expect(last5Tip({ result: "win", rrEarned: 18, startMs: at(45 * 60_000) }, NOW)).toBe("+18 RR · 45m");
    expect(last5Tip({ result: "loss", rrEarned: -15, startMs: at(2 * 3_600_000) }, NOW)).toBe("−15 RR · 2h");
    expect(last5Tip({ result: "draw", rrEarned: 0, startMs: at(4 * 3_600_000) }, NOW)).toBe("0 RR · 4h");
  });

  it("Win · 2h when rrEarned is null", () => {
    expect(last5Tip({ result: "win", rrEarned: null, startMs: at(2 * 3_600_000) }, NOW)).toBe("Win · 2h");
    expect(last5Tip({ result: "loss", rrEarned: null, startMs: at(3 * 86_400_000) }, NOW)).toBe("Loss · 3d");
    expect(last5Tip({ result: "draw", rrEarned: null, startMs: at(90 * 60_000) }, NOW)).toBe("Draw · 1h");
  });
});

describe("sessionChipTip (spec §9.6)", () => {
  const at = (msAgo: number) => NOW - msAgo;
  const m = (over: Partial<SessionMatch>): SessionMatch => ({
    matchId: "m1",
    mapId: HAVEN,
    rrEarned: 18,
    startMs: at(2 * 3_600_000),
    result: "W",
    ...over,
  });

  it("`{map} · {rr} RR · {span}`: the last-5 words with the map in front", () => {
    expect(sessionChipTip(m({}), meta, NOW)).toBe("Haven · +18 RR · 2h");
    expect(sessionChipTip(m({ result: "L", rrEarned: -15, startMs: at(45 * 60_000), mapId: SPLIT }), meta, NOW)).toBe("Split · −15 RR · 45m");
    expect(sessionChipTip(m({ result: "D", rrEarned: 0 }), meta, NOW)).toBe("Haven · 0 RR · 2h");
  });

  it("Win / Loss / Draw stands in for an RR change Riot did not record", () => {
    expect(sessionChipTip(m({ rrEarned: null }), meta, NOW)).toBe("Haven · Win · 2h");
    expect(sessionChipTip(m({ rrEarned: null, result: "L" }), meta, NOW)).toBe("Haven · Loss · 2h");
    expect(sessionChipTip(m({ rrEarned: null, result: "D", startMs: at(90 * 60_000) }), meta, NOW)).toBe("Haven · Draw · 1h");
  });

  it("the map is dropped while meta is pending, for a map the table lacks and for no map (spec §5.8)", () => {
    expect(sessionChipTip(m({}), null, NOW)).toBe("+18 RR · 2h");
    expect(sessionChipTip(m({ mapId: "not-in-meta" }), meta, NOW)).toBe("+18 RR · 2h");
    expect(sessionChipTip(m({ mapId: null }), meta, NOW)).toBe("+18 RR · 2h");
    expect(sessionChipTip(m({ mapId: null, rrEarned: null }), null, NOW)).toBe("Win · 2h");
  });

  it("the age is the span since the match started, on the shared clock", () => {
    expect(sessionChipTip(m({ startMs: at(30_000) }), meta, NOW)).toBe("Haven · +18 RR · 1m");
    expect(sessionChipTip(m({ startMs: at(26 * 3_600_000) }), meta, NOW)).toBe("Haven · +18 RR · 1d");
  });
});

describe("sessionChipLabel (the openable chip's accessible name)", () => {
  const m = (over: Partial<SessionMatch>): SessionMatch => ({ matchId: "m1", mapId: HAVEN, rrEarned: 18, startMs: NOW - 2 * 3_600_000, result: "W", ...over });

  it("the result word, then the tip: `Win · Haven · +18 RR · 2h`", () => {
    expect(sessionChipLabel(m({}), meta, NOW)).toBe("Win · Haven · +18 RR · 2h");
    expect(sessionChipLabel(m({ result: "L", rrEarned: -15 }), meta, NOW)).toBe("Loss · Haven · −15 RR · 2h");
    expect(sessionChipLabel(m({ result: "D", rrEarned: 0 }), meta, NOW)).toBe("Draw · Haven · 0 RR · 2h");
  });

  it("without a stored RR change the tip already names the result, so it is not said twice; no map, no map part", () => {
    expect(sessionChipLabel(m({ rrEarned: null }), meta, NOW)).toBe("Haven · Win · 2h");
    expect(sessionChipLabel(m({ rrEarned: null, result: "D", mapId: null }), meta, NOW)).toBe("Draw · 2h");
    expect(sessionChipLabel(m({ result: "L", rrEarned: -15 }), null, NOW)).toBe("Loss · −15 RR · 2h");
  });
});

describe("encounterTip, partyTip, winTip, aprTip", () => {
  it("encounterTip", () => {
    expect(encounterTip({ with: 1, against: 2 })).toBe("1 with · 2 against");
    expect(encounterTip({ with: 70, against: 53 })).toBe("70 with · 53 against");
  });

  it("partyTip", () => {
    expect(partyTip({ group: 0, confirmed: true })).toBe("Party · confirmed");
    expect(partyTip({ group: 1, confirmed: false })).toBe("Party · likely");
  });

  it("winTip is the wl: form, and '' when actGames is null", () => {
    expect(winTip({ actWins: 31, actGames: 53 } as PlayerCard)).toBe("wl:31:22:53");
    expect(winTip({ actWins: 1, actGames: 1 } as PlayerCard)).toBe("wl:1:0:1");
    expect(winTip({ actWins: 31, actGames: null } as PlayerCard)).toBe("");
    expect(winTip({ actWins: null, actGames: 53 } as PlayerCard)).toBe("");
    expect(winTip({} as PlayerCard)).toBe("");
  });

  it("aprTip pluralises", () => {
    expect(aprTip(1)).toBe("Average party rank\nFrom 1 ranked player");
    expect(aprTip(3).endsWith("From 3 ranked players")).toBe(true);
    expect(aprTip(2)).toBe("Average party rank\nFrom 2 ranked players");
  });
});

// ── Player Details and Match Details (spec §4.2, Task 18) ────────────────────────
describe("Details and Match tips", () => {
  const BREACH = "5f8d3a7f-467b-97f3-062c-13acf203c006";
  const dm: Meta = { agents: { [BREACH]: { name: "Breach" } }, maps: {}, tiers: { "21": { name: "ASCENDANT 1", color: "2a9d8fff" } } };

  it("hero K/D and HS%: `Last {N} competitive · this act` (R116); none for a null value (no counted match)", () => {
    expect(heroStatTip(1.52, 20)).toBe("Last 20 competitive · this act");
    expect(heroStatTip(0.29, 1)).toBe("Last 1 competitive · this act");
    expect(heroStatTip(null, 0)).toBe("");
    // HS% may be null with counted matches (no shot landed): still no tip, the — has nothing to explain.
    expect(heroStatTip(null, 12)).toBe("");
  });

  it("hero Games and W / L: `This act`; none while the value is still loading", () => {
    expect(actScopeTip(65)).toBe("This act");
    expect(actScopeTip(null)).toBe("This act");
    expect(actScopeTip(undefined)).toBe("");
  });

  it("recent row time text: `9:14 PM`", () => {
    expect(recentTimeTip(Date.UTC(2026, 8, 29, 21, 14))).toBe("9:14 PM");
    expect(recentTimeTip(Date.UTC(2026, 8, 29, 0, 5))).toBe("12:05 AM");
  });

  it("rank icon: the tier name; no tip while meta is pending, for a miss or below tier 3 (spec §5.8)", () => {
    expect(rankTip(dm, 21)).toBe("Ascendant 1");
    expect(rankTip(null, 21)).toBe("");
    expect(rankTip(dm, 22)).toBe("");
    expect(rankTip(dm, 0)).toBe("");
    expect(rankTip(dm, null)).toBe("");
  });

  it("encounter map cell: `Sep 12 · 11:18 PM · 41 min`, a year when not this year, no duration when unknown", () => {
    const sep12 = Date.UTC(2026, 8, 12, 23, 18);
    expect(encounterMapTip({ startMs: sep12, durationMs: 41 * 60_000 }, NOW)).toBe("Sep 12 · 11:18 PM · 41 min");
    expect(encounterMapTip({ startMs: sep12, durationMs: null }, NOW)).toBe("Sep 12 · 11:18 PM");
    expect(encounterMapTip({ startMs: Date.UTC(2025, 11, 30, 20, 5), durationMs: 36 * 60_000 }, NOW)).toBe("Dec 30, 2025 · 8:05 PM · 36 min");
  });

  it("Top 3 live icons: `In this match · Breach`; the agent part is dropped when unknown (spec §5.8)", () => {
    expect(liveTip(dm, BREACH)).toBe("In this match · Breach");
    expect(liveTip(dm, null)).toBe("In this match");
    expect(liveTip(null, BREACH)).toBe("In this match");
    expect(liveTip(dm, "not-in-meta")).toBe("In this match");
  });

  it("loadout well: `{Tier} Edition`; none for a default skin or an unknown tier", () => {
    expect(loadoutTip("60bca009-4182-7998-dee7-b8a2558dc369")).toBe("Premium Edition");
    expect(loadoutTip("411e4a55-4e59-7757-41f0-86a53f101bb5")).toBe("Ultra Edition");
    expect(loadoutTip(null)).toBe("");
    expect(loadoutTip("00000000-0000-0000-0000-000000000000")).toBe("");
  });

  it("match header and scoreboard: halves, ACS, +/−, ADR", () => {
    expect(HALVES_TIP).toBe("First half · Second half");
    expect(ACS_TIP).toBe("Average combat score");
    expect(PM_TIP).toBe("Kills minus deaths");
    expect(ADR_TIP).toBe("Average damage per round");
  });
});

describe("My Stats tips (spec §9.6)", () => {
  const NOW = Date.parse("2026-09-29T21:59:00Z");

  it("the scopes: the act, the last 5, the session", () => {
    expect(ACT_SCOPE).toBe("This act");
    expect(RECENT_SCOPE).toBe("Last 5 competitive");
    expect(SESSION_SCOPE).toBe("Current session");
  });

  it("the progress bar: `74 RR to next tier`; none without a next step", () => {
    expect(rrToNextTip(74)).toBe("74 RR to next tier");
    expect(rrToNextTip(1)).toBe("1 RR to next tier");
    expect(rrToNextTip(null)).toBe("");
    expect(rrToNextTip(undefined)).toBe("");
  });

  it("a rank change or a session start: the local date and clock time", () => {
    expect(whenTip(Date.UTC(2026, 8, 29, 21, 14))).toBe("Sep 29, 2026 · 9:14 PM");
  });

  it("an RR chart point: `+18 RR · Ascendant 2 · 47 RR · Sep 29`; the first point has no change", () => {
    const ms = Date.UTC(2026, 8, 29, 16, 40);
    expect(rrPointTip(18, "Ascendant 2", 47, ms, NOW)).toBe("+18 RR · Ascendant 2 · 47 RR · Sep 29");
    expect(rrPointTip(-15, "Immortal", 87, ms, NOW)).toBe("−15 RR · Immortal · 87 RR · Sep 29");
    expect(rrPointTip(0, "Ascendant 2", 0, ms, NOW)).toBe("0 RR · Ascendant 2 · 0 RR · Sep 29");
    expect(rrPointTip(null, "Ascendant 2", 47, ms, NOW)).toBe("Ascendant 2 · 47 RR · Sep 29");
    expect(rrPointTip(18, "Ascendant 2", 47, Date.UTC(2025, 11, 30), NOW)).toBe("+18 RR · Ascendant 2 · 47 RR · Dec 30, 2025");
  });

  it("an RR chart point drops a part it does not know (meta pending or a missing tier), never printing undefined or null", () => {
    const ms = Date.UTC(2026, 8, 29);
    // The RR goes with its tier: it is the tier's RR.
    expect(rrPointTip(18, undefined, 47, ms, NOW)).toBe("+18 RR · Sep 29");
    expect(rrPointTip(18, null, 47, ms, NOW)).toBe("+18 RR · Sep 29");
    expect(rrPointTip(null, null, 47, ms, NOW)).toBe("Sep 29");
  });

  it("an insight: the sample behind it", () => {
    expect(insightTip(16)).toBe("From 16 matches");
    expect(insightTip(1)).toBe("From 1 match");
    expect(insightTip(0)).toBe("From 0 matches");
  });

  it("a Win% cell is the structured W·L·games tip; a weekday or time-of-day cell adds its slice to the second row", () => {
    expect(winCellTip(41, 24, 65)).toBe("wl:41:24:65");
    expect(winCellTip(5, 3, 8, "Monday")).toBe("wl:5:3:8|This act · Monday");
    expect(winCellTip(5, 3, 8, "5 PM – 10 PM")).toBe("wl:5:3:8|This act · 5 PM – 10 PM");
    expect(parseWl(winCellTip(5, 3, 8, "Monday"))).toEqual({ w: 5, l: 3, g: 8, scope: "This act · Monday" });
    expect(wlRow(parseWl(winCellTip(41, 24, 65))!).join(" ")).toBe("41W · 24L · 65 games");
  });

  it("no games, no tip", () => {
    expect(winCellTip(0, 0, 0)).toBe("");
    expect(winCellTip(0, 0, 0, "Monday")).toBe("");
  });
});
