import { mount } from "svelte";
import type { Attachment } from "svelte/attachments";
import Tooltip from "../components/Tooltip.svelte";

/** One line, several lines (first line is the title), or nothing. */
export type TipContent = string | readonly string[] | null | undefined | false;

/** The single shared bubble's state; rendered by components/Tooltip.svelte. */
export const tipState = $state<{ lines: string[]; rect: DOMRect | null; owner: Element | null }>({
  lines: [],
  rect: null,
  owner: null,
});

/** Dismiss whatever bubble is showing. */
export function hideTip(): void {
  tipState.lines = [];
  tipState.rect = null;
  tipState.owner = null;
}

let hostMounted = false;

function ensureHost(): void {
  if (hostMounted) return;
  hostMounted = true;
  mount(Tooltip, { target: document.body });
}

function toLines(content: TipContent): string[] {
  if (!content) return [];
  return typeof content === "string" ? [content] : content.filter(Boolean);
}

/**
 * Hover/focus detail for any element: `<span {@attach tip("Account level")}>`.
 * Pointer-only on non-focusable elements; pair it with an accessible
 * description (see PlayerCard's aria-describedby) so the text is never mouse-only.
 * No wrapper element, so it never changes layout. The bubble is fixed-positioned
 * on <body> and clamped to the window, so card overflow never clips it.
 */
export function tip(content: TipContent): Attachment<HTMLElement> {
  return (el) => {
    const lines = toLines(content);
    if (lines.length === 0) return;

    const show = () => {
      ensureHost();
      tipState.lines = lines;
      tipState.rect = el.getBoundingClientRect();
      tipState.owner = el;
    };
    const hide = () => {
      if (tipState.owner === el) hideTip();
    };

    // Mouse hover and keyboard focus both show it; Escape (in Tooltip.svelte) dismisses it.
    el.addEventListener("pointerenter", show);
    el.addEventListener("pointerleave", hide);
    el.addEventListener("focus", show);
    el.addEventListener("blur", hide);
    // Content changed while hovered or focused (e.g. a "time since" tick): swap it in place.
    if (el.matches(":hover") || el === document.activeElement) show();

    return () => {
      el.removeEventListener("pointerenter", show);
      el.removeEventListener("pointerleave", hide);
      el.removeEventListener("focus", show);
      el.removeEventListener("blur", hide);
      hide();
    };
  };
}
