// DEV-only (R189): the `status` the demo shows for a scene. `?variant=lost|error|loading` and `?recap=1` override it,
// matching the design's `variant` / `stage: recap` props.
import type { AppStatus, LiveState } from "../../types";
import { DEMO_NOW, MAPS } from "../cdv8";
import { sessionMatchId } from "../cdv8";

const LOCKFILE = String.raw`C:\Users\you\AppData\Local\Riot Games\Riot Client\Config\lockfile`;

/** The demo recap's match: a win on Haven 13–9 that ended 3 minutes ago. */
export const RECAP_MATCH_ID = sessionMatchId(DEMO_NOW - 41 * 60_000, "W", MAPS.Haven);

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
    pregame: live?.stage === "pregame" ? { phaseEndsMs: DEMO_NOW + 42_000, locked: 3, total: 5 } : null,
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
