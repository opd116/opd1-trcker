// The Live page's pure helpers (spec §2.3, §3.1, §3.5, §3.8, §3.9, §5.1–§5.7) and the result-box styles (§1.5).
import { describe, expect, it } from "vitest";
import { card as mk } from "./demo/fixtures";
import { SCENES } from "./demo/scenes";
import { avgPartyRank, last5Tiles, liveColumns, showEncounters, showParty, type SlotItem } from "./live";
import { boxStyle } from "./resultBox";
import type { Last5Entry, LiveState, PlayerCard, Stage } from "./types";

const c = (puuid: string, extra: Partial<PlayerCard> = {}): PlayerCard => mk({ puuid, cardId: "card", ...extra });
const me = (extra: Partial<PlayerCard> = {}) => c("me", { isSelf: true, isSelfParty: true, ...extra });
const st = (stage: Stage, players: PlayerCard[], mapId: string | null = null): LiveState => ({ stage, mapId, players });
/** A column as puuids for cards and the kind for everything else. */
const col = (xs: SlotItem[]) => xs.map((x) => (x.kind === "card" ? x.card.puuid : x.kind));
const NONE5 = ["none", "none", "none", "none", "none"];

describe("liveColumns", () => {
  it("closed → self only in slot 1", () => {
    const { left, right } = liveColumns(st("closed", [me()]));
    expect(col(left)).toEqual(["me", "none", "none", "none", "none"]);
    expect(col(right)).toEqual(NONE5);
  });

  it("closed with players [] → no card and no skeleton", () => {
    const { left, right } = liveColumns(st("closed", []));
    expect(col(left)).toEqual(NONE5);
    expect(col(right)).toEqual(NONE5);
  });

  it("boot → 1 skeleton", () => {
    const { left, right } = liveColumns(null);
    expect(col(left)).toEqual(["skeleton", "none", "none", "none", "none"]);
    expect(col(right)).toEqual(NONE5);
  });

  it("menus keeps emitted order with self first", () => {
    const { left, right } = liveColumns(st("menus", [c("a"), c("b"), me(), c("d")]));
    expect(col(left)).toEqual(["me", "a", "b", "d", "none"]);
    expect(col(right)).toEqual(NONE5);
  });

  it("menus with players [] → 1 skeleton in slot 1", () => {
    expect(col(liveColumns(st("menus", [])).left)).toEqual(["skeleton", "none", "none", "none", "none"]);
  });

  it("pregame → 5 dashed on the right", () => {
    const { left, right } = liveColumns(st("pregame", [c("a"), c("b"), me(), c("d"), c("e")], "map"));
    expect(col(left)).toEqual(["me", "a", "b", "d", "e"]);
    expect(col(right)).toEqual(["dashed", "dashed", "dashed", "dashed", "dashed"]);
  });

  it("pregame-loading → 5 skeletons + 5 dashed", () => {
    const { left, right } = liveColumns(st("pregame", []));
    expect(col(left)).toEqual(["skeleton", "skeleton", "skeleton", "skeleton", "skeleton"]);
    expect(col(right)).toEqual(["dashed", "dashed", "dashed", "dashed", "dashed"]);
  });

  it("ingame-loading → 5 + 5 skeletons", () => {
    const { left, right } = liveColumns(st("ingame", []));
    expect(col(left)).toEqual(["skeleton", "skeleton", "skeleton", "skeleton", "skeleton"]);
    expect(col(right)).toEqual(["skeleton", "skeleton", "skeleton", "skeleton", "skeleton"]);
  });

  it("ingame puts allies left (self first) and enemies right in emitted order", () => {
    const e = (id: string) => c(id, { team: "enemy" });
    const { left, right } = liveColumns(st("ingame", [e("x"), c("a"), e("y"), me(), c("b")], "map"));
    expect(col(left)).toEqual(["me", "a", "b", "none", "none"]);
    expect(col(right)).toEqual(["x", "y", "none", "none", "none"]);
  });

  it("never more than 5 cards in a column", () => {
    const six = ["a", "b", "c", "d", "e", "f"].map((id) => c(id));
    expect(col(liveColumns(st("menus", six)).left)).toEqual(["a", "b", "c", "d", "e"]);
  });

  it("the demo ingame scene gives the design's two columns", () => {
    const { left, right } = liveColumns(SCENES.ingame.live);
    const names = (xs: SlotItem[]) =>
      xs.map((x) => (x.kind === "card" && x.card.name?.kind === "visible" ? x.card.name.name : x.kind === "card" ? x.card.name?.kind : x.kind));
    expect(names(left)).toEqual(["Fahad", "Sable", "VesperUnbreakabl", "Drake", "Nova"]);
    expect(names(right)).toEqual(["Mara", "Kilo", "Zephyr", "remembered", "hiddenAgent"]);
  });
});

const entry = (result: Last5Entry["result"], startMs: number): Last5Entry => ({ result, rrEarned: 0, startMs });

describe("last5Tiles", () => {
  it("last5Tiles puts empties left and the newest rightmost", () => {
    // Newest first on the wire: a win at t=5, then a loss at t=4.
    const tiles = last5Tiles([entry("win", 5), entry("loss", 4)]);
    if (tiles === "skeleton") throw new Error("expected tiles");
    expect(tiles.map((t) => t.key)).toEqual(["E", "E", "E", "L", "W"]);
    expect(tiles.map((t) => t.newest)).toEqual([false, false, false, false, true]);
    expect(tiles.map((t) => t.entry?.startMs ?? null)).toEqual([null, null, null, 4, 5]);
  });

  it("renders Fahad's W L W W L (newest first) as L W W L W", () => {
    const letters = { W: "win", L: "loss", D: "draw" } as const;
    const tiles = last5Tiles([..."WLWWL"].map((l, i) => entry(letters[l as "W" | "L"], 10 - i)));
    if (tiles === "skeleton") throw new Error("expected tiles");
    expect(tiles.map((t) => t.key).join(" ")).toBe("L W W L W");
    expect(tiles[4]!.newest).toBe(true);
  });

  it("last5Tiles(null) → 5 E", () => {
    for (const v of [null, []]) {
      const tiles = last5Tiles(v);
      if (tiles === "skeleton") throw new Error("expected tiles");
      expect(tiles.map((t) => t.key)).toEqual(["E", "E", "E", "E", "E"]);
      expect(tiles.some((t) => t.newest || t.entry)).toBe(false);
    }
  });

  it("last5Tiles(undefined) → skeleton", () => {
    expect(last5Tiles(undefined)).toBe("skeleton");
  });

  it("keeps only the newest 5 and maps a draw to D", () => {
    const tiles = last5Tiles([entry("draw", 9), entry("win", 8), entry("win", 7), entry("loss", 6), entry("loss", 5), entry("win", 4)]);
    if (tiles === "skeleton") throw new Error("expected tiles");
    expect(tiles.map((t) => t.key)).toEqual(["L", "L", "W", "W", "D"]);
    expect(tiles.map((t) => t.entry!.startMs)).toEqual([5, 6, 7, 8, 9]);
  });
});

const ranked = (id: string, tier: number) => c(id, { rank: { tier, rr: 10 } });

describe("avgPartyRank", () => {
  it("null when fewer than 2 players", () => {
    expect(avgPartyRank([])).toBeNull();
    expect(avgPartyRank([ranked("a", 21)])).toBeNull();
  });

  it("avgPartyRank rounds .5 up over tier ≥ 3 only", () => {
    // 21 and 22 → 21.5 → 22. The tier-0 player is not counted (with it the mean would be 14.3).
    expect(avgPartyRank([ranked("a", 21), ranked("b", 22), ranked("c", 0)])).toEqual({ tier: 22, n: 2, loading: false });
    // 20, 21, 21 → 20.67 → 21.
    expect(avgPartyRank([ranked("a", 20), ranked("b", 21), ranked("c", 21)])).toEqual({ tier: 21, n: 3, loading: false });
  });

  it("avgPartyRank with 0 ranked → tier null", () => {
    expect(avgPartyRank([c("a", { rank: null }), c("b", { rank: null })])).toEqual({ tier: null, n: 0, loading: false });
  });

  it("avgPartyRank loading when none ranked and one absent", () => {
    expect(avgPartyRank([c("a"), c("b", { rank: null })])).toEqual({ tier: null, n: 0, loading: true });
    // Once someone is ranked, a member still loading no longer holds the value back.
    expect(avgPartyRank([ranked("a", 21), c("b")])).toEqual({ tier: 21, n: 1, loading: false });
  });

  it("avgPartyRank lobby2 → n = 1", () => {
    expect(avgPartyRank(SCENES.lobby2.live!.players)).toEqual({ tier: 21, n: 1, loading: false });
  });
});

describe("card indicators", () => {
  it("showEncounters / showParty suppress self and self-party", () => {
    const enc = { with: 1, against: 2 };
    const party = { group: 0, confirmed: true };
    expect(showEncounters(c("a", { encounters: enc }))).toBe(true);
    expect(showEncounters(c("a", { encounters: { with: 0, against: 0 } }))).toBe(false);
    expect(showEncounters(c("a", { encounters: null }))).toBe(false);
    expect(showEncounters(me({ encounters: enc }))).toBe(false);
    expect(showEncounters(c("a", { encounters: enc, isSelfParty: true }))).toBe(false);

    expect(showParty(c("a", { party }))).toBe(true);
    expect(showParty(c("a", { party: null }))).toBe(false);
    expect(showParty(me({ party }))).toBe(false);
    expect(showParty(c("a", { party, isSelfParty: true }))).toBe(false);
  });
});

describe("boxStyle", () => {
  it("reads the plain result variables", () => {
    expect(boxStyle("W")).toBe("background:var(--r-w-bg);color:var(--r-w-fg);border:var(--r-w-bd)");
    expect(boxStyle("D")).toBe("background:var(--r-d-bg);color:var(--r-d-fg);border:var(--r-d-bd)");
    expect(boxStyle("E")).toBe("background:var(--r-e-bg);color:var(--r-e-fg);border:var(--r-e-bd)");
  });

  it("the newest tile reads the newest variables, except an empty slot", () => {
    expect(boxStyle("L", true)).toBe("background:var(--rn-l-bg);color:var(--rn-l-fg);border:var(--rn-l-bd)");
    expect(boxStyle("D", true)).toBe("background:var(--rn-d-bg);color:var(--rn-d-fg);border:var(--rn-d-bd)");
    expect(boxStyle("E", true)).toBe("background:var(--r-e-bg);color:var(--r-e-fg);border:var(--r-e-bd)");
  });
});
