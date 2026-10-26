// Navigation (design v2): one `$state` object. The nav rail picks a screen (`go`); the profile drawer opens over the
// current screen (`openProfile` / `closeProfile`); a match scoreboard is the History screen with a match id
// (`openMatch` / `backFromMatch`); glance mode is a flag (`setGlance`).
//
// The router also owns the data lifecycle of the two "open page" slots in boot.ts, so screens only read them:
//   openProfile → `openDetails.open(puuid, player_details)`;  closeProfile → `openDetails.close()` + `close_details`
//   openMatch   → `openMatch.open(matchId, match_details)`;   leaving it   → `openMatch.close()` + `close_match`
import { closeDetails, closeMatch, getMatchDetails, getPlayerDetails, setGlance as setGlanceWindow } from "./api";
import { DETAILS_RETRY_MS, DETAILS_TIMEOUT_MS, openDetails, openMatch as matchSlot } from "./boot";
import { emptyMatch, settledDetails } from "./details";
import { hideTip } from "./tooltip.svelte";
import { cardOpenGuard } from "./clickGuard";
import type { PlayerCard, Stage } from "./types";
import { reportError } from "./uilog";

export type Screen = "live" | "saved" | "history" | "comp" | "skins" | "ascii" | "settings" | "about";
/** Where a scoreboard was opened from: decides where Back goes (`backFromMatch`). */
export type MatchFrom = "history" | "recap" | "lastMatch" | "encounter" | "profile" | "comp";
/** The profile a scoreboard was opened from (an encounter row or a recent match in the drawer), and the screen under it. */
export interface EncFrom {
  puuid: string;
  screen: Screen;
}

export const SCREENS: readonly Screen[] = ["live", "saved", "history", "comp", "skins", "ascii", "settings", "about"];

export const router = $state<{
  screen: Screen;
  /** The open scoreboard (shown on the History screen), or null for the History list. */
  matchId: string | null;
  /** The player whose perspective the scoreboard takes (`match_details`' `focusPuuid`). */
  matchFocus: string | null;
  matchFrom: MatchFrom | null;
  encFrom: EncFrom | null;
  /** The puuid of the open profile drawer. */
  profile: string | null;
  /** The last profile opened (the prototype keeps its row marked after the drawer closes). */
  lastProfile: string | null;
  glance: boolean;
}>({
  screen: "live",
  matchId: null,
  matchFocus: null,
  matchFrom: null,
  encFrom: null,
  profile: null,
  lastProfile: null,
  glance: false,
});

// ── scroll ───────────────────────────────────────────────────────────────────────
/** The scroll container of the screen on display (`use:scrollHost`). */
let scroller: HTMLElement | null = null;
/** How far each list screen was scrolled when the user left it for a scoreboard. Keyed by screen. */
const savedScroll = new Map<Screen, number>();
/** What the next scroll container to mount is scrolled to (Back sets it; everything else starts at the top). */
let restoreTo = 0;

/**
 * `use:scrollHost` on a screen's own scroll container, so `go` can reset it and Back from a scoreboard can put the
 * list where it was. App keys each screen, so the container is new for every page and the restore happens when it
 * mounts; a position the content is still too short for is tried once more on the next frame.
 */
export function scrollHost(node: HTMLElement): { destroy(): void } {
  scroller = node;
  const y = restoreTo;
  restoreTo = 0;
  if (y > 0) {
    node.scrollTop = y;
    if (node.scrollTop !== y && typeof requestAnimationFrame === "function") {
      requestAnimationFrame(() => {
        if (scroller === node) node.scrollTop = y;
      });
    }
  }
  return {
    destroy() {
      if (scroller === node) scroller = null;
    },
  };
}

// ── profile drawer ───────────────────────────────────────────────────────────────
/** The element that opened the drawer: focus goes back to it when the drawer closes. */
let profileTrigger: HTMLElement | null = null;

/** The live card of a player, for the drawer's settled (failed) state. stores.ts registers it (no import cycle). */
let liveCardOf: (puuid: string) => PlayerCard | undefined = () => undefined;
export function setLiveCardLookup(fn: (puuid: string) => PlayerCard | undefined): void {
  liveCardOf = fn;
}

function activeElement(): HTMLElement | null {
  if (typeof document === "undefined") return null;
  const el = document.activeElement;
  return el instanceof HTMLElement && el !== document.body ? el : null;
}

/**
 * Open the drawer on `puuid` (and fetch `player_details` into `openDetails`). `trigger` gets the focus back when it
 * closes; omitted, the element focused now does. Opening another player while one is open swaps the drawer's player
 * (Rust replaces its open Details on the next `player_details`, so no `close_details` in between).
 */
export function openProfile(puuid: string, trigger?: HTMLElement | null): void {
  hideTip();
  cardOpenGuard.arm();
  profileTrigger = trigger === undefined ? activeElement() : trigger;
  if (router.profile === puuid) return;
  router.profile = puuid;
  router.lastProfile = puuid;
  openDetails
    .open(puuid, getPlayerDetails, {
      retryMs: DETAILS_RETRY_MS,
      timeoutMs: DETAILS_TIMEOUT_MS,
      settle: (p) => settledDetails(p, liveCardOf(p)),
    })
    .catch((err: unknown) => reportError("details", err));
}

/** Close the drawer without moving focus (a navigation replaces the page under it). */
function dropProfile(): void {
  const p = router.profile;
  if (p === null) return;
  router.profile = null;
  profileTrigger = null;
  openDetails.close();
  closeDetails(p).catch((err: unknown) => reportError("details close", err));
}

/** Close the drawer; focus returns to the element that opened it (when it is still in the page). */
export function closeProfile(): void {
  if (router.profile === null) return;
  hideTip();
  const el = profileTrigger;
  dropProfile();
  if (el && el.isConnected) {
    setTimeout(() => {
      try {
        el.focus({ preventScroll: true });
      } catch {
        // an element that cannot take focus any more
      }
    }, 0);
  }
}

// ── match scoreboard ─────────────────────────────────────────────────────────────
/** Leave the open scoreboard: the slot forgets it and Rust stops building it. */
function dropMatch(): void {
  const id = router.matchId;
  if (id === null) return;
  router.matchId = null;
  router.matchFocus = null;
  router.matchFrom = null;
  router.encFrom = null;
  matchSlot.close();
  closeMatch(id).catch((err: unknown) => reportError("match close", err));
}

/**
 * Open a scoreboard: the History screen with `matchId`, from `focusPuuid`'s side. Closes the drawer. The list screen
 * it was opened from keeps its scroll position for Back. `encFrom` (the profile to reopen on Back) defaults to the
 * drawer open right now, for `from: "encounter" | "profile"`.
 */
/** The scoreboard was opened from the keyboard (its opener had a visible focus): it starts with the focus on Back. */
let matchByKeyboard = false;
export const matchOpenedByKeyboard = (): boolean => matchByKeyboard;

export function openMatch(matchId: string, opts: { from: MatchFrom; focusPuuid: string; encFrom?: EncFrom }): void {
  hideTip();
  cardOpenGuard.arm();
  matchByKeyboard = activeElement()?.matches(":focus-visible") ?? false;
  const encFrom =
    opts.encFrom ??
    ((opts.from === "encounter" || opts.from === "profile") && router.profile !== null
      ? { puuid: router.profile, screen: router.screen }
      : null);
  if (scroller && router.matchId === null) savedScroll.set(router.screen, scroller.scrollTop);
  if (scroller) scroller.scrollTop = 0;
  restoreTo = 0;
  dropProfile();
  if (router.matchId !== matchId) dropMatch();
  router.screen = "history";
  router.matchId = matchId;
  router.matchFocus = opts.focusPuuid;
  router.matchFrom = opts.from;
  router.encFrom = encFrom;
  matchSlot
    .open(matchId, (id) => getMatchDetails(id, opts.focusPuuid), {
      retryMs: DETAILS_RETRY_MS,
      timeoutMs: DETAILS_TIMEOUT_MS,
      settle: emptyMatch,
    })
    .catch((err: unknown) => reportError("match", err));
}

/**
 * Back from a scoreboard: encounter / profile → the screen under the drawer, drawer reopened on that player;
 * recap / lastMatch → Live; comp → Competitive; history → the History list. The list comes back at its old scroll.
 */
export function backFromMatch(): void {
  if (router.matchId === null) return;
  hideTip();
  const matchId = router.matchId;
  const from = router.matchFrom;
  const enc = router.encFrom;
  dropMatch();
  const to: Screen =
    (from === "encounter" || from === "profile") && enc
      ? enc.screen
      : from === "recap" || from === "lastMatch"
        ? "live"
        : from === "comp"
          ? "comp"
          : "history";
  restoreTo = savedScroll.get(to) ?? 0;
  router.screen = to;
  if (enc && (from === "encounter" || from === "profile")) openProfile(enc.puuid, null);
  // The page under the scoreboard is drawn again: a keyboard user's focus goes back to the row that opened it (a
  // History row, Live's LAST MATCH and the recap's full-scoreboard button carry its `data-match-id`), so the next Tab
  // continues from there and not from the title bar.
  else if (matchByKeyboard && typeof document !== "undefined") {
    setTimeout(() => {
      const row = document.querySelector<HTMLElement>(`[data-match-id="${CSS.escape(matchId)}"]`);
      row?.focus({ preventScroll: true });
    }, 0);
  }
}

// ── screens ──────────────────────────────────────────────────────────────────────
/** Go to a screen (nav rail): closes the drawer and any scoreboard, hides the tooltip, starts at the top. */
export function go(screen: Screen): void {
  hideTip();
  dropProfile();
  dropMatch();
  if (scroller) scroller.scrollTop = 0;
  restoreTo = 0;
  router.screen = screen;
}

// ── glance ───────────────────────────────────────────────────────────────────────
/** Glance mode on/off: the UI flag, and the window (a 420px column, or back) through the backend's `set_glance`. */
export function setGlance(on: boolean): void {
  hideTip();
  router.glance = on;
  setGlanceWindow(on).catch((err: unknown) => reportError("glance", err));
}

// ── stage hooks (stores.ts `applyLive`) ──────────────────────────────────────────
/** A new live stage: entering Agent Select always brings the user to Live. */
export function onStage(prev: Stage | null, next: Stage): void {
  if (next === "pregame" && prev !== "pregame") go("live");
}

/** Kept for `applyLive`: the v2 design always starts on Live, so the first settled stage no longer picks a page. */
export function startPage(_stage: Stage, _settled: boolean): void {}
