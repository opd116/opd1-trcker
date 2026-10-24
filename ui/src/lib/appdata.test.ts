import { get } from "svelte/store";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { PlayerMarks, Settings } from "./types";

const api = vi.hoisted(() => ({
  saveSettings: vi.fn<(s: Settings) => Promise<Settings>>(),
  setNote: vi.fn<(puuid: string, note: string) => Promise<void>>(),
}));
vi.mock("./api", () => api);

const { DEFAULT_SETTINGS, applyMarks, applyStoredSettings, marks, setNote, settings, updateSettings } = await import("./appdata.svelte");

/** A promise with its resolve outside, to answer a call when the test says so. */
function deferred<T>() {
  let resolve!: (v: T) => void;
  const promise = new Promise<T>((r) => (resolve = r));
  return { promise, resolve };
}

const STORED: Settings = { ...DEFAULT_SETTINGS, boot: false, onTop: false };

describe("updateSettings", () => {
  afterEach(() => vi.clearAllMocks());

  it("a change before the stored copy loads waits for it and is laid over it, never over the defaults", async () => {
    api.saveSettings.mockImplementation(async (s) => s);
    const done = updateSettings({ highlight: false });
    expect(get(settings).highlight).toBe(false);
    await Promise.resolve();
    expect(api.saveSettings).not.toHaveBeenCalled();

    applyStoredSettings(STORED);
    expect(get(settings)).toMatchObject({ highlight: false, boot: false, onTop: false });
    await done;
    expect(api.saveSettings).toHaveBeenCalledTimes(1);
    expect(api.saveSettings.mock.calls[0][0]).toMatchObject({ highlight: false, boot: false, onTop: false });
  });

  it("quick changes are saved in order, and an older answer never undoes a newer change", async () => {
    applyStoredSettings(STORED);
    const first = deferred<Settings>();
    api.saveSettings.mockImplementationOnce(() => first.promise).mockImplementation(async (s) => s);

    const a = updateSettings({ sound: true });
    await Promise.resolve();
    await Promise.resolve();
    const b = updateSettings({ tray: false });
    expect(api.saveSettings).toHaveBeenCalledTimes(1);

    // The first save answers with what it stored (B not in it yet): B stays on screen.
    first.resolve({ ...STORED, highlight: false, sound: true });
    await a;
    expect(get(settings)).toMatchObject({ sound: true, tray: false });
    await b;
    expect(api.saveSettings).toHaveBeenCalledTimes(2);
    expect(api.saveSettings.mock.calls[1][0]).toMatchObject({ sound: true, tray: false });
    expect(get(settings)).toMatchObject({ sound: true, tray: false });
  });
});

describe("notes", () => {
  afterEach(() => {
    vi.useRealTimers();
    vi.clearAllMocks();
  });

  const MARKS: PlayerMarks = { saved: ["p1"], notes: {}, notSmurf: [] };

  it("an echo of an earlier write does not undo what was typed after it", async () => {
    vi.useFakeTimers();
    const write = deferred<void>();
    api.setNote.mockImplementationOnce(() => write.promise).mockResolvedValue(undefined);
    applyMarks(MARKS);

    setNote("p1", "abc");
    await vi.advanceTimersByTimeAsync(400);
    expect(api.setNote).toHaveBeenCalledWith("p1", "abc");
    setNote("p1", "abcd");
    // Rust's event for the first write arrives: "abc".
    applyMarks({ ...MARKS, notes: { p1: "abc" } });
    expect(get(marks).notes.p1).toBe("abcd");

    write.resolve();
    await vi.advanceTimersByTimeAsync(0);
    expect(get(marks).notes.p1).toBe("abcd");
    await vi.advanceTimersByTimeAsync(400);
    expect(api.setNote).toHaveBeenLastCalledWith("p1", "abcd");

    // Once its own write answered, Rust's copy counts again.
    applyMarks({ ...MARKS, notes: { p1: "from rust" } });
    expect(get(marks).notes.p1).toBe("from rust");
  });
});
