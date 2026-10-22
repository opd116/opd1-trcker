// DEV-only (R189): `saved_players` from the demo roster and the in-memory marks.
import type { DisplayName, SavedPlayer } from "../../types";
import { AG, DEMO_NOW, MAPS, PUUID } from "../cdv8";
import { state } from "./store";

const DAY = 86_400_000;

/** Per-player extras the roster does not carry (the design's `savedList()` values). */
const EXTRA: Record<string, { name: DisplayName; agent: string; tier: number; with: [number, number, number]; against: [number, number, number]; seenD: number; map: string }> = {
  [PUUID.mara!]: { name: { kind: "visible", name: "Mara", tag: "7788" }, agent: AG.Reyna, tier: 24, with: [0, 0, 0], against: [3, 0, 3], seenD: 0, map: MAPS.Haven },
  [PUUID.kilo!]: { name: { kind: "visible", name: "Kilo", tag: "2210" }, agent: AG.Breach, tier: 23, with: [2, 1, 1], against: [1, 0, 1], seenD: 0, map: MAPS.Haven },
};

export function demoSavedPlayers(): SavedPlayer[] {
  return state.marks.saved.map((puuid) => {
    const x = EXTRA[puuid];
    return {
      puuid,
      name: x?.name ?? { kind: "hiddenPlayer" },
      agentId: x?.agent ?? null,
      tier: x?.tier ?? null,
      with: { games: x?.with[0] ?? 0, wins: x?.with[1] ?? 0, losses: x?.with[2] ?? 0 },
      against: { games: x?.against[0] ?? 0, wins: x?.against[1] ?? 0, losses: x?.against[2] ?? 0 },
      note: state.marks.notes[puuid] ?? "",
      savedMs: state.savedAt.get(puuid) ?? DEMO_NOW,
      lastSeenMs: x ? DEMO_NOW - x.seenD * DAY : null,
      lastSeenMapId: x?.map ?? null,
    };
  });
}
