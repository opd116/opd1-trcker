import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { back, go, onStage, router, scrollHost, showMore, type Route } from "./router.svelte";
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
