import { get } from "svelte/store";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { PlayerMarks, Settings } from "./types";

const api = vi.hoisted(() => ({
  saveSettings: vi.fn<(s: Settings) => Promise<Settings>>(),
  setNote: vi.fn<(puuid: string, note: string) => Promise<void>>(),
  setSaved: vi.fn<(puuid: string, saved: boolean) => Promise<void>>(),
  dismissSmurf: vi.fn<(puuid: string) => Promise<void>>(),
  playerMarks: vi.fn<() => Promise<PlayerMarks>>(),
}));
vi.mock("./api", () => api);

const {
  DEFAULT_SETTINGS,
  NOTE_MAX_CHARS,
  NOTE_RETRY_MS,
  applyMarks,
  applyStoredSettings,
  dismissSmurf,
  marks,
  setNote,
  settings,
  toggleSaved,
  updateSettings,
} = await import("./appdata.svelte");

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

  it("the same key changed back while its save is out: the newer value stays and is saved next", async () => {
    applyStoredSettings({ ...STORED, sound: false });
    const first = deferred<Settings>();
    api.saveSettings.mockImplementationOnce(() => first.promise).mockImplementation(async (s) => s);

    const a = updateSettings({ sound: true });
    await Promise.resolve();
    await Promise.resolve();
    const b = updateSettings({ sound: false });
    expect(api.saveSettings).toHaveBeenCalledTimes(1);

    // The first save answers with what it stored: sound on. The newer "off" is still pending.
    first.resolve({ ...STORED, sound: true });
    await a;
    expect(get(settings).sound).toBe(false);
    await b;
    expect(api.saveSettings.mock.calls.map((c) => c[0].sound)).toEqual([true, false]);
    expect(get(settings).sound).toBe(false);
  });
});

describe("a failed settings save", () => {
  afterEach(() => vi.clearAllMocks());

  it("puts the stored value back instead of showing one that never took effect", async () => {
    applyStoredSettings({ ...STORED, tray: true });
    api.saveSettings.mockRejectedValueOnce("settings not stored");
    const done = updateSettings({ tray: false });
    expect(get(settings).tray).toBe(false);
    await done;
    expect(get(settings).tray).toBe(true);
    // A later stored copy is not overlaid with the failed change either.
    applyStoredSettings({ ...STORED, tray: true, sound: true });
    expect(get(settings)).toMatchObject({ tray: true, sound: true });
  });
});

describe("notes", () => {
  afterEach(() => {
    vi.useRealTimers();
    vi.clearAllMocks();
  });

  const MARKS: PlayerMarks = { saved: ["p1"], notes: {}, notSmurf: [] };

  it("a note is cut at Rust's limit (code points), so what shows is what is stored", async () => {
    vi.useFakeTimers();
    api.setNote.mockResolvedValue(undefined);
    applyMarks(MARKS);
    const long = "😀".repeat(NOTE_MAX_CHARS + 5);
    setNote("p2", long);
    expect([...get(marks).notes.p2!].length).toBe(NOTE_MAX_CHARS);
    await vi.advanceTimersByTimeAsync(400);
    expect([...api.setNote.mock.calls[0]![1]].length).toBe(NOTE_MAX_CHARS);
  });

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

  it("an echo that lands after its own write answered does not undo what was typed after it", async () => {
    vi.useFakeTimers();
    const write = deferred<void>();
    api.setNote.mockImplementationOnce(() => write.promise).mockResolvedValue(undefined);
    applyMarks(MARKS);

    setNote("p1", "abc");
    await vi.advanceTimersByTimeAsync(400);
    setNote("p1", "abcd");
    // The first write answers, then Rust's event for it arrives: "abc". "abcd" is not written yet.
    write.resolve();
    await vi.advanceTimersByTimeAsync(0);
    applyMarks({ ...MARKS, notes: { p1: "abc" } });
    expect(get(marks).notes.p1).toBe("abcd");
    await vi.advanceTimersByTimeAsync(400);
    expect(api.setNote).toHaveBeenLastCalledWith("p1", "abcd");
  });
});

describe("a failed mark write", () => {
  afterEach(() => {
    vi.useRealTimers();
    vi.clearAllMocks();
  });

  // Rust emits `marks` only after a write that worked: nothing else would take the optimistic change back.
  const STORED_MARKS: PlayerMarks = { saved: ["p1"], notes: {}, notSmurf: [] };

  it("set_saved: Rust's copy is read back, so the player does not show as saved", async () => {
    applyMarks(STORED_MARKS);
    api.setSaved.mockRejectedValueOnce("mark not stored");
    api.playerMarks.mockResolvedValueOnce(STORED_MARKS);
    const done = toggleSaved("p2", true);
    expect(get(marks).saved).toEqual(["p2", "p1"]);
    await done;
    await vi.waitFor(() => expect(get(marks).saved).toEqual(["p1"]));
  });

  it("dismiss_smurf: Rust's copy is read back, so the flag shows again", async () => {
    applyMarks(STORED_MARKS);
    api.dismissSmurf.mockRejectedValueOnce("mark not stored");
    api.playerMarks.mockResolvedValueOnce(STORED_MARKS);
    await dismissSmurf("p2");
    await vi.waitFor(() => expect(get(marks).notSmurf).toEqual([]));
  });

  it("set_note: the text stays through a later event and is written again", async () => {
    vi.useFakeTimers();
    applyMarks(STORED_MARKS);
    api.setNote.mockRejectedValueOnce("mark not stored").mockResolvedValue(undefined);
    setNote("p1", "throws");
    await vi.advanceTimersByTimeAsync(400);
    expect(api.setNote).toHaveBeenCalledTimes(1);
    // Another mark change's event (Rust's copy, without the note) does not wipe it.
    applyMarks({ ...STORED_MARKS, saved: ["p3", "p1"] });
    expect(get(marks).notes.p1).toBe("throws");
    await vi.advanceTimersByTimeAsync(NOTE_RETRY_MS);
    expect(api.setNote).toHaveBeenCalledTimes(2);
    expect(api.setNote).toHaveBeenLastCalledWith("p1", "throws");
    // Stored now: Rust's copy counts again.
    applyMarks({ ...STORED_MARKS, notes: { p1: "from rust" } });
    expect(get(marks).notes.p1).toBe("from rust");
  });
});

describe("mark writes", () => {
  afterEach(() => {
    vi.useRealTimers();
    vi.clearAllMocks();
  });

  it("go out one at a time, in order: Save then Cancel never reaches Rust together", async () => {
    vi.useFakeTimers();
    applyMarks({ saved: [], notes: {}, notSmurf: [] });
    const first = deferred<void>();
    api.setSaved.mockImplementationOnce(() => first.promise).mockResolvedValue(undefined);
    api.setNote.mockResolvedValue(undefined);
    api.dismissSmurf.mockResolvedValue(undefined);

    const save = toggleSaved("p1", true);
    const cancel = toggleSaved("p1", false);
    setNote("p1", "lurks");
    const dismiss = dismissSmurf("p1");
    await vi.advanceTimersByTimeAsync(400);
    // Only the first write is out while it has not answered.
    expect(api.setSaved.mock.calls).toEqual([["p1", true]]);
    expect(api.setNote).not.toHaveBeenCalled();
    expect(api.dismissSmurf).not.toHaveBeenCalled();

    first.resolve();
    await Promise.all([save, cancel, dismiss]);
    await vi.advanceTimersByTimeAsync(0);
    expect(api.setSaved.mock.calls).toEqual([
      ["p1", true],
      ["p1", false],
    ]);
    expect(api.setSaved.mock.invocationCallOrder[1]!).toBeLessThan(api.dismissSmurf.mock.invocationCallOrder[0]!);
    expect(api.setNote).toHaveBeenCalledWith("p1", "lurks");
  });
});
