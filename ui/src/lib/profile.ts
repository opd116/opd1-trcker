// The profile drawer's view model (design v2 `profile(id)`, TrackerApp.dc.html 1438–1574), built from real data only:
// `PlayerDetails` (the router's `openDetails` slot), the live roster, `profile_extras`, My Stats and the player marks.
// Pure: components/profile/* render what these return. A value the data does not carry is `—` (never invented), a
// still-loading one is `undefined` (a skeleton). Hidden names (CLAUDE.md): only what the DisplayName allows is shown.
import { CONTENT_TIERS } from "./details";
import { DASH, kd2, pct0, rrDelta, score } from "./format";
import { agentName, isRankedTier, mapName } from "./meta";
import { EDITION_COLORS, PARTY_COLORS, tierColor, tierName, tierShort as shortTier, type Edition } from "./theme";
import { nameTip } from "./tips";
import type {
  DisplayName,
  EncounterDetailRow,
  LiveState,
  MatchSummary,
  Meta,
  Peak,
  PlayerCard,
  PlayerDetails,
  ProfileExtras,
  Rank,
  Result,
  StatRow,
  WeaponSkin,
} from "./types";

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const WEEKDAYS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MIN = 60_000;
const DAY = 86_400_000;

// ── names ────────────────────────────────────────────────────────────────────────

/** How a name is drawn: as given, in the remembered style (eye + dotted underline), or in the hidden style (italic). */
export type NameStyle = "visible" | "remembered" | "hidden";

export interface NameView {
  /** The name part (no tag), `Hidden Player`, the locked agent's name, or `—`. */
  text: string;
  /** `#TAG`, or "" when the name is hidden or missing. */
  tag: string;
  style: NameStyle;
  /** The name tooltip (nameTip): the remembered source, `Hidden name`, or the full Riot ID. */
  tip: string;
  /** How sentences refer to the player: the name, or `this player` when it is hidden. */
  ref: string;
}

/**
 * A DisplayName as the drawer draws it. undefined while it is loading (a skeleton). A hidden agent whose name meta
 * does not know yet reads `Hidden Player` (never a placeholder agent, and never the raw id).
 */
export function nameView(n: DisplayName | null | undefined, meta: Meta | null): NameView | undefined {
  if (n === undefined) return undefined;
  if (n === null) return { text: DASH, tag: "", style: "visible", tip: "", ref: "this player" };
  switch (n.kind) {
    case "visible":
      return { text: n.name, tag: `#${n.tag}`, style: "visible", tip: `${n.name}#${n.tag}`, ref: n.name };
    case "remembered":
      return { text: n.name, tag: `#${n.tag}`, style: "remembered", tip: nameTip(n, meta), ref: n.name };
    case "hiddenPlayer":
      return { text: "Hidden Player", tag: "", style: "hidden", tip: "Hidden name", ref: "this player" };
    case "hiddenAgent":
      return { text: agentName(meta, n.agentId) || "Hidden Player", tag: "", style: "hidden", tip: "Hidden name", ref: "this player" };
  }
}

/** A sentence that starts with the player's reference: `This player`, capitalised. */
const cap = (s: string) => (s ? s[0]!.toUpperCase() + s.slice(1) : s);

// ── dates (prototype `stamp`) ────────────────────────────────────────────────────

const hm = (d: Date) => `${d.getHours() % 12 || 12}:${String(d.getMinutes()).padStart(2, "0")} ${d.getHours() < 12 ? "AM" : "PM"}`;

const dayStart = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();

/** `12 min ago`, `3 h ago`, `Yesterday`, `5 days ago`, then `Sep 13` (the prototype's relative stamp). */
export function relStamp(ms: number, nowMs: number): string {
  const mins = (nowMs - ms) / MIN;
  const dd = Math.round((dayStart(new Date(nowMs)) - dayStart(new Date(ms))) / DAY);
  if (mins < 60) return `${Math.max(1, Math.round(mins))} min ago`;
  if (dd <= 0) return `${Math.floor(mins / 60)} h ago`;
  if (dd === 1) return "Yesterday";
  if (dd < 30) return `${dd} days ago`;
  const d = new Date(ms);
  return `${MONTHS[d.getMonth()]} ${d.getDate()}`;
}

/** `Thu, 22 Oct 2026 · 10:40 PM` (the prototype's exact stamp, the tooltip under every relative date). */
export function exactStamp(ms: number): string {
  const d = new Date(ms);
  return `${WEEKDAYS[d.getDay()]}, ${d.getDate()} ${MONTHS[d.getMonth()]} ${d.getFullYear()} · ${hm(d)}`;
}

/** `5 days ago`, `1 month ago`, `3 months ago`, `1 year ago` (the peak's "since that act ended", prototype `peakAgo`). */
export function agoLong(ms: number): string {
  const days = Math.max(0, Math.floor(ms / DAY));
  const n = (v: number, w: string) => `${v} ${w}${v === 1 ? "" : "s"} ago`;
  if (days < 1) return "today";
  if (days < 30) return n(days, "day");
  if (days < 365) return n(Math.floor(days / 30), "month");
  return n(Math.floor(days / 365), "year");
}

// ── rank card ────────────────────────────────────────────────────────────────────

/** First tier with no fixed 100-RR step (Immortal 1): from it on the RR only accumulates. */
const UNCAPPED = 24;

export interface RankCell {
  /** Tier for the icon; null draws no icon. */
  icon: number | null;
  name: string;
  color: string;
  sub: string;
}

/** CURRENT RANK: `21 RR · 79 to Ascendant 2` below Immortal, `312 RR` from Immortal up, `— RR` when the RR is unknown. */
export function rankCell(rank: Rank | null | undefined, meta: Meta | null): RankCell | undefined {
  if (rank === undefined) return undefined;
  if (rank === null) return { icon: null, name: DASH, color: "var(--text-mid)", sub: "" };
  if (!isRankedTier(rank.tier)) return { icon: 0, name: "Unranked", color: tierColor(0), sub: "No ranked games" };
  const t = rank.tier;
  const rr = rank.rr === null ? DASH : String(rank.rr);
  const sub =
    t >= UNCAPPED || rank.rr === null ? `${rr} RR` : `${rr} RR · ${Math.max(0, 100 - rank.rr)} to ${tierName(t + 1, meta)}`;
  return { icon: t, name: tierName(t, meta), color: tierColor(t), sub };
}

/** PEAK: the tier, then `Reached this act` or `V26 A4 · 1 month ago` (time since that act ended). */
export function peakCell(peak: Peak | null | undefined, meta: Meta | null): RankCell | undefined {
  if (peak === undefined) return undefined;
  if (peak === null) return { icon: null, name: DASH, color: "var(--text-mid)", sub: "" };
  const sub = peak.sinceActEndMs === null ? "Reached this act" : `${peak.actLabel} · ${agoLong(peak.sinceActEndMs)}`;
  const ranked = isRankedTier(peak.tier);
  return { icon: ranked ? peak.tier : 0, name: tierName(peak.tier, meta), color: tierColor(peak.tier), sub };
}

/** LAST ACT: its final tier (`Final rank`), `Unranked` (`No ranked games`), or `—` when unknown. */
export function prevCell(tier: number | null | undefined, meta: Meta | null): RankCell | undefined {
  if (tier === undefined) return undefined;
  if (tier === null) return { icon: null, name: DASH, color: "var(--text-mid)", sub: "" };
  if (!isRankedTier(tier)) return { icon: 0, name: "Unranked", color: "var(--text-mid)", sub: "No ranked games" };
  return { icon: tier, name: tierName(tier, meta), color: "var(--text-mid)", sub: "Final rank" };
}

// ── stats row ────────────────────────────────────────────────────────────────────

export interface StatCell {
  label: string;
  /** undefined while loading (a skeleton). */
  value: string | undefined;
  color: string;
  tip: string;
}

/** A stat value's colour: standout white at `hi` and above (prototype), the stat colour otherwise; flat with highlight off. */
const tone = (v: number | null | undefined, hi: number, on: boolean): string =>
  !on ? "var(--text-soft)" : typeof v === "number" && v >= hi ? "var(--text-hi)" : "var(--text-mid)";

/** Mean ACS over the recent rows that have one, and how many that is; null when none has. */
export function avgAcs(recent: readonly MatchSummary[]): { acs: number; n: number } | null {
  const xs = recent.map((m) => m.acs).filter((a): a is number => typeof a === "number");
  if (xs.length === 0) return null;
  return { acs: Math.round(xs.reduce((a, b) => a + b, 0) / xs.length), n: xs.length };
}

/**
 * The stats row: K/D, Headshot %, Win rate, Record, Avg ACS (omitted when no recent row has an ACS) and, for the user,
 * Net RR. Each tooltip names its scope. `details` null = still loading; `netRr` undefined = My Stats not answered.
 */
export function statCells(
  card: PlayerCard,
  details: PlayerDetails | null,
  opts: { highlight: boolean; netRr?: number | null; netGames?: number | null },
): StatCell[] {
  const on = opts.highlight;
  const n = details?.heroMatches ?? 0;
  const heroScope = `Last ${n} competitive · this act`;
  const wins = card.actWins;
  const games = card.actGames;
  const losses = typeof wins === "number" && typeof games === "number" ? games - wins : null;
  const out: StatCell[] = [
    {
      label: "K/D",
      value: details ? kd2(details.heroKd) : undefined,
      color: tone(details?.heroKd, 1.3, on),
      tip: details && details.heroKd !== null ? `Kills per death\n${heroScope}` : "",
    },
    {
      label: "Headshot %",
      value: details ? pct0(details.heroHs) : undefined,
      color: tone(details?.heroHs == null ? null : details.heroHs * 100, 28, on),
      tip: details && details.heroHs !== null ? `Share of hits that landed on the head\n${heroScope}` : "",
    },
    {
      label: "Win rate",
      value: card.actWinrate === undefined ? undefined : pct0(card.actWinrate),
      color: tone(card.actWinrate == null ? null : card.actWinrate * 100, 60, on),
      tip:
        typeof wins === "number" && typeof games === "number"
          ? `${wins} ${wins === 1 ? "win" : "wins"} in ${games} ${games === 1 ? "game" : "games"}\nThis act · competitive`
          : "",
    },
    {
      label: "Record",
      value: wins === undefined || games === undefined ? undefined : score(wins, losses),
      color: on ? "var(--text-mid)" : "var(--text-soft)",
      tip: losses === null ? "" : "Wins – losses\nThis act · competitive",
    },
  ];
  if (!details) {
    out.push({ label: "Avg ACS", value: undefined, color: "var(--text-mid)", tip: "" });
  } else {
    const a = avgAcs(details.recent);
    if (a) {
      out.push({
        label: "Avg ACS",
        value: String(a.acs),
        color: on ? "var(--text-mid)" : "var(--text-soft)",
        tip: `Average combat score per match\nLast ${a.n} competitive`,
      });
    }
  }
  if (card.isSelf) {
    const net = opts.netRr;
    const g = opts.netGames;
    out.push({
      label: "Net RR",
      value: net === undefined ? undefined : rrDelta(net),
      color: typeof net !== "number" || net === 0 ? "var(--text-mid)" : net > 0 ? "var(--win)" : "var(--loss)",
      tip:
        typeof net === "number"
          ? `RR won minus RR lost this act${typeof g === "number" && g > 0 ? `\nOver ${g} stored ${g === 1 ? "match" : "matches"}` : ""}`
          : "",
    });
  }
  return out;
}

// ── party chip ───────────────────────────────────────────────────────────────────

export interface PartyChip {
  color: string;
  /** The other live members: their locked agent (null = not locked: an empty tile) and how the tooltip names them. */
  mates: { puuid: string; agentId: string | null; alt: string }[];
  tip: string;
}

/**
 * The header's party chip: the player's detected party in the live match (never the user's own party, which carries no
 * mark, CLAUDE.md). Names follow the hidden-name rules. null when there is no party or no other member on screen.
 */
export function partyChip(card: PlayerCard, live: LiveState | null, meta: Meta | null): PartyChip | null {
  const p = card.party;
  if (!p || card.isSelf || card.isSelfParty || !live) return null;
  const mates = live.players.filter(
    (c) => c.puuid !== card.puuid && c.team === card.team && c.party?.group === p.group && !c.isSelf && !c.isSelfParty,
  );
  if (mates.length === 0) return null;
  const nm = (n: DisplayName | null | undefined) => nameView(n, meta)?.text ?? DASH;
  const names = [nm(card.name), ...mates.map((m) => nm(m.name))].join(", ");
  return {
    color: PARTY_COLORS[p.group % PARTY_COLORS.length]!,
    mates: mates.map((m) => ({ puuid: m.puuid, agentId: m.agentId, alt: agentName(meta, m.agentId) || "" })),
    tip: `Queued together in this match: ${names}\n${p.confirmed ? "Party · confirmed" : "Party · likely"}`,
  };
}

// ── results ──────────────────────────────────────────────────────────────────────

export const RESULT_KEY: Record<Result, "W" | "L" | "D"> = { win: "W", loss: "L", draw: "D" };

/** A W / L / D square's colours (prototype `rs`). */
export function resultStyle(r: Result): { bg: string; fg: string } {
  if (r === "win") return { bg: "rgba(95,207,143,.16)", fg: "var(--win)" };
  if (r === "loss") return { bg: "rgba(239,106,91,.16)", fg: "var(--loss)" };
  return { bg: "var(--button-hover)", fg: "var(--text-mid)" };
}

const scoreColor = (r: Result) => (r === "win" ? "var(--win-text)" : r === "loss" ? "var(--loss-text)" : "var(--text-mid)");
const flip = (r: Result): Result => (r === "win" ? "loss" : r === "loss" ? "win" : "draw");

// ── encounters ───────────────────────────────────────────────────────────────────

export interface EncounterView {
  matchId: string;
  mapId: string;
  /** undefined while meta is pending. */
  map: string | null | undefined;
  res: "W" | "L" | "D";
  resStyle: { bg: string; fg: string };
  with: boolean;
  date: string;
  dateTip: string;
  self: { agentId: string | null; tier: number | null; score: string; scoreColor: string; tip: string };
  them: { agentId: string | null; tier: number | null; score: string; scoreColor: string; tip: string };
  aria: string;
}

const NUM = (n: number | null) => (n === null ? DASH : String(n));

/** One encounter row, from the user's side (the result, `Same team` / `Against you`, both players' agent + rank). */
export function encounterView(e: EncounterDetailRow, them: NameView | undefined, meta: Meta | null, nowMs: number): EncounterView {
  const map = mapName(meta, e.mapId);
  const theirResult = e.with ? e.result : flip(e.result);
  const theirScore = e.with ? e.roundsWon : e.roundsLost;
  const tierText = (t: number | null) => (t === null ? null : isRankedTier(t) ? tierName(t, meta) : "Unranked");
  const who = (head: string, agentId: string | null, t: number | null) =>
    [head, agentName(meta, agentId) || null, tierText(t)].filter((x): x is string => !!x).join(" · ");
  const verb = e.result === "win" ? "won" : e.result === "loss" ? "lost" : "drew";
  const mapText = typeof map === "string" ? map : "Unknown map";
  const dur = e.durationMs === null ? "" : ` · ${Math.round(e.durationMs / MIN)} min`;
  return {
    matchId: e.matchId,
    mapId: e.mapId,
    map,
    res: RESULT_KEY[e.result],
    resStyle: resultStyle(e.result),
    with: e.with,
    date: relStamp(e.startMs, nowMs),
    dateTip: exactStamp(e.startMs) + dur,
    self: { agentId: e.selfAgentId, tier: e.selfTier, score: NUM(e.roundsWon), scoreColor: scoreColor(e.result), tip: who("You", e.selfAgentId, e.selfTier) },
    them: {
      agentId: e.themAgentId,
      tier: e.themTier,
      score: NUM(theirScore),
      scoreColor: scoreColor(theirResult),
      tip: who(cap(them?.ref ?? "this player"), e.themAgentId, e.themTier),
    },
    aria: `${mapText}, ${verb} ${NUM(e.roundsWon)} to ${NUM(e.roundsLost)}. Open scoreboard`,
  };
}

// ── maps ─────────────────────────────────────────────────────────────────────────

export interface MapRowView {
  id: string;
  map: string | null | undefined;
  /** Win rate 0–100 (rounded). */
  wr: number;
  record: string;
  barColor: string;
  textColor: string;
  tip: string;
}

/** The five most played maps of the act (`profile_extras.maps`, already most played first). */
export function mapRows(rows: readonly StatRow[], meta: Meta | null, max = 5): MapRowView[] {
  return rows.slice(0, max).map((r) => {
    const wr = Math.round(r.winrate * 100);
    const name = mapName(meta, r.id);
    const label = typeof name === "string" ? name : "Map";
    const rr = r.netRr === null ? "" : ` · ${rrDelta(r.netRr)} RR`;
    return {
      id: r.id,
      map: name,
      wr,
      record: score(r.wins, r.losses),
      barColor: wr >= 55 ? "var(--win)" : wr <= 45 ? "var(--loss)" : "var(--text-faint)",
      textColor: wr >= 55 ? "var(--text)" : "var(--text-2)",
      tip: `${label}: ${r.wins} ${r.wins === 1 ? "win" : "wins"}, ${r.losses} ${r.losses === 1 ? "loss" : "losses"}${rr}\nThis act · competitive`,
    };
  });
}

// ── loadout ──────────────────────────────────────────────────────────────────────

export const WEAPONS = [
  { key: "vandal", label: "Vandal" },
  { key: "phantom", label: "Phantom" },
  { key: "operator", label: "Operator" },
  { key: "sheriff", label: "Sheriff" },
] as const;

export type LoadoutTile =
  | { state: "loading"; weapon: string }
  | { state: "none"; weapon: string }
  | { state: "skin"; weapon: string; chromaId: string; skin: string; edition: Edition | null; color: string; tint: string; tip: string };

/** One LOADOUT tile (R97 tri-state): absent = still resolving (skeleton), null = none or unknown (`—`), else the skin. */
export function loadoutTile(weapon: string, w: WeaponSkin | null | undefined): LoadoutTile {
  if (w === undefined) return { state: "loading", weapon };
  if (w === null) return { state: "none", weapon };
  const ed = w.tierId ? (CONTENT_TIERS[w.tierId]?.name as Edition | undefined) ?? null : null;
  const color = ed ? EDITION_COLORS[ed] : "#8a8f97";
  // Rust drops the weapon word from the skin's name ("Reaver", "Standard"); the design writes it ("Reaver Vandal").
  const skin = `${w.skinName} ${weapon}`;
  const full = w.chromaName ? `${skin} · ${w.chromaName}` : skin;
  return {
    state: "skin",
    weapon,
    chromaId: w.chromaId,
    skin,
    edition: ed,
    color,
    tint: ed ? `${color}26` : "rgba(255,255,255,.03)",
    tip: ed ? `${full}\n${ed} Edition` : full,
  };
}

// ── recent matches ───────────────────────────────────────────────────────────────

/** The design's table length (`Last 6 … newest first`). */
export const RECENT_ROWS = 6;

export interface LiveMateChip {
  puuid: string;
  agentId: string | null;
  name: string;
  hidden: boolean;
  tip: string;
}

/**
 * IN YOUR MATCH: the players of the current live match who were in this recent match (`profile_extras.liveMates`),
 * drawn from their live cards (so a hidden player stays hidden). The profile player and anyone not on the roster any
 * more are left out.
 */
export function liveMateChips(
  matchId: string,
  extras: ProfileExtras | null,
  live: LiveState | null,
  profilePuuid: string,
  meta: Meta | null,
): LiveMateChip[] {
  const row = extras?.liveMates.find((m) => m.matchId === matchId);
  if (!row || !live) return [];
  const out: LiveMateChip[] = [];
  for (const p of row.puuids) {
    if (p === profilePuuid) continue;
    const c = live.players.find((x) => x.puuid === p);
    if (!c) continue;
    const nv = nameView(c.isSelf && c.name == null ? null : c.name, meta);
    const text = c.isSelf ? "You" : (nv?.text ?? DASH);
    const agent = agentName(meta, c.agentId);
    const full = c.isSelf ? "You" : nv?.style === "hidden" ? "Hidden name" : `${nv?.text ?? DASH}${nv?.tag ?? ""}`;
    const head = agent ? `${full} · ${agent} in your match now` : `${full} · in your match now`;
    out.push({
      puuid: p,
      agentId: c.agentId,
      name: text,
      hidden: nv?.style === "hidden",
      tip: `${head}\n${c.team === "ally" ? "Your team" : "Enemy team"} now`,
    });
  }
  return out;
}

export interface RecentRowView {
  matchId: string;
  res: "W" | "L" | "D";
  resStyle: { bg: string; fg: string };
  score: string;
  mapId: string;
  map: string | null | undefined;
  when: string;
  whenTip: string;
  agentId: string | null;
  agent: string;
  kda: string;
  tier: number | null;
  rankShort: string;
  rankName: string;
  rankColor: string;
  rr: string;
  rrColor: string;
  aria: string;
}

export function recentRow(m: MatchSummary, meta: Meta | null, nowMs: number): RecentRowView {
  const map = mapName(meta, m.mapId);
  const k = m.kills;
  const kda = k === null || m.deaths === null || m.assists === null ? DASH : `${k} / ${m.deaths} / ${m.assists}`;
  const t = m.tier;
  const rr = m.rrEarned;
  const verb = m.result === "win" ? "win" : m.result === "loss" ? "loss" : "draw";
  return {
    matchId: m.matchId,
    res: RESULT_KEY[m.result],
    resStyle: resultStyle(m.result),
    score: score(m.roundsWon, m.roundsLost),
    mapId: m.mapId,
    map,
    when: relStamp(m.startMs, nowMs),
    whenTip: exactStamp(m.startMs),
    agentId: m.agentId,
    agent: agentName(meta, m.agentId) || "",
    kda,
    tier: t,
    rankShort: t === null ? DASH : isRankedTier(t) ? shortTier(t, meta) : "Unranked",
    rankName: t === null ? "" : isRankedTier(t) ? `Rank in this match: ${tierName(t, meta)}` : "Unranked in this match",
    rankColor: t === null ? "var(--text-mid)" : tierColor(t),
    rr: rrDelta(rr),
    rrColor: typeof rr !== "number" || rr === 0 ? "var(--text-2)" : rr > 0 ? "var(--win)" : "var(--loss)",
    aria: `${typeof map === "string" ? map : "Match"}, ${verb} ${score(m.roundsWon, m.roundsLost)}. Open scoreboard`,
  };
}

