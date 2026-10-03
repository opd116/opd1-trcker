// The UI's only doors to Rust: `invoke` and events. In the DEV browser (no Tauri) the same calls are
// answered from the scene registry, through a DEV-guarded dynamic import so `vite build` drops every mock.
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Bridge } from "./boot";
import type { MatchDetailsView, MyStats, PlayerDetails } from "./types";

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
 * `invoke('close_details')` (R145): the Player Details page was left, so Rust forgets it and never rebuilds or emits
 * for it. In the DEV browser there is no Rust: the demo module only counts the call.
 */
export async function closeDetails(): Promise<void> {
  if (import.meta.env.DEV && !isTauri()) {
    const { noteDetailsClosed } = await import("./demo/scenes");
    noteDetailsClosed();
    return;
  }
  await invoke("close_details");
}

/**
 * `invoke('close_match')` (R150): the Match Details page was left, so Rust forgets the open match and never rebuilds or
 * emits it. No arguments, idempotent. A page that replaces it (another match) sends its `match_details` after this
 * call, and that call opens the new match again. In the DEV browser there is no Rust: the demo module only counts it.
 */
export async function closeMatch(): Promise<void> {
  if (import.meta.env.DEV && !isTauri()) {
    const { noteMatchClosed } = await import("./demo/scenes");
    noteMatchClosed();
    return;
  }
  await invoke("close_match");
}

/** `invoke('match_details', { matchId, focusPuuid })`. */
export async function getMatchDetails(matchId: string, focusPuuid: string): Promise<MatchDetailsView> {
  if (import.meta.env.DEV && !isTauri()) {
    const { resolveMatch } = await import("./demo/scenes");
    return resolveMatch(devScene(), matchId, focusPuuid);
  }
  return invoke<MatchDetailsView>("match_details", { matchId, focusPuuid });
}
