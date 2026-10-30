import { isTauri } from "@tauri-apps/api/core";
import { derived, get, readable, writable } from "svelte/store";
import { myStats, tauriBridge } from "./api";
import { loadMeta, openDetails, openMatch, startLive, startMyStats } from "./boot";
import { backFromMatch, closeProfile, onStage, setLiveCardLookup, startPage } from "./router.svelte";
import { hideTip } from "./tooltip.svelte";
import type { LiveState, MatchDetailsView, Meta, MyStats, PlayerDetails, Stage } from "./types";
import { reportError } from "./uilog";

/** Live stage + roster. Pushed by Rust (`emit("live")`); the UI never polls Riot. null = nothing received yet. */
export const live = writable<LiveState | null>(null);

// The profile drawer's settled (failed) state keeps what the live card knew (router `openProfile`).
setLiveCardLookup((puuid) => get(live)?.players.find((c) => c.puuid === puuid));

/**
 * The My Stats page's data (`invoke("my_stats")` once at startup, then Rust's `mystats` events; the UI never asks
 * Riot). Tri-state like the card fields (R23): `undefined` = nothing answered yet (the page draws its skeleton),
 * `null` = Rust answered "unknown" (the user's puuid or the current act is not known yet: the frames show `—`).
 */
export const mystats = writable<MyStats | null | undefined>(undefined);

/**
 * The account a live state names: its own card's puuid, or the session's `selfPuuid` that Rust carries — which also
 * names it in a `closed` state and for an account the DB has never seen (no self card at all). null when it names none
 * (before the first connection, or a live state from a Rust that has no `selfPuuid` yet).
 */
const liveSelf = derived(
  live,
  ($live) => $live?.players.find((p) => p.isSelf)?.puuid ?? $live?.selfPuuid ?? null,
);

/** The last account a live state named: a self-less live state must not move the account back through `mystats`. */
let lastLiveSelf: string | null = null;
liveSelf.subscribe((id) => {
  if (id !== null) lastLiveSelf = id;
});

/**
 * The Riot account the app is showing: the live self card's puuid, else the live state's `selfPuuid` (Rust knows it
 * from the session, so a switch to an account with no self card and no computable My Stats is signalled at once), else
 * the last account a live state named, else the My Stats snapshot's puuid — which is only a fallback before any live
 * state has named one, so a stale My Stats snapshot can never move it back. null while none is known.
 */
export const selfId = derived(
  [liveSelf, mystats],
  ([$liveSelf, $mystats]) => $liveSelf ?? lastLiveSelf ?? $mystats?.puuid ?? null,
);

/**
 * My Stats only while the snapshot belongs to the account the app is showing (`mystats.puuid === selfId`). On an account
 * switch Rust re-pushes My Stats for the new account, but until that push lands the previous account's snapshot must not
 * be drawn by any user-relative surface: this keeps the tri-state each of them relies on — `undefined` = nothing answered
 * yet *for this account* (skeletons), `null` = Rust answered "unknown" (the `—` frames; a null answer carries no account
 * and is never another account's numbers), a snapshot = this account's. While no account is known yet (`selfId` null)
 * the snapshot passes through: that is the startup ordering (`my_stats` answering before the first `live` state) and
 * there is nothing to compare it with.
 */
export const myStatsForSelf = derived([mystats, selfId], ([$mystats, $selfId]) =>
  $mystats && $selfId !== null && $mystats.puuid !== $selfId ? undefined : $mystats,
);

/**
 * Runs `fn` when the Riot account changes while the app is open: two different PUUIDs were the known account in turn.
 * A change to or from "unknown" (nothing known at startup, a dropped connection) is not a switch, so the first account
 * learned never fires it and Closed↔Menus never does. One case can fire it at boot: the first live snapshot carries the
 * account stored in the DB and the connection then reports a different one (Riot was switched while the app was closed)
 * — that is a real account change, and the screens must not keep the stored account's data. Returns the unsubscribe
 * function.
 */
export function onAccountChange(fn: (from: string, to: string) => void): () => void {
  let last: string | null = null;
  return selfId.subscribe((id) => {
    if (id === null) return;
    if (last !== null && last !== id) fn(last, id);
    last = id;
  });
}

/**
 * Called when the My Stats page opens (or right after an account switch): while it has no data (nothing answered yet,
 * or Rust answered null because the user or the act was not known at startup) the snapshot is asked for again, a local
 * DB read. An event, or an answer that landed meanwhile, is never overwritten. A failure keeps what is there. An answer
 * for an account that is no longer the one on screen (a rapid A→B→C switch: the read was issued for B) is dropped
 * instead of occupying the store — a non-null answer may only fill an empty store when it is the current account's;
 * a null answer carries no account and is always the neutral "unknown" state.
 */
export async function refreshMyStats(): Promise<void> {
  if (get(mystats)) return;
  try {
    const s = await myStats();
    if (get(mystats)) return;
    const self = get(selfId);
    if (s !== null && self !== null && s.puuid !== self) return;
    mystats.set(s);
  } catch (err) {
    reportError("mystats", err);
  }
}

/** Names and colors for agents, maps and tiers (`invoke("meta")`). null = still pending (spec §5.8). */
export const meta = writable<Meta | null>(null);

/** The Player Details currently open (boot.ts: Details opens it, a `details` event replaces it, Live closes it). */
export { openDetails };

/** The Match Details currently open (boot.ts: the Match page opens it, a `match` event replaces it, leaving closes it). */
export { openMatch };

/**
 * The latest `match` event (R150: the open match rebuilt with names that arrived). The recap is not the router's open
 * match, so it takes its own match's updates from here.
 */
export const matchEvents = writable<MatchDetailsView | null>(null);

/**
 * Wall clock for "time since" labels; one tick every 30s is plenty. A subscription starting sets the clock at once:
 * `readable` keeps the last value, so with no subscriber for hours (VALORANT closed, no card mounted) the first
 * card to mount would otherwise read module-load time until the first tick.
 */
export const now = readable(Date.now(), (set) => {
  set(Date.now());
  const id = setInterval(() => set(Date.now()), 30_000);
  return () => clearInterval(id);
});

let prevStage: Stage | null = null;

/**
 * Every LiveState goes through here: entering Agent Select switches to the Live screen (router `onStage`).
 * `startPage` is a no-op kept for compatibility (the v2 design always starts on Live).
 */
export function applyLive(s: LiveState | null): void {
  if (s) {
    startPage(s.stage, s.settled);
    onStage(prevStage, s.stage);
    prevStage = s.stage;
    // R163 (M1): a Details or Match page that settled empty gets its one retry on the next live event.
    openDetails.retrySettled();
    openMatch.retrySettled();
  } else {
    prevStage = null;
  }
  live.set(s);
}

/**
 * A Riot account switch: the open profile drawer and scoreboard show values relative to the user (encounters
 * with/against, "your result", the user's own row) and were built for the previous account. They cannot be corrected in
 * place, so both close (and the open tooltip, which may describe the old account's card) instead of staying up with the
 * old account's numbers; the screen a scoreboard returns to is refreshed for the new account by the screen itself
 * (Saved / History / Skins). A drawer a scoreboard came from is not reopened: the switch did not ask for it, and the
 * screen under it is the new account's.
 *
 * The My Stats snapshot is the previous account's too. Normally Rust's switch already asks for a push
 * (`on_connected` → `my_stats::notify_now`), which emits the new account's stats and replaces it; but when the new
 * account is not computable (no current act/content tables, or no self puuid) that push emits nothing, and no later
 * event may ever come: the page would sit on its skeleton for good. So, as a cheap fallback for exactly that case (one
 * DB-only read per real switch), the store resets to `undefined` ("nothing known yet for this account": the page draws
 * its skeleton briefly) and the DB-only `my_stats` command is asked once for the account on screen; its `null` answer
 * settles the page on its `—` frames, its data shows immediately, and Rust's push updates as today. When the snapshot
 * already belongs to the new account (the switch was signalled by a `mystats` event itself: the normal case is `selfId`
 * from the live state, but the fallback exists), it is kept and nothing is asked.
 */
onAccountChange((_from, to) => {
  hideTip();
  closeProfile();
  backFromMatch({ reopenProfile: false });
  if (get(mystats)?.puuid !== to) {
    mystats.set(undefined);
    void refreshMyStats();
  }
});

let started = false;

/** Connect the stores to their source. Call once at startup (spec §2.4). */
export async function startFeeds(): Promise<void> {
  if (started) return;
  started = true;

  // DEV in a plain browser: the demo feed (dynamic import → compiled out of `vite build`).
  if (import.meta.env.DEV && !isTauri()) {
    const { startDemo } = await import("./demo/feed");
    startDemo(applyLive, meta, mystats);
    return;
  }

  const failed = (what: string) => (err: unknown) => reportError(what, err);
  // Not awaited: while the asset tables are unavailable it keeps retrying (see loadMeta), and the other feeds
  // must not wait for it. Only the first failure is logged, so an offline session doesn't repeat itself.
  // R163: Rust also pushes a `meta` event whenever the tables first load or are refreshed (a table that loaded by
  // another route, a patch's new ids); it replaces the store, and the retries stop once one arrived.
  let metaPushed = false;
  void loadMeta(
    tauriBridge,
    (m) => meta.set(m),
    (err, attempt) => {
      if (attempt === 0) failed("meta")(err);
    },
    () => metaPushed,
  );
  await Promise.all([
    tauriBridge
      .listen<Meta>("meta", (e) => {
        metaPushed = true;
        meta.set(e.payload);
      })
      .catch(failed("meta event")),
    startLive(tauriBridge, applyLive).catch(failed("live")),
    // Never rejects: a failed `my_stats()` settles the page on its `—` frames (see startMyStats).
    startMyStats(tauriBridge, (s) => mystats.set(s), failed("mystats")),
    tauriBridge
      .listen<PlayerDetails>("details", (e) => openDetails.event(e.payload))
      .catch(failed("details event")),
    // R150: Rust emits the open match again when names arrived for it (`MatchDetailsView`, same shape as `match_details`).
    tauriBridge
      .listen<MatchDetailsView>("match", (e) => {
        openMatch.event(e.payload);
        matchEvents.set(e.payload);
      })
      .catch(failed("match event")),
  ]);
}
