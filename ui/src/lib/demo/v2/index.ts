// DEV-only (R189): the v2 commands and events in the browser. `api.ts` reaches this only through a DEV-guarded
// dynamic import. `?empty=1` makes the list screens empty (the design's `variant: empty`).
import type {
  AppInfo,
  AppStatus,
  AsciiArt,
  Collection,
  HistoryPage,
  HistoryQueue,
  LiveState,
  PlayerMarks,
  ProfileExtras,
  SavedPlayer,
  Settings,
} from "../../types";
import { DEMO_NOW, MAPS, PUUID } from "../cdv8";
import { failNow } from "../scenes";
import { demoCollection } from "./collection";
import { demoHistory } from "./history";
import { demoSavedPlayers } from "./saved";
import { demoStatus } from "./status";
import { cloneMarks, marksChannel, settingsChannel, state, statusChannel } from "./store";

const q = () => new URLSearchParams(location.search);
const empty = () => q().get("empty") === "1";

export function v2Status(live: LiveState | null): AppStatus {
  return demoStatus(live, q());
}

export const listenStatus = statusChannel.listen;
export const listenMarks = marksChannel.listen;
export const listenSettings = settingsChannel.listen;

/** The demo feed calls this on every scene change, so the pill follows the scene. */
export function pushStatus(live: LiveState | null): void {
  statusChannel.emit(v2Status(live));
}

export function v2Marks(): PlayerMarks {
  if (empty()) return { saved: [], notes: {}, notSmurf: [] };
  return cloneMarks();
}

/** DEV: how often a screen asked for its rows (`data-history-asks` / `data-saved-asks` on <html>; Playwright reads it). */
function countAsk(key: "historyAsks" | "savedAsks"): void {
  const d = document.documentElement.dataset;
  d[key] = String(Number(d[key] ?? 0) + 1);
}

export function v2SavedPlayers(): SavedPlayer[] {
  countAsk("savedAsks");
  return empty() ? [] : demoSavedPlayers();
}

export function v2SetSaved(puuid: string, saved: boolean): void {
  const s = state.marks.saved.filter((p) => p !== puuid);
  if (saved) {
    s.unshift(puuid);
    state.savedAt.set(puuid, Date.now());
  }
  state.marks.saved = s;
  marksChannel.emit(cloneMarks());
}

export function v2SetNote(puuid: string, note: string): void {
  if (note.trim() === "") delete state.marks.notes[puuid];
  else state.marks.notes[puuid] = note;
  marksChannel.emit(cloneMarks());
}

export function v2DismissSmurf(puuid: string): void {
  if (!state.marks.notSmurf.includes(puuid)) state.marks.notSmurf.push(puuid);
  marksChannel.emit(cloneMarks());
}

export function v2History(queue: HistoryQueue, offset: number, limit: number): HistoryPage {
  countAsk("historyAsks");
  return demoHistory(queue, offset, limit, empty());
}

export function v2GetSettings(): Settings {
  return { ...state.settings };
}

export function v2SetSettings(s: Settings): Settings {
  state.settings = { ...s };
  settingsChannel.emit({ ...s });
  return { ...s };
}

export function v2AsciiList(): AsciiArt[] {
  return [...state.ascii];
}

export function v2AsciiSave(title: string, raw: string): AsciiArt {
  const art = { id: state.nextAsciiId++, title, raw, createdMs: Date.now() };
  state.ascii.unshift(art);
  return art;
}

export function v2AsciiDelete(id: number): void {
  state.ascii = state.ascii.filter((a) => a.id !== id);
}

export function v2AppInfo(): AppInfo {
  return {
    version: "0.4.0",
    commit: "2a5a381",
    buildMs: Date.UTC(2026, 9, 19),
    riotClientVersion: "98.0.1",
    valorantVersion: "release-11.08",
    dbMatches: 1284,
    dbBytes: 48 * 1024 * 1024,
  };
}

export function v2Collection(): Collection | null {
  if (failNow("collection")) throw new Error("demo: skins unavailable");
  return empty() ? null : demoCollection();
}

/** The design's per-map rows for a profile (`profile(id).maps`), and Nova in the user's last two recent matches. */
export function v2ProfileExtras(puuid: string): ProfileExtras {
  const row = (id: string, games: number, wins: number) => ({
    id,
    games,
    wins,
    losses: games - wins,
    winrate: wins / games,
    netRr: null,
    kd: 1.1,
    acs: 230,
  });
  return {
    puuid,
    maps: [row(MAPS.Haven, 14, 9), row(MAPS.Lotus, 11, 6), row(MAPS.Ascent, 9, 5), row(MAPS.Bind, 8, 3), row(MAPS.Split, 6, 3)],
    liveMates:
      puuid === PUUID.fahad
        ? [
            { matchId: `mdemo:fahad:r0`, puuids: [PUUID.nova!] },
            { matchId: `mdemo:fahad:r1`, puuids: [PUUID.nova!, PUUID.drake!] },
          ]
        : [],
  };
}

/** For specs that need "now". */
export const V2_NOW = DEMO_NOW;
