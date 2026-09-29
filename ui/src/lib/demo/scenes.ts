// DEV-only. The only scene registry (spec §8.2, R95): the feed, the DEV `api.ts` answers and the Playwright
// suite all read SCENES. Scenes are built from the design's roster in cdv8.ts; nothing here is shipped
// (it is reached only through a DEV-guarded dynamic import).
import type { Route } from "../router.svelte";
import type {
  LiveState,
  MatchDetailsView,
  Meta,
  PlayerCard,
  PlayerDetails,
  Stage,
  WeaponSkin,
} from "../types";
import {
  ACT,
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

export type SceneName =
  | "boot" | "closed" | "closed-empty" | "lobby1" | "lobby2" | "lobby2-loading" | "lobby2-unranked" | "lobby3" | "lobby4" | "lobby5"
  | "pregame" | "pregame-nomap" | "pregame-loading" | "ingame" | "ingame-longnames" | "ingame-parties4" | "ingame-partial" | "ingame-unranked"
  | "ingame-loading" | "ingame-nometa"
  | "details" | "details-more" | "details-self" | "details-hidden" | "details-empty" | "details-loading" | "details-longnames" | "details-partial"
  | "match" | "match-enc" | "match-hidden" | "match-dm" | "match-loading" | "match-longnames";

export type Scene = {
  /** null: no LiveState has arrived yet (`boot`). */
  live: LiveState | null;
  route: Route;
  /** Details by puuid; 'pending' never resolves. Roster players without an entry fall back to `resolveDetails`. */
  details?: Record<string, PlayerDetails | "pending">;
  /** Match views by id; 'pending' never resolves. Ids of the demo roster's matches fall back to `resolveMatch`. */
  matches?: Record<string, MatchDetailsView | "pending">;
  /** Defaults to demo/meta.json; 'pending' never resolves (`ingame-nometa`). */
  meta?: Meta | "pending";
};

// ── live states ─────────────────────────────────────────────────────────────────
const LIVE_ROUTE: Route = { name: "live" };

const state = (stage: Stage, mapId: string | null, players: PlayerCard[]): LiveState => ({ stage, mapId, players });
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

/** The long-name stress case: every visible name 16+5 W, every tier Ascendant 3, peaks 11 months old. */
function longnames(): PlayerCard[] {
  const p = roster();
  for (const c of p) {
    if (c.name?.kind === "visible" || c.name?.kind === "remembered") {
      c.name = { ...c.name, name: LONGEST_NAME, tag: LONGEST_TAG };
    }
    c.rank = { tier: 23, rr: c.rank?.rr ?? 0 };
    c.peak = peak(ACT["V26 A4"], 23, SINCE["11mo"]);
  }
  p[8]!.peak = peak(ACT["E10 A3"], 23, SINCE["11mo"]); // Rook: the longer act label
  p[7]!.encounters = { with: 70, against: 53 }; // Zephyr: a 3-digit ⟲ count, not self, no party
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
const matchRoute = (matchId: string, focusPuuid: string, from: "recent" | "encounter"): Route => ({
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

const REMEMBERED = { fromMapId: MAPS.Lotus, fromStartMs: Date.UTC(2026, 8, 5, 20, 10) };

function detailsLongnames(): Record<string, PlayerDetails> {
  const mara = longnames()[5]!;
  const d = detailsFor("mara", mara);
  const w = (tag: string) => ({ kind: "visible", name: LONGEST_NAME, tag }) as const;
  d.topTeammates = [
    { puuid: PUUID.kilo!, name: w(LONGEST_TAG), count: 14 }, // on the live roster
    { puuid: "d0000000-0000-4000-8000-000000000001", name: w("WWWWV"), count: 9 },
    { puuid: "d0000000-0000-4000-8000-000000000002", name: w("WWWWU"), count: 6 },
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
};

// ── the DEV `api.ts` answers ────────────────────────────────────────────────────
const sceneOf = (scene: string | null): Scene | undefined =>
  scene !== null && Object.hasOwn(SCENES, scene) ? SCENES[scene as SceneName] : undefined;

const never = <T>() => new Promise<T>(() => {});

/**
 * `player_details` for the DEV browser. A scene's own entry wins as authored ('pending' never resolves); any
 * roster player without one gets the roster details built with the scene's live card, and a Top 3 row for a
 * player who is hidden on that live screen follows the card (R93). Always a copy: the answer is the caller's
 * to mutate.
 */
export async function resolveDetails(scene: string | null, puuid: string): Promise<PlayerDetails> {
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
  const s = sceneOf(scene);
  const own = s?.matches?.[matchId];
  if (own === "pending") return never();
  return own ? structuredClone(own) : matchFor(matchId, s?.live?.players);
}
