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
