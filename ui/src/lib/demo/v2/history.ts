// DEV-only (R189): `match_history` from the design's 13 sample rows (TrackerApp.dc.html `historyRows()`, kept in
// cdv8.ts as HISTORY_RAW). Each row's id opens a scoreboard built from that same row (cdv8.ts `historyMatch`), so the
// score, K/D/A, ACS, HS% and duration agree between the list and the scoreboard.
import type { HistoryPage, HistoryQueue, HistoryRow } from "../../types";
import { AG, HISTORY_RAW, historyMatchId, historyStartMs, MAPS, type HistoryRaw } from "../cdv8";

function row(r: HistoryRaw, i: number): HistoryRow {
  const [map, , dur, res, us, them, agent, tier, k, d, a, acs, hs, rr, queue] = r;
  const startMs = historyStartMs(r);
  const comp = queue === "competitive";
  const hasStats = acs > 0;
  return {
    matchId: historyMatchId(i),
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
  const all = empty ? [] : HISTORY_RAW.map(row);
  const comp = all.filter((r) => r.queueId === "competitive");
  const list = queue === "competitive" ? comp : all;
  return { rows: list.slice(offset, offset + limit), total: list.length, competitiveTotal: comp.length, allTotal: all.length };
}
