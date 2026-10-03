// Startup wiring and the open Player Details and Match Details, testable without Tauri (spec §2.4). The Tauri-backed `Bridge` is in api.ts.
import { writable, type Readable } from "svelte/store";
import { EMPTY_META } from "./meta";
import type { LiveState, MatchDetailsView, Meta, MyStats, PlayerDetails } from "./types";

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

/**
 * My Stats (the user's own account, built by Rust from the DB): `listen('mystats')` (payload = a `MyStats`, never
 * null), then `invoke('my_stats')` (`MyStats | null`: null until the user's puuid and the current act are known).
 * The same order as `startLive`: listening first misses no event, and the snapshot counts only when no event came
 * before it. A failed call settles the page on null (its `—` frames) instead of a skeleton that never ends; a later
 * event still replaces that. Nothing here rejects: `onError` sees each failure.
 */
export async function startMyStats(
  b: Bridge,
  apply: (s: MyStats | null) => void,
  onError: (err: unknown) => void = () => {},
): Promise<void> {
  let sawEvent = false;
  try {
    await b.listen<MyStats>("mystats", (e) => {
      sawEvent = true;
      apply(e.payload);
    });
  } catch (err) {
    onError(err);
  }
  try {
    const snapshot = await b.invoke<MyStats | null>("my_stats");
    if (!sawEvent) apply(snapshot);
  } catch (err) {
    onError(err);
    if (!sawEvent) apply(null);
  }
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

/**
 * Keys WebView2 would act on that a finished app must not: reload (F5, Ctrl+F5, Ctrl+R, Ctrl+Shift+R) restarts the page
 * and drops the open Details, Ctrl+P prints, Ctrl+F opens find, Ctrl+U shows the source, F7 asks for caret browsing.
 * Never Ctrl+Shift+L or Ctrl+Shift+D (`keyAction` above); Ctrl+Alt chords are AltGr characters on some layouts, and
 * Tab, Enter and Space are the keyboard's way into the UI.
 */
export function browserKeyBlocked(
  e: Pick<KeyboardEvent, "key" | "ctrlKey" | "shiftKey" | "altKey" | "metaKey">,
): boolean {
  if (e.key === "F5" || e.key === "F7") return true;
  if (!e.ctrlKey || e.altKey || e.metaKey) return false;
  const k = e.key.toLowerCase();
  return k === "r" || (!e.shiftKey && (k === "p" || k === "f" || k === "u"));
}

/**
 * Release builds only (main.ts calls it when `!import.meta.env.DEV`): no context menu and none of `browserKeyBlocked`.
 * Development keeps the browser's own behaviour (reload, inspect).
 */
export function installReleaseGuards(target: Pick<Window, "addEventListener">): void {
  target.addEventListener("contextmenu", (e) => e.preventDefault());
  target.addEventListener("keydown", (e) => {
    if (browserKeyBlocked(e)) e.preventDefault();
  });
}

export async function runKeyAction(
  action: "copyLog" | "dumpRaw",
  b: Bridge,
  writeText: (text: string) => Promise<void>,
): Promise<void> {
  if (action === "copyLog") await writeText(await b.invoke<string>("copy_log"));
  else await b.invoke("dump_raw");
}

/** A rejected `player_details` / `match_details` is retried once after this long. */
export const DETAILS_RETRY_MS = 1_000;

/**
 * `call()`; when it rejects, one more `call()` after `delayMs`, whose result or rejection is the answer. `wanted` is
 * asked after the wait: when it says no (the user has moved on, and a repeated `player_details` would re-open the
 * page on the Rust side), the first error is thrown and nothing is repeated.
 */
export async function retryOnce<T>(call: () => Promise<T>, delayMs: number, wanted: () => boolean = () => true): Promise<T> {
  try {
    return await call();
  } catch (first) {
    await new Promise<void>((resolve) => setTimeout(resolve, delayMs));
    if (!wanted()) throw first;
    return call();
  }
}

/** The open page's payload (Player Details, Match Details): the payload on screen, and the three ways it changes. */
export interface OpenOptions<T = PlayerDetails> {
  /** A rejected call is retried once after this many ms; skipped when anything wrote the slot during the wait. */
  retryMs?: number;
  /**
   * When the call has failed for good (after the retry, if any) and the slot is still empty for this key: the
   * payload to settle on, so the skeleton ends. A later event still replaces it.
   */
  settle?: (key: string) => T;
}

/**
 * One open page's payload, keyed by what identifies the page (a player's puuid, a match id). The page opens it, the
 * Rust event for that page feeds it, and leaving the page closes it: nothing is ever shown for a page that is not the
 * open one, and an older payload never lands over a newer one.
 */
export interface Slot<T> extends Readable<T | null> {
  /**
   * The page opens `key` (a card click, or Back from Match): the command through `fetch`. Another key's payload is
   * cleared at once (the skeleton shows); the same key's stays on screen until the answer. The answer is dropped when
   * anything wrote the slot while it was in flight: an event (always newer than the snapshot the call returns),
   * another open, or a close. Rejects when the call fails for good (after `opts.retryMs`'s one retry, and after
   * `opts.settle` has been applied).
   */
  open(key: string, fetch: (key: string) => Promise<T>, opts?: OpenOptions<T>): Promise<void>;
  /** A Rust event: replaces the payload when it is for the key open or opening; anything else is ignored. */
  event(payload: T): void;
  /** The page was left: forget the payload, so a reopen starts from the skeleton (never a stale name). */
  close(): void;
}

export type DetailsSlot = Slot<PlayerDetails>;
export type MatchSlot = Slot<MatchDetailsView>;

/** A slot for payloads that `keyOf` tells apart: the key an event must carry to be for the page that is open. */
function keyedSlot<T>(keyOf: (payload: T) => string): Slot<T> {
  const store = writable<T | null>(null);
  /** What is on screen (the store's value, readable without a subscription). */
  let current: T | null = null;
  const put = (v: T | null) => {
    current = v;
    store.set(v);
  };
  /** The key of the page open or opening; null once it was left. */
  let openKey: string | null = null;
  /** Bumped by every write an answer still in flight must not overwrite. */
  let version = 0;
  return {
    subscribe: store.subscribe,
    async open(key, fetch, opts = {}) {
      if (openKey !== key) {
        openKey = key;
        put(null);
      }
      const v = ++version;
      try {
        const res =
          opts.retryMs === undefined
            ? await fetch(key)
            : await retryOnce(() => fetch(key), opts.retryMs, () => v === version);
        if (v === version) put(res);
      } catch (err) {
        // Failed for good: end the skeleton, unless something newer wrote the slot or this key's payload is
        // already on screen (Back from Match keeps what it had).
        if (v === version && current === null && opts.settle) put(opts.settle(key));
        throw err;
      }
    },
    event(payload) {
      if (keyOf(payload) !== openKey) return;
      version++;
      put(payload);
    },
    close() {
      openKey = null;
      version++;
      put(null);
    },
  };
}

/** The open Player Details: keyed by the player. */
export const detailsSlot = (): DetailsSlot => keyedSlot<PlayerDetails>((d) => d.card.puuid);

/** The open Match Details (R150): keyed by the match id. */
export const matchSlot = (): MatchSlot => keyedSlot<MatchDetailsView>((m) => m.matchId);

/**
 * The Player Details on screen (spec §2.4, §6.9). Details opens it, the `details` listener feeds it, and the router
 * closes it on every navigation to Live. It lives here, not in stores.ts, so the router can reach it without an
 * import cycle (stores.ts imports the router); stores.ts re-exports it.
 */
export const openDetails = detailsSlot();

/**
 * The Match Details on screen (R150). The Match page opens it and closes it when it is left (Back, the Live tab, a new
 * match's switch to Live); the `match` listener feeds it, so the names Rust finds for the open match replace the
 * agent names in place. Same reasons as `openDetails` for living here.
 */
export const openMatch = matchSlot();
