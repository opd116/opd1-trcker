// The tooltip catalogue (spec §4.2): pure strings. Names from meta come through meta.ts; a pending or
// missing part is dropped (spec §5.8), so no tip ever contains "undefined" or "null".
//
// Built so far: every card and top-bar row (name wrapper, ⟲, party, peak, K/D + HS%, win, last-5 tile,
// average party rank). The Details and Match rows land HERE as pure functions with tests, never inline in a
// screen (Task 18):
//   - hero K/D + HS% "Last {N} competitive · this act" (R116) and "This act" (Games, W/L blocks)
//   - recent-row time text and rank-icon tier name; encounter map cell ({Mon D} · {h:mm AM/PM} · {N min})
//   - top-3 live icons: "In this match · {Agent}", or "In this match" when the agent is unknown or the
//     meta lookup is pending or missing (§5.8: the part is dropped)
//   - loadout well "{Tier} Edition" (none for a default skin)
//   - match screen: halves "First half · Second half", the ACS / +/− / ADR headers, scoreboard rank icon
import { actDates, actTitle, count, dateLong, rrDelta, span } from "./format";
import { mapName } from "./meta";
import type { DisplayName, EncounterCount, Last5Entry, Meta, PartyMark, Peak, PlayerCard } from "./types";

/** Name wrapper. `visible` is used with overflow-only triggers (shown only when the name is cut). */
export function nameTip(n: DisplayName, meta: Meta | null): string {
  switch (n.kind) {
    case "visible":
      return `${n.name}#${n.tag}`;
    case "remembered": {
      const where = [mapName(meta, n.fromMapId), n.fromStartMs === null ? null : dateLong(n.fromStartMs)]
        .filter((p): p is string => typeof p === "string")
        .join(" · ");
      return where ? `Hidden name · known from your match\n${where}` : "Hidden name · known from your match";
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
