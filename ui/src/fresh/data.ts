// DEV prototype adapter. Import this module only from a DEV-guarded entry point.
// No requests, timers or clock reads: all samples use the fixture anchor.
import demoMeta from "../lib/demo/meta.json";
import { DEMO_NOW, PUUID, detailsFor, matchFor } from "../lib/demo/cdv8";
import { SCENES, sceneMyStats, type Scene, type SceneName } from "../lib/demo/scenes";
import { shortName } from "../lib/details";
import { DASH } from "../lib/format";
import { agentName as agentLookup, mapName as mapLookup, tierLabel } from "../lib/meta";
import type { DisplayName, MatchDetailsView, Meta, MyStats, PlayerCard, PlayerDetails } from "../lib/types";

export type { Scene, SceneName } from "../lib/demo/scenes";
export type { DisplayName, MatchDetailsView, Meta, MyStats, PlayerCard, PlayerDetails } from "../lib/types";
export { assetUrl as asset } from "../lib/assets";
export type { AssetKind } from "../lib/assets";
export { recentShown, pmText, queueLabel, halvesText, weaponState } from "../lib/details";
export { actDates, actTitle, durationMin, kd2, level, pct0, peakLabel, rr, rrDelta, score, span } from "../lib/format";
import { ago as formatAgo, dateShort as formatDateShort } from "../lib/format";

export const DEMO_TIME: number = DEMO_NOW;
export const meta: Meta = structuredClone(demoMeta);

const labels: Partial<Record<SceneName, string>> = {
  boot: "Starting", closed: "Game closed", lobby1: "Solo lobby", lobby2: "Party of two",
  lobby5: "Full party", pregame: "Agent select", ingame: "In game",
  "ingame-longnames": "Long names", "ingame-partial": "Partial data",
  "ingame-loading": "Loading roster", "ingame-unranked": "Unranked player",
  "ingame-remembered": "Remembered hidden names", "ingame-parties4": "Four parties",
  details: "Player details", match: "Match scoreboard", mystats: "My stats",
};

/** Every existing fixture is available; labels are DEV controls, never product status text. */
export const sceneNames: ReadonlyArray<{ label: string; value: SceneName }> =
  (Object.keys(SCENES) as SceneName[]).map((value) => ({
    value,
    label: labels[value] ?? value.replace(/-/g, " "),
  }));

const sourceScene = (name: string): Scene =>
  Object.hasOwn(SCENES, name) ? SCENES[name as SceneName] : SCENES.ingame;

/** Copies prevent a prototype interaction from mutating shared fixtures. */
export function getScene(name: string): Scene {
  const { meta: sceneMeta, ...data } = sourceScene(name);
  const copy = structuredClone(data);
  // The fixture's pure meta transform is a function, which structuredClone cannot copy.
  return sceneMeta === undefined ? copy : {
    ...copy,
    meta: typeof sceneMeta === "object" ? structuredClone(sceneMeta) : sceneMeta,
  };
}

/** null means metadata is pending; use this for scene-aware names in stress fixtures. */
export function getMeta(sceneName: string): Meta | null {
  const value = sourceScene(sceneName).meta;
  if (value === "pending") return null;
  const base = structuredClone(meta);
  return typeof value === "function" ? value(base) : value ? structuredClone(value) : base;
}

/** A live hidden identity also applies to details and historic match participants. */
function privateName(live: readonly PlayerCard[], puuid: string, fallback: DisplayName, rowAgent?: string | null): DisplayName {
  const card = live.find((player) => player.puuid === puuid);
  if (!card || card.isSelf || card.name?.kind === "visible") return fallback;
  if (card.name?.kind === "remembered") return structuredClone(card.name);
  if (rowAgent !== undefined) return rowAgent ? { kind: "hiddenAgent", agentId: rowAgent } : { kind: "hiddenPlayer" };
  return card.name ? structuredClone(card.name) : { kind: "hiddenPlayer" };
}

/** Pending or unsupported player IDs return null; absent card fields stay absent. */
export function getDetails(sceneName: string, puuid: string): PlayerDetails | null {
  const scene = sourceScene(sceneName);
  const own = Object.hasOwn(scene.details ?? {}, puuid) ? scene.details?.[puuid] : undefined;
  if (own === "pending") return null;
  const id = Object.keys(PUUID).find((key) => PUUID[key] === puuid);
  if (!own && !id) return null;
  const live = scene.live?.players ?? [];
  const data = own ? structuredClone(own) : detailsFor(id!, live.find((player) => player.puuid === puuid), live);
  const liveCard = live.find((player) => player.puuid === puuid);
  if (liveCard && !liveCard.isSelf && liveCard.name === undefined) delete data.card.name;
  else if (data.card.name) data.card.name = privateName(live, puuid, data.card.name);
  for (const teammate of data.topTeammates) teammate.name = privateName(live, teammate.puuid, teammate.name);
  return data;
}

/** Pending or unsupported match IDs return null. Match teams/perspective follow the fixture. */
export function getMatch(sceneName: string, matchId: string, focusPuuid: string): MatchDetailsView | null {
  const scene = sourceScene(sceneName);
  const own = Object.hasOwn(scene.matches ?? {}, matchId) ? scene.matches?.[matchId] : undefined;
  if (own === "pending") return null;
  const live = scene.live?.players ?? [];
  let data: MatchDetailsView;
  try {
    data = own ? structuredClone(own) : matchFor(matchId, live);
  } catch {
    return null;
  }
  const rows = data.teams.flatMap((team) => team.players);
  const hasFocus = rows.some((row) => row.puuid === focusPuuid);
  for (const row of rows) {
    row.name = privateName(live, row.puuid, row.name, row.agentId);
    if (hasFocus) row.isFocus = row.puuid === focusPuuid;
  }
  return data;
}

/** undefined = pending; null = no account/act; otherwise a fresh complete answer. */
export function getStats(sceneName: string): MyStats | null | undefined {
  const data = sceneMyStats(sourceScene(sceneName));
  return data == null ? data : structuredClone(data);
}

export function displayName(value: DisplayName | null | undefined): { name: string; tag: string; hidden: boolean; remembered: boolean } {
  if (!value) return { name: DASH, tag: "", hidden: false, remembered: false };
  const known = value.kind === "visible" || value.kind === "remembered";
  return {
    name: shortName(value, meta) ?? DASH,
    tag: known ? value.tag : "",
    hidden: value.kind !== "visible",
    remembered: value.kind === "remembered",
  };
}

export const rankName = (tier: number | null | undefined): string => tierLabel(meta, tier) ?? DASH;
export const mapName = (id: string | null | undefined): string => mapLookup(meta, id) ?? DASH;
export const agentName = (id: string | null | undefined): string => agentLookup(meta, id) ?? DASH;
/** Clock-dependent formatters are bound to the same date as the fixture generator. */
export const ago = (timestamp: number | null | undefined): string => formatAgo(timestamp, DEMO_TIME);
export const dateShort = (timestamp: number | null | undefined): string => formatDateShort(timestamp, DEMO_TIME);
