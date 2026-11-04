// Approved compact shell: title bar, grouped horizontal navigation, square Glance, keyboard and tooltip behavior.
import { expect, test, type Page } from "@playwright/test";
import { PUUID } from "../src/lib/demo/cdv8";
import { IMG_RETRY_MS } from "../src/lib/imgRetry";
import { gotoApp, NOW, openScene, openScreen, screenNav, routeMedia } from "./harness";
test.use({ viewport: { width: 700, height: 860 } });

const box = async (page: Page, sel: string) => (await page.locator(sel).first().boundingBox())!;

for (const [w, h] of [
  [700, 860],
  [680, 760],
] as const) {
  test(`shell layout at ${w}×${h}: title bar and compact nav, main fills the rest, no page scroll`, async ({ page }) => {
    await page.setViewportSize({ width: w, height: h });
    await openScene(page, "ingame");
    expect(await box(page, "#app > header")).toMatchObject({ x: 0, y: 0, width: w, height: 36 });
    expect(await box(page, "nav.compact-nav")).toMatchObject({ x: 0, y: 36, width: w, height: 36 });
    expect(await box(page, "main.main")).toMatchObject({ x: 0, y: 72, width: w, height: h - 72 });
    const scroll = await page.evaluate(() => [document.documentElement.scrollWidth, document.documentElement.scrollHeight]);
    expect(scroll).toEqual([w, h]);
    // The wordmark keeps the product's spelling.
    await expect(page.locator("#app > header .sub")).toHaveText("TRCKER");
    const items = page.locator("nav.compact-nav button");
    await expect(items).toHaveCount(5);
    for (let i = 0; i < 5; i++) expect((await items.nth(i).boundingBox())!.height).toBe(35);
  });
}

test("compact nav: Enter / Space navigate disclosures, Escape restores focus", async ({ page }) => {
  await openScene(page, "ingame");
  const nav = (label: string) => screenNav(page, label);
  await expect(nav("Live")).toHaveAttribute("aria-current", "page");
  await nav("History").focus();
  await page.keyboard.press("Enter");
  const history = page.getByRole("navigation").getByRole("button", { name: "History", exact: true });
  await history.focus();
  await page.keyboard.press("Enter");
  await expect(nav("History")).toHaveClass(/active/);
  await expect(nav("Live")).not.toHaveAttribute("aria-current", /.*/);
  await nav("About").focus();
  await page.keyboard.press("Space");
  await expect(nav("About")).toHaveAttribute("aria-current", "page");
  const ring = await nav("About").evaluate((el) => getComputedStyle(el).outlineColor);
  expect(ring).toBe("rgb(240, 116, 125)");
  await nav("Comp").click();
  await page.keyboard.press("Escape");
  await expect(nav("Comp")).toBeFocused();
  await expect(nav("Comp")).toHaveAttribute("aria-expanded", "false");
  await openScreen(page, "Comp");
  await expect(page.locator("main .mystats")).toBeVisible();
});

test("compact roster: a rank icon that failed to load is asked again (R163)", async ({ page }) => {
  // Every tier icon fails once (the 503 Rust answers while valorant-api can't be reached); its retry (`?r=1`) loads.
  await routeMedia(page, (url) => (/\/competitivetiers\/.*smallicon\.png$/.test(url) ? { status: 503 } : null));
  await page.clock.install({ time: NOW });
  await gotoApp(page, "/?scene=ingame");
  const icon = page.locator(".compact-player .rank-icon-slot img").first();
  await icon.waitFor({ state: "attached" });
  await page.waitForTimeout(300);
  await page.clock.runFor(IMG_RETRY_MS[0]!);
  await expect(icon).toHaveAttribute("src", /[?&]r=1$/);
  await expect.poll(() => icon.evaluate((img: HTMLImageElement) => img.complete && img.naturalWidth > 0)).toBe(true);
});

test("titlebar stays quiet; pregame status and countdown belong to Live", async ({ page }) => {
  await openScene(page, "pregame");
  await expect(page.locator("#app > header .pill")).toHaveCount(0);
  await expect(page.locator(".compact-match .pregame")).toHaveText("Agent select0:422/5 locked");
});

test("tooltip: the design's bubble, flipped below a top-edge trigger and clamped to the viewport", async ({ page }) => {
  // R206 removed the title bar's tipped pin; the glance bar's Exit glance is the top-edge trigger the shell still has.
  await page.setViewportSize({ width: 420, height: 420 });
  await openScene(page, "ingame", { glance: "1" });
  const exit = page.locator(".glance header.bar button.exit");
  await exit.hover();
  const tip = page.locator("#tooltip");
  await expect(tip).toBeVisible();
  await expect(tip).toHaveText("Back to the full window");
  await expect(tip).toHaveCSS("opacity", "1");
  const b = (await tip.boundingBox())!;
  const t = (await exit.boundingBox())!;
  // No room above the 32px glance bar: the bubble flips below it, 8px off, and the window's right edge clamps it.
  expect(b.y, "flipped below the bar").toBeGreaterThanOrEqual(t.y + t.height);
  expect(Math.abs(t.y + t.height + 8 - b.y)).toBeLessThanOrEqual(1);
  expect(b.x + b.width).toBeLessThanOrEqual(page.viewportSize()!.width - 6);
  await expect(tip).toHaveCSS("background-color", "rgb(29, 32, 38)");
  await expect(tip).toHaveCSS("border-radius", "7px");
  await expect(tip).toHaveCSS("font-size", "12.5px");
  await page.keyboard.press("Escape");
  await expect(tip).toHaveCount(0);
});

test("profile drawer mounts over main from ?profile=, and closes", async ({ page }) => {
  await openScene(page, "ingame", { profile: PUUID.mara! });
  const drawer = page.locator("main.main [role=dialog]");
  await expect(drawer).toBeVisible();
  await openScreen(page, "Saved"); // go() closes the drawer
  await expect(page.locator("main.main [role=dialog]")).toHaveCount(0);
});

test("glance mode: a 420px column with its 32px title bar; Exit glance returns to the full layout", async ({ page }) => {
  await page.setViewportSize({ width: 420, height: 420 });
  await openScene(page, "ingame", { glance: "1" });
  const bar = page.locator(".glance header.bar");
  expect(await bar.boundingBox()).toMatchObject({ x: 1, y: 0, width: 418, height: 32 });
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(420);
  await page.setViewportSize({ width: 700, height: 860 });
  await page.getByRole("button", { name: "Exit glance" }).click();
  await expect(page.locator("nav.compact-nav")).toBeVisible();
});
