// DEV-only demo data. Real valorant-api UUIDs for agents, player cards, maps and tiers;
// invented players. Shapes follow src/lib/types.ts exactly (tri-state fields: absent = loading).
import type { Last5Entry, LiveState, Peak, PlayerCard } from "../types";

const HOUR = 3_600_000;
const DAY = 24 * HOUR;

export const AGENT = {
  jett: "add6443a-41bd-e414-f6ad-e58d267f4e95",
  sova: "320b2a48-4d9b-a075-30f1-1f93a9b638fa",
  omen: "8e253930-4c05-31dd-1b6c-968525494517",
  killjoy: "1e58de9c-4950-5125-93e9-a0aee9f98746",
  skye: "6f2a04ca-43e0-be17-7f36-b3908627744d",
  raze: "f94c3b30-42be-e959-889c-5aa313dba261",
  clove: "1dbf2edd-4729-0984-3115-daa5eed44993",
  cypher: "117ed9e3-49f3-6512-3ccf-0cada7e3823b",
  neon: "bb2a4828-46eb-8cd1-e765-15848195d751",
  viper: "707eab51-4836-f488-046a-cda6bf494859",
} as const;

const CARD = {
  reyna: "1fb0bee0-49db-fb51-b090-bc834babdb2b",
  haven: "92c8a5c7-4ec3-5ce6-73c2-21937056cd77",
  sage: "7c2bc54d-4f20-3264-1497-03a0a1a506e1",
  omen: "50ee5414-45cc-85f6-2ec8-cb96d86f6039",
  viper: "6380e65b-4685-904d-1e80-4ab266493bae",
  killjoy: "55f584f9-4684-895d-88e0-a8ada2002d8a",
  sova: "52995983-424c-9517-2fe8-3795e092edb8",
  versus: "3432dc3d-47da-4675-67ae-53adb1fdad5e",
  neon: "a3047113-49d8-a402-393d-bb89c0bf0237",
  fade: "e0d4094a-4ae9-ba8c-5a65-dc8cafbc7b96",
  chamber: "532ea15e-4851-1a56-9226-638282f76472",
  skye: "9397e078-4140-cc2b-4fcd-b0afedb9ece8",
  yoru: "20193915-488b-e8dc-edb8-7cbfe7a99b3c",
  clove: "e9368c54-41ba-5d07-f9aa-7f98650e9885",
  gekko: "4e2283eb-44c4-dd2e-c0a3-9fbcf5c574fc",
  ascent: "a40f0146-4420-645d-6d53-73a362900af0",
} as const;

export const MAP = {
  haven: "2bee0dc9-4ffe-519b-1cbd-7fbe763a6047",
  ascent: "7eaecc1b-4337-bbf6-6ab9-04b8f06b3319",
} as const;

export const LONGEST_NAME = "WWWWWWWWWWWWWWWW";
export const LONGEST_TAG = "WWWWW";

/** Acts from valorant-api /v1/seasons (end dates are real). */
function act(label: string, actName: string, endIso: string | null, tier: number, now: number): Peak {
  return { tier, actLabel: label, actName, sinceActEndMs: endIso === null ? null : now - Date.parse(endIso) };
}

/** "W+18@2h" style: result, RR change, hours ago. Newest first. */
function last5(now: number, spec: string): Last5Entry[] {
  return spec.split(" ").map((s) => {
    const m = /^([WL])([+-]\d+)@(\d+)([hd])$/.exec(s);
    if (!m) throw new Error(`bad last5 spec ${s}`);
    const ago = Number(m[3]) * (m[4] === "h" ? HOUR : DAY);
    return { win: m[1] === "W", rrEarned: Number(m[2]), startMs: now - ago };
  });
}

type Base = Omit<PlayerCard, "team" | "isSelfParty">;

function players(now: number) {
  const self: Base = {
    puuid: "0c5a3b8e-7a51-5d0e-9b8f-3f6c2c1e9a01",
    name: { kind: "visible", name: "Falcon", tag: "2211" },
    level: 214,
    cardId: CARD.reyna,
    agentId: null,
    rank: { tier: 14, rr: 64 },
    prevActTier: 13,
    peak: act("E9 A3", "Episode 9 · Act III (23 Oct 2024 – 8 Jan 2025)", "2025-01-08T00:00:00Z", 15, now),
    kd: 1.18,
    hsPct: 0.231,
    actWinrate: 0.545,
    last5: last5(now, "W+18@2h L-16@3h W+21@5h W+19@1d L-14@1d"),
    party: null,
    encounters: null,
  };

  const party: Base[] = [
    {
      puuid: "5f1d7e2a-2b0c-5a8e-a3d1-7c9e4b2f6a12",
      name: { kind: "visible", name: "Sandstorm", tag: "DXB" },
      level: 388,
      cardId: CARD.haven,
      agentId: null,
      rank: { tier: 16, rr: 18 },
      prevActTier: 16,
      peak: act("V25 A6", "V25 · Act VI (15 Oct 2025 – 7 Jan 2026)", "2026-01-07T00:00:00Z", 18, now),
      kd: 1.31,
      hsPct: 0.268,
      actWinrate: 0.58,
      last5: last5(now, "W+22@2h L-17@3h W+20@5h W+24@1d W+19@2d"),
      party: null,
      encounters: null,
    },
    {
      puuid: "a93e41c7-8d2f-5b16-9c0e-1f7a3d5b8e23",
      name: { kind: "visible", name: "Mubarak", tag: "777" },
      level: 97,
      cardId: CARD.sage,
      agentId: null,
      rank: { tier: 11, rr: 41 },
      prevActTier: 10,
      peak: act("V26 A5", "V26 · Act V (19 Aug – 14 Oct 2026)", null, 11, now),
      kd: 0.94,
      hsPct: 0.172,
      actWinrate: 0.5,
      last5: last5(now, "L-19@4h W+17@6h L-21@1d W+20@2d L-18@2d"),
      party: null,
      encounters: null,
    },
    {
      puuid: "d2b8f0a4-3c6e-5e71-8a9d-6b0c2e4f1a34",
      name: { kind: "visible", name: "noor", tag: "nr1" },
      level: 156,
      cardId: CARD.skye,
      agentId: null,
      rank: { tier: 12, rr: 7 },
      prevActTier: 12,
      peak: act("E8 A3", "Episode 8 · Act III (30 Apr – 26 Jun 2024)", "2024-06-26T00:00:00Z", 14, now),
      kd: 1.02,
      hsPct: 0.194,
      actWinrate: 0.476,
      last5: last5(now, "W+15@1d L-22@1d L-20@3d W+18@4d W+16@6d"),
      party: null,
      encounters: null,
    },
    {
      puuid: "7e4c9a1b-6f3d-5c28-b0e5-9a8d1c3f2b45",
      name: { kind: "visible", name: "Kaizen", tag: "EU1" },
      level: 512,
      cardId: CARD.chamber,
      agentId: null,
      rank: { tier: 18, rr: 88 },
      prevActTier: 19,
    },
  ];

  const allies: Base[] = [
    {
      puuid: "3b6f8e2d-1a4c-5d93-8e7f-0c2b5a9d4e56",
      name: { kind: "visible", name: "velvet echo", tag: "2468" },
      level: 301,
      cardId: CARD.neon,
      agentId: AGENT.jett,
      rank: { tier: 15, rr: 92 },
      prevActTier: 14,
      peak: act("V26 A4", "V26 · Act IV (24 Jun – 19 Aug 2026)", "2026-08-19T00:00:00Z", 16, now),
      kd: 19,
      hsPct: 0.312,
      actWinrate: 0.611,
      last5: last5(now, "W+23@1d W+21@1d W+19@2d W+24@3d L-12@3d"),
      party: { group: 0, confirmed: false },
      encounters: { with: 2, against: 1 },
    },
    {
      puuid: "9c1e5a7f-4b2d-5f08-a6c3-2d8e0b7f5a67",
      name: { kind: "hiddenPlayer" },
      level: null,
      cardId: CARD.omen,
      agentId: null,
      party: { group: 0, confirmed: false },
      encounters: null,
    },
    {
      puuid: "e8a2c6f0-5d3b-5a19-b7e4-3f9c1d8a6b78",
      name: { kind: "remembered", name: "Tariq", tag: "404" },
      level: 143,
      cardId: CARD.sova,
      agentId: AGENT.sova,
      rank: { tier: 13, rr: 55 },
      prevActTier: 14,
      peak: act("V25 A6", "V25 · Act VI (15 Oct 2025 – 7 Jan 2026)", "2026-01-07T00:00:00Z", 15, now),
      kd: 1.07,
      hsPct: 0.205,
      actWinrate: 0.52,
      last5: last5(now, "L-18@5h W+19@8h W+17@1d L-20@2d W+18@4d"),
      party: null,
      encounters: { with: 3, against: 0 },
    },
  ];

  const hiddenAllyLoaded: Base = {
    ...allies[1],
    name: { kind: "hiddenAgent", agentId: AGENT.omen },
    agentId: AGENT.omen,
    rank: { tier: 15, rr: 33 },
    prevActTier: 15,
    peak: act("V26 A5", "V26 · Act V (19 Aug – 14 Oct 2026)", null, 15, now),
    kd: 0.88,
    hsPct: 0.164,
    actWinrate: 0.44,
    last5: last5(now, "W+20@1d W+18@1d L-23@2d L-21@2d W+17@5d"),
  };

  const enemies: Base[] = [
    {
      puuid: "1f7d3b9e-8c5a-5b42-9e0d-4a6c2f8b1d89",
      name: { kind: "visible", name: "Kaito", tag: "JP1" },
      level: 677,
      cardId: CARD.yoru,
      agentId: AGENT.neon,
      rank: { tier: 25, rr: 187 },
      prevActTier: 26,
      peak: act("E6 A1", "Episode 6 · Act I (10 Jan – 7 Mar 2023)", "2023-03-07T00:00:00Z", 27, now),
      kd: 1.46,
      hsPct: 0.338,
      actWinrate: 0.563,
      last5: last5(now, "W+14@6h W+12@7h L-16@1d W+13@1d W+15@2d"),
      party: { group: 1, confirmed: true },
      encounters: { with: 0, against: 2 },
    },
    {
      puuid: "6a0c8e4f-2d7b-5e13-b9a5-8d1f3c6e0a90",
      name: { kind: "visible", name: "shiro", tag: "JP1" },
      level: 245,
      cardId: CARD.clove,
      agentId: AGENT.viper,
      rank: { tier: 23, rr: 71 },
      prevActTier: 22,
      peak: act("V26 A5", "V26 · Act V (19 Aug – 14 Oct 2026)", null, 23, now),
      kd: 1.12,
      hsPct: 0.247,
      actWinrate: 0.534,
      last5: last5(now, "W+19@6h W+18@7h L-15@1d L-17@1d W+21@2d"),
      party: { group: 1, confirmed: true },
      encounters: null,
    },
    {
      puuid: "c4e0a8b2-9f6d-5c37-a1e8-5b3d7f9c2e01",
      name: { kind: "hiddenAgent", agentId: AGENT.raze },
      level: null,
      cardId: CARD.versus,
      agentId: AGENT.raze,
      rank: { tier: 20, rr: 46 },
      prevActTier: 19,
      peak: act("V25 A2", "V25 · Act II (5 Mar – 30 Apr 2025)", "2025-04-30T00:00:00Z", 21, now),
      kd: 1.24,
      hsPct: 0.291,
      actWinrate: 0.571,
      last5: last5(now, "W+17@3h L-19@4h W+18@4h W+16@1d W+20@2d"),
      party: { group: 2, confirmed: false },
      encounters: { with: 1, against: 1 },
    },
    {
      puuid: "b5d1f9c3-0e7a-5d48-8b2f-6c4e0a8d3f12",
      name: { kind: "remembered", name: "Layla", tag: "KWT" },
      level: 188,
      cardId: CARD.fade,
      agentId: AGENT.killjoy,
      rank: { tier: 21, rr: 12 },
      prevActTier: 20,
      peak: act("V26 A4", "V26 · Act IV (24 Jun – 19 Aug 2026)", "2026-08-19T00:00:00Z", 21, now),
      kd: 0.97,
      hsPct: 0.226,
      actWinrate: 0.49,
      last5: last5(now, "W+16@3h L-20@4h W+17@4h L-21@1d L-19@1d"),
      party: { group: 2, confirmed: false },
      encounters: { with: 0, against: 4 },
    },
    {
      puuid: "f2a6c0e4-1b8d-5e59-9c3a-7d5f1b9e4a23",
      name: { kind: "visible", name: "newbie", tag: "fresh" },
      level: 21,
      cardId: CARD.gekko,
      agentId: AGENT.clove,
      rank: null,
      prevActTier: null,
      peak: null,
      kd: 0.62,
      hsPct: 0.118,
      actWinrate: null,
      last5: [],
      party: null,
      encounters: null,
    },
  ];

  return { self, party, allies, hiddenAllyLoaded, enemies };
}

const ally = (p: Base, isSelfParty = false): PlayerCard => ({ ...p, team: "ally", isSelfParty });
const enemy = (p: Base): PlayerCard => ({ ...p, team: "enemy", isSelfParty: false });

export const SCENES = [
  "closed",
  "lobby1",
  "lobby2",
  "lobby3",
  "lobby4",
  "lobby5",
  "pregame",
  "ingame",
  "ingame-longnames",
] as const;

export type SceneName = (typeof SCENES)[number];

export function isScene(s: string | null): s is SceneName {
  return s !== null && (SCENES as readonly string[]).includes(s);
}

/** A complete LiveState for one demo scene. */
export function buildScene(scene: SceneName, now: number = Date.now()): LiveState {
  const p = players(now);
  const lobby = (n: number): LiveState => ({
    stage: "menus",
    mapId: null,
    players: [ally(p.self, true), ...p.party.slice(0, n - 1).map((m) => ally(m, true))],
  });

  switch (scene) {
    case "closed":
      return { stage: "closed", mapId: null, players: [ally(p.self, true)] };
    case "lobby1":
      return lobby(1);
    case "lobby2":
      return lobby(2);
    case "lobby3":
      return lobby(3);
    case "lobby4":
      return lobby(4);
    case "lobby5":
      return lobby(5);
    case "pregame":
      return {
        stage: "pregame",
        mapId: MAP.haven,
        players: [
          ally({ ...p.self, agentId: AGENT.killjoy }, true),
          ally({ ...p.party[0], agentId: null }, true),
          ally(p.allies[0]),
          ally(p.allies[1]),
          ally({ ...p.allies[2], agentId: null }),
        ],
      };
    case "ingame":
    case "ingame-longnames": {
      const allies = [
        ally({ ...p.self, agentId: AGENT.killjoy }, true),
        ally({ ...p.party[0], agentId: AGENT.cypher }, true),
        ally(p.allies[0]),
        ally(p.hiddenAllyLoaded),
        ally(p.allies[2]),
      ];
      const enemies = p.enemies.map(enemy);
      const state: LiveState = { stage: "ingame", mapId: MAP.haven, players: [...allies, ...enemies] };
      if (scene === "ingame-longnames") {
        const long = { name: LONGEST_NAME, tag: LONGEST_TAG };
        for (const i of [0, 2, 5, 6]) state.players[i] = { ...state.players[i], name: { kind: "visible", ...long } };
        state.players[8] = { ...state.players[8], name: { kind: "remembered", ...long } };
      }
      return state;
    }
  }
}

/** Every card variant the gallery needs, including the longest Riot ID. */
export function galleryCards(now: number = Date.now()): PlayerCard[] {
  const ingame = buildScene("ingame", now).players;
  const long = buildScene("ingame-longnames", now).players;
  const lobby = buildScene("lobby5", now).players;
  const pregame = buildScene("pregame", now).players;
  return [...ingame, long[0], long[5], long[8], lobby[4], pregame[3]];
}
