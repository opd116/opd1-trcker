import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { get } from "svelte/store";
import type { LiveState, MatchDetailsView, Meta, MyStats, PlayerCard, PlayerDetails } from "./types";

const state = (stage: LiveState["stage"], settled = true): LiveState => ({ stage, mapId: null, players: [], settled });

// A fresh module graph per test: stores.ts keeps the previous stage in module state.
async function load() {
  vi.resetModules();
  const stores = await import("./stores");
  const { go, openMatch, openProfile, router } = await import("./router.svelte");
  // `openMatchPage` is the router's open (the stores' `openMatch` is the payload slot).
  return { ...stores, go, openMatchPage: openMatch, openProfile, router };
}

describe("stores", () => {
  beforeEach(() => vi.resetModules());

  it("live and meta start null: nothing received yet, meta pending (spec §5.8)", async () => {
    const { live, meta, openDetails } = await load();
    expect(get(live)).toBeNull();
    expect(get(meta)).toBeNull();
    expect(get(openDetails)).toBeNull();
  });

  it("mystats starts undefined: nothing answered yet, the page draws its skeleton (spec §9.1)", async () => {
    const { mystats } = await load();
    expect(get(mystats)).toBeUndefined();
  });

  it("now is the wall clock when a subscription starts, not the module-load time (Minor 1)", async () => {
    vi.useFakeTimers();
    try {
      vi.setSystemTime(new Date("2026-09-29T10:00:00Z"));
      const { now } = await load();
      // Hours pass with nobody subscribed (VALORANT closed, no card mounted): no tick ever ran.
      vi.setSystemTime(new Date("2026-09-29T15:30:00Z"));
      const seen: number[] = [];
      const off = now.subscribe((v) => seen.push(v));
      expect(seen[seen.length - 1]).toBe(new Date("2026-09-29T15:30:00Z").getTime());
      // ...and the 30s tick still moves it on.
      await vi.advanceTimersByTimeAsync(30_000);
      expect(seen[seen.length - 1]).toBe(new Date("2026-09-29T15:30:30Z").getTime());
      off();
    } finally {
      vi.useRealTimers();
    }
  });

  it("applyLive stores the state", async () => {
    const { live, applyLive } = await load();
    applyLive(state("menus"));
    expect(get(live)?.stage).toBe("menus");
    applyLive(null);
    expect(get(live)).toBeNull();
  });

  it("entering Agent Select switches to Live; later pregame updates leave the screen alone", async () => {
    const { applyLive, go, router } = await load();
    applyLive(state("menus"));
    go("history");
    applyLive(state("pregame"));
    expect(router.screen).toBe("live");
    go("saved");
    applyLive(state("pregame")); // the same stage again: the user may be browsing
    expect(router.screen).toBe("saved");
    applyLive(state("ingame"));
    applyLive(state("menus"));
    applyLive(state("pregame")); // the next match
    expect(router.screen).toBe("live");
  });

  it("a null LiveState forgets the stage, so the next pregame counts as entering it", async () => {
    const { applyLive, go, router } = await load();
    applyLive(state("pregame"));
    applyLive(null);
    go("comp");
    applyLive(state("pregame"));
    expect(router.screen).toBe("live");
  });

  it("the v2 design always starts on Live: a closed first state no longer opens another page", async () => {
    const { applyLive, router } = await load();
    applyLive(state("closed"));
    expect(router.screen).toBe("live");
  });

  describe("Riot account switch", () => {
    const liveWith = (puuid: string, stage: LiveState["stage"] = "menus"): LiveState => ({
      stage,
      mapId: null,
      players: [{ puuid, isSelf: true } as PlayerCard],
      settled: true,
    });

    it("selfId prefers the live self card, keeps the last one a live state named, and uses My Stats only before that", async () => {
      const { applyLive, mystats, selfId } = await load();
      expect(get(selfId)).toBeNull();
      mystats.set({ puuid: "snapshot-account" } as MyStats);
      expect(get(selfId)).toBe("snapshot-account"); // nothing live yet: the snapshot is the only account known
      applyLive(liveWith("live-account"));
      expect(get(selfId)).toBe("live-account");
      // A self-less live state (the Closed state for an account with no stored player row) must not move the account
      // back through a stale My Stats snapshot.
      mystats.set({ puuid: "stale-snapshot" } as MyStats);
      applyLive({ ...liveWith("live-account"), players: [] });
      expect(get(selfId)).toBe("live-account");
    });

    it("selfId uses the live state's selfPuuid when it has no self card, and a switch to another one fires once", async () => {
      const { applyLive, mystats, onAccountChange, selfId } = await load();
      const closedWith = (puuid: string): LiveState => ({ stage: "closed", mapId: null, players: [], selfPuuid: puuid, settled: true });
      mystats.set({ puuid: "stale-snapshot" } as MyStats);
      // Rust carries the session's own account, so a closed state for a never-seen account (no self card, no My Stats)
      // still names it; the snapshot is only a fallback and never wins over it.
      applyLive(closedWith("a"));
      expect(get(selfId)).toBe("a");
      const seen: [string, string][] = [];
      const off = onAccountChange((from, to) => seen.push([from, to]));
      applyLive(closedWith("b"));
      expect(get(selfId)).toBe("b");
      expect(seen).toEqual([["a", "b"]]);
      off();
    });

    it("fires once for a real account switch, never on startup, a dropped connection or the same account", async () => {
      const { applyLive, onAccountChange } = await load();
      const seen: [string, string][] = [];
      const off = onAccountChange((from, to) => seen.push([from, to]));
      applyLive(liveWith("a")); // the first account: not a switch
      applyLive(liveWith("a", "pregame"));
      applyLive(null); // a dropped connection is "unknown", not another account
      applyLive({ ...liveWith("a"), players: [] }); // an empty live state keeps the last known account
      expect(seen).toEqual([]);
      applyLive(liveWith("b"));
      expect(seen).toEqual([["a", "b"]]);
      applyLive(liveWith("b", "ingame"));
      applyLive(liveWith("b", "menus"));
      expect(seen).toEqual([["a", "b"]]);
      applyLive(liveWith("c"));
      expect(seen).toEqual([
        ["a", "b"],
        ["b", "c"],
      ]);
      off();
      applyLive(liveWith("d"));
      expect(seen).toHaveLength(2);
    });

    it("a stale My Stats snapshot cannot move a known live account back (no switch fires)", async () => {
      const { applyLive, mystats, onAccountChange, selfId } = await load();
      applyLive(liveWith("a"));
      const seen: [string, string][] = [];
      const off = onAccountChange((from, to) => seen.push([from, to]));
      mystats.set({ puuid: "stale-b" } as MyStats); // My Stats from another account, the live state still names A
      expect(get(selfId)).toBe("a");
      applyLive({ ...liveWith("a"), players: [] }); // and a live state that names nobody
      expect(get(selfId)).toBe("a");
      expect(seen).toEqual([]);
      off();
    });

    it("live B self and My Stats B arriving in either order fire exactly once", async () => {
      // Live first, then the My Stats push.
      const first = await load();
      first.applyLive(liveWith("a"));
      const seen1: [string, string][] = [];
      const off1 = first.onAccountChange((from, to) => seen1.push([from, to]));
      first.applyLive(liveWith("b"));
      first.mystats.set({ puuid: "b" } as MyStats);
      expect(seen1).toEqual([["a", "b"]]);
      off1();

      // My Stats first (it does not move the account while the live self card still names A), then the live state.
      const second = await load();
      second.applyLive(liveWith("a"));
      const seen2: [string, string][] = [];
      const off2 = second.onAccountChange((from, to) => seen2.push([from, to]));
      second.mystats.set({ puuid: "b" } as MyStats);
      expect(seen2).toEqual([]);
      second.applyLive(liveWith("b"));
      expect(seen2).toEqual([["a", "b"]]);
      off2();
    });

    it("a My Stats-only change signals a switch while the live state has no self card (the documented fallback)", async () => {
      const { applyLive, mystats, onAccountChange, selfId } = await load();
      // The fallback case: Rust's Closed live state for an account with no stored player row names nobody.
      applyLive({ stage: "closed", mapId: null, players: [], settled: true });
      const seen: [string, string][] = [];
      const off = onAccountChange((from, to) => seen.push([from, to]));
      mystats.set({ puuid: "a" } as MyStats); // the first account learned: not a switch
      expect(get(selfId)).toBe("a");
      expect(seen).toEqual([]);
      mystats.set({ puuid: "b" } as MyStats); // the switch is only visible through My Stats
      expect(get(selfId)).toBe("b");
      expect(seen).toEqual([["a", "b"]]);
      // The live state naming B afterwards is the same account: no second switch.
      applyLive(liveWith("b"));
      expect(seen).toEqual([["a", "b"]]);
      off();
    });

    it("a switch closes the open drawer, and a scoreboard back to its source screen, instead of the old account's numbers", async () => {
      const { applyLive, openMatchPage, openProfile, router } = await load();
      applyLive(liveWith("a"));
      openProfile("some-player");
      expect(router.profile).toBe("some-player");
      applyLive(liveWith("b"));
      expect(router.profile).toBeNull();

      openMatchPage("some-match", { from: "history", focusPuuid: "b" });
      expect(router.matchId).toBe("some-match");
      applyLive(liveWith("c"));
      expect(router.matchId).toBeNull();
      expect(router.screen).toBe("history");
    });
  });

  describe("myStatsForSelf", () => {
    const statsOf = (puuid: string) => ({ puuid }) as MyStats;
    const closedWith = (puuid: string): LiveState => ({ stage: "closed", mapId: null, players: [], selfPuuid: puuid, settled: true });

    it("passes a snapshot only for the account on screen, keeping the tri-state each consumer relies on", async () => {
      const { applyLive, myStatsForSelf, mystats, selfId } = await load();
      expect(get(myStatsForSelf)).toBeUndefined(); // nothing answered yet
      mystats.set(null);
      expect(get(myStatsForSelf)).toBeNull(); // Rust answered "unknown": — frames, no numbers
      // Answered before any live state: the startup ordering — the snapshot is the only account known so far.
      mystats.set(statsOf("a"));
      expect(get(selfId)).toBe("a");
      expect(get(myStatsForSelf)).toEqual(statsOf("a"));
      applyLive(closedWith("a"));
      expect(get(myStatsForSelf)).toEqual(statsOf("a")); // the same account: kept
      applyLive(closedWith("b"));
      // The switch, while Rust's push for B is in flight: A's numbers are never passed through.
      expect(get(myStatsForSelf)).toBeUndefined();
      mystats.set(statsOf("b"));
      expect(get(myStatsForSelf)).toEqual(statsOf("b"));
    });

    it("a null answer stays null after a switch (— frames, never another account's numbers)", async () => {
      const { applyLive, myStatsForSelf, mystats } = await load();
      applyLive(closedWith("a"));
      mystats.set(statsOf("a"));
      applyLive(closedWith("b"));
      mystats.set(null);
      expect(get(myStatsForSelf)).toBeNull();
    });
  });

  describe("startFeeds under Tauri", () => {
    const good: Meta = { agents: {}, maps: { m1: { name: "Ascent" } }, tiers: {} };

    afterEach(() => {
      vi.useRealTimers();
      vi.restoreAllMocks(); // the console.error spy from `underTauri`
      vi.unstubAllEnvs();
      vi.doUnmock("@tauri-apps/api/core");
      vi.doUnmock("./api");
    });

    /** Stores wired to a fake Tauri bridge whose `meta` command rejects `failures` times before it resolves. */
    async function underTauri(failures: number, myStatsAnswer: () => Promise<MyStats | null> = async () => null) {
      let metaCalls = 0;
      const invoke = vi.fn(async (cmd: string, _args?: unknown) => {
        if (cmd === "live_state") return state("closed");
        if (cmd === "my_stats") return myStatsAnswer();
        if (cmd === "ui_log") return undefined;
        if (cmd === "meta") {
          if (metaCalls++ < failures) throw new Error("asset tables unavailable");
          return good;
        }
        throw new Error(`unexpected ${cmd}`);
      });
      const handlers = new Map<string, (e: { payload: unknown }) => void>();
      const listen = vi.fn(async (ev: string, cb: (e: { payload: unknown }) => void) => {
        handlers.set(ev, cb);
        return () => {};
      });
      vi.doMock("@tauri-apps/api/core", () => ({ isTauri: () => true, invoke }));
      vi.doMock("./api", () => ({ tauriBridge: { invoke, listen }, myStats: () => invoke("my_stats") }));
      vi.stubEnv("DEV", true); // the console sink of the reporter is a development-build feature: pinned, not inherited
      vi.spyOn(console, "error").mockImplementation(() => {});
      const emit = (ev: string, payload: unknown) => handlers.get(ev)!({ payload });
      return { ...(await load()), invoke, emit, metaCalls: () => metaCalls };
    }

    it("a details event reaches the open-details slot and wins over the answer in flight (Task 18 review, Minor 2)", async () => {
      const { startFeeds, openDetails, emit } = await underTauri(0);
      await startFeeds();
      const d = (marker: string) => ({ card: { puuid: "p1" }, marker }) as unknown as PlayerDetails;
      let answer!: (v: PlayerDetails) => void;
      const done = openDetails.open("p1", () => new Promise<PlayerDetails>((res) => (answer = res)));
      emit("details", d("event"));
      answer(d("snapshot"));
      await done;
      expect(get(openDetails)).toEqual(d("event"));
    });

    it("a match event reaches the open-match slot, wins over the answer in flight, and only for the open match (R150)", async () => {
      const { startFeeds, openMatch, emit } = await underTauri(0);
      await startFeeds();
      const v = (matchId: string, marker: string) => ({ matchId, marker }) as unknown as MatchDetailsView;
      let answer!: (v: MatchDetailsView) => void;
      const done = openMatch.open("m1", () => new Promise<MatchDetailsView>((res) => (answer = res)));
      emit("match", v("m2", "another match")); // ignored
      expect(get(openMatch)).toBeNull();
      emit("match", v("m1", "names"));
      answer(v("m1", "snapshot"));
      await done;
      expect(get(openMatch)).toEqual(v("m1", "names"));
      openMatch.close(); // the page was left: an event after that changes nothing
      emit("match", v("m1", "after leaving"));
      expect(get(openMatch)).toBeNull();
    });

    const stats = (marker: string) => ({ marker }) as unknown as MyStats;

    it("startFeeds asks my_stats once and applies the answer", async () => {
      const { startFeeds, mystats, invoke } = await underTauri(0, async () => stats("snapshot"));
      await startFeeds();
      expect(get(mystats)).toEqual(stats("snapshot"));
      expect(invoke.mock.calls.filter(([cmd]) => cmd === "my_stats")).toHaveLength(1);
    });

    it("a null my_stats answer is not the skeleton: the page settles on its frames", async () => {
      const { startFeeds, mystats } = await underTauri(0, async () => null);
      await startFeeds();
      expect(get(mystats)).toBeNull();
    });

    it("a mystats event replaces the data, and wins over the snapshot still in flight", async () => {
      let answer!: (v: MyStats | null) => void;
      const { startFeeds, mystats, emit } = await underTauri(0, () => new Promise<MyStats | null>((res) => (answer = res)));
      const started = startFeeds();
      await vi.waitFor(() => expect(answer).toBeTypeOf("function"));
      emit("mystats", stats("event"));
      answer(stats("snapshot"));
      await started;
      expect(get(mystats)).toEqual(stats("event"));
      emit("mystats", stats("newer")); // later events keep replacing it
      expect(get(mystats)).toEqual(stats("newer"));
    });

    it("a failed my_stats() does not leave the skeleton up forever: null now, a later event still fills it", async () => {
      const { startFeeds, mystats, emit } = await underTauri(0, async () => {
        throw new Error("db busy");
      });
      await startFeeds();
      expect(get(mystats)).toBeNull();
      expect(console.error).toHaveBeenCalledWith("mystats", expect.any(Error));
      emit("mystats", stats("event"));
      expect(get(mystats)).toEqual(stats("event"));
    });

    it("a failed my_stats() reaches the app log: one ui_log line, tagged with the site", async () => {
      const { startFeeds, invoke } = await underTauri(0, async () => {
        throw new Error("db busy");
      });
      await startFeeds();
      const sent = invoke.mock.calls.filter(([cmd]) => cmd === "ui_log");
      expect(sent).toHaveLength(1);
      expect(sent[0][1]).toEqual({ level: "error", message: expect.stringMatching(/^mystats: Error: db busy/) });
    });

    it("refreshMyStats asks again only while the page has nothing, and never overwrites what landed meanwhile", async () => {
      let calls = 0;
      let answer!: (v: MyStats | null) => void;
      const { startFeeds, mystats, refreshMyStats, emit } = await underTauri(0, () => {
        calls++;
        return calls === 1 ? Promise.resolve(null) : new Promise<MyStats | null>((res) => (answer = res));
      });
      await startFeeds();
      expect(get(mystats)).toBeNull();
      const refreshing = refreshMyStats(); // the page opened with nothing to show
      emit("mystats", stats("event")); // an event lands first
      answer(stats("snapshot"));
      await refreshing;
      expect(get(mystats)).toEqual(stats("event"));
      await refreshMyStats(); // data on screen: no more calls
      expect(calls).toBe(2);
    });

    it("R163: a meta event replaces the tables, and ends the invoke retries", async () => {
      vi.useFakeTimers();
      const fresh: Meta = { agents: { a1: { name: "Gekko" } }, maps: {}, tiers: {} };
      const { startFeeds, meta, emit, metaCalls } = await underTauri(Number.POSITIVE_INFINITY);
      await startFeeds();
      await vi.advanceTimersByTimeAsync(0);
      expect(get(meta)).toEqual({ agents: {}, maps: {}, tiers: {} }); // the failed first invoke
      emit("meta", fresh); // Rust loaded the tables by another route
      expect(get(meta)).toEqual(fresh);
      const calls = metaCalls();
      await vi.advanceTimersByTimeAsync(120_000);
      expect(metaCalls()).toBe(calls); // the retry loop stopped
      emit("meta", good); // a patch refreshed them mid-session
      expect(get(meta)).toEqual(good);
    });

    it("R163: a live event gives a settled Details or Match page its one retry", async () => {
      const { startFeeds, openDetails, openMatch, emit } = await underTauri(0);
      await startFeeds();
      const d = (marker: string) => ({ card: { puuid: "p1" }, marker }) as unknown as PlayerDetails;
      let n = 0;
      const fetch = vi.fn(async () => {
        if (n++ === 0) throw new Error("db busy");
        return d("late");
      });
      await openDetails.open("p1", fetch, { settle: () => d("settled") }).catch(() => {});
      expect((get(openDetails) as unknown as { marker: string }).marker).toBe("settled");
      emit("live", state("menus"));
      await vi.waitFor(() => expect((get(openDetails) as unknown as { marker: string }).marker).toBe("late"));
      expect(get(openMatch)).toBeNull();
    });

    it("a rejected meta() does not leave meta pending: EMPTY_META now, the real tables on a retry", async () => {
      vi.useFakeTimers();
      const { startFeeds, meta, live, invoke } = await underTauri(1);
      await startFeeds(); // resolves without waiting for meta
      expect(get(live)?.stage).toBe("closed"); // the live feed is not held up by a failing meta()
      await vi.advanceTimersByTimeAsync(0);
      expect(get(meta)).toEqual({ agents: {}, maps: {}, tiers: {} }); // loaded-miss "—", never a skeleton forever
      expect(get(meta)).not.toBeNull();
      expect(console.error).toHaveBeenCalledTimes(1);
      // Only the first failure is reported, to the app log as well: one ui_log line for the `meta` site.
      const sent = invoke.mock.calls.filter(([cmd]) => cmd === "ui_log");
      expect(sent).toHaveLength(1);
      expect(sent[0][1]).toEqual({ level: "error", message: expect.stringMatching(/^meta: Error: asset tables unavailable/) });

      await vi.advanceTimersByTimeAsync(2_000);
      expect(get(meta)).toEqual(good);
      expect(invoke.mock.calls.filter(([cmd]) => cmd === "meta")).toHaveLength(2);
    });

    it("a healthy meta() is set once with no retry", async () => {
      vi.useFakeTimers();
      const { startFeeds, meta, metaCalls, invoke } = await underTauri(0);
      await startFeeds();
      await vi.advanceTimersByTimeAsync(0);
      expect(get(meta)).toEqual(good);
      await vi.advanceTimersByTimeAsync(120_000);
      expect(metaCalls()).toBe(1);
      expect(console.error).not.toHaveBeenCalled();
      expect(invoke.mock.calls.filter(([cmd]) => cmd === "ui_log")).toHaveLength(0); // nothing failed: nothing in the app log
    });
  });
});
