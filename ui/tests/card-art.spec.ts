// R163 review fix: card art (the `background-image` of a Live card and of the Details hero) is kept when the card
// object is replaced by an event that carries the same `cardId`: no new probe `Image`, and the style is never
// cleared. A new `live` / `details` event builds new card objects every time, so the attachment must be keyed on
// the URL, not on the object.
import { expect, test, type Page } from "@playwright/test";
import { openScene, waitPage } from "./harness";

/** Counts every `new Image()` (the probes `retryBg` makes) and every `style` change of an art element. */
async function instrument(page: Page): Promise<void> {
  await page.addInitScript(() => {
    const w = window as unknown as { __probes: number; __artStyle: string[]; Image: typeof Image };
    w.__probes = 0;
    w.__artStyle = [];
    const Original = w.Image;
    w.Image = function (...args: ConstructorParameters<typeof Image>) {
      w.__probes++;
      return new Original(...args);
    } as unknown as typeof Image;
    w.Image.prototype = Original.prototype;
  });
}

const watchArt = (page: Page) =>
  page.evaluate(() => {
    const w = window as unknown as { __probes: number; __artStyle: string[] };
    w.__probes = 0;
    w.__artStyle = [];
    new MutationObserver((records) => {
      for (const r of records) {
        const el = r.target as HTMLElement;
        if (el.matches(".art-img, .hart-img")) w.__artStyle.push(el.style.backgroundImage === "" ? "cleared" : "set");
      }
    }).observe(document.body, { subtree: true, attributes: true, attributeFilter: ["style"] });
  });

const seen = (page: Page) =>
  page.evaluate(() => {
    const w = window as unknown as { __probes: number; __artStyle: string[] };
    return { probes: w.__probes, style: w.__artStyle.slice() };
  });

/** DEV only: the app's own module instances (the dev server serves each module at one URL). */
const fromApp = (page: Page, expr: string) => page.evaluate(expr);

test("a live event with the same cardIds keeps every card's art: no new probe, no style change", async ({ page }) => {
  await instrument(page);
  await openScene(page, "ingame");
  const arts = page.locator(".live .card .art-img");
  expect(await arts.count()).toBe(10);
  for (const el of await arts.all()) await expect(el).toHaveCSS("background-image", /^url\(/);
  await watchArt(page);
  for (let i = 0; i < 2; i++) {
    await fromApp(
      page,
      `import("/src/lib/stores.ts").then(({ live, applyLive }) => { let s; live.subscribe((v) => (s = v))(); applyLive(structuredClone(s)); })`,
    );
    await page.waitForTimeout(100);
  }
  expect(await seen(page)).toEqual({ probes: 0, style: [] });
  for (const el of await arts.all()) await expect(el).toHaveCSS("background-image", /^url\(/);
});

test("a details event with the same cardId keeps the hero's art", async ({ page }) => {
  await instrument(page);
  await openScene(page, "details");
  await waitPage(page, "loaded");
  await expect(page.locator(".hart-img")).toHaveCSS("background-image", /^url\(/);
  await watchArt(page);
  await fromApp(
    page,
    `import("/src/lib/stores.ts").then(({ openDetails }) => { let d; openDetails.subscribe((v) => (d = v))(); openDetails.event(structuredClone(d)); })`,
  );
  await page.waitForTimeout(100);
  expect(await seen(page)).toEqual({ probes: 0, style: [] });
  await expect(page.locator(".hart-img")).toHaveCSS("background-image", /^url\(/);
});
