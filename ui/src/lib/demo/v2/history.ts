// DEV-only (R189): `match_history` from the design's 13 sample rows (TrackerApp.dc.html `historyRows()`). Every id is
// a demo session-match id, so the scoreboard opens for any row (cdv8.ts `matchFor`).
import type { HistoryPage, HistoryQueue, HistoryRow } from "../../types";
import { AG, DEMO_NOW, MAPS, sessionMatchId } from "../cdv8";

type MapName = keyof typeof MAPS;
type Agent = keyof typeof AG;
// [map, minsAgo, durationMin, W/L, us, them, agent, tier, k, d, a, acs, hs% | null, rr, queue]
type Raw = [MapName, number, number, "W" | "L", number, number, Agent, number, number, number, number, number, number | null, number, string];

const RAW: Raw[] = [
  ["Lotus", 45, 38, "W", 13, 9, "Jett", 21, 24, 14, 5, 286, 31, 18, "competitive"],
  ["Ascent", 92, 41, "W", 13, 11, "Jett", 21, 19, 16, 7, 241, 27, 19, "competitive"],
  ["Bind", 140, 35, "L", 8, 13, "Omen", 21, 13, 17, 9, 187, 22, -16, "competitive"],
  ["Haven", 190, 44, "W", 14, 12, "Jett", 21, 27, 19, 4, 301, null, 21, "competitive"],
  ["Split", 246, 39, "W", 13, 10, "Sova", 20, 17, 13, 11, 222, 25, 17, "competitive"],
  ["Sunset", 300, 37, "L", 10, 13, "Jett", 20, 18, 18, 3, 233, 26, -19, "competitive"],
  ["Pearl", 1500, 33, "W", 13, 6, "Jett", 20, 22, 9, 6, 312, 34, 22, "competitive"],
  ["Icebox", 1560, 18, "W", 13, 2, "Killjoy", 20, 0, 0, 0, 0, null, 0, "swiftplay"],
  ["Ascent", 1620, 42, "L", 11, 13, "Jett", 20, 20, 17, 5, 247, 29, -15, "competitive"],
  ["Lotus", 2950, 40, "W", 13, 8, "Omen", 20, 16, 12, 12, 214, 21, 20, "competitive"],
  ["Bind", 3010, 36, "L", 9, 13, "Jett", 20, 14, 16, 4, 198, 24, -17, "competitive"],
  ["Haven", 4400, 31, "W", 13, 5, "Jett", 19, 25, 10, 3, 331, 33, 24, "unrated"],
  ["Split", 4460, 47, "L", 12, 14, "Sova", 19, 21, 20, 9, 238, 23, -18, "competitive"],
];

function row(r: Raw): HistoryRow {
  const [map, mins, dur, res, us, them, agent, tier, k, d, a, acs, hs, rr, queue] = r;
  const startMs = DEMO_NOW - (mins + dur) * 60_000;
  const comp = queue === "competitive";
  const hasStats = acs > 0;
  return {
    matchId: sessionMatchId(startMs, res, MAPS[map]),
    mapId: MAPS[map],
    queueId: queue,
    startMs,
    durationMs: dur * 60_000,
    result: res === "W" ? "win" : "loss",
    roundsWon: us,
    roundsLost: them,
    agentId: AG[agent],
    kills: hasStats ? k : null,
    deaths: hasStats ? d : null,
    assists: hasStats ? a : null,
    kd: hasStats ? Math.round((k / Math.max(d, 1)) * 100) / 100 : null,
    acs: hasStats ? acs : null,
    hsPct: hs === null ? null : hs / 100,
    rrEarned: comp ? rr : null,
    tierAfter: comp ? tier : null,
  };
}

export function demoHistory(queue: HistoryQueue, offset: number, limit: number, empty = false): HistoryPage {
  const all = empty ? [] : RAW.map(row);
  const comp = all.filter((r) => r.queueId === "competitive");
  const list = queue === "competitive" ? comp : all;
  return { rows: list.slice(offset, offset + limit), total: list.length, competitiveTotal: comp.length, allTotal: all.length };
}
