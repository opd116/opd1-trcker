// Saved players (design v2, TrackerApp.dc.html 450–522; logic `extraVals` 1680–1740). Pure: filter, search and sort
// the `saved_players` rows, and the texts of their cells. Hidden-name rules: the name a row searches and sorts by is
// the one it shows (a hidden player's real name never reaches the UI, so it can't be searched either).
import { agentName } from "./meta";
import { monthsOrDays, stamp } from "./historyv2";
import type { DisplayName, LiveState, Meta, PlayerCard, SavedPlayer, SharedRecord } from "./types";

export type SavedFilter = "all" | "mates" | "opp";
export type SortKey = "name" | "rank" | "with" | "against" | "seen";
/** `dir: 1` is each key's natural order (A→Z, highest rank, most games, most recently seen); -1 reverses it. */
export interface SavedSort {
  key: SortKey;
  dir: 1 | -1;
}

export const DEFAULT_SORT: SavedSort = { key: "seen", dir: 1 };

/** The text a name shows: `{ name, tag }` for a known name, the hidden-style text with no tag otherwise. */
export function nameParts(n: DisplayName, meta: Meta | null): { name: string; tag: string | null } {
  switch (n.kind) {
    case "visible":
    case "remembered":
      return { name: n.name, tag: n.tag };
    case "hiddenPlayer":
      return { name: "Hidden Player", tag: null };
    case "hiddenAgent":
      return { name: agentName(meta, n.agentId) ?? "Hidden Player", tag: null };
  }
}

/** The live match's other players by puuid, while there is one (Agent Select / In Game). */
export function liveCards(l: LiveState | null): Map<string, PlayerCard> {
  const m = new Map<string, PlayerCard>();
  if (!l || (l.stage !== "pregame" && l.stage !== "ingame")) return m;
  for (const p of l.players) if (!p.isSelf) m.set(p.puuid, p);
  return m;
}

/**
 * The name a saved row shows: a player in the live match who is incognito there shows as on their card (the
 * remembered name, else the agent / `Hidden Player`, CLAUDE.md Hidden names); anyone else as `saved_players` gives it.
 */
export function rowName(s: SavedPlayer, card: PlayerCard | undefined): DisplayName {
  const n = card?.name;
  if (n && n.kind !== "visible") return n;
  return s.name;
}

export function counts(list: readonly SavedPlayer[]): Record<SavedFilter, number> {
  return {
    all: list.length,
    mates: list.filter((s) => s.with.games > 0).length,
    opp: list.filter((s) => s.against.games > 0).length,
  };
}

export interface SavedView {
  s: SavedPlayer;
  name: DisplayName;
  note: string;
  inMatch: boolean;
}

/** Filter pill, then the search (name, tag or note, case-insensitive), then the sort. Stable: ties keep saved order. */
export function arrange(
  rows: readonly SavedView[],
  filter: SavedFilter,
  query: string,
  sort: SavedSort,
  meta: Meta | null,
  nowMs: number,
): SavedView[] {
  const q = query.trim().toLowerCase();
  const out = rows
    .filter((r) => filter === "all" || (filter === "mates" ? r.s.with.games > 0 : r.s.against.games > 0))
    .filter((r) => {
      if (!q) return true;
      const p = nameParts(r.name, meta);
      return [p.name, p.tag ?? "", r.note].join(" ").toLowerCase().includes(q);
    });
  const key = (r: SavedView): number | string => {
    switch (sort.key) {
      case "name":
        return nameParts(r.name, meta).name.toLowerCase();
      case "rank":
        return -(r.s.tier ?? -1);
      case "with":
        return -r.s.with.games;
      case "against":
        return -r.s.against.games;
      case "seen":
        // In the match now first; never seen last.
        return r.inMatch ? -1 : r.s.lastSeenMs === null ? Number.POSITIVE_INFINITY : Math.max(0, nowMs - r.s.lastSeenMs);
    }
  };
  return out
    .map((r, i) => ({ r, i, k: key(r) }))
    .sort((a, b) => {
      const c = a.k < b.k ? -1 : a.k > b.k ? 1 : 0;
      return c * sort.dir || a.i - b.i;
    })
    .map((x) => x.r);
}

/** A header click: the same column flips its direction, another column starts in its natural order. */
export function nextSort(cur: SavedSort, key: SortKey): SavedSort {
  return cur.key === key ? { key, dir: cur.dir === 1 ? -1 : 1 } : { key, dir: 1 };
}

/**
 * `aria-sort` and the arrow of a column. The natural order of `name` is A→Z (ascending); every other column's is
 * highest / most recent first (descending). ▼ = descending, ▲ = ascending.
 */
export function sortState(sort: SavedSort, key: SortKey): { aria: "ascending" | "descending" | "none"; arrow: string } {
  if (sort.key !== key) return { aria: "none", arrow: "" };
  const natural = key === "name" ? "ascending" : "descending";
  const aria = sort.dir === 1 ? natural : natural === "ascending" ? "descending" : "ascending";
  return { aria, arrow: aria === "descending" ? "▼" : "▲" };
}

const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

/** The header's line under the title: `8 players`, or `Nobody saved yet`. */
export const summary = (n: number): string => (n === 0 ? "Nobody saved yet" : plural(n, "player"));

/** A record cell: `14 games` + `10W` `4L`, or null (`—`); and its tooltip. */
export function recordCell(
  r: SharedRecord,
  who: string,
  side: "with" | "against",
): { games: string; w: string; l: string; tip: string } | { games: null; tip: string } {
  if (r.games <= 0) return { games: null, tip: side === "with" ? "Never on your team in a stored match" : "Never against you in a stored match" };
  const n = plural(r.games, "stored match", "stored matches");
  const tip =
    side === "with"
      ? `${who} was on your team in ${n}.\nYour record together: ${r.wins}W – ${r.losses}L`
      : `${who} was on the other team in ${n}.\nYour record against them: ${r.wins}W – ${r.losses}L`;
  return { games: plural(r.games, "game"), w: `${r.wins}W`, l: `${r.losses}L`, tip };
}

/** Last seen: `Now` in the live match, else relative (`2 h ago`, `Yesterday`, `9 days ago`, `Sep 15`), with its tooltip. */
export function seenCell(lastSeenMs: number | null, inMatch: boolean, nowMs: number): { text: string; tip: string } {
  if (inMatch) return { text: "Now", tip: "In your current match" };
  if (lastSeenMs === null) return { text: "—", tip: "" };
  const st = stamp(lastSeenMs, nowMs);
  return { text: st.rel, tip: st.dayDiff >= 30 ? `${monthsOrDays(lastSeenMs, nowMs)} ago` : st.exact.split(" · ")[0]! };
}

/** A note longer than this gets the `Show full note` toggle. */
export const LONG_NOTE = 70;

export const noResultsText = (query: string): string =>
  query.trim() ? `No saved player matches “${query.trim()}”.` : "Nobody in this filter yet.";
