// The automated smurf signal (R189; the v2 design's Settings "Smurf warnings" text). Pure: it reads only what the
// live cards already carry, so it needs no backend. Always a guess, never a fact: the UI labels it "automated" and
// "not confirmed", and the user can dismiss it per player (`marks.notSmurf`).
//
// The four signals, from the design's Settings copy:
//   1. account level under 60
//   2. headshots 12+ points above the lobby average
//   3. win rate over 70%
//   4. fewer than 40 ranked games this act
// Cautious flags when 3 of 4 agree, Eager when any 2 agree, Off never.
import type { PlayerCard, SmurfLevel } from "./types";

export const SMURF_LEVEL_MAX = 60;
export const SMURF_HS_ABOVE = 0.12;
export const SMURF_WINRATE_MIN = 0.7;
export const SMURF_GAMES_MAX = 40;

export interface SmurfSignal {
  /** One finished English line per signal that agreed (the profile's smurf panel rows, prototype 1547). */
  reasons: string[];
  /** The same signals in the board pill's tooltip wording (prototype 1323): "Level 41, lobby average 280". */
  tips: string[];
}

/** Mean of the known values, or null when none is known. */
function mean(xs: (number | null | undefined)[]): number | null {
  const known = xs.filter((x): x is number => typeof x === "number");
  return known.length === 0 ? null : known.reduce((a, b) => a + b, 0) / known.length;
}

/** The lobby averages the signals compare against: every other card of the live roster (the design: "38% headshots, lobby average 23%"). */
export function lobbyAverages(lobby: readonly PlayerCard[]): { level: number | null; hs: number | null } {
  return { level: mean(lobby.map((c) => c.level)), hs: mean(lobby.map((c) => c.hsPct)) };
}

/**
 * The signal for one player, or null when it does not fire (or `level` is off, or the player is the user, or the user
 * dismissed it). A signal whose input is still loading or unknown does not count.
 */
export function smurfSignal(
  card: PlayerCard,
  lobby: readonly PlayerCard[],
  level: SmurfLevel,
  dismissed: readonly string[] = [],
): SmurfSignal | null {
  if (level === "off" || card.isSelf || dismissed.includes(card.puuid)) return null;
  const avg = lobbyAverages(lobby.filter((c) => c.puuid !== card.puuid));
  const reasons: string[] = [];
  const tips: string[] = [];
  if (typeof card.level === "number" && card.level < SMURF_LEVEL_MAX) {
    reasons.push(`Account level ${card.level}${avg.level !== null ? `; this lobby averages ${Math.round(avg.level)}` : ""}`);
    tips.push(`Level ${card.level}${avg.level !== null ? `, lobby average ${Math.round(avg.level)}` : ""}`);
  }
  if (typeof card.hsPct === "number" && avg.hs !== null && card.hsPct - avg.hs >= SMURF_HS_ABOVE) {
    reasons.push(`${Math.round(card.hsPct * 100)}% headshots; this lobby averages ${Math.round(avg.hs * 100)}%`);
    tips.push(`${Math.round(card.hsPct * 100)}% headshots, lobby average ${Math.round(avg.hs * 100)}%`);
  }
  const games = card.actGames;
  if (typeof card.actWinrate === "number" && card.actWinrate > SMURF_WINRATE_MIN && typeof games === "number" && games > 0) {
    const line = `${Math.round(card.actWinrate * 100)}% wins over only ${games} games`;
    reasons.push(line);
    tips.push(line);
  }
  if (typeof games === "number" && games < SMURF_GAMES_MAX) {
    const line = `${games} ranked ${games === 1 ? "game" : "games"} this act`;
    reasons.push(line);
    tips.push(line);
  }
  const need = level === "eager" ? 2 : 3;
  return reasons.length >= need ? { reasons, tips } : null;
}
