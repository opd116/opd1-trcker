// The UI's only doors to Rust: `invoke` and events. In the DEV browser (no Tauri) the same calls are
// answered from the scene registry, through a DEV-guarded dynamic import so `vite build` drops every mock.
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Bridge } from "./boot";
import type {
  AppInfo,
  AppStatus,
  AsciiArt,
  Collection,
  HistoryPage,
  HistoryQueue,
  LiveState,
  MatchDetailsView,
  MyStats,
  PlayerDetails,
  PlayerMarks,
  ProfileExtras,
  SavedPlayer,
  Settings,
} from "./types";

export const tauriBridge: Bridge = {
  listen: (ev, cb) => listen(ev, cb),
  invoke: (cmd, args) => invoke(cmd, args),
};

/** DEV only: the scene the demo feed is showing (it keeps ?scene= current). */
const devScene = () => new URLSearchParams(location.search).get("scene");

/**
 * `invoke('my_stats')`: the user's own My Stats for the current act, read from the DB (never from Riot), or null
 * while the user's puuid or the current act is not known. Never resolves for a demo scene marked 'pending'.
 */
export async function myStats(): Promise<MyStats | null> {
  if (import.meta.env.DEV && !isTauri()) {
    const { resolveMyStats } = await import("./demo/scenes");
    return resolveMyStats(devScene());
  }
  return invoke<MyStats | null>("my_stats");
}

/** `invoke('player_details', { puuid })`. Never resolves for a demo scene entry marked 'pending'. */
export async function getPlayerDetails(puuid: string): Promise<PlayerDetails> {
  if (import.meta.env.DEV && !isTauri()) {
    const { resolveDetails } = await import("./demo/scenes");
    return resolveDetails(devScene(), puuid);
  }
  return invoke<PlayerDetails>("player_details", { puuid });
}

/**
 * `invoke('close_details', { puuid })` (R145): the Player Details page was left, so Rust forgets it and never rebuilds
 * or emits for it. The puuid (R163) lets Rust ignore a close for a player that is no longer the open one and void an
 * open still being built for this one; without it, whatever is open closes. In the DEV browser there is no Rust: the
 * demo module only counts the call.
 */
export async function closeDetails(puuid?: string): Promise<void> {
  if (import.meta.env.DEV && !isTauri()) {
    const { noteDetailsClosed } = await import("./demo/scenes");
    noteDetailsClosed();
    return;
  }
  await (puuid === undefined ? invoke("close_details") : invoke("close_details", { puuid }));
}

/**
 * `invoke('close_match', { matchId })` (R150): the Match Details page was left, so Rust forgets the open match and never
 * rebuilds or emits it. Idempotent. The match id (R163) lets Rust ignore a close for a match that is no longer the open
 * one and void an open still being built for this one; without it, whatever is open closes. A page that replaces it
 * (another match) sends its `match_details` after this call, and that call opens the new match again. In the DEV
 * browser there is no Rust: the demo module only counts it.
 */
export async function closeMatch(matchId?: string): Promise<void> {
  if (import.meta.env.DEV && !isTauri()) {
    const { noteMatchClosed } = await import("./demo/scenes");
    noteMatchClosed();
    return;
  }
  await (matchId === undefined ? invoke("close_match") : invoke("close_match", { matchId }));
}

/** `invoke('match_details', { matchId, focusPuuid })`. */
export async function getMatchDetails(matchId: string, focusPuuid: string): Promise<MatchDetailsView> {
  if (import.meta.env.DEV && !isTauri()) {
    const { resolveMatch } = await import("./demo/scenes");
    return resolveMatch(devScene(), matchId, focusPuuid);
  }
  return invoke<MatchDetailsView>("match_details", { matchId, focusPuuid });
}

// ── v2 redesign (R189): one wrapper per command in src-tauri/src/v2/. The DEV browser answers from demo/v2. ──────

const dev = () => import.meta.env.DEV && !isTauri();
const demoV2 = () => import("./demo/v2");

/** `invoke('app_status')`: the connection pill, Agent Select timer, queue timer and recap pointer. */
export async function appStatus(live: LiveState | null = null): Promise<AppStatus> {
  if (dev()) return (await demoV2()).v2Status(live);
  return invoke<AppStatus>("app_status");
}

/** "Retry now" on the lost banner and the error screen. */
export async function retryNow(): Promise<void> {
  if (dev()) return;
  await invoke("retry_now");
}

/** "Launch VALORANT" on the waiting screen. Rejects when the Riot Client install was not found. */
export async function launchValorant(): Promise<void> {
  if (dev()) return;
  await invoke("launch_valorant");
}

export async function playerMarks(): Promise<PlayerMarks> {
  if (dev()) return (await demoV2()).v2Marks();
  return invoke<PlayerMarks>("player_marks");
}

export async function savedPlayers(): Promise<SavedPlayer[]> {
  if (dev()) return (await demoV2()).v2SavedPlayers();
  return invoke<SavedPlayer[]>("saved_players");
}

/** Saves or unsaves a player; the `marks` event follows. */
export async function setSaved(puuid: string, saved: boolean): Promise<void> {
  if (dev()) return (await demoV2()).v2SetSaved(puuid, saved);
  await invoke("set_saved", { puuid, saved });
}

/** Sets (or, with "", clears) a private note; the `marks` event follows. */
export async function setNote(puuid: string, note: string): Promise<void> {
  if (dev()) return (await demoV2()).v2SetNote(puuid, note);
  await invoke("set_note", { puuid, note });
}

/** "Not a smurf · hide flag"; the `marks` event follows. */
export async function dismissSmurf(puuid: string): Promise<void> {
  if (dev()) return (await demoV2()).v2DismissSmurf(puuid);
  await invoke("dismiss_smurf", { puuid });
}

/** The user's own stored matches, newest first. */
export async function matchHistory(queue: HistoryQueue, offset: number, limit: number): Promise<HistoryPage> {
  if (dev()) return (await demoV2()).v2History(queue, offset, limit);
  return invoke<HistoryPage>("match_history", { queue, offset, limit });
}

export async function getSettings(): Promise<Settings> {
  if (dev()) return (await demoV2()).v2GetSettings();
  return invoke<Settings>("get_settings");
}

/** Stores the settings (Rust applies keep-on-top, autostart and close-to-tray); the `settings` event follows. */
export async function saveSettings(settings: Settings): Promise<Settings> {
  if (dev()) return (await demoV2()).v2SetSettings(settings);
  return invoke<Settings>("set_settings", { settings });
}

export async function asciiList(): Promise<AsciiArt[]> {
  if (dev()) return (await demoV2()).v2AsciiList();
  return invoke<AsciiArt[]>("ascii_list");
}

export async function asciiSave(title: string, raw: string): Promise<AsciiArt> {
  if (dev()) return (await demoV2()).v2AsciiSave(title, raw);
  return invoke<AsciiArt>("ascii_save", { title, raw });
}

export async function asciiDelete(id: number): Promise<void> {
  if (dev()) return (await demoV2()).v2AsciiDelete(id);
  await invoke("ascii_delete", { id });
}

export async function appInfo(): Promise<AppInfo> {
  if (dev()) return (await demoV2()).v2AppInfo();
  return invoke<AppInfo>("app_info");
}

/**
 * The user's owned skins. `null` while no refresh was attempted yet (the game has not run since this account was
 * first seen); an attempted refresh that left nothing stored rejects with Rust's `Err`, so the screen shows its retry
 * state instead of the "never fetched" empty state. A call superseded by an account switch also rejects, so the screen
 * keeps the retry state while it re-asks for the current account.
 */
export async function collection(): Promise<Collection | null> {
  if (dev()) return (await demoV2()).v2Collection();
  return invoke<Collection | null>("collection");
}

export async function profileExtras(puuid: string): Promise<ProfileExtras> {
  if (dev()) return (await demoV2()).v2ProfileExtras(puuid);
  return invoke<ProfileExtras>("profile_extras", { puuid });
}

/** Glance mode: Rust resizes the window to a 420px column (on) or back (off). */
export async function setGlance(on: boolean): Promise<void> {
  if (dev()) return;
  await invoke("set_glance", { on });
}
