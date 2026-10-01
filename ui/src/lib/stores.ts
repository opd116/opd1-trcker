import { isTauri } from "@tauri-apps/api/core";
import { readable, writable } from "svelte/store";
import { tauriBridge } from "./api";
import { loadMeta, openDetails, openMatch, startLive } from "./boot";
import { onStage } from "./router.svelte";
import type { LiveState, MatchDetailsView, Meta, PlayerDetails, Stage } from "./types";

/** Live stage + roster. Pushed by Rust (`emit("live")`); the UI never polls Riot. null = nothing received yet. */
export const live = writable<LiveState | null>(null);

/** Names and colors for agents, maps and tiers (`invoke("meta")`). null = still pending (spec §5.8). */
export const meta = writable<Meta | null>(null);

/** The Player Details currently open (boot.ts: Details opens it, a `details` event replaces it, Live closes it). */
export { openDetails };

/** The Match Details currently open (boot.ts: the Match page opens it, a `match` event replaces it, leaving closes it). */
export { openMatch };

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

/** Every LiveState goes through here so entering Agent Select can switch the route to Live. */
export function applyLive(s: LiveState | null): void {
  if (s) {
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

let started = false;

/** Connect the stores to their source. Call once at startup (spec §2.4). */
export async function startFeeds(): Promise<void> {
  if (started) return;
  started = true;

  // DEV in a plain browser: the demo feed (dynamic import → compiled out of `vite build`).
  if (import.meta.env.DEV && !isTauri()) {
    const { startDemo } = await import("./demo/feed");
    startDemo(applyLive, meta);
    return;
  }

  const failed = (what: string) => (err: unknown) => console.error(`${what} failed`, err);
  // Not awaited: while the asset tables are unavailable it keeps retrying (see loadMeta), and the other feeds
  // must not wait for it. Only the first failure is logged, so an offline session doesn't repeat itself.
  // R163: Rust also pushes a `meta` event whenever the tables first load or are refreshed (a table that loaded by
  // another route, a patch's new ids); it replaces the store, and the retries stop once one arrived.
  let metaPushed = false;
  void loadMeta(
    tauriBridge,
    (m) => meta.set(m),
    (err, attempt) => {
      if (attempt === 0) failed("meta()")(err);
    },
    () => metaPushed,
  );
  await Promise.all([
    tauriBridge
      .listen<Meta>("meta", (e) => {
        metaPushed = true;
        meta.set(e.payload);
      })
      .catch(failed("meta listener")),
    startLive(tauriBridge, applyLive).catch(failed("live feed")),
    tauriBridge
      .listen<PlayerDetails>("details", (e) => openDetails.event(e.payload))
      .catch(failed("details listener")),
    // R150: Rust emits the open match again when names arrived for it (`MatchDetailsView`, same shape as `match_details`).
    tauriBridge
      .listen<MatchDetailsView>("match", (e) => openMatch.event(e.payload))
      .catch(failed("match listener")),
  ]);
}
