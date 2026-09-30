// Player Details and Match Details helpers (spec §6). Pure: the screens render what these return. Names from meta
// come through meta.ts (R95), never looked up here twice; tooltip strings live in tips.ts.
import { agentName, isRankedTier } from "./meta";
import { DASH, dateLong, dateShort, durationMin, rrDelta, time12 } from "./format";
import type { DisplayName, LiveState, MatchDetailsView, MatchSummary, Meta, PlayerCard, PlayerDetails, WeaponSkin } from "./types";

/** Rows shown before Show more, and the most Show more reveals (spec §6.4, §6.5). */
const COLLAPSED = 3;
const EXPANDED = 8;

/** Recent Competitive Matches: the newest 3 with a Show more button when there are more; after it, up to 8 and no button. */
export function recentShown(recent: MatchSummary[], more: boolean): { rows: MatchSummary[]; showMore: boolean } {
  if (more) return { rows: recent.slice(0, EXPANDED), showMore: false };
  return { rows: recent.slice(0, COLLAPSED), showMore: recent.length > COLLAPSED };
}

/**
 * The short name the encounters header and the `{name}'s team` label use (spec §6.6): the name without its tag,
 * `Hidden Player`, or the agent name for a hidden agent (undefined while meta is pending: draw the 60×10 bar,
 * spec §5.8; `—` when the agent is missing from loaded meta).
 */
export function shortName(n: DisplayName, meta: Meta | null): string | undefined {
  switch (n.kind) {
    case "visible":
    case "remembered":
      return n.name;
    case "hiddenPlayer":
      return "Hidden Player";
    case "hiddenAgent": {
      const a = agentName(meta, n.agentId);
      return a === undefined ? undefined : (a ?? DASH);
    }
  }
}

/**
 * The Details page after `player_details` failed twice: the settled empty state (spec §6.9, the `details-empty`
 * look), so the skeleton ends. The hero keeps what the live card knows, with every still-loading field settled to
 * null (a bar that would never fill becomes `—`); every section is empty. No live card for that puuid: an all-null one.
 */
export function settledDetails(puuid: string, card: PlayerCard | undefined): PlayerDetails {
  const base: PlayerCard = {
    puuid,
    name: null,
    level: null,
    cardId: "",
    agentId: null,
    rank: null,
    prevActTier: null,
    peak: null,
    kd: null,
    hsPct: null,
    actWinrate: null,
    actWins: null,
    actGames: null,
    last5: null,
    party: null,
    encounters: null,
    team: "ally",
    isSelfParty: false,
    isSelf: false,
  };
  const settled: PlayerCard = card
    ? {
        ...card,
        name: card.name ?? null,
        level: card.level ?? null,
        rank: card.rank ?? null,
        prevActTier: card.prevActTier ?? null,
        peak: card.peak ?? null,
        kd: card.kd ?? null,
        hsPct: card.hsPct ?? null,
        actWinrate: card.actWinrate ?? null,
        actWins: card.actWins ?? null,
        actGames: card.actGames ?? null,
        last5: card.last5 ?? null,
      }
    : base;
  return {
    card: settled,
    recent: [],
    topTeammates: [],
    loadout: { vandal: null, phantom: null, operator: null, sheriff: null },
    encounters: [],
    heroKd: null,
    heroHs: null,
    heroMatches: 0,
  };
}

/**
 * The Match page after `match_details` failed twice: a view with no teams, so the header reads `—` (no map, no
 * score, no chip, no halves, no date line) and the scoreboard keeps only its column header row. Nothing invented.
 */
export function emptyMatch(matchId: string): MatchDetailsView {
  return { matchId, mapId: "", queueId: "", startMs: 0, durationMs: null, perspective: "focus", teams: [] };
}

/** A row that opens its match (Recent, Encounters): Enter or Space does what a click does, without scrolling the page. */
export function rowKeydown(open: () => void): (e: KeyboardEvent) => void {
  return (e) => {
    if (e.key !== "Enter" && e.key !== " ") return;
    e.preventDefault();
    open();
  };
}

/** A loadout weapon's tri-state (R97): absent = its skin name is still resolving, null = none or unknown. */
export function weaponState(w: WeaponSkin | null | undefined): "loading" | "none" | WeaponSkin {
  if (w === undefined) return "loading";
  if (w === null) return "none";
  return w;
}

/**
 * The Top 3 live indicator (spec §6.7): the teammate's agent and rank on the live screen, only when that live card
 * shows their name (`visible`; the user's own card always does, R43). A hidden live card never gets the indicator,
 * so a Top 3 row can't reveal who is behind it. Unranked or unknown rank → tier null (an empty box).
 */
export function liveTeammate(puuid: string, live: LiveState | null): { agentId: string | null; tier: number | null } | null {
  const c = live?.players.find((p) => p.puuid === puuid);
  if (!c || !(c.isSelf || c.name?.kind === "visible")) return null;
  const tier = c.rank?.tier;
  return { agentId: c.agentId, tier: isRankedTier(tier) ? tier : null };
}

const QUEUES: Record<string, string> = {
  competitive: "Competitive",
  unrated: "Unrated",
  swiftplay: "Swiftplay",
  spikerush: "Spike Rush",
  deathmatch: "Deathmatch",
  hurm: "Team Deathmatch",
  ggteam: "Escalation",
  onefa: "Replication",
  premier: "Premier",
  "": "Custom",
};

/** The Match header's mode name (spec §6.10 table); an id the table lacks is shown title-cased. */
export function queueLabel(queueId: string): string {
  if (Object.hasOwn(QUEUES, queueId)) return QUEUES[queueId]!;
  return queueId
    .split(/[\s_-]+/)
    .filter(Boolean)
    .map((w) => w[0]!.toUpperCase() + w.slice(1))
    .join(" ");
}

/** Which Details row opened the Match page (the route's `from`): a recent-match row or an encounter row. */
export type MatchFrom = "recent" | "encounter";

/**
 * The Match header's second line, `Competitive · Sep 29, 2026 · 9:14 PM · 38 min`; the duration part is dropped when
 * it is unknown. R140: the design writes the date of a match opened from an encounter row without the year
 * (v8:642, v8:685: `Sep 12 · 11:18 PM · 41 min`), so that entry uses `dateShort` (the year only when it is not the
 * current year, as the encounter rows do) and a match opened from a recent row keeps `dateLong` (v8:53).
 */
export function whenText(v: MatchDetailsView, from: MatchFrom, nowMs: number): string {
  const date = from === "encounter" ? dateShort(v.startMs, nowMs) : dateLong(v.startMs);
  const parts = [queueLabel(v.queueId), date, time12(v.startMs)];
  if (v.durationMs !== null) parts.push(durationMin(v.durationMs));
  return parts.join(" · ");
}

/** A no-team mode (Deathmatch): one block, no sides (spec §6.10). */
const noTeams = (v: MatchDetailsView) => v.teams.length < 2;

/**
 * `a1–b1 · a2–b2` from team 0's side: the first-half rounds, then the rest (overtime counts in the second, v8:659).
 * null when a half or a score is unknown, and in a no-team mode.
 */
export function halvesText(v: MatchDetailsView): string | null {
  if (noTeams(v)) return null;
  const [a, b] = v.teams as [MatchDetailsView["teams"][0], MatchDetailsView["teams"][0]];
  if (a.firstHalf === null || b.firstHalf === null || a.roundsWon === null || b.roundsWon === null) return null;
  return `${a.firstHalf}–${b.firstHalf} · ${a.roundsWon - a.firstHalf}–${b.roundsWon - b.firstHalf}`;
}

/** A team header's label: team 0 is the user's side (`self`) or the focus player's; a no-team block is `Players`. */
export function teamLabel(v: MatchDetailsView, i: number, focusShort: string): string {
  if (noTeams(v)) return "Players";
  if (i !== 0) return "Opponents";
  return v.perspective === "self" ? "Your team" : `${focusShort}'s team`;
}

/** Kills minus deaths (`+6` / `−4` with U+2212 / `0`), with its colour; `—` in the flat tone when either count is unknown (R124). */
export function pmText(k: number | null, d: number | null): { text: string; tone: "win" | "loss" | "flat" } {
  if (k === null || d === null) return { text: DASH, tone: "flat" };
  const pm = k - d;
  return { text: rrDelta(pm), tone: pm > 0 ? "win" : pm < 0 ? "loss" : "flat" };
}

/** Skin content tiers by uuid (spec §6.8): the name for the well's tooltip, the colour T for its tint. */
export const CONTENT_TIERS: Record<string, { name: string; color: string }> = {
  "12683d76-48d7-84a3-4e09-6985794f0445": { name: "Select", color: "#5a9fe2" },
  "0cebb8be-46d7-c12a-d306-e9907bfc5a25": { name: "Deluxe", color: "#009587" },
  "60bca009-4182-7998-dee7-b8a2558dc369": { name: "Premium", color: "#d1548d" },
  "e046854e-406c-37f4-6607-19a9ba8426fc": { name: "Exclusive", color: "#f5955b" },
  "411e4a55-4e59-7757-41f0-86a53f101bb5": { name: "Ultra", color: "#fad663" },
};
