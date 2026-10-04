// UI failures into the app log (`ui_log`): the one-line format, the 10 s dedupe and 20-per-10 s cap, the Tauri-only send
// that never throws, and the window listeners. The default `reportError` is bound to the real `invoke`; everything here
// goes through `createReporter` with fakes, or hands `installErrorReporting` its own reporter. The development console
// sink is pinned with `vi.stubEnv("DEV", …)`, so none of it depends on the mode the runner is in.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createReporter, DEDUPE_MS, formatError, installErrorReporting, MAX_LINE, MAX_SENDS, type ReporterDeps } from "./uilog";

/** An Error whose stack is exactly `stack` (engines differ, so tests never depend on this machine's own stack). */
function withStack(message: string, stack: string, name = "TypeError"): Error {
  const e = new Error(message);
  e.name = name;
  e.stack = stack;
  return e;
}

describe("formatError", () => {
  it("an Error: name, message and the first frame as path:line:col (no origin, no ?query)", () => {
    const e = withStack(
      "Cannot read properties of null (reading 'puuid')",
      [
        "TypeError: Cannot read properties of null (reading 'puuid')",
        "    at Array.map (<anonymous>)",
        "    at renderCard (http://tauri.localhost/assets/index-Ab12.js?v=3:10:5)",
        "    at other (http://tauri.localhost/assets/index-Ab12.js:99:1)",
      ].join("\n"),
    );
    expect(formatError("page", e)).toBe("page: TypeError: Cannot read properties of null (reading 'puuid') @ /assets/index-Ab12.js:10:5");
  });

  it("a frame without a function name, a dev-server URL with a ?t= query, and a file:// path", () => {
    expect(formatError("x", withStack("m", "TypeError: m\n    at http://127.0.0.1:1420/src/lib/stores.ts?t=1700000000:42:7"))).toBe(
      "x: TypeError: m @ /src/lib/stores.ts:42:7",
    );
    expect(formatError("x", withStack("m", "TypeError: m\n    at f (file:///C:/app/main.js:3:4)"))).toBe("x: TypeError: m @ /C:/app/main.js:3:4");
  });

  it("WebKit / Firefox frames (fn@url:line:col) work too", () => {
    expect(formatError("x", withStack("m", "render@http://localhost:1420/src/a.ts?t=9:12:3\nother@http://localhost:1420/src/b.ts:1:1"))).toBe(
      "x: TypeError: m @ /src/a.ts:12:3",
    );
  });

  it("a stack line inside the message is not a frame; an Error with no frame or no message still reads well", () => {
    // The message itself looks like `file:1:2`, and sits on the stack's first line (V8 prints it there).
    const e = withStack("bad json at data.json:1:2", "TypeError: bad json at data.json:1:2\n    at f (http://h/a.js:5:6)");
    expect(formatError("x", e)).toBe("x: TypeError: bad json at data.json:1:2 @ /a.js:5:6");
    expect(formatError("x", withStack("boom", "TypeError: boom\n    at <anonymous>"))).toBe("x: TypeError: boom");
    expect(formatError("x", withStack("boom", ""))).toBe("x: TypeError: boom");
    expect(formatError("x", withStack("", "Error\n    at f (http://h/a.js:5:6)", "Error"))).toBe("x: Error @ /a.js:5:6");
  });

  it("a real Error from this machine: ends in a frame location and is one line", () => {
    const line = formatError("real", new RangeError("out of range"));
    expect(line).toMatch(/^real: RangeError: out of range( @ \S+:\d+:\d+)?$/);
    expect(line).not.toMatch(/[\r\n]/);
  });

  it("a string is used as it is", () => {
    expect(formatError("details", "db busy")).toBe("details: db busy");
  });

  it("a primitive is its own String(); an object with a string message is that message (Tauri's rejections)", () => {
    expect(formatError("x", 42)).toBe("x: 42");
    expect(formatError("x", null)).toBe("x: null");
    expect(formatError("x", undefined)).toBe("x: undefined");
    expect(formatError("x", true)).toBe("x: true");
    expect(formatError("x", { message: "no shard", code: 7 })).toBe("x: no shard");
  });

  it("an object with no string message is its shape, never its values: a Riot ID in a card or roster never reaches the log", () => {
    const card = { puuid: "p-1", name: "Mara", tag: "7788", level: 212, nested: { name: "Sable#1234" } };
    const line = formatError("page", card);
    expect(line).toBe("page: [object Object] [puuid, name, tag, level, nested]");
    expect(line).not.toMatch(/Mara|7788|Sable|1234|212/);
    // A roster: an array of cards, and an object keyed by a name.
    expect(formatError("page", [card])).toBe("page: [object Array]");
    expect(formatError("page", { "Mara#7788": 1, ok: 2 })).toBe("page: [object Object] [ok]"); // a key that is no identifier is left out
    expect(formatError("page", { message: 5, name: "Mara" })).toBe("page: [object Object] [message, name]"); // a message that is no string is not used
    expect(formatError("page", {})).toBe("page: [object Object]");
    // At most 8 key names.
    const wide = Object.fromEntries(Array.from({ length: 12 }, (_, i) => [`k${i}`, i]));
    expect(formatError("page", wide)).toBe("page: [object Object] [k0, k1, k2, k3, k4, k5, k6, k7]");
    // Its own toString (which could print the whole card) is never called.
    const printing = { name: "Mara", toString: () => "Mara#7788" };
    expect(formatError("page", printing)).not.toMatch(/Mara#7788/);
  });

  it("a value that cannot be inspected is still one line, never an exception", () => {
    const hostile = {
      toString() {
        throw new Error("nope");
      },
    };
    expect(() => formatError("x", hostile)).not.toThrow();
    expect(formatError("x", hostile)).toBe("x: [object Object] [toString]");
    expect(formatError("x", Object.create(null))).toBe("x: [object Object]");
    expect(formatError("x", Symbol("s"))).toMatch(/^x: /);
    // A throwing getter for the message, and a Proxy whose every trap throws.
    const getter = {
      get message(): string {
        throw new Error("no message for you");
      },
    };
    expect(formatError("x", getter)).toBe("x: unprintable error");
    const proxy = new Proxy(
      {},
      {
        get() {
          throw new Error("trap");
        },
        ownKeys() {
          throw new Error("trap");
        },
      },
    );
    expect(formatError("x", proxy)).toBe("x: unprintable error");
    // An Error whose stack throws keeps its name and message, just without the frame.
    const e = new Error("m");
    Object.defineProperty(e, "stack", {
      get() {
        throw new Error("no stack for you");
      },
    });
    expect(formatError("x", e)).toBe("x: Error: m");
  });

  it("newlines and runs of whitespace collapse to single spaces", () => {
    expect(formatError("x", "line one\n\nline   two\t\tthree\r\nfour")).toBe("x: line one line two three four");
    expect(formatError("x", withStack("a\nb", "TypeError: a\nb\n    at f (http://h/a.js:1:2)"))).toBe("x: TypeError: a b @ /a.js:1:2");
  });

  it("long text is capped at 500 characters, ending in an ellipsis", () => {
    const line = formatError("x", "a".repeat(2000));
    expect(line).toHaveLength(MAX_LINE);
    expect(line.endsWith("…")).toBe(true);
    expect(line.startsWith("x: aaaa")).toBe(true);
    // Exactly at the cap: untouched.
    const exact = "y".repeat(MAX_LINE - 3);
    expect(formatError("x", exact)).toBe(`x: ${exact}`);
    expect(formatError("x", exact)).toHaveLength(MAX_LINE);
  });

  it("a long message is cut, not the frame: `kind: Name: message… @ path:line:col` still fits 500 chars", () => {
    const e = withStack("m".repeat(2000), "TypeError: mmm\n    at f (http://h/assets/index-Ab12.js?v=3:10:5)");
    const line = formatError("page", e);
    expect(line).toHaveLength(MAX_LINE);
    expect(line.startsWith("page: TypeError: mmmm")).toBe(true);
    expect(line.endsWith("m… @ /assets/index-Ab12.js:10:5")).toBe(true);
    // One char over the cap: only the message gives way.
    const edge = withStack("m".repeat(MAX_LINE - "page: TypeError: ".length - " @ /a.js:1:2".length + 1), "TypeError: m\n    at http://h/a.js:1:2");
    const cut = formatError("page", edge);
    expect(cut).toHaveLength(MAX_LINE);
    expect(cut.endsWith("m… @ /a.js:1:2")).toBe(true);
    // A frame that leaves no room for the message gives up the frame, never the cap.
    const huge = withStack("m".repeat(900), `TypeError: m\n    at http://h/${"d/".repeat(300)}a.js:1:2`);
    expect(formatError("page", huge)).toHaveLength(MAX_LINE);
  });

  it("the cut never leaves half a surrogate pair before the ellipsis", () => {
    // `x: ` is 3 chars; 495 more fill up to index 498, so the emoji's two halves straddle the cut at 499.
    const line = formatError("x", "a".repeat(495) + "😀" + "b".repeat(50));
    expect(line).toHaveLength(MAX_LINE - 1);
    expect(line.endsWith("a…")).toBe(true);
  });
});

describe("createReporter", () => {
  let t: number;
  let sent: Array<[string, Record<string, unknown>]>;
  let tauri: boolean;
  let reject: boolean;
  let deps: ReporterDeps;

  beforeEach(() => {
    t = 1_000_000;
    sent = [];
    tauri = true;
    reject = false;
    deps = {
      invoke: (cmd, args) => {
        sent.push([cmd, args]);
        return reject ? Promise.reject(new Error("ui_log is gone")) : Promise.resolve();
      },
      inTauri: () => tauri,
      now: () => t,
    };
    // The console sink is a development-build feature: pin it on, whatever mode the runner is in.
    vi.stubEnv("DEV", true);
    vi.spyOn(console, "error").mockImplementation(() => {});
    vi.spyOn(console, "warn").mockImplementation(() => {});
  });
  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllEnvs();
  });

  it("under Tauri: invoke('ui_log', { level, message }) with the formatted line, level error by default", () => {
    const report = createReporter(deps);
    report("page", "boom");
    expect(sent).toEqual([["ui_log", { level: "error", message: "page: boom" }]]);
  });

  it("the level is passed through", () => {
    const report = createReporter(deps);
    report("a", "x", "warn");
    report("b", "y", "info");
    expect(sent.map(([, a]) => a.level)).toEqual(["warn", "info"]);
  });

  it("development builds always print to the console, with the real error object", () => {
    const report = createReporter(deps);
    const err = new Error("boom");
    report("page", err);
    expect(console.error).toHaveBeenCalledWith("page", err);
    tauri = false;
    report("page", err);
    expect(console.error).toHaveBeenCalledTimes(2);
  });

  it("a release build (import.meta.env.DEV off) never touches the console, even when the send fails", async () => {
    vi.stubEnv("DEV", false);
    reject = true;
    const report = createReporter(deps);
    report("page", "boom");
    await new Promise((r) => setTimeout(r, 10));
    expect(console.error).not.toHaveBeenCalled();
    expect(console.warn).not.toHaveBeenCalled();
    expect(sent).toHaveLength(1);
  });

  it("not in Tauri (the DEV browser): nothing is sent", () => {
    tauri = false;
    const report = createReporter(deps);
    report("page", "boom");
    expect(sent).toEqual([]);
  });

  it("no invoke given: nothing is sent and nothing throws", () => {
    const report = createReporter({ inTauri: () => true, now: () => t });
    expect(() => report("page", "boom")).not.toThrow();
  });

  it("an identical line within 10 s is dropped, a different one goes out, and it is sent again after 10 s", () => {
    const report = createReporter(deps);
    report("page", "boom");
    t += 1_000;
    report("page", "boom"); // same line
    report("page", "boom", "warn"); // same text at another level is another line
    report("page", "other"); // another line
    expect(sent.map(([, a]) => `${a.level} ${a.message}`)).toEqual(["error page: boom", "warn page: boom", "error page: other"]);
    t += DEDUPE_MS - 1_001; // 9 999 ms after the first send
    report("page", "boom");
    expect(sent).toHaveLength(3);
    t += 1; // 10 000 ms
    report("page", "boom");
    expect(sent).toHaveLength(4);
  });

  it("the window is counted from the send, not from the last drop: a failure that repeats every second goes out every 10 s", () => {
    const report = createReporter(deps);
    for (let i = 0; i < 25; i++) {
      report("page", "boom");
      t += 1_000;
    }
    expect(sent).toHaveLength(3); // at 0 s, 10 s and 20 s
  });

  it("two errors that format to the same line count as identical (the dedupe is on the line)", () => {
    const report = createReporter(deps);
    report("page", withStack("same", "TypeError: same")); // no stack frame, so the line is `page: TypeError: same`
    report("page", "TypeError: same");
    expect(sent).toHaveLength(1);
  });

  it("at most 20 lines per 10 s: the rest are dropped without a trace, and sending resumes as the window moves", () => {
    const report = createReporter(deps);
    for (let i = 0; i < 30; i++) report("page", `distinct ${i}`); // all at the same instant
    expect(sent).toHaveLength(MAX_SENDS);
    expect(sent.map(([, a]) => a.message)).toEqual(Array.from({ length: MAX_SENDS }, (_, i) => `page: distinct ${i}`));
    t += DEDUPE_MS - 1;
    report("page", "late");
    expect(sent).toHaveLength(MAX_SENDS); // still inside the window of the first 20
    t += 1; // the first 20 are 10 s old now
    report("page", "late");
    expect(sent).toHaveLength(MAX_SENDS + 1);
    // A dropped line was never "sent": once there is room it goes out.
    report("page", "distinct 25");
    expect(sent[sent.length - 1][1].message).toBe("page: distinct 25");
  });

  it("the cap is a rolling window, not a bucket that refills: sends spread over time count together", () => {
    const report = createReporter(deps);
    for (let i = 0; i < 40; i++) {
      report("page", `line ${i}`);
      t += 1_000; // one distinct line a second: never more than 10 inside a window, so none is dropped
    }
    expect(sent).toHaveLength(40);
    // Now: the sends from 1 s to 9 s ago are still inside the window (9 of them; the one 10 s ago has expired).
    for (let i = 0; i < 40; i++) report("page", `burst ${i}`);
    expect(sent).toHaveLength(40 + (MAX_SENDS - 9));
  });

  it("a clock set back does not hold the dedupe or the cap for hours: entries from the future expire", () => {
    const report = createReporter(deps);
    for (let i = 0; i < MAX_SENDS; i++) report("page", `distinct ${i}`);
    report("page", "dup");
    expect(sent).toHaveLength(MAX_SENDS); // the cap is full
    t -= 3_600_000; // the clock jumps back an hour
    report("page", "distinct 0"); // same line as an earlier one, but that was "an hour from now"
    expect(sent).toHaveLength(MAX_SENDS + 1);
    report("page", "distinct 0"); // and now it dedupes normally again
    expect(sent).toHaveLength(MAX_SENDS + 1);
  });

  it("a rejected invoke is swallowed: nothing thrown, no unhandled rejection of its own, one dev console warning", async () => {
    reject = true;
    const unhandled: unknown[] = [];
    const onUnhandled = (e: unknown) => unhandled.push(e);
    process.on("unhandledRejection", onUnhandled);
    try {
      const report = createReporter(deps);
      expect(() => report("page", "boom")).not.toThrow();
      await new Promise((r) => setTimeout(r, 10));
      expect(unhandled).toEqual([]);
      expect(sent).toHaveLength(1);
      expect(console.warn).toHaveBeenCalledTimes(1);
      expect(console.warn).toHaveBeenCalledWith("ui_log failed", expect.objectContaining({ message: "ui_log is gone" }));
    } finally {
      process.off("unhandledRejection", onUnhandled);
    }
  });

  it("an invoke that throws at once, and an inTauri that throws, are swallowed too", () => {
    const boom = () => {
      throw new Error("bridge missing");
    };
    expect(() => createReporter({ ...deps, invoke: boom })("page", "x")).not.toThrow();
    expect(() => createReporter({ ...deps, inTauri: boom })("page", "x")).not.toThrow();
    expect(() => createReporter({ ...deps, now: boom })("page", "x")).not.toThrow();
    // A console that throws is no reason to throw either.
    vi.mocked(console.warn).mockImplementation(boom);
    vi.mocked(console.error).mockImplementation(boom);
    expect(() => createReporter({ ...deps, invoke: boom })("page", "x")).not.toThrow();
  });

  it("a hostile error value is reported as a line, not thrown", () => {
    const report = createReporter(deps);
    report("page", {
      toString() {
        throw new Error("nope");
      },
    });
    expect(sent).toHaveLength(1);
    expect(sent[0][1].message).toBe("page: [object Object] [toString]");
  });

  it("a card or roster object that crashed a page sends its shape, not a Riot ID", () => {
    const report = createReporter(deps);
    report("page", { puuid: "p-1", name: "Mara", tag: "7788" });
    report("page", [{ name: "Sable", tag: "1234" }]);
    expect(sent.map(([, a]) => a.message)).toEqual(["page: [object Object] [puuid, name, tag]", "page: [object Array]"]);
  });
});

describe("installErrorReporting", () => {
  /** A stand-in for `window`: the real EventTarget, dispatching real (plain) events carrying what the browser's carry. */
  const fakeWindow = () => new EventTarget() as unknown as Window;
  const fire = (w: Window, type: string, props: Record<string, unknown>) => w.dispatchEvent(Object.assign(new Event(type), props));

  it("an uncaught error is reported as kind window, with the Error object", () => {
    const w = fakeWindow();
    const report = vi.fn();
    installErrorReporting(w, report);
    const err = new Error("boom");
    fire(w, "error", { error: err, message: "Uncaught Error: boom" });
    expect(report).toHaveBeenCalledTimes(1);
    expect(report).toHaveBeenCalledWith("window", err);
  });

  it("an error event with no error object (a cross-origin script, a ResizeObserver notice) reports its message", () => {
    const w = fakeWindow();
    const report = vi.fn();
    installErrorReporting(w, report);
    fire(w, "error", { error: null, message: "Script error." });
    expect(report).toHaveBeenCalledWith("window", "Script error.");
  });

  it("an unhandled rejection is reported as kind promise, with its reason, whatever that is", () => {
    const w = fakeWindow();
    const report = vi.fn();
    installErrorReporting(w, report);
    const reason = new Error("nobody caught me");
    fire(w, "unhandledrejection", { reason });
    fire(w, "unhandledrejection", { reason: "plain string" });
    fire(w, "unhandledrejection", { reason: undefined });
    expect(report.mock.calls).toEqual([
      ["promise", reason],
      ["promise", "plain string"],
      ["promise", undefined],
    ]);
  });

  it("installing twice on the same window adds the listeners once (each event is reported once)", () => {
    const w = fakeWindow();
    const report = vi.fn();
    installErrorReporting(w, report);
    installErrorReporting(w, report);
    installErrorReporting(w, vi.fn()); // even with another reporter: the first install stands
    fire(w, "error", { error: new Error("a"), message: "a" });
    fire(w, "unhandledrejection", { reason: "b" });
    expect(report).toHaveBeenCalledTimes(2);
  });

  it("each window is installed on its own", () => {
    const a = fakeWindow();
    const b = fakeWindow();
    const report = vi.fn();
    installErrorReporting(a, report);
    installErrorReporting(b, report);
    fire(a, "unhandledrejection", { reason: 1 });
    fire(b, "unhandledrejection", { reason: 2 });
    expect(report).toHaveBeenCalledTimes(2);
  });

  it("leaves the events alone: the browser's own handling of them is not cancelled", () => {
    const w = fakeWindow();
    installErrorReporting(w, vi.fn());
    const ev = Object.assign(new Event("unhandledrejection", { cancelable: true }), { reason: "x" });
    w.dispatchEvent(ev);
    expect(ev.defaultPrevented).toBe(false);
  });
});

describe("uilog-install (the side-effect module main.ts imports first)", () => {
  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
    vi.unstubAllEnvs();
    vi.resetModules();
  });

  it("importing it puts both listeners on the window, with the app's own reporter", async () => {
    vi.resetModules();
    vi.stubEnv("DEV", true);
    vi.spyOn(console, "error").mockImplementation(() => {});
    const w = new EventTarget();
    vi.stubGlobal("window", w);
    await import("./uilog-install");
    const err = new Error("boom");
    w.dispatchEvent(Object.assign(new Event("error"), { error: err, message: "boom" }));
    w.dispatchEvent(Object.assign(new Event("unhandledrejection"), { reason: "late" }));
    // Not under Tauri here, so only the development console sees them; the point is that the listeners are there.
    expect(console.error).toHaveBeenCalledWith("window", err);
    expect(console.error).toHaveBeenCalledWith("promise", "late");
  });
});
