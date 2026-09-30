// Player Details and Match Details helpers (spec §6, Task 18). Pure functions; the screens only render them.
import { describe, expect, it, vi } from "vitest";
import {
  CONTENT_TIERS,
  emptyMatch,
  halvesText,
  liveTeammate,
  pmText,
  queueLabel,
  recentShown,
  rowKeydown,
  settledDetails,
  shortName,
  teamLabel,
  weaponState,
  whenText,
} from "./details";
import type { DisplayName, LiveState, MatchDetailsView, MatchSummary, MatchTeamView, Meta, PlayerCard, WeaponSkin } from "./types";

const JETT = "add6443a-41bd-e414-f6ad-e58d267f4e95";
const meta: Meta = { agents: { [JETT]: { name: "Jett" } }, maps: {}, tiers: {} };

const row = (i: number): MatchSummary => ({
  matchId: `m${i}`,
  mapId: "7eaecc1b-4337-bbf6-6ab9-04b8f06b3319",
  startMs: 1_000_000 - i,
  result: "win",
  roundsWon: 13,
  roundsLost: 9,
  agentId: JETT,
  kills: 20,
  deaths: 10,
  assists: 4,
  kd: 2,
  acs: 240,
  hsPct: 0.25,
  rrEarned: 18,
  tier: 21,
});
const rows = (n: number) => Array.from({ length: n }, (_, i) => row(i));

describe("recentShown (spec §6.4, §6.5)", () => {
  it("recentShown 3 then 8 and hides the button", () => {
    // Collapsed: the newest 3, and a button only when there is more to show.
    expect(recentShown(rows(8), false)).toEqual({ rows: rows(3), showMore: true });
    expect(recentShown(rows(4), false)).toEqual({ rows: rows(3), showMore: true });
    expect(recentShown(rows(3), false)).toEqual({ rows: rows(3), showMore: false });
    expect(recentShown(rows(1), false)).toEqual({ rows: rows(1), showMore: false });
    expect(recentShown([], false)).toEqual({ rows: [], showMore: false });
    // After Show more: up to 8, newest first, and never a button again (no "show less").
    expect(recentShown(rows(8), true)).toEqual({ rows: rows(8), showMore: false });
    expect(recentShown(rows(10), true)).toEqual({ rows: rows(8), showMore: false });
    expect(recentShown(rows(5), true)).toEqual({ rows: rows(5), showMore: false });
  });
});

describe("shortName (spec §6.6, §5.8, R95)", () => {
  const visible: DisplayName = { kind: "visible", name: "Mara", tag: "7788" };
  const remembered: DisplayName = { kind: "remembered", name: "Rook", tag: "4413", source: "ownMatch", fromMapId: null, fromStartMs: null };

  it("shortName per variant", () => {
    expect(shortName(visible, meta)).toBe("Mara");
    expect(shortName(remembered, meta)).toBe("Rook");
    expect(shortName({ kind: "hiddenPlayer" }, meta)).toBe("Hidden Player");
    expect(shortName({ kind: "hiddenAgent", agentId: JETT }, meta)).toBe("Jett");
    // Meta pending: only the agent name waits (the caller draws the 60×10 bar); a miss in loaded meta is —.
    expect(shortName({ kind: "hiddenAgent", agentId: JETT }, null)).toBeUndefined();
    expect(shortName({ kind: "hiddenAgent", agentId: "not-in-meta" }, meta)).toBe("—");
    // Visible, remembered and hiddenPlayer never wait for meta.
    expect(shortName(visible, null)).toBe("Mara");
    expect(shortName(remembered, null)).toBe("Rook");
    expect(shortName({ kind: "hiddenPlayer" }, null)).toBe("Hidden Player");
  });

  it("a hidden kind never yields a tag or a real name", () => {
    for (const m of [meta, null]) {
      expect(shortName({ kind: "hiddenAgent", agentId: JETT }, m) ?? "").not.toContain("#");
      expect(shortName({ kind: "hiddenPlayer" }, m)).not.toContain("#");
    }
  });
});

describe("weaponState (spec §6.8, R97)", () => {
  const skin: WeaponSkin = { chromaId: "c", skinName: "Reaver", chromaName: "Red", tierId: null };

  it("weaponState: absent → loading, null → none, value → the skin", () => {
    expect(weaponState(undefined)).toBe("loading");
    expect(weaponState(null)).toBe("none");
    expect(weaponState(skin)).toBe(skin);
  });
});

describe("liveTeammate (spec §6.7)", () => {
  const card = (puuid: string, name: PlayerCard["name"], extra: Partial<PlayerCard> = {}): PlayerCard => ({
    puuid,
    name,
    cardId: "card",
    agentId: JETT,
    rank: { tier: 23, rr: 47 },
    party: null,
    encounters: null,
    team: "enemy",
    isSelfParty: false,
    isSelf: false,
    ...extra,
  });
  const live: LiveState = {
    stage: "ingame",
    mapId: null,
    players: [
      card("vis", { kind: "visible", name: "Kilo", tag: "2210" }),
      card("rem", { kind: "remembered", name: "Rook", tag: "4413", source: "ownMatch", fromMapId: null, fromStartMs: null }),
      card("agent", { kind: "hiddenAgent", agentId: JETT }),
      card("hp", { kind: "hiddenPlayer" }, { agentId: null }),
      card("self", undefined, { isSelf: true, isSelfParty: true }),
      card("unranked", { kind: "visible", name: "New", tag: "0001" }, { agentId: null, rank: { tier: 0, rr: 0 } }),
      card("norank", { kind: "visible", name: "Nil", tag: "0002" }, { rank: null }),
      card("loadingrank", { kind: "visible", name: "Wait", tag: "0003" }, { rank: undefined }),
    ],
  };

  it("liveTeammate ignores hidden live cards", () => {
    expect(liveTeammate("vis", live)).toEqual({ agentId: JETT, tier: 23 });
    // Every hidden style hides the live indicator: the row must not reveal who that hidden card is.
    expect(liveTeammate("rem", live)).toBeNull();
    expect(liveTeammate("agent", live)).toBeNull();
    expect(liveTeammate("hp", live)).toBeNull();
  });

  it("self counts as visible (its name is the user's own, R43), even while the name is loading", () => {
    expect(liveTeammate("self", live)).toEqual({ agentId: JETT, tier: 23 });
  });

  it("no agent yet, unranked or unknown rank → null parts (empty boxes); not on screen or no live state → null", () => {
    expect(liveTeammate("unranked", live)).toEqual({ agentId: null, tier: null });
    expect(liveTeammate("norank", live)).toEqual({ agentId: JETT, tier: null });
    expect(liveTeammate("loadingrank", live)).toEqual({ agentId: JETT, tier: null });
    expect(liveTeammate("nobody", live)).toBeNull();
    expect(liveTeammate("vis", null)).toBeNull();
  });
});

describe("queueLabel (spec §6.10)", () => {
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
      "": "Custom",
    };
    for (const [id, label] of Object.entries(table)) expect(queueLabel(id), id).toBe(label);
    expect(queueLabel("snowball")).toBe("Snowball");
    expect(queueLabel("new_mode")).toBe("New Mode");
    expect(queueLabel("newmap-test")).toBe("Newmap Test");
  });
});

// ── match views ─────────────────────────────────────────────────────────────────
const team = (t: Partial<MatchTeamView>): MatchTeamView => ({ teamId: "Blue", result: "win", roundsWon: 13, firstHalf: 7, players: [], ...t });
const view = (v: Partial<MatchDetailsView> = {}): MatchDetailsView => ({
  matchId: "m",
  mapId: "7eaecc1b-4337-bbf6-6ab9-04b8f06b3319",
  queueId: "competitive",
  startMs: Date.UTC(2026, 8, 29, 21, 14),
  durationMs: 38 * 60_000,
  perspective: "focus",
  teams: [team({}), team({ teamId: "Red", result: "loss", roundsWon: 9, firstHalf: 4 })],
  ...v,
});
const dm = () =>
  view({ queueId: "deathmatch", teams: [team({ teamId: "", result: null, roundsWon: null, firstHalf: null })], durationMs: 9 * 60_000 });

describe("whenText (spec §6.10, R140)", () => {
  // The page clock: Sep 30, 2026, so `view()` (Sep 29, 2026) is a current-year match.
  const NOW = Date.UTC(2026, 8, 30, 12);

  it("a match opened from a recent row: '{queueLabel} · {dateLong} · {time12} · {durationMin}'", () => {
    expect(whenText(view(), "recent", NOW)).toBe("Competitive · Sep 29, 2026 · 9:14 PM · 38 min");
    expect(whenText(dm(), "recent", NOW)).toBe("Deathmatch · Sep 29, 2026 · 9:14 PM · 9 min");
    expect(whenText(view({ queueId: "" }), "recent", NOW)).toBe("Custom · Sep 29, 2026 · 9:14 PM · 38 min");
  });

  it("a match opened from an encounter row in the current year drops the year (v8:642, v8:685)", () => {
    expect(whenText(view(), "encounter", NOW)).toBe("Competitive · Sep 29 · 9:14 PM · 38 min");
    expect(whenText(dm(), "encounter", NOW)).toBe("Deathmatch · Sep 29 · 9:14 PM · 9 min");
    expect(whenText(view({ startMs: Date.UTC(2026, 8, 12, 23, 18), durationMs: 41 * 60_000 }), "encounter", NOW)).toBe(
      "Competitive · Sep 12 · 11:18 PM · 41 min",
    );
  });

  it("a match opened from an encounter row in a previous year keeps the year", () => {
    expect(whenText(view({ startMs: Date.UTC(2025, 11, 31, 10, 5) }), "encounter", NOW)).toBe("Competitive · Dec 31, 2025 · 10:05 AM · 38 min");
    // The same match a year later: the current year moved on, so the year comes back.
    expect(whenText(view(), "encounter", Date.UTC(2027, 0, 2))).toBe("Competitive · Sep 29, 2026 · 9:14 PM · 38 min");
  });

  it("the recent entry always has the year, whatever the clock", () => {
    expect(whenText(view(), "recent", Date.UTC(2027, 0, 2))).toBe("Competitive · Sep 29, 2026 · 9:14 PM · 38 min");
  });

  it("whenText drops the duration when unknown", () => {
    expect(whenText(view({ durationMs: null }), "recent", NOW)).toBe("Competitive · Sep 29, 2026 · 9:14 PM");
    expect(whenText(view({ durationMs: null }), "encounter", NOW)).toBe("Competitive · Sep 29 · 9:14 PM");
  });
});

describe("halvesText (spec §6.10, v8:651, v8:659)", () => {
  it("halvesText a1–b1 · a2–b2 incl. overtime in the second", () => {
    expect(halvesText(view())).toBe("7–4 · 6–5");
    // 14–12 after overtime: 7–5 at the half, so the second part (overtime included) is 7–7.
    expect(halvesText(view({ teams: [team({ roundsWon: 14, firstHalf: 7 }), team({ roundsWon: 12, firstHalf: 5 })] }))).toBe("7–5 · 7–7");
  });

  it("null when a half is unknown, a score is unknown, or in a no-team mode", () => {
    expect(halvesText(view({ teams: [team({ firstHalf: null }), team({ firstHalf: 4, roundsWon: 9 })] }))).toBeNull();
    expect(halvesText(view({ teams: [team({}), team({ firstHalf: null, roundsWon: 9 })] }))).toBeNull();
    expect(halvesText(view({ teams: [team({ roundsWon: null }), team({ firstHalf: 4, roundsWon: 9 })] }))).toBeNull();
    expect(halvesText(dm())).toBeNull();
  });
});

describe("teamLabel (spec §6.10)", () => {
  it("teamLabel self / focus / dm", () => {
    expect(teamLabel(view({ perspective: "self" }), 0, "Mara")).toBe("Your team");
    expect(teamLabel(view({ perspective: "self" }), 1, "Mara")).toBe("Opponents");
    expect(teamLabel(view({ perspective: "focus" }), 0, "Mara")).toBe("Mara's team");
    expect(teamLabel(view({ perspective: "focus" }), 1, "Mara")).toBe("Opponents");
    expect(teamLabel(view({ perspective: "focus" }), 0, "Hidden Player")).toBe("Hidden Player's team");
    expect(teamLabel(dm(), 0, "Mara")).toBe("Players");
  });
});

describe("pmText (spec §6.10, R124)", () => {
  it("pmText signs with U+2212", () => {
    expect(pmText(22, 16)).toEqual({ text: "+6", tone: "win" });
    expect(pmText(12, 16)).toEqual({ text: "−4", tone: "loss" });
    expect(pmText(15, 15)).toEqual({ text: "0", tone: "flat" });
    expect(pmText(12, 16).text).not.toContain("-");
  });

  it("an unknown kill or death count is — in the flat tone, never 0 (R124)", () => {
    expect(pmText(null, 16)).toEqual({ text: "—", tone: "flat" });
    expect(pmText(12, null)).toEqual({ text: "—", tone: "flat" });
    expect(pmText(null, null)).toEqual({ text: "—", tone: "flat" });
  });
});

describe("CONTENT_TIERS (spec §6.8)", () => {
  it("the five content tiers by uuid, with the design's colours", () => {
    expect(CONTENT_TIERS).toEqual({
      "12683d76-48d7-84a3-4e09-6985794f0445": { name: "Select", color: "#5a9fe2" },
      "0cebb8be-46d7-c12a-d306-e9907bfc5a25": { name: "Deluxe", color: "#009587" },
      "60bca009-4182-7998-dee7-b8a2558dc369": { name: "Premium", color: "#d1548d" },
      "e046854e-406c-37f4-6607-19a9ba8426fc": { name: "Exclusive", color: "#f5955b" },
      "411e4a55-4e59-7757-41f0-86a53f101bb5": { name: "Ultra", color: "#fad663" },
    });
  });
});

describe("rowKeydown (Recent and Encounters rows, one handler for both)", () => {
  const key = (k: string) => ({ key: k, preventDefault: vi.fn() }) as unknown as KeyboardEvent & { preventDefault: ReturnType<typeof vi.fn> };

  it("Enter and Space open the row as a click does, and keep the key from scrolling the page", () => {
    for (const k of ["Enter", " "]) {
      const open = vi.fn();
      const e = key(k);
      rowKeydown(open)(e);
      expect(open, k).toHaveBeenCalledTimes(1);
      expect(e.preventDefault, k).toHaveBeenCalledTimes(1);
    }
  });

  it("any other key does nothing", () => {
    for (const k of ["Tab", "ArrowDown", "a", "Escape"]) {
      const open = vi.fn();
      const e = key(k);
      rowKeydown(open)(e);
      expect(open, k).not.toHaveBeenCalled();
      expect(e.preventDefault, k).not.toHaveBeenCalled();
    }
  });
});

describe("settledDetails (a player_details that failed twice: the settled empty state, never a skeleton)", () => {
  const live: PlayerCard = {
    puuid: "p1",
    name: { kind: "visible", name: "Mara", tag: "1177" },
    cardId: "card-1",
    agentId: JETT,
    // Still loading on the live card: `undefined` would keep a skeleton bar in the hero forever.
    level: undefined,
    rank: { tier: 21, rr: 40 },
    party: null,
    encounters: { with: 1, against: 2 },
    team: "enemy",
    isSelfParty: false,
    isSelf: false,
  };

  it("takes the hero from the live card and settles every still-loading field to null (—)", () => {
    const d = settledDetails("p1", live);
    expect(d.card.puuid).toBe("p1");
    expect(d.card.name).toEqual(live.name);
    expect(d.card.rank).toEqual({ tier: 21, rr: 40 });
    expect(d.card.cardId).toBe("card-1");
    for (const k of ["level", "prevActTier", "peak", "kd", "hsPct", "actWinrate", "actWins", "actGames", "last5"] as const) {
      expect(d.card[k], k).toBeNull();
      expect(k in d.card, k).toBe(true);
    }
  });

  it("without a live card: an all-null card for that puuid (— everywhere, never a name it does not know)", () => {
    const d = settledDetails("p9", undefined);
    expect(d.card.puuid).toBe("p9");
    expect(d.card.name).toBeNull();
    expect(d.card.cardId).toBe("");
    expect(d.card.isSelf).toBe(false);
    expect(d.card.party).toBeNull();
    expect(d.card.encounters).toBeNull();
    for (const k of ["level", "rank", "prevActTier", "peak", "kd", "hsPct", "actWinrate", "actWins", "actGames", "last5"] as const) {
      expect(d.card[k], k).toBeNull();
    }
  });

  it("every section is the empty one: no rows, no teammates, no encounters, four null weapons, no hero stats", () => {
    for (const d of [settledDetails("p1", live), settledDetails("p9", undefined)]) {
      expect(d.recent).toEqual([]);
      expect(d.topTeammates).toEqual([]);
      expect(d.encounters).toEqual([]);
      expect(d.loadout).toEqual({ vandal: null, phantom: null, operator: null, sheriff: null });
      expect(d.heroKd).toBeNull();
      expect(d.heroHs).toBeNull();
      expect(d.heroMatches).toBe(0);
    }
  });

  it("does not mutate the live card", () => {
    const before = structuredClone(live);
    settledDetails("p1", live);
    expect(live).toEqual(before);
  });
});

describe("emptyMatch (a match_details that failed twice)", () => {
  it("is a well-formed view with no teams: the header reads — and the scoreboard shows only its header row", () => {
    const v = emptyMatch("m1");
    expect(v.matchId).toBe("m1");
    expect(v.mapId).toBe("");
    expect(v.teams).toEqual([]);
    expect(v.durationMs).toBeNull();
    expect(halvesText(v)).toBeNull();
  });
});
