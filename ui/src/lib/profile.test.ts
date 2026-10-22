import { describe, expect, it } from "vitest";
import {
  agoLong,
  avgAcs,
  encounterView,
  exactStamp,
  liveMateChips,
  loadoutTile,
  mapRows,
  nameView,
  partyChip,
  peakCell,
  prevCell,
  rankCell,
  recentRow,
  relStamp,
  statCells,
} from "./profile";
import type { EncounterDetailRow, LiveState, MatchSummary, Meta, PlayerCard, PlayerDetails } from "./types";

const META: Meta = {
  agents: { jett: { name: "Jett" }, sova: { name: "Sova" }, cypher: { name: "Cypher" } },
  maps: { lotus: { name: "Lotus" } },
  tiers: {},
};

const NOW = Date.UTC(2026, 9, 22, 22, 40);
const MIN = 60_000;
const DAY = 86_400_000;

function card(over: Partial<PlayerCard> = {}): PlayerCard {
  return {
    puuid: "p1",
    name: { kind: "visible", name: "Mara", tag: "7788" },
    level: 402,
    cardId: "c",
    agentId: "jett",
    rank: { tier: 21, rr: 30 },
    prevActTier: 20,
    peak: null,
    kd: 1.2,
    hsPct: 0.25,
    actWinrate: 0.6,
    actWins: 6,
    actGames: 10,
    last5: [],
    party: null,
    encounters: null,
    team: "enemy",
    isSelfParty: false,
    isSelf: false,
    ...over,
  };
}

function match(over: Partial<MatchSummary> = {}): MatchSummary {
  return {
    matchId: "m1",
    mapId: "lotus",
    startMs: NOW - 3 * 60 * MIN,
    result: "win",
    roundsWon: 13,
    roundsLost: 9,
    agentId: "jett",
    kills: 20,
    deaths: 12,
    assists: 4,
    kd: 1.67,
    acs: 250,
    hsPct: 0.3,
    rrEarned: 18,
    tier: 21,
    ...over,
  };
}

function details(over: Partial<PlayerDetails> = {}): PlayerDetails {
  return {
    card: card(),
    recent: [match(), match({ matchId: "m2", acs: 210 }), match({ matchId: "m3", acs: null })],
    topTeammates: [],
    loadout: {},
    encounters: [],
    heroKd: 1.52,
    heroHs: 0.29,
    heroMatches: 14,
    ...over,
  };
}

describe("nameView (hidden-name rules)", () => {
  it("shows a visible Riot ID with its tag", () => {
    expect(nameView({ kind: "visible", name: "Mara", tag: "7788" }, META)).toMatchObject({ text: "Mara", tag: "#7788", style: "visible" });
  });
  it("never shows a tag for a hidden player, and names a locked agent", () => {
    expect(nameView({ kind: "hiddenPlayer" }, META)).toMatchObject({ text: "Hidden Player", tag: "", style: "hidden", ref: "this player" });
    expect(nameView({ kind: "hiddenAgent", agentId: "cypher" }, META)).toMatchObject({ text: "Cypher", tag: "", style: "hidden" });
    expect(nameView({ kind: "hiddenAgent", agentId: "cypher" }, null)?.text).toBe("Hidden Player");
  });
  it("draws a remembered name in its own style with the source tooltip", () => {
    const v = nameView({ kind: "remembered", name: "Rook", tag: "4413", source: "seen", fromMapId: null, fromStartMs: null }, META);
    expect(v).toMatchObject({ text: "Rook", tag: "#4413", style: "remembered", tip: "Hidden name · seen before" });
  });
  it("is undefined while loading and — when missing", () => {
    expect(nameView(undefined, META)).toBeUndefined();
    expect(nameView(null, META)?.text).toBe("—");
  });
});

describe("dates", () => {
  it("relStamp follows the prototype's stamp", () => {
    expect(relStamp(NOW - 12 * MIN, NOW)).toBe("12 min ago");
    expect(relStamp(NOW - 3 * 60 * MIN, NOW)).toBe("3 h ago");
    expect(relStamp(NOW - DAY, NOW)).toBe("Yesterday");
    expect(relStamp(NOW - 5 * DAY, NOW)).toBe("5 days ago");
    expect(relStamp(Date.UTC(2026, 8, 13, 12), NOW)).toBe("Sep 13");
  });
  it("exactStamp writes the weekday, date and clock", () => {
    expect(exactStamp(NOW)).toBe("Thu, 22 Oct 2026 · 10:40 PM");
  });
  it("agoLong spells the time out", () => {
    expect(agoLong(35 * DAY)).toBe("1 month ago");
    expect(agoLong(155 * DAY)).toBe("5 months ago");
    expect(agoLong(420 * DAY)).toBe("1 year ago");
    expect(agoLong(3 * DAY)).toBe("3 days ago");
  });
});

describe("rank card", () => {
  it("adds the RR to the next tier below Immortal", () => {
    expect(rankCell({ tier: 21, rr: 30 }, null)).toMatchObject({ name: "Ascendant 1", sub: "30 RR · 70 to Ascendant 2" });
  });
  it("has no fixed next step from Immortal up, and — RR when unknown", () => {
    expect(rankCell({ tier: 24, rr: 19 }, null)?.sub).toBe("19 RR");
    expect(rankCell({ tier: 21, rr: null }, null)?.sub).toBe("— RR");
  });
  it("tri-state: loading, missing, unranked", () => {
    expect(rankCell(undefined, null)).toBeUndefined();
    expect(rankCell(null, null)).toMatchObject({ name: "—", icon: null });
    expect(rankCell({ tier: 0, rr: 0 }, null)).toMatchObject({ name: "Unranked", icon: 0 });
  });
  it("peak: this act, or the act and the time since it ended", () => {
    const base = { tier: 24, actLabel: "V26 A4", actName: "V26 ACT IV", actStartMs: 0, actEndMs: 1 };
    expect(peakCell({ ...base, sinceActEndMs: null }, null)?.sub).toBe("Reached this act");
    expect(peakCell({ ...base, sinceActEndMs: 35 * DAY }, null)?.sub).toBe("V26 A4 · 1 month ago");
  });
  it("last act: final rank, unranked, or —", () => {
    expect(prevCell(22, null)).toMatchObject({ name: "Ascendant 2", sub: "Final rank" });
    expect(prevCell(0, null)).toMatchObject({ name: "Unranked", sub: "No ranked games" });
    expect(prevCell(null, null)).toMatchObject({ name: "—", sub: "" });
  });
});

describe("statCells", () => {
  it("names each stat's scope in its tooltip", () => {
    const s = statCells(card(), details(), { highlight: true });
    expect(s.map((x) => x.label)).toEqual(["K/D", "Headshot %", "Win rate", "Record", "Avg ACS"]);
    expect(s.map((x) => x.value)).toEqual(["1.52", "29%", "60%", "6–4", "230"]);
    expect(s[0]!.tip).toBe("Kills per death\nLast 14 competitive · this act");
    expect(s[2]!.tip).toBe("6 wins in 10 games\nThis act · competitive");
    expect(s[4]!.tip).toBe("Average combat score per match\nLast 2 competitive");
    expect(s[0]!.color).toBe("var(--text-hi)");
  });
  it("skeletons the details stats while loading; omits Avg ACS when no row has one", () => {
    expect(statCells(card(), null, { highlight: true }).map((x) => x.value)).toEqual([undefined, undefined, "60%", "6–4", undefined]);
    const none = statCells(card(), details({ recent: [match({ acs: null })] }), { highlight: true });
    expect(none.map((x) => x.label)).not.toContain("Avg ACS");
  });
  it("adds Net RR for the user only", () => {
    const self = statCells(card({ isSelf: true }), details(), { highlight: true, netRr: 293, netGames: 59 });
    expect(self.at(-1)).toMatchObject({ label: "Net RR", value: "+293", color: "var(--win)" });
    expect(statCells(card({ isSelf: true }), details(), { highlight: true, netRr: undefined }).at(-1)?.value).toBeUndefined();
    expect(statCells(card({ isSelf: true }), details(), { highlight: true, netRr: null }).at(-1)?.value).toBe("—");
  });
  it("draws — for unknown values", () => {
    const s = statCells(card({ actWinrate: null, actWins: null, actGames: null }), details({ heroKd: null, heroHs: null }), { highlight: true });
    expect(s.slice(0, 4).map((x) => x.value)).toEqual(["—", "—", "—", "—"]);
    expect(s[0]!.tip).toBe("");
  });
});

describe("partyChip", () => {
  const live = (players: PlayerCard[]): LiveState => ({ stage: "ingame", mapId: null, players, settled: true });
  it("lists the other live members, hidden names kept hidden", () => {
    const a = card({ puuid: "a", party: { group: 1, confirmed: false } });
    const b = card({ puuid: "b", agentId: "sova", name: { kind: "hiddenAgent", agentId: "sova" }, party: { group: 1, confirmed: false } });
    const c = card({ puuid: "c", party: { group: 2, confirmed: true } });
    const chip = partyChip(a, live([a, b, c]), META)!;
    expect(chip.mates.map((m) => m.puuid)).toEqual(["b"]);
    expect(chip.tip).toBe("Queued together in this match: Mara, Sova\nParty · likely");
    expect(chip.color).toBe("#f0c75e");
  });
  it("never marks the user or their own party", () => {
    const a = card({ puuid: "a", isSelfParty: true, party: { group: 0, confirmed: true } });
    expect(partyChip(a, live([a]), META)).toBeNull();
    expect(partyChip(card({ party: null }), live([]), META)).toBeNull();
  });
});

describe("encounterView", () => {
  const row: EncounterDetailRow = {
    matchId: "e1",
    mapId: "lotus",
    startMs: NOW - 5 * DAY,
    with: false,
    result: "loss",
    roundsWon: 10,
    roundsLost: 13,
    durationMs: 38 * MIN,
    themAgentId: "jett",
    themTier: 24,
    selfAgentId: "sova",
    selfTier: 21,
  };
  it("reads from the user's side, the other player's score from theirs", () => {
    const v = encounterView(row, nameView({ kind: "visible", name: "Mara", tag: "7788" }, META), META, NOW);
    expect(v).toMatchObject({ map: "Lotus", res: "L", with: false, date: "5 days ago" });
    expect(v.self).toMatchObject({ score: "10", scoreColor: "var(--loss-text)", tip: "You · Sova · Ascendant 1" });
    expect(v.them).toMatchObject({ score: "13", scoreColor: "var(--win-text)", tip: "Mara · Jett · Immortal 1" });
    expect(v.dateTip).toBe("Sat, 17 Oct 2026 · 10:40 PM · 38 min");
    expect(v.aria).toBe("Lotus, lost 10 to 13. Open scoreboard");
  });
  it("names a hidden player `This player`", () => {
    expect(encounterView({ ...row, with: true }, nameView({ kind: "hiddenPlayer" }, META), META, NOW).them.tip).toBe("This player · Jett · Immortal 1");
  });
});

describe("mapRows / loadoutTile / avgAcs", () => {
  it("colours the bar by win rate", () => {
    const r = mapRows([{ id: "lotus", games: 10, wins: 6, losses: 4, winrate: 0.6, netRr: 24, kd: 1, acs: 200 }], META)[0]!;
    expect(r).toMatchObject({ map: "Lotus", wr: 60, record: "6–4", barColor: "var(--win)" });
    expect(r.tip).toBe("Lotus: 6 wins, 4 losses · +24 RR\nThis act · competitive");
  });
  it("loadout tri-state and the edition", () => {
    expect(loadoutTile("Vandal", undefined).state).toBe("loading");
    expect(loadoutTile("Vandal", null).state).toBe("none");
    const t = loadoutTile("Vandal", { chromaId: "x", skinName: "Reaver", chromaName: null, tierId: "60bca009-4182-7998-dee7-b8a2558dc369" });
    expect(t).toMatchObject({ state: "skin", skin: "Reaver Vandal", edition: "Premium" });
    expect(loadoutTile("Sheriff", { chromaId: "y", skinName: "Standard", chromaName: null, tierId: null })).toMatchObject({
      skin: "Standard Sheriff",
      edition: null,
    });
  });
  it("averages the known ACS values", () => {
    expect(avgAcs([match({ acs: 200 }), match({ acs: 300 }), match({ acs: null })])).toEqual({ acs: 250, n: 2 });
    expect(avgAcs([match({ acs: null })])).toBeNull();
  });
});

describe("recent rows", () => {
  it("formats a row", () => {
    const r = recentRow(match({ rrEarned: -15, result: "loss", roundsWon: 9, roundsLost: 13 }), META, NOW);
    expect(r).toMatchObject({ res: "L", score: "9–13", map: "Lotus", when: "3 h ago", kda: "20 / 12 / 4", rr: "−15", rankShort: "Asc 1" });
    expect(recentRow(match({ kills: null, deaths: null, assists: null, tier: null, rrEarned: null }), META, NOW)).toMatchObject({ kda: "—", rankShort: "—", rr: "—" });
  });
  it("in-your-match chips come from the live cards, never revealing a hidden name", () => {
    const live: LiveState = {
      stage: "ingame",
      mapId: null,
      settled: true,
      players: [
        card({ puuid: "x", team: "ally", agentId: "sova", name: { kind: "hiddenAgent", agentId: "sova" } }),
        card({ puuid: "y", team: "enemy", name: { kind: "visible", name: "Kilo", tag: "2210" } }),
      ],
    };
    const extras = { puuid: "p1", maps: [], liveMates: [{ matchId: "m1", puuids: ["x", "y", "gone", "p1"] }] };
    const chips = liveMateChips("m1", extras, live, "p1", META);
    expect(chips.map((c) => c.name)).toEqual(["Sova", "Kilo"]);
    expect(chips[0]).toMatchObject({ hidden: true, tip: "Hidden name · Sova in your match now\nYour team now" });
    expect(chips[1]!.tip).toBe("Kilo#2210 · Jett in your match now\nEnemy team now");
    expect(liveMateChips("m2", extras, live, "p1", META)).toEqual([]);
  });
});
