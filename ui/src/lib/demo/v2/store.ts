// DEV-only (R189): the v2 commands answered in the browser, with no Rust. Loaded through `api.ts`'s DEV-guarded
// dynamic import, so `vite build` drops it. One in-memory copy per page load: saving a player, a note, a setting or an
// ASCII drawing works until reload, like the real app's DB would. Scene-specific data lives in the sibling files.
import type { AppStatus, AsciiArt, PlayerMarks, Settings } from "../../types";
import { DEMO_NOW, PUUID } from "../cdv8";

type Listener<T> = (v: T) => void;

/** A tiny event channel per v2 event, standing in for Tauri's `listen`. */
function channel<T>() {
  const ls = new Set<Listener<T>>();
  return {
    listen(cb: Listener<T>): () => void {
      ls.add(cb);
      return () => ls.delete(cb);
    },
    emit(v: T): void {
      for (const l of ls) l(v);
    },
  };
}

export const statusChannel = channel<AppStatus>();
export const marksChannel = channel<PlayerMarks>();
export const settingsChannel = channel<Settings>();

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

export const state = {
  settings: { ...DEFAULT_SETTINGS } as Settings,
  /** The design's sample: Mara saved with a note (TrackerApp.dc.html `P('mara', …)`). */
  marks: {
    saved: [PUUID.mara!, PUUID.kilo!],
    notes: { [PUUID.mara!]: "Entry every round, dry-peeks A long" },
    notSmurf: [],
  } as PlayerMarks,
  savedAt: new Map<string, number>([
    [PUUID.mara!, DEMO_NOW - 12 * 86_400_000],
    [PUUID.kilo!, DEMO_NOW - 30 * 86_400_000],
  ]),
  ascii: [] as AsciiArt[],
  nextAsciiId: 1,
};

export function cloneMarks(): PlayerMarks {
  return { saved: [...state.marks.saved], notes: { ...state.marks.notes }, notSmurf: [...state.marks.notSmurf] };
}
