// The v2 redesign's shared stores (R189): the connection status, the stored settings and the player marks (saved,
// notes, dismissed smurf flags). Each follows the `startLive` pattern: listen first, then ask for the snapshot, and the
// snapshot counts only when no event came before it. In the DEV browser the demo channels stand in for Tauri events.
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { derived, get, writable } from "svelte/store";
import * as api from "./api";
import { live } from "./stores";
import type { AppStatus, LiveState, PlayerMarks, Settings } from "./types";
import { reportError } from "./uilog";

/** null until the first answer. */
export const appStatus = writable<AppStatus | null>(null);

/** When the current status arrived: its `retryInMs` counts from then (an unchanged status is not sent again). */
let statusArrivedMs = 0;
export const statusArrivedAt = (): number => statusArrivedMs;

/**
 * "Retry now": Rust's next automatic retry counts from the click, but a status that stays the same is not sent again,
 * so the countdown is restarted here. (A slow failed connect still drifts: Rust counts from the failure, not the click.)
 */
export function retryCountFromNow(): void {
  statusArrivedMs = Date.now();
}

/** A status from Rust (snapshot or event), stamped with its arrival so every view counts the same retry down. */
export function applyStatus(s: AppStatus): void {
  statusArrivedMs = Date.now();
  appStatus.set(s);
}

/** The design's defaults until the stored copy arrives, so nothing waits on it. */
export const DEFAULT_SETTINGS: Settings = {
  highlight: true,
  enemyFirst: false,
  smurf: "cautious",
  savedAlert: true,
  sound: false,
  onTop: true,
  boot: true,
  tray: true,
};
export const settings = writable<Settings>({ ...DEFAULT_SETTINGS });

export const EMPTY_MARKS: PlayerMarks = { saved: [], notes: {}, notSmurf: [] };
export const marks = writable<PlayerMarks>(EMPTY_MARKS);

/** Saved puuids as a set, for per-row lookups. */
export const savedSet = derived(marks, (m) => new Set(m.saved));

export const isSaved = (m: PlayerMarks, puuid: string): boolean => m.saved.includes(puuid);
export const noteOf = (m: PlayerMarks, puuid: string): string => m.notes[puuid] ?? "";

/** Saved players in the current live match: the Saved nav badge. */
export const savedInMatch = derived([marks, live], ([m, l]: [PlayerMarks, LiveState | null]) => {
  if (!l || (l.stage !== "pregame" && l.stage !== "ingame")) return 0;
  const s = new Set(m.saved);
  return l.players.filter((p) => !p.isSelf && s.has(p.puuid)).length;
});

/** Applies a marks change at once (the UI never waits on the round trip), then lets Rust's `marks` event confirm it. */
function optimistic(f: (m: PlayerMarks) => PlayerMarks): void {
  marks.update((m) => f({ saved: [...m.saved], notes: { ...m.notes }, notSmurf: [...m.notSmurf] }));
}

/**
 * A mark write failed: Rust sends no `marks` event then, so the optimistic change would stay on screen as if stored.
 * Rust's own copy is read back instead.
 */
function resyncMarks(): Promise<void> {
  return api
    .playerMarks()
    .then(applyMarks)
    .catch((err) => reportError("player_marks", err));
}

/**
 * Mark writes go out one at a time, in order, as settings saves do: Rust runs concurrent writes in no set order, so
 * two quick ones (Save, then Cancel) could be stored and echoed the wrong way round. Each step (the write, and its
 * read-back when it failed) has answered before the next is sent.
 */
let markWrites: Promise<void> = Promise.resolve();
function queueMark(step: () => Promise<void>): Promise<void> {
  const run = markWrites.then(step);
  // A step that throws must not stop every later write.
  markWrites = run.catch(() => {});
  return run;
}

export function toggleSaved(puuid: string, on?: boolean): Promise<void> {
  const next = on ?? !isSaved(get(marks), puuid);
  optimistic((m) => ({ ...m, saved: next ? [puuid, ...m.saved.filter((p) => p !== puuid)] : m.saved.filter((p) => p !== puuid) }));
  return queueMark(async () => {
    try {
      await api.setSaved(puuid, next);
    } catch (err) {
      reportError("set_saved", err);
      await resyncMarks();
    }
  });
}

const noteTimers = new Map<string, ReturnType<typeof setTimeout>>();

/** Rust's `NOTE_MAX_CHARS` (crates/core/src/db/marks.rs): a longer note is cut there. The textareas stop at it. */
export const NOTE_MAX_CHARS = 2000;

/**
 * Notes typed here that Rust has not confirmed yet (waiting for the 400ms timer, or written and not answered). A
 * `marks` event can carry an older copy of such a note (the echo of an earlier write): the typed text wins until its
 * own write has answered, so a keystroke is never undone.
 */
const typedNotes = new Map<string, string>();

function withNote(m: PlayerMarks, puuid: string, note: string): void {
  if (note.trim() === "") delete m.notes[puuid];
  else m.notes[puuid] = note;
}

/** A `marks` snapshot or event from Rust, with the notes still being typed laid over it. */
export function applyMarks(m: PlayerMarks): void {
  if (typedNotes.size === 0) {
    marks.set(m);
    return;
  }
  const next: PlayerMarks = { ...m, notes: { ...m.notes } };
  for (const [puuid, note] of typedNotes) withNote(next, puuid, note);
  marks.set(next);
}

/** A note whose write failed is written again after this long (it stays on screen, typed, meanwhile). */
export const NOTE_RETRY_MS = 5_000;

/** "Saves as you type": the store changes at once, the write goes out 400ms after the last keystroke. */
export function setNote(puuid: string, typed: string, delayMs = 400): void {
  // Code points, as Rust's cut: what shows is what is stored.
  const chars = [...typed];
  const note = chars.length > NOTE_MAX_CHARS ? chars.slice(0, NOTE_MAX_CHARS).join("") : typed;
  typedNotes.set(puuid, note);
  optimistic((m) => {
    withNote(m, puuid, note);
    return m;
  });
  clearTimeout(noteTimers.get(puuid));
  noteTimers.set(
    puuid,
    setTimeout(() => {
      noteTimers.delete(puuid);
      void queueMark(() =>
        api.setNote(puuid, note).then(
          () => {
            // Answered: Rust's copy is this text now, unless more was typed meanwhile.
            if (typedNotes.get(puuid) === note) typedNotes.delete(puuid);
          },
          (err) => {
            reportError("set_note", err);
            // Not stored: the text stays typed (no event wipes it) and is written again, unless newer text is queued.
            if (typedNotes.get(puuid) === note && !noteTimers.has(puuid)) setNote(puuid, note, NOTE_RETRY_MS);
          },
        ),
      );
    }, delayMs),
  );
}

export function dismissSmurf(puuid: string): Promise<void> {
  optimistic((m) => ({ ...m, notSmurf: [...m.notSmurf, puuid] }));
  return queueMark(async () => {
    try {
      await api.dismissSmurf(puuid);
    } catch (err) {
      reportError("dismiss_smurf", err);
      await resyncMarks();
    }
  });
}

/** Setting changes not stored yet: laid over every stored copy that arrives, until their own save answers. */
let pendingSettings: Partial<Settings> = {};
let settingsLoaded!: () => void;
/** Resolves once the stored copy has been applied (or could not be read): a save before it would store defaults. */
const settingsReady = new Promise<void>((resolve) => (settingsLoaded = resolve));
/** Saves go out one at a time, in order, so an older write never lands over a newer one. */
let saving: Promise<void> = Promise.resolve();

/** The last copy Rust stored (what a failed save falls back to). */
let lastStoredSettings: Settings | null = null;

/** The stored settings from Rust (snapshot, event or a save's answer), with the changes not stored yet on top. */
export function applyStoredSettings(s: Settings): void {
  lastStoredSettings = s;
  settings.set({ ...s, ...pendingSettings });
  settingsLoaded();
}

/**
 * Changes one setting; the store changes at once and the stored copy follows. Before the stored copy has loaded the
 * change waits for it (it is laid over it, never over the defaults); quick changes are saved in order.
 */
export function updateSettings(patch: Partial<Settings>): Promise<void> {
  pendingSettings = { ...pendingSettings, ...patch };
  settings.update((s) => ({ ...s, ...patch }));
  saving = saving.then(async () => {
    await settingsReady;
    const sent = pendingSettings;
    if (Object.keys(sent).length === 0) return; // an earlier save in the queue carried it
    // The keys this save carried leave the pending set either way (unless changed again meanwhile: that newer change
    // has its own queued save).
    const settle = () => {
      const rest = { ...pendingSettings };
      for (const k of Object.keys(sent) as (keyof Settings)[]) if (rest[k] === sent[k]) delete rest[k];
      pendingSettings = rest;
    };
    try {
      const stored = await api.saveSettings(get(settings));
      settle();
      applyStoredSettings(stored);
    } catch (err) {
      reportError("set_settings", err);
      // Not stored, so not applied either: the toggle goes back to what Rust has, instead of showing a value that
      // never took effect.
      settle();
      settings.set({ ...(lastStoredSettings ?? DEFAULT_SETTINGS), ...pendingSettings });
    }
  });
  return saving;
}

/** listen → snapshot, the snapshot only when no event came first. */
async function follow<T>(
  listenTo: (cb: (v: T) => void) => Promise<unknown> | unknown,
  snapshot: () => Promise<T>,
  apply: (v: T) => void,
  what: string,
): Promise<void> {
  let sawEvent = false;
  try {
    await listenTo((v) => {
      sawEvent = true;
      apply(v);
    });
  } catch (err) {
    reportError(`${what} event`, err);
  }
  try {
    const v = await snapshot();
    if (!sawEvent) apply(v);
  } catch (err) {
    reportError(what, err);
  }
}

let started = false;

/** Connects the three stores. Call once at startup (with `startFeeds`, which it does not wait for). */
export async function startAppData(): Promise<void> {
  if (started) return;
  started = true;

  if (import.meta.env.DEV && !isTauri()) {
    const demo = await import("./demo/v2");
    demo.listenMarks(applyMarks);
    demo.listenSettings(applyStoredSettings);
    demo.listenStatus(applyStatus);
    // The demo's status follows its scene.
    live.subscribe((l) => demo.pushStatus(l));
    applyMarks(demo.v2Marks());
    applyStoredSettings(demo.v2GetSettings());
    return;
  }

  await Promise.all([
    follow<AppStatus>((cb) => listen<AppStatus>("status", (e) => cb(e.payload)), () => api.appStatus(), applyStatus, "status"),
    follow<PlayerMarks>((cb) => listen<PlayerMarks>("marks", (e) => cb(e.payload)), api.playerMarks, applyMarks, "marks"),
    follow<Settings>((cb) => listen<Settings>("settings", (e) => cb(e.payload)), api.getSettings, applyStoredSettings, "settings"),
  ]);
  // Unreadable settings: changes are saved over the defaults, as before.
  settingsLoaded();
}
