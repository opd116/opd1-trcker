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

export async function toggleSaved(puuid: string, on?: boolean): Promise<void> {
  const next = on ?? !isSaved(get(marks), puuid);
  optimistic((m) => ({ ...m, saved: next ? [puuid, ...m.saved.filter((p) => p !== puuid)] : m.saved.filter((p) => p !== puuid) }));
  try {
    await api.setSaved(puuid, next);
  } catch (err) {
    reportError("set_saved", err);
  }
}

const noteTimers = new Map<string, ReturnType<typeof setTimeout>>();

/** "Saves as you type": the store changes at once, the write goes out 400ms after the last keystroke. */
export function setNote(puuid: string, note: string, delayMs = 400): void {
  optimistic((m) => {
    if (note.trim() === "") delete m.notes[puuid];
    else m.notes[puuid] = note;
    return m;
  });
  clearTimeout(noteTimers.get(puuid));
  noteTimers.set(
    puuid,
    setTimeout(() => {
      noteTimers.delete(puuid);
      api.setNote(puuid, note).catch((err) => reportError("set_note", err));
    }, delayMs),
  );
}

export async function dismissSmurf(puuid: string): Promise<void> {
  optimistic((m) => ({ ...m, notSmurf: [...m.notSmurf, puuid] }));
  try {
    await api.dismissSmurf(puuid);
  } catch (err) {
    reportError("dismiss_smurf", err);
  }
}

/** Changes one setting; the store changes at once and the stored copy follows. */
export async function updateSettings(patch: Partial<Settings>): Promise<void> {
  const next = { ...get(settings), ...patch };
  settings.set(next);
  try {
    settings.set(await api.saveSettings(next));
  } catch (err) {
    reportError("set_settings", err);
  }
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

/** Connects the three stores. Call once at startup, after `startFeeds`. */
export async function startAppData(): Promise<void> {
  if (started) return;
  started = true;

  if (import.meta.env.DEV && !isTauri()) {
    const demo = await import("./demo/v2");
    demo.listenMarks((m) => marks.set(m));
    demo.listenSettings((s) => settings.set(s));
    demo.listenStatus((s) => appStatus.set(s));
    // The demo's status follows its scene.
    live.subscribe((l) => demo.pushStatus(l));
    marks.set(demo.v2Marks());
    settings.set(demo.v2GetSettings());
    return;
  }

  await Promise.all([
    follow<AppStatus>((cb) => listen<AppStatus>("status", (e) => cb(e.payload)), () => api.appStatus(), (s) => appStatus.set(s), "status"),
    follow<PlayerMarks>((cb) => listen<PlayerMarks>("marks", (e) => cb(e.payload)), api.playerMarks, (m) => marks.set(m), "marks"),
    follow<Settings>((cb) => listen<Settings>("settings", (e) => cb(e.payload)), api.getSettings, (s) => settings.set(s), "settings"),
  ]);
}
