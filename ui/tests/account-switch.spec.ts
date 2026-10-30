// Riot account switch (fix/account-switch-data): the app supports switching Riot accounts while it stays open (the
// Riot client's session drops and reconnects for the new account; Rust replaces the whole live state, so its self card's
// puuid changes). An open screen must not keep the previous account's data: History, Skins and Saved re-ask for the
// self card's account and drop a response that was issued for the old one, and the profile drawer / scoreboard (whose
// values are relative to the user) close instead of showing the old account's numbers.
//
// The DEV browser has no Rust, so this spec puts a fake behind Tauri's own door (window.isTauri,
// __TAURI_INTERNALS__.invoke) exactly like close-match.spec.ts. The fake commands are account-aware: a `__account`
// field the test flips before emitting the switch, and every answer differs by account so a stale draw is visible.
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { expect, test, type Page } from "@playwright/test";
import { detailsFor, emptyDetails, historyMatchId, matchFor, MAPS, PUUID } from "../src/lib/demo/cdv8";
import { SCENES } from "../src/lib/demo/scenes";
import { demoStatus } from "../src/lib/demo/v2/status";
import { DEFAULT_SETTINGS } from "../src/lib/demo/v2/store";
import type {
  Collection,
  HistoryPage,
  HistoryRow,
  LiveState,
  OwnedSkin,
  SavedPlayer,
} from "../src/lib/types";
import { APP_READY, gotoApp, NOW, routeMedia, UI_DIR } from "./harness";

/** Account A is the demo scene's user (Fahad); account B is the account the Riot client switches to. */
const A_PUUID = PUUID.fahad!;
const B_PUUID = "bbbbbbbb-0000-4000-8000-00000000000b";
const MARA = PUUID.mara!;

const LIVE_A = SCENES.ingame.live!;
const LIVE_B: LiveState = (() => {
  const next = structuredClone(LIVE_A);
  const self = next.players.find((p) => p.isSelf)!;
  self.puuid = B_PUUID;
  self.name = { kind: "visible", name: "Alt", tag: "B0B0" };
  next.selfPuuid = B_PUUID;
  return next;
})();
// The switch keeps the live stage on purpose: History and Skins already refresh on a stage change (a pre-existing path),
// and that must not be what makes these cases pass — only the account change may.
expect(LIVE_A.stage).toBe(LIVE_B.stage);

/**
 * The real-client edge: Riot switched accounts while VALORANT is closed. The live state names nobody on screen, and a
 * never-seen account has no self card and no computable My Stats — only the live state's `selfPuuid` names the account.
 */
const CLOSED_A: LiveState = { stage: "closed", mapId: null, players: [], selfPuuid: A_PUUID, settled: true };
const CLOSED_B: LiveState = { stage: "closed", mapId: null, players: [], selfPuuid: B_PUUID, settled: true };
expect(CLOSED_A.stage).toBe(CLOSED_B.stage);

/** The two accounts' own newest matches: different match ids, so a stale row is visible by id. */
const ROW_A = historyMatchId(0);
const ROW_B = historyMatchId(5);

const HOUR = 60 * 60_000;
const START = Date.UTC(2026, 8, 29, 20, 30);

const historyRow = (matchId: string, startMs: number): HistoryRow => ({
  matchId,
  mapId: MAPS.Ascent,
  queueId: "competitive",
  startMs,
  durationMs: 38 * 60_000,
  result: "win",
  roundsWon: 13,
  roundsLost: 9,
  agentId: null,
  kills: 22,
  deaths: 16,
  assists: 3,
  kd: 1.38,
  acs: 265,
  hsPct: 0.29,
  rrEarned: 18,
  tierAfter: 21,
});

const historyPage = (matchId: string, startMs: number): HistoryPage => {
  const rows = [historyRow(matchId, startMs)];
  return { rows, total: 1, competitiveTotal: 1, allTotal: 1 };
};

const WEAPON = "9c82e19d-4575-0200-1a81-3eacf00cf872";

const ownedSkin = (skinId: string, name: string, vp: number): OwnedSkin => ({
  skinId,
  levelId: "578e9077-4f88-260c-e54c-b988425c60e4",
  name,
  weaponId: WEAPON,
  edition: "select",
  vp,
});

/** A's collection is one 875 VP skin; B's is two priced skins summing to 2,550 VP: the header total tells them apart. */
const collection = (items: OwnedSkin[]): Collection => ({
  items,
  weapons: [{ id: WEAPON, name: "Vandal" }],
  fetchedMs: NOW.getTime() - HOUR,
});

const collectionA = collection([ownedSkin("11111111-0000-4000-8000-000000000001", "Alpha", 875)]);
const collectionB = collection([
  ownedSkin("22222222-0000-4000-8000-000000000002", "Beta", 1275),
  ownedSkin("33333333-0000-4000-8000-000000000003", "Gamma", 1275),
]);

const SAVED_PUUID = "cccccccc-0000-4000-8000-00000000000c";

/** One saved player; AS TEAMMATE games differ by account (3 against A's account, 9 against B's). */
const savedRow = (withGames: number, withWins: number, againstGames: number): SavedPlayer => ({
  puuid: SAVED_PUUID,
  name: { kind: "visible", name: "Nova", tag: "0001" },
  agentId: null,
  tier: 21,
  with: { games: withGames, wins: withWins, losses: withGames - withWins },
  against: { games: againstGames, wins: againstGames, losses: 0 },
  note: "",
  savedMs: START - 2 * 24 * HOUR,
  lastSeenMs: START - HOUR,
  lastSeenMapId: MAPS.Ascent,
});

const LIVE = { a: LIVE_A, b: LIVE_B };
const CLOSED = { a: CLOSED_A, b: CLOSED_B };
const STATUS = {
  ingame: { a: demoStatus(LIVE_A, new URLSearchParams()), b: demoStatus(LIVE_B, new URLSearchParams()) },
  closed: { a: demoStatus(CLOSED_A, new URLSearchParams()), b: demoStatus(CLOSED_B, new URLSearchParams()) },
};
const HISTORY = { a: historyPage(ROW_A, START), b: historyPage(ROW_B, START - HOUR) };
const COLLECTION = { a: collectionA, b: collectionB };
const SAVED = { a: [savedRow(3, 2, 2)], b: [savedRow(9, 8, 5)] };

/** Mara's drawer: its encounter rows open scoreboards (the drawer → scoreboard → switch case). */
const MARA_DETAILS = detailsFor("mara", LIVE_A.players.find((c) => c.puuid === MARA), LIVE_A.players);

/** The Saved row's drawer: a settled, minimal profile (the closed-game case only needs it open and then closed). */
const SAVED_DETAILS = emptyDetails({
  puuid: SAVED_PUUID,
  name: { kind: "visible", name: "Nova", tag: "0001" },
  cardId: "",
  agentId: null,
  party: null,
  encounters: null,
  team: "ally",
  isSelfParty: false,
  isSelf: false,
});

const data = {
  meta: JSON.parse(readFileSync(join(UI_DIR, "src", "lib", "demo", "meta.json"), "utf8")) as unknown,
  live: { ingame: LIVE, closed: CLOSED },
  status: STATUS,
  settings: DEFAULT_SETTINGS,
  history: HISTORY,
  collection: COLLECTION,
  saved: SAVED,
  details: { [MARA]: MARA_DETAILS, [SAVED_PUUID]: SAVED_DETAILS },
  matches: {
    [ROW_A]: matchFor(ROW_A, LIVE_A.players),
    [ROW_B]: matchFor(ROW_B, LIVE_A.players),
    ...Object.fromEntries(MARA_DETAILS.encounters.map((e) => [e.matchId, matchFor(e.matchId, LIVE_A.players)])),
  },
};

/** How the fake starts: mid-match (A has a self card) or with VALORANT closed (only the session's `selfPuuid`). */
type Start = "ingame" | "closed";

type Account = "a" | "b";

type Fake = {
  __calls: string[];
  __account: Account;
  __hangHistory: boolean;
  __hungHistory: boolean;
  __releaseHistory?: () => void;
  __emit: (event: string, payload: unknown) => void;
};

const calls = (page: Page): Promise<string[]> => page.evaluate(() => (window as unknown as Fake).__calls.slice());
const nav = (page: Page, label: string) => page.locator(`nav.rail button[aria-label="${label}"]`);

/** The app's own double-click guard (lib/clickGuard.ts): a click within 400 ms of opening a drawer may be swallowed. */
async function guardDone(page: Page): Promise<void> {
  await expect
    .poll(() => page.evaluate(`import("/src/lib/clickGuard.ts").then(({ cardOpenGuard }) => cardOpenGuard.swallows())`))
    .toBe(false);
}

/** Boot the app as if inside Tauri, with account-aware answers for every command the screens ask. */
async function bootInTauri(page: Page, start: Start = "ingame"): Promise<void> {
  await page.addInitScript((d) => {
    const w = window as unknown as Record<string, unknown> & Fake;
    w.isTauri = true;
    w.__calls = [];
    w.__account = "a";
    w.__hangHistory = false;
    w.__hungHistory = false;
    let n = 0;
    const cbs = new Map<number, (e: unknown) => void>();
    const listeners = new Map<string, number>();
    w.__emit = (event, payload) => cbs.get(listeners.get(event)!)?.({ event, id: 0, payload });
    w.__TAURI_INTERNALS__ = {
      transformCallback: (cb: (e: unknown) => void) => {
        cbs.set(++n, cb);
        return n;
      },
      unregisterCallback: (id: number) => cbs.delete(id),
      convertFileSrc: (p: string) => p,
      // The title bar's window controls read the window's label (@tauri-apps/api/window).
      metadata: { currentWindow: { label: "main" }, currentWebview: { windowLabel: "main", label: "main" } },
      invoke: async (cmd: string, args?: Record<string, unknown>) => {
        if (cmd === "plugin:event|listen") {
          listeners.set(String(args!.event), Number(args!.handler));
          return ++n;
        }
        if (cmd.startsWith("plugin:")) return null;
        w.__calls.push(cmd);
        const account = w.__account;
        const liveAt = (d.live as Record<Start, typeof d.live.ingame>)[d.start];
        const statusAt = (d.status as Record<Start, typeof d.status.ingame>)[d.start];
        switch (cmd) {
          case "meta":
            return structuredClone(d.meta);
          case "live_state":
            return structuredClone(liveAt[account]);
          case "app_status":
            return structuredClone(statusAt[account]);
          case "get_settings":
            return structuredClone(d.settings);
          case "player_marks":
            return { saved: [], notes: {}, notSmurf: [] };
          case "my_stats":
            return null;
          case "profile_extras":
            return { puuid: args?.puuid, maps: [], liveMates: [] };
          case "player_details":
            return structuredClone((d.details as Record<string, unknown>)[String(args?.puuid)] ?? null);
          case "match_details":
            return structuredClone((d.matches as Record<string, unknown>)[String(args?.matchId)] ?? null);
          case "match_history": {
            if (w.__hangHistory && !w.__hungHistory) {
              // The first page is asked for but does not answer until the test lets it: the answer is the account the
              // call was issued for ("a"), even after the switch.
              w.__hungHistory = true;
              return new Promise((resolve) => {
                w.__releaseHistory = () => resolve(structuredClone(d.history[account]));
              });
            }
            return structuredClone(d.history[account]);
          }
          case "collection":
            return structuredClone(d.collection[account]);
          case "saved_players":
            return structuredClone(d.saved[account]);
          default:
            return null;
        }
      },
    };
  }, { ...data, start });
  await routeMedia(page);
  await page.clock.setFixedTime(NOW);
  await gotoApp(page, "/", APP_READY);
  if (start === "ingame") await page.locator("main [data-row]").first().waitFor();
  else await page.locator("nav.rail button").first().waitFor();
  // Nothing went wrong on the way in (the app reports its own errors through `ui_log`).
  expect((await calls(page)).filter((c) => c === "ui_log")).toEqual([]);
}

/**
 * The Riot client switched accounts while the app is open: the new account's live state (same stage) and, from here on,
 * every command answers for B. Set the account first, so an ask the switch triggers already gets B's data.
 */
async function switchAccount(page: Page, live: LiveState = LIVE_B): Promise<void> {
  await page.evaluate((payload) => {
    const w = window as unknown as Fake;
    w.__account = "b";
    w.__emit("live", payload);
  }, live);
}

test.describe("Riot account switch", () => {
  test("an open History page replaces account A's rows with account B's", async ({ page }) => {
    await bootInTauri(page);
    await nav(page, "History").click();
    await expect(page.locator(`main [data-match-id="${ROW_A}"]`)).toHaveCount(1);

    await switchAccount(page);

    await expect(page.locator(`main [data-match-id="${ROW_B}"]`)).toHaveCount(1);
    await expect(page.locator(`main [data-match-id="${ROW_A}"]`)).toHaveCount(0);
  });

  test("a History page issued for account A cannot land after the switch", async ({ page }) => {
    await bootInTauri(page);
    // Account A's first page never answers on its own: the switch happens while it is in flight.
    await page.evaluate(() => ((window as unknown as Fake).__hangHistory = true));
    await nav(page, "History").click();
    // The request really is held in the fake and has not answered (not just "slow to mount").
    await expect.poll(() => page.evaluate(() => (window as unknown as Fake).__hungHistory)).toBe(true);
    await expect(page.locator(`main [data-match-id="${ROW_A}"]`)).toHaveCount(0); // still loading: no rows yet

    await switchAccount(page);
    // The switch asks again, for B, and that page draws.
    await expect(page.locator(`main [data-match-id="${ROW_B}"]`)).toHaveCount(1);

    // Now A's page arrives: it was issued for the previous account and must be dropped, not drawn. Releasing resolves
    // the fake's promise; the list handles it in microtasks, and Svelte flushes before the next task, so one task plus
    // two frames puts the assertions after any draw the stale answer could have caused (no fixed-duration sleep).
    await page.evaluate(async () => {
      (window as unknown as Fake).__releaseHistory?.();
      await new Promise((resolve) => setTimeout(resolve, 0));
      await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    });
    await expect(page.locator(`main [data-match-id="${ROW_A}"]`)).toHaveCount(0);
    await expect(page.locator(`main [data-match-id="${ROW_B}"]`)).toHaveCount(1);
  });

  test("an open Skins page clears account A's collection and draws account B's", async ({ page }) => {
    await bootInTauri(page);
    await nav(page, "Skins").click();
    await expect(page.locator("main .value .num")).toHaveText("875");

    await switchAccount(page);

    await expect(page.locator("main .value .num")).toHaveText("2,550");
  });

  test("an open Saved page reloads the records for the new account", async ({ page }) => {
    await bootInTauri(page);
    await nav(page, "Saved").click();
    const row = page.locator("main [data-row]").filter({ hasText: "Nova" });
    await expect(row.locator(".rec").first().locator(".games")).toHaveText("3 games");

    await switchAccount(page);

    await expect(row.locator(".rec").first().locator(".games")).toHaveText("9 games");
  });

  test("an open profile drawer closes when the account switches", async ({ page }) => {
    await bootInTauri(page);
    await page.locator("main [data-row]").filter({ hasText: "Mara" }).first().click();
    const dialog = page.getByRole("dialog");
    await expect(dialog).toHaveAttribute("aria-busy", "false");

    await switchAccount(page);

    await expect(dialog).toHaveCount(0);
  });

  test("an open scoreboard closes on the switch, and the list under it is the new account's", async ({ page }) => {
    await bootInTauri(page);
    await nav(page, "History").click();
    await page.locator(`main [data-match-id="${ROW_A}"]`).click();
    const board = page.locator('main .match[aria-busy="false"]');
    await expect(board).toHaveCount(1);

    await switchAccount(page);

    await expect(board).toHaveCount(0);
    await expect(page.locator(`main [data-match-id="${ROW_B}"]`)).toHaveCount(1);
  });

  test("a scoreboard opened from the drawer closes on the switch and its drawer does not reopen", async ({ page }) => {
    await bootInTauri(page);
    await page.locator("main [data-row]").filter({ hasText: "Mara" }).first().click();
    const dialog = page.getByRole("dialog");
    await expect(dialog).toHaveAttribute("aria-busy", "false");
    await guardDone(page);
    // An encounter row opens the scoreboard from the drawer (Back from it would reopen the drawer).
    await dialog.getByRole("button", { name: /Open scoreboard$/ }).first().click();
    const board = page.locator('main .match[aria-busy="false"]');
    await expect(board).toHaveCount(1);
    await expect(dialog).toHaveCount(0); // opening the scoreboard already closed the drawer

    await switchAccount(page);

    await expect(board).toHaveCount(0);
    // The switch closes the scoreboard for the new account, not back through the old account's drawer.
    await expect(dialog).toHaveCount(0);
  });

  // The real-client edge: Riot switched accounts while VALORANT is closed. The live state names nobody on screen, and a
  // never-seen account has no self card and no computable My Stats, so only the live state's `selfPuuid` identifies the
  // new account. Without it the UI has no signal at all and every case below keeps account A's data.
  test.describe("while VALORANT is closed: only the live state's selfPuuid names the account", () => {
    test("History replaces account A's rows", async ({ page }) => {
      await bootInTauri(page, "closed");
      await nav(page, "History").click();
      await expect(page.locator(`main [data-match-id="${ROW_A}"]`)).toHaveCount(1);

      await switchAccount(page, CLOSED_B);

      await expect(page.locator(`main [data-match-id="${ROW_B}"]`)).toHaveCount(1);
      await expect(page.locator(`main [data-match-id="${ROW_A}"]`)).toHaveCount(0);
    });

    test("Skins clears account A's collection and draws account B's", async ({ page }) => {
      await bootInTauri(page, "closed");
      await nav(page, "Skins").click();
      await expect(page.locator("main .value .num")).toHaveText("875");

      await switchAccount(page, CLOSED_B);

      await expect(page.locator("main .value .num")).toHaveText("2,550");
    });

    test("Saved reloads the records and the open drawer closes", async ({ page }) => {
      await bootInTauri(page, "closed");
      await nav(page, "Saved").click();
      const row = page.locator("main [data-row]").filter({ hasText: "Nova" });
      await expect(row.locator(".rec").first().locator(".games")).toHaveText("3 games");
      await row.click();
      const dialog = page.getByRole("dialog");
      await expect(dialog).toHaveCount(1);
      await guardDone(page);

      await switchAccount(page, CLOSED_B);

      await expect(dialog).toHaveCount(0);
      await expect(row.locator(".rec").first().locator(".games")).toHaveText("9 games");
    });
  });
});
