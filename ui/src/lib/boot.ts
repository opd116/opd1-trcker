// Startup wiring that can be tested without Tauri (spec §2.4). The Tauri-backed `Bridge` is in api.ts.
import { EMPTY_META } from "./meta";
import type { LiveState, Meta, PlayerDetails } from "./types";

export interface Bridge {
  listen<T>(ev: string, cb: (e: { payload: T }) => void): Promise<() => void>;
  invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T>;
}

/**
 * `listen('live')`, then `invoke('live_state')`. Listening first means no event is missed; the invoke
 * result is applied only if no event came first (an event is always newer than the snapshot).
 */
export async function startLive(b: Bridge, apply: (s: LiveState) => void): Promise<void> {
  let sawEvent = false;
  await b.listen<LiveState>("live", (e) => {
    sawEvent = true;
    apply(e.payload);
  });
  const snapshot = await b.invoke<LiveState>("live_state");
  if (!sawEvent) apply(snapshot);
}

/** Retry delays for `loadMeta`: 2s, doubling, capped at 30s (Rust re-tries a failed asset download only after 30s). */
export const META_RETRY_FIRST_MS = 2_000;
export const META_RETRY_MAX_MS = 30_000;

/**
 * `invoke('meta')` until it succeeds. Rust rejects it whenever the asset tables are unavailable (offline first
 * start, or a transient download failure). The first rejection sets `EMPTY_META` at once, so names render as
 * the loaded-miss "—" instead of a skeleton bar that never resolves (spec §5.8: null = pending, EMPTY_META =
 * loaded but empty); each retry that succeeds replaces it with the real tables. `onError` sees every failure
 * with its attempt number (0-based). Resolves after the first success; the caller does not await it at startup.
 */
export async function loadMeta(
  b: Bridge,
  set: (m: Meta) => void,
  onError: (err: unknown, attempt: number) => void = () => {},
): Promise<void> {
  let delay = META_RETRY_FIRST_MS;
  for (let attempt = 0; ; attempt++) {
    try {
      set(await b.invoke<Meta>("meta"));
      return;
    } catch (err) {
      onError(err, attempt);
      if (attempt === 0) set(EMPTY_META);
    }
    await new Promise<void>((resolve) => setTimeout(resolve, delay));
    delay = Math.min(delay * 2, META_RETRY_MAX_MS);
  }
}

/** Ctrl+Shift+L copies the log, Ctrl+Shift+D dumps the raw captures. No UI for either. */
export function keyAction(
  e: Pick<KeyboardEvent, "key" | "ctrlKey" | "shiftKey" | "altKey" | "metaKey">,
): "copyLog" | "dumpRaw" | null {
  if (!e.ctrlKey || !e.shiftKey || e.altKey || e.metaKey) return null;
  const k = e.key.toLowerCase();
  return k === "l" ? "copyLog" : k === "d" ? "dumpRaw" : null;
}

export async function runKeyAction(
  action: "copyLog" | "dumpRaw",
  b: Bridge,
  writeText: (text: string) => Promise<void>,
): Promise<void> {
  if (action === "copyLog") await writeText(await b.invoke<string>("copy_log"));
  else await b.invoke("dump_raw");
}

/** A `details` event replaces the open details only when it is for the same player. */
export function applyDetailsEvent(open: PlayerDetails | null, payload: PlayerDetails): PlayerDetails | null {
  return open && open.card.puuid === payload.card.puuid ? payload : open;
}
