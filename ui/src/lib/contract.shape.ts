// TEST ONLY (Task 16b, R96). One descriptor per wire type, written from the golden JSON that the Rust
// side pins (crates/core/tests/fixtures/contract/*.json), not from types.ts. contract.test.ts walks the
// goldens (and the demo fixtures) with these; contract.test-d.ts holds them against types.ts. Nothing in
// the app imports this file, so `vite build` drops it.
//
// Two differences in form from the plan's interface text (accepted in the Task 16b fix round, for Task 18):
//  - the four `DisplayName` variants are their own export, `displayNameShapes` (keyed by the `kind` tag), not
//    entries of `shapes`: `DisplayName` is a union, so it has no single `ShapeOf<DisplayName>`. The walker picks the
//    variant by `kind`; contract.test-d.ts asserts the variant set equals `DisplayName["kind"]`.
//  - `Desc` has two optional fields beyond `{ kinds, optional?, shape? }`: `enum` (a closed string set) and
//    `fraction` (a 0..1 number). Both are additive, and the type tests pin them.
import type {
  AppInfo,
  AsciiArt,
  DisplayName,
  EncounterCount,
  EncounterDetailRow,
  HistoryPage,
  HistoryRow,
  Insight,
  Last5Entry,
  LiveState,
  Loadout,
  MatchDetailsView,
  MatchSummary,
  MatchTeamView,
  MyStats,
  PartyMark,
  Peak,
  PlayerCard,
  PlayerDetails,
  PlayerMarks,
  Rank,
  RankChange,
  RankSummary,
  RrPoint,
  SavedPlayer,
  ScoreRow,
  SessionMatch,
  SessionSummary,
  Settings,
  SharedRecord,
  StatRow,
  TeammateSummary,
  WeaponSkin,
  WinCell,
} from "./types";

export type Kind = "string" | "number" | "boolean" | "null" | "object" | "array";

/** The closed string sets on the wire. contract.test-d.ts pins each to its TS union. */
export const enums = {
  stage: ["closed", "menus", "pregame", "ingame"],
  result: ["win", "loss", "draw"],
  team: ["ally", "enemy"],
  perspective: ["self", "focus"],
  rememberedSource: ["ownMatch", "otherMatch", "seen"],
  streakMark: ["W", "L", "D"],
  insightKind: ["bestMap", "worstMap", "timeOfDay", "weekdayWeekend", "rrEconomy", "last7Days"],
  smurfLevel: ["off", "cautious", "eager"],
} as const satisfies Record<string, readonly string[]>;

export type Desc = {
  /** Every JSON kind the value takes in the goldens or in the Rust type (`Option`: `null` is a kind). */
  kinds: Kind[];
  /** The key can be absent: R23 tri-state, "still loading" (Rust `Field<T>`, `skip_serializing_if`). */
  optional?: true;
  /** The nested descriptor: for an `object` value itself, for an `array` value each element. */
  shape?: string;
  /** A string value is one of these. */
  enum?: keyof typeof enums;
  /** A number value is a 0..1 fraction (R5, R120), never a percent. */
  fraction?: true;
};

/** Every key of `T`, no extra key: a dropped, renamed or new key on either side fails to compile. */
export type ShapeOf<T> = { [K in keyof T]-?: Desc };

/** Shorthands for the descriptors below. `satisfies` (not an annotation) keeps the literal kinds. */
const str = { kinds: ["string"] } satisfies Desc;
const num = { kinds: ["number"] } satisfies Desc;
const bool = { kinds: ["boolean"] } satisfies Desc;
const strOrNull = { kinds: ["string", "null"] } satisfies Desc;
const numOrNull = { kinds: ["number", "null"] } satisfies Desc;

// `satisfies` (not an annotation) keeps every literal's own type, so `keyof typeof shapes.X` is the exact
// key set the descriptor spells and the type tests compare it with the TS type's (R96).
export const shapes = {
  Rank: { tier: num, rr: numOrNull } satisfies ShapeOf<Rank>,

  Peak: {
    tier: num,
    actLabel: str,
    actName: str,
    sinceActEndMs: numOrNull,
    actStartMs: num,
    actEndMs: num,
  } satisfies ShapeOf<Peak>,

  Last5Entry: { result: { kinds: ["string"], enum: "result" }, rrEarned: numOrNull, startMs: num } satisfies ShapeOf<Last5Entry>,

  PartyMark: { group: num, confirmed: bool } satisfies ShapeOf<PartyMark>,

  EncounterCount: { with: num, against: num } satisfies ShapeOf<EncounterCount>,

  PlayerCard: {
    puuid: str,
    name: { kinds: ["object", "null"], optional: true, shape: "DisplayName" },
    level: { kinds: ["number", "null"], optional: true },
    cardId: str,
    agentId: strOrNull,
    rank: { kinds: ["object", "null"], optional: true, shape: "Rank" },
    prevActTier: { kinds: ["number", "null"], optional: true },
    peak: { kinds: ["object", "null"], optional: true, shape: "Peak" },
    kd: { kinds: ["number", "null"], optional: true },
    hsPct: { kinds: ["number", "null"], optional: true, fraction: true },
    actWinrate: { kinds: ["number", "null"], optional: true, fraction: true },
    actWins: { kinds: ["number", "null"], optional: true },
    actGames: { kinds: ["number", "null"], optional: true },
    last5: { kinds: ["array", "null"], optional: true, shape: "Last5Entry" },
    party: { kinds: ["object", "null"], shape: "PartyMark" },
    encounters: { kinds: ["object", "null"], shape: "EncounterCount" },
    team: { kinds: ["string"], enum: "team" },
    isSelfParty: bool,
    isSelf: bool,
  } satisfies ShapeOf<PlayerCard>,

  LiveState: {
    stage: { kinds: ["string"], enum: "stage" },
    mapId: strOrNull,
    players: { kinds: ["array"], shape: "PlayerCard" },
    settled: bool,
  } satisfies ShapeOf<LiveState>,

  MatchSummary: {
    matchId: str,
    mapId: str,
    startMs: num,
    result: { kinds: ["string"], enum: "result" },
    roundsWon: numOrNull,
    roundsLost: numOrNull,
    agentId: strOrNull,
    // R102: null (never a counted 0) when the match has no stats row for the player.
    kills: numOrNull,
    deaths: numOrNull,
    assists: numOrNull,
    kd: numOrNull,
    acs: numOrNull,
    hsPct: { kinds: ["number", "null"], fraction: true },
    rrEarned: numOrNull,
    tier: numOrNull,
  } satisfies ShapeOf<MatchSummary>,

  WeaponSkin: { chromaId: str, skinName: str, chromaName: strOrNull, tierId: strOrNull } satisfies ShapeOf<WeaponSkin>,

  // R97: per weapon, absent = the skin name is still resolving, null = none or unknown.
  Loadout: {
    vandal: { kinds: ["object", "null"], optional: true, shape: "WeaponSkin" },
    phantom: { kinds: ["object", "null"], optional: true, shape: "WeaponSkin" },
    operator: { kinds: ["object", "null"], optional: true, shape: "WeaponSkin" },
    sheriff: { kinds: ["object", "null"], optional: true, shape: "WeaponSkin" },
  } satisfies ShapeOf<Loadout>,

  EncounterDetailRow: {
    matchId: str,
    mapId: str,
    startMs: num,
    with: bool,
    result: { kinds: ["string"], enum: "result" },
    roundsWon: numOrNull,
    roundsLost: numOrNull,
    durationMs: numOrNull,
    themAgentId: strOrNull,
    themTier: numOrNull,
    selfAgentId: strOrNull,
    selfTier: numOrNull,
  } satisfies ShapeOf<EncounterDetailRow>,

  TeammateSummary: { puuid: str, name: { kinds: ["object"], shape: "DisplayName" }, count: num } satisfies ShapeOf<TeammateSummary>,

  PlayerDetails: {
    card: { kinds: ["object"], shape: "PlayerCard" },
    recent: { kinds: ["array"], shape: "MatchSummary" },
    topTeammates: { kinds: ["array"], shape: "TeammateSummary" },
    loadout: { kinds: ["object"], shape: "Loadout" },
    encounters: { kinds: ["array"], shape: "EncounterDetailRow" },
    // R116 / R120: over the newest <= 20 current-act competitive matches; null when there are none (heroHs also
    // when those matches landed no shot, as Rust's hs_pct returns None).
    heroKd: numOrNull,
    heroHs: { kinds: ["number", "null"], fraction: true },
    heroMatches: num,
  } satisfies ShapeOf<PlayerDetails>,

  ScoreRow: {
    puuid: str,
    name: { kinds: ["object"], shape: "DisplayName" },
    agentId: strOrNull,
    tier: numOrNull,
    // R124: null (never a counted 0/0/0) when the player has no stats row.
    kills: numOrNull,
    deaths: numOrNull,
    assists: numOrNull,
    acs: numOrNull,
    adr: numOrNull,
    hsPct: { kinds: ["number", "null"], fraction: true },
    isSelf: bool,
    isFocus: bool,
  } satisfies ShapeOf<ScoreRow>,

  MatchTeamView: {
    teamId: str,
    result: { kinds: ["string", "null"], enum: "result" },
    roundsWon: numOrNull,
    firstHalf: numOrNull,
    players: { kinds: ["array"], shape: "ScoreRow" },
  } satisfies ShapeOf<MatchTeamView>,

  MatchDetailsView: {
    matchId: str,
    mapId: str,
    queueId: str,
    startMs: num,
    durationMs: numOrNull,
    perspective: { kinds: ["string"], enum: "perspective" },
    teams: { kinds: ["array"], shape: "MatchTeamView" },
  } satisfies ShapeOf<MatchDetailsView>,

  // ── My Stats (my_stats.json holds [a full case, an edge case]) ──
  RankChange: { tier: num, up: bool, startMs: num } satisfies ShapeOf<RankChange>,

  RankSummary: {
    tier: numOrNull,
    rr: numOrNull,
    wins: numOrNull,
    losses: numOrNull,
    rrToNext: numOrNull,
    netRr: numOrNull,
    avgWin: numOrNull,
    avgLoss: numOrNull,
    changes: { kinds: ["array"], shape: "RankChange" },
  } satisfies ShapeOf<RankSummary>,

  RrPoint: { startMs: num, absoluteRr: num } satisfies ShapeOf<RrPoint>,

  SessionMatch: {
    matchId: str,
    mapId: strOrNull,
    rrEarned: numOrNull,
    startMs: num,
    result: { kinds: ["string"], enum: "streakMark" },
  } satisfies ShapeOf<SessionMatch>,

  SessionSummary: {
    netRr: numOrNull,
    wins: num,
    losses: num,
    startedMs: num,
    matches: { kinds: ["array"], shape: "SessionMatch" },
  } satisfies ShapeOf<SessionSummary>,

  StatRow: {
    id: str,
    games: num,
    wins: num,
    losses: num,
    winrate: { kinds: ["number"], fraction: true },
    netRr: numOrNull,
    kd: num,
    acs: numOrNull,
  } satisfies ShapeOf<StatRow>,

  WinCell: { games: num, wins: num, winrate: { kinds: ["number", "null"], fraction: true } } satisfies ShapeOf<WinCell>,

  Insight: {
    kind: { kinds: ["string"], enum: "insightKind" },
    text: str,
    matches: num,
    positive: bool,
  } satisfies ShapeOf<Insight>,

  MyStats: {
    puuid: str,
    rank: { kinds: ["object"], shape: "RankSummary" },
    rrSeries: { kinds: ["array"], shape: "RrPoint" },
    session: { kinds: ["object", "null"], shape: "SessionSummary" },
    last5: { kinds: ["array"], shape: "MatchSummary" },
    maps: { kinds: ["array"], shape: "StatRow" },
    agents: { kinds: ["array"], shape: "StatRow" },
    byWeekday: { kinds: ["array"], shape: "WinCell" },
    byDaypart: { kinds: ["array"], shape: "WinCell" },
    insights: { kinds: ["array"], shape: "Insight" },
  } satisfies ShapeOf<MyStats>,

  // ── v2 redesign (R189; history_page.json, saved_players.json, player_marks.json, settings.json, ascii_art.json,
  // app_info.json) ──
  HistoryRow: {
    matchId: str,
    // null when the asset tables do not know the map: never a Riot path.
    mapId: strOrNull,
    queueId: str,
    startMs: num,
    durationMs: numOrNull,
    result: { kinds: ["string"], enum: "result" },
    roundsWon: numOrNull,
    roundsLost: numOrNull,
    agentId: strOrNull,
    // R102: the whole stat line is null when the match has no stats row for the user.
    kills: numOrNull,
    deaths: numOrNull,
    assists: numOrNull,
    kd: numOrNull,
    acs: numOrNull,
    hsPct: { kinds: ["number", "null"], fraction: true },
    rrEarned: numOrNull,
    tierAfter: numOrNull,
  } satisfies ShapeOf<HistoryRow>,

  HistoryPage: {
    rows: { kinds: ["array"], shape: "HistoryRow" },
    total: num,
    competitiveTotal: num,
    allTotal: num,
  } satisfies ShapeOf<HistoryPage>,

  SharedRecord: { games: num, wins: num, losses: num } satisfies ShapeOf<SharedRecord>,

  SavedPlayer: {
    puuid: str,
    name: { kinds: ["object"], shape: "DisplayName" },
    agentId: strOrNull,
    tier: numOrNull,
    with: { kinds: ["object"], shape: "SharedRecord" },
    against: { kinds: ["object"], shape: "SharedRecord" },
    note: str,
    savedMs: num,
    lastSeenMs: numOrNull,
    lastSeenMapId: strOrNull,
  } satisfies ShapeOf<SavedPlayer>,

  // `notes` is a puuid -> note map: an object with no fixed keys, so no nested descriptor.
  PlayerMarks: {
    saved: { kinds: ["array"] },
    notes: { kinds: ["object"] },
    notSmurf: { kinds: ["array"] },
  } satisfies ShapeOf<PlayerMarks>,

  Settings: {
    highlight: bool,
    enemyFirst: bool,
    smurf: { kinds: ["string"], enum: "smurfLevel" },
    savedAlert: bool,
    sound: bool,
    onTop: bool,
    boot: bool,
    tray: bool,
  } satisfies ShapeOf<Settings>,

  AsciiArt: { id: num, title: str, raw: str, createdMs: num } satisfies ShapeOf<AsciiArt>,

  AppInfo: {
    version: str,
    commit: strOrNull,
    buildMs: numOrNull,
    riotClientVersion: strOrNull,
    valorantVersion: strOrNull,
    dbMatches: num,
    dbBytes: num,
  } satisfies ShapeOf<AppInfo>,
};

/** One descriptor per `DisplayName` variant, keyed by its `kind` tag. */
export const displayNameShapes = {
  visible: { kind: { kinds: ["string"] }, name: str, tag: str } satisfies ShapeOf<Extract<DisplayName, { kind: "visible" }>>,
  remembered: {
    kind: { kinds: ["string"] },
    name: str,
    tag: str,
    source: { kinds: ["string"], enum: "rememberedSource" },
    fromMapId: strOrNull,
    fromStartMs: numOrNull,
  } satisfies ShapeOf<Extract<DisplayName, { kind: "remembered" }>>,
  hiddenPlayer: { kind: { kinds: ["string"] } } satisfies ShapeOf<Extract<DisplayName, { kind: "hiddenPlayer" }>>,
  hiddenAgent: { kind: { kinds: ["string"] }, agentId: str } satisfies ShapeOf<Extract<DisplayName, { kind: "hiddenAgent" }>>,
};
