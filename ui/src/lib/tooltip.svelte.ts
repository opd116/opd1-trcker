// The one tooltip engine (spec §4.1, v8:516-532). Triggers carry `data-tip` (set by the `tip` action);
// one delegated `mouseover` on #app finds the trigger with `closest('[data-tip]')`. A trigger that takes keyboard focus
// (`:focus-visible`) shows its tip the same way, and Escape hides it. The bubble is drawn by components/Tooltip.svelte
// from `tipState`.
import { count } from "./format";

export type TipView = { text: string; x: number; top: number; transform: string };

/** The tip on screen, or null. */
export const tipState = $state<{ tip: TipView | null }>({ tip: null });

/**
 * `overflowOnly`: the tip shows only while the trigger's first span is cut off (a visible Riot ID).
 * `ifCut`: the tip always shows, and this line is added under it only while that span is cut off (a remembered name:
 * its own tip, then the full `Name#TAG`).
 */
export type TipArg = string | { text: string; overflowOnly: true } | { text: string; ifCut: string };

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

/** Where the bubble goes, relative to #app (`r` = trigger rect, `R` = app rect). */
export function tipPosition(r: DOMRectReadOnly, R: DOMRectReadOnly): { x: number; top: number; transform: string } {
  const x = Math.max(136, Math.min(864, r.left + r.width / 2 - R.left));
  const below = r.bottom - R.top + 8;
  return below > 720
    ? { x, top: r.top - R.top - 8, transform: "translate(-50%,-100%)" }
    : { x, top: below, transform: "translate(-50%,0)" };
}

/**
 * `wl:31:22:53` → wins, losses, games. `wl:31:22:53|This act · Monday` also carries the text of the second row (My
 * Stats' weekday and time-of-day cells name their slice there); without it the row reads `This act`.
 */
export function parseWl(text: string): { w: number; l: number; g: number; scope?: string } | null {
  const m = /^wl:(\d+):(\d+):(\d+)(?:\|([^\n]+))?$/.exec(text);
  if (!m) return null;
  const wl = { w: Number(m[1]), l: Number(m[2]), g: Number(m[3]) };
  return m[4] === undefined ? wl : { ...wl, scope: m[4] };
}

/** The spans of the structured W/L row, in order: `31W`, `·`, `22L`, `·`, `53 games`. */
export function wlRow(wl: { w: number; l: number; g: number }): string[] {
  return [`${wl.w}W`, "·", `${wl.l}L`, "·", count(wl.g, "game")];
}

/** `mouseover` on #app: show, keep or clear the tip for whatever the pointer is now on. */
export function tipOver(target: Element | null, root: Element): void {
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
    if (!text || (text.startsWith("wl:") && !parseWl(text))) return;
    const more = el.getAttribute("data-tip-cut");
    if (more && cut()) text = `${text}\n${more}`;
    tipState.tip = { text, ...tipPosition(el.getBoundingClientRect(), root.getBoundingClientRect()) };
    if (focusOwner && describeOwner) describe(focusOwner);
  }, DELAY_MS);
}

/**
 * `focusin` on #app: a trigger that itself took keyboard focus (`:focus-visible`, never a mouse click's focus) shows
 * its tip as a hover would, after the same delay and placed by the same rules. A focused element that is not a
 * trigger leaves the tip alone (the `focusout` of the one before it already cleared it).
 */
export function tipFocus(target: Element | null, root: Element): void {
  if (!target?.hasAttribute?.("data-tip") || !target.matches(":focus-visible")) return;
  tipOver(target, root);
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
  tipOver(trigger, appRoot);
  if (!owner || current !== trigger) return;
  focusOwner = owner;
  describeOwner = announce;
}

/**
 * Wire the engine to #app. Leaving the app, a click, or scrolling a Details/Match container hides the tip
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
  root.addEventListener("click", hideTip);
  root.addEventListener("scroll", scroll, true);
  root.addEventListener("focusin", focusin);
  root.addEventListener("focusout", focusout);
  doc.addEventListener("keydown", tipKey);
  return () => {
    root.removeEventListener("mouseover", over);
    root.removeEventListener("mouseleave", hideTip);
    root.removeEventListener("click", hideTip);
    root.removeEventListener("scroll", scroll, true);
    root.removeEventListener("focusin", focusin);
    root.removeEventListener("focusout", focusout);
    doc.removeEventListener("keydown", tipKey);
    if (appRoot === root) appRoot = null;
    hideTip();
  };
}

/**
 * `use:tip={text}`, `use:tip={{ text, overflowOnly: true }}` or `use:tip={{ text, ifCut }}`. It only sets attributes;
 * the engine does the rest. An empty text leaves an empty `data-tip`, which shows nothing (and, as in the design,
 * shadows an outer trigger).
 */
export function tip(node: Element, arg: TipArg): { update(a: TipArg): void; destroy(): void } {
  const set = (a: TipArg) => {
    node.setAttribute("data-tip", typeof a === "string" ? a : a.text);
    if (typeof a !== "string" && "overflowOnly" in a) node.setAttribute("data-tip-ov", "1");
    else node.removeAttribute("data-tip-ov");
    if (typeof a !== "string" && "ifCut" in a) node.setAttribute("data-tip-cut", a.ifCut);
    else node.removeAttribute("data-tip-cut");
  };
  set(arg);
  return {
    update: set,
    destroy() {
      node.removeAttribute("data-tip");
      node.removeAttribute("data-tip-ov");
      node.removeAttribute("data-tip-cut");
      if (current === node) hideTip();
    },
  };
}
