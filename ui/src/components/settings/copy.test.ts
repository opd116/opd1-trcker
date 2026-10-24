import { describe, expect, it } from "vitest";
import { DEFAULT_SETTINGS } from "../../lib/appdata.svelte";
import { disabledReason, GROUPS, SMURF, TOGGLES } from "./copy";

describe("Settings copy", () => {
  it("four groups in the design's order, every setting once", () => {
    expect(GROUPS.map((g) => g.title)).toEqual(["Live match", "Player signals", "Alerts", "Window and startup"]);
    const keys = GROUPS.flatMap((g) => g.rows.map((r) => (r.kind === "toggle" ? r.key : "smurf")));
    expect(keys.sort()).toEqual(Object.keys(DEFAULT_SETTINGS).sort());
    expect(Object.keys(TOGGLES)).toHaveLength(7);
    expect(SMURF.options.map((o) => o.label)).toEqual(["Off", "Cautious", "Eager"]);
  });
  it("Play a sound needs the saved-player alert", () => {
    expect(disabledReason("sound", { ...DEFAULT_SETTINGS, savedAlert: false })).toBe("Turn on “Saved player in my match” first");
    expect(disabledReason("sound", { ...DEFAULT_SETTINGS, savedAlert: true })).toBeNull();
    expect(disabledReason("tray", { ...DEFAULT_SETTINGS, savedAlert: false })).toBeNull();
  });
});
