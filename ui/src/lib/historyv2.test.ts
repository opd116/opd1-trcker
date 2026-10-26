// vite.config.ts pins TZ=UTC, so local time == UTC here. "Now" is the design's clock: Thu 22 Oct 2026, 22:40.
import { describe, expect, it } from "vitest";
import {
  byAcs,
  dayDiff,
  endMs,
  footText,
  groupDays,
  halvesLine,
  heroMeta,
  queueLabel,
  hsCell,
  HS_MISSING_TIP,
  kdOf,
  kdTone,
  monthsOrDays,
  mvpOf,
  rowAria,
  rrCell,
  stamp,
  whenText,
  yourRrOf,
} from "./historyv2";
import type { HistoryRow, MatchDetailsView, MatchSummary, ScoreRow } from "./types";

const NOW = Date.UTC(2026, 9, 22, 22, 40);
const MIN = 60_000;
const ago = (mins: number) => NOW - mins * MIN;

function row(over: Partial<HistoryRow> = {}): HistoryRow {
  return {
    matchId: "m",
    mapId: "map",
    queueId: "competitive",
    startMs: ago(83),
    durationMs: 38 * MIN,
    result: "win",
    roundsWon: 13,
    roundsLost: 9,
    agentId: "a",
    kills: 24,
    deaths: 14,
    assists: 5,
    kd: 1.71,
    acs: 286,
    hsPct: 0.31,
    rrEarned: 18,
    tierAfter: 21,
    ...over,
  };
}

describe("stamp (the design's time helper)", () => {
  it("relative text by age", () => {
    expect(stamp(ago(45), NOW).rel).toBe("45 min ago");
    expect(stamp(ago(0.2), NOW).rel).toBe("1 min ago");
    expect(stamp(ago(190), NOW).rel).toBe("3 h ago");
    expect(stamp(ago(24 * 60), NOW).rel).toBe("Yesterday");
    expect(stamp(ago(9 * 1440), NOW).rel).toBe("9 days ago");
    expect(stamp(Date.UTC(2026, 8, 15, 12), NOW).rel).toBe("Sep 15");
    expect(stamp(Date.UTC(2025, 8, 15, 12), NOW).rel).toBe("Sep 15, 2025");
  });

  it("exact date, clock time and day labels", () => {
    const s = stamp(ago(45), NOW);
    expect(s.exact).toBe("Thu, 22 Oct 2026 · 9:55 PM");
    expect(s.hm).toBe("9:55 PM");
    expect(s.dayLabel).toBe("Today");
    expect(stamp(ago(1408), NOW).dayLabel).toBe("Yesterday");
    expect(stamp(Date.UTC(2026, 9, 18, 20), NOW).dayLabel).toBe("Sunday, Oct 18");
    expect(stamp(Date.UTC(2025, 9, 18, 20), NOW).dayLabel).toBe("Saturday, Oct 18, 2025");
  });

  it("a match just after midnight is Today, the one just before is Yesterday", () => {
    expect(dayDiff(Date.UTC(2026, 9, 22, 0, 5), NOW)).toBe(0);
    expect(dayDiff(Date.UTC(2026, 9, 21, 23, 55), NOW)).toBe(1);
    expect(stamp(Date.UTC(2026, 9, 21, 23, 55), NOW).rel).toBe("Yesterday");
  });

  it("months or days for a last-seen 30+ days back", () => {
    expect(monthsOrDays(Date.UTC(2026, 8, 15, 12), NOW)).toBe("1 month");
    expect(monthsOrDays(Date.UTC(2026, 6, 1), NOW)).toBe("3 months");
    expect(monthsOrDays(Date.UTC(2026, 9, 21, 12), NOW)).toBe("1 day");
  });
});

describe("history rows", () => {
  it("times a match by its end", () => {
    const r = row();
    expect(endMs(r)).toBe(ago(45));
    expect(whenText(r, NOW)).toBe("45 min ago · 38 min");
    expect(whenText({ startMs: ago(1450), durationMs: 42 * MIN }, NOW)).toBe("11:12 PM · 42 min");
    expect(whenText({ startMs: ago(45), durationMs: null }, NOW)).toBe("45 min ago");
    // A non-competitive row leads with its mode; a competitive one stays as the design draws it.
    expect(whenText({ startMs: ago(63), durationMs: 18 * MIN, queueId: "swiftplay" }, NOW)).toBe("Swiftplay · 45 min ago · 18 min");
    expect(whenText({ startMs: ago(83), durationMs: 38 * MIN, queueId: "competitive" }, NOW)).toBe("45 min ago · 38 min");
    expect(whenText({ startMs: ago(1450), durationMs: 42 * MIN, queueId: "hurm" }, NOW)).toBe("Team Deathmatch · 11:12 PM · 42 min");
    expect(whenText({ startMs: ago(45), durationMs: null, queueId: "some_new" }, NOW)).toBe("Some New · 45 min ago");
  });

  it("groups by day with W/L, the strip oldest → newest and RR from competitive rows only", () => {
    const rows = [
      row({ matchId: "a", startMs: ago(83), result: "win", rrEarned: 18 }),
      row({ matchId: "b", startMs: ago(200), result: "loss", rrEarned: -19 }),
      row({ matchId: "c", startMs: ago(260), queueId: "swiftplay", result: "win", rrEarned: null, tierAfter: null }),
      row({ matchId: "d", startMs: ago(1450), result: "loss", rrEarned: -18 }),
      row({ matchId: "e", startMs: ago(1500), queueId: "unrated", result: "win", rrEarned: 25 }),
    ];
    const g = groupDays(rows, NOW);
    expect(g.map((x) => x.label)).toEqual(["Today", "Yesterday"]);
    expect(g[0]).toMatchObject({ wins: 2, losses: 1, strip: ["win", "loss", "win"], rr: -1 });
    // An unrated row's RR never counts, even if one were sent.
    expect(g[1]).toMatchObject({ wins: 1, losses: 1, rr: -18 });
    expect(groupDays([row({ queueId: "swiftplay", rrEarned: null })], NOW)[0]!.rr).toBeNull();
  });

  it("cells", () => {
    expect(kdTone(1.71)).toBe("hi");
    expect(kdTone(0.94)).toBe("lo");
    expect(kdTone(1.0)).toBe("mid");
    expect(kdTone(null)).toBe("none");
    expect(hsCell(row())).toEqual({ text: "31%", tip: "" });
    expect(hsCell(row({ hsPct: null }))).toEqual({ text: "—", tip: HS_MISSING_TIP });
    expect(hsCell(row({ hsPct: null, kills: null }))).toEqual({ text: "—", tip: "" });
    expect(rrCell(row())).toEqual({ text: "+18", tone: "win" });
    expect(rrCell(row({ rrEarned: -19 }))).toEqual({ text: "−19", tone: "loss" });
    expect(rrCell(row({ rrEarned: 0 }))).toEqual({ text: "0", tone: "flat" });
    expect(rrCell(row({ queueId: "swiftplay", rrEarned: 5 }))).toEqual({ text: "—", tone: "flat" });
    expect(rowAria(row(), "Lotus")).toBe("Lotus, win 13 to 9, plus 18 RR. Open scoreboard");
    expect(rowAria(row({ queueId: "unrated", result: "loss", roundsWon: null }), null)).toBe("Unknown map, loss. Open scoreboard");
  });

  it("footer", () => {
    expect(footText(13, 13)).toMatch(/^Showing all 13 stored matches, newest first\. Matches are stored/);
    expect(footText(1, 1)).toMatch(/^Showing all 1 stored match,/);
    expect(footText(50, 213)).toMatch(/^Showing 50 of 213 stored matches, newest first\./);
  });
});

const p = (puuid: string, acs: number | null, over: Partial<ScoreRow> = {}): ScoreRow => ({
  puuid,
  name: { kind: "visible", name: puuid, tag: "0" },
  agentId: null,
  tier: 21,
  kills: acs === null ? null : 10,
  deaths: acs === null ? null : 10,
  assists: acs === null ? null : 2,
  acs,
  adr: null,
  hsPct: null,
  isSelf: false,
  isFocus: false,
  ...over,
});

function view(over: Partial<MatchDetailsView> = {}): MatchDetailsView {
  return {
    matchId: "m",
    mapId: "map",
    queueId: "competitive",
    startMs: ago(83),
    durationMs: 38 * MIN,
    perspective: "self",
    teams: [
      { teamId: "Blue", result: "win", roundsWon: 13, firstHalf: 7, players: [p("a", 200), p("b", 292), p("c", null)] },
      { teamId: "Red", result: "loss", roundsWon: 9, firstHalf: 5, players: [p("d", 279), p("e", 227)] },
    ],
    ...over,
  };
}

describe("scoreboard", () => {
  it("hero meta and halves", () => {
    expect(heroMeta(view())).toBe("Competitive · Thu, 22 Oct 2026 · 9:55 PM · 38 min");
    expect(heroMeta(view({ queueId: "unrated", durationMs: null }))).toBe("Unrated · Thu, 22 Oct 2026 · 9:17 PM");
    expect(halvesLine(view())).toBe("First half 7–5");
    const ot = view();
    ot.teams[0]!.roundsWon = 15;
    ot.teams[1]!.roundsWon = 13;
    expect(halvesLine(ot)).toBe("Went to overtime");
    const nohalf = view();
    nohalf.teams[1]!.firstHalf = null;
    expect(halvesLine(nohalf)).toBeNull();
    expect(halvesLine(view({ teams: [view().teams[0]!] }))).toBeNull();
  });

  it("rows by ACS, a row without stats last; MVPs", () => {
    expect(byAcs(view().teams[0]!.players).map((x) => x.puuid)).toEqual(["b", "a", "c"]);
    const m = mvpOf(view());
    expect(m.match).toBe("b");
    expect([...m.team]).toEqual(["d"]);
    // A no-team mode: one block, a match MVP and no team MVP.
    const dm = mvpOf(view({ teams: [{ teamId: "", result: null, roundsWon: null, firstHalf: null, players: [p("x", 300), p("y", 310)] }] }));
    expect(dm).toEqual({ match: "y", team: new Set() });
    // Nobody with stats: no MVP.
    expect(mvpOf(view({ teams: [{ teamId: "", result: null, roundsWon: null, firstHalf: null, players: [p("x", null)] }] })).match).toBeNull();
  });

  it("YOUR RR: the rank after the match comes from a history row, never from My Stats' rank during it", () => {
    // A promotion game: Gold 3 (14) during the match, Platinum 1 (15) after it.
    const ms = { last5: [{ matchId: "m", rrEarned: 22, tier: 14 } as MatchSummary], session: null };
    // The History cache lacks the match (and the scoreboard's own ask has not answered): the RR change alone.
    expect(yourRrOf("m", null, ms)).toEqual({ rr: 22, tier: null });
    // The history row, cached or asked for, has the rank after it.
    expect(yourRrOf("m", { rrEarned: 22, tierAfter: 15 }, ms)).toEqual({ rr: 22, tier: 15 });
    expect(yourRrOf("m", { rrEarned: null, tierAfter: null }, ms)).toEqual({ rr: 22, tier: null });
    const session = { netRr: 22, wins: 1, losses: 0, startedMs: 0, matches: [{ matchId: "s", rrEarned: -15 }] } as never;
    expect(yourRrOf("s", null, { last5: [], session })).toEqual({ rr: -15, tier: null });
    expect(yourRrOf("x", null, ms)).toBeNull();
  });

  it("K/D", () => {
    expect(kdOf(8, 13)).toBe("0.62");
    expect(kdOf(5, 0)).toBe("5.00");
    expect(kdOf(null, 3)).toBe("—");
  });
});

describe("queueLabel (History rows, scoreboard, recap, queue timer)", () => {
  it("queueLabel table incl. '' → Custom and unknown → title case", () => {
    const table: Record<string, string> = {
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
    for (const [id, label] of Object.entries(table)) expect(queueLabel(id), id).toBe(label);
    expect(queueLabel("valaram")).toBe("Valaram");
    expect(queueLabel("new_mode")).toBe("New Mode");
    expect(queueLabel("newmap-test")).toBe("Newmap Test");
  });
});
