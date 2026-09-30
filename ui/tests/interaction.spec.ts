// Hover and click (spec §8.5 "Hover and click"), Live part. Task 18 adds the Details part.
import { expect, test, type Locator, type Page } from "@playwright/test";
import { openScene } from "./harness";

const onLive = async (page: Page) => {
  await expect(page.locator(".live")).toHaveCount(1);
  await expect(page.locator(".scroll")).toHaveCount(0);
};
const cursor = (l: Locator) => l.evaluate((el) => getComputedStyle(el).cursor);

test("hovering a card gives an inset 1px --line-hover shadow", async ({ page }) => {
  await openScene(page, "ingame");
  const sable = page.locator(".live .card").filter({ hasText: "Sable" });
  const shadow = () => sable.evaluate((el) => getComputedStyle(el).boxShadow);
  expect(await shadow()).toBe("none");
  // The art strip: inside the card, on no tooltip trigger.
  await sable.hover({ position: { x: 20, y: 70 } });
  const want = await page.evaluate(() => {
    const d = document.createElement("div");
    d.style.color = "var(--line-hover)";
    document.body.appendChild(d);
    const c = getComputedStyle(d).color;
    d.remove();
    return c;
  });
  expect(await shadow()).toBe(`${want} 0px 0px 0px 1px inset`);
  expect(await cursor(sable)).toBe("pointer");
});

test("clicking a card opens that player's details", async ({ page }) => {
  await openScene(page, "ingame");
  await page.locator(".live .card").filter({ hasText: "Sable" }).click({ position: { x: 20, y: 70 } });
  await expect(page.locator(".live")).toHaveCount(0);
  await expect(page.locator(".scroll")).toHaveCount(1);
});

test("clicking a dashed slot leaves the route unchanged", async ({ page }) => {
  await openScene(page, "pregame");
  const slot = page.locator(".live .dashed").first();
  expect(await cursor(slot)).toBe("default");
  await slot.click();
  await onLive(page);
  const shadow = await slot.evaluate((el) => getComputedStyle(el).boxShadow);
  expect(shadow).toBe("none");
});

test("clicking a skeleton card leaves the route unchanged", async ({ page }) => {
  await openScene(page, "ingame-loading");
  const sk = page.locator(".live .skcard").first();
  expect(await cursor(sk)).toBe("default");
  await sk.click();
  await onLive(page);
});

test("clicking the My Stats tab leaves the route unchanged", async ({ page }) => {
  await openScene(page, "ingame");
  const tab = page.locator("header.bar .tabs").getByText("My Stats", { exact: true });
  expect(await cursor(tab)).toBe("default");
  await tab.click();
  await onLive(page);
});

test("a click while a tip is open hides it", async ({ page }) => {
  await openScene(page, "lobby2", {}, { clock: "manual" });
  const apr = page.locator("header.bar .apr");
  await apr.hover();
  await page.clock.runFor(460);
  await expect(page.locator("#app [role=tooltip]")).toHaveCount(1);
  await apr.click();
  await expect(page.locator("#app [role=tooltip]")).toHaveCount(0);
  await onLive(page);
});
