// The one tooltip engine (spec §4.1, v8:516-532). Triggers carry `data-tip` (set by the `tip` action);
// one delegated `mouseover` on #app finds the trigger with `closest('[data-tip]')`. The bubble is
// drawn by components/Tooltip.svelte from `tipState`.
import { count } from "./format";

export type TipView = { text: string; x: number; top: number; transform: string };

/** The tip on screen, or null. */
export const tipState = $state<{ tip: TipView | null }>({ tip: null });

export type TipArg = string | { text: string; overflowOnly: true };

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
  if (el.getAttribute("data-tip-ov") === "1") {
    // Overflow-only (a Riot ID name): the trigger's first span must actually be cut off.
    const s = el.querySelector("span");
    if (!s || s.scrollWidth <= s.clientWidth) return;
  }
  timer = setTimeout(() => {
    if (current !== el) return;
    // Read at fire time, as the design does (v8:525): a trigger whose text changed during the delay shows the
    // new string, and one that went empty or malformed shows nothing.
    const text = el.getAttribute("data-tip");
    if (!text || (text.startsWith("wl:") && !parseWl(text))) return;
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
 * `use:tip={text}` or `use:tip={{ text, overflowOnly: true }}`. It only sets attributes; the engine does the rest.
 * An empty text leaves an empty `data-tip`, which shows nothing (and, as in the design, shadows an outer trigger).
 */
export function tip(node: HTMLElement, arg: TipArg): { update(a: TipArg): void; destroy(): void } {
  const set = (a: TipArg) => {
    node.setAttribute("data-tip", typeof a === "string" ? a : a.text);
    if (typeof a !== "string" && a.overflowOnly) node.setAttribute("data-tip-ov", "1");
    else node.removeAttribute("data-tip-ov");
  };
  set(arg);
  return {
    update: set,
    destroy() {
      node.removeAttribute("data-tip");
      node.removeAttribute("data-tip-ov");
      if (current === node) hideTip();
    },
  };
}
