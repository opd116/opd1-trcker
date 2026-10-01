// R150: the Match Details page tells Rust when it is left (`close_match`, like `close_details`). The DEV browser has no
// Rust, so this spec puts a recording fake behind Tauri's own door (`window.isTauri`, `__TAURI_INTERNALS__.invoke`): the
// app takes its real Tauri paths (api.ts's `invoke`, startFeeds' `listen`) and every command it sends is logged, in
// order, for the assertions. 1000×800, DPR 1, against the same demo data as the other specs.
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { expect, test, type Page } from "@playwright/test";
import { matchFor, PUUID, recentMatchId } from "../src/lib/demo/cdv8";
import { SCENES } from "../src/lib/demo/scenes";
import { NOW, routeMedia, UI_DIR, waitPage } from "./harness";

const MARA = PUUID.mara!;
const A = recentMatchId("mara", 0);
const B = recentMatchId("mara", 1);

/** The commands the app sent since `mark`, in order (`match_details` carries the id: `match_details:<id>`). */
const calls = (page: Page): Promise<string[]> => page.evaluate(() => (window as unknown as { __calls: string[] }).__calls.slice());
const mark = (page: Page) => page.evaluate(() => ((window as unknown as { __calls: string[] }).__calls.length = 0));

/** Boot the app as if inside Tauri: the fake answers meta, live_state, player_details and match_details, and lets a test emit events. */
async function bootInTauri(page: Page): Promise<void> {
  const data = {
    meta: JSON.parse(readFileSync(join(UI_DIR, "src", "lib", "demo", "meta.json"), "utf8")),
    live: SCENES.ingame.live,
    details: SCENES.details.details,
    matches: { [A]: matchFor(A), [B]: matchFor(B) },
  };
  await page.addInitScript((d) => {
    type Fake = Record<string, unknown> & { __calls: string[]; __closes: string[]; __emit: (event: string, payload: unknown) => void };
    const w = window as unknown as Fake;
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
      invoke: async (cmd: string, args: Record<string, string>) => {
        if (cmd === "plugin:event|listen") {
          listeners.set(args.event!, Number(args.handler));
          return ++n;
        }
        w.__calls.push(cmd === "match_details" ? `match_details:${args.matchId}` : cmd);
        if (cmd.startsWith("close_")) w.__closes.push(`${cmd}:${JSON.stringify(args ?? null)}`);
        const table: Record<string, unknown> = {
          meta: d.meta,
          live_state: d.live,
          player_details: (d.details as Record<string, unknown>)[args.puuid!],
          match_details: (d.matches as Record<string, unknown>)[args.matchId!],
        };
        return structuredClone(table[cmd] ?? null);
      },
    };
  }, data);
  await routeMedia(page);
  await page.clock.setFixedTime(NOW);
  await page.goto("/");
  await page.locator(".live .card").first().waitFor();
}

/** Mara's Details, then her first recent row: the Match page for A, loaded. */
async function openMatchA(page: Page): Promise<void> {
  await page.locator(".live .card").filter({ hasText: "Mara" }).click({ position: { x: 20, y: 70 } });
  await waitPage(page, "loaded");
  await page.waitForTimeout(450); // past the double-click guard
  await page.locator(".details .recent .rrow").first().click();
  await expect(page.locator(".match")).toHaveCount(1);
  await waitPage(page, "loaded");
  expect(await calls(page)).toContain(`match_details:${A}`);
}

/** The `close_*` commands with their arguments, as sent (R163: each carries the key of the page it closes). */
const closes = (page: Page): Promise<string[]> => page.evaluate(() => (window as unknown as { __closes: string[] }).__closes.slice());

const count = (log: string[], cmd: string) => log.filter((c) => c === cmd).length;

test.describe("close_match", () => {
  test("Details opening the match closes the details, not the match; the Match page's own open comes after", async ({ page }) => {
    await bootInTauri(page);
    await openMatchA(page);
    const log = await calls(page);
    expect(log.indexOf("close_details")).toBeGreaterThan(-1);
    expect(log.indexOf("close_details")).toBeLessThan(log.indexOf(`match_details:${A}`));
    expect(count(log, "close_match")).toBe(0);
  });

  test("Back from Match: close_match once, before the Details page opens again", async ({ page }) => {
    await bootInTauri(page);
    await openMatchA(page);
    await mark(page);
    await page.locator(".match .back").click();
    await waitPage(page, "loaded");
    await page.waitForTimeout(300);
    const log = await calls(page);
    expect(count(log, "close_match")).toBe(1);
    expect(log.indexOf("close_match")).toBeLessThan(log.indexOf("player_details"));
    expect(count(log, "close_details")).toBe(0);
  });

  test("R163: the closes carry the key of the page they close", async ({ page }) => {
    await bootInTauri(page);
    await openMatchA(page);
    // Details → Match closed the details page: its puuid.
    expect(await closes(page)).toEqual([`close_details:${JSON.stringify({ puuid: MARA })}`]);
    await page.locator(".match .back").click();
    await waitPage(page, "loaded");
    // Back from Match closed the match: its id.
    expect((await closes(page)).at(-1)).toBe(`close_match:${JSON.stringify({ matchId: A })}`);
  });

  test("the Live tab from Match: close_match once", async ({ page }) => {
    await bootInTauri(page);
    await openMatchA(page);
    await mark(page);
    await page.locator("header.bar .tab.on").click();
    await expect(page.locator(".live")).toHaveCount(1);
    await page.waitForTimeout(300);
    expect(await calls(page)).toEqual(["close_match"]);
  });

  test("Agent Select's switch to Live from Match: close_match once", async ({ page }) => {
    await bootInTauri(page);
    await openMatchA(page);
    await mark(page);
    await page.evaluate((live) => (window as unknown as { __emit: (e: string, p: unknown) => void }).__emit("live", live), SCENES.pregame.live);
    await expect(page.locator(".live")).toHaveCount(1);
    await expect(page.locator(".match")).toHaveCount(0);
    await page.waitForTimeout(300);
    expect(await calls(page)).toEqual(["close_match"]);
  });

  test("a match-to-match switch ends with the new match open: the old page's close_match goes out before the new match_details", async ({ page }) => {
    await bootInTauri(page);
    await openMatchA(page);
    const first = await page.locator(".match .mtitle").innerText();
    await mark(page);
    await page.evaluate(
      `import("/src/lib/router.svelte.ts").then(({ go }) => go(${JSON.stringify({ name: "match", matchId: B, focusPuuid: MARA, from: "recent", back: { puuid: MARA, more: false } })}))`,
    );
    await waitPage(page, "loaded");
    await expect(page.locator(".match .mtitle")).not.toHaveText(first);
    await page.waitForTimeout(400);
    // Exactly the two commands, in this order, and nothing after them: the new match is not closed again.
    expect(await calls(page)).toEqual(["close_match", `match_details:${B}`]);
    const open = await page.evaluate(
      `import("/src/lib/stores.ts").then((s) => { let v; s.openMatch.subscribe((x) => (v = x))(); return v ? v.matchId : null; })`,
    );
    expect(open).toBe(B);
    await expect(page.locator(".match .srow")).toHaveCount(10);
  });
});
