// Hover detail for the player card, in one place: the same lines feed the
// visible tooltips and the card's screen-reader description.
import { actDates, DASH, kd, level, pct, rrDelta, span, timeAgo } from "./format";
import { agentName, FIRST_UNCAPPED_TIER, isRankedTier, tierLabel } from "./meta";
import type { DisplayName, Encounters, Last5Entry, Meta, PartyMark, Peak, PlayerCard, Rank } from "./types";

/** Tooltip lines (first line is the title), or null for "no tooltip" (still loading). */
export type TipLines = string[] | null;

export const LEVEL_TIP = "Account level";
export const STAT_TIPS = {
  kd: "K/D · last 5 competitive",
  hs: "Headshot % · last 5 competitive",
  wr: "Win rate · current act",
} as const;
export const REMEMBERED_TIP = ["Remembered name", "Seen in a match you played"];
export const HIDDEN_TIP = "Name hidden by the player";

export function rankTip(m: Meta, rank: Rank | null | undefined): TipLines {
  if (rank === undefined) return null;
  if (!rank || !isRankedTier(rank.tier)) return ["Unranked"];
  const name = tierLabel(m, rank.tier) ?? "Current rank";
  if (rank.tier >= FIRST_UNCAPPED_TIER) return [name, `${rank.rr} RR`];
  return [name, `${rank.rr} RR · ${Math.max(0, 100 - rank.rr)} RR to next tier`];
}

export function prevActTip(m: Meta, tier: number | null | undefined): TipLines {
  if (tier === undefined) return null;
  return ["Previous act", tierLabel(m, tier) ?? "Unranked"];
}

/** Peak tier, the full act name with its dates (R28), and how long ago it ended. */
export function peakTip(m: Meta, peak: Peak | null | undefined): TipLines {
  if (peak === undefined) return null;
  if (peak === null) return ["Peak rank", "No ranked history"];
  return [
    `Peak · ${tierLabel(m, peak.tier) ?? DASH}`,
    `${peak.actName} · ${actDates(peak.actStartMs, peak.actEndMs)}`,
    peak.sinceActEndMs === null ? "This act" : `Act ended ${span(peak.sinceActEndMs)} ago`,
  ];
}

const WHEN = new Intl.DateTimeFormat("en-US", {
  month: "short",
  day: "numeric",
  hour: "2-digit",
  minute: "2-digit",
  hourCycle: "h23",
});

export function resultTip(e: Last5Entry): string[] {
  return [`${e.win ? "Win" : "Loss"} · ${rrDelta(e.rrEarned)} RR`, WHEN.format(e.startMs)];
}

export function encounterTip(e: Encounters): string[] {
  return [`${e.with} with · ${e.against} against`, "Past matches with you"];
}

export function partyTip(p: PartyMark): string {
  return p.confirmed ? "Party" : "Likely party";
}

/** Text shown in place of the Riot ID (also the card's accessible name). */
export function displayNameText(m: Meta, name: DisplayName): string {
  switch (name.kind) {
    case "visible":
    case "remembered":
      return `${name.name}#${name.tag}`;
    case "hiddenAgent":
      return agentName(m, name.agentId) ?? "Hidden Player";
    case "hiddenPlayer":
      return "Hidden Player";
  }
}

/** Whether the card shows a party stripe / encounter badge (never for the user's own party). */
export function shownParty(card: PlayerCard): PartyMark | null {
  return !card.isSelfParty && card.party ? card.party : null;
}

export function shownEncounters(card: PlayerCard): Encounters | null {
  const e = card.encounters;
  return !card.isSelfParty && e && e.with + e.against > 0 ? e : null;
}

const spoken = (s: string) => (s === DASH ? "none" : s);

/**
 * Everything on the card plus its hover detail, as sentences, for the card's
 * aria-describedby. Loading (absent) fields are left out.
 */
export function cardDescription(m: Meta, card: PlayerCard, nowMs: number): string {
  const out: string[] = [];
  const add = (lines: TipLines, sep = ", ") => {
    if (lines) out.push(lines.join(sep));
  };

  if (card.name.kind === "remembered") out.push(REMEMBERED_TIP.join(", "));
  if (card.name.kind === "hiddenPlayer" || card.name.kind === "hiddenAgent") out.push(HIDDEN_TIP);
  const agent = agentName(m, card.agentId);
  if (agent) out.push(`Agent ${agent}`);
  out.push(`${LEVEL_TIP} ${spoken(level(card.level))}`);

  const rank = rankTip(m, card.rank);
  if (rank) out.push(`Rank ${rank.join(", ")}`);
  add(prevActTip(m, card.prevActTier), " ");
  add(peakTip(m, card.peak));

  if (card.kd !== undefined) out.push(`K/D ${spoken(kd(card.kd))}, last 5 competitive`);
  if (card.hsPct !== undefined) out.push(`Headshot ${spoken(pct(card.hsPct))}, last 5 competitive`);
  if (card.actWinrate !== undefined) out.push(`Win rate ${spoken(pct(card.actWinrate))}, current act`);
  if (card.last5 !== undefined) {
    const games = card.last5 ?? [];
    out.push(
      games.length === 0
        ? "Last 5 competitive: none"
        : `Last 5 competitive, newest first: ${games
            .slice(0, 5)
            .map((e) => `${e.win ? "Win" : "Loss"} ${rrDelta(e.rrEarned)} RR ${timeAgo(e.startMs, nowMs)} ago`)
            .join("; ")}`,
    );
  }

  const party = shownParty(card);
  if (party) out.push(partyTip(party));
  const enc = shownEncounters(card);
  if (enc) out.push(`Encounters ${enc.with} with, ${enc.against} against`);

  return `${out.join(". ")}.`;
}
