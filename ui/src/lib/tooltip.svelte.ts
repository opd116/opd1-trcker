// The one tooltip engine (spec §4.1, v8:516-532). Triggers carry `data-tip` (set by the `tip` action);
// one delegated `mouseover` on #app finds the trigger with `closest('[data-tip]')`. The bubble is
// drawn by components/Tooltip.svelte from `tipState`.
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

let timer: ReturnType<typeof setTimeout> | undefined;
/** The trigger the pointer is on, whether or not its tip will show (so a re-entry inside it is a no-op). */
let current: Element | null = null;

/** Clear the tip now and cancel a pending one. */
export function hideTip(): void {
  clearTimeout(timer);
  timer = undefined;
  current = null;
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

/** `wl:31:22:53` → wins, losses, games. */
export function parseWl(text: string): { w: number; l: number; g: number } | null {
  const m = /^wl:(\d+):(\d+):(\d+)$/.exec(text);
  return m ? { w: Number(m[1]), l: Number(m[2]), g: Number(m[3]) } : null;
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
  }, DELAY_MS);
}

/**
 * Wire the engine to #app. Leaving the app, a click, or scrolling a Details/Match container hides the tip
 * (scroll does not bubble, so it is caught in the capture phase). Returns the cleanup.
 */
export function attachTooltips(root: HTMLElement): () => void {
  const over = (e: MouseEvent) => tipOver(e.target as Element | null, root);
  root.addEventListener("mouseover", over);
  root.addEventListener("mouseleave", hideTip);
  root.addEventListener("click", hideTip);
  root.addEventListener("scroll", hideTip, true);
  return () => {
    root.removeEventListener("mouseover", over);
    root.removeEventListener("mouseleave", hideTip);
    root.removeEventListener("click", hideTip);
    root.removeEventListener("scroll", hideTip, true);
    hideTip();
  };
}

/**
 * `use:tip={text}`, `use:tip={{ text, overflowOnly: true }}` or `use:tip={{ text, ifCut }}`. It only sets attributes;
 * the engine does the rest. An empty text leaves an empty `data-tip`, which shows nothing (and, as in the design,
 * shadows an outer trigger).
 */
export function tip(node: HTMLElement, arg: TipArg): { update(a: TipArg): void; destroy(): void } {
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
