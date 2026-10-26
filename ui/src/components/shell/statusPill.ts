// The title bar's status pill (design v2, TrackerApp.dc.html 2055–2060): what each connection state looks like and
// says. Pure, so the wording is testable; StatusPill.svelte draws it.
import { ageShort } from "../../lib/format";
import { PALETTE as P, rgba } from "../../lib/theme";
import type { AppStatus, Stage } from "../../lib/types";

export type PillView = {
  conn: AppStatus["conn"];
  label: string;
  /** Small mono caps after the label; "" for none. */
  detail: string;
  tip: string;
  color: string;
  bg: string;
  bd: string;
  dot: string;
  pulse: boolean;
};

const secs = (ms: number) => Math.max(0, Math.round(ms / 1000));

/** `LOCKFILE_NOT_FOUND` → "LOCKFILE NOT FOUND". */
const caps = (code: string) => code.replace(/_/g, " ").toUpperCase();

const ERROR_TIPS: Record<string, string> = {
  LOCKFILE_NOT_FOUND: "The lockfile wasn’t found. Check the Riot Client install folder.",
  CLIENT_UNREACHABLE: "The Riot Client isn’t answering on its local port.",
};

/**
 * The pill for a status, the live stage (for the "ok" detail) and the current time. `arrivedMs`: when this status
 * arrived; its `retryInMs` counts from then (an unchanged status is not sent again), so the wait shown counts down.
 */
export function pillView(s: AppStatus, stage: Stage | null, nowMs: number, arrivedMs: number = nowMs): PillView {
  switch (s.conn) {
    case "waiting":
      return {
        conn: "waiting",
        label: "Waiting for VALORANT",
        detail: "RIOT CLIENT ONLINE",
        tip: "Riot Client is running. Start VALORANT to load a match.",
        color: P.text2,
        bg: P.surface,
        bd: P.track,
        dot: P.textDim,
        pulse: true,
      };
    case "ok": {
      const live = stage === "pregame" || stage === "ingame";
      const detail =
        stage === "pregame" ? "AGENT SELECT" : stage === "menus" ? (s.recapMatchId ? "MATCH ENDED" : "IN LOBBY") : "";
      const lines = [s.riotClientVersion ? `Riot Client ${s.riotClientVersion}` : "Riot Client"];
      if (s.lastDataMs !== null) lines.push(`Last update ${ageShort(nowMs - s.lastDataMs)} ago`);
      return {
        conn: "ok",
        label: "Connected",
        detail,
        tip: lines.join("\n"),
        color: P.okText,
        bg: rgba(P.win, 0.07),
        bd: rgba(P.win, 0.26),
        dot: P.win,
        pulse: live,
      };
    }
    case "lost": {
      const lines: string[] = [];
      if (s.lastDataMs !== null) lines.push(`Last data received ${ageShort(nowMs - s.lastDataMs)} ago.`);
      if (s.retryInMs !== null) lines.push(`Retrying in ${secs(Math.max(0, s.retryInMs - (nowMs - arrivedMs)))} s.`);
      return {
        conn: "lost",
        label: "Reconnecting…",
        detail: s.maxAttempts > 0 ? `ATTEMPT ${s.attempt} OF ${s.maxAttempts}` : "",
        tip: lines.join("\n") || "Lost the connection to the Riot Client.",
        color: P.lostText,
        bg: rgba(P.warn, 0.07),
        bd: rgba(P.warn, 0.3),
        dot: P.warn,
        pulse: true,
      };
    }
    case "error":
      return {
        conn: "error",
        label: "Riot Client unreachable",
        detail: s.errorCode ? caps(s.errorCode) : "",
        tip: (s.errorCode && ERROR_TIPS[s.errorCode]) || "The Riot Client couldn’t be reached.",
        color: P.errorText,
        bg: rgba(P.loss, 0.07),
        bd: rgba(P.loss, 0.3),
        dot: P.loss,
        pulse: false,
      };
  }
}
