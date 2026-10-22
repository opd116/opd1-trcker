// Competitive and Collection (design v2, TrackerApp.dc.html 658–861) against the DEV demo feed: the cards render from
// the snapshot, the RR chart's hover card / keyboard / click, the session chips opening their scoreboards, every
// My Stats state fitting without sideways scroll at both window sizes, and the Collection's filters and empty state.
import { expect, test, type Page } from "@playwright/test";
import { gotoApp, NOW, routeMedia, settle, type MediaOverride } from "./harness";

/** DEV only: a value read from the app's own router instance (the dev server serves each module at one URL). */
const routerState = (page: Page) =>
  page.evaluate(
    `import("/src/lib/router.svelte.ts").then(({ router }) => ({ screen: router.screen, matchId: router.matchId, from: router.matchFrom }))`,
  ) as Promise<{ screen: string; matchId: string | null; from: string | null }>;

/** `override`: the media URLs a test fails on purpose (harness `routeMedia`). */
async function open(page: Page, query: string, override?: MediaOverride): Promise<void> {
  await routeMedia(page, override);
  await page.clock.setFixedTime(NOW);
  await gotoApp(page, `/?${query}`);
}

async function openComp(page: Page, scene = "mystats"): Promise<void> {
  await open(page, `scene=${scene}&screen=comp`);
  await page.locator(`.mystats[aria-busy="${scene === "mystats-loading"}"]`).waitFor();
  await settle(page);
}

test.describe("Competitive", () => {
  test("the cards render the snapshot", async ({ page }) => {
    await openComp(page);
    await expect(page.locator("main h1")).toHaveText("Competitive");
    const rank = page.getByRole("region", { name: "Rank now" });
    await expect(rank).toContainText("Ascendant 1");
    await expect(rank).toContainText("21 / 100 RR · 79 to Ascendant 2");
    await expect(rank).toContainText("Act start");
    await expect(page.getByRole("region", { name: "RR progression" })).toContainText("LAST 40 OF 60 MATCHES");
    await expect(page.getByRole("region", { name: "Maps" }).locator(".row")).toHaveCount(7);
    await expect(page.getByRole("region", { name: "Best times to play" }).locator(".day")).toHaveCount(7);
  });

  test("hovering a chart column shows its card; a known match opens its scoreboard", async ({ page }) => {
    await openComp(page);
    const cols = page.locator(".plot .col");
    await expect(cols).toHaveCount(40);
    await cols.nth(39).hover();
    const card = page.locator(".hcard");
    await expect(card).toContainText("Match 40 of 40");
    await expect(card).toContainText("RR →");
    // the newest point is the user's newest match (last5), so it carries a score and opens
    await expect(cols.nth(39)).toHaveClass(/\bopen\b/);
    await cols.nth(39).click();
    await expect.poll(() => routerState(page)).toMatchObject({ screen: "history", from: "comp" });
    expect((await routerState(page)).matchId).not.toBeNull();
  });

  test("the chart is one keyboard stop: focus picks the newest, ← moves, Enter opens", async ({ page }) => {
    await openComp(page);
    const slider = page.getByRole("slider", { name: "RR progression" });
    await page.keyboard.press("Tab"); // keyboard modality for :focus-visible
    await slider.focus();
    await expect(slider).toHaveAttribute("aria-valuenow", "39");
    await page.keyboard.press("ArrowLeft");
    await expect(slider).toHaveAttribute("aria-valuenow", "38");
    await expect(page.locator(".hcard")).toContainText("Match 39 of 40");
    await page.keyboard.press("End");
    await page.keyboard.press("Enter");
    await expect.poll(() => routerState(page)).toMatchObject({ screen: "history", from: "comp" });
  });

  test("a session chip opens its match", async ({ page }) => {
    await openComp(page);
    const chips = page.getByRole("region", { name: "Today's session" }).locator("button.chip");
    expect(await chips.count()).toBeGreaterThan(0);
    await chips.last().click();
    await expect.poll(() => routerState(page)).toMatchObject({ screen: "history", from: "comp" });
  });

  for (const size of [
    { width: 1280, height: 800 },
    { width: 1180, height: 760 },
  ]) {
    for (const scene of ["mystats", "mystats-new", "mystats-loading", "mystats-unranked", "mystats-immortal", "mystats-rrnull", "mystats-longnames"]) {
      test(`${scene} at ${size.width}×${size.height}: no sideways scroll, nothing spills out of a card`, async ({ page }) => {
        await page.setViewportSize(size);
        await openComp(page, scene);
        const fit = await page.evaluate(() => {
          const s = document.querySelector<HTMLElement>("main .screen")!;
          const spill = [...s.querySelectorAll<HTMLElement>("section")].filter((c) => c.scrollWidth > c.clientWidth + 1).length;
          return { sideways: s.scrollWidth > s.clientWidth, spill };
        });
        expect(fit).toEqual({ sideways: false, spill: 0 });
      });
    }
  }
});

test.describe("Collection", () => {
  test("value, weapon filter, edition toggle and the no-results state", async ({ page }) => {
    await open(page, "scene=closed&screen=skins");
    await expect(page.locator("main h1")).toHaveText("Collection");
    await expect(page.locator(".value .num")).toHaveText("46,075");
    await expect(page.locator(".value .sar")).toHaveText("≈ 1,571 SAR");
    const cards = page.getByRole("region", { name: "Skins" }).locator(".card");
    await expect(cards).toHaveCount(22);
    // sorted by value: the melee skins (double price) first, then by full name; Rust's short names get their weapon
    await expect(cards.first()).toContainText("Elderflame Dagger");
    await expect(cards.first()).toContainText("4,350 VP");
    await expect(cards.filter({ hasText: "Reaver Vandal" })).toHaveCount(1);

    const weapons = page.getByRole("region", { name: "By weapon" }).locator("button");
    await expect(weapons.first()).toContainText("All weapons");
    await expect(weapons.first()).toHaveAttribute("aria-pressed", "true");
    await weapons.filter({ hasText: "Vandal" }).click();
    await expect(cards).toHaveCount(4);

    const editions = page.getByRole("region", { name: "Editions" });
    const ultra = editions.locator("button", { hasText: "Ultra" });
    await ultra.click();
    await expect(ultra).toHaveAttribute("aria-pressed", "true");
    await expect(cards).toHaveCount(1);
    await editions.locator("button", { hasText: "Select" }).click();
    await expect(cards).toHaveCount(0);
    await expect(page.getByText("No skins with these filters.")).toBeVisible();
    await editions.locator("button", { hasText: "Select" }).click();
    await expect(cards).toHaveCount(4);
  });

  test("art that cannot load shows the unavailable frame", async ({ page }) => {
    await open(page, "scene=closed&screen=skins", (url) => (/weaponskinlevels/.test(url) ? { status: 404 } : null));
    await expect(page.getByText("ARTWORK UNAVAILABLE")).toHaveCount(22);
  });

  test("a collection never fetched: the empty state, no figures", async ({ page }) => {
    await open(page, "scene=closed&screen=skins&empty=1");
    await expect(page.getByText("No collection yet")).toBeVisible();
    await expect(page.getByText("Start VALORANT once to load your collection.", { exact: false })).toBeVisible();
    await expect(page.locator(".value")).toHaveCount(0);
  });

  for (const size of [
    { width: 1280, height: 800 },
    { width: 1180, height: 760 },
  ]) {
    test(`fits ${size.width}×${size.height}: the page never scrolls, the lists scroll inside their cards`, async ({ page }) => {
      await page.setViewportSize(size);
      await open(page, "scene=closed&screen=skins");
      const fit = await page.evaluate(() => {
        const s = document.querySelector<HTMLElement>("main .screen")!;
        return { y: s.scrollHeight > s.clientHeight, x: s.scrollWidth > s.clientWidth };
      });
      expect(fit).toEqual({ y: false, x: false });
    });
  }
});
