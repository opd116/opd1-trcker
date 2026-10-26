// The v0.2.0 polish rules (R149) on the v2 screens: a failing `player_details` / `match_details` retries once after 1s
// and then settles on its empty state (never a skeleton forever, never status text), the page error boundary (a crashed
// screen leaves the title bar and the nav rail and recovers on the next update of its own data), the UI's own errors
// reaching `ui_log` under Tauri, and the double-click guard on the board's rows. DEV browser against the demo feed;
// `?fail=details:N` / `?fail=match:N` (demo/scenes.ts) makes the first N calls reject.
import { expect, test, type Page } from "@playwright/test";
import { DETAILS_RETRY_MS } from "../src/lib/boot";
import { PUUID } from "../src/lib/demo/cdv8";
import { gotoApp, NOW, openScene, routeMedia, sceneUrl, settle } from "./harness";

const NO_STATUS_TEXT = /error|fail|retry|try again|unavailable|offline|updated|loading/i;
const MARA = PUUID.mara!;

/** Open a scene without waiting for its data: these tests watch the loading happen. */
async function gotoScene(page: Page, scene: string, params: Record<string, string> = {}): Promise<void> {
  await routeMedia(page);
  await page.clock.setFixedTime(NOW);
  await gotoApp(page, sceneUrl(scene, params));
}

const consoleErrors = (page: Page): string[] => {
  const errors: string[] = [];
  page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
  page.on("pageerror", (e) => errors.push(String(e)));
  return errors;
};

const dialog = (page: Page) => page.getByRole("dialog");

/**
 * A live payload only the board's rendering trips over: a card whose last-5 list holds a null (the stores and their
 * subscribers never read it; the row's streak does).
 */
const BREAK_BOARD = `import("/src/lib/stores.ts").then(({ live }) => { let s; live.subscribe((v) => (s = v))(); s = structuredClone(s); s.players[1].last5 = [null]; live.set(s); })`;
const nav = (page: Page, label: string) => page.locator(`nav.rail button[aria-label="${label}"]`);

test.describe("player_details that fails", () => {
  test("once: the skeleton stays for the 1s wait, then the retry fills the drawer", async ({ page }) => {
    const errors = consoleErrors(page);
    // Timed from before the page loads: the first call fails after that, and its retry waits DETAILS_RETRY_MS.
    const t0 = Date.now();
    await gotoScene(page, "details", { fail: "details:1" });
    await expect(dialog(page)).toHaveAttribute("aria-busy", "true");
    await expect(dialog(page)).toHaveAttribute("aria-busy", "false");
    expect(Date.now() - t0, "the retry waits 1s").toBeGreaterThanOrEqual(DETAILS_RETRY_MS);
    await settle(page);
    await expect(dialog(page)).toHaveAttribute("aria-label", "Mara profile");
    // Real data, not the settled state: her encounters with the user are there.
    await expect(dialog(page).getByRole("button", { name: /Open scoreboard$/ })).toHaveCount(3);
    expect(errors.filter((e) => e.includes("player_details failed"))).toEqual([]); // the retry worked: nothing to log
  });

  test("twice: the drawer settles on its empty state (— values, no skeleton, no status text), and asks no third time", async ({ page }) => {
    const errors = consoleErrors(page);
    await gotoScene(page, "details", { fail: "details:2" });
    await expect(dialog(page)).toHaveAttribute("aria-busy", "true");
    await expect(dialog(page)).toHaveAttribute("aria-busy", "false");
    await settle(page);
    // Not a skeleton any more: no grey shape anywhere in the drawer.
    await expect(dialog(page).locator(".sk")).toHaveCount(0);
    // The header keeps what the live card knows; what only the details carry is —.
    await expect(dialog(page)).toHaveAttribute("aria-label", "Mara profile");
    await expect(dialog(page).getByText("—").first()).toBeVisible();
    await expect(dialog(page).getByRole("button", { name: /Open scoreboard$/ })).toHaveCount(0);
    expect(await dialog(page).innerText()).not.toMatch(NO_STATUS_TEXT);
    // It logged once; a third attempt would log again (each failed attempt that is not retried is reported).
    await expect.poll(() => errors.filter((e) => e.includes("player_details failed")).length).toBe(1);
    await page.waitForTimeout(2500); // past another retry period: still one
    expect(errors.filter((e) => e.includes("player_details failed"))).toHaveLength(1);
    await expect(dialog(page)).toHaveAttribute("aria-busy", "false");
  });

  test("the settled drawer still closes", async ({ page }) => {
    await gotoScene(page, "details", { fail: "details:2" });
    await expect(dialog(page)).toHaveAttribute("aria-busy", "false");
    await dialog(page).getByRole("button", { name: "Close profile" }).click();
    await expect(dialog(page)).toHaveCount(0);
    await expect(nav(page, "Live")).toHaveAttribute("aria-current", "page");
  });
});

test.describe("match_details that fails", () => {
  const board = (page: Page, busy: boolean) => page.locator(`main .match[aria-busy="${busy}"]`);

  test("once: the retry after 1s fills the scoreboard", async ({ page }) => {
    await gotoScene(page, "match", { fail: "match:1" });
    await expect(board(page, true)).toHaveCount(1);
    await expect(board(page, false)).toHaveCount(1);
    await settle(page);
    await expect(page.locator("main .match .srow")).toHaveCount(10);
  });

  test("twice: the settled empty state (— header, no rows, no status text, not busy); Back still works", async ({ page }) => {
    const errors = consoleErrors(page);
    await gotoScene(page, "match", { fail: "match:2" });
    await expect(board(page, true)).toHaveCount(1);
    await expect(board(page, false)).toHaveCount(1);
    await settle(page);
    await expect(page.locator("main .match .sk")).toHaveCount(0);
    await expect(page.locator("main .match .srow")).toHaveCount(0);
    await expect(page.locator("main .match .hmap")).toHaveText("—");
    await expect(page.locator("main .match .hmeta")).toHaveText("—");
    expect(await page.locator("main .match").innerText()).not.toMatch(NO_STATUS_TEXT);
    await expect.poll(() => errors.filter((e) => e.includes("match_details failed")).length).toBe(1);
    await page.waitForTimeout(2500);
    expect(errors.filter((e) => e.includes("match_details failed"))).toHaveLength(1);
    // Back from it reopens the profile it came from.
    await page.locator("main .match .back").click();
    await expect(dialog(page)).toHaveAttribute("aria-label", "Mara profile");
  });
});

// A crash while rendering a screen leaves the title bar, the nav rail and a quiet empty page; the next live update
// tries again.
test("a screen that throws while rendering shows a quiet empty page (shell stays), and the next live update recovers it", async ({ page }) => {
  const errors = consoleErrors(page);
  await openScene(page, "ingame");
  await expect(page.locator("main [data-row]")).toHaveCount(10);
  await page.evaluate(BREAK_BOARD);
  await expect(page.locator("main .live")).toHaveCount(0);
  await expect(page.locator("main .blank")).toHaveCount(1);
  expect(await page.locator("main .blank").innerText()).toBe("");
  await expect(page.locator("#app > header")).toBeVisible();
  await expect(nav(page, "Live")).toHaveAttribute("aria-current", "page");
  expect(errors.some((e) => e.startsWith("page "))).toBe(true);
  // The next good live state brings the page back.
  await page.evaluate(`import("/src/lib/stores.ts").then(({ live }) => live.set({ stage: "closed", mapId: null, players: [] }))`);
  await expect(page.locator("main .blank")).toHaveCount(0);
  await expect(page.locator("main .live")).toHaveCount(1);
});

// A crash in the drawer keeps the screen under it; once good details are back, the next live update redraws it.
test("a drawer that throws while rendering leaves the screen, and recovers on the next live update", async ({ page }) => {
  await openScene(page, "ingame", { profile: MARA });
  await expect(dialog(page)).toBeVisible();
  await page.evaluate(
    `import("/src/lib/stores.ts").then(({ openDetails }) => { let d; openDetails.subscribe((v) => (d = v))(); window.__good = structuredClone(d); openDetails.event({ ...d, recent: [null] }); })`,
  );
  await expect(dialog(page)).toHaveCount(0);
  await expect(page.locator("main [data-row]")).toHaveCount(10);
  await expect(page.locator("#app > header")).toBeVisible();
  await page.evaluate(`import("/src/lib/stores.ts").then(({ openDetails }) => openDetails.event(window.__good))`);
  await expect(dialog(page)).toHaveCount(0); // the boundary retries on a live update only
  await page.evaluate(`import("/src/lib/stores.ts").then(({ live, applyLive }) => { let s; live.subscribe((v) => (s = v))(); applyLive(structuredClone(s)); })`);
  await expect(dialog(page)).toBeVisible();
  await expect(dialog(page)).toHaveAttribute("aria-label", "Mara profile");
});

// The crashed drawer used to stay a flex child of <main> (half its height) and kept the next drawer from opening.
test("a crashed drawer takes no room from the screen, and opening another player tries again", async ({ page }) => {
  await openScene(page, "ingame", { profile: MARA });
  await expect(dialog(page)).toBeVisible();
  await page.evaluate(
    `import("/src/lib/stores.ts").then(({ openDetails }) => { let d; openDetails.subscribe((v) => (d = v))(); openDetails.event({ ...d, recent: [null] }); })`,
  );
  await expect(dialog(page)).toHaveCount(0);
  const h = await page.evaluate(() => ({
    main: document.querySelector("main")!.clientHeight,
    live: document.querySelector("main .live")!.getBoundingClientRect().height,
  }));
  expect(h.live).toBe(h.main);
  // No live update in between: the other player's row alone brings a drawer back.
  await page.getByRole("button").filter({ hasText: "Sable" }).first().click();
  await expect(dialog(page)).toHaveAttribute("aria-label", "Sable profile");
});

// Glance mode has no other way out than its own title bar (no native frame, always on top).
test("a glance body that throws keeps the glance title bar, and the next live update recovers it", async ({ page }) => {
  const errors = consoleErrors(page);
  await page.setViewportSize({ width: 420, height: 800 });
  await openScene(page, "ingame", { glance: "1" });
  await expect(page.locator(".glance-body [data-row]")).toHaveCount(10);
  await page.evaluate(BREAK_BOARD);
  await expect(page.locator(".glance-body")).toHaveCount(0);
  expect(errors.some((e) => e.startsWith("glance "))).toBe(true);
  await expect(page.locator(".glance .bar")).toBeVisible();
  await expect(page.getByRole("button", { name: "Exit glance" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Close", exact: true })).toBeVisible();
  await page.evaluate(`import("/src/lib/stores.ts").then(({ live }) => live.set({ stage: "closed", mapId: null, players: [] }))`);
  await expect(page.locator(".glance-body")).toHaveCount(1);
});

// Under Tauri the UI's own failures reach the app log (`ui_log`, Ctrl+Shift+L / Ctrl+Shift+D). The DEV browser has no
// Rust, so the Tauri side is faked after the page loaded: `isTauri()` reads `globalThis.isTauri` when it is called, and
// `invoke` is `window.__TAURI_INTERNALS__.invoke(cmd, args, options)` (@tauri-apps/api/core).
test("under Tauri a crashed page, an uncaught error and an unhandled rejection each reach ui_log", async ({ page }) => {
  await openScene(page, "ingame");
  await expect(page.locator("main [data-row]")).toHaveCount(10);
  await page.evaluate(() => {
    const w = window as unknown as {
      isTauri?: boolean;
      __calls?: Array<[string, Record<string, unknown>]>;
      __TAURI_INTERNALS__?: { invoke: (cmd: string, args: Record<string, unknown>, options?: unknown) => Promise<void> };
    };
    w.isTauri = true;
    w.__calls = [];
    w.__TAURI_INTERNALS__ = { invoke: (cmd, args) => (w.__calls!.push([cmd, args]), Promise.resolve()) };
  });
  const logged = () =>
    page.evaluate(() =>
      (window as unknown as { __calls: Array<[string, { level: string; message: string }]> }).__calls
        .filter(([cmd]) => cmd === "ui_log")
        .map(([, args]) => args),
    );

  // A page that crashes while rendering: the boundary's `page` line, with the error's name.
  await page.evaluate(BREAK_BOARD);
  await expect(page.locator("main .blank")).toHaveCount(1);
  await expect.poll(async () => (await logged()).map((a) => a.message)).toContainEqual(expect.stringMatching(/^page: TypeError/));

  // An uncaught error (the `window` listener) and a promise rejected with no handler (`promise`).
  await page.evaluate(() => {
    window.dispatchEvent(new ErrorEvent("error", { error: new Error("x"), message: "x" }));
  });
  await page.evaluate(() => {
    void Promise.reject(new Error("y"));
  });
  await expect.poll(async () => (await logged()).map((a) => a.message)).toContainEqual(expect.stringMatching(/^window: Error: x/));
  await expect.poll(async () => (await logged()).map((a) => a.message)).toContainEqual(expect.stringMatching(/^promise: Error: y/));

  const all = await logged();
  expect(all.every((a) => a.level === "error")).toBe(true);
  expect(all.every((a) => a.message.length <= 500 && !/\n/.test(a.message))).toBe(true);
  // Each line once, however often the page re-rendered meanwhile (the dedupe).
  expect(new Set(all.map((a) => a.message)).size).toBe(all.length);
});

// Competitive shows `mystats` data, not live data: only a My Stats update retries it (a live update says nothing new).
test("a crashed Competitive screen retries only on a My Stats update, not on every live update", async ({ page }) => {
  const errors = consoleErrors(page);
  await openScene(page, "mystats");
  await expect(page.locator("main .mystats")).toHaveCount(1);
  // A payload the RR chart cannot render: a null where a point must be.
  await page.evaluate(
    `import("/src/lib/stores.ts").then(({ mystats }) => { let s; mystats.subscribe((v) => (s = v))(); mystats.set({ ...s, rrSeries: [null, null, null] }); })`,
  );
  await expect(page.locator("main .mystats")).toHaveCount(0);
  await expect(page.locator("main .blank")).toHaveCount(1);
  await expect(page.locator("#app > header")).toBeVisible();
  expect(errors.filter((e) => e.startsWith("page "))).toHaveLength(1);
  // Live updates come all the time: none of them re-runs the crash.
  for (let i = 0; i < 3; i++) {
    await page.evaluate(`import("/src/lib/stores.ts").then(({ applyLive, live }) => { let s; live.subscribe((v) => (s = v))(); applyLive({ ...s }); })`);
  }
  await expect(page.locator("main .blank")).toHaveCount(1);
  expect(errors.filter((e) => e.startsWith("page "))).toHaveLength(1);
  // The next good My Stats answer brings the page back.
  await page.evaluate(
    `import("/src/lib/stores.ts").then(({ mystats }) => import("/src/lib/demo/mystats.ts").then(({ myStatsFull }) => mystats.set(myStatsFull())))`,
  );
  await expect(page.locator("main .blank")).toHaveCount(0);
  await expect(page.locator("main .mystats")).toHaveCount(1);
});

test("an unknown stage renders the waiting view instead of crashing", async ({ page }) => {
  await openScene(page, "ingame");
  await page.evaluate(`import("/src/lib/stores.ts").then(({ live }) => live.set({ stage: "champselect", mapId: null, players: [] }))`);
  await expect(page.locator("main .live")).toHaveCount(1);
  await expect(page.locator("main [data-row]")).toHaveCount(0);
  await expect(page.locator("main .blank")).toHaveCount(0);
});

// A row click opens the drawer; the second click of a double-click lands on the scrim (the drawer covers <main> but
// for its left strip), which would close it again. App swallows clicks for 400 ms after a row opens the drawer.
test.describe("double-click guard", () => {
  /** A spot on the user's row that the drawer's scrim covers once it is open. */
  async function scrimSpot(page: Page): Promise<{ x: number; y: number }> {
    const row = (await page.locator("main [data-row]").first().boundingBox())!;
    return { x: row.x + 60, y: row.y + row.height / 2 };
  }
  const guard = (page: Page) => page.evaluate(`import("/src/lib/clickGuard.ts").then(({ cardOpenGuard }) => cardOpenGuard.swallows())`);

  test("a double-click on a row opens the drawer once and the second click does not close it", async ({ page }) => {
    await openScene(page, "ingame");
    const spot = await scrimSpot(page);
    await page.mouse.dblclick(spot.x, spot.y);
    await expect(dialog(page)).toBeVisible();
    // The spot is on the scrim now (a click there would close the drawer).
    expect(await page.evaluate(({ x, y }) => document.elementFromPoint(x, y)?.className ?? "", spot)).toMatch(/\bscrim\b/);
    await expect.poll(() => guard(page)).toBe(false);
    await expect(dialog(page)).toBeVisible();
  });

  test("once the guard lets go, a click on the scrim closes the drawer", async ({ page }) => {
    await openScene(page, "ingame");
    const spot = await scrimSpot(page);
    await page.mouse.click(spot.x, spot.y);
    await expect(dialog(page)).toBeVisible();
    await expect.poll(() => guard(page)).toBe(false);
    await page.mouse.click(spot.x, spot.y);
    await expect(dialog(page)).toHaveCount(0);
  });

  test("a double-click on a Glance row opens the drawer once: the second click neither closes it nor hits the rail", async ({ page }) => {
    await page.setViewportSize({ width: 420, height: 800 });
    await openScene(page, "ingame", { glance: "1" });
    const row = (await page.locator('.glance-body [data-row][role="button"]').first().boundingBox())!;
    await page.mouse.dblclick(row.x + 150, row.y + row.height / 2);
    await expect(dialog(page)).toBeVisible();
    await expect.poll(() => guard(page)).toBe(false);
    await expect(dialog(page)).toBeVisible();
  });

  test("the guard does not delay the first click on a row", async ({ page }) => {
    await openScene(page, "ingame");
    await page.locator("main [data-row]").nth(3).click();
    await expect(dialog(page)).toBeVisible();
  });
});
