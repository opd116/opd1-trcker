// The double-click guard (v0.2.0 polish): a card opens Details, and a second click within 400 ms must not land on
// the page that replaced the card grid (Back sits where the top-left card was, recent rows where the middle ones were).
import { describe, expect, it } from "vitest";
import { DOUBLE_CLICK_MS, createClickGuard } from "./clickGuard";

describe("createClickGuard", () => {
  it("is 400 ms", () => {
    expect(DOUBLE_CLICK_MS).toBe(400);
  });

  it("swallows nothing until a card opened Details", () => {
    let t = 1_000;
    const g = createClickGuard(() => t);
    expect(g.swallows()).toBe(false);
    t += 10_000;
    expect(g.swallows()).toBe(false);
  });

  it("swallows a click for 400 ms after the open, then lets clicks through", () => {
    let t = 5_000;
    const g = createClickGuard(() => t);
    g.arm();
    expect(g.swallows()).toBe(true);
    t += 399;
    expect(g.swallows()).toBe(true);
    t += 1;
    expect(g.swallows()).toBe(false);
    t += 5_000;
    expect(g.swallows()).toBe(false);
  });

  it("a later open re-arms it", () => {
    let t = 0;
    const g = createClickGuard(() => t);
    g.arm();
    t = 1_000;
    expect(g.swallows()).toBe(false);
    g.arm();
    t = 1_300;
    expect(g.swallows()).toBe(true);
  });
});
