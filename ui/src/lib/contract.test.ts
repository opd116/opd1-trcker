/// <reference types="node" />
// The wire contract, UI side (Task 16b, R96/R97). The Rust tests pin the JSON the backend sends in
// crates/core/tests/fixtures/contract/*.json; here the same files are walked with the descriptors in
// contract.shape.ts (written from them), so a renamed, dropped or retyped key fails on the UI side too.
// The demo fixtures are held to the same descriptors and invariants: a DEV scene can never drift from
// what the real backend sends. contract.test-d.ts ties the descriptors to types.ts.
import { readdirSync, readFileSync } from "node:fs";
import { render } from "svelte/server";
import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";
import { displayNameShapes, enums, shapes, type Desc, type Kind } from "./contract.shape";
import { detailsFor, encounterMatchId, matchFor, PUUID, recentMatchId, roster } from "./demo/cdv8";
import demoMeta from "./demo/meta.json";
import { SCENES } from "./demo/scenes";
import type { Meta, PlayerCard as Card, PlayerDetails } from "./types";

// ── goldens ──────────────────────────────────────────────────────────────────────
const GOLDEN_DIR = new URL("../../../crates/core/tests/fixtures/contract/", import.meta.url);
const golden = (name: string): unknown => JSON.parse(readFileSync(new URL(`${name}.json`, GOLDEN_DIR), "utf8"));

const LIVE = golden("live_state") as { players: Record<string, unknown>[] } & Record<string, unknown>;
const DETAILS = golden("player_details") as Record<string, unknown>;
const MATCHES = golden("match_details_view") as Record<string, unknown>[];

// ── the walker ───────────────────────────────────────────────────────────────────
type Shape = Record<string, Desc>;
type Obj = Record<string, unknown>;

// A plain `Desc` map: the assignment fails to compile if a descriptor is not one.
const registry: Record<string, Shape> = shapes;
const variants: Record<string, Shape> = displayNameShapes;

const kindOf = (v: unknown): Kind | "undefined" =>
  v === null ? "null" : Array.isArray(v) ? "array" : typeof v === "object" ? "object" : (typeof v as Kind | "undefined");

/**
 * Every problem `value` has as `shapeName`, one line each: a key the descriptor lacks, a required key that is
 * missing, a value of a kind the descriptor does not list, a string outside its closed set, a fraction outside
 * 0..1, a nested object or array element that fails its own descriptor. `DisplayName` picks its descriptor by `kind`.
 */
function problems(value: unknown, shapeName: string, path: string): string[] {
  if (kindOf(value) !== "object") return [`${path}: ${shapeName} is not an object (${kindOf(value)})`];
  const obj = value as Obj;
  const shape = shapeName === "DisplayName" ? variants[String(obj.kind)] : registry[shapeName];
  if (!shape) return [`${path}: no ${shapeName} descriptor for kind ${JSON.stringify(obj.kind)}`];
  const out: string[] = [];
  for (const key of Object.keys(obj)) {
    if (!(key in shape)) out.push(`${path}.${key}: not a key of ${shapeName}`);
  }
  for (const [key, desc] of Object.entries(shape)) {
    if (!(key in obj)) {
      if (!desc.optional) out.push(`${path}.${key}: required key of ${shapeName} is missing`);
      continue;
    }
    const v = obj[key];
    const k = kindOf(v);
    const here = `${path}.${key}`;
    if (!(desc.kinds as string[]).includes(k)) {
      out.push(`${here}: ${k} is not one of ${desc.kinds.join("|")}`);
      continue;
    }
    if (desc.enum && typeof v === "string" && !(enums[desc.enum] as readonly string[]).includes(v)) {
      out.push(`${here}: ${JSON.stringify(v)} is not one of ${enums[desc.enum].join("|")}`);
    }
    if (desc.fraction && typeof v === "number" && !(v >= 0 && v <= 1)) out.push(`${here}: ${v} is not a 0..1 fraction`);
    if (desc.shape && k === "object") out.push(...problems(v, desc.shape, here));
    if (desc.shape && k === "array") (v as unknown[]).forEach((e, i) => out.push(...problems(e, desc.shape!, `${here}[${i}]`)));
  }
  return out;
}

/** Every object anywhere inside `value` (itself included) with the key it hangs under. */
function objects(value: unknown, key = "$"): Array<[string, Obj]> {
  if (Array.isArray(value)) return value.flatMap((e) => objects(e, key));
  if (kindOf(value) !== "object") return [];
  return [[key, value as Obj], ...Object.entries(value as Obj).flatMap(([k, v]) => objects(v, k))];
}

/** Every string anywhere inside `value`. */
const strings = (value: unknown): string[] =>
  typeof value === "string"
    ? [value]
    : Array.isArray(value)
      ? value.flatMap(strings)
      : kindOf(value) === "object"
        ? Object.values(value as Obj).flatMap(strings)
        : [];

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const desc = (xs: number[]) => xs.every((x, i) => i === 0 || xs[i - 1]! >= x);

// ── the guard itself ─────────────────────────────────────────────────────────────
describe("the walker can fail (a check that cannot fail proves nothing)", () => {
  const rank = { tier: 15, rr: 42 };

  it("accepts a correct object", () => {
    expect(problems(rank, "Rank", "r")).toEqual([]);
  });

  it("a key the descriptor lacks", () => {
    expect(problems({ ...rank, extra: 1 }, "Rank", "r")).toEqual(["r.extra: not a key of Rank"]);
  });

  it("a required key that is missing", () => {
    expect(problems({ tier: 15 }, "Rank", "r")).toEqual(["r.rr: required key of Rank is missing"]);
  });

  it("a value of the wrong kind, and null where the descriptor does not allow it", () => {
    expect(problems({ tier: "15", rr: 42 }, "Rank", "r")).toEqual(["r.tier: string is not one of number"]);
    expect(problems({ tier: null, rr: 42 }, "Rank", "r")).toEqual(["r.tier: null is not one of number"]);
  });

  it("an optional key may be absent, and null only where the kinds say so", () => {
    const card = LIVE.players[0]!;
    const { level: _l, ...noLevel } = card;
    expect(problems(noLevel, "PlayerCard", "c")).toEqual([]);
    expect(problems({ ...card, level: null }, "PlayerCard", "c")).toEqual([]);
    expect(problems({ ...card, puuid: null }, "PlayerCard", "c")).toEqual(["c.puuid: null is not one of string"]);
  });

  it("a string outside its closed set, and a fraction that is a percent", () => {
    expect(problems({ result: "won", rrEarned: 1, startMs: 1 }, "Last5Entry", "e")).toEqual([
      'e.result: "won" is not one of win|loss|draw',
    ]);
    expect(problems({ ...LIVE.players[0]!, hsPct: 27 }, "PlayerCard", "c")).toEqual(["c.hsPct: 27 is not a 0..1 fraction"]);
  });

  it("nested objects and array elements are checked, with their path", () => {
    const card = { ...LIVE.players[0]!, rank: { tier: 15 }, last5: [{ result: "win", startMs: 1 }] };
    expect(problems(card, "PlayerCard", "c")).toEqual([
      "c.rank.rr: required key of Rank is missing",
      "c.last5[0].rrEarned: required key of Last5Entry is missing",
    ]);
  });

  it("DisplayName picks its variant by kind; an unknown kind fails", () => {
    expect(problems({ kind: "hiddenAgent", agentId: "a" }, "DisplayName", "n")).toEqual([]);
    expect(problems({ kind: "hiddenAgent" }, "DisplayName", "n")).toEqual(["n.agentId: required key of DisplayName is missing"]);
    expect(problems({ kind: "visible", name: "A" }, "DisplayName", "n")).toEqual(["n.tag: required key of DisplayName is missing"]);
    expect(problems({ kind: "mystery" }, "DisplayName", "n")).toEqual(['n: no DisplayName descriptor for kind "mystery"']);
  });

  it("every `shape` a descriptor names exists", () => {
    const named = new Set([...Object.keys(registry), "DisplayName"]);
    for (const [owner, shape] of Object.entries(registry)) {
      for (const [key, d] of Object.entries(shape)) {
        if (d.shape) expect(named.has(d.shape), `${owner}.${key} -> ${d.shape}`).toBe(true);
      }
    }
  });
});

// ── the goldens ──────────────────────────────────────────────────────────────────
describe("the goldens match the descriptors", () => {
  it("these are all the goldens: a new one on the Rust side needs a descriptor and a test here", () => {
    expect(readdirSync(GOLDEN_DIR).sort()).toEqual(["live_state.json", "match_details_view.json", "player_details.json"]);
  });

  it("live_state.json is a LiveState", () => {
    expect(problems(LIVE, "LiveState", "live_state")).toEqual([]);
  });

  it("player_details.json is a PlayerDetails", () => {
    expect(problems(DETAILS, "PlayerDetails", "player_details")).toEqual([]);
  });

  it("match_details_view.json holds two MatchDetailsViews (a competitive match and a deathmatch)", () => {
    expect(MATCHES).toHaveLength(2);
    MATCHES.forEach((m, i) => expect(problems(m, "MatchDetailsView", `match_details_view[${i}]`)).toEqual([]));
  });

  it("the goldens exercise the tri-state: every optional card key is absent on some card and null on another (R23)", () => {
    const cards = LIVE.players;
    for (const [key, d] of Object.entries(shapes.PlayerCard)) {
      if (!("optional" in d)) continue;
      expect(
        cards.some((c) => !(key in c)),
        `${key} is never absent`,
      ).toBe(true);
      expect(
        cards.some((c) => c[key] === null),
        `${key} is never null`,
      ).toBe(true);
    }
  });
});

describe("golden invariants the UI relies on", () => {
  it("the loadout has exactly one absent weapon (still resolving) and one null (none), R97", () => {
    const lo = DETAILS.loadout as Obj;
    const weapons = ["vandal", "phantom", "operator", "sheriff"];
    expect(weapons.filter((w) => !(w in lo))).toHaveLength(1);
    expect(weapons.filter((w) => lo[w] === null)).toHaveLength(1);
    expect(weapons.filter((w) => kindOf(lo[w]) === "object")).toHaveLength(2);
  });

  it("every fromMapId is a uuid or null, and no Riot map path reaches the wire", () => {
    const all = [LIVE, DETAILS, MATCHES];
    const from = all.flatMap((g) => objects(g)).filter(([, o]) => "fromMapId" in o);
    expect(from.length).toBeGreaterThan(3);
    for (const [, o] of from) expect(o.fromMapId === null || UUID.test(String(o.fromMapId))).toBe(true);
    expect(from.some(([, o]) => o.fromMapId === null)).toBe(true);
    expect(strings(all).filter((s) => s.includes("/Game/"))).toEqual([]);
  });

  it("every mapId is a uuid; '' or null only where the map is unknown", () => {
    const ids = [LIVE, DETAILS, MATCHES].flatMap((g) => objects(g)).filter(([, o]) => "mapId" in o).map(([, o]) => o.mapId);
    expect(ids.length).toBeGreaterThan(5);
    for (const id of ids) expect(id === null || id === "" || UUID.test(String(id))).toBe(true);
  });

  it("recent, encounters and last5 come newest first", () => {
    const starts = (xs: unknown) => (xs as Array<{ startMs: number }>).map((x) => x.startMs);
    expect(desc(starts(DETAILS.recent))).toBe(true);
    expect(desc(starts(DETAILS.encounters))).toBe(true);
    for (const c of [...LIVE.players, DETAILS.card as Obj]) {
      if (Array.isArray(c.last5)) expect(desc(starts(c.last5))).toBe(true);
    }
    expect((DETAILS.recent as unknown[]).length).toBeGreaterThan(2);
  });

  it("a hidden player's card keeps its rank and stats: only the Riot ID is hidden", () => {
    const hidden = LIVE.players.filter((c) => (c.name as Obj | undefined)?.kind === "hiddenPlayer");
    expect(hidden).toHaveLength(1);
    for (const k of ["rank", "prevActTier", "peak", "kd", "hsPct", "actWinrate", "actWins", "actGames", "last5", "level"]) {
      expect(hidden[0]![k], k).not.toBeNull();
      expect(hidden[0]![k], k).not.toBeUndefined();
    }
    expect(hidden[0]!.agentId).toBeNull(); // R99: no agent before the lock
  });

  it("heroKd and heroHs are null exactly when no act match has stats; heroHs is a fraction (R116, R120)", () => {
    expect(DETAILS.heroMatches).toBeGreaterThan(0);
    expect(DETAILS.heroMatches).toBeLessThanOrEqual(20);
    expect(DETAILS.heroKd).not.toBeNull();
    expect(DETAILS.heroHs as number).toBeLessThanOrEqual(1);
  });

  it("a match without a stats row sends null for the whole stat line, never a counted 0 (R102, R124)", () => {
    const recent = DETAILS.recent as Obj[];
    const bare = recent.filter((m) => m.kills === null);
    expect(bare.length).toBeGreaterThan(0);
    for (const m of bare) for (const k of ["deaths", "assists", "kd", "acs", "hsPct"]) expect(m[k], `recent.${k}`).toBeNull();
    for (const m of recent.filter((m) => m.kills !== null)) for (const k of ["kills", "deaths", "assists", "kd"]) expect(kindOf(m[k])).toBe("number");

    const rows = MATCHES.flatMap((m) => (m.teams as Obj[]).flatMap((t) => t.players as Obj[]));
    const scoreless = rows.filter((r) => r.kills === null);
    expect(scoreless.length).toBeGreaterThan(0);
    for (const r of scoreless) for (const k of ["deaths", "assists", "acs", "adr", "hsPct", "tier"]) expect(r[k], `row.${k}`).toBeNull();
  });

  it("scoreboard rows are sorted by ACS descending, null last; team 0 holds the focus or self row; no-team modes have one team", () => {
    for (const m of MATCHES) {
      const teams = m.teams as Obj[];
      for (const t of teams) {
        const acs = (t.players as Obj[]).map((p) => (p.acs as number | null) ?? -1);
        expect(desc(acs)).toBe(true);
      }
      const flag = m.perspective === "self" ? "isSelf" : "isFocus";
      expect((teams[0]!.players as Obj[]).some((p) => p[flag] === true)).toBe(true);
    }
    const dm = MATCHES.find((m) => m.queueId === "deathmatch")!;
    expect(dm.teams as unknown[]).toHaveLength(1);
    expect((dm.teams as Obj[])[0]).toMatchObject({ teamId: "", result: null, roundsWon: null, firstHalf: null });
    for (const p of (dm.teams as Obj[])[0]!.players as Obj[]) expect(p).toMatchObject({ tier: null, adr: null });
  });

  it("a remembered name in a scoreboard row is on a player hidden live; a hidden agent's name is the agent id", () => {
    const rows = MATCHES.flatMap((m) => (m.teams as Obj[]).flatMap((t) => t.players as Obj[]));
    const kinds = new Set(rows.map((r) => (r.name as Obj).kind));
    expect(kinds).toEqual(new Set(["visible", "remembered", "hiddenAgent"]));
    for (const r of rows) {
      const n = r.name as Obj;
      if (n.kind === "hiddenAgent") expect(n.agentId).toBe(r.agentId);
    }
  });
});

// ── absent key -> skeleton, null -> — (R23), through the real card ────────────────
describe("the card reads the golden tri-state: an absent key is a skeleton, null is —", () => {
  let PlayerCard: typeof import("../components/PlayerCard.svelte").default;
  beforeAll(async () => {
    vi.stubGlobal("location", { search: "" });
    vi.stubGlobal("document", { documentElement: { style: { setProperty: vi.fn() } }, getElementById: () => ({ setAttribute: vi.fn() }) });
    PlayerCard = (await import("../components/PlayerCard.svelte")).default;
    // With the names pending (spec §5.8) a tier name is a skeleton whatever the card carries.
    (await import("./stores")).meta.set(demoMeta as Meta);
  });
  afterAll(() => vi.unstubAllGlobals());

  const html = (card: unknown) => render(PlayerCard, { props: { card: card as Card } }).body.replace(/<!--.*?-->/g, "");
  const byPuuid = (p: string) => LIVE.players.find((c) => c.puuid === p)!;
  const text = (h: string) => h.replace(/<[^>]+>/g, "");
  const skeletons = (h: string) => (h.match(/class="sk[ "]/g) ?? []).length;

  it("the loading card (every tri-state key absent) shows skeletons and no —", () => {
    const h = html(byPuuid("puuid-loading"));
    expect(skeletons(h)).toBeGreaterThanOrEqual(7); // name, level, rank, peak, prev, K/D + HS% + Win%, last 5
    expect(text(h)).not.toContain("—");
  });

  it("the missing card (every tri-state key null) shows — and no skeleton", () => {
    const h = html(byPuuid("puuid-missing"));
    expect(skeletons(h)).toBe(0);
    expect(text(h).split("—").length - 1).toBeGreaterThanOrEqual(6);
    expect(text(h)).toContain("Lv —");
  });

  it("a full card has neither", () => {
    const h = html(byPuuid("puuid-self"));
    expect(skeletons(h)).toBe(0);
    expect(text(h)).not.toContain("—");
  });
});

// ── the demo fixtures follow the same contract ────────────────────────────────────
describe("the demo fixtures follow the golden contract", () => {
  const scenes = Object.entries(SCENES);
  const liveCards = scenes.flatMap(([n, s]) => (s.live?.players ?? []).map((c, i) => [`${n}.players[${i}]`, c] as const));
  const detailsOf = (d: PlayerDetails | "pending") => (d === "pending" ? [] : [d]);
  const allDetails = [
    ...scenes.flatMap(([n, s]) => Object.entries(s.details ?? {}).flatMap(([p, d]) => detailsOf(d).map((x) => [`${n}.details[${p.slice(0, 4)}]`, x] as const))),
    ...Object.entries(PUUID).map(([id]) => [`detailsFor(${id})`, detailsFor(id)] as const),
  ];
  const allMatches = [
    ...scenes.flatMap(([n, s]) => Object.entries(s.matches ?? {}).flatMap(([id, m]) => (m === "pending" ? [] : [[`${n}.matches[${id}]`, m] as const]))),
    ...Object.keys(PUUID).flatMap((id) => [
      ...Array.from({ length: 8 }, (_, i) => recentMatchId(id, i)),
      ...Array.from({ length: 3 }, (_, i) => encounterMatchId(id, i)),
      `mdemo:${id}:dm`,
    ].flatMap((mid) => {
      try {
        return [[mid, matchFor(mid)] as const];
      } catch (e) {
        if (!/^demo: /.test((e as Error).message)) throw e;
        return []; // the roster player has fewer rows than the index
      }
    })),
  ];

  it("there is something to check", () => {
    expect(liveCards.length).toBeGreaterThan(80);
    expect(allDetails.length).toBeGreaterThan(20);
    expect(allMatches.length).toBeGreaterThan(50);
    expect(roster()).toHaveLength(10);
  });

  it("every LiveState is a LiveState", () => {
    for (const [n, s] of scenes) if (s.live) expect(problems(s.live, "LiveState", n)).toEqual([]);
  });

  it("every PlayerDetails is a PlayerDetails (hero stats, tri-state loadout, nullable stat lines)", () => {
    for (const [n, d] of allDetails) expect(problems(d, "PlayerDetails", n)).toEqual([]);
  });

  it("every MatchDetailsView is a MatchDetailsView", () => {
    for (const [n, m] of allMatches) expect(problems(m, "MatchDetailsView", n)).toEqual([]);
  });

  it("no Riot map path anywhere; every map id a uuid; every fromMapId a uuid or null", () => {
    const everything = [...liveCards.map(([, c]) => c), ...allDetails.map(([, d]) => d), ...allMatches.map(([, m]) => m)];
    expect(strings(everything).filter((s) => s.includes("/Game/"))).toEqual([]);
    for (const [, o] of objects(everything)) {
      if ("mapId" in o) expect(o.mapId === null || o.mapId === "" || UUID.test(String(o.mapId)), String(o.mapId)).toBe(true);
      if ("fromMapId" in o) expect(o.fromMapId === null || UUID.test(String(o.fromMapId)), String(o.fromMapId)).toBe(true);
    }
  });

  it("recent and encounters are newest first, at most 8 recent; last5 newest first", () => {
    for (const [n, d] of allDetails) {
      expect(d.recent.length, n).toBeLessThanOrEqual(8);
      expect(desc(d.recent.map((m) => m.startMs)), `${n} recent`).toBe(true);
      expect(desc(d.encounters.map((e) => e.startMs)), `${n} encounters`).toBe(true);
    }
    for (const [n, c] of liveCards) if (c.last5) expect(desc(c.last5.map((e) => e.startMs)), n).toBe(true);
  });

  it("hero stats: null exactly when there is no counted match, heroHs a fraction, at most 20 matches (R116, R120)", () => {
    for (const [n, d] of allDetails) {
      expect((d.heroKd === null) === (d.heroMatches === 0), `${n} heroKd null iff heroMatches 0`).toBe(true);
      expect((d.heroHs === null) === (d.heroMatches === 0), `${n} heroHs null iff heroMatches 0`).toBe(true);
      expect(d.heroMatches, n).toBeLessThanOrEqual(20);
    }
    expect(allDetails.some(([, d]) => d.heroMatches > 0)).toBe(true);
    expect(allDetails.some(([, d]) => d.heroMatches === 0)).toBe(true);
  });

  it("a hidden player's card keeps its rank and stats in every scene (only the Riot ID is hidden)", () => {
    const hidden = liveCards.filter(([, c]) => c.name?.kind === "hiddenPlayer");
    expect(hidden.length).toBeGreaterThan(0);
    for (const [n, c] of hidden) {
      if (n.startsWith("pregame")) {
        for (const k of ["level", "rank", "prevActTier", "peak", "kd", "hsPct", "actWinrate", "last5"] as const) {
          expect(c[k], `${n}.${k}`).not.toBeUndefined();
        }
      }
      expect(c.agentId, n).toBeNull(); // R99
    }
  });

  it("a stat line that is null is null as a whole; a scoreboard sorts by ACS, null last", () => {
    for (const [n, d] of allDetails) {
      for (const m of d.recent) {
        if (m.kills === null) expect([m.deaths, m.assists, m.kd, m.acs, m.hsPct], `${n} ${m.matchId}`).toEqual([null, null, null, null, null]);
      }
    }
    for (const [n, m] of allMatches) {
      for (const t of m.teams) {
        expect(desc(t.players.map((p) => p.acs ?? -1)), `${n} ${t.teamId}`).toBe(true);
        for (const p of t.players) {
          if (p.kills === null) expect([p.deaths, p.assists, p.acs, p.adr, p.hsPct, p.tier], `${n} ${p.puuid}`).toEqual([null, null, null, null, null, null]);
        }
      }
    }
  });

  it("a no-team mode has one team with no id, result, rounds or halves, and rows with no rank and no ADR", () => {
    const dm = allMatches.filter(([, m]) => m.queueId === "deathmatch");
    expect(dm.length).toBeGreaterThan(0);
    for (const [n, m] of dm) {
      expect(m.teams, n).toHaveLength(1);
      expect(m.teams[0], n).toMatchObject({ teamId: "", result: null, roundsWon: null, firstHalf: null });
      for (const p of m.teams[0]!.players) expect(p, `${n} ${p.puuid}`).toMatchObject({ tier: null, adr: null });
    }
  });

  it("team 0 is the user's team in a self view, else the focus player's", () => {
    for (const [n, m] of allMatches) {
      const flag = m.perspective === "self" ? "isSelf" : "isFocus";
      expect(m.teams[0]!.players.some((p) => p[flag]), n).toBe(true);
    }
  });
});
