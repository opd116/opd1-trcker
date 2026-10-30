// Mirror of the Rust → UI JSON contract (crates/core/src/model.rs, serde camelCase; spec §7.1). The golden JSON in
// crates/core/tests/fixtures/contract/ pins it: contract.shape.ts holds one descriptor per type written from those
// files, contract.test.ts walks the goldens (and the demo fixtures) with them, contract.test-d.ts ties them to the
// types below. Rulings: R5 (rates are 0..1 fractions), R10 (enum shapes), R23 (tri-state fields), R97 (per-weapon
// tri-state), R99 (agent set only on lock), R102 / R124 (a match without a stats row sends null, never a counted 0),
// R116 / R120 (Details hero stats), R154 (a rank's RR may be unknown).
//
// Tri-state convention for loadable fields:
//   key absent (undefined) → still loading → skeleton
//   null                   → missing        → "—"
//   value                  → render it

export type Stage = "closed" | "menus" | "pregame" | "ingame";

export type Result = "win" | "loss" | "draw";

export type Team = "ally" | "enemy";

/**
 * R145: where a remembered name came from. `ownMatch`: a finished match the user played in showed it. `otherMatch`: a
 * stored match the user did not play in carried it. `seen`: a name-service answer from a time the player was visible.
 * Not a privacy tier: every source is drawn in the remembered style; only the tooltip's wording differs.
 */
export type RememberedSource = "ownMatch" | "otherMatch" | "seen";

export type DisplayName =
  | { kind: "visible"; name: string; tag: string }
  | {
      kind: "remembered";
      name: string;
      tag: string;
      source: RememberedSource;
      /** null for `seen` (a name-service answer has no map), and when unknown. */
      fromMapId: string | null;
      /** When the source match started; for `seen`, when the name was last confirmed. */
      fromStartMs: number | null;
    }
  | { kind: "hiddenPlayer" }
  | { kind: "hiddenAgent"; agentId: string };

export interface Rank {
  tier: number;
  /** R154: null when the MMR request failed and the tier came from the player's stored matches (the exact RR is unknown). */
  rr: number | null;
}

export interface Peak {
  tier: number;
  /** Short label from the backend, e.g. "E8 A3". */
  actLabel: string;
  /** Full act name for the hover tooltip, e.g. "V26 ACT V". */
  actName: string;
  /** Time since the peak act ended; null = the peak is in the current act. */
  sinceActEndMs: number | null;
  /** Act start / end, epoch ms (ruling R28). */
  actStartMs: number;
  actEndMs: number;
}

export interface Last5Entry {
  result: Result;
  /** null when Riot gave no RR change for that match. */
  rrEarned: number | null;
  startMs: number;
}

export interface PartyMark {
  group: number;
  confirmed: boolean;
}

export interface EncounterCount {
  with: number;
  against: number;
}

export interface PlayerCard {
  puuid: string;
  name?: DisplayName | null;
  /** null when the player hides it (HideAccountLevel), except on the user's own card (R100). */
  level?: number | null;
  cardId: string;
  /** Set only after the lock (R99); a hover leaves it null. */
  agentId: string | null;
  rank?: Rank | null;
  prevActTier?: number | null;
  peak?: Peak | null;
  kd?: number | null;
  /** Fraction 0..1. */
  hsPct?: number | null;
  /** Fraction 0..1. */
  actWinrate?: number | null;
  actWins?: number | null;
  /** Losses = games − wins. */
  actGames?: number | null;
  /** Newest first. */
  last5?: Last5Entry[] | null;
  party: PartyMark | null;
  encounters: EncounterCount | null;
  team: Team;
  isSelfParty: boolean;
  isSelf: boolean;
}

export interface LiveState {
  stage: Stage;
  mapId: string | null;
  players: PlayerCard[];
  /**
   * Rust has reached its first verdict on whether VALORANT is running: false while the very first presence check is
   * still pending (the `closed` state published before it is the model's starting value, not a verdict). Once true it
   * stays true for the app's lifetime. The start page decides on the first state with `settled: true`.
   */
  settled: boolean;
}

/** One row of `PlayerDetails.recent`, and of `MyStats.last5` (the same Rust struct). */
export interface MatchSummary {
  matchId: string;
  mapId: string;
  startMs: number;
  result: Result;
  roundsWon: number | null;
  roundsLost: number | null;
  agentId: string | null;
  /** R102: kills, deaths, assists, kd, acs and hsPct are all null when the match has no stats row for the player (shown as —). */
  kills: number | null;
  deaths: number | null;
  assists: number | null;
  kd: number | null;
  acs: number | null;
  /** Fraction 0..1; also null when no shot landed. */
  hsPct: number | null;
  rrEarned: number | null;
  /** The player's rank during that match (R81: one rule for recent rows, encounters and the scoreboard). */
  tier: number | null;
}

export interface WeaponSkin {
  chromaId: string;
  skinName: string;
  /** The text inside "(Variant N …)", or null for the base chroma. */
  chromaName: string | null;
  tierId: string | null;
}

/** Tri-state per weapon (R97): absent = the name is still resolving, null = none or unknown. */
export interface Loadout {
  vandal?: WeaponSkin | null;
  phantom?: WeaponSkin | null;
  operator?: WeaponSkin | null;
  sheriff?: WeaponSkin | null;
}

export interface EncounterDetailRow {
  matchId: string;
  mapId: string;
  startMs: number;
  with: boolean;
  /** The user's side. */
  result: Result;
  roundsWon: number | null;
  roundsLost: number | null;
  durationMs: number | null;
  themAgentId: string | null;
  themTier: number | null;
  selfAgentId: string | null;
  selfTier: number | null;
}

export interface TeammateSummary {
  puuid: string;
  name: DisplayName;
  count: number;
  /** Of those `count` shared competitive matches, the profile owner's wins and losses (a draw is neither). */
  wins: number;
  losses: number;
}

export interface PlayerDetails {
  card: PlayerCard;
  /** At most 8, newest first. */
  recent: MatchSummary[];
  topTeammates: TeammateSummary[];
  loadout: Loadout;
  /** Newest first. */
  encounters: EncounterDetailRow[];
  /** R116: K/D over the newest (up to 20) current-act competitive matches that have stats; null when there are none. */
  heroKd: number | null;
  /**
   * R116, R120: HS% over the same matches, a 0..1 fraction like every other HS% field; null when there are none, or
   * when those matches landed no shot (so it may be null while heroMatches > 0).
   */
  heroHs: number | null;
  /** R116: how many matches heroKd / heroHs use (0..20). Feeds the hero tooltip. */
  heroMatches: number;
}

export interface ScoreRow {
  puuid: string;
  name: DisplayName;
  agentId: string | null;
  tier: number | null;
  /**
   * R124: kills, deaths, assists, acs, adr and hsPct are all null for a player with no stats row (shown as —); adr is
   * also null when only the damage is unknown. `tier` is independent of the stats (R81): a ranked player with no
   * stats row still has one, so draw the rank and the — cells together.
   */
  kills: number | null;
  deaths: number | null;
  assists: number | null;
  acs: number | null;
  adr: number | null;
  /** Fraction 0..1. */
  hsPct: number | null;
  isSelf: boolean;
  isFocus: boolean;
}

export interface MatchTeamView {
  teamId: string;
  result: Result | null;
  roundsWon: number | null;
  /** Rounds 1–12 won. */
  firstHalf: number | null;
  /** Sorted by ACS descending. */
  players: ScoreRow[];
}

/** `invoke("match_details", { matchId, focusPuuid })`. */
export interface MatchDetailsView {
  matchId: string;
  mapId: string;
  queueId: string;
  startMs: number;
  durationMs: number | null;
  perspective: "self" | "focus";
  /** [0] = the perspective team; one entry for no-team modes. */
  teams: MatchTeamView[];
}

// ── My Stats (`invoke("my_stats")`, the `mystats` event; crates/core/src/model.rs, spec §9) ─────────────────────────
// The user's own account over the current act, built by Rust from the DB. `null` always means "the DB does not know"
// (shown as —); a 0 is only ever a counted zero. None of these fields is tri-state: the page has no loading state of
// its own beyond `my_stats` still being unanswered.

/** A session streak mark. The wire letters are W / L / D, not the `Result` words. */
export type StreakMark = "W" | "L" | "D";

/** One tier change of the rank card, newest first in `RankSummary.changes`. */
export interface RankChange {
  /** The tier reached. */
  tier: number;
  up: boolean;
  startMs: number;
}

/** Section 1, the rank card. */
export interface RankSummary {
  /** From the stored MMR snapshot, like the live card's rank. */
  tier: number | null;
  /** null when the exact RR is unknown (the tier may then still be known: the card shows `— RR`). */
  rr: number | null;
  /** Counted from the act's stored matches; draws are neither. */
  wins: number | null;
  losses: number | null;
  /** null from Immortal up (RR accumulates, no fixed next step) and when unranked. */
  rrToNext: number | null;
  /** Sum of the act's stored RR changes. */
  netRr: number | null;
  /** Mean RR change over ranked wins / losses with a known RR change (`avgLoss` is negative). */
  avgWin: number | null;
  avgLoss: number | null;
  /** The latest tier changes, newest first, at most 5. */
  changes: RankChange[];
}

/** One point of the RR chart. `absoluteRr` is `tier × 100 + rr` below Immortal, `2400 + rr` from Immortal 1 up. */
export interface RrPoint {
  startMs: number;
  absoluteRr: number;
}

/** One match of the current session: a chip of section 3 (its result, its tooltip, and the Match Details it opens). */
export interface SessionMatch {
  /** What the chip opens in Match Details. */
  matchId: string;
  /** A valorant-api map uuid; null when the map is unknown (never `""`). */
  mapId: string | null;
  /** The RR change Riot recorded for the match; null when none is stored (an unknown change, not 0). */
  rrEarned: number | null;
  startMs: number;
  result: StreakMark;
}

/** Section 3. */
export interface SessionSummary {
  /** null when no match in the session has a stored RR change. */
  netRr: number | null;
  wins: number;
  losses: number;
  startedMs: number;
  /** Every match of the session, oldest first. */
  matches: SessionMatch[];
}

/** One row of the Maps or Agents table; `id` is a valorant-api map or agent uuid. */
export interface StatRow {
  id: string;
  games: number;
  wins: number;
  losses: number;
  /** Fraction 0..1: wins / games (a draw counts as a game). */
  winrate: number;
  /** null when none of the row's matches has a stored RR change. */
  netRr: number | null;
  /** null when none of the row's matches has a stats row (R102). */
  kd: number | null;
  /** null when no match of the row has a known round count. */
  acs: number | null;
}

/** One weekday or time-of-day bucket. */
export interface WinCell {
  games: number;
  wins: number;
  /** Not `games - wins`: a draw is neither. */
  losses: number;
  /** Fraction 0..1; null when the bucket has no games. */
  winrate: number | null;
}

export type InsightKind = "bestMap" | "worstMap" | "timeOfDay" | "weekdayWeekend" | "rrEconomy" | "last7Days";

/** Section 8. `text` is finished English UI copy; `matches` is the sample it rests on. */
export interface Insight {
  kind: InsightKind;
  text: string;
  matches: number;
  /** Good news (a strength, RR gained) against a weak spot. */
  positive: boolean;
}

type Tuple<T, N extends number, A extends T[] = []> = A["length"] extends N ? A : Tuple<T, N, [...A, T]>;

export interface MyStats {
  /** The user's own PUUID: the page opens Match Details with it as the focus player (a row is clickable whenever My Stats exists). */
  puuid: string;
  rank: RankSummary;
  /** The RR chart, oldest first. */
  rrSeries: RrPoint[];
  /** null when the act's newest match started more than 6h ago (that session is over), or there is none. */
  session: SessionSummary | null;
  /** The act's newest 5 competitive matches, newest first. */
  last5: MatchSummary[];
  /** Most played first. */
  maps: StatRow[];
  /** Most played first. */
  agents: StatRow[];
  /** Local time, Monday first: index 0 = Monday … 6 = Sunday. */
  byWeekday: Tuple<WinCell, 7>;
  /** Local time, in this order: Morning 05–12, Afternoon 12–17, Evening 17–22, Night 22–05. */
  byDaypart: Tuple<WinCell, 4>;
  insights: Insight[];
}

/** `invoke("meta")` in Tauri; a static valorant-api snapshot in the DEV browser (R21). */
export interface Meta {
  agents: Record<string, { name: string }>;
  maps: Record<string, { name: string }>;
  /** Keyed by tier number (JSON object keys are strings). `color` is valorant-api's "rrggbbaa" or a CSS hex. */
  tiers: Record<string, { name: string; color: string }>;
}

// ── v2 redesign (R189, user 2026-10-22; crates/core/src/wire.rs) ─────────────────────────────────────────────────────
// New commands and events only: nothing above changes shape. `null` is a known-missing value (shown as —).

/** The title-bar pill: Riot Client up but VALORANT not running / game running / reconnecting / client unreachable. */
export type ConnState = "waiting" | "ok" | "lost" | "error";

/** `invoke("app_status")`, then the `status` event. */
export interface AppStatus {
  conn: ConnState;
  riotClientVersion: string | null;
  /** The last successful local-API answer, epoch ms. */
  lastDataMs: number | null;
  /** While `lost`: the reconnect attempt (1-based) of `maxAttempts`. */
  attempt: number;
  maxAttempts: number;
  /** While `lost` / `error`: ms until the next automatic retry. */
  retryInMs: number | null;
  /** While `error`: `LOCKFILE_NOT_FOUND` or `CLIENT_UNREACHABLE`. */
  errorCode: string | null;
  errorAtMs: number | null;
  lockfilePath: string;
  /** Agent Select only. */
  pregame: { phaseEndsMs: number | null; locked: number; total: number } | null;
  /** Menus while matchmaking only. */
  queue: { queueId: string; searchingSinceMs: number } | null;
  /** The user's own match that just ended, until they queue again: the recap reads `match_details(recapMatchId, self)`. */
  recapMatchId: string | null;
  /** e.g. `release-11.08`. */
  gameVersion: string | null;
}

/** `invoke("player_marks")`, then the `marks` event. */
export interface PlayerMarks {
  /** Newest saved first. */
  saved: string[];
  /** Non-empty notes by puuid; the user's own puuid holds their personal notes. */
  notes: Record<string, string>;
  /** Dismissed smurf flags. */
  notSmurf: string[];
}

/** Stored competitive matches with the user on one side, counted from the user's side. */
export interface SharedRecord {
  games: number;
  wins: number;
  losses: number;
}

/** One row of `invoke("saved_players")`. */
export interface SavedPlayer {
  puuid: string;
  name: DisplayName;
  agentId: string | null;
  tier: number | null;
  with: SharedRecord;
  against: SharedRecord;
  /** "" when none. */
  note: string;
  savedMs: number;
  lastSeenMs: number | null;
  lastSeenMapId: string | null;
}

export type HistoryQueue = "competitive" | "all";

/** One of the user's own stored matches, any queue. */
export interface HistoryRow {
  matchId: string;
  mapId: string | null;
  queueId: string;
  startMs: number;
  durationMs: number | null;
  result: Result;
  roundsWon: number | null;
  roundsLost: number | null;
  agentId: string | null;
  kills: number | null;
  deaths: number | null;
  assists: number | null;
  kd: number | null;
  acs: number | null;
  /** Fraction 0..1. */
  hsPct: number | null;
  rrEarned: number | null;
  tierAfter: number | null;
}

/** `invoke("match_history", { queue, offset, limit })`. */
export interface HistoryPage {
  /** Newest first. */
  rows: HistoryRow[];
  total: number;
  competitiveTotal: number;
  allTotal: number;
}

export type SmurfLevel = "off" | "cautious" | "eager";

/** `invoke("get_settings")` / `invoke("set_settings", { settings })`, then the `settings` event. */
export interface Settings {
  highlight: boolean;
  enemyFirst: boolean;
  smurf: SmurfLevel;
  savedAlert: boolean;
  sound: boolean;
  onTop: boolean;
  boot: boolean;
  tray: boolean;
}

/** One saved ASCII gallery item. */
export interface AsciiArt {
  id: number;
  title: string;
  /** Rows joined by "\n". */
  raw: string;
  createdMs: number;
}

/** `invoke("app_info")`: the About screen. */
export interface AppInfo {
  version: string;
  commit: string | null;
  buildMs: number | null;
  riotClientVersion: string | null;
  valorantVersion: string | null;
  dbMatches: number;
  dbBytes: number;
}

export type Edition = "select" | "deluxe" | "premium" | "exclusive" | "ultra";

export interface OwnedSkin {
  skinId: string;
  /** The skin's first level: its `skinLevel` image. */
  levelId: string;
  name: string;
  weaponId: string;
  edition: Edition | null;
  /** Standard store price for its edition; null when not sold for VP. */
  vp: number | null;
}

/**
 * `invoke("collection")`: `null` while no refresh was attempted yet (the game has not run); an attempted refresh
 * that left nothing stored, or one superseded by an account switch, is a rejected invoke (Rust `Err`), not `null`.
 */
export interface Collection {
  items: OwnedSkin[];
  weapons: { id: string; name: string }[];
  fetchedMs: number;
}

/** `invoke("profile_extras", { puuid })`: the profile drawer's maps and "in your match" chips. */
export interface ProfileExtras {
  puuid: string;
  /** Current-act competitive record per map, most played first. */
  maps: StatRow[];
  /** For the player's recent matches: which players of the current live match were in each. */
  liveMates: { matchId: string; puuids: string[] }[];
}
