// The Live page's pure helpers: the slot model (spec §3.1, §5.1–§5.7), last-5 tiles (§3.8), the average party
// rank (§2.3) and the card indicator rules (§3.5, §3.9). Names from meta come from meta.ts (R95), never here.
import { isRankedTier } from "./meta";
import type { Last5Entry, LiveState, PlayerCard } from "./types";

export type SlotItem = { kind: "card"; card: PlayerCard } | { kind: "skeleton" } | { kind: "dashed" } | { kind: "none" };

const SLOTS = 5;
const fill = (kind: "skeleton" | "dashed" | "none", n = SLOTS): SlotItem[] => Array.from({ length: n }, () => ({ kind }));

/** Up to 5 cards, then empty slots: every column is a fixed 5-slot track (spec §3.1). */
function column(cards: PlayerCard[]): SlotItem[] {
  const items: SlotItem[] = cards.slice(0, SLOTS).map((card) => ({ kind: "card", card }));
  return [...items, ...fill("none", SLOTS - items.length)];
}

/** The user's own card first, the rest in emitted order (never reordered when party marks arrive, R53). */
const selfFirst = (ps: PlayerCard[]) => [...ps.filter((p) => p.isSelf), ...ps.filter((p) => !p.isSelf)];

/** Always 5 + 5 items. `null` = no LiveState yet: one skeleton card in slot 1 (spec §5.3). */
export function liveColumns(s: LiveState | null): { left: SlotItem[]; right: SlotItem[] } {
  if (!s) return { left: [{ kind: "skeleton" }, ...fill("none", SLOTS - 1)], right: fill("none") };
  const empty = s.players.length === 0;
  switch (s.stage) {
    case "closed":
      // Only the user's own card (spec §5.3), whatever else was emitted. No card at all before the user's PUUID was
      // ever learned: a skeleton here would never resolve.
      return { left: column(s.players.filter((p) => p.isSelf)), right: fill("none") };
    case "menus":
      return {
        left: empty ? [{ kind: "skeleton" }, ...fill("none", SLOTS - 1)] : column(selfFirst(s.players)),
        right: fill("none"),
      };
    case "pregame":
      return {
        left: empty ? fill("skeleton") : column(selfFirst(s.players.filter((p) => p.team === "ally"))),
        right: fill("dashed"),
      };
    case "ingame":
      if (empty) return { left: fill("skeleton"), right: fill("skeleton") };
      return {
        left: column(selfFirst(s.players.filter((p) => p.team === "ally"))),
        right: column(s.players.filter((p) => p.team === "enemy")),
      };
    default:
      // A stage this UI does not know (a contract drift): a quiet empty grid, never a throw.
      return { left: fill("none"), right: fill("none") };
  }
}

export type Tile = { key: "W" | "L" | "D" | "E"; newest: boolean; entry: Last5Entry | null };

const KEY = { win: "W", loss: "L", draw: "D" } as const;

/**
 * The 5 last-5 tiles, left to right (spec §3.8): empty slots first, then the entries oldest → newest, so the
 * newest match is the rightmost tile. `entries` arrive newest first. Absent (loading) → 'skeleton'.
 */
export function last5Tiles(entries: Last5Entry[] | null | undefined): Tile[] | "skeleton" {
  if (entries === undefined) return "skeleton";
  const real = (entries ?? []).slice(0, SLOTS);
  const empties: Tile[] = Array.from({ length: SLOTS - real.length }, () => ({ key: "E", newest: false, entry: null }));
  const tiles = real.map((entry, i): Tile => ({ key: KEY[entry.result], newest: i === 0, entry })).reverse();
  return [...empties, ...tiles];
}

/**
 * Average Party Rank (spec §2.3): the mean tier of ranked members (tier ≥ 3), .5 rounding up. null with fewer
 * than 2 players. `loading` while nobody is ranked yet and someone's rank is still absent.
 */
export function avgPartyRank(ps: PlayerCard[]): { tier: number | null; n: number; loading: boolean } | null {
  if (ps.length < 2) return null;
  const tiers = ps.map((p) => p.rank?.tier).filter(isRankedTier);
  const n = tiers.length;
  const tier = n ? Math.floor(tiers.reduce((a, b) => a + b, 0) / n + 0.5) : null;
  return { tier, n, loading: n === 0 && ps.some((p) => p.rank === undefined) };
}

/** ⟲ badge: never on the user or the user's own party, and only with at least one shared match (spec §3.5). */
export const showEncounters = (c: PlayerCard): boolean =>
  !c.isSelf && !c.isSelfParty && !!c.encounters && c.encounters.with + c.encounters.against > 0;

/** Party stripe and tab: never on the user or the user's own party (spec §3.9). */
export const showParty = (c: PlayerCard): boolean => !!c.party && !c.isSelf && !c.isSelfParty;
