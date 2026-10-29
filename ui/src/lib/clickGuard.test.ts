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

  it("allows fresh single clicks and keyboard/programmatic activation while armed", () => {
    const g = createClickGuard(() => 1_000);
    g.arm();
    expect(g.swallows()).toBe(true);
    expect(g.swallows(0)).toBe(false);
    expect(g.swallows(1)).toBe(false);
    expect(g.swallows(2)).toBe(true);
    expect(g.swallows(3)).toBe(true);
  });

  it("does not block repeated clicks before arming or after the interval", () => {
    let t = 0;
    const g = createClickGuard(() => t);
    expect(g.swallows(2)).toBe(false);
    g.arm();
    t = DOUBLE_CLICK_MS;
    expect(g.swallows(2)).toBe(false);
    expect(g.swallows(3)).toBe(false);
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

describe("opening pointer position", () => {
  it("blocks a second click at the opening spot, but allows fresh controls elsewhere and keyboard clicks", () => {
    const g = createClickGuard(() => 1_000);
    g.noteClick({ detail: 1, clientX: 100, clientY: 200, eventPhase: 2 });
    g.arm();
    expect(g.swallows(1, 100, 200)).toBe(true);
    expect(g.swallows(1, 103, 198)).toBe(true);
    expect(g.swallows(1, 150, 200)).toBe(false);
    expect(g.swallows(0, 100, 200)).toBe(false);
  });

  it("a later programmatic open does not inherit a stale click position", async () => {
    const g = createClickGuard(() => 1_000);
    const event = { detail: 1, clientX: 100, clientY: 200, eventPhase: 2 };
    g.noteClick(event);
    event.eventPhase = 0;
    await Promise.resolve();
    g.arm();
    expect(g.swallows(1, 100, 200)).toBe(false);
    expect(g.swallows(2, 100, 200)).toBe(true);
  });

  it("a keyboard activation clears the pointer origin", () => {
    const g = createClickGuard(() => 1_000);
    g.noteClick({ detail: 1, clientX: 100, clientY: 200, eventPhase: 2 });
    g.noteClick({ detail: 0, clientX: 0, clientY: 0, eventPhase: 2 });
    g.arm();
    expect(g.swallows(1, 100, 200)).toBe(false);
  });

  it("keeps the opening click through a microtask between capture and the target listener", async () => {
    const g = createClickGuard(() => 1_000);
    const event = { detail: 1, clientX: 100, clientY: 200, eventPhase: 1 };
    g.noteClick(event);
    await Promise.resolve();
    event.eventPhase = 2;
    g.arm();
    expect(g.swallows(1, 100, 200)).toBe(true);
  });

  it("the opening point expires at 400 ms", () => {
    let t = 0;
    const g = createClickGuard(() => t);
    g.noteClick({ detail: 1, clientX: 100, clientY: 200, eventPhase: 2 });
    g.arm();
    t = DOUBLE_CLICK_MS;
    expect(g.swallows(1, 100, 200)).toBe(false);
  });
});
