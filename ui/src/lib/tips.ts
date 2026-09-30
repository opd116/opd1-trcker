// The tooltip catalogue (spec §4.2): pure strings. Names from meta come through meta.ts; a pending or
// missing part is dropped (spec §5.8), so no tip ever contains "undefined" or "null".
//
// Every card and top-bar row (name wrapper, ⟲, party, peak, K/D + HS%, win, last-5 tile, average party rank), and
// the Details and Match rows (Task 18): hero K/D + HS% and the act scope, recent-row time and rank icon, encounter
// map cell, Top 3 live icons, loadout well, and the Match halves, column headers and scoreboard rank icon.
import { CONTENT_TIERS } from "./details";
import { actDates, actTitle, count, dateLong, dateShort, durationMin, rrDelta, span, time12 } from "./format";
import { agentName, mapName, tierLabel } from "./meta";
import type { DisplayName, EncounterCount, Last5Entry, Meta, PartyMark, Peak, PlayerCard } from "./types";

/** The first line of a remembered name's tip, by where the name came from (R145). */
const REMEMBERED_HEAD = {
  ownMatch: "Hidden name · known from your match",
  otherMatch: "Hidden name · known from a past match",
  seen: "Hidden name · seen before",
} as const;

/**
 * Name wrapper. `visible` is used with overflow-only triggers (shown only when the name is cut). A remembered name
 * is worded by its source (R145): `known from your match` / `known from a past match` with the map and date, or
 * `seen before` with the date alone (a name-service answer has no map). A part that is unknown is dropped (spec §5.8).
 */
export function nameTip(n: DisplayName, meta: Meta | null): string {
  switch (n.kind) {
    case "visible":
      return `${n.name}#${n.tag}`;
    case "remembered": {
      const map = n.source === "seen" ? null : mapName(meta, n.fromMapId);
      const where = [map, n.fromStartMs === null ? null : dateLong(n.fromStartMs)]
        .filter((p): p is string => typeof p === "string")
        .join(" · ");
      const head = REMEMBERED_HEAD[n.source];
      return where ? `${head}\n${where}` : head;
    }
    case "hiddenPlayer":
    case "hiddenAgent":
      return "Hidden name";
  }
}

/** K/D and HS% blocks on the card: their scope (spec §3.7 [bound]). */
export const LAST5_SCOPE = "Last 5 competitive";

/** Peak group: a past act shows its full title, the current act its label. Always with the dates. */
export function peakTip(p: Peak): string {
  const head = p.sinceActEndMs === null ? `${p.actLabel} · Current act` : actTitle(p.actName);
  return `${head}\n${actDates(p.actStartMs, p.actEndMs)}`;
}

/** Win% block: the structured `wl:` form; nothing while the act's games are unknown. */
export function winTip(c: PlayerCard): string {
  const { actWins, actGames } = c;
  if (typeof actWins !== "number" || typeof actGames !== "number") return "";
  return `wl:${actWins}:${actGames - actWins}:${actGames}`;
}

const WORD = { win: "Win", loss: "Loss", draw: "Draw" } as const;

/** Last-5 tile: `+18 RR · 45m`, or `Win · 2h` when Riot gave no RR change. */
export function last5Tip(e: Last5Entry, nowMs: number): string {
  const when = span(nowMs - e.startMs);
  return e.rrEarned === null ? `${WORD[e.result]} · ${when}` : `${rrDelta(e.rrEarned)} RR · ${when}`;
}

export function encounterTip(e: EncounterCount): string {
  return `${e.with} with · ${e.against} against`;
}

export function partyTip(p: PartyMark): string {
  return p.confirmed ? "Party · confirmed" : "Party · likely";
}

/** Average party rank group in the top bar. `n` = ranked members counted. */
export function aprTip(n: number): string {
  return `Average party rank\nFrom ${count(n, "ranked player")}`;
}

// ── Player Details and Match Details (spec §4.2) ─────────────────────────────────────────────────────────────────

/**
 * Hero K/D and HS% (R116): the current act's newest `matches` competitive matches with stats. None for a null value:
 * no counted match (N would be 0), or an HS% whose matches landed no shot.
 */
export function heroStatTip(value: number | null | undefined, matches: number): string {
  return typeof value === "number" ? `Last ${matches} competitive · this act` : "";
}

/** Hero Games and W / L blocks: their scope [bound]. None while the value is still loading (the bar has no number to explain). */
export function actScopeTip(value: number | null | undefined): string {
  return value === undefined ? "" : "This act";
}

/** Recent row "time ago" text: the local clock time, `9:14 PM` (v8:627). */
export const recentTimeTip = (startMs: number): string => time12(startMs);

/** A rank icon (recent row, scoreboard): its tier name; none while meta is pending, for a miss, or below tier 3 (spec §5.8). */
export function rankTip(meta: Meta | null, tier: number | null | undefined): string {
  return tierLabel(meta, tier) ?? "";
}

/** Encounter map cell: `Sep 12 · 11:18 PM · 41 min` (`, YYYY` after the day when not this year); no duration when unknown. */
export function encounterMapTip(e: { startMs: number; durationMs: number | null }, nowMs: number): string {
  const head = `${dateShort(e.startMs, nowMs)} · ${time12(e.startMs)}`;
  return e.durationMs === null ? head : `${head} · ${durationMin(e.durationMs)}`;
}

/** Top 3 live icons: `In this match · Breach`; the agent part is dropped when there is none yet or its name is unknown. */
export function liveTip(meta: Meta | null, agentId: string | null): string {
  const a = agentName(meta, agentId);
  return a ? `In this match · ${a}` : "In this match";
}

/** Loadout well: `Premium Edition` [bound]; none for a default skin (no tier) or a tier the table lacks. */
export function loadoutTip(tierId: string | null): string {
  const t = tierId ? CONTENT_TIERS[tierId] : undefined;
  return t ? `${t.name} Edition` : "";
}

export const HALVES_TIP = "First half · Second half";
export const ACS_TIP = "Average combat score";
export const PM_TIP = "Kills minus deaths";
export const ADR_TIP = "Average damage per round";
