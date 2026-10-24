// R145 / R150 / R163 on the v2 router: leaving the profile drawer sends `close_details(puuid)` and leaving a scoreboard
// sends `close_match(matchId)`, each once, before the next page's own open, so Rust never rebuilds or emits a page
// nobody sees. The DEV browser has no Rust, so this spec puts a recording fake behind Tauri's own door
// (`window.isTauri`, `__TAURI_INTERNALS__.invoke`): the app takes its real Tauri paths (api.ts's `invoke`, the feeds'
// `listen`) and every command it sends is logged, in order, for the assertions.
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { expect, test, type Page } from "@playwright/test";
import { detailsFor, encounterMatchId, matchFor, PUUID, recentMatchId } from "../src/lib/demo/cdv8";
import { SCENES } from "../src/lib/demo/scenes";
import { DEFAULT_SETTINGS } from "../src/lib/demo/v2/store";
import { demoStatus } from "../src/lib/demo/v2/status";
import { APP_READY, gotoApp, NOW, routeMedia, UI_DIR } from "./harness";

const MARA = PUUID.mara!;
const FAHAD = PUUID.fahad!;
/** Mara's first encounter with the user, and the user's own two newest matches. */
const ENC = encounterMatchId("mara", 0);
const A = recentMatchId("fahad", 0);
const B = recentMatchId("fahad", 1);

type Fake = { __calls: string[]; __closes: string[]; __emit: (event: string, payload: unknown) => void };

/** The commands the app sent since `mark`, in order (`match_details` / `player_details` carry their key). */
const calls = (page: Page): Promise<string[]> => page.evaluate(() => (window as unknown as Fake).__calls.slice());
const mark = (page: Page) => page.evaluate(() => ((window as unknown as Fake).__calls.length = 0));
/** The `close_*` commands with their arguments, as sent (R163: each carries the key of the page it closes). */
const closes = (page: Page): Promise<string[]> => page.evaluate(() => (window as unknown as Fake).__closes.slice());
const count = (log: string[], cmd: string) => log.filter((c) => c === cmd).length;
/** The page-level commands only (the drawer's `profile_extras` and the like are not what this spec is about). */
const pageCalls = async (page: Page) => (await calls(page)).filter((c) => /^(close_|player_details|match_details)/.test(c));

/** Boot the app as if inside Tauri: the fake answers what the screens ask for and lets a test emit events. */
async function bootInTauri(page: Page): Promise<void> {
  const live = SCENES.ingame.live!;
  const data = {
    meta: JSON.parse(readFileSync(join(UI_DIR, "src", "lib", "demo", "meta.json"), "utf8")),
    live,
    status: demoStatus(live, new URLSearchParams()),
    settings: DEFAULT_SETTINGS,
    details: {
      [MARA]: detailsFor("mara", live.players.find((c) => c.puuid === MARA), live.players),
      [FAHAD]: detailsFor("fahad", live.players.find((c) => c.puuid === FAHAD), live.players),
    },
    matches: { [ENC]: matchFor(ENC, live.players), [A]: matchFor(A, live.players), [B]: matchFor(B, live.players) },
  };
  await page.addInitScript((d) => {
    const w = window as unknown as Record<string, unknown> & Fake;
    w.isTauri = true;
    w.__calls = [];
    w.__closes = [];
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
      invoke: async (cmd: string, args: Record<string, string> | undefined) => {
        if (cmd === "plugin:event|listen") {
          listeners.set(args!.event!, Number(args!.handler));
          return ++n;
        }
        if (cmd.startsWith("plugin:")) return null;
        w.__calls.push(cmd === "match_details" ? `match_details:${args!.matchId}` : cmd === "player_details" ? `player_details:${args!.puuid}` : cmd);
        if (cmd.startsWith("close_")) w.__closes.push(`${cmd}:${JSON.stringify(args ?? null)}`);
        const table: Record<string, unknown> = {
          meta: d.meta,
          live_state: d.live,
          app_status: d.status,
          get_settings: d.settings,
          player_marks: { saved: [], notes: {}, notSmurf: [] },
          profile_extras: { puuid: args?.puuid, maps: [], liveMates: [] },
          player_details: (d.details as Record<string, unknown>)[args?.puuid ?? ""],
          match_details: (d.matches as Record<string, unknown>)[args?.matchId ?? ""],
        };
        return structuredClone(table[cmd] ?? null);
      },
    };
  }, data);
  await routeMedia(page);
  await page.clock.setFixedTime(NOW);
  await gotoApp(page, "/", APP_READY);
  await page.locator("main [data-row]").first().waitFor();
  // Nothing went wrong on the way in (the app reports its own errors through `ui_log`).
  expect((await calls(page)).filter((c) => c === "ui_log")).toEqual([]);
}

const dialog = (page: Page) => page.getByRole("dialog");
const board = (page: Page) => page.locator('main .match[aria-busy="false"]');
const nav = (page: Page, label: string) => page.locator(`nav.rail button[aria-label="${label}"]`);

/** The app's own double-click guard (lib/clickGuard.ts): a click within 400 ms of opening a drawer is swallowed. */
async function guardDone(page: Page): Promise<void> {
  await expect
    .poll(() => page.evaluate(`import("/src/lib/clickGuard.ts").then(({ cardOpenGuard }) => cardOpenGuard.swallows())`))
    .toBe(false);
}

/** Open a player's drawer from the board and wait for their details (and for the double-click guard to let go). */
async function openDrawer(page: Page, name: string): Promise<void> {
  await page.locator("main [data-row]").filter({ hasText: name }).first().click();
  await expect(dialog(page)).toHaveAttribute("aria-busy", "false");
  await guardDone(page);
}

/** Mara's drawer, then her first encounter row: the scoreboard for ENC, loaded. */
async function openEncounter(page: Page): Promise<void> {
  await openDrawer(page, "Mara");
  await dialog(page).getByRole("button", { name: /Open scoreboard$/ }).first().click();
  await expect(board(page)).toHaveCount(1);
  expect(await calls(page)).toContain(`match_details:${ENC}`);
}

test.describe("close_details / close_match", () => {
  test("a drawer row opening a scoreboard closes the details (with the puuid), not a match; the scoreboard's own open comes after", async ({ page }) => {
    await bootInTauri(page);
    await openEncounter(page);
    const log = await calls(page);
    expect(log.indexOf("close_details")).toBeGreaterThan(-1);
    expect(log.indexOf("close_details")).toBeLessThan(log.indexOf(`match_details:${ENC}`));
    expect(count(log, "close_match")).toBe(0);
    expect(await closes(page)).toEqual([`close_details:${JSON.stringify({ puuid: MARA })}`]);
  });

  test("Back from the scoreboard: close_match(matchId) once, before the drawer's player_details; no close_details", async ({ page }) => {
    await bootInTauri(page);
    await openEncounter(page);
    await mark(page);
    await page.locator("main .match .back").click();
    await expect(dialog(page)).toHaveAttribute("aria-busy", "false");
    const log = await pageCalls(page);
    expect(log).toEqual(["close_match", `player_details:${MARA}`]);
    expect((await closes(page)).at(-1)).toBe(`close_match:${JSON.stringify({ matchId: ENC })}`);
  });

  test("the user's own recent row: the same order (close_details, then match_details)", async ({ page }) => {
    await bootInTauri(page);
    await openDrawer(page, "Fahad");
    await mark(page);
    await dialog(page).getByRole("button", { name: /Open scoreboard$/ }).first().click();
    await expect(board(page)).toHaveCount(1);
    expect(await pageCalls(page)).toEqual(["close_details", `match_details:${A}`]);
  });

  test("the nav rail sends the keys too: close_match(matchId) from a scoreboard, close_details(puuid) from the drawer", async ({ page }) => {
    await bootInTauri(page);
    await openEncounter(page);
    await mark(page);
    await nav(page, "Live").click();
    await expect(page.locator("main .match")).toHaveCount(0);
    await expect.poll(() => pageCalls(page)).toEqual(["close_match"]);
    expect((await closes(page)).at(-1)).toBe(`close_match:${JSON.stringify({ matchId: ENC })}`);

    // And from the drawer straight to another screen.
    await openDrawer(page, "Mara");
    await mark(page);
    await nav(page, "Saved").click();
    await expect(dialog(page)).toHaveCount(0);
    await expect.poll(() => pageCalls(page)).toEqual(["close_details"]);
    expect((await closes(page)).at(-1)).toBe(`close_details:${JSON.stringify({ puuid: MARA })}`);
  });

  test("Escape and × on the drawer close the details once", async ({ page }) => {
    await bootInTauri(page);
    await openDrawer(page, "Mara");
    await mark(page);
    await page.keyboard.press("Escape");
    await expect(dialog(page)).toHaveCount(0);
    await expect.poll(() => pageCalls(page)).toEqual(["close_details"]);
    await openDrawer(page, "Mara");
    await mark(page);
    await dialog(page).getByRole("button", { name: "Close profile" }).click();
    await expect(dialog(page)).toHaveCount(0);
    await expect.poll(() => pageCalls(page)).toEqual(["close_details"]);
  });

  test("Agent Select's switch to Live from a scoreboard: close_match once", async ({ page }) => {
    await bootInTauri(page);
    await openEncounter(page);
    await mark(page);
    await page.evaluate((live) => (window as unknown as Fake).__emit("live", live), SCENES.pregame.live);
    await expect(nav(page, "Live")).toHaveAttribute("aria-current", "page");
    await expect(page.locator("main .match")).toHaveCount(0);
    await expect.poll(() => pageCalls(page)).toEqual(["close_match"]);
  });

  test("a match-to-match switch ends with the new match open: the old one's close_match goes out before the new match_details", async ({ page }) => {
    await bootInTauri(page);
    await openDrawer(page, "Fahad");
    await dialog(page).getByRole("button", { name: /Open scoreboard$/ }).first().click();
    await expect(board(page)).toHaveCount(1);
    await mark(page);
    await page.evaluate(
      `import("/src/lib/router.svelte.ts").then(({ openMatch }) => openMatch(${JSON.stringify(B)}, { from: "history", focusPuuid: ${JSON.stringify(FAHAD)} }))`,
    );
    await expect(board(page)).toHaveCount(1);
    await expect.poll(() => pageCalls(page)).toEqual(["close_match", `match_details:${B}`]);
    expect((await closes(page)).at(-1)).toBe(`close_match:${JSON.stringify({ matchId: A })}`);
    const open = await page.evaluate(
      `import("/src/lib/stores.ts").then((s) => { let v; s.openMatch.subscribe((x) => (v = x))(); return v ? v.matchId : null; })`,
    );
    expect(open).toBe(B);
    await expect(page.locator("main .match .srow")).toHaveCount(10);
  });
});
