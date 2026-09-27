// Mirror of the Rust → UI JSON contract (crates/core/src/model.rs, serde camelCase).
// Rulings: R5 (rates are 0..1 fractions), R10 (enum shapes), R23 (tri-state fields).
//
// Tri-state convention for loadable fields:
//   key absent (undefined) → still loading → skeleton
//   null                   → missing        → "—"
//   value                  → render it

export type Stage = "closed" | "menus" | "pregame" | "ingame";

export type Team = "ally" | "enemy";

export type DisplayName =
  | { kind: "visible"; name: string; tag: string }
  | { kind: "remembered"; name: string; tag: string }
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
  win: boolean;
  rrEarned: number;
  startMs: number;
}

export interface PartyMark {
  group: number;
  confirmed: boolean;
}

export interface Encounters {
  with: number;
  against: number;
}

export interface PlayerCard {
  puuid: string;
  name: DisplayName;
  level: number | null;
  cardId: string;
  agentId: string | null;
  rank?: Rank | null;
  prevActTier?: number | null;
  peak?: Peak | null;
  kd?: number | null;
  /** Fraction 0..1. */
  hsPct?: number | null;
  /** Fraction 0..1. */
  actWinrate?: number | null;
  /** Newest first. */
  last5?: Last5Entry[] | null;
  party?: PartyMark | null;
  encounters?: Encounters | null;
  team: Team;
  isSelfParty: boolean;
}

export interface LiveState {
  stage: Stage;
  mapId: string | null;
  players: PlayerCard[];
}

/** `invoke("meta")` in Tauri; a static valorant-api snapshot in the DEV browser (R21). */
export interface Meta {
  agents: Record<string, { name: string }>;
  maps: Record<string, { name: string }>;
  /** Keyed by tier number (JSON object keys are strings). `color` is valorant-api's "rrggbbaa" or a CSS hex. */
  tiers: Record<string, { name: string; color: string }>;
}
