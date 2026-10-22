// DEV-only. The design's mock data (v8:427-497) and its derivations (v8:558-692), ported to the wire types
// and anchored at DEMO_NOW. This is the only file that names the demo roster, so a scene bundle that
// leaks into `vite build` is caught by grepping dist for `VesperUnbreakabl` / `Vantablack`.
import type {
  DisplayName,
  EncounterDetailRow,
  Loadout,
  MatchDetailsView,
  MatchSummary,
  PlayerCard,
  PlayerDetails,
  ScoreRow,
  TeammateSummary,
  WeaponSkin,
} from "../types";
import { acsOf, card, last5, opposite, peak, recent, RESULT, type Act, type Letter } from "./fixtures";

/** The page clock the Playwright suite freezes at (spec §8.1). At this time the design's 45m / 2h / 4h rows agree with its clock times. */
export const DEMO_NOW = Date.parse("2026-09-29T21:59:00Z");

const MIN = 60_000;
const DAY = 86_400_000;
const at = (m: number, d: number, h = 0, mi = 0, y = 2026) => Date.UTC(y, m, d, h, mi);

export const AG = {
  Jett: "add6443a-41bd-e414-f6ad-e58d267f4e95",
  Omen: "8e253930-4c05-31dd-1b6c-968525494517",
  Sova: "320b2a48-4d9b-a075-30f1-1f93a9b638fa",
  Killjoy: "1e58de9c-4950-5125-93e9-a0aee9f98746",
  Brimstone: "9f0d8ba9-4140-b941-57d3-a7ad57c6b417",
  Reyna: "a3bfb853-43b2-7238-a4f1-ad90e9e46bcc",
  Breach: "5f8d3a7f-467b-97f3-062c-13acf203c006",
  Viper: "707eab51-4836-f488-046a-cda6bf494859",
  Raze: "f94c3b30-42be-e959-889c-5aa313dba261",
  Cypher: "117ed9e3-49f3-6512-3ccf-0cada7e3823b",
  Clove: "1dbf2edd-4729-0984-3115-daa5eed44993",
  Sage: "569fdd95-4d10-43ab-ca70-79becc718b46",
  Skye: "6f2a04ca-43e0-be17-7f36-b3908627744d",
  Fade: "dade69b4-4f5a-8528-247b-219e5a1facd6",
} as const;
type Agent = keyof typeof AG;

export const MAPS = {
  Ascent: "7eaecc1b-4337-bbf6-6ab9-04b8f06b3319",
  Split: "d960549e-485c-e861-8d71-aa9d1aed12a2",
  Bind: "2c9d57ec-4431-9c5e-2939-8f9ef6dd5cba",
  Lotus: "2fe4ed3a-450a-948b-6d6b-e89a78e680a9",
  Sunset: "92584fbe-486a-b1b2-9faa-39b0f486b498",
  Pearl: "fd267378-4d1d-484f-ff52-77821ed10dc2",
  Icebox: "e2ad5c54-4114-a870-9641-8ea21279579a",
  Haven: "2bee0dc9-4ffe-519b-1cbd-7fbe763a6047",
} as const;
type MapName = keyof typeof MAPS;

const CARDS = [
  "40638788-4b84-b2b4-0581-b6832676a567",
  "3ab51388-48a4-5de7-488d-aea7c14f8ce4",
  "1a0a55c6-40fe-c28a-1e48-46a954a69029",
  "36377e6d-4be8-a585-f4a6-4fbdbb697324",
  "fb0d94ab-4186-389d-6e8d-439c45e00aa9",
  "8de9de55-4e26-94e4-bdba-d790b1bd9b34",
  "e17c2e94-44fb-9486-8497-9dab8b942b3d",
  "1711d20d-4b1c-c64a-14be-d4ae58a457c6",
  "24b2ab94-47bc-0af2-507c-b5b7fd13aeed",
  "89605a51-4778-0f1d-067a-22acea6d19ff",
  "e5ab0071-4603-b189-5664-7b8802d2fec0",
  "9fb348bc-41a0-91ad-8a3e-818035c4e561",
];

// ── acts (v8:434-440) ──────────────────────────────────────────────────────────
export const ACT = {
  "V26 A5": { label: "V26 A5", name: "V26 ACT V", startMs: at(7, 26), endMs: at(9, 21) },
  "V26 A4": { label: "V26 A4", name: "V26 ACT IV", startMs: at(5, 24), endMs: at(7, 26) },
  "V26 A2": { label: "V26 A2", name: "V26 ACT II", startMs: at(2, 4), endMs: at(3, 29) },
  "V26 A1": { label: "V26 A1", name: "V26 ACT I", startMs: at(0, 7), endMs: at(2, 4) },
  "V25 A6": { label: "V25 A6", name: "V25 ACT VI", startMs: at(10, 12, 0, 0, 2025), endMs: at(0, 7) },
  "V25 A3": { label: "V25 A3", name: "V25 ACT III", startMs: at(5, 24, 0, 0, 2025), endMs: at(7, 20, 0, 0, 2025) },
  "E9 A3": { label: "E9 A3", name: "EPISODE 9 ACT III", startMs: at(9, 22, 0, 0, 2024), endMs: at(0, 9, 0, 0, 2025) },
  // Only in `ingame-longnames`: the design's 13-character peak text is `V26 A4 · 11mo`, this is the 14-character one.
  "E10 A3": { label: "E10 A3", name: "EPISODE 10 ACT III", startMs: at(2, 5, 0, 0, 2025), endMs: at(4, 1, 0, 0, 2025) },
} satisfies Record<string, Act>;
export type ActKey = keyof typeof ACT;

/**
 * "Time since that act ended" as the design prints it. The design's strings are hand-written, and one of
 * them (`V26 A1 · 7mo`) is not what the act dates give (6mo), so these are the printed values, not derived.
 */
export const SINCE = { "1mo": 35 * DAY, "5mo": 155 * DAY, "7mo": 215 * DAY, "8mo": 265 * DAY, "11mo": 340 * DAY, "1y": 420 * DAY };

// ── the roster (v8:475-486) ────────────────────────────────────────────────────
type Raw = {
  id: string;
  name?: string;
  tag?: string;
  agent: Agent;
  lv: number;
  cur: [number, number];
  peak: [number, ActKey | "This Act", (keyof typeof SINCE)?];
  prev: number | null;
  kd: number;
  hs: number;
  win: [number, number, number];
  last: string;
  enc?: [number, number];
  party?: [number, boolean];
  hidden?: boolean;
  known?: boolean;
  card: number;
  self?: boolean;
};

const MINE_RAW: Raw[] = [
  { id: "fahad", name: "Fahad", tag: "OPD1", agent: "Jett", lv: 312, cur: [21, 21], peak: [24, "V26 A4", "1mo"], prev: 22, kd: 1.42, hs: 27, win: [58, 31, 22], last: "WLWWL", self: true, card: 0 },
  { id: "sable", name: "Sable", tag: "1177", agent: "Omen", lv: 204, cur: [20, 64], peak: [22, "V25 A6", "8mo"], prev: 19, kd: 1.08, hs: 22, win: [52, 22, 20], last: "WWLLW", card: 1 },
  { id: "vesper", name: "VesperUnbreakabl", tag: "EUW7", agent: "Sova", lv: 156, cur: [19, 33], peak: [21, "E9 A3", "1y"], prev: 20, kd: 0.97, hs: 19, win: [49, 19, 20], last: "LWLDW", enc: [1, 0], card: 2 },
  { id: "drake", name: "Drake", tag: "5520", agent: "Killjoy", lv: 289, cur: [21, 88], peak: [22, "V26 A2", "5mo"], prev: 20, kd: 1.21, hs: 24, win: [55, 27, 22], last: "WLLWW", party: [0, true], card: 3 },
  { id: "nova", name: "Nova", tag: "3341", agent: "Brimstone", lv: 341, cur: [22, 12], peak: [22, "This Act"], prev: null, kd: 1.33, hs: 26, win: [61, 25, 16], last: "WLWWW", party: [0, true], card: 4 },
];

const THEIRS_RAW: Raw[] = [
  { id: "mara", name: "Mara", tag: "7788", agent: "Reyna", lv: 402, cur: [24, 19], peak: [25, "V26 A4", "1mo"], prev: 24, kd: 1.52, hs: 29, win: [63, 41, 24], last: "WWLWL", enc: [1, 2], party: [1, false], card: 5 },
  { id: "kilo", name: "Kilo", tag: "2210", agent: "Breach", lv: 333, cur: [23, 47], peak: [24, "V25 A6", "8mo"], prev: 24, kd: 1.12, hs: 21, win: [54, 30, 26], last: "LWLWL", enc: [0, 1], party: [1, false], card: 6 },
  { id: "zephyr", name: "Zephyr", tag: "6640", agent: "Viper", lv: 277, cur: [21, 52], peak: [23, "V25 A3", "1y"], prev: 20, kd: 1.18, hs: 23, win: [56, 28, 22], last: "LDWLW", card: 7 },
  { id: "rook", name: "Rook", tag: "4413", agent: "Raze", lv: 221, cur: [20, 71], peak: [20, "E9 A3", "1y"], prev: 18, kd: 0.94, hs: 18, win: [49, 20, 21], last: "LWWWL", enc: [0, 1], hidden: true, known: true, card: 8 },
  { id: "cypher", agent: "Cypher", lv: 198, cur: [21, 27], peak: [22, "V26 A1", "7mo"], prev: 21, kd: 1.04, hs: 20, win: [67, 2, 1], last: "WLW", hidden: true, card: 11 },
];

const ROSTER_RAW = [...MINE_RAW, ...THEIRS_RAW];
const rawById = (id: string) => ROSTER_RAW.find((r) => r.id === id);

/** Valid v4-shaped ids (the Rust side checks `is_uuid`), stable across runs. */
const uuid = (prefix: string, n: number) => `${prefix}0000000-0000-4000-8000-${n.toString(16).padStart(12, "0")}`;
export const PUUID = Object.fromEntries(ROSTER_RAW.map((r, i) => [r.id, uuid("a", i + 1)])) as Record<string, string>;

const REMEMBERED_AT = at(7, 30, 22, 31);

// ── per-match data (v8:453-473) ────────────────────────────────────────────────
const W_RR = [18, 16, 21, 17, 22, 15, 19, 24];
const L_RR = [15, 19, 13, 17, 21, 14];
export const rrFor = (l: Letter, i: number): number => (l === "W" ? W_RR[i % W_RR.length]! : l === "L" ? -L_RR[i % L_RR.length]! : 0);

const SCORES: Record<Letter, string[]> = {
  W: ["13–9", "13–11", "13–6", "14–12", "13–4", "13–10", "13–7", "13–8"],
  L: ["9–13", "11–13", "7–13", "10–13", "12–14", "8–13", "6–13", "11–13"],
  D: ["13–13"],
};
const rounds = (l: Letter, i: number): [number, number] => {
  const pool = SCORES[l];
  const [a, b] = pool[i % pool.length]!.split("–").map(Number);
  return [a!, b!];
};

const KDA: Array<[number, number, number]> = [[24, 14, 5], [19, 15, 7], [16, 17, 3], [21, 10, 4], [18, 17, 6], [26, 19, 8], [19, 8, 3], [12, 16, 5]];
const HS = [31, 27, 22, 34, 25, 29, 36, 19];
/** "Sep 29, 2026 · 9:14 PM · 38 min" …: the clock times and durations of the design's 8 recent rows. */
const WHEN: Array<[number, number, number, number, number]> = [
  [8, 29, 21, 14, 38], [8, 29, 19, 2, 44], [8, 29, 17, 20, 35], [8, 28, 23, 41, 31],
  [8, 28, 22, 5, 47], [8, 27, 20, 30, 52], [8, 26, 22, 12, 26], [8, 26, 20, 48, 33],
];
const START = WHEN.map(([m, d, h, mi]) => at(m, d, h, mi));
const DURATION = WHEN.map((w) => w[4] * MIN);
const RMAPS: MapName[] = ["Lotus", "Sunset", "Split", "Haven", "Icebox", "Bind", "Pearl", "Ascent"];

// ── cards ──────────────────────────────────────────────────────────────────────
function displayName(r: Raw): DisplayName {
  if (!r.hidden) return { kind: "visible", name: r.name!, tag: r.tag! };
  if (r.known) return { kind: "remembered", name: r.name!, tag: r.tag!, source: "ownMatch", fromMapId: MAPS.Split, fromStartMs: REMEMBERED_AT };
  return { kind: "hiddenAgent", agentId: AG[r.agent] };
}

function toCard(r: Raw, team: "ally" | "enemy"): PlayerCard {
  const [pTier, pAct, pWhen] = r.peak;
  const act = ACT[pAct === "This Act" ? "V26 A5" : pAct];
  // The design's first number is the rounded win rate; the card carries wins and games, and the rate follows from them.
  const [, wins, losses] = r.win;
  return card({
    puuid: PUUID[r.id]!,
    name: displayName(r),
    level: r.lv,
    cardId: CARDS[r.card]!,
    agentId: AG[r.agent],
    rank: { tier: r.cur[0], rr: r.cur[1] },
    prevActTier: r.prev,
    peak: peak(act, pTier, pAct === "This Act" ? null : SINCE[pWhen!]),
    kd: r.kd,
    hsPct: r.hs / 100,
    actWinrate: wins / (wins + losses),
    actWins: wins,
    actGames: wins + losses,
    last5: last5(r.last, START, rrFor),
    party: r.party ? { group: r.party[0], confirmed: r.party[1] } : null,
    encounters: r.enc ? { with: r.enc[0], against: r.enc[1] } : null,
    team,
    isSelf: !!r.self,
  });
}

/** The design's In Game roster: 5 allies (Fahad first), then 5 enemies. Fresh objects on every call. */
export function roster(): PlayerCard[] {
  return [...MINE_RAW.map((r) => toCard(r, "ally")), ...THEIRS_RAW.map((r) => toCard(r, "enemy"))];
}

// ── Player Details (v8:583-611) ────────────────────────────────────────────────
const SKIN_TIER = {
  Select: "12683d76-48d7-84a3-4e09-6985794f0445",
  Deluxe: "0cebb8be-46d7-c12a-d306-e9907bfc5a25",
  Premium: "60bca009-4182-7998-dee7-b8a2558dc369",
  Exclusive: "e046854e-406c-37f4-6607-19a9ba8426fc",
  Ultra: "411e4a55-4e59-7757-41f0-86a53f101bb5",
} as const;

/** All four weapons, each a real skin. */
export function loadout(): Record<keyof Loadout, WeaponSkin> {
  const w = (chromaId: string, skinName: string, chromaName: string, tier: keyof typeof SKIN_TIER): WeaponSkin => ({
    chromaId,
    skinName,
    chromaName,
    tierId: SKIN_TIER[tier],
  });
  return {
    vandal: w("b2a065c0-4632-ccaa-f496-7681dc2a6185", "Reaver", "Red", "Premium"),
    phantom: w("6f9c7109-485a-f2a3-cb1d-9f9a31a995d7", "Oni", "Hana", "Premium"),
    operator: w("37cca29b-4468-a01e-e31b-8f8978a81eef", "Elderflame", "Blue", "Ultra"),
    sheriff: w("2bad99e6-4984-bfc9-5b12-76a01735223e", "Kuronami", "White", "Exclusive"),
  };
}

type Recent = {
  i: number;
  letter: Letter;
  map: MapName;
  agent: Agent;
  tier: number;
  rounds: [number, number];
  kda: [number, number, number];
  hs: number;
  rr: number;
  startMs: number;
  durationMs: number;
};

/** The player's recent rows: their last letters, then W W L, up to 8 (v8:583). */
function recentData(r: Raw): Recent[] {
  return [...(r.last + "WWL")].slice(0, 8).map((ch, i) => {
    const letter = ch as Letter;
    return {
      i,
      letter,
      map: RMAPS[i]!,
      agent: i === 2 || i === 6 ? "Clove" : r.agent,
      tier: Math.max(3, i < 4 ? r.cur[0] : r.cur[0] - 1),
      rounds: rounds(letter, i),
      kda: KDA[i]!,
      hs: HS[i]!,
      rr: rrFor(letter, i),
      startMs: START[i]!,
      durationMs: DURATION[i]!,
    };
  });
}

const rid = (id: string, i: number) => `mdemo:${id}:r${i}`;
const eid = (id: string, i: number) => `mdemo:${id}:e${i}`;

type Enc = { map: MapName; startMs: number; side: "With" | "Against"; them: [Agent, number]; me: [Agent, number]; t: Letter; score: string };

const MARA_ENC: Enc[] = [
  { map: "Lotus", startMs: at(8, 12, 23, 18), side: "Against", them: ["Reyna", 24], me: ["Jett", 21], t: "L", score: "10–13" },
  { map: "Bind", startMs: at(7, 30, 21, 40), side: "With", them: ["Clove", 23], me: ["Omen", 21], t: "W", score: "13–7" },
  { map: "Ascent", startMs: at(7, 21, 20, 5), side: "Against", them: ["Reyna", 23], me: ["Jett", 20], t: "W", score: "13–11" },
];
const ENC_BASE: Array<Pick<Enc, "map" | "startMs" | "t" | "score">> = [
  { map: "Split", startMs: at(7, 30, 22, 31), t: "W", score: "13–10" },
  { map: "Pearl", startMs: at(7, 14, 19, 52), t: "L", score: "8–13" },
  { map: "Icebox", startMs: at(6, 29, 23, 5), t: "W", score: "13–5" },
];
const ENC_MIN = [41, 29, 36];

function encData(r: Raw): Enc[] {
  if (r.id === "mara") return MARA_ENC;
  const [w, a] = r.enc ?? [0, 0];
  const sides: Array<"With" | "Against"> = [...Array<"Against">(a).fill("Against"), ...Array<"With">(w).fill("With")];
  return sides.map((side, i) => ({ ...ENC_BASE[i % 3]!, side, them: [r.agent, r.cur[0]], me: ["Jett", 21] }));
}

// ── people who are not on the live roster (v8:490-497) ─────────────────────────
type Pool = [name: string | null, tag: string, agent: Agent, tier: number];
const POOL: Pool[] = [
  ["Kilo", "2210", "Breach", 23], ["Vantablack", "0001", "Omen", 22], ["Ishi", "KR1", "Sova", 22], ["Fennec", "4402", "Killjoy", 21],
  ["Tarek", "0552", "Viper", 21], ["Drift", "9981", "Raze", 22], ["Nyx", "EU1", "Clove", 23], ["Halo", "3030", "Sage", 21],
  [null, "", "Cypher", 22], ["Brisk", "7712", "Jett", 24], ["Sol", "4040", "Skye", 22], ["Wren", "1212", "Fade", 21],
];
const SB: Array<[number, number, number]> = [[18, 16, 8], [17, 17, 3], [14, 16, 10], [11, 17, 9], [21, 15, 4], [17, 17, 6], [15, 16, 9], [14, 17, 7], [13, 17, 5], [16, 15, 6], [12, 16, 11], [15, 17, 8]];
const HSP = [21, 25, 19, 18, 27, 22, 20, 17, 23, 24, 16, 21];

/** A pool player who is also on the live roster keeps their live puuid (Kilo). */
const poolPuuid = (j: number) => {
  const [name, tag] = POOL[j]!;
  const live = ROSTER_RAW.find((r) => r.name === name && r.tag === tag);
  return live ? PUUID[live.id]! : uuid("b", j + 1);
};

/** Kova is only ever a Top 3 name, never on a roster or in a pool. */
const KOVA = uuid("c", 1);

/** name, tag, games together, the profile owner's wins and losses in them (a draw is neither). */
type Mate = [string, string, number, number, number];
const MATES: Record<string, Mate[]> = {
  mara: [["Kilo", "2210", 14, 9, 5], ["Vantablack", "0001", 9, 5, 4], ["Ishi", "KR1", 6, 2, 4]],
  fahad: [["Sable", "1177", 13, 9, 4], ["Drake", "5520", 10, 6, 3], ["Kova", "0482", 7, 3, 4]],
};
const MATES_DEFAULT: Mate[] = [["Kova", "0482", 12, 7, 5], ["Tarek", "0552", 8, 4, 4], ["Fennec", "4402", 5, 2, 3]];

/**
 * R93: a player who is hidden on the live screen (their live card's name is not `visible`) shows as that card
 * does in every row that names them, never as the raw roster name. Returns a copy, or null when the live
 * cards do not hide this puuid (or there are no live cards).
 */
function hiddenName(live: readonly PlayerCard[] | undefined, puuid: string): DisplayName | null {
  const n = live?.find((c) => c.puuid === puuid)?.name;
  return n && n.kind !== "visible" ? { ...n } : null;
}

/**
 * R93 in a match row, as the backend builds it (matchview.rs `hidden_style`): a player hidden on the live screen shows
 * by the remembered name when the live card has one, else as the agent of that row (else `Hidden Player`, for a row
 * with no agent). Never the live card's `hiddenPlayer` or its live agent. null when the live cards do not hide them.
 */
function hiddenRowName(live: readonly PlayerCard[] | undefined, row: ScoreRow): DisplayName | null {
  const n = hiddenName(live, row.puuid);
  if (!n || n.kind === "remembered") return n;
  return row.agentId ? { kind: "hiddenAgent", agentId: row.agentId } : { kind: "hiddenPlayer" };
}

function mates(r: Raw, live?: readonly PlayerCard[]): TeammateSummary[] {
  return (MATES[r.id] ?? MATES_DEFAULT).map(([name, tag, count, wins, losses]) => {
    const onRoster = ROSTER_RAW.find((x) => x.name === name && x.tag === tag);
    const pool = POOL.findIndex((p) => p[0] === name && p[1] === tag);
    const puuid = onRoster ? PUUID[onRoster.id]! : pool >= 0 ? poolPuuid(pool) : KOVA;
    return { puuid, name: hiddenName(live, puuid) ?? { kind: "visible", name, tag }, count, wins, losses };
  });
}

/**
 * Everything Player Details shows for a roster player, as fresh objects on every call. `c` is the card as the
 * scene has it (copied); `live` are the scene's live cards, whose hidden names the Top 3 rows follow (R93).
 */
export function detailsFor(id: string, c?: PlayerCard, live?: readonly PlayerCard[]): PlayerDetails {
  const r = rawById(id);
  if (!r) throw new Error(`demo: no roster player ${id}`);
  const base = c ? structuredClone(c) : roster().find((x) => x.puuid === PUUID[id])!;
  const rec = recentData(r).map((m): MatchSummary =>
    recent({
      matchId: rid(id, m.i), mapId: MAPS[m.map], startMs: m.startMs, letter: m.letter, rounds: m.rounds,
      agentId: AG[m.agent], kda: m.kda, hs: m.hs, rr: m.rr, tier: m.tier,
    }),
  );
  const enc = encData(r).map((e, i): EncounterDetailRow => {
    const [us, them] = e.score.split("–").map(Number);
    return {
      matchId: eid(id, i), mapId: MAPS[e.map], startMs: e.startMs, with: e.side === "With", result: RESULT[e.t],
      roundsWon: us!, roundsLost: them!, durationMs: ENC_MIN[i % 3]! * MIN,
      themAgentId: AG[e.them[0]], themTier: e.them[1], selfAgentId: AG[e.me[0]], selfTier: e.me[1],
    };
  });
  // R116: the hero's K/D and HS% cover up to 20 current-act competitive matches; the demo shows the design's own numbers over them.
  const [, wins, losses] = r.win;
  return {
    card: base,
    recent: rec,
    topTeammates: mates(r, live),
    loadout: loadout(),
    encounters: enc,
    heroKd: r.kd,
    heroHs: r.hs / 100,
    heroMatches: Math.min(20, wins + losses),
  };
}

/** Details for a player with no history at all (`details-empty`): nothing to list, nothing known about the loadout. */
export function emptyDetails(c: PlayerCard): PlayerDetails {
  return {
    card: c,
    recent: [],
    topTeammates: [],
    loadout: { vandal: null, phantom: null, operator: null, sheriff: null },
    encounters: [],
    heroKd: null,
    heroHs: null,
    heroMatches: 0,
  };
}

// ── Match Details (v8:613-660) ─────────────────────────────────────────────────
const nameOf = (name: string | null, tag: string, agent: Agent): DisplayName =>
  name === null ? { kind: "hiddenAgent", agentId: AG[agent] } : { kind: "visible", name, tag };

type RowIn = { puuid: string; name: DisplayName; agent: Agent; tier: number; kda: [number, number, number]; hs: number; isSelf?: boolean; isFocus?: boolean };

function scoreRow(o: RowIn): ScoreRow {
  const [kills, deaths, assists] = o.kda;
  const acs = acsOf(kills, assists);
  return {
    puuid: o.puuid, name: { ...o.name }, agentId: AG[o.agent], tier: o.tier, kills, deaths, assists, acs,
    adr: Math.round(acs * 0.64), hsPct: o.hs / 100, isSelf: !!o.isSelf, isFocus: !!o.isFocus,
  };
}

const byAcs = (a: ScoreRow, b: ScoreRow) => (b.acs ?? -1) - (a.acs ?? -1);

const poolRow = (j: number): RowIn => {
  const [name, tag, agent, tier] = POOL[j]!;
  return { puuid: poolPuuid(j), name: nameOf(name, tag, agent), agent, tier, kda: SB[j]!, hs: HSP[j]! };
};

/** The half-time score: attackers take the first 12 rounds, capped as in v8:651. */
const firstHalf = (us: number, them: number): [number, number] => [Math.min(12, Math.ceil(us / 2)), Math.min(12, Math.floor(them / 2))];

const FAHAD_ROW: RowIn = {
  puuid: PUUID.fahad!, name: { kind: "visible", name: "Fahad", tag: "OPD1" }, agent: "Jett", tier: 21, kda: [22, 16, 3], hs: 29, isSelf: true,
};

/**
 * The match behind a recent row (`mdemo:<id>:r<i>`, the focus player's own game) or an encounter row
 * (`mdemo:<id>:e<i>`, a game shared with the user: self perspective), or the Deathmatch (`mdemo:<id>:dm`).
 * Throws for any other id. Fresh objects on every call; `live` are the scene's live cards, and a row whose
 * puuid is hidden there shows as its card does, never as the raw roster name (R93).
 */
export function matchFor(matchId: string, live?: readonly PlayerCard[]): MatchDetailsView {
  const view = buildMatch(matchId);
  for (const t of view.teams) for (const p of t.players) p.name = hiddenRowName(live, p) ?? p.name;
  return view;
}

function buildMatch(matchId: string): MatchDetailsView {
  if (/^mdemo:[a-z]+:dm$/.test(matchId)) return deathmatch(matchId);
  const sm = SESSION_ID.exec(matchId);
  if (sm) return sessionMatch(matchId, Number(sm[1]), sm[2] as Letter, sm[3]!);
  const m = /^mdemo:([a-z]+):([re])(\d)$/.exec(matchId);
  const r = m && rawById(m[1]!);
  if (!m || !r) throw new Error(`demo: unknown match ${matchId}`);
  const i = Number(m[3]);
  return m[2] === "r" ? recentMatch(r, i, matchId) : encounterMatch(r, i, matchId);
}

function teamsOf(a: ScoreRow[], b: ScoreRow[], ra: [number, number], letter: Letter): MatchDetailsView["teams"] {
  const [us, them] = ra;
  const [h1a, h1b] = firstHalf(us, them);
  return [
    { teamId: "Blue", result: RESULT[letter], roundsWon: us, firstHalf: h1a, players: [...a].sort(byAcs) },
    { teamId: "Red", result: RESULT[opposite(letter)], roundsWon: them, firstHalf: h1b, players: [...b].sort(byAcs) },
  ];
}

function recentMatch(r: Raw, i: number, matchId: string): MatchDetailsView {
  const d = recentData(r)[i];
  if (!d) throw new Error(`demo: ${r.id} has no recent row ${i}`);
  return matchOf(r, d, matchId);
}

/** The user's own game (`recentData`'s shape) as the Match page shows it. */
function matchOf(r: Raw, d: Recent, matchId: string): MatchDetailsView {
  const focus = scoreRow({
    puuid: PUUID[r.id]!, name: displayName(r), agent: d.agent, tier: d.tier, kda: d.kda, hs: d.hs,
    isSelf: !!r.self, isFocus: true,
  });
  // The other nine come from the pool in order, skipping the focus player if they are in it (Kilo).
  const others = POOL.map((_, j) => j).filter((j) => poolPuuid(j) !== PUUID[r.id]);
  const a = [focus, ...others.slice(0, 4).map((j) => scoreRow(poolRow(j)))];
  const b = others.slice(4, 9).map((j) => scoreRow(poolRow(j)));
  return {
    matchId, mapId: MAPS[d.map], queueId: "competitive", startMs: d.startMs, durationMs: d.durationMs,
    perspective: r.self ? "self" : "focus", teams: teamsOf(a, b, d.rounds, d.letter),
  };
}

/** A match of My Stats' demo session that is not one of the user's recent rows: `mdemo:s:<startMs>:<W|L|D>:<map uuid>`. */
const SESSION_ID = /^mdemo:s:(\d+):([WLD]):([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})$/;

/** The id of a demo session match: everything the Match page needs rides in it, so any scene can answer it. */
export const sessionMatchId = (startMs: number, result: Letter, mapId: string): string => `mdemo:s:${startMs}:${result}:${mapId}`;

/**
 * A session match of the user (Fahad): one of the design's own recent rows of the same result supplies the players, the
 * scores and the stats, under the match's own map, start and result (a draw is 13–13), so the page agrees with the chip.
 */
function sessionMatch(matchId: string, startMs: number, letter: Letter, mapId: string): MatchDetailsView {
  const r = rawById("fahad")!;
  const pool = recentData(r).filter((d) => d.letter === (letter === "D" ? "W" : letter));
  const t = pool[Math.floor(startMs / MIN) % pool.length]!;
  const d: Recent = { ...t, letter, rounds: letter === "D" ? [13, 13] : t.rounds, startMs };
  return { ...matchOf(r, d, matchId), mapId };
}

function encounterMatch(r: Raw, i: number, matchId: string): MatchDetailsView {
  const e = encData(r)[i];
  if (!e) throw new Error(`demo: ${r.id} has no encounter ${i}`);
  const [us, them] = e.score.split("–").map(Number) as [number, number];
  const k = KDA[(i + 3) % 8]!;
  const focus = scoreRow({
    puuid: PUUID[r.id]!, name: displayName(r), agent: e.them[0], tier: e.them[1], kda: k, hs: HS[(i + 3) % 8]!, isFocus: true,
  });
  const me = scoreRow(FAHAD_ROW);
  const used = new Set<Agent>([e.me[0], e.them[0]]);
  const idx = POOL.map((_, j) => j).filter((j) => !used.has(POOL[j]![2]) && POOL[j]![0] !== r.name).map((j) => scoreRow(poolRow(j)));
  const withMe = e.side === "With";
  const a = withMe ? [me, focus, ...idx.slice(0, 3)] : [me, ...idx.slice(0, 4)];
  const b = withMe ? idx.slice(3, 8) : [focus, ...idx.slice(4, 8)];
  return {
    matchId, mapId: MAPS[e.map], queueId: "competitive", startMs: e.startMs, durationMs: ENC_MIN[i % 3]! * MIN,
    perspective: "self", teams: teamsOf(a, b, [us, them], e.t),
  };
}

/**
 * A Deathmatch (no teams): one block with an empty team id, no result, no rounds, no halves. Like the backend's
 * (golden match_details_view.json), its rows carry no rank (competitive tier 0) and no ADR. Mara is the focus player.
 */
export function deathmatch(matchId: string): MatchDetailsView {
  const r = rawById("mara")!;
  const kills = [38, 34, 31, 29, 27, 24, 22, 19, 17, 14];
  const rows = kills.map((k, n): ScoreRow => {
    const kda: [number, number, number] = [k, 12 + n, n % 4];
    const row =
      n === 0
        ? scoreRow({ puuid: PUUID.mara!, name: displayName(r), agent: "Reyna", tier: 24, kda, hs: 31, isFocus: true })
        : scoreRow({ ...poolRow(n - 1), kda });
    return { ...row, tier: null, adr: null };
  });
  return {
    matchId, mapId: MAPS.Lotus, queueId: "deathmatch", startMs: at(8, 29, 20, 40), durationMs: 9 * MIN,
    perspective: "focus",
    teams: [{ teamId: "", result: null, roundsWon: null, firstHalf: null, players: rows.sort(byAcs) }],
  };
}

/** The ids of a player's recent-row and encounter-row matches, for scenes that route straight to one. */
export const recentMatchId = rid;
export const encounterMatchId = eid;
