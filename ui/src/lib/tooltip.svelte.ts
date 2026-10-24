// The one tooltip engine (design v2 `attachTips`, TrackerApp.dc.html 1138-1188). Triggers carry `data-tip` (set by
// the `tip` action, or written by hand); one delegated `mouseover` on #app finds the trigger with
// `closest('[data-tip]')`. A trigger that takes keyboard focus (`:focus-visible`) shows its tip the same way, after the
// same 450 ms, and Escape hides it. The bubble is drawn by components/Tooltip.svelte from `tipState`, which measures it
// and places it with `tipPlace` (above the trigger, flipped below when there is no room, clamped to the viewport).
//
// Variants, by attribute (as in the design):
//   data-tip-img="<src>" [+ data-tip-color]  36px image beside a bold first line (coloured), the rest under it
//   data-tip-kv                               "Title\nkey|value\nkey|value": a title over a two-column grid
//   data-tip-wl="w,l"                         a W/L bar (data-tip must still be non-empty)

/** The trigger's box at fire time, in viewport coordinates. */
export type TipAnchor = { left: number; top: number; width: number; bottom: number };

export type TipView =
  | { kind: "text"; text: string; anchor: TipAnchor }
  | { kind: "img"; text: string; img: string; color: string | null; head: string; sub: string; anchor: TipAnchor }
  | { kind: "kv"; text: string; head: string; rows: [string, string][]; anchor: TipAnchor }
  | { kind: "wl"; text: string; w: number; l: number; anchor: TipAnchor };

/** The tip on screen, or null. */
export const tipState = $state<{ tip: TipView | null }>({ tip: null });

/**
 * `overflowOnly`: the tip shows only while the trigger's first span is cut off (a visible Riot ID).
 * `ifCut`: the tip always shows, and this line is added under it only while that span is cut off (a remembered name:
 * its own tip, then the full `Name#TAG`).
 */
export type TipArg =
  | string
  | { text: string; overflowOnly: true }
  | { text: string; ifCut: string }
  /** Image variant: `img` beside the bold first line of `text` (in `color`), the other lines under it. */
  | { text: string; img: string; color?: string | null }
  /** Key/value variant: "Title\nkey|value\nkey|value". */
  | { kv: string }
  /** W/L bar variant: wins and losses. `text` is what a screen reader hears (default "{w} wins, {l} losses"). */
  | { wl: readonly [number, number] | string; text?: string };

const DELAY_MS = 450;

/** The bubble's id: the keyboard-focused element points at it with `aria-describedby` while its tip shows. */
export const TIP_ID = "tooltip";

let timer: ReturnType<typeof setTimeout> | undefined;
/** The trigger the pointer is on, whether or not its tip will show (so a re-entry inside it is a no-op). */
let current: Element | null = null;
/** The element the engine is attached to (`attachTooltips`): where a `retip` places its bubble. */
let appRoot: Element | null = null;
/**
 * The keyboard-focused element the pending or shown tip belongs to: the trigger itself, or, for one surface with many
 * targets (the RR chart), the focusable wrapper around it. Its `focusout` hides the tip, and it carries
 * `aria-describedby` while the bubble shows. Null for a tip the pointer started.
 */
let focusOwner: Element | null = null;
/** Whether `focusOwner` carries `aria-describedby` while the bubble shows: false for a surface whose own value text says it (the RR chart's `aria-valuetext`). */
let describeOwner = true;
/** The element that carries `aria-describedby` for the bubble on screen. */
let described: Element | null = null;

const idsOf = (el: Element): string[] => (el.getAttribute("aria-describedby") ?? "").split(/\s+/).filter(Boolean);

function describe(el: Element): void {
  undescribe();
  const ids = idsOf(el);
  if (!ids.includes(TIP_ID)) el.setAttribute("aria-describedby", [...ids, TIP_ID].join(" "));
  described = el;
}

/** Take the bubble's id back out, leaving any other id the element had. */
function undescribe(): void {
  if (!described) return;
  const rest = idsOf(described).filter((id) => id !== TIP_ID);
  if (rest.length > 0) described.setAttribute("aria-describedby", rest.join(" "));
  else described.removeAttribute("aria-describedby");
  described = null;
}

/** Clear the tip now and cancel a pending one. */
export function hideTip(): void {
  clearTimeout(timer);
  timer = undefined;
  current = null;
  focusOwner = null;
  describeOwner = true;
  undescribe();
  if (tipState.tip) tipState.tip = null;
}

/** The gap between the trigger and the bubble, and the bubble's minimum distance from the viewport edges. */
export const TIP_GAP = 8;
export const TIP_EDGE = 6;

/**
 * Where the bubble goes (viewport px): centred above the trigger, 8px off; below it when the top would come closer
 * than 6px to the viewport's top; `left` clamped 6px inside the viewport (`vw` = its width).
 */
export function tipPlace(a: TipAnchor, w: number, h: number, vw: number): { left: number; top: number } {
  let top = a.top - h - TIP_GAP;
  if (top < TIP_EDGE) top = a.bottom + TIP_GAP;
  const left = Math.max(TIP_EDGE, Math.min(vw - w - TIP_EDGE, a.left + a.width / 2 - w / 2));
  return { left, top };
}

/** The view for a trigger's attributes (read at fire time), or null when it must show nothing. */
export function tipView(el: Pick<Element, "getAttribute">, text: string, anchor: TipAnchor): TipView | null {
  const img = el.getAttribute("data-tip-img");
  if (img) {
    const [head = "", ...rest] = text.split("\n");
    return { kind: "img", text, img, color: el.getAttribute("data-tip-color") || null, head, sub: rest.join("\n"), anchor };
  }
  if (el.getAttribute("data-tip-kv") !== null) {
    const [head = "", ...rows] = text.split("\n");
    return {
      kind: "kv",
      text,
      head,
      rows: rows.map((r) => {
        const i = r.indexOf("|");
        return (i < 0 ? [r, ""] : [r.slice(0, i), r.slice(i + 1)]) as [string, string];
      }),
      anchor,
    };
  }
  const wlAttr = el.getAttribute("data-tip-wl");
  if (wlAttr) {
    const [w = NaN, l = NaN] = wlAttr.split(",").map(Number);
    if (!Number.isFinite(w) || !Number.isFinite(l)) return null;
    return { kind: "wl", text, w, l, anchor };
  }
  return { kind: "text", text, anchor };
}

/** The W/L bar's fill, as the design rounds it: wins / max(1, games), in whole percent. */
export const wlFill = (w: number, l: number): number => Math.round((w / Math.max(1, w + l)) * 100);

/** `mouseover` on #app: show, keep or clear the tip for whatever the pointer is now on. */
export function tipOver(target: Element | null, _root?: Element): void {
  const el = target?.closest?.("[data-tip]") ?? null;
  if (el && el === current) return;
  hideTip();
  if (!el) return;
  if (!el.getAttribute("data-tip")) return;
  current = el;
  /** The trigger's first span (the name) is cut off by its ellipsis. */
  const cut = () => {
    const s = el.querySelector("span");
    return !!s && s.scrollWidth > s.clientWidth;
  };
  // Overflow-only (a Riot ID name): the trigger's first span must actually be cut off.
  if (el.getAttribute("data-tip-ov") === "1" && !cut()) return;
  timer = setTimeout(() => {
    if (current !== el) return;
    // Read at fire time, as the design does (v8:525): a trigger whose text changed during the delay shows the
    // new string, and one that went empty or malformed shows nothing.
    let text = el.getAttribute("data-tip");
    if (!text) return;
    const more = el.getAttribute("data-tip-cut");
    if (more && cut()) text = `${text}\n${more}`;
    const r = el.getBoundingClientRect();
    const view = tipView(el, text, { left: r.left, top: r.top, width: r.width, bottom: r.bottom });
    if (!view) return;
    tipState.tip = view;
    if (focusOwner && describeOwner) describe(focusOwner);
  }, DELAY_MS);
}

/**
 * `focusin` on #app: a trigger that itself took keyboard focus (`:focus-visible`, never a mouse click's focus) shows
 * its tip as a hover would, after the same delay and placed by the same rules. A focused element that is not a
 * trigger leaves the tip alone (the `focusout` of the one before it already cleared it).
 */
export function tipFocus(target: Element | null, _root?: Element): void {
  if (!target?.hasAttribute?.("data-tip") || !target.matches(":focus-visible")) return;
  tipOver(target);
  if (current !== target) return;
  focusOwner = target;
  describeOwner = true;
  // The pointer was already on it and its bubble is up: the bubble now describes the focused element too.
  if (tipState.tip) describe(target);
}

/** `focusout` on #app: the tip a keyboard focus started goes with it. A hover's tip is not the focus's to hide. */
export function tipBlur(target: Element | null): void {
  if (target && target === focusOwner) hideTip();
}

/** `keydown`: Escape hides any open or pending tip, however it started, and does nothing else (no preventDefault). */
export function tipKey(e: Pick<KeyboardEvent, "key">): void {
  if (e.key === "Escape") hideTip();
}

/**
 * One surface with many targets (the RR chart: a single hover layer, the tip belongs to the point nearest the
 * pointer): the pointer moved to another target without leaving the surface, so no `mouseover` came. Restarts the tip
 * for `trigger` as one would — the old bubble goes, the new text shows after the delay, placed by `trigger`'s box.
 * `owner` is the keyboard-focused element behind a move that came from the keyboard (the chart's wrapper): its blur
 * hides the tip, a scroll spares the tip while it is pending, and it carries `aria-describedby` while the bubble shows
 * unless `announce` is false (the owner announces the point by its own value text).
 */
export function retip(trigger: Element, owner?: Element, announce = true): void {
  if (!appRoot) return;
  hideTip();
  tipOver(trigger);
  if (!owner || current !== trigger) return;
  focusOwner = owner;
  describeOwner = announce;
}

/**
 * Wire the engine to #app. Leaving the app, a mouse press or click, or scrolling any container hides the tip
 * (scroll does not bubble, so it is caught in the capture phase). A scroll does not cancel a keyboard focus's tip that
 * has not shown yet: moving focus scrolls its target into view, and the tip is placed when it shows. Keyboard focus
 * shows a tip and Escape hides it (`tipFocus`, `tipBlur`, `tipKey`). Returns the cleanup.
 */
export function attachTooltips(root: HTMLElement): () => void {
  appRoot = root;
  const doc = root.ownerDocument;
  const over = (e: MouseEvent) => tipOver(e.target as Element | null, root);
  const focusin = (e: FocusEvent) => tipFocus(e.target as Element | null, root);
  const focusout = (e: FocusEvent) => tipBlur(e.target as Element | null);
  const scroll = () => {
    if (focusOwner && !tipState.tip) return;
    hideTip();
  };
  root.addEventListener("mouseover", over);
  root.addEventListener("mouseleave", hideTip);
  root.addEventListener("mousedown", hideTip);
  root.addEventListener("click", hideTip);
  root.addEventListener("scroll", scroll, true);
  root.addEventListener("focusin", focusin);
  root.addEventListener("focusout", focusout);
  doc.addEventListener("keydown", tipKey);
  return () => {
    root.removeEventListener("mouseover", over);
    root.removeEventListener("mouseleave", hideTip);
    root.removeEventListener("mousedown", hideTip);
    root.removeEventListener("click", hideTip);
    root.removeEventListener("scroll", scroll, true);
    root.removeEventListener("focusin", focusin);
    root.removeEventListener("focusout", focusout);
    doc.removeEventListener("keydown", tipKey);
    if (appRoot === root) appRoot = null;
    hideTip();
  };
}

const TIP_ATTRS = ["data-tip", "data-tip-ov", "data-tip-cut", "data-tip-img", "data-tip-color", "data-tip-kv", "data-tip-wl"];

/**
 * `use:tip={text}`, or an options form: `{ text, overflowOnly: true }`, `{ text, ifCut }`, `{ text, img, color }`,
 * `{ kv }`, `{ wl: [w, l] }`. It only sets attributes; the engine does the rest. An empty text leaves an empty
 * `data-tip`, which shows nothing (and, as in the design, shadows an outer trigger).
 */
export function tip(node: Element, arg: TipArg): { update(a: TipArg): void; destroy(): void } {
  const set = (a: TipArg) => {
    const attrs: Record<string, string> = {};
    if (typeof a === "string") attrs["data-tip"] = a;
    else if ("kv" in a) {
      attrs["data-tip"] = a.kv;
      attrs["data-tip-kv"] = "";
    } else if ("wl" in a) {
      const [w, l] = typeof a.wl === "string" ? a.wl.split(",").map(Number) : a.wl;
      attrs["data-tip"] = a.text ?? `${w} wins, ${l} losses`;
      attrs["data-tip-wl"] = `${w},${l}`;
    } else {
      attrs["data-tip"] = a.text;
      if ("overflowOnly" in a) attrs["data-tip-ov"] = "1";
      if ("ifCut" in a) attrs["data-tip-cut"] = a.ifCut;
      if ("img" in a) {
        attrs["data-tip-img"] = a.img;
        if (a.color) attrs["data-tip-color"] = a.color;
      }
    }
    for (const k of TIP_ATTRS) {
      const v = attrs[k];
      if (v === undefined) node.removeAttribute(k);
      else node.setAttribute(k, v);
    }
  };
  set(arg);
  return {
    update: set,
    destroy() {
      for (const k of TIP_ATTRS) node.removeAttribute(k);
      if (current === node) hideTip();
    },
  };
}
