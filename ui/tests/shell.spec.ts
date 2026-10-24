// The v2 shell (title bar, nav rail, glance frame, tooltip) against the DEV demo feed: sizes from the design
// (.superpowers/design/v2/TrackerApp.dc.html 97–139), keyboard navigation, the tooltip's placement, no scrollbars.
import { expect, test, type Page } from "@playwright/test";
import { PUUID } from "../src/lib/demo/cdv8";
import { openScene } from "./harness";

const box = async (page: Page, sel: string) => (await page.locator(sel).first().boundingBox())!;

for (const [w, h] of [
  [1280, 800],
  [1180, 760],
] as const) {
  test(`shell layout at ${w}×${h}: 36px title bar, 76px rail, main fills the rest, no page scroll`, async ({ page }) => {
    await page.setViewportSize({ width: w, height: h });
    await openScene(page, "ingame");
    expect(await box(page, "#app > header")).toMatchObject({ x: 0, y: 0, width: w, height: 36 });
    expect(await box(page, "nav.rail")).toMatchObject({ x: 0, y: 36, width: 76, height: h - 36 });
    expect(await box(page, "main.main")).toMatchObject({ x: 76, y: 36, width: w - 76, height: h - 36 });
    const scroll = await page.evaluate(() => [document.documentElement.scrollWidth, document.documentElement.scrollHeight]);
    expect(scroll).toEqual([w, h]);
    // The wordmark keeps the product's spelling.
    await expect(page.locator("#app > header .sub")).toHaveText("TRCKER");
    // Eight nav items of 58px, the design's colours.
    const items = page.locator("nav.rail button.item");
    await expect(items).toHaveCount(8);
    for (let i = 0; i < 8; i++) expect((await items.nth(i).boundingBox())!.height).toBe(58);
  });
}

test("nav rail: Enter / Space navigate, aria-current follows, the live dot shows in a match", async ({ page }) => {
  await openScene(page, "ingame");
  const nav = (label: string) => page.locator(`nav.rail button[aria-label="${label}"]`);
  await expect(nav("Live")).toHaveAttribute("aria-current", "page");
  await expect(nav("Live").locator(".live-dot")).toHaveCount(1);
  await nav("History").focus();
  await page.keyboard.press("Enter");
  await expect(nav("History")).toHaveAttribute("aria-current", "page");
  await expect(nav("Live")).not.toHaveAttribute("aria-current", /.*/);
  await nav("About").focus();
  await page.keyboard.press("Space");
  await expect(nav("About")).toHaveAttribute("aria-current", "page");
  const ring = await nav("About").evaluate((el) => getComputedStyle(el).outlineColor);
  expect(ring).toBe("rgb(183, 171, 255)");
  // The rank widget opens Competitive.
  await page.locator('nav.rail button[aria-label="Your rank: open Competitive"]').click();
  await expect(nav("Comp")).toHaveAttribute("aria-current", "page");
});

test("status pill: Connected, with AGENT SELECT in pregame", async ({ page }) => {
  await openScene(page, "pregame");
  const pill = page.locator("#app > header .pill");
  await expect(pill.locator(".label")).toHaveText("Connected");
  await expect(pill.locator(".detail")).toHaveText("AGENT SELECT");
  expect((await pill.boundingBox())!.height).toBe(22);
});

test("tooltip: the design's bubble, after 450 ms, flipped below the title bar and clamped to the viewport", async ({ page }) => {
  await openScene(page, "ingame");
  await page.locator('#app > header button[aria-label="Close"]').hover(); // right edge: clamped
  await page.locator('#app > header button[aria-label="Keep on top during matches"]').hover();
  const tip = page.locator("#tooltip");
  await expect(tip).toBeVisible();
  await expect(tip).toHaveText("Keep on top during matches");
  await expect(tip).toHaveCSS("opacity", "1");
  const b = (await tip.boundingBox())!;
  expect(b.y).toBe(43.5); // under the 36px bar (no room above): bottom 35.5 + 8
  expect(b.x + b.width).toBeLessThanOrEqual(1280 - 6);
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
  const nav = page.locator('nav.rail button[aria-label="Saved"]');
  await nav.click(); // go() closes the drawer
  await expect(page.locator("main.main [role=dialog]")).toHaveCount(0);
});

test("glance mode: a 420px column with its 32px title bar; Exit glance returns to the full layout", async ({ page }) => {
  await page.setViewportSize({ width: 420, height: 760 });
  await openScene(page, "ingame", { glance: "1" });
  const bar = page.locator(".glance header.bar");
  expect(await bar.boundingBox()).toMatchObject({ x: 1, y: 0, width: 418, height: 32 });
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(420);
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.getByRole("button", { name: "Exit glance" }).click();
  await expect(page.locator("nav.rail")).toBeVisible();
});
