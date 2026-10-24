import { get } from "svelte/store";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { MatchDetailsView, PlayerDetails } from "./types";

// The router talks to Rust through api.ts: stub every door it uses, so the calls can be counted.
const api = vi.hoisted(() => ({
  getPlayerDetails: vi.fn(),
  closeDetails: vi.fn(),
  getMatchDetails: vi.fn(),
  closeMatch: vi.fn(),
  setGlance: vi.fn(),
}));
vi.mock("./api", () => api);

const { openDetails, openMatch: matchSlot } = await import("./boot");
const R = await import("./router.svelte");
const { backFromMatch, closeProfile, go, onStage, openMatch, openProfile, router, scrollHost, setGlance, startPage } = R;
const { hideTip, tipOver, tipState } = await import("./tooltip.svelte");

const details = (puuid: string) => ({ card: { puuid } }) as unknown as PlayerDetails;
const view = (matchId: string) => ({ matchId }) as unknown as MatchDetailsView;

beforeEach(() => {
  for (const f of Object.values(api)) f.mockReset();
  api.getPlayerDetails.mockImplementation(async (p: string) => details(p));
  api.getMatchDetails.mockImplementation(async (m: string) => view(m));
  api.closeDetails.mockResolvedValue(undefined);
  api.closeMatch.mockResolvedValue(undefined);
  api.setGlance.mockResolvedValue(undefined);
  go("live");
  router.lastProfile = null;
  router.glance = false;
  for (const f of Object.values(api)) f.mockClear();
});

const flush = () => new Promise((r) => setTimeout(r, 0));

describe("router state", () => {
  it("starts on Live with nothing open", async () => {
    vi.resetModules();
    const fresh = await import("./router.svelte");
    expect({ ...fresh.router }).toEqual({
      screen: "live",
      matchId: null,
      matchFocus: null,
      matchFrom: null,
      encFrom: null,
      profile: null,
      lastProfile: null,
      glance: false,
    });
  });

  it("go switches the screen and closes the drawer and any scoreboard", async () => {
    openProfile("p1");
    go("saved");
    expect(router.screen).toBe("saved");
    expect(router.profile).toBeNull();
    expect(api.closeDetails).toHaveBeenCalledWith("p1");
    openMatch("m1", { from: "history", focusPuuid: "me" });
    go("skins");
    expect(router).toMatchObject({ screen: "skins", matchId: null, matchFocus: null, matchFrom: null });
    expect(api.closeMatch).toHaveBeenCalledWith("m1");
  });

  it("entering Agent Select goes to Live; other stage changes leave the screen alone", () => {
    go("history");
    onStage("pregame", "ingame");
    onStage("ingame", "menus");
    onStage(null, "menus");
    onStage("pregame", "pregame");
    expect(router.screen).toBe("history");
    openProfile("p1");
    onStage("menus", "pregame");
    expect(router.screen).toBe("live");
    expect(router.profile).toBeNull();
    go("comp");
    onStage(null, "pregame");
    expect(router.screen).toBe("live");
  });

  it("startPage is a no-op (the v2 design always starts on Live)", () => {
    go("saved");
    startPage("closed", true);
    expect(router.screen).toBe("saved");
  });
});

describe("profile drawer", () => {
  it("openProfile sets profile + lastProfile and fetches player_details into openDetails", async () => {
    openProfile("p1");
    expect(router.profile).toBe("p1");
    expect(router.lastProfile).toBe("p1");
    expect(api.getPlayerDetails).toHaveBeenCalledWith("p1");
    await flush();
    expect(get(openDetails)).toEqual(details("p1"));
  });

  it("opening the same player again fetches nothing; another player swaps without close_details", () => {
    openProfile("p1");
    openProfile("p1");
    expect(api.getPlayerDetails).toHaveBeenCalledTimes(1);
    openProfile("p2");
    expect(router.profile).toBe("p2");
    expect(api.closeDetails).not.toHaveBeenCalled();
  });

  it("closeProfile clears the slot, tells Rust, keeps lastProfile and returns focus to the trigger", async () => {
    vi.useFakeTimers();
    try {
      const trigger = { isConnected: true, focus: vi.fn() } as unknown as HTMLElement;
      openProfile("p1", trigger);
      closeProfile();
      expect(router.profile).toBeNull();
      expect(router.lastProfile).toBe("p1");
      expect(get(openDetails)).toBeNull();
      expect(api.closeDetails).toHaveBeenCalledWith("p1");
      vi.runAllTimers();
      expect(trigger.focus).toHaveBeenCalledWith({ preventScroll: true });
      closeProfile(); // nothing open: nothing happens
      expect(api.closeDetails).toHaveBeenCalledTimes(1);
    } finally {
      vi.useRealTimers();
    }
  });

  it("a trigger that left the page gets no focus", () => {
    vi.useFakeTimers();
    try {
      const trigger = { isConnected: false, focus: vi.fn() } as unknown as HTMLElement;
      openProfile("p1", trigger);
      closeProfile();
      vi.runAllTimers();
      expect(trigger.focus).not.toHaveBeenCalled();
    } finally {
      vi.useRealTimers();
    }
  });

  it("the settled (failed) state keeps what the live card knew", async () => {
    vi.useFakeTimers();
    try {
      R.setLiveCardLookup((p) => (p === "p9" ? ({ puuid: "p9", level: 77 } as never) : undefined));
      api.getPlayerDetails.mockRejectedValue(new Error("boom"));
      openProfile("p9");
      await vi.runAllTimersAsync();
      expect(get(openDetails)?.card.puuid).toBe("p9");
    } finally {
      R.setLiveCardLookup(() => undefined);
      vi.useRealTimers();
    }
  });
});

describe("match scoreboard", () => {
  it("openMatch shows the scoreboard on History, closes the drawer and fetches match_details", async () => {
    go("saved");
    openProfile("p1");
    openMatch("m1", { from: "encounter", focusPuuid: "p1" });
    expect(router).toMatchObject({ screen: "history", matchId: "m1", matchFocus: "p1", matchFrom: "encounter", profile: null });
    expect(router.encFrom).toEqual({ puuid: "p1", screen: "saved" }); // defaults to the drawer that was open
    expect(api.getMatchDetails).toHaveBeenCalledWith("m1", "p1");
    await flush();
    expect(get(matchSlot)).toEqual(view("m1"));
  });

  it("encounter / profile → back to the screen under the drawer, the drawer reopened", () => {
    openMatch("m1", { from: "profile", focusPuuid: "p1", encFrom: { puuid: "p1", screen: "comp" } });
    backFromMatch();
    expect(router).toMatchObject({ screen: "comp", matchId: null, profile: "p1", encFrom: null });
    expect(api.closeMatch).toHaveBeenCalledWith("m1");
    expect(get(matchSlot)).toBeNull();
  });

  it("recap / lastMatch → Live, comp → Competitive, history → the History list", () => {
    for (const [from, to] of [
      ["recap", "live"],
      ["lastMatch", "live"],
      ["comp", "comp"],
      ["history", "history"],
    ] as const) {
      go("about");
      openMatch("m1", { from, focusPuuid: "me" });
      backFromMatch();
      expect(router.screen, from).toBe(to);
      expect(router.matchId, from).toBeNull();
      expect(router.profile, from).toBeNull();
    }
  });

  it("another match replacing the open one closes it first", () => {
    openMatch("m1", { from: "history", focusPuuid: "me" });
    openMatch("m2", { from: "history", focusPuuid: "me" });
    expect(api.closeMatch).toHaveBeenCalledWith("m1");
    expect(router.matchId).toBe("m2");
  });

  it("backFromMatch with no scoreboard open does nothing", () => {
    go("saved");
    backFromMatch();
    expect(router.screen).toBe("saved");
  });
});

describe("scroll and tooltip", () => {
  /** What App does: every page gets a fresh container, registered as it mounts. */
  const mount = (current: { host: HTMLElement; action: { destroy(): void } } | null) => {
    current?.action.destroy();
    const host = { scrollTop: 0 } as HTMLElement;
    return { host, action: scrollHost(host) };
  };

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
    tipOver({ closest: () => trigger } as unknown as Element);
    vi.advanceTimersByTime(450);
    expect(tipState.tip?.text).toBe("Party · likely");
    go("history");
    expect(tipState.tip).toBeNull();
    expect(container.scrollTop).toBe(0);
    action.destroy();
    container.scrollTop = 50;
    go("live"); // no host registered any more: nothing to reset, no throw
    expect(container.scrollTop).toBe(50);
  });

  it("History list → match → Back puts the list where it was", () => {
    go("history");
    let page = mount(null);
    page.host.scrollTop = 420;
    openMatch("m1", { from: "history", focusPuuid: "me" });
    page = mount(page); // the scoreboard's container
    expect(page.host.scrollTop).toBe(0);
    backFromMatch();
    page = mount(page);
    expect(page.host.scrollTop).toBe(420);
    page.action.destroy();
  });

  it("an encounter's Back restores the screen under the drawer; a plain go starts at the top", () => {
    go("saved");
    let page = mount(null);
    page.host.scrollTop = 310;
    openProfile("p1");
    openMatch("m1", { from: "encounter", focusPuuid: "p1" });
    page = mount(page);
    backFromMatch();
    page = mount(page);
    expect(page.host.scrollTop).toBe(310);
    go("history");
    page = mount(page);
    expect(page.host.scrollTop).toBe(0);
    page.action.destroy();
  });
});

describe("glance", () => {
  it("setGlance flips the flag and asks the backend to resize the window", () => {
    setGlance(true);
    expect(router.glance).toBe(true);
    expect(api.setGlance).toHaveBeenCalledWith(true);
    setGlance(false);
    expect(router.glance).toBe(false);
    expect(api.setGlance).toHaveBeenLastCalledWith(false);
  });
});
