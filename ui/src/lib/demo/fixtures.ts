// DEV-only demo builders. They fill the wire defaults and derive the small computed fields; the data itself
// is in cdv8.ts and the only scene registry is scenes.ts. Shapes follow src/lib/types.ts exactly
// (tri-state fields: an absent key means "loading").
import type { Last5Entry, MatchSummary, Peak, PlayerCard, Result } from "../types";

export type Letter = "W" | "L" | "D";

export const RESULT: Record<Letter, Result> = { W: "win", L: "loss", D: "draw" };

/** The other side of a result: the opponents of a win lost, a draw is a draw for both. */
export const opposite = (l: Letter): Letter => (l === "W" ? "L" : l === "L" ? "W" : "D");

export const LONGEST_NAME = "WWWWWWWWWWWWWWWW";
export const LONGEST_TAG = "WWWWW";

/**
 * A card with the wire defaults filled in. Tri-state keys you leave out stay absent (loading). Like the backend
 * (`state.rs`: `r.is_self_party || is_self`), the user's own card is also `isSelfParty` unless you say otherwise.
 */
export function card(c: Pick<PlayerCard, "puuid" | "cardId"> & Partial<PlayerCard>): PlayerCard {
  return { agentId: null, party: null, encounters: null, team: "ally", isSelfParty: !!c.isSelf, isSelf: false, ...c };
}

/** Newest-first entries from letters like "WLWWL"; `startsMs[i]` and `rr(letter, i)` give the rest. */
export function last5(letters: string, startsMs: readonly number[], rr: (l: Letter, i: number) => number): Last5Entry[] {
  return [...letters].map((ch, i) => {
    const l = ch as Letter;
    return { result: RESULT[l], rrEarned: rr(l, i), startMs: startsMs[i]! };
  });
}

export type Act = { label: string; name: string; startMs: number; endMs: number };

/** A peak reached in `act`; `sinceActEndMs` null means the current act. */
export function peak(act: Act, tier: number, sinceActEndMs: number | null): Peak {
  return { tier, actLabel: act.label, actName: act.name, sinceActEndMs, actStartMs: act.startMs, actEndMs: act.endMs };
}

/** Average combat score as the demo derives it from kills and assists. */
export const acsOf = (kills: number, assists: number): number => Math.round(kills * 10.2 + assists * 3.4 + 24);

/** One Recent Competitive Matches row. */
export function recent(m: {
  matchId: string;
  mapId: string;
  startMs: number;
  letter: Letter;
  rounds: readonly [number, number];
  agentId: string | null;
  kda: readonly [number, number, number];
  /** 0..100 */
  hs: number;
  rr: number;
  tier: number | null;
}): MatchSummary {
  const [kills, deaths, assists] = m.kda;
  return {
    matchId: m.matchId,
    mapId: m.mapId,
    startMs: m.startMs,
    result: RESULT[m.letter],
    roundsWon: m.rounds[0],
    roundsLost: m.rounds[1],
    agentId: m.agentId,
    kills,
    deaths,
    assists,
    kd: kills / deaths,
    acs: acsOf(kills, assists),
    hsPct: m.hs / 100,
    rrEarned: m.rr,
    tier: m.tier,
  };
}
