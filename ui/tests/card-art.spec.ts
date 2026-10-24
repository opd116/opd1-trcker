// Card art and images on the v2 screens (R163): the profile drawer's banner (the player card's wide art, a
// `background-image` behind `retryBg`) is kept when an event replaces the card object with one that has the same
// `cardId` (no new probe `Image`, the style never cleared); and an image that fails to load is asked again after the
// first backoff (`retryImg` / `retryBg`, IMG_RETRY_MS) with `?r=1`, and then shows.
import { expect, test, type Page } from "@playwright/test";
import { IMG_RETRY_MS } from "../src/lib/imgRetry";
import { PUUID } from "../src/lib/demo/cdv8";
import { gotoApp, NOW, openScene, routeMedia } from "./harness";

const MARA = PUUID.mara!;
const art = (page: Page) => page.getByRole("dialog").locator(".head .art");

/** Counts every `new Image()` (the probes `retryBg` makes) and every `style` change of the drawer's art. */
async function instrument(page: Page): Promise<void> {
  await page.addInitScript(() => {
    const w = window as unknown as { __probes: number; Image: typeof Image };
    w.__probes = 0;
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
        if (el.matches(".art")) w.__artStyle.push(el.style.backgroundImage === "" ? "cleared" : "set");
      }
    }).observe(document.body, { subtree: true, attributes: true, attributeFilter: ["style"] });
  });

const seen = (page: Page) =>
  page.evaluate(() => {
    const w = window as unknown as { __probes: number; __artStyle: string[] };
    return { probes: w.__probes, style: w.__artStyle.slice() };
  });

test("a details event with the same cardId keeps the drawer's art: no new probe, no style change", async ({ page }) => {
  await instrument(page);
  await openScene(page, "ingame", { profile: MARA });
  await expect(art(page)).toHaveCSS("background-image", /^url\(/);
  await watchArt(page);
  for (let i = 0; i < 2; i++) {
    await page.evaluate(`import("/src/lib/stores.ts").then(({ openDetails }) => { let d; openDetails.subscribe((v) => (d = v))(); openDetails.event(structuredClone(d)); })`);
  }
  // And a live event with the same cards.
  await page.evaluate(`import("/src/lib/stores.ts").then(({ live, applyLive }) => { let s; live.subscribe((v) => (s = v))(); applyLive(structuredClone(s)); })`);
  await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
  expect(await seen(page)).toEqual({ probes: 0, style: [] });
  await expect(art(page)).toHaveCSS("background-image", /^url\(/);
});

test("a changed cardId loads the new art once; an empty one takes the banner away", async ({ page }) => {
  await instrument(page);
  await openScene(page, "ingame", { profile: MARA });
  const before = await art(page).evaluate((el) => (el as HTMLElement).style.backgroundImage);
  const other = (await page.evaluate(
    `import("/src/lib/stores.ts").then(({ live }) => { let s; live.subscribe((v) => (s = v))(); return s.players.find((p) => p.puuid !== ${JSON.stringify(MARA)} && p.cardId).cardId; })`,
  )) as string;
  expect(before).not.toContain(other);
  await watchArt(page);
  const setCard = (id: string) =>
    page.evaluate(
      `import("/src/lib/stores.ts").then(({ openDetails }) => { let d; openDetails.subscribe((v) => (d = v))(); d = structuredClone(d); d.card.cardId = ${JSON.stringify(id)}; openDetails.event(d); })`,
    );
  await setCard(other);
  await expect(art(page)).toHaveCSS("background-image", new RegExp(other));
  expect((await seen(page)).probes).toBe(1);
  await setCard("");
  await expect(art(page)).toHaveCount(0);
});

/**
 * Open a scene whose first request for each URL matching `pattern` fails (503); later ones go to the media cache. The
 * page clock is Playwright's fake one (installed at NOW, running), so the retry's backoff can be skipped.
 */
async function openFailingOnce(page: Page, pattern: RegExp, query: string): Promise<{ failed: () => number }> {
  const seenUrls = new Set<string>();
  await routeMedia(page, (url) => {
    if (!pattern.test(url) || seenUrls.has(url)) return null;
    seenUrls.add(url);
    return { status: 503 };
  });
  await page.clock.install({ time: NOW });
  await gotoApp(page, `/?${query}`);
  return { failed: () => seenUrls.size };
}

test("a card art that fails is asked again after the first backoff (?r=1) and then shows", async ({ page }) => {
  const { failed } = await openFailingOnce(page, /\/playercards\/[^/]+\/wideart\.png$/, `scene=ingame&profile=${MARA}`);
  await expect(page.getByRole("dialog")).toHaveAttribute("aria-busy", "false");
  await expect.poll(failed).toBe(1);
  await page.clock.runFor(IMG_RETRY_MS[0]! + 100);
  await expect(art(page)).toHaveCSS("background-image", /wideart\.png\?r=1/);
  expect(failed()).toBe(1);
});

test("an agent icon that fails is asked again with ?r=1 and then loads", async ({ page }) => {
  const { failed } = await openFailingOnce(page, /\/agents\/[^/]+\/displayicon\.png$/, "scene=ingame");
  const icon = page.locator("main [data-row] .agent img").first();
  await expect(icon).toBeAttached();
  await expect.poll(failed).toBeGreaterThan(0);
  await page.clock.runFor(IMG_RETRY_MS[0]! + 100);
  await expect(icon).toHaveAttribute("src", /\?r=1$/);
  await expect.poll(() => icon.evaluate((img) => (img as HTMLImageElement).complete && (img as HTMLImageElement).naturalWidth > 0)).toBe(true);
});
