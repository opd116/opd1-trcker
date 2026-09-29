import { isTauri } from "@tauri-apps/api/core";
import { readable, writable } from "svelte/store";
import { tauriBridge } from "./api";
import { applyDetailsEvent, loadMeta, startLive } from "./boot";
import { onStage } from "./router.svelte";
import type { LiveState, Meta, PlayerDetails, Stage } from "./types";

/** Live stage + roster. Pushed by Rust (`emit("live")`); the UI never polls Riot. null = nothing received yet. */
export const live = writable<LiveState | null>(null);

/** Names and colors for agents, maps and tiers (`invoke("meta")`). null = still pending (spec §5.8). */
export const meta = writable<Meta | null>(null);

/** The Player Details currently open (set by the Details screen; a `details` event replaces it in place). */
export const openDetails = writable<PlayerDetails | null>(null);

/** Wall clock for "time since" labels; one tick every 30s is plenty. */
export const now = readable(Date.now(), (set) => {
  const id = setInterval(() => set(Date.now()), 30_000);
  return () => clearInterval(id);
});

let prevStage: Stage | null = null;

/** Every LiveState goes through here so entering Agent Select can switch the route to Live. */
export function applyLive(s: LiveState | null): void {
  if (s) {
    onStage(prevStage, s.stage);
    prevStage = s.stage;
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
  void loadMeta(
    tauriBridge,
    (m) => meta.set(m),
    (err, attempt) => {
      if (attempt === 0) failed("meta()")(err);
    },
  );
  await Promise.all([
    startLive(tauriBridge, applyLive).catch(failed("live feed")),
    tauriBridge
      .listen<PlayerDetails>("details", (e) => openDetails.update((open) => applyDetailsEvent(open, e.payload)))
      .catch(failed("details listener")),
  ]);
}
