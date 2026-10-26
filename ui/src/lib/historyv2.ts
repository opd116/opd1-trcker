// Match History and the scoreboard (design v2, TrackerApp.dc.html 523–657; logic `stamp` 1389–1404, `historyRows`
// 1593–1616, `scoreboard` 1627–1674, `extraVals` 1741–1769). Pure: the screens render what these return. Local time,
// en-US / en-GB written out by hand (no Intl, like format.ts: it would put a narrow no-break space before AM/PM).
import { DASH, durationMin, rrDelta, time12 } from "./format";
import type { HistoryRow, MatchDetailsView, MatchTeamView, MyStats, Result, ScoreRow } from "./types";

const MIN = 60_000;
const DAY = 86_400_000;
const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const WEEKDAYS = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];

/** Whole local calendar days between two instants (`now`'s day minus `ts`'s day). */
export function dayDiff(tsMs: number, nowMs: number): number {
  const a = new Date(tsMs);
  const b = new Date(nowMs);
  const da = Date.UTC(a.getFullYear(), a.getMonth(), a.getDate());
  const db = Date.UTC(b.getFullYear(), b.getMonth(), b.getDate());
  return Math.round((db - da) / DAY);
}

/** `Thu, 22 Oct 2026` (the design's en-GB weekday-short date). */
export function dateGb(tsMs: number): string {
  const d = new Date(tsMs);
  return `${WEEKDAYS[d.getDay()]!.slice(0, 3)}, ${d.getDate()} ${MONTHS[d.getMonth()]} ${d.getFullYear()}`;
}

/** The design's `stamp()`: the relative text, the exact date + time, the clock time and the day-group label. */
export interface Stamp {
  /** `45 min ago`, `3 h ago` (same day), `Yesterday`, `12 days ago`, `Sep 15` (30+ days). */
  rel: string;
  /** `Thu, 22 Oct 2026 · 9:55 PM`. */
  exact: string;
  /** `9:55 PM`. */
  hm: string;
  dayDiff: number;
  /** `Today`, `Yesterday`, `Thursday, Oct 18` (`, 2025` when not this year). */
  dayLabel: string;
}

export function stamp(tsMs: number, nowMs: number): Stamp {
  const mins = Math.max(0, (nowMs - tsMs) / MIN);
  const dd = dayDiff(tsMs, nowMs);
  const d = new Date(tsMs);
  const hm = time12(tsMs);
  const thisYear = d.getFullYear() === new Date(nowMs).getFullYear();
  const monthDay = `${MONTHS[d.getMonth()]} ${d.getDate()}`;
  let rel: string;
  if (mins < 60) rel = `${Math.max(1, Math.round(mins))} min ago`;
  else if (dd <= 0) rel = `${Math.floor(mins / 60)} h ago`;
  else if (dd === 1) rel = "Yesterday";
  else if (dd < 30) rel = `${dd} days ago`;
  else rel = thisYear ? monthDay : `${monthDay}, ${d.getFullYear()}`;
  const dayLabel =
    dd <= 0 ? "Today" : dd === 1 ? "Yesterday" : `${WEEKDAYS[d.getDay()]}, ${monthDay}${thisYear ? "" : `, ${d.getFullYear()}`}`;
  return { rel, exact: `${dateGb(tsMs)} · ${hm}`, hm, dayDiff: dd, dayLabel };
}

/** `5 days`, `1 month`, `14 months`: the design's `span` for a last-seen 30+ days back (whole months, else days). */
export function monthsOrDays(tsMs: number, nowMs: number): string {
  const from = new Date(tsMs);
  const to = new Date(nowMs);
  let m = (to.getFullYear() - from.getFullYear()) * 12 + to.getMonth() - from.getMonth();
  const add = (n: number) => {
    const a = new Date(from);
    a.setMonth(a.getMonth() + n);
    return a;
  };
  if (m > 0 && add(m).getTime() > nowMs) m--;
  const days = Math.max(0, Math.floor((nowMs - add(Math.max(m, 0)).getTime()) / DAY));
  const pl = (n: number, w: string) => `${n} ${n === 1 ? w : `${w}s`}`;
  return m > 0 ? pl(m, "month") : pl(days, "day");
}

/**
 * When a match ended (start + duration; the start when the duration is unknown). The design times every match by
 * its end: `45 min ago · 38 min`, the day groups, the scoreboard's `Today · 9:55 PM`.
 */
export const endMs = (m: { startMs: number; durationMs: number | null }): number => m.startMs + (m.durationMs ?? 0);

export const isComp = (r: { queueId: string }): boolean => r.queueId === "competitive";

/** Riot's queue ids → the mode's name (the History row, the scoreboard header, the recap, the queue timer). */
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
  newmap: "New Map",
  snowball: "Snowball Fight",
  "": "Custom",
};

/** The mode's name for a queue id; an id the table lacks is shown title-cased (`new_mode` → `New Mode`). */
export function queueLabel(queueId: string): string {
  if (Object.hasOwn(QUEUES, queueId)) return QUEUES[queueId]!;
  return queueId
    .split(/[\s_-]+/)
    .filter(Boolean)
    .map((w) => w[0]!.toUpperCase() + w.slice(1))
    .join(" ");
}

// ── the list ──────────────────────────────────────────────────────────────────────
export interface DayGroup {
  /** The day's label, which is also its key. */
  label: string;
  rows: HistoryRow[];
  wins: number;
  losses: number;
  /** Oldest → newest, left to right (the design reverses the newest-first rows). */
  strip: Result[];
  /** Sum of the day's competitive RR changes; null when none of its competitive rows has one (the pill is left out). */
  rr: number | null;
}

/** Newest-first rows into day groups (by when each ended) (Today / Yesterday / `Thursday, Oct 18`), in order. */
export function groupDays(rows: readonly HistoryRow[], nowMs: number): DayGroup[] {
  const out: DayGroup[] = [];
  for (const r of rows) {
    const label = stamp(endMs(r), nowMs).dayLabel;
    let g = out.find((x) => x.label === label);
    if (!g) {
      g = { label, rows: [], wins: 0, losses: 0, strip: [], rr: null };
      out.push(g);
    }
    g.rows.push(r);
    if (r.result === "win") g.wins++;
    else if (r.result === "loss") g.losses++;
    if (isComp(r) && typeof r.rrEarned === "number") g.rr = (g.rr ?? 0) + r.rrEarned;
  }
  for (const g of out) g.strip = g.rows.map((r) => r.result).reverse();
  return out;
}

/**
 * The row's sub-line: `45 min ago · 38 min` today, `11:12 PM · 42 min` on an older day; the duration is dropped when
 * unknown. A non-competitive row (only listed under "All modes", where its RR and rank cells are dashes) leads with its
 * mode: `Swiftplay · 45 min ago · 18 min`.
 */
export function whenText(r: Pick<HistoryRow, "startMs" | "durationMs"> & { queueId?: string }, nowMs: number): string {
  const s = stamp(endMs(r), nowMs);
  const parts = [s.dayDiff <= 0 ? s.rel : s.hm];
  if (r.queueId !== undefined && !isComp({ queueId: r.queueId })) parts.unshift(queueLabel(r.queueId));
  if (r.durationMs !== null) parts.push(durationMin(r.durationMs));
  return parts.join(" · ");
}

/** The design's history K/D colour: ≥ 1.3 white, < 1 dimmed, else the normal band (`extraVals` 1764). */
export function kdTone(kd: number | null): "hi" | "lo" | "mid" | "none" {
  if (kd === null || !Number.isFinite(kd)) return "none";
  return kd >= 1.3 ? "hi" : kd < 1 ? "lo" : "mid";
}

/** The design's tooltip for a missing HS% on a match that has stats (`hsMissing`). */
export const HS_MISSING_TIP = "Not available: you reconnected mid-match, so hit data is incomplete";

/** HS% cell: the value, or `—` with the reconnect tooltip when the match has stats but no hit data. */
export function hsCell(r: Pick<HistoryRow, "hsPct" | "kills">): { text: string; tip: string } {
  if (r.hsPct !== null) return { text: `${Math.round(r.hsPct * 100)}%`, tip: "" };
  return { text: DASH, tip: r.kills !== null ? HS_MISSING_TIP : "" };
}

/** `24 / 14 / 5`, or `—` without a stats row. */
export function kdaText(k: number | null, d: number | null, a: number | null): string {
  return k === null || d === null || a === null ? DASH : `${k} / ${d} / ${a}`;
}

/** The RR cell: competitive rows with a known change only (`+18`, `−19`, `0`); everything else `—`. */
export function rrCell(r: Pick<HistoryRow, "queueId" | "rrEarned">): { text: string; tone: "win" | "loss" | "flat" } {
  if (!isComp(r) || r.rrEarned === null) return { text: DASH, tone: "flat" };
  return { text: rrDelta(r.rrEarned), tone: r.rrEarned > 0 ? "win" : r.rrEarned < 0 ? "loss" : "flat" };
}

const RESULT_WORD: Record<Result, string> = { win: "win", loss: "loss", draw: "draw" };
export const RESULT_LETTER: Record<Result, string> = { win: "W", loss: "L", draw: "D" };

/** The row's accessible name: `Lotus, win 13 to 9, plus 18 RR. Open scoreboard`. */
export function rowAria(r: HistoryRow, mapName: string | null): string {
  let s = `${mapName ?? "Unknown map"}, ${RESULT_WORD[r.result]}`;
  if (r.roundsWon !== null && r.roundsLost !== null) s += ` ${r.roundsWon} to ${r.roundsLost}`;
  if (isComp(r) && r.rrEarned !== null) s += `, ${r.rrEarned >= 0 ? "plus" : "minus"} ${Math.abs(r.rrEarned)} RR`;
  return `${s}. Open scoreboard`;
}

/** `N stored match(es)`. */
const stored = (n: number) => `${n} stored ${n === 1 ? "match" : "matches"}`;

/** The footer under the list (`histFoot`): all of them, or how many of the total are shown so far. */
export function footText(shown: number, total: number): string {
  const head = shown >= total ? `Showing all ${stored(total)}, newest first.` : `Showing ${shown} of ${stored(total)}, newest first.`;
  return `${head} Matches are stored when they end while OPD1 is running; older ones can’t be fetched from Riot.`;
}

// ── the scoreboard ────────────────────────────────────────────────────────────────
/** The hero's second line: `Competitive · Thu, 22 Oct 2026 · 9:55 PM · 38 min` (no duration when unknown). */
export function heroMeta(v: Pick<MatchDetailsView, "queueId" | "startMs" | "durationMs">): string {
  const end = endMs(v);
  const parts = [queueLabel(v.queueId), dateGb(end), time12(end)];
  if (v.durationMs !== null) parts.push(durationMin(v.durationMs));
  return parts.join(" · ");
}

/** `First half 7–5`, `Went to overtime` (more than 24 rounds), or null (a no-team mode, or a half unknown). */
export function halvesLine(v: MatchDetailsView): string | null {
  if (v.teams.length < 2) return null;
  const [a, b] = v.teams as [MatchTeamView, MatchTeamView];
  if (a.roundsWon !== null && b.roundsWon !== null && a.roundsWon + b.roundsWon > 24) return "Went to overtime";
  if (a.firstHalf === null || b.firstHalf === null) return null;
  return `First half ${a.firstHalf}–${b.firstHalf}`;
}

export const RESULT_WORD_BIG: Record<Result, string> = { win: "VICTORY", loss: "DEFEAT", draw: "DRAW" };

/** Rows by ACS, highest first; a row without stats goes last (stable otherwise). */
export function byAcs(rows: readonly ScoreRow[]): ScoreRow[] {
  return [...rows].sort((a, b) => (b.acs ?? -1) - (a.acs ?? -1));
}

/** The match MVP (highest ACS of everyone, first on a tie) and, per team, its own top row: `MATCH MVP` / `TEAM MVP`. */
export function mvpOf(v: MatchDetailsView): { match: string | null; team: Set<string> } {
  let best: ScoreRow | null = null;
  const team = new Set<string>();
  for (const t of v.teams) {
    const top = byAcs(t.players)[0];
    if (top && top.acs !== null) {
      team.add(top.puuid);
      if (!best || top.acs > (best.acs ?? -1)) best = top;
    }
  }
  // A no-team mode has one block: its top row is the match MVP and nobody is a team MVP.
  if (v.teams.length < 2) team.clear();
  if (best) team.delete(best.puuid);
  return { match: best?.puuid ?? null, team };
}

/**
 * YOUR RR on the user's own competitive scoreboard: the RR change and the rank after it. Only a history row knows that
 * rank (`tierAfter`); My Stats' rows carry the rank during the match, which differs on a promotion or demotion game,
 * so they give the RR change alone.
 */
export function yourRrOf(
  matchId: string,
  row: Pick<HistoryRow, "rrEarned" | "tierAfter"> | null | undefined,
  ms: Pick<MyStats, "last5" | "session"> | null | undefined,
): { rr: number; tier: number | null } | null {
  if (row && row.rrEarned !== null) return { rr: row.rrEarned, tier: row.tierAfter };
  const m = ms?.last5.find((x) => x.matchId === matchId) ?? ms?.session?.matches.find((x) => x.matchId === matchId);
  if (m && m.rrEarned !== null) return { rr: m.rrEarned, tier: null };
  return null;
}

/** `0.62`; deaths of 0 count as 1 (the design's K/D never divides by zero); `—` without a stats row. */
export function kdOf(k: number | null, d: number | null): string {
  if (k === null || d === null) return DASH;
  return (k / Math.max(d, 1)).toFixed(2);
}
