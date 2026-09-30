// Mirror of the Rust → UI JSON contract (crates/core/src/model.rs, serde camelCase; spec §7.1). The golden JSON in
// crates/core/tests/fixtures/contract/ pins it: contract.shape.ts holds one descriptor per type written from those
// files, contract.test.ts walks the goldens (and the demo fixtures) with them, contract.test-d.ts ties them to the
// types below. Rulings: R5 (rates are 0..1 fractions), R10 (enum shapes), R23 (tri-state fields), R97 (per-weapon
// tri-state), R99 (agent set only on lock), R102 / R124 (a match without a stats row sends null, never a counted 0),
// R116 / R120 (Details hero stats).
//
// Tri-state convention for loadable fields:
//   key absent (undefined) → still loading → skeleton
//   null                   → missing        → "—"
//   value                  → render it

export type Stage = "closed" | "menus" | "pregame" | "ingame";

export type Result = "win" | "loss" | "draw";

export type Team = "ally" | "enemy";

export type DisplayName =
  | { kind: "visible"; name: string; tag: string }
  | { kind: "remembered"; name: string; tag: string; fromMapId: string | null; fromStartMs: number | null }
  | { kind: "hiddenPlayer" }
  | { kind: "hiddenAgent"; agentId: string };

export interface Rank {
  tier: number;
  rr: number;
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
}

/** One row of `PlayerDetails.recent`. */
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

/** `invoke("meta")` in Tauri; a static valorant-api snapshot in the DEV browser (R21). */
export interface Meta {
  agents: Record<string, { name: string }>;
  maps: Record<string, { name: string }>;
  /** Keyed by tier number (JSON object keys are strings). `color` is valorant-api's "rrggbbaa" or a CSS hex. */
  tiers: Record<string, { name: string; color: string }>;
}
