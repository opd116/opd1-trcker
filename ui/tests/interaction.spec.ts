// Hover and click (spec §8.5 "Hover and click"): the Live part, then the Details and Match part (Task 18).
import { expect, test, type Locator, type Page } from "@playwright/test";
import { openScene, varRgb, waitPage } from "./harness";

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
  expect(await shadow()).toBe(`${await varRgb(page, "--line-hover")} 0px 0px 0px 1px inset`);
  expect(await cursor(sable)).toBe("pointer");
});

/**
 * DEV only: a value read from the app's own module instances. The dev server serves each module at one URL, so a
 * dynamic import in the page returns the very instance the app uses. The body is a string so no test transform
 * touches the `import()`.
 */
const fromApp = <T>(page: Page, expr: string): Promise<T> => page.evaluate(expr) as Promise<T>;

test("clicking a card opens that player's details (Sable, not another card)", async ({ page }) => {
  await openScene(page, "ingame");
  const sable = await fromApp<string>(
    page,
    `import("/src/lib/stores.ts").then(({ live }) => {
      let s; live.subscribe((v) => (s = v))();
      return s.players.find((p) => p.name?.kind === "visible" && p.name.name === "Sable").puuid;
    })`,
  );
  expect(sable).toBeTruthy();
  await page.locator(".live .card").filter({ hasText: "Sable" }).click({ position: { x: 20, y: 70 } });
  await expect(page.locator(".live")).toHaveCount(0);
  await expect(page.locator(".scroll")).toHaveCount(1);
  const route = await fromApp<unknown>(page, `import("/src/lib/router.svelte.ts").then(({ router }) => JSON.parse(JSON.stringify(router.route)))`);
  expect(route).toEqual({ name: "details", puuid: sable, more: false });
  // And the page shows that player (Task 18).
  await waitPage(page, "loaded");
  await expect(page.locator(".details .hero .nm")).toHaveText(/Sable\s*#1177/);
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

// ── Details and Match (spec §6.1, §6.4, §6.5, §6.7, §6.10) ──────────────────────────────────────────────────────
const bg = (l: Locator) => l.evaluate((el) => getComputedStyle(el).backgroundColor);
const TRANSPARENT = "rgba(0, 0, 0, 0)";
const routeOf = (page: Page) => fromApp<unknown>(page, `import("/src/lib/router.svelte.ts").then(({ router }) => JSON.parse(JSON.stringify(router.route)))`);

/** Hover a spot of `l` that holds no tooltip trigger (its right end: the chevron column), read the background. */
async function hoverBg(page: Page, l: Locator): Promise<string> {
  const b = await l.boundingBox();
  if (!b) throw new Error("no box");
  await page.mouse.move(b.x + b.width - 6, b.y + b.height / 2);
  return bg(l);
}

test("Details: Back, recent rows, encounter rows and Show more change background on hover, with a pointer cursor", async ({ page }) => {
  await openScene(page, "details");
  const cases: Array<[string, Locator, string, string]> = [
    ["Back", page.locator(".details .back"), "--card", "--raised"],
    ["recent row", page.locator(".details .recent .rrow").first(), "", "--hover-row"],
    ["encounter row", page.locator(".details .encs .erow").first(), "", "--hover-row"],
    ["Show more", page.locator(".details .more"), "", "--card"],
  ];
  for (const [what, l, rest, hover] of cases) {
    await l.scrollIntoViewIfNeeded();
    await page.mouse.move(5, 5);
    expect(await bg(l), `${what} at rest`).toBe(rest ? await varRgb(page, rest) : TRANSPARENT);
    expect(await hoverBg(page, l), `${what} hovered`).toBe(await varRgb(page, hover));
    expect(await cursor(l), `${what} cursor`).toBe("pointer");
  }
});

test("Match: Back changes background on hover", async ({ page }) => {
  await openScene(page, "match");
  const back = page.locator(".match .back");
  expect(await bg(back)).toBe(await varRgb(page, "--card"));
  expect(await hoverBg(page, back)).toBe(await varRgb(page, "--raised"));
  expect(await cursor(back)).toBe("pointer");
});

test("Details: Top 3 rows, loadout rows and the hero are not clickable", async ({ page }) => {
  await openScene(page, "details");
  const before = await routeOf(page);
  for (const [what, l] of [
    ["Top 3 row", page.locator(".details .mate").first()],
    ["loadout row", page.locator(".details .wrow").first()],
    ["hero", page.locator(".details .hero")],
  ] as const) {
    expect(await cursor(l), `${what} cursor`).not.toBe("pointer");
    expect(await hoverBg(page, l), `${what} hover`).toBe(await bg(l));
    await l.click({ position: { x: 4, y: 4 } });
    expect(await routeOf(page), what).toEqual(before);
  }
});

test("Match: scoreboard rows and team headers are not clickable and do not change on hover", async ({ page }) => {
  await openScene(page, "match");
  const before = await routeOf(page);
  for (const [what, l] of [
    ["scoreboard row", page.locator(".match .srow").nth(3)],
    ["team header", page.locator(".match .thead").first()],
    ["match header", page.locator(".match .mhead")],
  ] as const) {
    expect(await cursor(l), `${what} cursor`).not.toBe("pointer");
    await page.mouse.move(5, 5);
    const rest = await bg(l);
    expect(await hoverBg(page, l), `${what} hover`).toBe(rest);
    await l.click({ position: { x: 4, y: 4 } });
    expect(await routeOf(page), what).toEqual(before);
  }
});

test("Details: Enter on a focused recent row opens its match (keyboard)", async ({ page }) => {
  await openScene(page, "details");
  await page.locator(".details .recent .rrow").first().focus();
  await page.keyboard.press("Enter");
  await waitPage(page, "loaded");
  expect(await routeOf(page)).toMatchObject({ name: "match", from: "recent" });
});
