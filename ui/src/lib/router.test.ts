import { get } from "svelte/store";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { openDetails } from "./boot";
import { activeTab, back, go, onStage, router, scrollHost, showMore, type Route } from "./router.svelte";
import type { PlayerDetails } from "./types";
import { hideTip, tipOver, tipState } from "./tooltip.svelte";

const LIVE: Route = { name: "live" };
const details = (puuid: string, more = false): Route => ({ name: "details", puuid, more });

beforeEach(() => {
  go(LIVE);
});

describe("router", () => {
  it("starts on live", async () => {
    vi.resetModules();
    const fresh = await import("./router.svelte");
    expect(fresh.router.route).toEqual({ name: "live" });
  });

  it("pregame switches to live from details", () => {
    go(details("p1"));
    onStage("menus", "pregame");
    expect(router.route).toEqual(LIVE);
  });

  it("pregame switches to live from a match, and on the first stage ever", () => {
    go({ name: "match", matchId: "m1", focusPuuid: "p1", from: "recent", back: { puuid: "p1", more: false } });
    onStage("ingame", "pregame");
    expect(router.route).toEqual(LIVE);
    go(details("p1"));
    onStage(null, "pregame");
    expect(router.route).toEqual(LIVE);
  });

  it("other stage changes leave the route alone, and so does staying in pregame", () => {
    go(details("p1"));
    onStage("pregame", "ingame");
    onStage("ingame", "menus");
    onStage("closed", "closed");
    onStage(null, "menus");
    expect(router.route).toEqual(details("p1"));
    onStage("pregame", "pregame"); // every live update repeats the stage
    expect(router.route).toEqual(details("p1"));
  });

  it("match back returns to details with more kept", () => {
    go(details("p1", false));
    showMore();
    expect(router.route).toEqual(details("p1", true));
    const from = router.route as Extract<Route, { name: "details" }>;
    go({ name: "match", matchId: "m1", focusPuuid: "p1", from: "recent", back: { puuid: from.puuid, more: from.more } });
    back();
    expect(router.route).toEqual(details("p1", true));
  });

  it("details back returns to live", () => {
    go(details("p1"));
    back();
    expect(router.route).toEqual(LIVE);
    back();
    expect(router.route).toEqual(LIVE);
  });

  it("leaving for Live forgets the open details; Match and Back keep them (Task 18 review, Minor 1)", async () => {
    const mara = { card: { puuid: "p1" } } as unknown as PlayerDetails;
    go(details("p1"));
    await openDetails.open("p1", async () => mara);
    go({ name: "match", matchId: "m1", focusPuuid: "p1", from: "recent", back: { puuid: "p1", more: false } });
    expect(get(openDetails)).toBe(mara);
    back();
    expect(get(openDetails)).toBe(mara); // Back from Match shows them at once
    back(); // Details → Live
    expect(get(openDetails)).toBeNull();
    // Entering Agent Select from a Match page forgets them too.
    go(details("p1"));
    await openDetails.open("p1", async () => mara);
    go({ name: "match", matchId: "m1", focusPuuid: "p1", from: "recent", back: { puuid: "p1", more: false } });
    onStage("menus", "pregame");
    expect(get(openDetails)).toBeNull();
  });

  it("My Stats is a route of its own; the router leaves it alone on Back and Show more", () => {
    go({ name: "mystats" });
    expect(router.route).toEqual({ name: "mystats" });
    back(); // the My Stats page has no Back button
    expect(router.route).toEqual({ name: "mystats" });
    showMore(); // and "Show more" is Details'
    expect(router.route).toEqual({ name: "mystats" });
    go(LIVE);
    expect(router.route).toEqual(LIVE);
  });

  it("a Match opened from My Stats goes Back to My Stats; one opened from Details still goes to Details (more kept)", () => {
    go({ name: "mystats" });
    go({ name: "match", matchId: "m1", focusPuuid: "me", from: "recent", back: "mystats" });
    back();
    expect(router.route).toEqual({ name: "mystats" });

    go(details("p1", true));
    go({ name: "match", matchId: "m2", focusPuuid: "p1", from: "recent", back: { puuid: "p1", more: true } });
    back();
    expect(router.route).toEqual(details("p1", true));
  });

  it("entering Agent Select leaves My Stats, and a Match opened from it, for Live", () => {
    go({ name: "mystats" });
    onStage("menus", "pregame");
    expect(router.route).toEqual(LIVE);
    go({ name: "match", matchId: "m1", focusPuuid: "me", from: "recent", back: "mystats" });
    onStage("menus", "pregame");
    expect(router.route).toEqual(LIVE);
  });

  it("showMore only applies to details", () => {
    showMore();
    expect(router.route).toEqual(LIVE);
  });
});

describe("go", () => {
  const root = { getBoundingClientRect: () => ({ left: 0, top: 0, width: 1000, height: 800 }) } as unknown as Element;

  beforeEach(() => vi.useFakeTimers());
  afterEach(() => {
    hideTip();
    vi.useRealTimers();
  });

  it("go hides the tip and resets the scroll container", () => {
    const container = { scrollTop: 240 } as HTMLElement;
    const action = scrollHost(container);

    const trigger = {
      getAttribute: (k: string) => (k === "data-tip" ? "Party · likely" : null),
      getBoundingClientRect: () => ({ left: 100, top: 100, width: 20, height: 20, right: 120, bottom: 120 }),
      querySelector: () => null,
    } as unknown as Element;
    tipOver({ closest: () => trigger } as unknown as Element, root);
    vi.advanceTimersByTime(450);
    expect(tipState.tip?.text).toBe("Party · likely");

    go(details("p1"));
    expect(tipState.tip).toBeNull();
    expect(container.scrollTop).toBe(0);
    expect(router.route).toEqual(details("p1"));

    action.destroy();
    container.scrollTop = 50;
    go(LIVE); // no host registered any more: nothing to reset, no throw
    expect(container.scrollTop).toBe(50);
  });

  it("go cancels a tip that is still pending", () => {
    const trigger = {
      getAttribute: (k: string) => (k === "data-tip" ? "A" : null),
      getBoundingClientRect: () => ({ left: 100, top: 100, width: 20, height: 20, right: 120, bottom: 120 }),
      querySelector: () => null,
    } as unknown as Element;
    tipOver({ closest: () => trigger } as unknown as Element, root);
    vi.advanceTimersByTime(200);
    go(details("p2"));
    vi.advanceTimersByTime(1000);
    expect(tipState.tip).toBeNull();
  });
});

describe("activeTab (spec §2.2)", () => {
  const match = (back: Extract<Route, { name: "match" }>["back"]): Route => ({
    name: "match",
    matchId: "m1",
    focusPuuid: "p1",
    from: "recent",
    back,
  });

  it("My Stats and a Match opened from it are the My Stats tab; everything else is Live", () => {
    expect(activeTab({ name: "mystats" })).toBe("mystats");
    expect(activeTab(match("mystats"))).toBe("mystats");
    expect(activeTab(LIVE)).toBe("live");
    expect(activeTab(details("p1"))).toBe("live");
    expect(activeTab(match({ puuid: "p1", more: false }))).toBe("live");
  });
});

describe("startPage (the first LiveState picks the start page)", () => {
  // `started` is module state: a fresh module per case.
  async function fresh() {
    vi.resetModules();
    return import("./router.svelte");
  }

  it("closed goes to My Stats; any other stage stays on Live", async () => {
    for (const stage of ["menus", "pregame", "ingame"] as const) {
      const r = await fresh();
      r.startPage(stage);
      expect(r.router.route, stage).toEqual(LIVE);
    }
    const r = await fresh();
    r.startPage("closed");
    expect(r.router.route).toEqual({ name: "mystats" });
  });

  it("once: a second call changes nothing, whatever the stage", async () => {
    const r = await fresh();
    r.startPage("menus");
    r.startPage("closed");
    expect(r.router.route).toEqual(LIVE);
  });

  it("never moves a user who has already navigated", async () => {
    const r = await fresh();
    r.go(details("p1"));
    r.startPage("closed");
    expect(r.router.route).toEqual(details("p1"));
  });
});
