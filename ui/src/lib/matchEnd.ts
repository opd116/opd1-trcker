// A match that just ended is not stored yet when the stage leaves In Game: Rust stores it once a sync has fetched it
// from Riot, the lobby re-sync or the post-match follow-up at +10, 20, 40, 80 and 160 s (src-tauri/src/poll.rs
// POST_MATCH_DELAYS). A screen listing stored matches asks again on that schedule, a little after each check.
import type { Readable } from "svelte/store";
import type { LiveState } from "./types";

/** When to ask again, counted from the stage leaving In Game. */
export const AFTER_MATCH_MS: readonly number[] = [5_000, 15_000, 25_000, 45_000, 85_000, 165_000];

/**
 * Each time the live stage leaves In Game, calls `start()` for that match end and then the asker it returns on the
 * AFTER_MATCH_MS schedule, until the asker answers true (what it waited for is stored) or the schedule runs out. A newer
 * match end replaces the run. Returns the stop function (for a component's teardown).
 */
export function followMatchEnds(
  live: Readable<LiveState | null>,
  start: () => () => Promise<boolean>,
  delays: readonly number[] = AFTER_MATCH_MS,
): () => void {
  let timer: ReturnType<typeof setTimeout> | undefined;
  let run = 0;
  let last: string | null | undefined;
  const unsubscribe = live.subscribe((l) => {
    const stage = l?.stage ?? null;
    const ended = last === "ingame" && stage !== "ingame";
    last = stage;
    if (!ended) return;
    clearTimeout(timer);
    const r = ++run;
    const ask = start();
    const step = (i: number) => {
      if (i >= delays.length) return;
      timer = setTimeout(
        () => {
          void ask()
            .catch(() => false)
            .then((done) => {
              if (!done && r === run) step(i + 1);
            });
        },
        delays[i]! - (i === 0 ? 0 : delays[i - 1]!),
      );
    };
    step(0);
  });
  return () => {
    run++;
    clearTimeout(timer);
    unsubscribe();
  };
}
