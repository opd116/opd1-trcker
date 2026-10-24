// DEV-only (R189): the `status` the demo shows for a scene. `?variant=lost|error|loading` and `?recap=1` override it,
// matching the design's `variant` / `stage: recap` props.
import type { AppStatus, LiveState } from "../../types";
import { DEMO_NOW, recentMatchId } from "../cdv8";

const LOCKFILE = String.raw`C:\Users\you\AppData\Local\Riot Games\Riot Client\Config\lockfile`;

/**
 * The demo recap's match: the user's newest match (Lotus, W 13–9, +18 RR, 38 min), as in the app, where the recap is
 * always the match that just ended. Being `mystats.last5[0]`, it carries the RR change the recap's hero stat and rank
 * card read (+18, the design's figure), and it is the same match as the Waiting card's LAST MATCH and History's top row.
 */
export const RECAP_MATCH_ID = recentMatchId("fahad", 0);

export function demoStatus(live: LiveState | null, q: URLSearchParams): AppStatus {
  const variant = q.get("variant");
  const base: AppStatus = {
    conn: live && live.stage !== "closed" ? "ok" : "waiting",
    riotClientVersion: "98.0.1",
    lastDataMs: DEMO_NOW - 2_000,
    attempt: 0,
    maxAttempts: 5,
    retryInMs: null,
    errorCode: null,
    errorAtMs: null,
    lockfilePath: LOCKFILE,
    // Locked = the allies whose agent is known (a hover is not a lock, R99), so "N OF 5 LOCKED" counts the rows that
    // do not read "Picking…".
    pregame:
      live?.stage === "pregame"
        ? { phaseEndsMs: DEMO_NOW + 42_000, locked: live.players.filter((p) => p.team === "ally" && p.agentId).length, total: 5 }
        : null,
    queue: live?.stage === "menus" && q.get("queue") === "1" ? { queueId: "competitive", searchingSinceMs: DEMO_NOW - 84_000 } : null,
    recapMatchId: q.get("recap") === "1" ? RECAP_MATCH_ID : null,
    gameVersion: "release-11.08",
  };
  if (variant === "lost") return { ...base, conn: "lost", lastDataMs: DEMO_NOW - 42_000, attempt: 2, retryInMs: 5_000 };
  if (variant === "error") {
    return { ...base, conn: "error", errorCode: "LOCKFILE_NOT_FOUND", errorAtMs: DEMO_NOW - 60_000, retryInMs: 8_000, riotClientVersion: null };
  }
  return base;
}
