// The shared tooltip strings (spec §4.2): pure strings. Names from meta come through meta.ts; a pending or missing part
// is dropped (spec §5.8), so no tip ever contains "undefined" or "null". The v2 screens build most of their tips next
// to their own view models (livev2.ts, profile.ts, compv2.ts, historyv2.ts); these are the ones several screens share.
import { actDates, actTitle, dateLong } from "./format";
import { agentName, mapName } from "./meta";
import type { DisplayName, Meta, Peak } from "./types";

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

/** Peak group: a past act shows its full title, the current act its label. Always with the dates. */
export function peakTip(p: Peak): string {
  const head = p.sinceActEndMs === null ? `${p.actLabel} · Current act` : actTitle(p.actName);
  return `${head}\n${actDates(p.actStartMs, p.actEndMs)}`;
}

/** Top 3 live icons: `In this match · Breach`; the agent part is dropped when there is none yet or its name is unknown. */
export function liveTip(meta: Meta | null, agentId: string | null): string {
  const a = agentName(meta, agentId);
  return a ? `In this match · ${a}` : "In this match";
}

export const ACS_TIP = "Average combat score";
export const PM_TIP = "Kills minus deaths";
export const ADR_TIP = "Average damage per round";
