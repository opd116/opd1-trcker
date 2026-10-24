import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  attachTooltips,
  hideTip,
  retip,
  tip,
  TIP_ID,
  tipBlur,
  tipFocus,
  tipKey,
  tipOver,
  tipPlace,
  tipView,
  wlFill,
  tipState,
} from "./tooltip.svelte";
import { ACS_TIP, ADR_TIP, liveTip, nameTip, peakTip, PM_TIP } from "./tips";
import type { Meta, Peak } from "./types";

const rect = (left: number, top: number, width: number, height: number): DOMRectReadOnly =>
  ({ left, top, width, height, right: left + width, bottom: top + height, x: left, y: top }) as DOMRectReadOnly;

const anchor = (left: number, top: number, width: number, height: number) => ({ left, top, width, bottom: top + height });

describe("tipPlace (design v2 attachTips)", () => {
  it("centres above the trigger, 8px off", () => {
    expect(tipPlace(anchor(400, 100, 100, 20), 120, 30, 1280)).toEqual({ left: 390, top: 62 });
  });

  it("flips below when the top would come closer than 6px to the viewport", () => {
    expect(tipPlace(anchor(400, 44, 100, 20), 120, 30, 1280)).toEqual({ left: 390, top: 6 }); // exactly 6: stays above
    expect(tipPlace(anchor(400, 43, 100, 20), 120, 30, 1280)).toEqual({ left: 390, top: 71 });
  });

  it("clamps left 6px inside the viewport, whatever its width", () => {
    expect(tipPlace(anchor(0, 300, 20, 20), 200, 30, 1280).left).toBe(6);
    expect(tipPlace(anchor(1270, 300, 10, 20), 200, 30, 1280).left).toBe(1280 - 200 - 6);
    expect(tipPlace(anchor(1100, 300, 10, 20), 200, 30, 1180).left).toBe(1180 - 200 - 6);
  });
});

describe("tipView: the design's variants", () => {
  const attrs = (a: Record<string, string>) => ({ getAttribute: (k: string) => a[k] ?? null });
  const at = anchor(0, 0, 10, 10);

  it("plain text", () => {
    expect(tipView(attrs({}), "Hello", at)).toEqual({ kind: "text", text: "Hello", anchor: at });
  });

  it("data-tip-img: image beside a bold coloured first line", () => {
    expect(tipView(attrs({ "data-tip-img": "x.png", "data-tip-color": "#6ae2af" }), "Ascendant 2\n47 RR\nPeak Imm 1", at)).toEqual({
      kind: "img",
      text: "Ascendant 2\n47 RR\nPeak Imm 1",
      img: "x.png",
      color: "#6ae2af",
      head: "Ascendant 2",
      sub: "47 RR\nPeak Imm 1",
      anchor: at,
    });
    expect(tipView(attrs({ "data-tip-img": "x.png" }), "One", at)).toMatchObject({ color: null, head: "One", sub: "" });
  });

  it("data-tip-kv: a title over key|value rows", () => {
    expect(tipView(attrs({ "data-tip-kv": "" }), "Played before\nWith you|1\nAgainst you|3", at)).toMatchObject({
      kind: "kv",
      head: "Played before",
      rows: [
        ["With you", "1"],
        ["Against you", "3"],
      ],
    });
  });

  it("data-tip-wl: a W/L bar; a malformed one shows nothing", () => {
    expect(tipView(attrs({ "data-tip-wl": "31,22" }), "31 wins, 22 losses", at)).toMatchObject({ kind: "wl", w: 31, l: 22 });
    expect(tipView(attrs({ "data-tip-wl": "x,2" }), "t", at)).toBeNull();
    expect(wlFill(31, 22)).toBe(58);
    expect(wlFill(0, 0)).toBe(0);
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

  it("the options form sets the design's variant attributes, and an update to another form clears the old ones", () => {
    const n = node();
    const a = tip(n as unknown as HTMLElement, { text: "Ascendant 2\n47 RR", img: "x.png", color: "#6ae2af" });
    expect(Object.fromEntries(n.attrs)).toEqual({ "data-tip": "Ascendant 2\n47 RR", "data-tip-img": "x.png", "data-tip-color": "#6ae2af" });
    a.update({ kv: "Played before\nWith you|1" });
    expect(Object.fromEntries(n.attrs)).toEqual({ "data-tip": "Played before\nWith you|1", "data-tip-kv": "" });
    a.update({ wl: [31, 22] });
    expect(Object.fromEntries(n.attrs)).toEqual({ "data-tip": "31 wins, 22 losses", "data-tip-wl": "31,22" });
    a.update({ wl: "3,4", text: "This act" });
    expect(Object.fromEntries(n.attrs)).toEqual({ "data-tip": "This act", "data-tip-wl": "3,4" });
    a.update("plain");
    expect(Object.fromEntries(n.attrs)).toEqual({ "data-tip": "plain" });
    a.destroy();
    expect(n.attrs.size).toBe(0);
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
    expect(tipState.tip).toEqual({ kind: "text", text: "Party · likely", anchor: anchor(400, 100, 100, 20) });
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
      ownerDocument: { addEventListener: vi.fn(), removeEventListener: vi.fn() },
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
      expect(tipState.tip).toEqual({ kind: "text", text: "−15 RR · Ascendant 2 · 32 RR · Sep 29", anchor: anchor(500, 120, 20, 28) });
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

  it("reads data-tip at fire time: text changed during the delay shows the new string (v8:525)", () => {
    const attrs = { "data-tip": "2h" };
    over(trigger(attrs, rect(400, 100, 100, 20)));
    vi.advanceTimersByTime(200);
    attrs["data-tip"] = "3h";
    vi.advanceTimersByTime(250);
    expect(tipState.tip?.text).toBe("3h");
  });

  it("skips the tip when data-tip went empty during the delay", () => {
    const attrs = { "data-tip": "2h" };
    over(trigger(attrs, rect(400, 100, 100, 20)));
    vi.advanceTimersByTime(200);
    attrs["data-tip"] = "";
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

// ── keyboard: focus shows, focusout and Escape hide, aria-describedby (spec §4.1) ──
describe("keyboard focus", () => {
  const root = { getBoundingClientRect: () => rect(0, 0, 1000, 800) } as unknown as Element;

  /** An element with real attribute storage; `focusVisible` is what `:focus-visible` answers (a mouse click's focus: false). */
  function el(attrs: Record<string, string>, r: DOMRectReadOnly = rect(400, 100, 100, 20), focusVisible = true) {
    const a = { ...attrs };
    const node: Record<string, unknown> = {
      hasAttribute: (k: string) => k in a,
      getAttribute: (k: string) => a[k] ?? null,
      setAttribute: (k: string, v: string) => void (a[k] = v),
      removeAttribute: (k: string) => void delete a[k],
      matches: (sel: string) => sel === ":focus-visible" && focusVisible,
      getBoundingClientRect: () => r,
      querySelector: () => null,
    };
    node.closest = (sel: string) => (sel === "[data-tip]" && "data-tip" in a ? node : null);
    return { node: node as unknown as Element, attrs: a };
  }

  beforeEach(() => {
    vi.useFakeTimers();
    hideTip();
  });
  afterEach(() => {
    hideTip();
    vi.useRealTimers();
  });

  it("a trigger that takes :focus-visible shows its tip after 450 ms, placed as a hover would", () => {
    const { node } = el({ "data-tip": "Party · likely" });
    tipFocus(node, root);
    vi.advanceTimersByTime(449);
    expect(tipState.tip).toBeNull();
    vi.advanceTimersByTime(1);
    expect(tipState.tip).toEqual({ kind: "text", text: "Party · likely", anchor: anchor(400, 100, 100, 20) });
  });

  it("anchors to the focused element's box, like a hover's", () => {
    const { node } = el({ "data-tip": "Low" }, rect(400, 700, 100, 20));
    tipFocus(node, root);
    vi.advanceTimersByTime(450);
    expect(tipState.tip).toEqual({ kind: "text", text: "Low", anchor: anchor(400, 700, 100, 20) });
  });

  it("a focus a mouse click gave (not :focus-visible) shows nothing and leaves a hover's tip alone", () => {
    const hovered = el({ "data-tip": "Hover" }, rect(100, 100, 40, 20));
    tipOver(hovered.node, root);
    vi.advanceTimersByTime(450);
    expect(tipState.tip?.text).toBe("Hover");

    const clicked = el({ "data-tip": "Click" }, rect(300, 100, 40, 20), false);
    tipFocus(clicked.node, root);
    vi.advanceTimersByTime(1000);
    expect(tipState.tip?.text).toBe("Hover");

    hideTip();
    tipFocus(clicked.node, root);
    vi.advanceTimersByTime(1000);
    expect(tipState.tip).toBeNull();
  });

  it("only the focused element itself counts: focus on something inside a trigger, or on a non-trigger, shows nothing", () => {
    const trigger = el({ "data-tip": "Outer" });
    const inner = el({}); // focusable, no data-tip of its own
    (inner.node as unknown as { closest: () => Element }).closest = () => trigger.node;
    tipFocus(inner.node, root);
    tipFocus(null, root);
    vi.advanceTimersByTime(1000);
    expect(tipState.tip).toBeNull();
  });

  it("an empty tip shows nothing; an overflow-only one only while its name is cut", () => {
    tipFocus(el({ "data-tip": "" }).node, root);
    vi.advanceTimersByTime(1000);
    expect(tipState.tip).toBeNull();

    const fits = el({ "data-tip": "Fahad#OPD1", "data-tip-ov": "1" });
    (fits.node as unknown as { querySelector: () => unknown }).querySelector = () => ({ scrollWidth: 60, clientWidth: 60 });
    tipFocus(fits.node, root);
    vi.advanceTimersByTime(1000);
    expect(tipState.tip).toBeNull();
  });

  it("focusout hides the tip its focus started, pending or shown", () => {
    const a = el({ "data-tip": "A" });
    tipFocus(a.node, root);
    vi.advanceTimersByTime(200);
    tipBlur(a.node);
    vi.advanceTimersByTime(1000);
    expect(tipState.tip, "pending: cancelled").toBeNull();

    tipFocus(a.node, root);
    vi.advanceTimersByTime(450);
    expect(tipState.tip?.text).toBe("A");
    tipBlur(a.node);
    expect(tipState.tip, "shown: gone at once").toBeNull();
  });

  it("focusout of some other element leaves a hover's tip alone; tabbing to another trigger swaps the tip", () => {
    const hovered = el({ "data-tip": "Hover" }, rect(100, 100, 40, 20));
    const other = el({ "data-tip": "Other" }, rect(600, 100, 40, 20));
    tipOver(hovered.node, root);
    vi.advanceTimersByTime(450);
    tipBlur(other.node);
    expect(tipState.tip?.text, "not the focus's tip").toBe("Hover");

    const a = el({ "data-tip": "A" }, rect(100, 300, 40, 20));
    const b = el({ "data-tip": "B" }, rect(300, 300, 40, 20));
    tipFocus(a.node, root);
    vi.advanceTimersByTime(450);
    expect(tipState.tip?.text).toBe("A");
    tipBlur(a.node); // Tab: focusout of the old one, then focusin of the new one
    tipFocus(b.node, root);
    expect(tipState.tip).toBeNull();
    vi.advanceTimersByTime(450);
    expect(tipState.tip?.text).toBe("B");
  });

  it("Escape hides a pending, a hover's and a keyboard tip, and nothing else; no other key does", () => {
    const a = el({ "data-tip": "A" });
    tipOver(a.node, root);
    vi.advanceTimersByTime(450);
    for (const key of ["Enter", " ", "Tab", "ArrowLeft", "Esc"]) tipKey({ key });
    expect(tipState.tip?.text, "other keys").toBe("A");
    tipKey({ key: "Escape" });
    expect(tipState.tip, "hover's, shown").toBeNull();

    tipFocus(a.node, root);
    vi.advanceTimersByTime(200);
    tipKey({ key: "Escape" });
    vi.advanceTimersByTime(1000);
    expect(tipState.tip, "keyboard, pending").toBeNull();

    tipFocus(a.node, root);
    vi.advanceTimersByTime(450);
    expect(tipState.tip?.text).toBe("A");
    tipKey({ key: "Escape" });
    expect(tipState.tip, "keyboard, shown").toBeNull();
    tipKey({ key: "Escape" }); // nothing open: nothing happens
    expect(tipState.tip).toBeNull();
  });

  it("the focused trigger carries aria-describedby while its bubble shows, and not before or after", () => {
    const a = el({ "data-tip": "A" });
    tipFocus(a.node, root);
    expect(a.attrs["aria-describedby"], "pending").toBeUndefined();
    vi.advanceTimersByTime(450);
    expect(a.attrs["aria-describedby"]).toBe(TIP_ID);
    tipBlur(a.node);
    expect(a.attrs["aria-describedby"], "after focusout").toBeUndefined();

    tipFocus(a.node, root);
    vi.advanceTimersByTime(450);
    tipKey({ key: "Escape" });
    expect(a.attrs["aria-describedby"], "after Escape").toBeUndefined();
  });

  it("aria-describedby keeps another id the element had, and leaves it when the tip goes", () => {
    const a = el({ "data-tip": "A", "aria-describedby": "hint" });
    tipFocus(a.node, root);
    vi.advanceTimersByTime(450);
    expect(a.attrs["aria-describedby"]).toBe(`hint ${TIP_ID}`);
    hideTip();
    expect(a.attrs["aria-describedby"]).toBe("hint");
  });

  it("a hover's tip describes nothing; pointing at a trigger that already has the focus makes the bubble describe it", () => {
    const a = el({ "data-tip": "A" });
    tipOver(a.node, root);
    vi.advanceTimersByTime(450);
    expect(a.attrs["aria-describedby"]).toBeUndefined();
    tipFocus(a.node, root); // the pointer is on it and its bubble is up: the focus adopts it
    expect(tipState.tip?.text).toBe("A");
    expect(a.attrs["aria-describedby"]).toBe(TIP_ID);
    tipBlur(a.node);
    expect(tipState.tip).toBeNull();
    expect(a.attrs["aria-describedby"]).toBeUndefined();
  });

  it("the pointer taking over from a focused trigger moves the description with the bubble", () => {
    const a = el({ "data-tip": "A" }, rect(100, 100, 40, 20));
    const b = el({ "data-tip": "B" }, rect(300, 100, 40, 20));
    tipFocus(a.node, root);
    vi.advanceTimersByTime(450);
    tipOver(b.node, root);
    expect(a.attrs["aria-describedby"]).toBeUndefined();
    vi.advanceTimersByTime(450);
    expect(tipState.tip?.text).toBe("B");
    expect(b.attrs["aria-describedby"], "a hover's tip describes nothing").toBeUndefined();
    tipBlur(a.node); // the old focus owns nothing now
    expect(tipState.tip?.text).toBe("B");
  });

  it("the tip action's destroy takes the description off the element", () => {
    const a = el({ "data-tip": "A" });
    tipFocus(a.node, root);
    vi.advanceTimersByTime(450);
    // The `tip` action's own cleanup: a trigger removed while its tip is up.
    const action = tip(a.node, "A");
    action.destroy();
    expect(tipState.tip).toBeNull();
    expect(a.attrs["aria-describedby"]).toBeUndefined();
  });

  describe("retip with an owner: one surface, the focus is on its wrapper (the RR chart)", () => {
    const fakeRoot = {
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      ownerDocument: { addEventListener: vi.fn(), removeEventListener: vi.fn() },
      getBoundingClientRect: () => rect(0, 0, 1000, 800),
    } as unknown as HTMLElement;

    it("the wrapper carries aria-describedby while the bubble shows, and its blur hides the tip", () => {
      const detach = attachTooltips(fakeRoot);
      const anchor = el({ "data-tip": "+18 RR · Ascendant 2 · 47 RR · Sep 28" });
      const wrapper = el({ role: "slider" });
      retip(anchor.node, wrapper.node);
      expect(wrapper.attrs["aria-describedby"], "pending").toBeUndefined();
      vi.advanceTimersByTime(450);
      expect(tipState.tip?.text).toBe("+18 RR · Ascendant 2 · 47 RR · Sep 28");
      expect(wrapper.attrs["aria-describedby"]).toBe(TIP_ID);
      expect(anchor.attrs["aria-describedby"]).toBeUndefined();

      // The next arrow key: the old bubble goes, the description with it, and comes back with the new one.
      anchor.attrs["data-tip"] = "−15 RR · Ascendant 2 · 32 RR · Sep 29";
      retip(anchor.node, wrapper.node);
      expect(tipState.tip).toBeNull();
      expect(wrapper.attrs["aria-describedby"]).toBeUndefined();
      vi.advanceTimersByTime(450);
      expect(tipState.tip?.text).toBe("−15 RR · Ascendant 2 · 32 RR · Sep 29");
      expect(wrapper.attrs["aria-describedby"]).toBe(TIP_ID);

      tipBlur(anchor.node); // the anchor is not the focus
      expect(tipState.tip).not.toBeNull();
      tipBlur(wrapper.node);
      expect(tipState.tip).toBeNull();
      expect(wrapper.attrs["aria-describedby"]).toBeUndefined();
      detach();
    });

    it("with `announce` false (the chart: its aria-valuetext says the point) the owner still hides the tip on blur but carries no aria-describedby", () => {
      const detach = attachTooltips(fakeRoot);
      const anchor = el({ "data-tip": "+18 RR · Ascendant 2 · 47 RR · Sep 28" });
      const wrapper = el({ role: "slider" });
      retip(anchor.node, wrapper.node, false);
      vi.advanceTimersByTime(450);
      expect(tipState.tip?.text).toBe("+18 RR · Ascendant 2 · 47 RR · Sep 28");
      expect(wrapper.attrs["aria-describedby"]).toBeUndefined();
      expect(anchor.attrs["aria-describedby"]).toBeUndefined();
      tipBlur(wrapper.node);
      expect(tipState.tip).toBeNull();

      // The flag does not stick: the next owner that announces is described again.
      retip(anchor.node, wrapper.node);
      vi.advanceTimersByTime(450);
      expect(wrapper.attrs["aria-describedby"]).toBe(TIP_ID);
      hideTip();
      detach();
    });

    it("a pointer move's retip (no owner) is not the focus's: the wrapper's blur leaves it", () => {
      const detach = attachTooltips(fakeRoot);
      const anchor = el({ "data-tip": "P" });
      const wrapper = el({ role: "slider" });
      retip(anchor.node);
      vi.advanceTimersByTime(450);
      tipBlur(wrapper.node);
      expect(tipState.tip?.text).toBe("P");
      expect(wrapper.attrs["aria-describedby"]).toBeUndefined();
      detach();
    });
  });

  describe("attachTooltips wiring", () => {
    type Handler = (e: unknown) => void;
    function mount() {
      const onRoot = new Map<string, Handler>();
      const onDoc = new Map<string, Handler>();
      const rootEl = {
        addEventListener: (t: string, f: Handler) => void onRoot.set(t, f),
        removeEventListener: (t: string, f: Handler) => void (onRoot.get(t) === f && onRoot.delete(t)),
        ownerDocument: {
          addEventListener: (t: string, f: Handler) => void onDoc.set(t, f),
          removeEventListener: (t: string, f: Handler) => void (onDoc.get(t) === f && onDoc.delete(t)),
        },
        getBoundingClientRect: () => rect(0, 0, 1000, 800),
      } as unknown as HTMLElement;
      return { onRoot, onDoc, detach: attachTooltips(rootEl) };
    }

    it("focusin and focusout on #app and keydown on the document; the cleanup removes them", () => {
      const { onRoot, onDoc, detach } = mount();
      expect([...onRoot.keys()].sort()).toEqual(["click", "focusin", "focusout", "mousedown", "mouseleave", "mouseover", "scroll"]);
      expect([...onDoc.keys()]).toEqual(["keydown"]);

      const a = el({ "data-tip": "A" });
      onRoot.get("focusin")!({ target: a.node });
      vi.advanceTimersByTime(450);
      expect(tipState.tip?.text).toBe("A");
      expect(a.attrs["aria-describedby"]).toBe(TIP_ID);
      onDoc.get("keydown")!({ key: "Escape" });
      expect(tipState.tip).toBeNull();

      onRoot.get("focusin")!({ target: a.node });
      vi.advanceTimersByTime(450);
      onRoot.get("focusout")!({ target: a.node });
      expect(tipState.tip).toBeNull();

      detach();
      expect(onRoot.size).toBe(0);
      expect(onDoc.size).toBe(0);
    });

    it("moving focus scrolls its target into view: that scroll does not cancel a keyboard tip still pending, but hides a shown one", () => {
      const { onRoot, detach } = mount();
      const a = el({ "data-tip": "A" });
      onRoot.get("focusin")!({ target: a.node });
      vi.advanceTimersByTime(100);
      onRoot.get("scroll")!({});
      vi.advanceTimersByTime(350);
      expect(tipState.tip?.text, "pending focus tip survived the scroll").toBe("A");
      onRoot.get("scroll")!({});
      expect(tipState.tip, "a shown one goes").toBeNull();
      detach();
    });

    it("a scroll still cancels a hover's pending tip", () => {
      const { onRoot, detach } = mount();
      const a = el({ "data-tip": "A" });
      onRoot.get("mouseover")!({ target: a.node });
      vi.advanceTimersByTime(100);
      onRoot.get("scroll")!({});
      vi.advanceTimersByTime(1000);
      expect(tipState.tip).toBeNull();
      detach();
    });

    it("a click hides a keyboard tip too (Enter or Space on a focused trigger clicks it)", () => {
      const { onRoot, detach } = mount();
      const a = el({ "data-tip": "A" });
      onRoot.get("focusin")!({ target: a.node });
      vi.advanceTimersByTime(450);
      onRoot.get("click")!({});
      expect(tipState.tip).toBeNull();
      expect(a.attrs["aria-describedby"]).toBeUndefined();
      detach();
    });
  });
});

// ── tips.ts catalogue (spec §4.2) ───────────────────────────────────────────────
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

// ── Player Details and Match Details (spec §4.2, Task 18) ────────────────────────
describe("profile and scoreboard tips", () => {
  const BREACH = "5f8d3a7f-467b-97f3-062c-13acf203c006";
  const dm: Meta = { agents: { [BREACH]: { name: "Breach" } }, maps: {}, tiers: { "21": { name: "ASCENDANT 1", color: "2a9d8fff" } } };

  it("Top 3 live icons: `In this match · Breach`; the agent part is dropped when unknown (spec §5.8)", () => {
    expect(liveTip(dm, BREACH)).toBe("In this match · Breach");
    expect(liveTip(dm, null)).toBe("In this match");
    expect(liveTip(null, BREACH)).toBe("In this match");
    expect(liveTip(dm, "not-in-meta")).toBe("In this match");
  });

  it("scoreboard column headers: ACS, +/−, ADR", () => {
    expect(ACS_TIP).toBe("Average combat score");
    expect(PM_TIP).toBe("Kills minus deaths");
    expect(ADR_TIP).toBe("Average damage per round");
  });
});
