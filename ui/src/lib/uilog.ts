// UI failures into the app's log. A release build has no visible console, so a crashed page or an unhandled promise would
// leave no trace; here they go to Rust (`ui_log`), into the same ring Ctrl+Shift+L copies and Ctrl+Shift+D dumps.
// Rust rate-limits (20/min), caps at 500 chars and shortens uuids; this side still sends one line per distinct failure
// per 10 s and at most 20 lines per 10 s, never throws, and never leaves a rejection of its own behind. A line carries
// nothing from the failing value beyond an Error's own name and message, a string, or an object's `message` and key
// names: a Riot ID inside a card or roster object never reaches the log (logbuf.rs rule).
import { invoke, isTauri } from "@tauri-apps/api/core";

export type LogLevel = "error" | "warn" | "info";

/** One message is at most this long (Rust's cap): the formatted line ends in `…` when it had to be cut. */
export const MAX_LINE = 500;
/** An identical line is sent once per this many ms, and no more than `MAX_SENDS` lines of any kind in the same window. */
export const DEDUPE_MS = 10_000;
export const MAX_SENDS = 20;
/** At most this many key names of a non-Error object are named in its line. */
const MAX_KEYS = 8;

/**
 * `path:line:col` of the first stack frame that has one: no origin, no `?query`. V8 (WebView2) writes
 * `at fn (http://host/p.js?t=1:10:5)` or `at http://host/p.js:10:5`; WebKit and Firefox write `fn@http://host/p.js:10:5`.
 * A line that is no frame (the message lines) or has no `:line:col` (`at Array.map (<anonymous>)`) is skipped.
 */
function firstFrame(stack: unknown): string | null {
  if (typeof stack !== "string") return null;
  for (const line of stack.split("\n")) {
    // V8 prints the message first (it may contain a `x:1:2` of its own): only `at …` and `fn@…` lines are frames.
    if (!/^\s*at\s/.test(line) && !/^\S*@/.test(line)) continue;
    const m = /([^\s()@]+?):(\d+):(\d+)\)?\s*$/.exec(line);
    if (!m) continue;
    const path = m[1].replace(/^[a-z][a-z0-9+.-]*:\/\/[^/]*/i, "").replace(/[?#].*$/, "");
    return `${path}:${m[2]}:${m[3]}`;
  }
  return null;
}

const collapse = (s: string) => s.replace(/\s+/g, " ").trim();

/** `text` cut to `max` chars with a closing `…` (so the result is `max` long), never between the halves of a surrogate pair. */
function cap(text: string, max: number): string {
  if (text.length <= max) return text;
  let cut = text.slice(0, Math.max(0, max - 1));
  if (/[\uD800-\uDBFF]$/.test(cut)) cut = cut.slice(0, -1);
  return `${cut}…`;
}

/**
 * What a thrown value that is neither an Error nor a string says about itself, and nothing else: its `message` when it
 * has a string one (Tauri rejects with such objects), else `[object Object]` and up to 8 of its key names (identifier-like
 * ones only), so the shape is known but no value is. Never JSON, never its own `toString()`: a card or roster object
 * holds Riot IDs. A primitive (a number, `null`, a boolean) is its own `String`.
 */
function describe(v: unknown): string {
  try {
    if ((typeof v !== "object" && typeof v !== "function") || v === null) return String(v);
    const message = (v as { message?: unknown }).message;
    if (typeof message === "string") return message;
    // An array's keys are its indexes: nothing to tell.
    const keys = Array.isArray(v)
      ? []
      : Object.keys(v)
          .filter((k) => /^[\w$.-]{1,32}$/.test(k))
          .slice(0, MAX_KEYS);
    const tag = Object.prototype.toString.call(v);
    return keys.length ? `${tag} [${keys.join(", ")}]` : tag;
  } catch {
    return "unprintable error";
  }
}

/**
 * The one-line form of a failure: `kind: detail`, whitespace collapsed, at most `MAX_LINE` chars. When it is too long the
 * message is cut, never the ` @ path:line:col` of the Error's first frame. Never throws.
 */
export function formatError(kind: string, err: unknown): string {
  let head: string;
  try {
    if (err instanceof Error) head = err.message ? `${err.name}: ${err.message}` : String(err.name);
    else if (typeof err === "string") head = err;
    else head = describe(err);
  } catch {
    head = "unprintable error";
  }
  let tail = "";
  try {
    const frame = err instanceof Error ? firstFrame(err.stack) : null;
    if (frame) tail = ` @ ${frame}`;
  } catch {
    // An Error whose `stack` throws is reported without a frame.
  }
  const line = collapse(`${kind}: ${head}`);
  if (line.length + tail.length <= MAX_LINE) return line + tail;
  // Too long: keep the frame and cut the message to what is left. Too little room (a huge kind or path): cut it all.
  const room = MAX_LINE - tail.length;
  return room >= kind.length + 16 ? cap(line, room) + tail : cap(line + tail, MAX_LINE);
}

export interface ReporterDeps {
  /** The Tauri `invoke`; absent = nothing is ever sent. */
  invoke?: (cmd: string, args: Record<string, unknown>) => Promise<unknown>;
  /** True only when Rust is there (false in the DEV browser). */
  inTauri: () => boolean;
  now: () => number;
}

export type Reporter = (kind: string, err: unknown, level?: LogLevel) => void;

/**
 * `report(kind, err, level)`: development builds always show it in the console (with the real error, so DevTools can
 * expand its stack); under Tauri the formatted line goes to Rust unless the same line went out in the last 10 s (the
 * first send's time counts, so a failure that repeats every second is sent every 10 s) or 20 lines already did (the rest
 * are dropped without a trace). Never throws; a failed send is dropped (a dev console warning, nothing more).
 */
export function createReporter(deps: ReporterDeps): Reporter {
  /**
   * Send time per line, for the sends of the last 10 s. A line is never sent twice inside the window, so each entry is
   * exactly one send: its size is the rolling count for the cap, and it never holds more than `MAX_SENDS` entries.
   */
  const sent = new Map<string, number>();
  // The development console, and only that: written inline under `import.meta.env.DEV` so a release build carries none of
  // it, and a console that throws is no reason to skip the send or to throw.
  const failed = (e: unknown) => {
    if (import.meta.env.DEV) {
      try {
        console.warn("ui_log failed", e);
      } catch {
        // Nothing left to tell.
      }
    }
  };
  return (kind, err, level = "error") => {
    try {
      if (import.meta.env.DEV) {
        try {
          console.error(kind, err);
        } catch {
          // A broken console never blocks the send.
        }
      }
      if (!deps.invoke || !deps.inTauri()) return;
      const message = formatError(kind, err);
      const t = deps.now();
      // A clock that was set back leaves entries "from the future": they expire too, or the cap would hold for hours.
      for (const [k, at] of sent) if (t - at >= DEDUPE_MS || t < at) sent.delete(k);
      const key = `${level}\n${message}`;
      if (sent.has(key) || sent.size >= MAX_SENDS) return;
      sent.set(key, t);
      void Promise.resolve(deps.invoke("ui_log", { level, message })).catch(failed);
    } catch (e) {
      // Reporting a failure must never become one.
      failed(e);
    }
  };
}

/** The app's reporter: the real `invoke`, `isTauri` and clock. */
export const reportError: Reporter = createReporter({
  invoke: (cmd, args) => invoke(cmd, args),
  inTauri: () => isTauri(),
  now: () => Date.now(),
});

const installed = new WeakSet<object>();

/**
 * Whatever escapes every handler goes to the log: an uncaught error (`window`) and a promise rejected with no one
 * listening (`promise`). Once per window; the default behaviour of both events is left alone.
 */
export function installErrorReporting(target: Window, report: Reporter = reportError): void {
  if (installed.has(target)) return;
  installed.add(target);
  target.addEventListener("error", (ev: ErrorEvent) => report("window", ev.error ?? ev.message));
  target.addEventListener("unhandledrejection", (ev: PromiseRejectionEvent) => report("promise", ev.reason));
}
