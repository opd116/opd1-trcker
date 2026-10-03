/// <reference types="node" />
import { readFileSync, readdirSync } from "node:fs";
import { basename } from "node:path";
import { fileURLToPath } from "node:url";
import { afterEach, describe, expect, it, vi } from "vitest";
import { writable } from "svelte/store";
import demoMeta from "./demo/meta.json";
import { DEMO_NOW, detailsFor, encounterMatchId, matchFor, PUUID, recentMatchId, roster } from "./demo/cdv8";
import * as fixtures from "./demo/fixtures";
import { resolveDetails, resolveMatch, resolveMyStats, SCENES, type SceneName } from "./demo/scenes";
import { dateLong, durationMin, score, time12 } from "./format";
import type { DisplayName, LiveState, MatchDetailsView, Meta, MyStats, PlayerCard, PlayerDetails } from "./types";

const NAMES: SceneName[] = [
  "boot", "closed", "closed-empty", "lobby1", "lobby2", "lobby2-loading", "lobby2-unranked", "lobby3", "lobby4", "lobby5",
  "pregame", "pregame-nomap", "pregame-loading", "ingame", "ingame-longnames", "ingame-parties4", "ingame-partial",
  "ingame-unranked", "ingame-loading", "ingame-nometa", "ingame-remembered", "ingame-rr-unknown",
  "details", "details-more", "details-self", "details-hidden", "details-empty", "details-loading", "details-longnames",
  "details-partial", "details-nostats",
  "match", "match-enc", "match-hidden", "match-dm", "match-loading", "match-longnames", "match-nostats",
  "mystats", "mystats-new", "mystats-loading", "mystats-unranked", "mystats-immortal", "mystats-rrnull",
  "mystats-longnames", "mystats-closed",
];

const meta = demoMeta as Meta;
const W16 = "WWWWWWWWWWWWWWWW";
const players = (s: SceneName): PlayerCard[] => SCENES[s].live?.players ?? [];
const nameOf = (c: PlayerCard): DisplayName | null | undefined => c.name;
const detailsOf = (s: SceneName, puuid: string): PlayerDetails => {
  const d = SCENES[s].details?.[puuid];
  if (!d || d === "pending") throw new Error(`${s}: no details for ${puuid}`);
  return d;
};
const routeOf = <N extends "details" | "match">(s: SceneName, name: N) => {
  const r = SCENES[s].route;
  if (r.name !== name) throw new Error(`${s}: route is ${r.name}`);
  return r as Extract<typeof r, { name: N }>;
};
const matchOf = (s: SceneName): MatchDetailsView => {
  const r = routeOf(s, "match");
  const m = SCENES[s].matches?.[r.matchId];
  if (!m || m === "pending") throw new Error(`${s}: no match ${r.matchId}`);
  return m;
};

describe("registry", () => {
  it("every SceneName resolves", () => {
    expect(Object.keys(SCENES).sort()).toEqual([...NAMES].sort());
    for (const n of NAMES) {
      const s = SCENES[n];
      expect(s, n).toBeDefined();
      expect(s.route, n).toBeDefined();
      expect(s.live === null || typeof s.live === "object", n).toBe(true);
    }
  });

  it("boot has no LiveState yet, closed-empty has no players", () => {
    expect(SCENES.boot.live).toBeNull();
    expect(SCENES["closed-empty"].live).toEqual({ stage: "closed", mapId: null, players: [] });
  });

  it("fixtures.ts exports no buildScene, isScene or scene list (one registry, R95)", () => {
    const keys = Object.keys(fixtures);
    expect(keys).not.toContain("buildScene");
    expect(keys).not.toContain("isScene");
    expect(keys.filter((k) => /scene/i.test(k))).toEqual([]);
    expect(keys).toEqual(expect.arrayContaining(["card", "last5", "recent"]));
  });

  it("every agent and map id used by a scene exists in the demo meta", () => {
    const agents = new Set<string>();
    const maps = new Set<string>();
    const name = (n: DisplayName | null | undefined) => {
      if (n?.kind === "hiddenAgent") agents.add(n.agentId);
      if (n?.kind === "remembered" && n.fromMapId) maps.add(n.fromMapId);
    };
    const card = (c: PlayerCard) => {
      if (c.agentId) agents.add(c.agentId);
      name(c.name);
    };
    for (const n of NAMES) {
      const s = SCENES[n];
      if (s.meta === "pending") continue;
      if (s.live?.mapId) maps.add(s.live.mapId);
      s.live?.players.forEach(card);
      for (const d of Object.values(s.details ?? {})) {
        if (d === "pending") continue;
        card(d.card);
        // "" is the wire's unknown map (R133 details-nostats), not an id: it renders as —, never looked up.
        d.recent.forEach((r) => (r.mapId && maps.add(r.mapId), r.agentId && agents.add(r.agentId)));
        d.encounters.forEach((e) => {
          if (e.mapId) maps.add(e.mapId);
          if (e.themAgentId) agents.add(e.themAgentId);
          if (e.selfAgentId) agents.add(e.selfAgentId);
        });
        d.topTeammates.forEach((t) => name(t.name));
      }
      for (const m of Object.values(s.matches ?? {})) {
        if (m === "pending") continue;
        if (m.mapId) maps.add(m.mapId); // "" (unknown) as above: the nostats Against encounter's match
        m.teams.forEach((t) =>
          t.players.forEach((p) => {
            if (p.agentId) agents.add(p.agentId);
            name(p.name);
          }),
        );
      }
    }
    for (const id of agents) expect(meta.agents[id], `agent ${id}`).toBeDefined();
    for (const id of maps) expect(meta.maps[id], `map ${id}`).toBeDefined();
  });
});

describe("live scenes", () => {
  it("ingame has 5+5 cards", () => {
    const p = players("ingame");
    expect(SCENES.ingame.live?.stage).toBe("ingame");
    expect(p.filter((c) => c.team === "ally")).toHaveLength(5);
    expect(p.filter((c) => c.team === "enemy")).toHaveLength(5);
    expect(p.filter((c) => c.isSelf)).toHaveLength(1);
    expect(p[0]?.isSelf).toBe(true);
  });

  it("Fahad's last5 results are newest first", () => {
    const fahad = players("ingame")[0]!;
    expect(fahad.last5!.map((r) => r.result)).toEqual(["win", "loss", "win", "win", "loss"]);
    const starts = fahad.last5!.map((r) => r.startMs);
    expect([...starts].sort((a, b) => b - a)).toEqual(starts);
    expect(fahad.last5![0]!.rrEarned).toBe(18);
    expect(DEMO_NOW - starts[0]!).toBe(45 * 60_000);
  });

  it("Math.round(actWinrate*100) equals the design's win[0] for all 10 players", () => {
    expect(players("ingame").map((c) => Math.round(c.actWinrate! * 100))).toEqual([58, 52, 49, 55, 61, 63, 54, 56, 49, 67]);
  });

  it("wins and losses are the design's win[1] / win[2] for all 10 players (v8 roster)", () => {
    const p = players("ingame");
    for (const c of p) expect(c.actWins!).toBeLessThanOrEqual(c.actGames!);
    expect(p.map((c) => [c.actWins, c.actGames! - c.actWins!])).toEqual([
      [31, 22], [22, 20], [19, 20], [27, 22], [25, 16], [41, 24], [30, 26], [28, 22], [20, 21], [2, 1],
    ]);
    expect(p.map((c) => c.actGames)).toEqual([53, 42, 39, 49, 41, 65, 56, 50, 41, 3]);
  });

  it("the design's roster: party marks, encounters, hidden and remembered names", () => {
    const by = (n: string) => players("ingame").find((c) => c.name && "name" in c.name && c.name.name === n)!;
    expect(by("Drake").party).toEqual({ group: 0, confirmed: true });
    expect(by("Nova").party).toEqual({ group: 0, confirmed: true });
    expect(by("Mara").party).toEqual({ group: 1, confirmed: false });
    expect(by("Kilo").party).toEqual({ group: 1, confirmed: false });
    expect(by("Rook").name?.kind).toBe("remembered");
    const hiddenAgent = players("ingame").filter((c) => c.name?.kind === "hiddenAgent");
    expect(hiddenAgent).toHaveLength(1);
    const enc = players("ingame").filter((c) => c.encounters && c.encounters.with + c.encounters.against > 0);
    expect(enc.map((c) => (c.name && "name" in c.name ? c.name.name : "?")).sort()).toEqual(["Kilo", "Mara", "Rook", "VesperUnbreakabl"]);
    // The backend marks the user's own card `isSelfParty` too (state.rs `r.is_self_party || is_self`): only Fahad here.
    expect(players("ingame").filter((c) => c.isSelfParty).map((c) => c.puuid)).toEqual([players("ingame")[0]!.puuid]);
  });

  it("ingame-remembered (R145): one remembered card of each source, all hidden on the live screen", () => {
    const rem = players("ingame-remembered").flatMap((c) => (c.name?.kind === "remembered" ? [c.name] : []));
    expect(rem.map((n) => n.source).sort()).toEqual(["otherMatch", "ownMatch", "seen"]);
    for (const n of rem) {
      if (n.source === "seen") {
        expect(n.fromMapId, "a name-service answer has no map").toBeNull();
        expect(n.fromStartMs, "seen carries when it was last confirmed").not.toBeNull();
      } else {
        expect(n.fromMapId, n.source).not.toBeNull();
        expect(n.fromStartMs, n.source).not.toBeNull();
      }
    }
    // The design's roster is untouched elsewhere: `ingame` keeps exactly Rook (own match) as its one remembered card.
    const inGame = players("ingame").flatMap((c) => (c.name?.kind === "remembered" ? [c.name] : []));
    expect(inGame.map((n) => [n.name, n.source])).toEqual([["Rook", "ownMatch"]]);
  });

  it("ingame-rr-unknown (R154): two cards whose RR is unknown (null) with their tier kept, the rest is the ingame roster", () => {
    const base = players("ingame");
    const unknown = players("ingame-rr-unknown").filter((c) => c.rank && c.rank.rr === null);
    expect(unknown.map((c) => (c.name && "name" in c.name ? c.name.name : "?"))).toEqual(["Sable", "Mara"]);
    for (const c of unknown) {
      const was = base.find((b) => b.puuid === c.puuid)!.rank!;
      expect(c.rank, "the tier is the one the design has").toEqual({ tier: was.tier, rr: null });
    }
    // Nothing else moved: with the two ranks put back it is the ingame roster.
    const restored = players("ingame-rr-unknown").map((c) => (unknown.includes(c) ? { ...c, rank: base.find((b) => b.puuid === c.puuid)!.rank } : c));
    expect(restored).toEqual(base);
  });

  it("wire invariant in every scene: the self card is isSelfParty, and own-party cards carry no party or encounters", () => {
    for (const n of NAMES) {
      for (const c of players(n)) {
        if (c.isSelf) expect(c.isSelfParty, `${n}: self card`).toBe(true);
        if (c.isSelfParty) {
          expect(c.party, `${n}: own party`).toBeNull();
          expect(c.encounters, `${n}: own party`).toBeNull();
        }
      }
    }
  });

  it("closed has only the user's own card, with the outline flag", () => {
    const p = players("closed");
    expect(SCENES.closed.live?.stage).toBe("closed");
    expect(p).toHaveLength(1);
    expect(p[0]!.isSelf).toBe(true);
    expect(p[0]!.agentId).toBeNull();
    expect(SCENES["closed-empty"].live?.players).toEqual([]);
  });

  it("lobby1..5 hold n cards, self first, the rest in the self party", () => {
    (["lobby1", "lobby2", "lobby3", "lobby4", "lobby5"] as const).forEach((s, i) => {
      const p = players(s);
      expect(SCENES[s].live?.stage).toBe("menus");
      expect(p, s).toHaveLength(i + 1);
      expect(p[0]!.isSelf).toBe(true);
      expect(p.every((c) => c.isSelfParty && c.party === null && c.encounters === null)).toBe(true);
      expect(p.slice(1).every((c) => !c.isSelf)).toBe(true);
    });
  });

  it("lobby2 has exactly 1 ranked player; lobby3 has 2; lobby4 has a rank still loading", () => {
    const ranked = (s: SceneName) => players(s).filter((c) => (c.rank?.tier ?? 0) >= 3).length;
    expect(ranked("lobby2")).toBe(1);
    expect(ranked("lobby3")).toBe(2);
    expect(players("lobby3")).toHaveLength(3);
    const loading = players("lobby4").filter((c) => c.rank === undefined);
    expect(loading).toHaveLength(1);
    expect(ranked("lobby4")).toBeGreaterThanOrEqual(1);
  });

  it("lobby2-loading: none ranked, exactly one rank absent; lobby2-unranked: both null", () => {
    const l = players("lobby2-loading");
    expect(l).toHaveLength(2);
    expect(l.filter((c) => (c.rank?.tier ?? 0) >= 3)).toHaveLength(0);
    expect(l.filter((c) => c.rank === undefined)).toHaveLength(1);
    const u = players("lobby2-unranked");
    expect(u).toHaveLength(2);
    expect(u.every((c) => c.rank === null)).toBe(true);
  });

  it("pregame (R99): only the 2 locked allies have an agent", () => {
    const p = players("pregame");
    expect(SCENES.pregame.live?.stage).toBe("pregame");
    expect(SCENES.pregame.live?.mapId).toBeTruthy();
    expect(p).toHaveLength(5);
    expect(p.every((c) => c.team === "ally")).toBe(true);
    const withAgent = p.filter((c) => c.agentId !== null);
    expect(withAgent).toHaveLength(2);
    const hiddenPlayer = p.filter((c) => c.name?.kind === "hiddenPlayer");
    expect(hiddenPlayer).toHaveLength(1);
    expect(hiddenPlayer[0]!.agentId).toBeNull(); // hasn't locked
    // the hovered and the unpicked ally look the same on the wire
    const noAgent = p.filter((c) => c.agentId === null && c.name?.kind !== "hiddenPlayer");
    expect(noAgent).toHaveLength(2);
    expect(SCENES["pregame-nomap"].live?.mapId).toBeNull();
    expect(players("pregame-nomap")).toEqual(p);
    expect(SCENES["pregame-loading"].live?.players).toEqual([]);
  });

  it("ingame-longnames: long names, Ascendant 3 (current and prev), V26 A4 · 11mo except one E10 A3, one 123", () => {
    const p = players("ingame-longnames");
    expect(p).toHaveLength(10);
    for (const c of p) {
      if (c.name?.kind === "visible" || c.name?.kind === "remembered") {
        expect(c.name.name).toBe(W16);
        expect(c.name.tag).toBe("WWWWW");
      }
      expect(c.rank?.tier).toBe(23);
      // R125/R126: the widest Prev row (`Prev` + `Ascendant 3`) on every card.
      expect(c.prevActTier).toBe(23);
      expect(c.peak?.tier).toBe(23);
      expect(c.peak && Math.floor(c.peak.sinceActEndMs! / 86_400_000 / 30)).toBe(11);
    }
    const labels = p.map((c) => c.peak!.actLabel);
    expect(labels.filter((l) => l === "V26 A4")).toHaveLength(9);
    expect(labels.filter((l) => l === "E10 A3")).toHaveLength(1);
    const big = p.filter((c) => c.encounters && c.encounters.with + c.encounters.against === 123);
    expect(big).toHaveLength(1);
    expect(big[0]!.isSelf || big[0]!.isSelfParty || big[0]!.party).toBeFalsy();
  });

  it("ingame-parties4: 4 groups, likely and confirmed, none on self", () => {
    const p = players("ingame-parties4");
    const marked = p.filter((c) => c.party);
    expect(new Set(marked.map((c) => c.party!.group))).toEqual(new Set([0, 1, 2, 3]));
    expect(marked).toHaveLength(8);
    expect(marked.filter((c) => c.party!.confirmed).length).toBeGreaterThanOrEqual(2);
    expect(marked.filter((c) => !c.party!.confirmed).length).toBeGreaterThanOrEqual(2);
    expect(marked.some((c) => c.isSelf)).toBe(false);
    expect(marked.filter((c) => c.team === "enemy").map((c) => c.party!.group).sort()).toEqual([2, 2, 3, 3]);
  });

  it("ingame-partial: every loadable field is absent on some card", () => {
    const p = players("ingame-partial");
    const absent = (k: keyof PlayerCard) => p.some((c) => !(k in c) || c[k] === undefined);
    for (const k of ["name", "level", "rank", "peak", "prevActTier", "kd", "hsPct", "actWinrate", "last5"] as const) {
      expect(absent(k), k).toBe(true);
    }
    expect(p[0]!.name).toBeDefined();
    expect(p[0]!.rank).toBeDefined();
  });

  it("ingame-unranked: an all-null card and a 2-match card", () => {
    const p = players("ingame-unranked");
    const empty = p.filter(
      (c) => c.rank === null && c.peak === null && c.prevActTier === null && c.kd === null && c.hsPct === null && c.actWinrate === null && c.level === null,
    );
    expect(empty).toHaveLength(1);
    expect(empty[0]!.last5).toEqual([]);
    expect(p.filter((c) => c.last5?.length === 2)).toHaveLength(1);
  });

  it("ingame-loading and ingame-nometa", () => {
    expect(SCENES["ingame-loading"].live).toEqual({ stage: "ingame", mapId: null, players: [] });
    expect(SCENES["ingame-nometa"].meta).toBe("pending");
    expect(players("ingame-nometa")).toEqual(players("ingame"));
    expect(SCENES.ingame.meta).toBeUndefined();
  });
});

describe("details scenes", () => {
  it("details (Mara): 8 recent rows, 3 encounters, Kilo as a live teammate", () => {
    const r = routeOf("details", "details");
    expect(r.more).toBe(false);
    const d = detailsOf("details", r.puuid);
    expect(d.card.name).toEqual({ kind: "visible", name: "Mara", tag: "7788" });
    expect(d.recent).toHaveLength(8);
    expect(d.recent.slice(0, 3).map((m) => meta.maps[m.mapId]!.name)).toEqual(["Lotus", "Sunset", "Split"]);
    expect(d.recent.slice(0, 3).map((m) => score(m.roundsWon, m.roundsLost))).toEqual(["13–9", "13–11", "7–13"]);
    expect(d.encounters).toHaveLength(3);
    expect(d.topTeammates.map((t) => t.count)).toEqual([14, 9, 6]);
    const live = players("details");
    const liveMates = d.topTeammates.filter((t) => live.some((c) => c.puuid === t.puuid && c.name?.kind === "visible"));
    expect(liveMates).toHaveLength(1);
    expect(liveMates[0]!.name).toMatchObject({ kind: "visible", name: "Kilo" });
    expect(Object.keys(d.loadout).sort()).toEqual(["operator", "phantom", "sheriff", "vandal"]);
    expect(d.card.puuid).toBe(r.puuid);
    // newest first
    expect(d.recent.map((m) => m.startMs)).toEqual([...d.recent.map((m) => m.startMs)].sort((a, b) => b - a));
  });

  it("details-more opens with 8 shown; details-self has no encounters", () => {
    expect(routeOf("details-more", "details").more).toBe(true);
    const s = routeOf("details-self", "details");
    expect(players("details-self").find((c) => c.puuid === s.puuid)?.isSelf).toBe(true);
    const d = detailsOf("details-self", s.puuid);
    expect(d.card.isSelf).toBe(true);
    expect(d.encounters).toEqual([]);
    expect(d.topTeammates).toHaveLength(3);
  });

  it("details-hidden: Cypher (hiddenAgent) by default, Rook (remembered) is in the map too", () => {
    const r = routeOf("details-hidden", "details");
    expect(detailsOf("details-hidden", r.puuid).card.name?.kind).toBe("hiddenAgent");
    const kinds = Object.values(SCENES["details-hidden"].details!).map((d) => (d === "pending" ? "" : d.card.name?.kind));
    expect(kinds).toContain("remembered");
  });

  it("details-empty: a no-history player", () => {
    const r = routeOf("details-empty", "details");
    const d = detailsOf("details-empty", r.puuid);
    expect(d.recent).toEqual([]);
    expect(d.topTeammates).toEqual([]);
    expect(d.encounters).toEqual([]);
    expect(d.card.rank).toBeNull();
    expect(d.card.last5).toEqual([]);
    expect(d.loadout).toEqual({ vandal: null, phantom: null, operator: null, sheriff: null });
  });

  it("details-loading never resolves", () => {
    const r = routeOf("details-loading", "details");
    expect(SCENES["details-loading"].details![r.puuid]).toBe("pending");
  });

  it("details-longnames: W names; Top 3 = a live W teammate, a long remembered name and a hidden agent; long skin lines", () => {
    const r = routeOf("details-longnames", "details");
    const d = detailsOf("details-longnames", r.puuid);
    expect(d.card.name).toMatchObject({ kind: "visible", name: W16, tag: "WWWWW" });
    expect(d.encounters.length).toBeGreaterThan(0);
    expect(d.topTeammates).toHaveLength(3);
    // Task 18 review, Minor 4: the Top 3 hidden variants (spec §6.7) at their widest.
    expect(d.topTeammates.map((t) => t.name.kind)).toEqual(["visible", "remembered", "hiddenAgent"]);
    for (const t of d.topTeammates.slice(0, 2)) expect(t.name).toMatchObject({ name: W16 });
    const live = players("details-longnames");
    expect(d.topTeammates.filter((t) => live.some((c) => c.puuid === t.puuid && c.name?.kind === "visible"))).toHaveLength(1);
    expect(d.topTeammates.filter((t) => live.some((c) => c.puuid === t.puuid))).toHaveLength(1); // the hidden two are off the live screen
    for (const w of ["vandal", "phantom", "operator", "sheriff"] as const) {
      expect(d.loadout[w]).toMatchObject({ skinName: "Protocol 781-A", chromaName: "Sunrise" });
    }
  });

  it("details-partial: per-field loading, one loadout weapon absent and one null", () => {
    const r = routeOf("details-partial", "details");
    const d = detailsOf("details-partial", r.puuid);
    for (const k of ["name", "level", "rank", "peak", "kd", "hsPct", "actWinrate", "actWins", "actGames", "last5"] as const) {
      expect(d.card[k], k).toBeUndefined();
      expect(k in d.card, k).toBe(false);
    }
    expect(d.recent.length).toBeGreaterThan(0);
    const weapons = ["vandal", "phantom", "operator", "sheriff"] as const;
    expect(weapons.filter((w) => !(w in d.loadout))).toHaveLength(1);
    expect(weapons.filter((w) => d.loadout[w] === null)).toHaveLength(1);
  });

  it("details-nostats (R133): every hero value null, null stat lines, unknown map/score/agent/rank, empty Top 3 and loadout", () => {
    const r = routeOf("details-nostats", "details");
    const d = detailsOf("details-nostats", r.puuid);
    const c = d.card;
    for (const k of ["level", "rank", "prevActTier", "peak", "kd", "hsPct", "actWinrate", "actWins", "actGames"] as const) expect(c[k], k).toBeNull();
    expect(c.last5).toEqual([]);
    expect([d.heroKd, d.heroHs, d.heroMatches]).toEqual([null, null, 0]);
    expect(d.recent.length).toBe(4); // Show more is there, and every row's stat line is null (R102)
    for (const m of d.recent) expect([m.kills, m.deaths, m.assists, m.kd, m.acs, m.hsPct]).toEqual([null, null, null, null, null, null]);
    expect(d.recent.some((m) => m.mapId === "" && m.roundsWon === null && m.agentId === null && m.tier === null)).toBe(true);
    expect(d.encounters.some((e) => e.mapId === "" && e.roundsWon === null && e.themTier === null && e.selfTier === null && e.themAgentId === null)).toBe(true);
    expect(d.encounters.every((e) => e.durationMs === null)).toBe(true);
    expect(d.topTeammates).toEqual([]);
    expect(d.loadout).toEqual({ vandal: null, phantom: null, operator: null, sheriff: null });
    // Every row opens a match the scene can serve.
    for (const id of [d.recent[0]!.matchId, ...d.encounters.map((e) => e.matchId)]) expect(SCENES["details-nostats"].matches?.[id]).toBeDefined();
  });

  it("details-nostats: each encounter row agrees with the match it opens (side, result, rounds, map, start) (Task 18 review, Minor 10)", () => {
    const r = routeOf("details-nostats", "details");
    const d = detailsOf("details-nostats", r.puuid);
    expect(d.encounters.map((e) => e.with)).toEqual([false, true]); // one Against, one With
    for (const e of d.encounters) {
      const m = SCENES["details-nostats"].matches![e.matchId] as MatchDetailsView;
      const [us, them] = m.teams as [MatchDetailsView["teams"][0], MatchDetailsView["teams"][0]];
      expect(m.perspective, e.matchId).toBe("self");
      expect(us.players.some((p) => p.isSelf), `${e.matchId}: the user is in team 0`).toBe(true);
      expect(us.players.some((p) => p.isFocus), `${e.matchId}: ${e.with ? "With" : "Against"}`).toBe(e.with);
      expect(them.players.some((p) => p.isFocus), `${e.matchId}: focus on the other team when Against`).toBe(!e.with);
      expect([us.result, us.roundsWon, them.roundsWon], `${e.matchId}: the user's result and score`).toEqual([e.result, e.roundsWon, e.roundsLost]);
      expect(them.result).toBe(({ win: "loss", loss: "win", draw: "draw" } as const)[e.result]);
      expect([m.mapId, m.startMs, m.durationMs], `${e.matchId}: map, start, duration`).toEqual([e.mapId, e.startMs, e.durationMs]);
    }
  });

  it("loadout skins carry chroma, name and tier", () => {
    const d = detailsOf("details", routeOf("details", "details").puuid);
    expect(d.loadout.vandal).toEqual({
      chromaId: "b2a065c0-4632-ccaa-f496-7681dc2a6185",
      skinName: "Reaver",
      chromaName: "Red",
      tierId: "60bca009-4182-7998-dee7-b8a2558dc369",
    });
    expect(d.loadout.operator?.tierId).toBe("411e4a55-4e59-7757-41f0-86a53f101bb5");
  });
});

describe("match scenes", () => {
  const acsDesc = (m: MatchDetailsView) =>
    m.teams.forEach((t) => {
      const a = t.players.map((p) => p.acs ?? -1);
      expect(a).toEqual([...a].sort((x, y) => y - x));
    });

  it("match: focus perspective, halves 7–4 · 6–5, header string, sorted by ACS", () => {
    const r = routeOf("match", "match");
    expect(r.from).toBe("recent");
    expect(r.back).toEqual({ puuid: r.focusPuuid, more: false });
    const m = matchOf("match");
    expect(m.perspective).toBe("focus");
    expect(m.teams).toHaveLength(2);
    const [a, b] = m.teams as [MatchDetailsView["teams"][0], MatchDetailsView["teams"][0]];
    expect(`${a.firstHalf}–${b.firstHalf} · ${a.roundsWon! - a.firstHalf!}–${b.roundsWon! - b.firstHalf!}`).toBe("7–4 · 6–5");
    expect(score(a.roundsWon, b.roundsWon)).toBe("13–9");
    expect(a.result).toBe("win");
    expect(b.result).toBe("loss");
    expect(m.queueId).toBe("competitive");
    expect(`Competitive · ${dateLong(m.startMs)} · ${time12(m.startMs)} · ${durationMin(m.durationMs)}`).toBe(
      "Competitive · Sep 29, 2026 · 9:14 PM · 38 min",
    );
    expect(meta.maps[m.mapId]!.name).toBe("Lotus");
    expect(a.players.filter((p) => p.isFocus)).toHaveLength(1);
    expect(a.players.filter((p) => p.isSelf)).toHaveLength(0);
    acsDesc(m);
  });

  it("match-enc: self perspective with a focus row and a self row", () => {
    const m = matchOf("match-enc");
    expect(routeOf("match-enc", "match").from).toBe("encounter");
    expect(m.perspective).toBe("self");
    const rows = m.teams.flatMap((t) => t.players);
    const self = rows.filter((p) => p.isSelf);
    const focus = rows.filter((p) => p.isFocus);
    expect(self).toHaveLength(1);
    expect(focus).toHaveLength(1);
    expect(self[0]!.puuid).not.toBe(focus[0]!.puuid);
    expect(m.teams[0]!.players.some((p) => p.isSelf)).toBe(true); // team 0 is the user's
    acsDesc(m);
  });

  it("match-hidden has hiddenAgent and remembered rows", () => {
    const kinds = new Set(matchOf("match-hidden").teams.flatMap((t) => t.players.map((p) => p.name.kind)));
    expect(kinds.has("hiddenAgent")).toBe(true);
    expect(kinds.has("remembered")).toBe(true);
  });

  it("match-dm: one team block, no result, no halves", () => {
    const m = matchOf("match-dm");
    expect(m.queueId).toBe("deathmatch");
    expect(m.teams).toHaveLength(1);
    expect(m.teams[0]).toMatchObject({ result: null, roundsWon: null, firstHalf: null });
    expect(m.teams[0]!.players.length).toBeGreaterThan(5);
    acsDesc(m);
  });

  it("match-nostats (R133): no duration, rounds or halves; no-stats rows keep their rank (R81); one lost only its ADR", () => {
    const m = matchOf("match-nostats");
    expect(m.durationMs).toBeNull();
    for (const t of m.teams) expect([t.roundsWon, t.firstHalf]).toEqual([null, null]);
    const rows = m.teams.flatMap((t) => t.players);
    const bare = rows.filter((p) => p.kills === null);
    expect(bare.length).toBeGreaterThanOrEqual(4);
    expect(bare.some((p) => p.tier !== null && p.tier >= 3)).toBe(true); // R81
    expect(bare.some((p) => p.tier === null && p.agentId === null)).toBe(true);
    expect(rows.some((p) => p.kills !== null && p.adr === null)).toBe(true);
    expect(m.teams[0]!.players.find((p) => p.isFocus)!.kills).toBeNull();
    // The encounter rows' matches are the user's own: self perspective with the user's row in team 0.
    const enc = SCENES["match-nostats"].matches!;
    const selfViews = Object.values(enc).filter((v): v is MatchDetailsView => v !== "pending" && v.perspective === "self");
    expect(selfViews).toHaveLength(2);
    for (const v of selfViews) expect(v.teams[0]!.players.some((p) => p.isSelf)).toBe(true);
  });

  it("match-loading never resolves", () => {
    const r = routeOf("match-loading", "match");
    expect(SCENES["match-loading"].matches![r.matchId]).toBe("pending");
  });

  it("match-longnames: 10 W-named rows and the focus player's long team label", () => {
    const m = matchOf("match-longnames");
    const rows = m.teams.flatMap((t) => t.players);
    expect(rows).toHaveLength(10);
    const named = rows.filter((p) => p.name.kind === "visible" || p.name.kind === "remembered");
    expect(named.length).toBeGreaterThanOrEqual(8);
    for (const p of named) expect(p.name).toMatchObject({ name: W16, tag: "WWWWW" });
    expect(rows.some((p) => p.name.kind === "remembered")).toBe(true);
    const focus = rows.find((p) => p.isFocus)!;
    expect(focus.name).toMatchObject({ kind: "visible", name: W16 });
    expect(m.perspective).toBe("focus");
    expect(m.teams[0]!.players.some((p) => p.isFocus)).toBe(true);
  });
});

describe("resolvers (DEV api)", () => {
  it("resolveDetails finds a scene's details and falls back across the registry", async () => {
    const mara = routeOf("details", "details").puuid;
    expect((await resolveDetails("details", mara)).card.puuid).toBe(mara);
    // a roster puuid that this scene does not list explicitly still resolves
    const other = players("lobby1")[0]!.puuid;
    expect((await resolveDetails("lobby1", other)).card.puuid).toBe(other);
    await expect(resolveDetails("details", "00000000-0000-4000-8000-00000000dead")).rejects.toThrow();
  });

  it("resolveMatch serves the scene's match, or builds one from the id", async () => {
    const r = routeOf("match", "match");
    expect((await resolveMatch("match", r.matchId, r.focusPuuid)).matchId).toBe(r.matchId);
    const fallback = await resolveMatch("ingame", r.matchId, r.focusPuuid);
    expect(fallback.matchId).toBe(r.matchId);
    await expect(resolveMatch("ingame", "nonsense", r.focusPuuid)).rejects.toThrow();
  });

  it("resolveMatch follows the scene's live hidden names, never the raw roster name (R93)", async () => {
    const rowOf = (m: MatchDetailsView, puuid: string) => m.teams.flatMap((t) => t.players).find((p) => p.puuid === puuid)!;
    // pregame: Nova is a hidden player who has not locked. Her own recent match must not print `Nova#3341`.
    const nova = PUUID.nova!;
    expect(players("pregame").find((c) => c.puuid === nova)?.name).toEqual({ kind: "hiddenPlayer" });
    // As the backend sends it (matchview.rs `hidden_style`, R93): the row's own agent, never `hiddenPlayer` in a match row.
    const hidden = await resolveMatch("pregame", recentMatchId("nova", 0), nova);
    expect(rowOf(hidden, nova).name).toEqual({ kind: "hiddenAgent", agentId: rowOf(hidden, nova).agentId });
    expect(rowOf(hidden, nova).agentId).toBeTruthy();
    expect(JSON.stringify(hidden)).not.toMatch(/Nova|3341/);
    // The same match where she is visible keeps her name.
    expect(rowOf(await resolveMatch("ingame", recentMatchId("nova", 0), nova), nova).name).toEqual({
      kind: "visible", name: "Nova", tag: "3341",
    });
    // Cypher (hiddenAgent) shows as the agent he played in that match; Rook (remembered) by his remembered name.
    const cypher = PUUID.cypher!;
    const rook = PUUID.rook!;
    const live = (id: string) => players("ingame").find((c) => c.puuid === id)!.name;
    const cy = rowOf(await resolveMatch("ingame", recentMatchId("cypher", 0), cypher), cypher);
    expect(cy.name).toEqual({ kind: "hiddenAgent", agentId: cy.agentId });
    expect(rowOf(await resolveMatch("ingame", recentMatchId("rook", 0), rook), rook).name).toEqual(live(rook));
  });

  it("matchFor / detailsFor apply a hidden live card to every row that names it (R93)", () => {
    const kilo = PUUID.kilo!;
    const live = roster();
    live.find((c) => c.puuid === kilo)!.name = { kind: "hiddenAgent", agentId: "hidden-agent-id" };
    // Kilo is on Mara's team in her recent match, and is her top teammate. A match row takes the agent of that row
    // (matchview.rs `hidden_style`), not the live card's.
    const m = matchFor(recentMatchId("mara", 0), live);
    const kiloRow = m.teams.flatMap((t) => t.players).find((p) => p.puuid === kilo)!;
    expect(kiloRow.name).toEqual({ kind: "hiddenAgent", agentId: kiloRow.agentId });
    expect(kiloRow.agentId).not.toBe("hidden-agent-id");
    expect(JSON.stringify(m)).not.toContain("Kilo");
    const d = detailsFor("mara", undefined, live);
    expect(d.topTeammates[0]).toMatchObject({ puuid: kilo, count: 14, name: { kind: "hiddenAgent", agentId: "hidden-agent-id" } });
    expect(d.topTeammates.slice(1).every((t) => t.name.kind === "visible")).toBe(true);
    // A Deathmatch built from the id follows the live names too (Mara is its focus row).
    live.find((c) => c.puuid === PUUID.mara)!.name = { kind: "hiddenAgent", agentId: "hidden-agent-id" };
    const dm = matchFor("mdemo:mara:dm", live);
    expect(dm.queueId).toBe("deathmatch");
    const focus = dm.teams[0]!.players.find((p) => p.isFocus)!;
    expect(focus.name).toEqual({ kind: "hiddenAgent", agentId: focus.agentId });
    // A remembered live name stays remembered in the match row.
    live.find((c) => c.puuid === kilo)!.name = { kind: "remembered", name: "Kilo", tag: "2210", source: "ownMatch", fromMapId: null, fromStartMs: null };
    expect(matchFor(recentMatchId("mara", 0), live).teams.flatMap((t) => t.players).find((p) => p.puuid === kilo)?.name).toMatchObject({ kind: "remembered" });
    // Without live cards the rows are the roster's own.
    expect(detailsFor("mara").topTeammates[0]!.name).toMatchObject({ kind: "visible", name: "Kilo" });
    expect(JSON.stringify(matchFor(recentMatchId("mara", 0)))).toContain("Kilo");
  });

  it("resolvers return copies: no scene, roster or memoized object is shared", async () => {
    const mara = routeOf("details", "details").puuid;
    const a = await resolveDetails("details", mara);
    const b = await resolveDetails("details", mara);
    expect(a).toEqual(b);
    expect(a).not.toBe(b);
    expect(a.card).not.toBe(b.card);
    expect(a).not.toBe(SCENES.details.details![mara]);
    a.card.level = 1;
    a.recent.pop();
    a.topTeammates[0]!.name = { kind: "hiddenPlayer" };
    expect(b.recent).toHaveLength(8);
    expect((await resolveDetails("details", mara)).topTeammates[0]!.name.kind).toBe("visible");
    expect(detailsOf("details", mara).recent).toHaveLength(8);
    // No two scenes share a details record.
    expect(SCENES.details.details).not.toBe(SCENES["details-more"].details);
    expect(SCENES.details.details![mara]).not.toBe(SCENES["details-more"].details![mara]);

    // The fallback card is a copy of the scene's live card.
    const self = players("lobby1")[0]!;
    const f = await resolveDetails("lobby1", self.puuid);
    expect(f.card).toEqual(self);
    expect(f.card).not.toBe(self);
    f.card.level = 1;
    expect(players("lobby1")[0]!.level).toBe(312);

    // Match views: the scene's own entry and the built ones.
    const r = routeOf("match", "match");
    const own = await resolveMatch("match", r.matchId, r.focusPuuid);
    expect(own).toEqual(matchOf("match"));
    expect(own).not.toBe(matchOf("match"));
    own.teams[0]!.players[0]!.name = { kind: "hiddenPlayer" };
    expect(matchOf("match").teams[0]!.players[0]!.name.kind).not.toBe("hiddenPlayer");

    // Fahad's row in an encounter match is not one shared name object.
    const selfName = (m: MatchDetailsView) => m.teams.flatMap((t) => t.players).find((p) => p.isSelf)!.name;
    const e1 = matchFor(encounterMatchId("mara", 0));
    const e2 = matchFor(encounterMatchId("mara", 0));
    expect(selfName(e1)).toEqual(selfName(e2));
    expect(selfName(e1)).not.toBe(selfName(e2));
  });
});

describe("feed", () => {
  afterEach(() => vi.unstubAllGlobals());

  function stubBrowser(search: string) {
    const listeners: Record<string, (e: unknown) => void> = {};
    const dataset: Record<string, string> = {};
    const replaceState = vi.fn();
    vi.stubGlobal("document", { documentElement: { dataset } });
    vi.stubGlobal("location", { search, href: `http://127.0.0.1:1420/${search}` });
    vi.stubGlobal("history", { replaceState });
    vi.stubGlobal("window", { addEventListener: (ev: string, cb: (e: unknown) => void) => (listeners[ev] = cb) });
    return { listeners, dataset, replaceState };
  }
  const key = (k: string) => ({ key: k, ctrlKey: false, metaKey: false, altKey: false, repeat: false, target: { closest: () => null } });

  it("the demo-feed marker is set by startDemo", async () => {
    vi.resetModules();
    const b = stubBrowser("?scene=ingame");
    const { startDemo } = await import("./demo/feed");
    expect(b.dataset.feed).toBeUndefined();
    const applied: Array<LiveState | null> = [];
    const m = writable<Meta | null>(null);
    startDemo((s) => applied.push(s), m, writable<MyStats | null | undefined>(undefined));
    expect(b.dataset.feed).toBe("demo-feed");
    expect(applied.at(-1)?.stage).toBe("ingame");
    let seen: Meta | null = null;
    m.subscribe((v) => (seen = v))();
    expect(seen).toEqual(demoMeta);
  });

  it("?scene= picks the scene; 'pending' meta stays null; boot applies null", async () => {
    vi.resetModules();
    stubBrowser("?scene=ingame-nometa");
    const { startDemo } = await import("./demo/feed");
    const applied: Array<LiveState | null> = [];
    const m = writable<Meta | null>(null);
    startDemo((s) => applied.push(s), m, writable<MyStats | null | undefined>(undefined));
    expect(applied.at(-1)?.players).toHaveLength(10);
    let seen: Meta | null | "unset" = "unset";
    m.subscribe((v) => (seen = v))();
    expect(seen).toBeNull();

    vi.resetModules();
    stubBrowser("?scene=boot");
    const again = await import("./demo/feed");
    const applied2: Array<LiveState | null> = [];
    again.startDemo((s) => applied2.push(s), writable<Meta | null>(null), writable<MyStats | null | undefined>(undefined));
    expect(applied2).toEqual([null]);
  });

  it("keys 0-8 switch scenes (R12), set the route and update the URL", async () => {
    vi.resetModules();
    const b = stubBrowser("?scene=lobby1");
    const feed = await import("./demo/feed");
    const router = await import("./router.svelte");
    const applied: Array<LiveState | null> = [];
    feed.startDemo((s) => applied.push(s), writable<Meta | null>(null), writable<MyStats | null | undefined>(undefined));
    const press = (k: string) => b.listeners.keydown!(key(k));
    const want: Record<string, [string, number]> = {
      "0": ["closed", 1], "1": ["menus", 1], "2": ["menus", 2], "3": ["menus", 3], "4": ["menus", 4], "5": ["menus", 5],
      "6": ["pregame", 5], "7": ["ingame", 10], "8": ["ingame", 10],
    };
    for (const [k, [stage, n]] of Object.entries(want)) {
      router.go({ name: "details", puuid: "x", more: false });
      press(k);
      expect(applied.at(-1)?.stage, k).toBe(stage);
      expect(applied.at(-1)?.players, k).toHaveLength(n);
      expect(router.router.route, k).toEqual({ name: "live" });
    }
    expect(b.replaceState).toHaveBeenCalled();
    press("9"); // no scene on 9
    expect(applied.at(-1)?.players).toHaveLength(10);
  });

  it("the demo feed hands the scene's data to the store, and an autoRoute scene sets no route", async () => {
    vi.resetModules();
    stubBrowser("?scene=mystats-closed");
    const feed = await import("./demo/feed");
    const router = await import("./router.svelte");
    const store = writable<MyStats | null | undefined>(undefined);
    const applied: Array<LiveState | null> = [];
    feed.startDemo(
      (s) => {
        applied.push(s);
        if (s) router.startPage(s.stage); // what applyLive does with the first LiveState
      },
      writable<Meta | null>(null),
      store,
    );
    let seen: MyStats | null | undefined;
    store.subscribe((v) => (seen = v))();
    expect(seen).toEqual(SCENES["mystats-closed"].mystats);
    expect(router.router.route).toEqual({ name: "mystats" });

    vi.resetModules();
    stubBrowser("?scene=mystats-loading");
    const loading = await import("./demo/feed");
    const s2 = writable<MyStats | null | undefined>(null);
    loading.startDemo(() => {}, writable<Meta | null>(null), s2);
    let seen2: MyStats | null | undefined = null;
    s2.subscribe((v) => (seen2 = v))();
    expect(seen2).toBeUndefined();
  });

  it("routeFromSearch: &route=mystats", async () => {
    const { routeFromSearch } = await import("./demo/feed");
    expect(routeFromSearch(new URLSearchParams("route=mystats"), { name: "live" })).toEqual({ name: "mystats" });
  });

  it("routeFromSearch: &route=details / match override the scene route (spec §8.1)", async () => {
    const { routeFromSearch } = await import("./demo/feed");
    const live = { name: "live" } as const;
    const q = (s: string) => new URLSearchParams(s);
    expect(routeFromSearch(q(""), live)).toEqual(live);
    expect(routeFromSearch(q("route=details&puuid=P"), live)).toEqual({ name: "details", puuid: "P", more: false });
    expect(routeFromSearch(q("route=details&puuid=P&more=1"), live)).toEqual({ name: "details", puuid: "P", more: true });
    expect(routeFromSearch(q("route=match&match=M&puuid=P&from=encounter"), live)).toEqual({
      name: "match",
      matchId: "M",
      focusPuuid: "P",
      from: "encounter",
      back: { puuid: "P", more: false },
    });
    expect(routeFromSearch(q("route=match&match=M&puuid=P"), live)).toMatchObject({ from: "recent" });
    expect(routeFromSearch(q("route=details"), live)).toEqual(live); // no puuid
    expect(routeFromSearch(q("route=match&puuid=P"), live)).toEqual(live); // no match id
    expect(routeFromSearch(q("route=nope"), live)).toEqual(live);
    expect(routeFromSearch(q("route=live"), { name: "details", puuid: "x", more: false })).toEqual(live);
  });
});

// ── My Stats scenes (spec §9.6) ─────────────────────────────────────────────────
describe("My Stats scenes", () => {
  const stats = (s: SceneName): MyStats => {
    const v = SCENES[s].mystats;
    if (!v || v === "pending") throw new Error(`${s}: no My Stats data`);
    return v;
  };
  const sum = (xs: number[]) => xs.reduce((a, b) => a + b, 0);
  /** The meta a scene shows: its own, or the demo's. */
  const meta_ = (n: SceneName): Meta => {
    const m = SCENES[n].meta;
    return m && m !== "pending" ? m : meta;
  };

  it("mystats: about 60 RR points, 7 maps, 8 agents, a session of 15 and 6 insights", () => {
    const m = stats("mystats");
    expect(m.rrSeries).toHaveLength(60);
    expect(m.maps).toHaveLength(7);
    expect(m.agents).toHaveLength(8);
    expect(m.session?.streak).toHaveLength(15);
    expect(m.insights.map((i) => i.kind)).toEqual(["bestMap", "worstMap", "timeOfDay", "weekdayWeekend", "rrEconomy", "last7Days"]);
    expect(m.last5).toHaveLength(5);
    expect(SCENES.mystats.route).toEqual({ name: "mystats" });
  });

  it("every figure agrees with every other: the same 60 matches behind the tables, the weekday and time-of-day cells and the rank", () => {
    const m = stats("mystats");
    expect(sum(m.maps.map((r) => r.games))).toBe(60);
    expect(sum(m.agents.map((r) => r.games))).toBe(60);
    expect(sum(m.byWeekday.map((c) => c.games))).toBe(60);
    expect(sum(m.byDaypart.map((c) => c.games))).toBe(60);
    expect(sum(m.maps.map((r) => r.wins))).toBe(m.rank.wins);
    expect(sum(m.maps.map((r) => r.losses))).toBe(m.rank.losses);
    expect(sum(m.maps.map((r) => r.netRr!))).toBe(m.rank.netRr);
    expect(m.rank.rr! + 100 * (m.rank.tier! - 21)).toBe(m.rrSeries.at(-1)!.absoluteRr - 2100);
    expect(m.rank.rrToNext).toBe(100 - m.rank.rr!);
    // The tables come most played first.
    for (const rows of [m.maps, m.agents]) expect(rows.map((r) => r.games)).toEqual([...rows.map((r) => r.games)].sort((a, b) => b - a));
    expect(m.maps.map((r) => r.winrate)).toEqual(m.maps.map((r) => r.wins / r.games));
  });

  it("the series is oldest first, the changes newest first (at most 5), and each change is a tier step of the series", () => {
    const m = stats("mystats");
    const starts = m.rrSeries.map((p) => p.startMs);
    expect([...starts].sort((a, b) => a - b)).toEqual(starts);
    expect(m.rank.changes.length).toBeGreaterThan(0);
    expect(m.rank.changes.length).toBeLessThanOrEqual(5);
    const t = m.rank.changes.map((c) => c.startMs);
    expect([...t].sort((a, b) => b - a)).toEqual(t);
    for (const c of m.rank.changes) {
      const i = starts.indexOf(c.startMs);
      const [a, b] = [Math.floor(m.rrSeries[i - 1]!.absoluteRr / 100), Math.floor(m.rrSeries[i]!.absoluteRr / 100)];
      expect([c.tier, c.up]).toEqual([b, b > a]);
    }
  });

  it("the session is the run of matches with no 6h gap, ending with the newest; its net RR is theirs", () => {
    const m = stats("mystats");
    const s = m.session!;
    const tail = m.rrSeries.slice(-15);
    expect(s.startedMs).toBe(tail[0]!.startMs);
    expect(m.rrSeries.at(-16)!.startMs).toBeLessThan(s.startedMs - 6 * 3_600_000);
    for (let i = 1; i < tail.length; i++) expect(tail[i]!.startMs - tail[i - 1]!.startMs).toBeLessThan(6 * 3_600_000);
    expect(s.streak.filter((x) => x === "W")).toHaveLength(s.wins);
    expect(s.streak.filter((x) => x === "L")).toHaveLength(s.losses);
    expect(s.netRr).toBe(tail[tail.length - 1]!.absoluteRr - m.rrSeries.at(-16)!.absoluteRr);
  });

  it("the last 5 are the user's own recent rows (the Match page opens the same matches), newest first, with a stat line each", () => {
    const m = stats("mystats");
    expect(m.last5.map((r) => r.matchId)).toEqual([0, 1, 2, 3, 4].map((i) => recentMatchId("fahad", i)));
    expect(m.last5.map((r) => r.result)).toEqual(["win", "loss", "win", "win", "loss"]);
    for (const r of m.last5) {
      expect(r.kills).not.toBeNull();
      expect(matchFor(r.matchId).teams[0]!.players.some((p) => p.isSelf)).toBe(true);
    }
  });

  it("every map and agent id exists in the meta the scene uses; the longest-names scene names each only once", () => {
    for (const n of NAMES.filter((x) => x.startsWith("mystats"))) {
      const s = SCENES[n];
      if (!s.mystats || s.mystats === "pending") continue;
      const m = meta_(n);
      const ids = (kind: "maps" | "agents", xs: string[]) => xs.forEach((id) => expect(m[kind][id], `${n}: ${kind} ${id}`).toBeDefined());
      ids("maps", [...s.mystats.maps.map((r) => r.id), ...s.mystats.last5.map((r) => r.mapId)]);
      ids("agents", [...s.mystats.agents.map((r) => r.id), ...s.mystats.last5.flatMap((r) => (r.agentId ? [r.agentId] : []))]);
    }
    const m = meta_("mystats-longnames");
    for (const [kind, rows] of [["maps", stats("mystats-longnames").maps], ["agents", stats("mystats-longnames").agents]] as const) {
      const names = rows.map((r) => m[kind][r.id]!.name);
      expect(new Set(names).size, `${kind} names are unique`).toBe(names.length);
    }
    expect(meta_("mystats-longnames").maps[stats("mystats-longnames").maps[0]!.id]!.name).toBe("Basic Training");
  });

  it("mystats-new answers null, mystats-loading never answers (the skeleton), the others carry data", async () => {
    expect(SCENES["mystats-new"].mystats).toBeNull();
    expect(SCENES["mystats-loading"].mystats).toBe("pending");
    expect(await resolveMyStats("mystats-new")).toBeNull();
    const pending = vi.fn();
    void resolveMyStats("mystats-loading").then(pending);
    await new Promise((r) => setTimeout(r, 20));
    expect(pending).not.toHaveBeenCalled();
  });

  it("mystats-unranked: no rank, no RR anywhere (null, never a counted 0), nothing to chart, no insight", () => {
    const m = stats("mystats-unranked");
    expect(m.rank).toMatchObject({ tier: null, rr: null, rrToNext: null, netRr: null, avgWin: null, avgLoss: null, changes: [] });
    expect(m.rrSeries).toEqual([]);
    expect(m.insights).toEqual([]);
    expect(m.session?.netRr).toBeNull();
    for (const r of [...m.last5]) expect([r.rrEarned, r.tier]).toEqual([null, null]);
    for (const r of [...m.maps, ...m.agents]) expect(r.netRr).toBeNull();
  });

  it("mystats-immortal has no next tier and chart values from 2400 up; mystats-rrnull has a tier and no RR", () => {
    const imm = stats("mystats-immortal");
    expect([imm.rank.tier, imm.rank.rr, imm.rank.rrToNext]).toEqual([24, 87, null]);
    expect(imm.rrSeries.at(-1)!.absoluteRr).toBe(2400 + 87);
    const nul = stats("mystats-rrnull");
    expect([nul.rank.tier, nul.rank.rr, nul.rank.rrToNext]).toEqual([21, null, null]);
    expect(nul.rrSeries).toEqual(stats("mystats").rrSeries);
  });

  it("mystats-longnames holds the widest numbers: 3-digit games, net RR in the hundreds, a session of 30", () => {
    const m = stats("mystats-longnames");
    expect(Math.max(...m.maps.map((r) => r.games))).toBeGreaterThanOrEqual(100);
    expect(Math.min(...m.maps.map((r) => r.netRr!))).toBeLessThanOrEqual(-300);
    expect(m.session?.streak).toHaveLength(30);
    expect(m.session?.streak).toContain("D");
    expect(m.rank.wins! + m.rank.losses!).toBeGreaterThan(300);
  });

  it("mystats-closed: VALORANT closed at startup, and no route is set for the scene (the app's start page decides)", () => {
    expect(SCENES["mystats-closed"].live?.stage).toBe("closed");
    expect(SCENES["mystats-closed"].autoRoute).toBe(true);
    expect(stats("mystats-closed")).toEqual(stats("mystats"));
  });

  it("a scene without My Stats data of its own answers the full demo, so the tab has content in every scene", async () => {
    expect(SCENES.ingame.mystats).toBeUndefined();
    expect(await resolveMyStats("ingame")).toEqual(stats("mystats"));
    expect(await resolveMyStats(null)).toEqual(stats("mystats"));
  });

  it("the answer is a copy: mutating it never reaches the scene", async () => {
    const a = (await resolveMyStats("mystats"))!;
    a.rank.netRr = 99999;
    expect(stats("mystats").rank.netRr).not.toBe(99999);
  });
});

describe("hygiene", () => {
  it("no scene leaks a hidden player's real name (R96): hidden cards carry no name text", () => {
    for (const n of NAMES) {
      for (const c of players(n)) {
        if (c.name?.kind === "hiddenPlayer") expect(JSON.stringify(c.name)).toBe('{"kind":"hiddenPlayer"}');
        if (c.name?.kind === "hiddenAgent") expect(Object.keys(c.name).sort()).toEqual(["agentId", "kind"]);
      }
    }
    expect(nameOf(players("ingame")[0]!)).toMatchObject({ kind: "visible" });
  });

  it("the demo roster names live only in demo/cdv8.ts (they must never reach dist)", () => {
    const dir = new URL("./", import.meta.url);
    // Real file paths (fileURLToPath), never a URL's pathname: that is `/D:/…` on Windows, and fs cannot open it.
    const walk = (u: URL): string[] =>
      readdirSync(u, { withFileTypes: true }).flatMap((e) =>
        e.isDirectory() ? walk(new URL(`${e.name}/`, u)) : [fileURLToPath(new URL(e.name, u))],
      );
    const hits = walk(new URL("../", dir))
      .filter((f) => /\.(ts|svelte)$/.test(f) && !f.endsWith(".test.ts"))
      .filter((f) => /VesperUnbreakabl|Vantablack/.test(readFileSync(f, "utf8")));
    expect(hits.map((h) => basename(h))).toEqual(["cdv8.ts"]);
  });
});
