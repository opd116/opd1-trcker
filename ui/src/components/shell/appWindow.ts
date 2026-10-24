// The Tauri window behind the custom title bar (window `decorations: false`). In the DEV browser there is no Tauri:
// every call is a no-op and the window reads as not maximized.
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow, type Window } from "@tauri-apps/api/window";
import { reportError } from "../../lib/uilog";

function current(): Window | null {
  if (!isTauri()) return null;
  try {
    return getCurrentWindow();
  } catch (err) {
    reportError("window", err);
    return null;
  }
}

const run = (what: string, f: (w: Window) => Promise<unknown>) => {
  const w = current();
  if (w) f(w).catch((err: unknown) => reportError(`window ${what}`, err));
};

export const minimize = () => run("minimize", (w) => w.minimize());
export const toggleMaximize = () => run("maximize", (w) => w.toggleMaximize());
/** Close: Rust decides between quitting and hiding to the tray (the "Close to tray" setting). */
export const close = () => run("close", (w) => w.close());

/** Calls `cb` with the maximized state now and after every resize. Returns the cleanup. */
export function watchMaximized(cb: (maximized: boolean) => void): () => void {
  const w = current();
  if (!w) return () => {};
  let stop: (() => void) | null = null;
  let dead = false;
  const read = () => w.isMaximized().then(cb, (err: unknown) => reportError("window isMaximized", err));
  void read();
  w.onResized(() => void read()).then(
    (un) => (dead ? un() : (stop = un)),
    (err: unknown) => reportError("window onResized", err),
  );
  return () => {
    dead = true;
    stop?.();
  };
}
