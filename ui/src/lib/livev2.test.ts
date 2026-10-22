import { describe, expect, it } from "vitest";
import {
  agoCaps,
  boardTeams,
  byAcs,
  cardLoaded,
  clock,
  countdown,
  durationCaps,
  loadingProgress,
  matchup,
  nameView,
  nextTierText,
  peakOf,
  plainName,
  rankOf,
  rankRrText,
  recapModel,
  resultScore,
  rowView,
  savedCallout,
  sessionChip,
  statsLoading,
  streakOf,
  teamTier,
  teamWinrate,
  waitChecks,
  type RowCtx,
} from "./livev2";
import type { AppStatus, Last5Entry, LiveState, MatchDetailsView, Meta, MyStats, PlayerCard, ScoreRow } from "./types";

const META: Meta = {
  agents: { jett: { name: "Jett" }, sova: { name: "Sova" } },
  maps: { haven: { name: "Haven" } },
  tiers: { "21": { name: "ASCENDANT 1", color: "" }, "22": { name: "ASCENDANT 2", color: "" }, "24": { name: "IMMORTAL 1", color: "" } },
};

const e = (result: Last5Entry["result"]): Last5Entry => ({ result, rrEarned: null, startMs: 0 });

function card(over: Partial<PlayerCard> = {}): PlayerCard {
  return {
    puuid: "p1",
    name: { kind: "visible", name: "Nova", tag: "3341" },
    level: 120,
    cardId: "c",
    agentId: "jett",
    rank: { tier: 21, rr: 40 },
    prevActTier: 20,
    peak: { tier: 24, actLabel: "V26 A4", actName: "V26 ACT IV", sinceActEndMs: 35 * 86_400_000, actStartMs: Date.UTC(2026, 5, 24), actEndMs: Date.UTC(2026, 7, 26) },
    kd: 1.31,
    hsPct: 0.27,
    actWinrate: 0.61,
    actWins: 36,
    actGames: 59,
    last5: [e("win"), e("win"), e("loss")],
    party: null,
    encounters: null,
    team: "ally",
    isSelfParty: false,
    isSelf: false,
    ...over,
  };
}

const ctx = (over: Partial<RowCtx> = {}): RowCtx => ({
  meta: META,
  lobby: [],
  settings: { highlight: true, smurf: "cautious" },
  marks: { saved: [], notes: {}, notSmurf: [] },
  selected: null,
  stage: "ingame",
  ...over,
});

const live = (stage: LiveState["stage"], players: PlayerCard[], mapId: string | null = "haven"): LiveState => ({ stage, mapId, players, settled: true });

describe("names (hidden-name rules)", () => {
  it("visible: name + tag, tooltip only when cut", () => {
    const v = nameView({ kind: "visible", name: "Nova", tag: "3341" }, META);
    expect(v).toMatchObject({ style: "visible", text: "Nova", tag: "#3341", tip: { text: "Nova#3341", overflowOnly: true } });
  });

  it("hidden player before the lock, the agent name after it; never a tag", () => {
    expect(nameView({ kind: "hiddenPlayer" }, META)).toMatchObject({ style: "hidden", text: "Hidden player", tag: "" });
    expect(nameView({ kind: "hiddenAgent", agentId: "sova" }, META)).toMatchObject({ style: "hidden", text: "Sova", tag: "" });
  });

  it("remembered: the tooltip names the source, the map and the date; the full Riot ID joins it when cut", () => {
    const v = nameView(
      { kind: "remembered", name: "Rook", tag: "4413", source: "ownMatch", fromMapId: "haven", fromStartMs: Date.UTC(2026, 8, 12, 12) },
      META,
    );
    expect(v.style).toBe("remembered");
    expect(v.tip).toEqual({ text: "Hidden name · known from your match\nHaven · Sep 12, 2026", ifCut: "Rook#4413" });
  });

  it("loading and missing", () => {
    expect(nameView(undefined, META).style).toBe("loading");
    expect(nameView(null, META)).toMatchObject({ style: "missing", text: "—" });
    expect(plainName(undefined, META)).toBe("Player");
  });
});

describe("row fields", () => {
  it("streak: two or more of the same newest results; a draw ends it", () => {
    expect(streakOf([e("win"), e("win"), e("loss")])).toMatchObject({ n: 2, win: true, tip: "Won the last 2 matches\nLast 5, oldest first: L W W" });
    expect(streakOf([e("loss"), e("loss"), e("loss")])?.win).toBe(false);
    expect(streakOf([e("win"), e("loss")])).toBeNull();
    expect(streakOf([e("draw"), e("draw")])).toBeNull();
    expect(streakOf(undefined)).toBeNull();
  });

  it("peak chip only when the peak is above the current rank", () => {
    const p = peakOf(card(), META)!;
    expect(p.delta).toBe(3);
    expect(p.tip.split("\n")[0]).toBe("Immortal 1");
    expect(p.tip.split("\n")[1]).toBe("V26 A4 · 1mo ago");
    expect(peakOf(card({ peak: { ...card().peak!, tier: 21 } }), META)).toBeNull();
    expect(peakOf(card({ rank: undefined }), META)).toBeNull();
  });

  it("rank: loading / missing / RR unknown", () => {
    expect(rankOf(card({ rank: undefined }), META).state).toBe("loading");
    expect(rankOf(card({ rank: null }), META)).toMatchObject({ state: "missing", rrText: "— RR" });
    expect(rankOf(card({ rank: { tier: 21, rr: null } }), META)).toMatchObject({ state: "ranked", name: "Ascendant 1", rrText: "— RR", rrPct: 0 });
    expect(rankOf(card({ rank: { tier: 24, rr: 230 } }), META).rrPct).toBe(100);
  });

  it("maps a card onto the prototype's row", () => {
    const r = rowView(card(), 0, [card()], ctx());
    expect(r.agent).toBe("Jett");
    expect(r.level).toBe("120");
    expect(r.wr).toMatchObject({ text: "61%", w: 36, l: 23 });
    expect(r.games).toBe("59 games");
    expect(r.kd?.text).toBe("1.31");
    expect(r.hs?.text).toBe("27%");
    expect(r.aria).toBe("Nova, Jett, Ascendant 1. Open profile");
    expect(r.statsLoading).toBe(false);
  });

  it("highlight colours follow the setting", () => {
    const on = rowView(card(), 0, [card()], ctx());
    const off = rowView(card(), 0, [card()], ctx({ settings: { highlight: false, smurf: "cautious" } }));
    expect(on.wr && on.wr.color).toBe("#ffffff");
    expect(off.wr && off.wr.color).toBe("#d6d7da");
  });

  it("picking: no agent before the lock", () => {
    const r = rowView(card({ agentId: null }), 0, [], ctx({ stage: "pregame" }));
    expect(r.picking).toBe(true);
    expect(r.agent).toBe("Picking…");
    expect(r.agentBorder).toContain("dashed");
  });

  it("hidden level shows —; absent stays loading", () => {
    expect(rowView(card({ level: null }), 0, [], ctx()).level).toBe("—");
    expect(rowView(card({ level: undefined }), 0, [], ctx()).level).toBeUndefined();
  });

  it("party: linked to neighbours of the same group, never on the user's own party", () => {
    const a = card({ puuid: "a", party: { group: 1, confirmed: true } });
    const b = card({ puuid: "b", name: { kind: "hiddenAgent", agentId: "sova" }, party: { group: 1, confirmed: true } });
    const c = card({ puuid: "c", party: { group: 1, confirmed: true }, isSelfParty: true });
    const list = [a, b, c];
    const ra = rowView(a, 0, list, ctx());
    const rb = rowView(b, 1, list, ctx());
    expect(ra.party).toMatchObject({ top: false, bottom: true, tip: "Queued together\nNova, Sova" });
    expect(rb.party).toMatchObject({ top: true, bottom: false });
    expect(rowView(c, 2, list, ctx()).party).toBeNull();
    expect(rowView(card({ party: { group: 0, confirmed: false } }), 0, [], ctx()).party?.tip.startsWith("Probably")).toBe(true);
  });

  it("encounters badge (key/value tooltip), saved bookmark and note", () => {
    const c = card({ encounters: { with: 1, against: 3 } });
    const r = rowView(c, 0, [c], ctx({ marks: { saved: ["p1"], notes: { p1: "Entry every round" }, notSmurf: [] } }));
    expect(r.met?.tip).toBe("Played before\nWith you|1\nAgainst you|3");
    expect(r.saved).toBe(true);
    expect(r.note).toBe("Entry every round");
    expect(rowView(card({ isSelf: true, encounters: { with: 2, against: 0 } }), 0, [], ctx()).met).toBeNull();
  });

  it("a note shows only for a saved player", () => {
    const r = rowView(card(), 0, [], ctx({ marks: { saved: [], notes: { p1: "x" }, notSmurf: [] } }));
    expect(r.note).toBe("");
  });

  it("smurf chip with its evidence, unless dismissed", () => {
    const s = card({ puuid: "s", level: 20, hsPct: 0.45, actWinrate: 0.85, actWins: 11, actGames: 13 });
    const others = [card({ puuid: "o1", level: 300, hsPct: 0.2 }), card({ puuid: "o2", level: 250, hsPct: 0.22 })];
    const lobby = [s, ...others];
    const r = rowView(s, 0, lobby, ctx({ lobby }));
    expect(r.smurf?.split("\n")[0]).toBe("Automated signal, not confirmed");
    expect(r.subColor).toBe("var(--warn-text)");
    expect(rowView(s, 0, lobby, ctx({ lobby, marks: { saved: [], notes: {}, notSmurf: ["s"] } })).smurf).toBeNull();
  });

  it("selected row", () => {
    expect(rowView(card(), 0, [], ctx({ selected: "p1" })).selected).toBe(true);
  });

  it("stats loading vs loaded", () => {
    const loading = card({ rank: undefined, actWinrate: undefined, kd: undefined, hsPct: undefined });
    expect(statsLoading(loading)).toBe(true);
    expect(cardLoaded(loading)).toBe(false);
    expect(statsLoading(card({ rank: undefined }))).toBe(false);
    expect(cardLoaded(card())).toBe(true);
  });
});

describe("board", () => {
  const ally = [card({ puuid: "a1" }), card({ puuid: "me", isSelf: true }), card({ puuid: "a2", actWinrate: 0.41 })];
  const enemy = [card({ puuid: "e1", team: "enemy", actWinrate: 0.5 })];

  it("Agent Select shows your team only; In Game both, enemy first on request; the user first", () => {
    expect(boardTeams(live("pregame", ally), false).map((t) => t.team)).toEqual(["ally"]);
    expect(boardTeams(live("ingame", [...ally, ...enemy]), false).map((t) => t.team)).toEqual(["ally", "enemy"]);
    expect(boardTeams(live("ingame", [...ally, ...enemy]), true).map((t) => t.team)).toEqual(["enemy", "ally"]);
    expect(boardTeams(live("ingame", ally), false)[0]!.cards[0]!.puuid).toBe("me");
  });

  it("an empty roster draws 5 placeholder rows per team", () => {
    expect(boardTeams(live("ingame", []), false).map((t) => t.placeholders)).toEqual([5, 5]);
  });

  it("team win rate and average tier", () => {
    expect(teamWinrate(ally)).toBeCloseTo((0.61 + 0.61 + 0.41) / 3);
    expect(teamWinrate([card({ actWinrate: null })])).toBeNull();
    expect(teamTier([card({ rank: { tier: 21, rr: 0 } }), card({ rank: { tier: 22, rr: 0 } })])).toBe(22);
    expect(teamTier([card({ rank: null })])).toBeNull();
  });

  it("matchup: Agent Select hides the enemy side", () => {
    const m = matchup(live("pregame", ally));
    expect(m[1]).toMatchObject({ wr: "?", hidden: true });
    expect(matchup(live("ingame", [...ally, ...enemy]))[1]).toMatchObject({ wr: "50%", hidden: false });
  });

  it("loading progress counts complete cards out of 10 (In Game) or 5 (Agent Select)", () => {
    expect(loadingProgress(live("ingame", []))).toEqual({ loaded: 0, total: 10 });
    expect(loadingProgress(live("ingame", [...ally, card({ puuid: "x", kd: undefined })]))).toEqual({ loaded: 3, total: 10 });
    const five = [1, 2, 3, 4, 5].map((i) => card({ puuid: `p${i}` }));
    expect(loadingProgress(live("pregame", five))).toBeNull();
  });

  it("clock and countdown", () => {
    expect(clock(84_000)).toBe("1:24");
    expect(countdown(41_200)).toBe("0:42");
    expect(countdown(-5)).toBe("0:00");
  });
});

describe("waiting", () => {
  const status = { riotClientVersion: "98.0.1" } as AppStatus;

  it("checks follow the stage", () => {
    expect(waitChecks(status, "menus").map((c) => c.value)).toEqual(["RUNNING · 98.0.1", "IN MENUS", "READY"]);
    expect(waitChecks(null, "closed").map((c) => [c.value, c.ok])).toEqual([
      ["RUNNING", true],
      ["NOT RUNNING", false],
      ["WAITING FOR GAME", false],
    ]);
  });

  it("caps labels", () => {
    expect(agoCaps(45 * 60_000)).toBe("45 MIN AGO");
    expect(agoCaps(3 * 3_600_000)).toBe("3 H AGO");
    expect(durationCaps((3 * 60 + 48) * 60_000)).toBe("3 H 48 MIN");
    expect(durationCaps(25 * 60_000)).toBe("25 MIN");
    expect(resultScore({ result: "win", roundsWon: 13, roundsLost: 9 })).toBe("W 13–9");
    expect(rankRrText(21, 21)).toBe("21 / 100 RR");
    expect(rankRrText(25, 312)).toBe("312 RR");
    expect(rankRrText(21, null)).toBe("— RR");
  });

  it("session chip: the letter, `Map +RR` on hover", () => {
    expect(sessionChip({ matchId: "m", mapId: "haven", rrEarned: 18, startMs: 0, result: "W" }, META)).toMatchObject({ letter: "W", tip: "Haven +18" });
    expect(sessionChip({ matchId: "m", mapId: null, rrEarned: -16, startMs: 0, result: "L" }, META).tip).toBe("−16");
  });
});

describe("recap", () => {
  const row = (o: Partial<ScoreRow>): ScoreRow => ({
    puuid: "x",
    name: { kind: "visible", name: "X", tag: "1" },
    agentId: "jett",
    tier: 21,
    kills: 10,
    deaths: 10,
    assists: 5,
    acs: 200,
    adr: 130,
    hsPct: 0.2,
    isSelf: false,
    isFocus: false,
    ...o,
  });
  const view: MatchDetailsView = {
    matchId: "m1",
    mapId: "haven",
    queueId: "competitive",
    startMs: 0,
    durationMs: 38 * 60_000,
    perspective: "self",
    teams: [
      { teamId: "Blue", result: "win", roundsWon: 13, firstHalf: 7, players: [row({ puuid: "me", isSelf: true, acs: 286, kills: 24, deaths: 14, assists: 5, adr: 168, hsPct: 0.31 }), row({})] },
      { teamId: "Red", result: "loss", roundsWon: 9, firstHalf: 5, players: [row({ puuid: "e", acs: 300 })] },
    ],
  };

  it("hero numbers, MVP and the RR from My Stats", () => {
    const ms = { last5: [{ matchId: "m1", rrEarned: 18 }] } as unknown as MyStats;
    const m = recapModel(view, ms);
    expect(m).toMatchObject({ word: "VICTORY", ours: 13, theirs: 9, mvp: true, rr: 18 });
    expect(m.stats.map((s) => s.v)).toEqual(["24 / 14 / 5", "286", "168", "31%", "+18"]);
    expect(recapModel(view, null).stats.map((s) => s.l)).toEqual(["K / D / A", "ACS", "ADR", "HS%"]);
  });

  it("rows by ACS, the ones without stats last", () => {
    expect(byAcs([row({ puuid: "a", acs: null }), row({ puuid: "b", acs: 150 }), row({ puuid: "c", acs: 250 })]).map((r) => r.puuid)).toEqual(["c", "b", "a"]);
  });

  it("next tier line", () => {
    expect(nextTierText(21, 61, 19, META)).toBe("61 RR to Ascendant 2 · about 4 wins");
    expect(nextTierText(21, 61, null, META)).toBe("61 RR to Ascendant 2");
    expect(nextTierText(25, null, 19, META)).toBeNull();
  });
});

describe("glance callout", () => {
  it("names the first saved player in the match, by side, with the note", () => {
    const l = live("ingame", [card({ puuid: "me", isSelf: true }), card({ puuid: "m", team: "enemy", name: { kind: "visible", name: "Mara", tag: "7788" } })]);
    expect(savedCallout(l, { saved: ["m"], notes: { m: "Entry every round" }, notSmurf: [] }, META)).toEqual({ text: "Mara is on the enemy team", note: "Entry every round" });
    expect(savedCallout(l, { saved: ["me"], notes: {}, notSmurf: [] }, META)).toBeNull();
    expect(savedCallout(live("menus", l.players), { saved: ["m"], notes: {}, notSmurf: [] }, META)).toBeNull();
  });
});
