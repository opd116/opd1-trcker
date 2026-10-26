import { describe, expect, it } from "vitest";
import type { AppStatus } from "../../lib/types";
import { pillView } from "./statusPill";

const base: AppStatus = {
  conn: "ok",
  riotClientVersion: "98.0.1",
  lastDataMs: 10_000,
  attempt: 0,
  maxAttempts: 5,
  retryInMs: null,
  errorCode: null,
  errorAtMs: null,
  lockfilePath: "",
  pregame: null,
  queue: null,
  recapMatchId: null,
  gameVersion: null,
};

describe("status pill (design v2 2055–2060)", () => {
  it("waiting", () => {
    expect(pillView({ ...base, conn: "waiting" }, "closed", 0)).toMatchObject({
      label: "Waiting for VALORANT",
      detail: "RIOT CLIENT ONLINE",
      tip: "Riot Client is running. Start VALORANT to load a match.",
      color: "#c9cbd0",
      bg: "#121418",
      bd: "#24272d",
      dot: "#8a8f97",
      pulse: true,
    });
  });

  it("ok: the detail follows the stage, the dot pulses only in a match", () => {
    const at = (stage: Parameters<typeof pillView>[1], s: Partial<AppStatus> = {}) => pillView({ ...base, ...s }, stage, 12_000);
    expect(at("menus")).toMatchObject({ label: "Connected", detail: "IN LOBBY", pulse: false, tip: "Riot Client 98.0.1\nLast update 2 s ago" });
    expect(at("menus", { recapMatchId: "m" }).detail).toBe("MATCH ENDED");
    expect(at("pregame")).toMatchObject({ detail: "AGENT SELECT", pulse: true });
    expect(at("ingame")).toMatchObject({ detail: "", pulse: true });
    expect(at("ingame")).toMatchObject({ color: "#cdeedb", bg: "rgba(95,207,143,.07)", bd: "rgba(95,207,143,.26)", dot: "#5fcf8f" });
  });

  it("lost and error", () => {
    expect(pillView({ ...base, conn: "lost", attempt: 2, retryInMs: 5000 }, "ingame", 52_000)).toMatchObject({
      label: "Reconnecting…",
      detail: "ATTEMPT 2 OF 5",
      tip: "Last data received 42 s ago.\nRetrying in 5 s.",
      color: "#ffc690",
      dot: "#ff9a3c",
      pulse: true,
    });
    // The wait counts from when the status arrived: 3 s later, 2 s are left; past it, 0.
    const lost = { ...base, conn: "lost" as const, attempt: 2, retryInMs: 5000 };
    expect(pillView(lost, "ingame", 55_000, 52_000).tip).toBe("Last data received 45 s ago.\nRetrying in 2 s.");
    expect(pillView(lost, "ingame", 60_000, 52_000).tip).toContain("Retrying in 0 s.");
    expect(pillView({ ...base, conn: "error", errorCode: "LOCKFILE_NOT_FOUND" }, null, 0)).toMatchObject({
      label: "Riot Client unreachable",
      detail: "LOCKFILE NOT FOUND",
      tip: "The lockfile wasn’t found. Check the Riot Client install folder.",
      color: "#ffb1a8",
      pulse: false,
    });
  });

  it("the last-update age steps up to days", () => {
    const s: AppStatus = { ...base, lastDataMs: 0 };
    expect(pillView(s, "menus", 3 * 86_400_000).tip).toContain("Last update 3 d ago");
  });
});
