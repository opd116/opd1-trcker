// vite.config.ts pins TZ=UTC. "Now" is the design's clock: Thu 22 Oct 2026, 22:40.
import { describe, expect, it } from "vitest";
import {
  arrange,
  counts,
  DEFAULT_SORT,
  liveCards,
  nameParts,
  nextSort,
  noResultsText,
  recordCell,
  rowName,
  seenCell,
  sortState,
  summary,
  type SavedView,
} from "./savedv2";
import type { LiveState, Meta, PlayerCard, SavedPlayer } from "./types";

const NOW = Date.UTC(2026, 9, 22, 22, 40);
const DAY = 86_400_000;
const META: Meta = { agents: { reyna: { name: "Reyna" } }, maps: {}, tiers: {} };

function sp(puuid: string, over: Partial<SavedPlayer> = {}): SavedPlayer {
  return {
    puuid,
    name: { kind: "visible", name: puuid, tag: "TAG" },
    agentId: null,
    tier: 20,
    with: { games: 0, wins: 0, losses: 0 },
    against: { games: 0, wins: 0, losses: 0 },
    note: "",
    savedMs: 0,
    lastSeenMs: NOW - DAY,
    lastSeenMapId: null,
    ...over,
  };
}

const v = (s: SavedPlayer, over: Partial<SavedView> = {}): SavedView => ({ s, name: s.name, note: s.note, inMatch: false, ...over });

describe("names", () => {
  it("shows what the DisplayName allows", () => {
    expect(nameParts({ kind: "visible", name: "Mara", tag: "7788" }, META)).toEqual({ name: "Mara", tag: "7788" });
    expect(nameParts({ kind: "hiddenPlayer" }, META)).toEqual({ name: "Hidden Player", tag: null });
    expect(nameParts({ kind: "hiddenAgent", agentId: "reyna" }, META)).toEqual({ name: "Reyna", tag: null });
  });

  it("a player incognito in the live match shows as on their card", () => {
    const s = sp("p", { name: { kind: "visible", name: "Real", tag: "1" } });
    const card = { name: { kind: "hiddenAgent", agentId: "reyna" } } as PlayerCard;
    expect(rowName(s, card)).toEqual({ kind: "hiddenAgent", agentId: "reyna" });
    expect(rowName(s, { name: { kind: "visible", name: "Real", tag: "1" } } as PlayerCard)).toBe(s.name);
    expect(rowName(s, undefined)).toBe(s.name);
  });

  it("live cards only during a match, never the user", () => {
    const players = [{ puuid: "me", isSelf: true }, { puuid: "x", isSelf: false }] as PlayerCard[];
    expect([...liveCards({ stage: "ingame", mapId: null, players, settled: true } as LiveState).keys()]).toEqual(["x"]);
    expect(liveCards({ stage: "menus", mapId: null, players, settled: true }).size).toBe(0);
    expect(liveCards(null).size).toBe(0);
  });
});

describe("filter, search, sort", () => {
  const rows = [
    v(sp("Mara", { tier: 24, against: { games: 3, wins: 0, losses: 3 }, lastSeenMs: NOW - 5 * DAY }), { inMatch: true }),
    v(sp("ishi", { tier: 22, with: { games: 14, wins: 10, losses: 4 }, note: "Great comms", lastSeenMs: NOW - 2 * 3_600_000 }), { note: "Great comms" }),
    v(sp("Nyx", { tier: null, with: { games: 3, wins: 2, losses: 1 }, against: { games: 1, wins: 1, losses: 0 }, lastSeenMs: null })),
    v(sp("halo", { tier: 23, against: { games: 4, wins: 1, losses: 3 }, lastSeenMs: NOW - 3 * DAY })),
  ];
  const ids = (xs: SavedView[]) => xs.map((x) => x.s.puuid);

  it("counts (overlapping filters)", () => {
    expect(counts(rows.map((r) => r.s))).toEqual({ all: 4, mates: 2, opp: 3 });
  });

  it("default: last seen, in the match first, never seen last", () => {
    expect(ids(arrange(rows, "all", "", DEFAULT_SORT, META, NOW))).toEqual(["Mara", "ishi", "halo", "Nyx"]);
  });

  it("filters and searches name, tag and note", () => {
    expect(ids(arrange(rows, "mates", "", DEFAULT_SORT, META, NOW))).toEqual(["ishi", "Nyx"]);
    expect(ids(arrange(rows, "opp", "", DEFAULT_SORT, META, NOW))).toEqual(["Mara", "halo", "Nyx"]);
    expect(ids(arrange(rows, "all", "  COMMS ", DEFAULT_SORT, META, NOW))).toEqual(["ishi"]);
    expect(ids(arrange(rows, "all", "tag", DEFAULT_SORT, META, NOW))).toHaveLength(4);
    expect(arrange(rows, "all", "zzz", DEFAULT_SORT, META, NOW)).toEqual([]);
  });

  it("sorts by each column, and flips", () => {
    expect(ids(arrange(rows, "all", "", { key: "name", dir: 1 }, META, NOW))).toEqual(["halo", "ishi", "Mara", "Nyx"]);
    expect(ids(arrange(rows, "all", "", { key: "rank", dir: 1 }, META, NOW))).toEqual(["Mara", "halo", "ishi", "Nyx"]);
    expect(ids(arrange(rows, "all", "", { key: "with", dir: 1 }, META, NOW))).toEqual(["ishi", "Nyx", "Mara", "halo"]);
    expect(ids(arrange(rows, "all", "", { key: "against", dir: -1 }, META, NOW))).toEqual(["ishi", "Nyx", "Mara", "halo"]);
  });

  it("header clicks and aria-sort", () => {
    expect(nextSort(DEFAULT_SORT, "seen")).toEqual({ key: "seen", dir: -1 });
    expect(nextSort(DEFAULT_SORT, "name")).toEqual({ key: "name", dir: 1 });
    expect(sortState(DEFAULT_SORT, "seen")).toEqual({ aria: "descending", arrow: "▼" });
    expect(sortState({ key: "seen", dir: -1 }, "seen")).toEqual({ aria: "ascending", arrow: "▲" });
    expect(sortState({ key: "name", dir: 1 }, "name")).toEqual({ aria: "ascending", arrow: "▲" });
    expect(sortState(DEFAULT_SORT, "name")).toEqual({ aria: "none", arrow: "" });
  });
});

describe("cells", () => {
  it("records", () => {
    expect(recordCell({ games: 14, wins: 10, losses: 4 }, "Ishi", "with")).toEqual({
      games: "14 games",
      w: "10W",
      l: "4L",
      tip: "Ishi was on your team in 14 stored matches.\nYour record together: 10W – 4L",
    });
    expect(recordCell({ games: 1, wins: 1, losses: 0 }, "Tarek", "against").tip).toBe(
      "Tarek was on the other team in 1 stored match.\nYour record against them: 1W – 0L",
    );
    expect(recordCell({ games: 0, wins: 0, losses: 0 }, "X", "with")).toEqual({ games: null, tip: "Never on your team in a stored match" });
    expect(recordCell({ games: 0, wins: 0, losses: 0 }, "X", "against").tip).toBe("Never against you in a stored match");
  });

  it("last seen", () => {
    expect(seenCell(NOW - DAY, true, NOW)).toEqual({ text: "Now", tip: "In your current match" });
    expect(seenCell(NOW - 2 * 3_600_000, false, NOW)).toEqual({ text: "2 h ago", tip: "Thu, 22 Oct 2026" });
    expect(seenCell(NOW - 9 * DAY, false, NOW)).toEqual({ text: "9 days ago", tip: "Tue, 13 Oct 2026" });
    expect(seenCell(Date.UTC(2026, 8, 15, 12), false, NOW)).toEqual({ text: "Sep 15", tip: "1 month ago" });
    expect(seenCell(null, false, NOW)).toEqual({ text: "—", tip: "" });
  });

  it("texts", () => {
    expect(summary(0)).toBe("Nobody saved yet");
    expect(summary(1)).toBe("1 player");
    expect(summary(8)).toBe("8 players");
    expect(noResultsText(" kai ")).toBe("No saved player matches “kai”.");
    expect(noResultsText("")).toBe("Nobody in this filter yet.");
  });
});
