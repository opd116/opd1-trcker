// DEV-only. The only scene registry (spec §8.2, R95): the feed, the DEV `api.ts` answers and the Playwright
// suite all read SCENES. Scenes are built from the design's roster in cdv8.ts; nothing here is shipped
// (it is reached only through a DEV-guarded dynamic import).
import type { MatchFrom } from "../details";
import type {
  EncounterDetailRow,
  LiveState,
  MatchDetailsView,
  Meta,
  MyStats,
  PlayerCard,
  PlayerDetails,
  ScoreRow,
  Stage,
  WeaponSkin,
} from "../types";
import {
  ACT,
  AG,
  deathmatch,
  detailsFor,
  emptyDetails,
  encounterMatchId,
  loadout,
  MAPS,
  matchFor,
  PUUID,
  recentMatchId,
  roster,
  SINCE,
} from "./cdv8";
import { LONGEST_NAME, LONGEST_TAG, peak } from "./fixtures";
import {
  longNameMeta,
  myStatsFull,
  myStatsImmortal,
  myStatsLongnames,
  myStatsRrNull,
  myStatsUnranked,
} from "./mystats";

export type SceneName =
  | "boot" | "closed" | "closed-empty" | "lobby1" | "lobby2" | "lobby2-loading" | "lobby2-unranked" | "lobby3" | "lobby4" | "lobby5"
  | "pregame" | "pregame-nomap" | "pregame-loading" | "ingame" | "ingame-longnames" | "ingame-parties4" | "ingame-partial" | "ingame-unranked"
  | "ingame-loading" | "ingame-nometa" | "ingame-remembered" | "ingame-rr-unknown"
  | "details" | "details-more" | "details-self" | "details-hidden" | "details-empty" | "details-loading" | "details-longnames" | "details-partial"
  | "details-nostats"
  | "match" | "match-enc" | "match-hidden" | "match-dm" | "match-loading" | "match-longnames" | "match-nostats"
  | "mystats" | "mystats-new" | "mystats-loading" | "mystats-unranked" | "mystats-immortal" | "mystats-rrnull"
  | "mystats-longnames" | "mystats-closed" | "closed-unsettled";

/**
 * Where a scene opens, in the v8 terms the scene table was written in. demo/feed.ts maps it onto the v2 router:
 * live → Live, mystats → Competitive, details → the profile drawer, match → the History scoreboard.
 */
export type DemoRoute =
  | { name: "live" }
  | { name: "mystats" }
  | { name: "details"; puuid: string; more: boolean }
  | { name: "match"; matchId: string; focusPuuid: string; from: MatchFrom; back: { puuid: string; more: boolean } | "mystats" };
type Route = DemoRoute;

export type Scene = {
  /** null: no LiveState has arrived yet (`boot`). */
  live: LiveState | null;
  route: Route;
  /** Details by puuid; 'pending' never resolves. Roster players without an entry fall back to `resolveDetails`. */
  details?: Record<string, PlayerDetails | "pending">;
  /** Match views by id; 'pending' never resolves. Ids of the demo roster's matches fall back to `resolveMatch`. */
  matches?: Record<string, MatchDetailsView | "pending">;
  /**
   * Defaults to demo/meta.json; 'pending' never resolves (`ingame-nometa`); a function edits the default (the feed
   * reads the JSON, so this module, which Playwright specs import under plain Node, never has to).
   */
  meta?: Meta | "pending" | ((base: Meta) => Meta);
  /**
   * The My Stats answer: data, `null` (Rust does not know the user or the act yet), or 'pending' (never answered: the
   * skeleton). Omitted: the full demo data, so the My Stats tab has something in every scene.
   */
  mystats?: MyStats | null | "pending";
  /**
   * Leave the route to the app: the first LiveState picks the start page itself (VALORANT closed → My Stats), so the
   * scene tests that rule instead of setting the route. `route` is then only what an `&route=` override replaces.
   */
  autoRoute?: true;
};

// ── live states ─────────────────────────────────────────────────────────────────
const LIVE_ROUTE: Route = { name: "live" };

/** `settled` is true in every scene unless it says otherwise: Rust's first verdict has long been made. */
const state = (stage: Stage, mapId: string | null, players: PlayerCard[], settled = true): LiveState => ({ stage, mapId, players, settled });
const ingame = (players: PlayerCard[] = roster()) => state("ingame", MAPS.Haven, players);

/** Fahad alone, as when VALORANT is closed: values from the DB, no agent. */
const closed = (): LiveState => state("closed", null, [{ ...roster()[0]!, agentId: null }]);

/** Self plus n−1 party members, no agents, no party marks (the client gives the party). Every card is `isSelfParty`, self included, as the backend sends it. */
function lobby(n: number, edit?: (p: PlayerCard[]) => void): LiveState {
  const players = roster()
    .slice(0, n)
    .map((c): PlayerCard => ({ ...c, agentId: null, party: null, encounters: null, isSelfParty: true }));
  edit?.(players);
  return state("menus", null, players);
}

function unranked(c: PlayerCard): void {
  c.rank = null;
  c.prevActTier = null;
  c.peak = null;
}

/** Agent Select: 2 locked, 1 hovering, 1 undecided, 1 hidden player who has not locked. */
function pregame(mapId: string | null): LiveState {
  const p = roster().slice(0, 5);
  p[2]!.agentId = null; // Vesper hovers: the wire looks exactly like "no pick" (R99)
  p[3]!.agentId = null; // Drake has not picked
  p[4]!.agentId = null; // Nova is hidden and has not locked
  p[4]!.name = { kind: "hiddenPlayer" };
  return state("pregame", mapId, p);
}

/** The long-name stress case: every visible name 16+5 W, every tier Ascendant 3 (the widest Prev row too, R125/R126), peaks 11 months old. */
function longnames(): PlayerCard[] {
  const p = roster();
  for (const c of p) {
    if (c.name?.kind === "visible" || c.name?.kind === "remembered") {
      c.name = { ...c.name, name: LONGEST_NAME, tag: LONGEST_TAG };
    }
    c.rank = { tier: 23, rr: c.rank?.rr ?? 0 };
    c.prevActTier = 23;
    c.peak = peak(ACT["V26 A4"], 23, SINCE["11mo"]);
  }
  p[8]!.peak = peak(ACT["E10 A3"], 23, SINCE["11mo"]); // Rook: the longer act label
  p[7]!.encounters = { with: 70, against: 53 }; // Zephyr: a 3-digit ⟲ count, not self, no party
  return p;
}

/**
 * R145: one remembered card of each source, in the design's roster otherwise. Rook is the design's own (a finished
 * match the user played, `ownMatch`); Nyx (Cypher's slot) was in someone else's stored match (`otherMatch`); Kilo's
 * name came from a name-service answer while he was visible (`seen`: no map, the date it was last confirmed).
 */
function rememberedSources(): PlayerCard[] {
  const p = roster();
  p[9]!.name = { kind: "remembered", name: "Nyx", tag: "0420", ...REMEMBERED, source: "otherMatch" };
  p[6]!.name = { kind: "remembered", name: "Kilo", tag: "2210", source: "seen", fromMapId: null, fromStartMs: Date.UTC(2026, 8, 20, 18, 5) };
  return p;
}

/**
 * R154: the MMR request failed for two cards, so their tier comes from the stored matches and the exact RR is unknown
 * (`rr: null`): Sable (left column) and Mara (right column). Everything else is the design's roster.
 */
function rrUnknown(): PlayerCard[] {
  const p = roster();
  for (const i of [1, 5]) p[i]!.rank = { tier: p[i]!.rank!.tier, rr: null };
  return p;
}

function parties4(): PlayerCard[] {
  const p = roster();
  const mark = (i: number, group: number, confirmed: boolean) => (p[i]!.party = { group, confirmed });
  mark(3, 0, true); // Drake, Nova: confirmed
  mark(4, 0, true);
  mark(1, 1, false); // Sable, Vesper: likely
  mark(2, 1, false);
  mark(5, 2, false); // Mara, Kilo: likely
  mark(6, 2, false);
  mark(7, 3, true); // Zephyr, Rook: confirmed
  mark(8, 3, true);
  return p;
}

/** Every loadable field is absent on some card (R23). */
function partial(): PlayerCard[] {
  const p = roster();
  delete p[1]!.name;
  delete p[2]!.level;
  delete p[3]!.rank;
  delete p[4]!.peak;
  delete p[5]!.prevActTier;
  for (const k of ["kd", "hsPct", "actWinrate", "actWins", "actGames"] as const) delete p[6]![k];
  delete p[7]!.last5;
  delete p[8]!.rank; // a card that is loading several things at once
  delete p[8]!.peak;
  delete p[8]!.last5;
  delete p[9]!.level;
  delete p[9]!.kd;
  return p;
}

function unrankedRoster(): PlayerCard[] {
  const p = roster();
  Object.assign(p[7]!, {
    level: null, rank: null, prevActTier: null, peak: null, kd: null, hsPct: null,
    actWinrate: null, actWins: null, actGames: null, last5: [],
  });
  p[6]!.last5 = p[6]!.last5!.slice(0, 2); // Kilo has played 2 matches: 3 empty tiles
  return p;
}

// ── details and matches ─────────────────────────────────────────────────────────
/** Details for all 10 roster players, keyed by puuid. Fresh objects on every call: no two scenes share one. */
function rosterDetails(): Record<string, PlayerDetails> {
  return Object.fromEntries(Object.entries(PUUID).map(([id, puuid]) => [puuid, detailsFor(id)]));
}

const details = (puuid: string, more = false): Route => ({ name: "details", puuid, more });
const matchRoute = (matchId: string, focusPuuid: string, from: MatchFrom): Route => ({
  name: "match",
  matchId,
  focusPuuid,
  from,
  back: { puuid: focusPuuid, more: false },
});

const MARA = PUUID.mara!;
const MARA_RECENT = recentMatchId("mara", 0);
const MARA_ENCOUNTER = encounterMatchId("mara", 0);
const MARA_DM = "mdemo:mara:dm";

const REMEMBERED = { source: "ownMatch", fromMapId: MAPS.Lotus, fromStartMs: Date.UTC(2026, 8, 5, 20, 10) } as const;

function detailsLongnames(): Record<string, PlayerDetails> {
  const mara = longnames()[5]!;
  const d = detailsFor("mara", mara);
  const w = (tag: string) => ({ kind: "visible", name: LONGEST_NAME, tag }) as const;
  // Spec §6.7's three name styles at their widest: a live visible teammate (the narrowest block), a remembered name
  // (eye + 16 W's + tag, so it wraps), and a hidden agent (Task 18 review, Minor 4).
  d.topTeammates = [
    { puuid: PUUID.kilo!, name: w(LONGEST_TAG), count: 14 }, // on the live roster
    { puuid: "d0000000-0000-4000-8000-000000000001", name: { kind: "remembered", name: LONGEST_NAME, tag: "WWWWV", ...REMEMBERED }, count: 9 },
    { puuid: "d0000000-0000-4000-8000-000000000002", name: { kind: "hiddenAgent", agentId: AG.Brimstone }, count: 6 },
  ];
  const lo = loadout();
  for (const k of ["vandal", "phantom", "operator", "sheriff"] as const) {
    lo[k] = { ...lo[k], skinName: "Protocol 781-A", chromaName: "Sunrise" };
  }
  d.loadout = lo;
  return { [MARA]: d };
}

function detailsPartial(): Record<string, PlayerDetails> {
  const mara = roster()[5]!;
  for (const k of ["name", "level", "rank", "peak", "kd", "hsPct", "actWinrate", "actWins", "actGames", "last5"] as const) {
    delete mara[k];
  }
  const d = detailsFor("mara", mara);
  const skin: WeaponSkin = loadout().vandal;
  // The sheriff key is left out on purpose (its name is still resolving); the operator is known to be none (R97).
  d.loadout = { vandal: skin, phantom: loadout().phantom, operator: null };
  return { [MARA]: d };
}

function matchHidden(): MatchDetailsView {
  const m = matchFor(MARA_RECENT);
  const drift = m.teams[1]!.players.find((p) => p.name.kind === "visible" && p.name.name === "Drift")!;
  drift.name = { kind: "remembered", name: "Drift", tag: "9981", ...REMEMBERED };
  return m;
}

function matchLongnames(): MatchDetailsView {
  const m = matchFor(MARA_RECENT);
  for (const p of m.teams.flatMap((t) => t.players)) {
    if (p.name.kind === "visible") p.name = { kind: "visible", name: LONGEST_NAME, tag: LONGEST_TAG };
  }
  // Two remembered rows besides the visible ones.
  m.teams[1]!.players
    .filter((p) => p.name.kind === "visible" && !p.isFocus)
    .slice(0, 2)
    .forEach((p) => (p.name = { kind: "remembered", name: LONGEST_NAME, tag: LONGEST_TAG, ...REMEMBERED }));
  return m;
}

const unrankedLive = state("ingame", MAPS.Haven, unrankedRoster());

// ── My Stats ────────────────────────────────────────────────────────────────────
const MYSTATS_ROUTE: Route = { name: "mystats" };

const LONGNAMES = myStatsLongnames();

/** The demo meta with the longest real map and agent names (`mystats-longnames`): the two most played of each are renamed. */
const longnamesMeta =
  (stats: MyStats) =>
  (m: Meta): Meta => {
    const long = longNameMeta(stats);
    return { ...m, maps: { ...m.maps, ...long.maps }, agents: { ...m.agents, ...long.agents } };
  };

// ── R133: every `—` cell of Details and Match ───────────────────────────────────
/** Zephyr in `ingame-unranked`: level, rank, peak, prev and every stat null, `last5: []`. */
const ZEPHYR = PUUID.zephyr!;
const NOSTATS_MATCH = recentMatchId("zephyr", 0);
const NOSTATS_ENCOUNTERS = [encounterMatchId("zephyr", 0), encounterMatchId("zephyr", 1)];

/** Zephyr's two encounter rows (an Against draw with nothing known, a With win); the matches they open follow them. */
function nostatsEncounters(): EncounterDetailRow[] {
  const enc = (i: number, e: Omit<EncounterDetailRow, "matchId">): EncounterDetailRow => ({ matchId: NOSTATS_ENCOUNTERS[i]!, ...e });
  return [
    enc(0, {
      mapId: "", startMs: Date.UTC(2026, 8, 12, 23, 18), with: false, result: "draw", roundsWon: null, roundsLost: null,
      durationMs: null, themAgentId: null, themTier: null, selfAgentId: null, selfTier: null,
    }),
    enc(1, {
      mapId: MAPS.Split, startMs: Date.UTC(2026, 7, 30, 22, 31), with: true, result: "win", roundsWon: 13, roundsLost: 10,
      durationMs: null, themAgentId: AG.Viper, themTier: 0, selfAgentId: AG.Jett, selfTier: null,
    }),
  ];
}

/**
 * Details where every value that can be unknown is (R102, R124, R133): the hero's cells, the recent stat lines (one row
 * also with no map, score, agent or rank), the encounter ranks, scores, agents and durations, an empty Top 3 and a
 * loadout of nothing. No counted act match, so the hero K/D and HS% are null too (R116).
 */
function detailsNostats(): Record<string, PlayerDetails> {
  const live = unrankedLive.players;
  const d = detailsFor("zephyr", live.find((c) => c.puuid === ZEPHYR), live);
  const none = { kills: null, deaths: null, assists: null, kd: null, acs: null, hsPct: null, rrEarned: null } as const;
  d.recent = d.recent.slice(0, 4).map((m, i) => ({
    ...m,
    ...none,
    ...(i === 1 ? { mapId: "", roundsWon: null, roundsLost: null, agentId: null, tier: null } : {}),
    ...(i === 2 ? { tier: 0 } : {}), // unranked in that match: the empty box, like null
  }));
  d.encounters = nostatsEncounters();
  d.topTeammates = [];
  d.loadout = { vandal: null, phantom: null, operator: null, sheriff: null };
  d.heroKd = null;
  d.heroHs = null;
  d.heroMatches = 0;
  return { [ZEPHYR]: d };
}

const byAcsDesc = (x: ScoreRow, y: ScoreRow) => (y.acs ?? -1) - (x.acs ?? -1);

const OPPOSITE = { win: "loss", loss: "win", draw: "draw" } as const;

/**
 * Zephyr's recent match with every scoreboard `—` (R124, R133): no duration, no rounds and no halves, and rows with no
 * stats row whose rank still shows (R81), one with no rank and no agent either, one that lost only its damage (ADR).
 * With an encounter row `enc`, it is that row's match, which the user played: the self perspective, team 0 the user's
 * (Zephyr's team for With, the other for Against), and the row's result, score, map, start and duration.
 */
function matchNostats(matchId: string, enc?: EncounterDetailRow): MatchDetailsView {
  const m = matchFor(NOSTATS_MATCH, unrankedLive.players);
  m.matchId = matchId;
  m.durationMs = null;
  for (const t of m.teams) {
    t.roundsWon = null;
    t.firstHalf = null;
  }
  const none = { kills: null, deaths: null, assists: null, acs: null, adr: null, hsPct: null } as const;
  const [a, b] = m.teams as [MatchDetailsView["teams"][0], MatchDetailsView["teams"][0]];
  // By position (this file names no roster player, so a leaked scene bundle is still caught by the dist grep).
  const [mate0, mate1, mate2] = a.players.filter((p) => !p.isFocus);
  Object.assign(a.players.find((p) => p.isFocus)!, none); // Zephyr: no stats row, rank kept
  Object.assign(mate1!, none, { tier: null, agentId: null });
  Object.assign(mate2!, { adr: null }); // only the damage is unknown
  Object.assign(b.players.find((p) => p.name.kind === "hiddenAgent")!, none);
  Object.assign(b.players.find((p) => p.name.kind === "visible")!, none, { tier: 0 });
  if (enc) {
    m.perspective = "self";
    const self = roster().find((c) => c.isSelf)!;
    // With: a teammate's row becomes the user's. Against: an opponent's that still has its stats.
    const host = enc.with ? mate0! : b.players.find((p) => p.kills !== null)!;
    Object.assign(host, { puuid: self.puuid, name: structuredClone(self.name!), isSelf: true });
    const [us, them] = enc.with ? [a, b] : [b, a];
    Object.assign(us, { result: enc.result, roundsWon: enc.roundsWon });
    Object.assign(them, { result: OPPOSITE[enc.result], roundsWon: enc.roundsLost });
    m.teams = [us, them];
    Object.assign(m, { mapId: enc.mapId, startMs: enc.startMs, durationMs: enc.durationMs });
  }
  for (const t of m.teams) t.players.sort(byAcsDesc);
  return m;
}

function nostatsMatches(): Record<string, MatchDetailsView> {
  return {
    [NOSTATS_MATCH]: matchNostats(NOSTATS_MATCH),
    ...Object.fromEntries(nostatsEncounters().map((e) => [e.matchId, matchNostats(e.matchId, e)])),
  };
}

export const SCENES: Record<SceneName, Scene> = {
  boot: { live: null, route: LIVE_ROUTE },

  closed: { live: closed(), route: LIVE_ROUTE },
  "closed-empty": { live: state("closed", null, []), route: LIVE_ROUTE },

  lobby1: { live: lobby(1), route: LIVE_ROUTE },
  lobby2: { live: lobby(2, (p) => unranked(p[1]!)), route: LIVE_ROUTE },
  "lobby2-loading": {
    live: lobby(2, (p) => {
      delete p[0]!.rank; // still loading
      p[1]!.rank = null; // unranked
    }),
    route: LIVE_ROUTE,
  },
  "lobby2-unranked": {
    live: lobby(2, (p) => {
      p[0]!.rank = null;
      p[1]!.rank = null;
    }),
    route: LIVE_ROUTE,
  },
  lobby3: { live: lobby(3, (p) => unranked(p[2]!)), route: LIVE_ROUTE },
  lobby4: { live: lobby(4, (p) => delete p[3]!.rank), route: LIVE_ROUTE },
  lobby5: { live: lobby(5), route: LIVE_ROUTE },

  pregame: { live: pregame(MAPS.Haven), route: LIVE_ROUTE },
  "pregame-nomap": { live: pregame(null), route: LIVE_ROUTE },
  "pregame-loading": { live: state("pregame", null, []), route: LIVE_ROUTE },

  ingame: { live: ingame(), route: LIVE_ROUTE },
  "ingame-longnames": { live: ingame(longnames()), route: LIVE_ROUTE },
  "ingame-parties4": { live: ingame(parties4()), route: LIVE_ROUTE },
  "ingame-partial": { live: ingame(partial()), route: LIVE_ROUTE },
  "ingame-unranked": { live: unrankedLive, route: LIVE_ROUTE },
  "ingame-loading": { live: state("ingame", null, []), route: LIVE_ROUTE },
  "ingame-nometa": { live: ingame(), route: LIVE_ROUTE, meta: "pending" },
  "ingame-remembered": { live: ingame(rememberedSources()), route: LIVE_ROUTE },
  "ingame-rr-unknown": { live: ingame(rrUnknown()), route: LIVE_ROUTE },

  details: { live: ingame(), route: details(MARA), details: rosterDetails() },
  "details-more": { live: ingame(), route: details(MARA, true), details: rosterDetails() },
  "details-self": { live: ingame(), route: details(PUUID.fahad!), details: rosterDetails() },
  "details-hidden": { live: ingame(), route: details(PUUID.cypher!), details: rosterDetails() },
  "details-empty": {
    live: unrankedLive,
    route: details(PUUID.zephyr!),
    details: { [PUUID.zephyr!]: emptyDetails(unrankedLive.players[7]!) },
  },
  "details-loading": { live: ingame(), route: details(MARA), details: { [MARA]: "pending" } },
  "details-longnames": { live: ingame(longnames()), route: details(MARA), details: detailsLongnames() },
  "details-partial": { live: ingame(), route: details(MARA), details: detailsPartial() },
  "details-nostats": { live: unrankedLive, route: details(ZEPHYR), details: detailsNostats(), matches: nostatsMatches() },

  match: {
    live: ingame(),
    route: matchRoute(MARA_RECENT, MARA, "recent"),
    matches: { [MARA_RECENT]: matchFor(MARA_RECENT) },
  },
  "match-enc": {
    live: ingame(),
    route: matchRoute(MARA_ENCOUNTER, MARA, "encounter"),
    matches: { [MARA_ENCOUNTER]: matchFor(MARA_ENCOUNTER) },
  },
  "match-hidden": {
    live: ingame(),
    route: matchRoute(MARA_RECENT, MARA, "recent"),
    matches: { [MARA_RECENT]: matchHidden() },
  },
  "match-dm": {
    live: ingame(),
    route: matchRoute(MARA_DM, MARA, "recent"),
    matches: { [MARA_DM]: deathmatch(MARA_DM) },
  },
  "match-loading": {
    live: ingame(),
    route: matchRoute(MARA_RECENT, MARA, "recent"),
    matches: { [MARA_RECENT]: "pending" },
  },
  "match-longnames": {
    live: ingame(longnames()),
    route: matchRoute(MARA_RECENT, MARA, "recent"),
    matches: { [MARA_RECENT]: matchLongnames() },
  },
  "match-nostats": { live: unrankedLive, route: matchRoute(NOSTATS_MATCH, ZEPHYR, "recent"), matches: nostatsMatches() },

  mystats: { live: ingame(), route: MYSTATS_ROUTE, mystats: myStatsFull() },
  "mystats-new": { live: ingame(), route: MYSTATS_ROUTE, mystats: null },
  "mystats-loading": { live: ingame(), route: MYSTATS_ROUTE, mystats: "pending" },
  "mystats-unranked": { live: ingame(), route: MYSTATS_ROUTE, mystats: myStatsUnranked() },
  "mystats-immortal": { live: ingame(), route: MYSTATS_ROUTE, mystats: myStatsImmortal() },
  "mystats-rrnull": { live: ingame(), route: MYSTATS_ROUTE, mystats: myStatsRrNull() },
  "mystats-longnames": { live: ingame(), route: MYSTATS_ROUTE, mystats: LONGNAMES, meta: longnamesMeta(LONGNAMES) },
  // VALORANT closed at startup: the app's own start-page rule opens My Stats (no route is set here).
  "mystats-closed": { live: closed(), route: LIVE_ROUTE, autoRoute: true, mystats: myStatsFull() },
  // Rust has not made its first verdict yet (its `closed` is the starting value): the start page waits on Live.
  "closed-unsettled": { live: state("closed", null, [], false), route: LIVE_ROUTE, autoRoute: true, mystats: myStatsFull() },
};

// ── the DEV `api.ts` answers ────────────────────────────────────────────────────
const sceneOf = (scene: string | null): Scene | undefined =>
  scene !== null && Object.hasOwn(SCENES, scene) ? SCENES[scene as SceneName] : undefined;

const never = <T>() => new Promise<T>(() => {});

/** What the scene's My Stats store starts as: its data, null, or undefined (pending: nothing answered yet). */
export function sceneMyStats(s: Scene): MyStats | null | undefined {
  if (s.mystats === "pending") return undefined;
  return s.mystats === undefined ? myStatsFull() : s.mystats;
}

/** `my_stats` for the DEV browser: the scene's own answer (a copy); 'pending' never resolves. */
export async function resolveMyStats(scene: string | null): Promise<MyStats | null> {
  const s = sceneOf(scene);
  if (s?.mystats === "pending") return never();
  const v = s ? sceneMyStats(s) : myStatsFull();
  return v === undefined ? never() : structuredClone(v);
}

/** DEV: `close_details` has no Rust side in the browser; the count is `data-details-closed` on <html> (Playwright reads it). */
export function noteDetailsClosed(): void {
  const d = document.documentElement.dataset;
  d.detailsClosed = String(Number(d.detailsClosed ?? 0) + 1);
}

/** DEV: `close_match` has no Rust side in the browser; the count is `data-match-closed` on <html> (Playwright reads it). */
export function noteMatchClosed(): void {
  const d = document.documentElement.dataset;
  d.matchClosed = String(Number(d.matchClosed ?? 0) + 1);
}

/** How many more calls of each command the URL says must fail (`?fail=details:2,match:1`), counted down as they do. */
const failsLeft = new Map<string, number>();

/**
 * DEV retry tests: `?fail=details:N` makes the first N `player_details` calls reject, `?fail=match:N` the first N
 * `match_details` calls (N = 1 succeeds on the 1s retry, N = 2 settles the page on its empty state).
 */
function failNow(cmd: "details" | "match"): boolean {
  if (!failsLeft.has(cmd)) {
    const param = typeof location === "undefined" ? "" : (new URLSearchParams(location.search).get("fail") ?? "");
    const m = new RegExp(`(?:^|,)${cmd}:(\\d+)`).exec(param);
    failsLeft.set(cmd, m ? Number(m[1]) : 0);
  }
  const n = failsLeft.get(cmd)!;
  if (n <= 0) return false;
  failsLeft.set(cmd, n - 1);
  return true;
}

/**
 * `player_details` for the DEV browser. A scene's own entry wins as authored ('pending' never resolves); any
 * roster player without one gets the roster details built with the scene's live card, and a Top 3 row for a
 * player who is hidden on that live screen follows the card (R93). Always a copy: the answer is the caller's
 * to mutate.
 */
export async function resolveDetails(scene: string | null, puuid: string): Promise<PlayerDetails> {
  if (failNow("details")) throw new Error("demo: player_details failed");
  const s = sceneOf(scene);
  const own = s?.details?.[puuid];
  if (own === "pending") return never();
  if (own) return structuredClone(own);
  const id = Object.keys(PUUID).find((k) => PUUID[k] === puuid);
  if (!id) throw new Error(`demo: no details for ${puuid}`);
  const live = s?.live?.players;
  return detailsFor(id, live?.find((c) => c.puuid === puuid), live);
}

/**
 * `match_details` for the DEV browser: the scene's own view as authored (a copy), or one built from the demo
 * match id whose rows follow the scene's live hidden names, never the raw roster names (R93).
 */
export async function resolveMatch(scene: string | null, matchId: string, _focusPuuid: string): Promise<MatchDetailsView> {
  if (failNow("match")) throw new Error("demo: match_details failed");
  const s = sceneOf(scene);
  const own = s?.matches?.[matchId];
  if (own === "pending") return never();
  return own ? structuredClone(own) : matchFor(matchId, s?.live?.players);
}
