// The v2 Live screen's view model (design v2 `.superpowers/design/v2/TrackerApp.dc.html`: `rows()` 1289–1350,
// `team()` 1352–1377, `renderVals` 2044–2151), mapped onto the real wire types. Pure: every function takes what it
// reads (live cards, meta, settings, marks, the clock) and returns plain data the components draw. Colours are CSS
// variables or the theme's tier/party hexes, never the prototype's literals.
//
// Tri-state (types.ts): an absent card field is still loading (a skeleton), null is missing ("—").
import { agentName, isRankedTier, mapName } from "./meta";
import { actDates, count, DASH, kd2, pct0, rrDelta, span } from "./format";
import { showEncounters, showParty } from "./live";
import { nameTip } from "./tips";
import { smurfSignal } from "./smurf";
import { hlHs, hlKd, hlWinPct, PARTY_COLORS, rgba, tierColor, tierName, tierShort } from "./theme";
import type { TipArg } from "./tooltip.svelte";
import type {
  AppStatus,
  DisplayName,
  LiveState,
  MatchDetailsView,
  MatchSummary,
  Meta,
  MyStats,
  PlayerCard,
  PlayerMarks,
  Result,
  ScoreRow,
  SessionMatch,
  SessionSummary,
  Settings,
  Stage,
  Team,
} from "./types";

// ── names (CLAUDE.md "Hidden names": only the Riot ID hides; the user's own card always shows their name) ──────────

export type NameStyle = "visible" | "remembered" | "hidden" | "missing" | "loading";

export interface NameView {
  style: NameStyle;
  /** What the name slot reads; "" while loading. */
  text: string;
  /** "#TAG", or "" for a hidden name (no tag) and the loading / missing states. */
  tag: string;
  tip: TipArg;
}

/**
 * A live player's name the way the v2 row draws it: a visible Riot ID (tooltip only when the name is cut off), a
 * remembered name (dotted underline; the tooltip names where it came from, R145), or a hidden one in italic:
 * `Hidden player` before the lock, the agent's name after it. Missing → "—"; absent → loading.
 */
export function nameView(n: DisplayName | null | undefined, meta: Meta | null): NameView {
  if (n === undefined) return { style: "loading", text: "", tag: "", tip: "" };
  if (n === null) return { style: "missing", text: DASH, tag: "", tip: "" };
  switch (n.kind) {
    case "visible":
      return { style: "visible", text: n.name, tag: `#${n.tag}`, tip: { text: `${n.name}#${n.tag}`, overflowOnly: true } };
    case "remembered":
      return { style: "remembered", text: n.name, tag: `#${n.tag}`, tip: { text: nameTip(n, meta), ifCut: `${n.name}#${n.tag}` } };
    case "hiddenPlayer":
      return { style: "hidden", text: "Hidden player", tag: "", tip: "This player hides their name" };
    case "hiddenAgent":
      return { style: "hidden", text: agentName(meta, n.agentId) ?? "Hidden player", tag: "", tip: "This player hides their name" };
  }
}

/** A name as plain words (party tooltip, aria labels, the glance callout): never a hidden player's real name. */
export function plainName(n: DisplayName | null | undefined, meta: Meta | null): string {
  const v = nameView(n, meta);
  return v.style === "loading" || v.style === "missing" ? "Player" : v.text;
}

// ── row fields ───────────────────────────────────────────────────────────────────────────────────────────────────

/** The newest results while they are the same (a win or a loss streak; a draw ends it). */
export interface StreakView {
  n: number;
  win: boolean;
  tip: string;
}

const LETTER: Record<Result, "W" | "L" | "D"> = { win: "W", loss: "L", draw: "D" };

/** The current streak from `last5` (newest first): shown from 2 same results on (prototype `rows()`). */
export function streakOf(last5: PlayerCard["last5"]): StreakView | null {
  if (!last5 || last5.length === 0) return null;
  const first = last5[0]!.result;
  if (first === "draw") return null;
  let n = 0;
  while (n < last5.length && last5[n]!.result === first) n++;
  if (n < 2) return null;
  const win = first === "win";
  const oldestFirst = last5.slice(0, 5).map((e) => LETTER[e.result]).reverse().join(" ");
  return { n: Math.min(n, 5), win, tip: `${win ? "Won" : "Lost"} the last ${n} matches\nLast 5, oldest first: ${oldestFirst}` };
}

export interface PeakView {
  delta: number;
  icon: number;
  color: string;
  tip: string;
}

/** ▲N beside the rank when the peak is N divisions higher; the image tooltip names the peak, its act and how long ago. */
export function peakOf(c: PlayerCard, meta: Meta | null): PeakView | null {
  const r = c.rank, p = c.peak;
  if (!r || !p || !isRankedTier(p.tier)) return null;
  const delta = p.tier - r.tier;
  if (delta <= 0) return null;
  const when = p.sinceActEndMs === null ? "This act" : `${p.actLabel} · ${span(p.sinceActEndMs)} ago`;
  return { delta, icon: p.tier, color: tierColor(p.tier), tip: `${tierName(p.tier, meta)}\n${when}\n${actDates(p.actStartMs, p.actEndMs)}` };
}

export interface RankView {
  state: "loading" | "missing" | "unranked" | "ranked";
  tier: number;
  name: string;
  color: string;
  rrText: string;
  /** 0–100, the bar's fill. */
  rrPct: number;
}

export function rankOf(c: PlayerCard, meta: Meta | null): RankView {
  const r = c.rank;
  if (r === undefined) return { state: "loading", tier: 0, name: "", color: "var(--text-dim)", rrText: "", rrPct: 0 };
  if (r === null) return { state: "missing", tier: 0, name: DASH, color: "var(--text-muted)", rrText: `${DASH} RR`, rrPct: 0 };
  const ranked = isRankedTier(r.tier);
  return {
    state: ranked ? "ranked" : "unranked",
    tier: ranked ? r.tier : 0,
    name: tierName(r.tier, meta),
    color: tierColor(r.tier),
    rrText: r.rr === null ? `${DASH} RR` : `${r.rr} RR`,
    rrPct: r.rr === null ? 0 : Math.max(0, Math.min(100, r.rr)),
  };
}

/** Colour of a party (group A–D) on screen. */
export const partyColor = (group: number): string => PARTY_COLORS[((group % 4) + 4) % 4]!;

export interface RowCtx {
  meta: Meta | null;
  /** Every live card (the smurf signal's lobby averages). */
  lobby: readonly PlayerCard[];
  settings: Pick<Settings, "highlight" | "smurf">;
  marks: PlayerMarks;
  /** `router.lastProfile`: the row stays marked after the drawer closes. */
  selected: string | null;
  stage: Stage;
}

export interface RowView {
  puuid: string;
  name: NameView;
  agentId: string | null;
  /** The agent's name, "Picking…" before the lock, undefined while meta is pending. */
  agent: string | undefined;
  picking: boolean;
  /** The agent tile's border. */
  agentBorder: string;
  isSelf: boolean;
  level: string | undefined;
  subColor: string;
  smurf: string | null;
  met: { tip: string; aria: string } | null;
  saved: boolean;
  note: string;
  party: { color: string; confirmed: boolean; top: boolean; bottom: boolean; tip: string } | null;
  rank: RankView;
  peak: PeakView | null;
  wr: { text: string; color: string; w: number; l: number } | null | undefined;
  streak: StreakView | null;
  games: string | undefined;
  kd: { text: string; color: string } | undefined;
  hs: { text: string; color: string } | undefined;
  selected: boolean;
  aria: string;
  /** Every stat is still loading: the row is drawn as the prototype's skeleton row. */
  statsLoading: boolean;
}

/** The row's stats have not arrived at all (prototype 238–255: agent and name show, the rest shimmers). */
export const statsLoading = (c: PlayerCard): boolean =>
  c.rank === undefined && c.actWinrate === undefined && c.kd === undefined && c.hsPct === undefined;

/** Every field the board shows has arrived (counts toward LOADING PLAYERS n / m). */
export const cardLoaded = (c: PlayerCard): boolean =>
  c.name !== undefined &&
  c.rank !== undefined &&
  c.actWinrate !== undefined &&
  c.actGames !== undefined &&
  c.kd !== undefined &&
  c.hsPct !== undefined;

function stat<T>(v: T | null | undefined, text: (v: T) => string, color: (v: T | null) => string): { text: string; color: string } | undefined {
  if (v === undefined) return undefined;
  return { text: v === null ? DASH : text(v), color: color(v) };
}

/** One table row (prototype `rows()`), for the player at `i` of `list` (the team in display order). */
export function rowView(c: PlayerCard, i: number, list: readonly PlayerCard[], ctx: RowCtx): RowView {
  const { meta } = ctx;
  const hlOn = ctx.settings.highlight;
  // The user's own card always carries their visible name (Rust reads it from the local chat session, R43/R49).
  const name = nameView(c.name, meta);
  const picking = c.agentId === null;
  const agent = picking ? "Picking…" : (agentName(meta, c.agentId) ?? (meta === null ? undefined : DASH));
  const sig = smurfSignal(c, ctx.lobby, ctx.settings.smurf, ctx.marks.notSmurf);
  const smurf = sig ? `Automated signal, not confirmed\n${sig.tips.join("\n")}\nOpen the profile for the full evidence.` : null;

  const inParty = showParty(c);
  const group = inParty ? c.party!.group : null;
  const same = (o: PlayerCard | undefined) => !!o && showParty(o) && o.party!.group === group;
  const party =
    group === null
      ? null
      : {
          color: partyColor(group),
          confirmed: c.party!.confirmed,
          top: same(list[i - 1]),
          bottom: same(list[i + 1]),
          tip: `${c.party!.confirmed ? "Queued together" : "Probably queued together"}\n${list
            .filter((o) => same(o))
            .map((o) => plainName(o.name, meta))
            .join(", ")}`,
        };

  const enc = showEncounters(c) ? c.encounters! : null;
  const saved = ctx.marks.saved.includes(c.puuid);
  const note = saved ? (ctx.marks.notes[c.puuid] ?? "") : "";

  const w = c.actWins, g = c.actGames;
  const wr =
    c.actWinrate === undefined
      ? undefined
      : c.actWinrate === null
        ? null
        : {
            text: pct0(c.actWinrate),
            color: hlWinPct(c.actWinrate * 100, hlOn),
            w: typeof w === "number" ? w : 0,
            l: typeof w === "number" && typeof g === "number" ? Math.max(0, g - w) : 0,
          };

  const rank = rankOf(c, meta);
  const lv = c.level === undefined ? undefined : c.level === null ? DASH : String(c.level);
  return {
    puuid: c.puuid,
    name,
    agentId: c.agentId,
    agent,
    picking,
    agentBorder: picking ? "1px dashed var(--line-strong)" : party ? `1px solid ${rgba(party.color, 0.7)}` : "1px solid var(--line-media)",
    isSelf: c.isSelf,
    level: lv,
    subColor: smurf ? "var(--warn-text)" : picking ? "var(--text-2)" : "var(--text-label)",
    smurf,
    met: enc
      ? {
          tip: `Played before\nWith you|${enc.with}\nAgainst you|${enc.against}`,
          aria: `Played before: ${enc.with} with you, ${enc.against} against you`,
        }
      : null,
    saved,
    note,
    party,
    rank,
    peak: peakOf(c, meta),
    wr,
    streak: streakOf(c.last5),
    games: g === undefined ? undefined : g === null ? DASH : count(g, "game"),
    kd: stat(c.kd, kd2, (v) => hlKd(v, hlOn)),
    hs: stat(c.hsPct, pct0, (v) => hlHs(v === null ? null : v * 100, hlOn)),
    selected: ctx.selected === c.puuid,
    aria: `${plainName(c.name, meta)}, ${picking ? "picking" : (agent ?? "agent")}, ${rank.state === "ranked" || rank.state === "unranked" ? rank.name : "rank unknown"}. Open profile`,
    statsLoading: statsLoading(c),
  };
}

// ── teams and the header ─────────────────────────────────────────────────────────────────────────────────────────

/** The user first, the rest in emitted order (never reordered when party marks arrive, R53). */
export const selfFirst = (ps: readonly PlayerCard[]): PlayerCard[] => [...ps.filter((p) => p.isSelf), ...ps.filter((p) => !p.isSelf)];

export interface TeamSpec {
  team: Team;
  label: string;
  color: string;
  cards: PlayerCard[];
  /** Placeholder skeleton rows (no card yet: the roster itself is still loading). */
  placeholders: number;
}

const TEAM_LABEL: Record<Team, string> = { ally: "YOUR TEAM", enemy: "ENEMY TEAM" };
const TEAM_COLOR: Record<Team, string> = { ally: "var(--ally)", enemy: "var(--enemy)" };

/** The tables on the board: Agent Select shows yours only; In Game both, enemy first with Settings' "Enemy team first". */
export function boardTeams(live: LiveState, enemyFirst: boolean): TeamSpec[] {
  const empty = live.players.length === 0;
  const spec = (team: Team): TeamSpec => {
    const cards = live.players.filter((p) => p.team === team);
    return { team, label: TEAM_LABEL[team], color: TEAM_COLOR[team], cards: team === "ally" ? selfFirst(cards) : cards, placeholders: empty ? 5 : 0 };
  };
  if (live.stage === "pregame") return [spec("ally")];
  return enemyFirst ? [spec("enemy"), spec("ally")] : [spec("ally"), spec("enemy")];
}

const mean = (xs: number[]): number | null => (xs.length ? xs.reduce((a, b) => a + b, 0) / xs.length : null);

/** A team's mean act win rate (0..1) over the players whose rate is known; null when none is. */
export function teamWinrate(cards: readonly PlayerCard[]): number | null {
  return mean(cards.map((c) => c.actWinrate).filter((v): v is number => typeof v === "number"));
}

/** A team's mean ranked tier, rounded half up; null when nobody is ranked. */
export function teamTier(cards: readonly PlayerCard[]): number | null {
  const m = mean(cards.map((c) => c.rank?.tier).filter(isRankedTier));
  return m === null ? null : Math.floor(m + 0.5);
}

/** One side of the header's matchup: the team's average act win rate. */
export interface Matchup {
  label: string;
  color: string;
  wr: string;
  /** Agent Select: the enemy side is unknown and drawn faded with "?". */
  hidden: boolean;
}

/** The header's YOUR TEAM vs ENEMY TEAM (prototype `matchup`): Agent Select fades the enemy side to "?". */
export function matchup(live: LiveState): Matchup[] {
  return (["ally", "enemy"] as const).map((team) => {
    const hidden = live.stage === "pregame" && team === "enemy";
    const wr = hidden ? null : teamWinrate(live.players.filter((p) => p.team === team));
    return { label: TEAM_LABEL[team], color: TEAM_COLOR[team], wr: hidden ? "?" : wr === null ? DASH : pct0(wr), hidden };
  });
}

/** LOADING PLAYERS n / m: shown until every expected player has every field. In Game expects 10, Agent Select 5. */
export function loadingProgress(live: LiveState): { loaded: number; total: number } | null {
  const cards = live.stage === "pregame" ? live.players.filter((p) => p.team === "ally") : live.players;
  const total = Math.max(live.stage === "pregame" ? 5 : 10, cards.length);
  const loaded = cards.filter(cardLoaded).length;
  return loaded >= total ? null : { loaded, total };
}

/** `0:42`, `1:24`, `12:05`, `1:15:13`: a whole-second countdown or stopwatch; hours appear from 60 minutes on. */
export function clock(ms: number): string {
  const s = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const ss = String(s % 60).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${ss}` : `${m}:${ss}`;
}

/** A countdown that rounds up, so `0:01` shows until the deadline, not `0:00` a second early. */
export function countdown(msLeft: number): string {
  return clock(Math.ceil(Math.max(0, msLeft) / 1000) * 1000);
}

// ── waiting (VALORANT closed / in menus) ─────────────────────────────────────────────────────────────────────────

export interface Check {
  label: string;
  value: string;
  ok: boolean;
}

/** The three status rows (prototype 2091–2094), from the connection status and the live stage. */
export function waitChecks(status: AppStatus | null, stage: Stage | null): Check[] {
  const menus = stage === "menus";
  const client = status?.riotClientVersion ? `RUNNING · ${status.riotClientVersion}` : "RUNNING";
  return [
    { label: "Riot Client", value: client, ok: true },
    { label: "VALORANT", value: menus ? "IN MENUS" : "NOT RUNNING", ok: menus },
    { label: "Match data", value: menus ? "READY" : "WAITING FOR GAME", ok: menus },
  ];
}

/** `45 MIN AGO`, `3 H AGO`, `2 D AGO` (the small caps labels). */
export function agoCaps(ms: number): string {
  const min = Math.max(1, Math.floor(Math.max(0, ms) / 60_000));
  if (min < 60) return `${min} MIN AGO`;
  const h = Math.floor(min / 60);
  if (h < 24) return `${h} H AGO`;
  return `${Math.floor(h / 24)} D AGO`;
}

/** `3 H 48 MIN`, `25 MIN`. */
/**
 * The TODAY card's session length: from the session's start to the start of its newest match, plus that match's
 * duration when it is known (`durationOf`; else nothing is added). Measured over the matches, not up to now, so the
 * figure stays put while the user idles in the menus instead of growing every minute (a session only ends after 6 h
 * without a competitive match). Neither `session.matches` nor `mystats.last5` carries a duration, so the Live page
 * passes the stored History row's duration when that list has been loaded.
 */
export function sessionSpanMs(
  session: Pick<SessionSummary, "startedMs" | "matches">,
  durationOf: (matchId: string) => number | null | undefined = () => null,
): number {
  const newest = session.matches.at(-1);
  if (!newest) return 0;
  const dur = durationOf(newest.matchId);
  return Math.max(0, newest.startMs + (typeof dur === "number" && dur > 0 ? dur : 0) - session.startedMs);
}

export function durationCaps(ms: number): string {
  const min = Math.max(0, Math.floor(ms / 60_000));
  const h = Math.floor(min / 60);
  return h > 0 ? `${h} H ${min % 60} MIN` : `${min} MIN`;
}

export const resultColor = (r: Result | "W" | "L" | "D" | null | undefined): string =>
  r === "win" || r === "W" ? "var(--win)" : r === "loss" || r === "L" ? "var(--loss)" : "var(--text-muted)";

/** A signed RR change's colour: gain green, loss red, 0 / unknown muted. */
export const rrColor = (n: number | null | undefined): string =>
  typeof n !== "number" || n === 0 ? "var(--text-muted)" : n > 0 ? "var(--win)" : "var(--loss)";

/** `W 13–9` for the LAST MATCH card. */
export function resultScore(m: Pick<MatchSummary, "result" | "roundsWon" | "roundsLost">): string {
  const s = typeof m.roundsWon === "number" && typeof m.roundsLost === "number" ? ` ${m.roundsWon}–${m.roundsLost}` : "";
  return `${LETTER[m.result]}${s}`;
}

/** `24/14/5`; a match without a stats row has none. */
export function kdaText(k: number | null, d: number | null, a: number | null, sep = "/"): string {
  return k === null || d === null || a === null ? DASH : [k, d, a].join(sep);
}

/** [C] Session chips show the newest 12 matches of a long session: more would be too narrow to read. */
export const SESSION_CHIPS = 12;

export interface ChipView {
  letter: "W" | "L" | "D";
  tip: string;
  bg: string;
  fg: string;
}

/** A session chip (prototype `chip()`): the result letter; the tooltip is `Map +RR`. */
export function sessionChip(m: SessionMatch, meta: Meta | null): ChipView {
  const map = mapName(meta, m.mapId) ?? "";
  const rr = m.rrEarned === null ? (m.result === "W" ? "Win" : m.result === "L" ? "Loss" : "Draw") : rrDelta(m.rrEarned);
  const tip = map ? `${map} ${rr}` : rr;
  if (m.result === "W") return { letter: "W", tip, bg: "var(--win-soft)", fg: "var(--win)" };
  if (m.result === "L") return { letter: "L", tip, bg: "var(--loss-soft)", fg: "var(--loss)" };
  return { letter: "D", tip, bg: "transparent", fg: "var(--text-muted)" };
}

/** The RANK card's `21 / 100 RR` (no ceiling from Immortal up: `312 RR`). */
export function rankRrText(tier: number | null, rr: number | null): string {
  if (rr === null) return `${DASH} RR`;
  return typeof tier === "number" && tier < 24 ? `${rr} / 100 RR` : `${rr} RR`;
}

// ── recap (prototype 372–447) ────────────────────────────────────────────────────────────────────────────────────

export interface RecapStat {
  l: string;
  v: string;
  c: string;
}

export interface RecapModel {
  result: Result | null;
  word: string;
  color: string;
  ours: number | null;
  theirs: number | null;
  stats: RecapStat[];
  mvp: boolean;
  /** The RR change this match made, when Riot recorded one (from My Stats' last 5). */
  rr: number | null;
}

const WORD: Record<Result, string> = { win: "VICTORY", loss: "DEFEAT", draw: "DRAW" };

/** The hero's numbers: the user's row of the perspective team (teams[0]), and the RR from My Stats when it has the match. */
export function recapModel(v: MatchDetailsView, mystats: MyStats | null | undefined): RecapModel {
  const ours = v.teams[0];
  const theirs = v.teams[1];
  const self = ours?.players.find((p) => p.isSelf) ?? ours?.players.find((p) => p.isFocus) ?? null;
  const rr = mystats?.last5.find((m) => m.matchId === v.matchId)?.rrEarned ?? null;
  const result = ours?.result ?? null;
  const stats: RecapStat[] = [];
  if (self) {
    stats.push(
      { l: "K / D / A", v: kdaText(self.kills, self.deaths, self.assists, " / "), c: "var(--text)" },
      { l: "ACS", v: self.acs === null ? DASH : String(Math.round(self.acs)), c: "var(--text)" },
      { l: "ADR", v: self.adr === null ? DASH : String(Math.round(self.adr)), c: "var(--text)" },
      { l: "HS%", v: pct0(self.hsPct), c: "var(--text)" },
    );
  }
  if (rr !== null) stats.push({ l: "RR", v: rrDelta(rr), c: rrColor(rr) });
  const top = ours?.players.reduce<ScoreRow | null>((best, p) => (p.acs !== null && (best === null || (best.acs ?? -1) < p.acs) ? p : best), null);
  return {
    result,
    word: result ? WORD[result] : "MATCH OVER",
    color: result === "win" ? "var(--win)" : result === "loss" ? "var(--loss)" : "var(--text)",
    ours: ours?.roundsWon ?? null,
    theirs: theirs?.roundsWon ?? null,
    stats,
    // Deathmatch has one block and no team: no TEAM MVP.
    mvp: v.teams.length >= 2 && !!self && !!top && top.puuid === self.puuid,
    rr,
  };
}

/** A recap team card's rows, by ACS (a row without stats last). */
export function byAcs(rows: readonly ScoreRow[]): ScoreRow[] {
  return [...rows].sort((a, b) => (b.acs ?? -1) - (a.acs ?? -1));
}

/** `61 RR to Ascendant 2 · about 3 wins` (the wins part needs a known average win). */
export function nextTierText(tier: number | null, rrToNext: number | null, avgWin: number | null, meta: Meta | null): string | null {
  if (typeof tier !== "number" || rrToNext === null || !isRankedTier(tier) || tier >= 27) return null;
  const head = `${rrToNext} RR to ${tierName(tier + 1, meta)}`;
  if (avgWin === null || avgWin <= 0) return head;
  const wins = Math.max(1, Math.ceil(rrToNext / avgWin));
  return `${head} · about ${wins} ${wins === 1 ? "win" : "wins"}`;
}

// ── glance ───────────────────────────────────────────────────────────────────────────────────────────────────────

/** The glance teams: enemy first, then yours (prototype `glanceTeams`); Agent Select has yours only. */
export function glanceTeams(live: LiveState): TeamSpec[] {
  return boardTeams(live, true);
}

/** The short average rank of a glance team header: `Asc 2`. */
export function avgRankShort(tier: number | null, meta: Meta | null): string {
  return tier === null ? DASH : tierShort(tier, meta);
}

/** The saved-player callout: the first saved player in this match and the note the user wrote. */
export function savedCallout(live: LiveState | null, marks: PlayerMarks, meta: Meta | null): { text: string; note: string } | null {
  if (!live || (live.stage !== "pregame" && live.stage !== "ingame")) return null;
  const order = new Map(marks.saved.map((p, i) => [p, i]));
  const hit = live.players
    .filter((p) => !p.isSelf && order.has(p.puuid))
    .sort((a, b) => order.get(a.puuid)! - order.get(b.puuid)!)[0];
  if (!hit) return null;
  const side = hit.team === "enemy" ? "the enemy team" : "your team";
  return { text: `${plainName(hit.name, meta)} is on ${side}`, note: marks.notes[hit.puuid] ?? "" };
}
