import { tierName } from "./format";
import type { Meta } from "./types";

export const EMPTY_META: Meta = { agents: {}, maps: {}, tiers: {} };

/** Tiers 0 (unranked) and 1–2 (unused) have no rank to show. */
export function isRankedTier(tier: number | null | undefined): tier is number {
  return typeof tier === "number" && tier >= 3;
}

/** "Diamond 2", or null when unknown. */
export function tierLabel(m: Meta, tier: number | null | undefined): string | null {
  if (!isRankedTier(tier)) return null;
  const raw = m.tiers[String(tier)]?.name;
  return raw ? tierName(raw) : null;
}

/** valorant-api colors are "rrggbbaa"; CSS wants "#rrggbb". */
export function cssColor(raw: string | undefined): string | undefined {
  if (!raw) return undefined;
  const hex = raw.replace(/^#/, "");
  if (/^[0-9a-f]{8}$/i.test(hex)) return `#${hex.slice(0, 6)}`;
  if (/^[0-9a-f]{6}$/i.test(hex)) return `#${hex}`;
  return raw;
}

export function tierColor(m: Meta, tier: number | null | undefined): string | undefined {
  if (!isRankedTier(tier)) return undefined;
  return cssColor(m.tiers[String(tier)]?.color);
}

export function agentName(m: Meta, id: string | null | undefined): string | null {
  return (id && m.agents[id]?.name) || null;
}

export function mapName(m: Meta, id: string | null | undefined): string | null {
  return (id && m.maps[id]?.name) || null;
}

/** Immortal 1 and above have no 100-RR ceiling. */
export const FIRST_UNCAPPED_TIER = 24;
