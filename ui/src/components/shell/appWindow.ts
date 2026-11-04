// The Tauri window behind the custom title bar (window `decorations: false`). In the DEV browser there is no Tauri:
// every call is a no-op.
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
/** Close: Rust decides between quitting and hiding to the tray (the "Close to tray" setting). */
export const close = () => run("close", (w) => w.close());
