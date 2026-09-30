// v0.2.0 polish (R149): the retry-then-settle of a failing player_details / match_details, the page boundary and the
// double-click guard. DEV browser against the demo feed; `?fail=details:N` / `?fail=match:N` (demo/scenes.ts) makes the
// first N calls reject.
import { expect, test, type Page } from "@playwright/test";
import { openScene, routeMedia, sceneUrl, settle, NOW } from "./harness";

const NO_STATUS_TEXT = /error|fail|retry|try again|unavailable|offline|saved|updated|loading/i;

/** The page's text, as a user would read it (Back, hero, section headers, values). */
const pageText = (page: Page, sel: string) => page.locator(sel).innerText();

async function gotoScene(page: Page, scene: string, params: Record<string, string> = {}): Promise<void> {
  await routeMedia(page);
  await page.clock.setFixedTime(NOW);
  await page.goto(sceneUrl(scene, params));
  await page.waitForFunction(() => document.documentElement.dataset.feed === "demo-feed");
}

const busy = (page: Page, sel: string, value: boolean) => page.locator(`${sel}[aria-busy="${value}"]`);

test.describe("player_details that fails", () => {
  test("once: the skeleton stays for the 1s wait, then the retry fills the page", async ({ page }) => {
    const errors: string[] = [];
    page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
    await gotoScene(page, "details", { fail: "details:1" });
    await expect(busy(page, ".details", true)).toHaveCount(1);
    const t0 = Date.now();
    await busy(page, ".details", false).waitFor();
    expect(Date.now() - t0, "the retry waits about 1s").toBeGreaterThanOrEqual(800);
    await settle(page);
    await expect(page.locator(".details .hero .nm")).toHaveText(/Mara\s*#7788/);
    await expect(page.locator(".details .recent .rrow")).toHaveCount(3); // real data, not the settled state
    expect(errors.filter((e) => e.includes("player_details failed"))).toEqual([]); // the retry worked: nothing to log
  });

  test("twice: the skeleton ends on the settled empty state (— values, no text, not busy), and no third call", async ({ page }) => {
    const errors: string[] = [];
    page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
    await gotoScene(page, "details", { fail: "details:2" });
    await expect(busy(page, ".details", true)).toHaveCount(1);
    await busy(page, ".details", false).waitFor();
    await settle(page);

    // Not a skeleton any more: no grey shape anywhere on the page.
    await expect(page.locator(".details .sk")).toHaveCount(0);
    await expect(page.locator(".details .hsk")).toHaveCount(0);
    // The hero keeps what the live card knew (name, ranks, the act's Win% / Games / W / L); what only the details
    // carry (K/D and HS% over the last matches) is —.
    await expect(page.locator(".details .hero .nm")).toHaveText(/Mara\s*#7788/);
    const stats = await page.locator(".details .hero .hstat").allInnerTexts();
    expect(stats.map((s) => s.replace(/\s+/g, " ").trim())).toEqual(["K/D —", "HS% —", "Win% 63%", "Games 65", "W / L 41 / 24"]);
    // Recent: its header and the one — row; no Show more; no Encounters; Top 3 and Loadout: —.
    await expect(page.locator(".details .recent .rrow")).toHaveCount(1);
    await expect(page.locator(".details .recent .rrow")).toHaveText("—");
    await expect(page.locator(".details .more")).toHaveCount(0);
    await expect(page.locator(".details .encs")).toHaveCount(0);
    await expect(page.locator(".details .loadout")).toContainText("—");
    // No status text of any kind.
    expect(await pageText(page, ".details")).not.toMatch(NO_STATUS_TEXT);

    // It logs once and never calls again.
    await page.waitForTimeout(2500);
    expect(errors.filter((e) => e.includes("player_details failed"))).toHaveLength(1);
    await expect(busy(page, ".details", false)).toHaveCount(1);
  });

  test("Back still works from the settled page", async ({ page }) => {
    await gotoScene(page, "details", { fail: "details:2" });
    await busy(page, ".details", false).waitFor();
    await page.locator(".details .back").click();
    await expect(page.locator(".live")).toHaveCount(1);
  });
});

test.describe("match_details that fails", () => {
  test("once: the retry after 1s fills the page", async ({ page }) => {
    await gotoScene(page, "match", { fail: "match:1" });
    await expect(busy(page, ".match", true)).toHaveCount(1);
    await busy(page, ".match", false).waitFor();
    await settle(page);
    await expect(page.locator(".match .srow")).toHaveCount(10);
  });

  test("twice: the settled empty state (— header, only the column row, no text, not busy)", async ({ page }) => {
    const errors: string[] = [];
    page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
    await gotoScene(page, "match", { fail: "match:2" });
    await expect(busy(page, ".match", true)).toHaveCount(1);
    await busy(page, ".match", false).waitFor();
    await settle(page);

    await expect(page.locator(".match .sk")).toHaveCount(0);
    await expect(page.locator(".match .mtitle")).toHaveText("—");
    await expect(page.locator(".match .mwhen")).toHaveText("—");
    await expect(page.locator(".match .mscore")).toHaveText("—");
    await expect(page.locator(".match .srow")).toHaveCount(0);
    await expect(page.locator(".match .shead")).toHaveCount(1); // the board keeps its column header row
    await expect(page.locator(".match .back")).toHaveCount(1);
    expect(await pageText(page, ".match")).not.toMatch(NO_STATUS_TEXT);
    await page.waitForTimeout(2500);
    expect(errors.filter((e) => e.includes("match_details failed"))).toHaveLength(1);
  });
});

// A crash while rendering a page leaves the top bar and a quiet empty page; the next live update tries again.
test("a page that throws while rendering shows a quiet empty page (top bar stays), and the next live update recovers it", async ({ page }) => {
  const errors: string[] = [];
  page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
  page.on("pageerror", (e) => errors.push(String(e)));
  await openScene(page, "ingame");
  await expect(page.locator(".live .card")).toHaveCount(10);
  // A payload the Live grid cannot render: a null where a card must be.
  await page.evaluate(`import("/src/lib/stores.ts").then(({ live }) => live.set({ stage: "ingame", mapId: null, players: [null] }))`);
  await expect(page.locator(".live")).toHaveCount(0);
  await expect(page.locator(".blank")).toHaveCount(1);
  expect(await page.locator(".blank").innerText()).toBe("");
  await expect(page.locator("header.bar")).toBeVisible();
  await expect(page.locator(".tab.on")).toHaveText("Live");
  expect(errors.some((e) => e.includes("page crashed"))).toBe(true);
  // The next good live state brings the page back.
  await page.evaluate(`import("/src/lib/stores.ts").then(({ live }) => live.set({ stage: "closed", mapId: null, players: [] }))`);
  await expect(page.locator(".blank")).toHaveCount(0);
  await expect(page.locator(".live")).toHaveCount(1);
});

test("an unknown stage renders an empty grid instead of crashing", async ({ page }) => {
  await openScene(page, "ingame");
  await page.evaluate(`import("/src/lib/stores.ts").then(({ live }) => live.set({ stage: "champselect", mapId: null, players: [] }))`);
  await expect(page.locator(".live")).toHaveCount(1);
  await expect(page.locator(".live .card, .live .skcard, .live .dashed")).toHaveCount(0);
  await expect(page.locator(".blank")).toHaveCount(0);
});

// Card click → Details. The second click of a double-click lands where the top-left card was: Back's spot.
test.describe("double-click guard", () => {
  /** Centre of Back on the Details page, which sits inside the first card on the Live page. */
  async function backSpot(page: Page): Promise<{ x: number; y: number }> {
    await openScene(page, "details");
    const b = await page.locator(".details .back").boundingBox();
    if (!b) throw new Error("no Back button");
    return { x: b.x + b.width / 2, y: b.y + b.height / 2 };
  }

  test("a double-click on a card opens Details once and the second click does not press Back", async ({ page }) => {
    const spot = await backSpot(page);
    await openScene(page, "ingame");
    const card = await page.locator(".live .card").first().boundingBox();
    expect(card!.x).toBeLessThan(spot.x);
    expect(card!.x + card!.width).toBeGreaterThan(spot.x);
    expect(card!.y).toBeLessThan(spot.y);
    expect(card!.y + card!.height).toBeGreaterThan(spot.y);
    await page.mouse.dblclick(spot.x, spot.y);
    await expect(page.locator(".details")).toHaveCount(1);
    await page.waitForTimeout(150);
    await expect(page.locator(".details")).toHaveCount(1);
    await expect(page.locator(".live")).toHaveCount(0);
  });

  test("400 ms later a click works again: Back returns to Live", async ({ page }) => {
    const spot = await backSpot(page);
    await openScene(page, "ingame");
    await page.mouse.click(spot.x, spot.y);
    await expect(page.locator(".details")).toHaveCount(1);
    await page.waitForTimeout(450);
    await page.mouse.click(spot.x, spot.y);
    await expect(page.locator(".live")).toHaveCount(1);
  });

  test("the guard does not delay the first click on a card", async ({ page }) => {
    await openScene(page, "ingame");
    await page.locator(".live .card").nth(3).click({ position: { x: 20, y: 70 } });
    await expect(page.locator(".details")).toHaveCount(1);
  });
});
