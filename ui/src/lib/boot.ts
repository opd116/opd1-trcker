// Startup wiring that can be tested without Tauri (spec §2.4). The Tauri-backed `Bridge` is in api.ts.
import type { LiveState, PlayerDetails } from "./types";

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
