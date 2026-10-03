// DEV-only. The My Stats demo data (spec §9.6): a simulated act of competitive matches, aggregated the way
// crates/core/src/mystats.rs does (same rules, same insight wording), so every figure on the page agrees with every
// other. Deterministic (a seeded generator, no clock), anchored at DEMO_NOW; the last five matches are the design's own
// recent rows of the user (Fahad), so a row opens the same Match page as the Details page does.
// Nothing here ships: it is reached only through the DEV-guarded scene registry.
import type { Insight, MatchSummary, MyStats, RankChange, StatRow, StreakMark, WinCell } from "../types";
import { AG, DEMO_NOW, detailsFor, MAPS } from "./cdv8";

const MIN = 60_000;
const HOUR = 3_600_000;
const DAY = 86_400_000;
const at = (m: number, d: number, h = 0, mi = 0) => Date.UTC(2026, m, d, h, mi);

/** The current act of the demo (V26 A5): 2026-08-26, which is day 0 of the simulation. */
const ACT_START = at(7, 26);

type Res = "win" | "loss" | "draw";

interface Sim {
  startMs: number;
  res: Res;
  map: string;
  agent: string;
  kills: number;
  deaths: number;
  score: number;
  rounds: number;
  rr: number;
  abs: number;
}

/** A small deterministic generator (the classic LCG), so the demo never changes between runs. */
function rng(seed: number): () => number {
  let s = seed >>> 0;
  return () => {
    s = (Math.imul(s, 1664525) + 1013904223) >>> 0;
    return s / 4294967296;
  };
}

const pick = <T>(r: () => number, items: readonly T[], weights: readonly number[]): T => {
  const total = weights.reduce((a, b) => a + b, 0);
  let x = r() * total;
  for (let i = 0; i < items.length; i++) {
    x -= weights[i]!;
    if (x < 0) return items[i]!;
  }
  return items[items.length - 1]!;
};

/** 7 maps and 8 agents, most played first. */
const MAP_SET = [MAPS.Ascent, MAPS.Split, MAPS.Bind, MAPS.Lotus, MAPS.Sunset, MAPS.Haven, MAPS.Icebox] as const;
const MAP_WEIGHTS = [14, 11, 9, 8, 7, 6, 5];
const AGENT_SET = [AG.Jett, AG.Omen, AG.Sova, AG.Killjoy, AG.Brimstone, AG.Reyna, AG.Clove, AG.Breach] as const;
const AGENT_WEIGHTS = [22, 12, 8, 6, 5, 4, 3, 2];

const winRr = (r: () => number) => 14 + Math.floor(r() * 12);
const lossRr = (r: () => number) => -(12 + Math.floor(r() * 11));

/** The user's own recent rows (the design's mock for Fahad), newest first: the last five matches of the act. */
function userRecent(): MatchSummary[] {
  return detailsFor("fahad").recent.slice(0, 5);
}

/** Today's earlier matches of the session (UTC, the clock the Playwright suite uses). */
const TODAY = [
  [7, 40], [8, 25], [9, 10], [9, 55], [10, 40], [11, 30], [12, 20], [13, 15], [14, 5], [14, 50], [15, 40], [16, 30],
] as const;

/**
 * The simulated act, oldest first: 43 older matches over the act's first 33 days, the user's two matches of the
 * evening before, today's 12 earlier matches, and the user's last three (today). 60 in all. `finalAbs` is where the
 * ladder ends (`tier × 100 + rr`, or `2400 + rr` from Immortal 1): the start is solved from it.
 */
function simulate(finalAbs: number, seed = 20260929): Sim[] {
  const r = rng(seed);
  const stats = (res: Res) => {
    const kills = 9 + Math.floor(r() * 18);
    const deaths = 8 + Math.floor(r() * 12);
    const rounds = 18 + Math.floor(r() * 9);
    // ACS 150-340: a base, 8.5 per kill and a bump for a win; the score is ACS over the rounds played.
    const acs = 90 + kills * 8.5 + (res === "win" ? 22 : 0) + r() * 30;
    return { kills, deaths, rounds, score: Math.round(acs * rounds) };
  };
  const result = (): Res => {
    const x = r();
    return x < 0.03 ? "draw" : x < 0.6 ? "win" : "loss";
  };
  const make = (startMs: number, res: Res, map?: string, agent?: string): Omit<Sim, "abs"> => ({
    startMs,
    res,
    map: map ?? pick(r, MAP_SET, MAP_WEIGHTS),
    agent: agent ?? pick(r, AGENT_SET, AGENT_WEIGHTS),
    ...stats(res),
    rr: res === "win" ? winRr(r) : res === "loss" ? lossRr(r) : 0,
  });

  const older: Array<Omit<Sim, "abs">> = [];
  for (let k = 0; k < 43; k++) {
    const day = Math.floor((k * 33) / 43);
    const hour = 11 + Math.floor(r() * 12);
    older.push(make(ACT_START + day * DAY + hour * HOUR + Math.floor(r() * 60) * MIN, result()));
  }
  older.sort((a, b) => a.startMs - b.startMs);

  const recent = userRecent(); // newest first
  const fromRow = (m: MatchSummary): Omit<Sim, "abs"> => ({
    startMs: m.startMs,
    res: m.result,
    map: m.mapId,
    agent: m.agentId ?? AG.Jett,
    kills: m.kills ?? 0,
    deaths: m.deaths ?? 1,
    rounds: (m.roundsWon ?? 13) + (m.roundsLost ?? 10),
    score: Math.round((m.acs ?? 200) * ((m.roundsWon ?? 13) + (m.roundsLost ?? 10))),
    rr: m.rrEarned ?? 0,
  });
  const today = TODAY.map(([h, mi]) => make(at(8, 29, h, mi), result()));
  const sims = [...older, fromRow(recent[4]!), fromRow(recent[3]!), ...today, fromRow(recent[2]!), fromRow(recent[1]!), fromRow(recent[0]!)];

  // The ladder is one line in absolute RR: every match adds its RR change, so the start is the end less the sum.
  const start = finalAbs - sims.reduce((a, s) => a + s.rr, 0);
  let abs = start;
  return sims.map((s) => ({ ...s, abs: (abs += s.rr) }));
}

// ── the aggregation (mirrors crates/core/src/mystats.rs) ────────────────────────────────────────────────────────

const tierOf = (abs: number) => (abs >= 2400 ? 24 : Math.floor(abs / 100));
const mean = (xs: number[]): number | null => (xs.length ? xs.reduce((a, b) => a + b, 0) / xs.length : null);

interface Tally {
  games: number;
  wins: number;
  losses: number;
  kills: number;
  deaths: number;
  score: number;
  rounds: number;
  rr: number;
}

const tally = (sims: Sim[]): Tally =>
  sims.reduce<Tally>(
    (t, s) => ({
      games: t.games + 1,
      wins: t.wins + (s.res === "win" ? 1 : 0),
      losses: t.losses + (s.res === "loss" ? 1 : 0),
      kills: t.kills + s.kills,
      deaths: t.deaths + s.deaths,
      score: t.score + s.score,
      rounds: t.rounds + s.rounds,
      rr: t.rr + s.rr,
    }),
    { games: 0, wins: 0, losses: 0, kills: 0, deaths: 0, score: 0, rounds: 0, rr: 0 },
  );

function table(sims: Sim[], key: (s: Sim) => string): StatRow[] {
  const groups = new Map<string, Sim[]>();
  for (const s of sims) groups.set(key(s), [...(groups.get(key(s)) ?? []), s]);
  return [...groups.entries()]
    .map(([id, g]): StatRow => {
      const t = tally(g);
      return {
        id,
        games: t.games,
        wins: t.wins,
        losses: t.losses,
        winrate: t.wins / t.games,
        netRr: t.rr,
        kd: t.deaths === 0 ? t.kills : t.kills / t.deaths,
        acs: t.rounds > 0 ? t.score / t.rounds : null,
      };
    })
    .sort((a, b) => b.games - a.games || b.wins - a.wins || (a.id < b.id ? -1 : 1));
}

/** Local time = UTC here (the suite pins the zone). Weekday 0 = Monday. */
const weekdayOf = (ms: number) => (new Date(ms).getUTCDay() + 6) % 7;
const daypartOf = (ms: number) => {
  const h = new Date(ms).getUTCHours();
  return h >= 5 && h < 12 ? 0 : h >= 12 && h < 17 ? 1 : h >= 17 && h < 22 ? 2 : 3;
};

function cells(sims: Sim[], n: number, index: (s: Sim) => number): WinCell[] {
  return Array.from({ length: n }, (_, i): WinCell => {
    const g = sims.filter((s) => index(s) === i);
    const wins = g.filter((s) => s.res === "win").length;
    return { games: g.length, wins, winrate: g.length ? wins / g.length : null };
  });
}

const MIN_GAMES = 5;
type Bucket = { label: string; games: number; wins: number };
const pct = (b: Bucket) => Math.round((100 * b.wins) / b.games);

function bestAndWorst(buckets: Bucket[]): [Bucket, Bucket] | null {
  const ranked = buckets
    .filter((b) => b.games >= MIN_GAMES)
    .sort((a, b) => b.wins * a.games - a.wins * b.games || b.games - a.games);
  const best = ranked[0];
  const worst = ranked[ranked.length - 1];
  return best && worst && pct(best) > pct(worst) ? [best, worst] : null;
}

const compare = (b: Bucket, w: Bucket) =>
  `You win ${pct(b)}% of ${b.games} ${b.label} games, compared with ${pct(w)}% of ${w.games} ${w.label} games.`;

const signed = (n: number) => (n >= 0 ? `+${n}` : `${n}`);

function insights(sims: Sim[], maps: StatRow[], mapNames: Record<string, string>, now: number): Insight[] {
  const out: Insight[] = [];
  const mapBest = bestAndWorst(maps.map((m) => ({ label: m.id, games: m.games, wins: m.wins })));
  if (mapBest) {
    const [best, worst] = mapBest;
    const line = (b: Bucket, kind: Insight["kind"], word: string, positive: boolean) => {
      const name = mapNames[b.label];
      if (name) out.push({ kind, text: `${name} is your ${word} map: ${pct(b)}% wins in ${b.games} games.`, matches: b.games, positive });
    };
    line(best, "bestMap", "best", true);
    line(worst, "worstMap", "weakest", false);
  }
  const parts = ["morning", "afternoon", "evening", "night"];
  const dp = bestAndWorst(
    cells(sims, 4, (s) => daypartOf(s.startMs)).map((c, i) => ({ label: parts[i]!, games: c.games, wins: c.wins })),
  );
  if (dp) out.push({ kind: "timeOfDay", text: compare(...dp), matches: dp[0].games + dp[1].games, positive: true });

  const we: Bucket = { label: "weekend", games: 0, wins: 0 };
  const wd: Bucket = { label: "weekday", games: 0, wins: 0 };
  for (const s of sims) {
    const b = weekdayOf(s.startMs) >= 5 ? we : wd;
    b.games++;
    if (s.res === "win") b.wins++;
  }
  const ww = bestAndWorst([we, wd]);
  if (ww) out.push({ kind: "weekdayWeekend", text: compare(...ww), matches: ww[0].games + ww[1].games, positive: true });

  const wins = sims.filter((s) => s.res === "win").map((s) => s.rr);
  const losses = sims.filter((s) => s.res === "loss").map((s) => s.rr);
  if (Math.min(wins.length, losses.length) >= MIN_GAMES) {
    const aw = Math.round(mean(wins)!);
    const al = Math.round(mean(losses)!);
    out.push({
      kind: "rrEconomy",
      text: `Average ${signed(aw)} RR on wins and ${signed(al)} RR on losses.`,
      matches: wins.length + losses.length,
      positive: aw + al >= 0,
    });
  }
  const week = sims.filter((s) => s.startMs >= now - 7 * DAY);
  if (week.length >= MIN_GAMES) {
    const net = week.reduce((a, s) => a + s.rr, 0);
    out.push({ kind: "last7Days", text: `${signed(net)} RR over ${week.length} games in the last 7 days.`, matches: week.length, positive: net >= 0 });
  }
  return out;
}

const mark = (r: Res): StreakMark => (r === "win" ? "W" : r === "loss" ? "L" : "D");

/** The map names the Rust side hands the insights, from the demo meta. */
export type NameTable = Record<string, string>;

/** The whole page from a simulated act, the way `mystats::build` builds it. */
function build(sims: Sim[], mapNames: NameTable, rank: { tier: number; rr: number }): MyStats {
  const t = tally(sims);
  const wins = sims.filter((s) => s.res === "win" && s.rr !== 0).map((s) => s.rr);
  const losses = sims.filter((s) => s.res === "loss" && s.rr !== 0).map((s) => s.rr);
  const changes: RankChange[] = [];
  for (let i = sims.length - 1; i > 0 && changes.length < 5; i--) {
    const [a, b] = [tierOf(sims[i - 1]!.abs), tierOf(sims[i]!.abs)];
    if (a !== b) changes.push({ tier: b, up: b > a, startMs: sims[i]!.startMs });
  }

  // The session: the run of matches at the end with no gap of 6h between two starts.
  let from = sims.length - 1;
  while (from > 0 && sims[from]!.startMs - sims[from - 1]!.startMs < 6 * HOUR) from--;
  const sessionSims = sims.slice(from);
  const live = DEMO_NOW - sims[sims.length - 1]!.startMs <= 6 * HOUR;
  const st = tally(sessionSims);

  const recent = userRecent();
  const maps = table(sims, (s) => s.map);
  return {
    rank: {
      tier: rank.tier,
      rr: rank.rr,
      wins: t.wins,
      losses: t.losses,
      rrToNext: rank.tier >= 3 && rank.tier < 24 ? 100 - rank.rr : null,
      netRr: t.rr,
      avgWin: mean(wins),
      avgLoss: mean(losses),
      changes,
    },
    rrSeries: sims.map((s) => ({ startMs: s.startMs, absoluteRr: s.abs })),
    session: live
      ? { netRr: st.rr, wins: st.wins, losses: st.losses, startedMs: sessionSims[0]!.startMs, streak: sessionSims.map((s) => mark(s.res)) }
      : null,
    last5: recent,
    maps,
    agents: table(sims, (s) => s.agent),
    byWeekday: cells(sims, 7, (s) => weekdayOf(s.startMs)) as MyStats["byWeekday"],
    byDaypart: cells(sims, 4, (s) => daypartOf(s.startMs)) as MyStats["byDaypart"],
    insights: insights(sims, maps, mapNames, DEMO_NOW),
  };
}

/** Immortal and Radiant RR count from 2400 (Immortal 1's floor): the ladder's absolute value for tier 24, `rr` RR. */
const IMMORTAL_BASE = 2400;

/** The demo map names for the insights (the Rust side builds this from valorant-api). */
export const MAP_NAMES: NameTable = {
  [MAPS.Ascent]: "Ascent",
  [MAPS.Split]: "Split",
  [MAPS.Bind]: "Bind",
  [MAPS.Lotus]: "Lotus",
  [MAPS.Sunset]: "Sunset",
  [MAPS.Haven]: "Haven",
  [MAPS.Icebox]: "Icebox",
  [MAPS.Pearl]: "Pearl",
};

/** Ascendant 1, 21 RR: the same rank as the user's live card in the demo roster. About 60 RR points, 7 maps, 8 agents, a session of 15, 6 insights. */
export function myStatsFull(): MyStats {
  return build(simulate(2121), MAP_NAMES, { tier: 21, rr: 21 });
}

/** Immortal 1 with 87 RR: no next tier (no progress bar), and the chart climbs above the 2400 floor. */
export function myStatsImmortal(): MyStats {
  return build(simulate(IMMORTAL_BASE + 87, 7), MAP_NAMES, { tier: 24, rr: 87 });
}

/** The tier is known and the exact RR is not (the MMR call gave no answer): `— RR`, and no progress bar to draw. */
export function myStatsRrNull(): MyStats {
  const s = myStatsFull();
  return { ...s, rank: { ...s.rank, rr: null, rrToNext: null } };
}

/**
 * A new account: three placement games, no rank yet. No RR anywhere (null, never a counted 0), nothing to chart,
 * three maps and two agents, and too little for an insight.
 */
export function myStatsUnranked(): MyStats {
  const recent = userRecent()
    .slice(0, 3)
    .map((m): MatchSummary => ({ ...m, rrEarned: null, tier: null }));
  const isWin = (m: MatchSummary) => (m.result === "win" ? 1 : 0);
  const isLoss = (m: MatchSummary) => (m.result === "loss" ? 1 : 0);
  /** One row per distinct key of `recent`, as the Maps and Agents tables group them (no RR: net null). */
  const rows = (key: (m: MatchSummary) => string): StatRow[] => {
    const groups = new Map<string, MatchSummary[]>();
    for (const m of recent) groups.set(key(m), [...(groups.get(key(m)) ?? []), m]);
    return [...groups.entries()].map(([id, g]): StatRow => {
      const wins = g.reduce((a, m) => a + isWin(m), 0);
      const kills = g.reduce((a, m) => a + (m.kills ?? 0), 0);
      const deaths = g.reduce((a, m) => a + (m.deaths ?? 0), 0);
      const acs = g.map((m) => m.acs).filter((x): x is number => x !== null);
      return {
        id,
        games: g.length,
        wins,
        losses: g.reduce((a, m) => a + isLoss(m), 0),
        winrate: wins / g.length,
        netRr: null,
        kd: deaths === 0 ? kills : kills / deaths,
        acs: acs.length ? acs.reduce((a, b) => a + b, 0) / acs.length : null,
      };
    });
  };
  /** Weekday and time-of-day cells of `recent`. */
  const buckets = (n: number, index: (ms: number) => number): WinCell[] =>
    Array.from({ length: n }, (_, i): WinCell => {
      const g = recent.filter((m) => index(m.startMs) === i);
      const wins = g.reduce((a, m) => a + isWin(m), 0);
      return { games: g.length, wins, winrate: g.length ? wins / g.length : null };
    });
  const wins = recent.reduce((a, m) => a + isWin(m), 0);
  const losses = recent.reduce((a, m) => a + isLoss(m), 0);
  return {
    rank: { tier: null, rr: null, wins, losses, rrToNext: null, netRr: null, avgWin: null, avgLoss: null, changes: [] },
    rrSeries: [],
    session: {
      netRr: null,
      wins,
      losses,
      startedMs: recent[recent.length - 1]!.startMs,
      streak: [...recent].reverse().map((m) => mark(m.result)),
    },
    last5: recent,
    maps: rows((m) => m.mapId),
    agents: rows((m) => m.agentId ?? AG.Jett),
    byWeekday: buckets(7, weekdayOf) as MyStats["byWeekday"],
    byDaypart: buckets(4, daypartOf) as MyStats["byDaypart"],
    insights: [],
  };
}

/**
 * The widest numbers and names: Ascendant 3 with 3-digit games, a 128-game map, net RR in the hundreds, a session of 30
 * that wraps its streak, 3-digit ACS, the longest real map and agent names, and insights at their longest.
 */
export function myStatsLongnames(): MyStats {
  const base = build(simulate(2399, 11), MAP_NAMES, { tier: 23, rr: 99 });
  /** Row `i` of a table at its widest: the first row has the longest numbers, the rest step down and change sign. */
  const big = (r: StatRow, i: number, games: number): StatRow => {
    const wins = Math.round(games * Math.min(0.9, 0.38 + 0.09 * i));
    return {
      ...r,
      games,
      wins,
      losses: games - wins,
      winrate: wins / games,
      netRr: (i % 2 === 0 ? -1 : 1) * (312 - i * 37),
      kd: 2.34 - i * 0.11,
      acs: 312.4 - i * 9,
    };
  };
  const maps = base.maps.map((m, i) => big(m, i, 128 - i * 11));
  const agents = base.agents.map((a, i) => big(a, i, 128 - i * 13));
  const streak: StreakMark[] = Array.from({ length: 30 }, (_, i) => (i % 7 === 3 ? "L" : i % 11 === 5 ? "D" : "W"));
  return {
    ...base,
    rank: { ...base.rank, wins: 301, losses: 188, netRr: -312, avgWin: 24.6, avgLoss: -21.4, rrToNext: 1 },
    session: {
      netRr: -128,
      wins: streak.filter((m) => m === "W").length,
      losses: streak.filter((m) => m === "L").length,
      startedMs: DEMO_NOW - 13 * HOUR,
      streak,
    },
    maps,
    agents,
    insights: [
      { kind: "bestMap", text: "Basic Training is your best map: 100% wins in 128 games.", matches: 128, positive: true },
      {
        kind: "worstMap",
        text: "Corrode is your weakest map: 38% wins in 117 games, and the gap to every other map is large.",
        matches: 117,
        positive: false,
      },
      { kind: "rrEconomy", text: "Average +25 RR on wins and −21 RR on losses.", matches: 489, positive: true },
    ],
  };
}

/**
 * What the longest-names scene renames in the demo meta: its two most played maps and agents take valorant-api's longest
 * real names (`Basic Training`, `Corrode`, `Brimstone`, `Deadlock`), so the widest name sits on the widest numbers.
 */
export function longNameMeta(stats: MyStats): { maps: Record<string, { name: string }>; agents: Record<string, { name: string }> } {
  const top = (rows: StatRow[], i: number) => rows[i]!.id;
  return {
    maps: { [top(stats.maps, 0)]: { name: "Basic Training" }, [top(stats.maps, 1)]: { name: "Corrode" } },
    // The real Brimstone changes places with the most played agent, so no name shows twice.
    agents: {
      [top(stats.agents, 0)]: { name: "Brimstone" },
      [top(stats.agents, 1)]: { name: "Deadlock" },
      ...(top(stats.agents, 0) === AG.Brimstone ? {} : { [AG.Brimstone]: { name: "Jett" } }),
    },
  };
}
