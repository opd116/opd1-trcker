// The tooltip catalogue (spec §4.2): pure strings. Names from meta come through meta.ts; a pending or
// missing part is dropped (spec §5.8), so no tip ever contains "undefined" or "null".
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
