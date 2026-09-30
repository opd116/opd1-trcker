import { tierName } from "./format";
import type { Meta } from "./types";

export const EMPTY_META: Meta = { agents: {}, maps: {}, tiers: {} };

/** Tiers 0 (unranked) and 1–2 (unused) have no rank to show. */
export function isRankedTier(tier: number | null | undefined): tier is number {
  return typeof tier === "number" && tier >= 3;
}

/**
 * The one meta lookup (spec §5.8, R95). `tips.ts`, the Live screen and the Details screens use it and
 * never re-implement it.
 *   undefined: meta is still pending, so render a skeleton bar
 *   null:      no id, or the id is missing from a loaded table, so render "—"
 *   string:    the name
 * "No id" wins over "pending": there is nothing to wait for.
 */
export function lookup(
  meta: Meta | null,
  table: "maps" | "agents",
  id: string | null | undefined,
): string | null | undefined {
  if (!id) return null;
  if (meta === null) return undefined;
  return meta[table][id]?.name || null;
}

export const agentName = (meta: Meta | null, id: string | null | undefined) => lookup(meta, "agents", id);

export const mapName = (meta: Meta | null, id: string | null | undefined) => lookup(meta, "maps", id);

/** "Diamond 2": the same rule as `lookup`; tier < 3 → null. */
export function tierLabel(meta: Meta | null, tier: number | null | undefined): string | null | undefined {
  if (!isRankedTier(tier)) return null;
  if (meta === null) return undefined;
  const raw = meta.tiers[String(tier)]?.name;
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

export function tierColor(meta: Meta | null, tier: number | null | undefined): string | undefined {
  if (meta === null || !isRankedTier(tier)) return undefined;
  return cssColor(meta.tiers[String(tier)]?.color);
}

/** Immortal 1 and above have no 100-RR ceiling. */
export const FIRST_UNCAPPED_TIER = 24;
