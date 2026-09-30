// Routes and navigation (spec §2.4). One `$state` object; every navigation goes through `go`.
import { openDetails } from "./boot";
import type { MatchFrom } from "./details";
import { hideTip } from "./tooltip.svelte";
import type { Stage } from "./types";

export type Route =
  | { name: "live" }
  | { name: "details"; puuid: string; more: boolean }
  | {
      name: "match";
      matchId: string;
      focusPuuid: string;
      from: MatchFrom;
      /** The Details page Back returns to, with its "Show more" state. */
      back: { puuid: string; more: boolean };
    };

/** Starts on Live (R71: until My Stats exists). */
export const router = $state<{ route: Route }>({ route: { name: "live" } });

/** The Details/Match scroll container (registered by App.svelte). Live has none. */
let scroller: HTMLElement | null = null;

/** `use:scrollHost` on the scroll container, so `go` can send it back to the top. */
export function scrollHost(node: HTMLElement): { destroy(): void } {
  scroller = node;
  return {
    destroy() {
      if (scroller === node) scroller = null;
    },
  };
}

/**
 * Navigate: clears the tooltip and resets the scroll container. Going to Live also forgets the open Player Details
 * (a Match page and Back keep them): a reopen then starts from the skeleton, never from an older answer.
 */
export function go(r: Route): void {
  hideTip();
  if (scroller) scroller.scrollTop = 0;
  if (r.name === "live") openDetails.close();
  router.route = r;
}

/** Back: Match → Details (same player, "Show more" kept); Details → Live. */
export function back(): void {
  const r = router.route;
  if (r.name === "match") go({ name: "details", puuid: r.back.puuid, more: r.back.more });
  else if (r.name === "details") go({ name: "live" });
}

/** "Show more" on Details: keeps the scroll position, so it is not a `go`. */
export function showMore(): void {
  const r = router.route;
  if (r.name === "details") router.route = { ...r, more: true };
}

/** A new live stage: entering Agent Select always brings the user back to Live. */
export function onStage(prev: Stage | null, next: Stage): void {
  if (next === "pregame" && prev !== "pregame") go({ name: "live" });
}
