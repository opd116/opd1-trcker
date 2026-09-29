// The UI's only doors to Rust: `invoke` and events. In the DEV browser (no Tauri) the same calls are
// answered from the scene registry, through a DEV-guarded dynamic import so `vite build` drops every mock.
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Bridge } from "./boot";
import type { MatchDetailsView, PlayerDetails } from "./types";

export const tauriBridge: Bridge = {
  listen: (ev, cb) => listen(ev, cb),
  invoke: (cmd, args) => invoke(cmd, args),
};

/** DEV only: the scene the demo feed is showing (it keeps ?scene= current). */
const devScene = () => new URLSearchParams(location.search).get("scene");

/** `invoke('player_details', { puuid })`. Never resolves for a demo scene entry marked 'pending'. */
export async function getPlayerDetails(puuid: string): Promise<PlayerDetails> {
  if (import.meta.env.DEV && !isTauri()) {
    const { resolveDetails } = await import("./demo/scenes");
    return resolveDetails(devScene(), puuid);
  }
  return invoke<PlayerDetails>("player_details", { puuid });
}

/** `invoke('match_details', { matchId, focusPuuid })`. */
export async function getMatchDetails(matchId: string, focusPuuid: string): Promise<MatchDetailsView> {
  if (import.meta.env.DEV && !isTauri()) {
    const { resolveMatch } = await import("./demo/scenes");
    return resolveMatch(devScene(), matchId, focusPuuid);
  }
  return invoke<MatchDetailsView>("match_details", { matchId, focusPuuid });
}
