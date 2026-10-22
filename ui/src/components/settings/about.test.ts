import { describe, expect, it } from "vitest";
import type { AppInfo, AppStatus } from "../../lib/types";
import { aboutRows, dbSize, gameVersion } from "./about";

const INFO: AppInfo = {
  version: "0.4.0",
  commit: "2a5a381",
  buildMs: Date.UTC(2026, 9, 19),
  riotClientVersion: "98.0.1",
  valorantVersion: "release-11.08",
  dbMatches: 1284,
  dbBytes: 48 * 1024 * 1024,
};

const status = (conn: AppStatus["conn"]): AppStatus => ({
  conn,
  riotClientVersion: "97.0.0",
  lastDataMs: null,
  attempt: 0,
  maxAttempts: 0,
  retryInMs: null,
  errorCode: null,
  errorAtMs: null,
  lockfilePath: "",
  pregame: null,
  queue: null,
  recapMatchId: null,
  gameVersion: "release-11.07",
});

describe("aboutRows", () => {
  it("the design's six rows, from app_info and the status", () => {
    expect(aboutRows(INFO, status("ok")).map((r) => [r.label, r.value])).toEqual([
      ["VERSION", "0.4.0 · 2a5a381"],
      ["BUILD", "Oct 19, 2026"],
      ["RIOT CLIENT", "98.0.1 · connected"],
      ["VALORANT", "11.08"],
      ["GAME DATA", "valorant-api.com"],
      ["LOCAL DATABASE", "1,284 matches · 48 MB"],
    ]);
  });

  it("missing values read —; the status fills in what app_info lacks", () => {
    const bare: AppInfo = { ...INFO, commit: null, buildMs: null, riotClientVersion: null, valorantVersion: null };
    const rows = Object.fromEntries(aboutRows(bare, status("lost")).map((r) => [r.label, r.value]));
    expect(rows).toMatchObject({ VERSION: "0.4.0", BUILD: "—", "RIOT CLIENT": "97.0.0 · reconnecting", VALORANT: "11.07" });
    const none = Object.fromEntries(aboutRows(null, null).map((r) => [r.label, r.value]));
    expect(none).toEqual({
      VERSION: "—",
      BUILD: "—",
      "RIOT CLIENT": "—",
      VALORANT: "—",
      "GAME DATA": "valorant-api.com",
      "LOCAL DATABASE": "—",
    });
    expect(aboutRows({ ...INFO, riotClientVersion: null }, status("error"))[2]!.value).toBe("97.0.0 · unreachable");
  });

  it("db size and game version formats", () => {
    expect(dbSize(48 * 1024 * 1024)).toBe("48 MB");
    expect(dbSize(3.44 * 1024 * 1024)).toBe("3.4 MB");
    expect(dbSize(820 * 1024)).toBe("820 KB");
    expect(dbSize(10)).toBe("1 KB");
    expect(dbSize(0)).toBe("0 KB");
    expect(dbSize(NaN)).toBe("—");
    expect(gameVersion("release-11.08")).toBe("11.08");
    expect(gameVersion(null)).toBeNull();
    expect(aboutRows({ ...INFO, dbMatches: 1 }, null)[5]!.value).toBe("1 match · 48 MB");
  });
});
