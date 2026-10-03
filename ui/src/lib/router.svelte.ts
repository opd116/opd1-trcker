// Routes and navigation (spec §2.4). One `$state` object; every navigation goes through `go`.
import { openDetails } from "./boot";
import type { MatchFrom } from "./details";
import { hideTip } from "./tooltip.svelte";
import type { Stage } from "./types";

/** Where Back goes from a Match: the Player Details it came from (with its "Show more" state), or My Stats. */
export type MatchBack = { puuid: string; more: boolean } | "mystats";

export type Route =
  | { name: "live" }
  | { name: "mystats" }
  | { name: "details"; puuid: string; more: boolean }
  | {
      name: "match";
      matchId: string;
      focusPuuid: string;
      from: MatchFrom;
      back: MatchBack;
    };

/** The tab a route sits under: My Stats, and a Match opened from it, are My Stats; everything else is Live (spec §2.2). */
export function activeTab(r: Route): "live" | "mystats" {
  return r.name === "mystats" || (r.name === "match" && r.back === "mystats") ? "mystats" : "live";
}

/** Starts on Live; the first live state may move it to My Stats (`startPage`). */
export const router = $state<{ route: Route }>({ route: { name: "live" } });

/** The Details/Match/My Stats scroll container (registered by App.svelte). Live has none. */
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

/** Back: Match → the page it was opened from (Details with "Show more" kept, or My Stats); Details → Live. */
export function back(): void {
  const r = router.route;
  if (r.name === "match") {
    go(r.back === "mystats" ? { name: "mystats" } : { name: "details", puuid: r.back.puuid, more: r.back.more });
  } else if (r.name === "details") go({ name: "live" });
}

/** "Show more" on Details: keeps the scroll position, so it is not a `go`. */
export function showMore(): void {
  const r = router.route;
  if (r.name === "details") router.route = { ...r, more: true };
}

let started = false;

/**
 * The first live state ever applied picks the start page, once: VALORANT closed → My Stats (the saved data is all
 * there is to show); anything else → Live. It moves only a user who has not navigated yet.
 */
export function startPage(stage: Stage): void {
  if (started) return;
  started = true;
  if (stage === "closed" && router.route.name === "live") go({ name: "mystats" });
}

/** A new live stage: entering Agent Select always brings the user back to Live. */
export function onStage(prev: Stage | null, next: Stage): void {
  if (next === "pregame" && prev !== "pregame") go({ name: "live" });
}
