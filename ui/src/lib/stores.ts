import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { readable, writable } from "svelte/store";
import { EMPTY_META } from "./meta";
import type { LiveState, Meta } from "./types";

export const EMPTY_LIVE: LiveState = { stage: "closed", mapId: null, players: [] };

/** Live stage + roster. Pushed by Rust (`emit("live")`); the UI never polls Riot. */
export const live = writable<LiveState>(EMPTY_LIVE);

/** Names and colors for agents, maps and tiers (`invoke("meta")`). */
export const meta = writable<Meta>(EMPTY_META);

/** Wall clock for "time since" labels; one tick every 30s is plenty. */
export const now = readable(Date.now(), (set) => {
  const id = setInterval(() => set(Date.now()), 30_000);
  return () => clearInterval(id);
});

let started = false;

/** Connect the stores to their source. Call once at startup. */
export async function startFeeds(): Promise<void> {
  if (started) return;
  started = true;

  // DEV in a plain browser: the demo feed (dynamic import → compiled out of `vite build`).
  if (import.meta.env.DEV && !isTauri()) {
    const { startDemo } = await import("./demo/feed");
    startDemo(live, meta);
    return;
  }

  await listen<LiveState>("live", (event) => live.set(event.payload));
  try {
    meta.set(await invoke<Meta>("meta"));
  } catch (err) {
    console.error("meta() failed", err);
  }
}
