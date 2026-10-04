// Navigation around My Stats (spec §2.4, §9.1): the tab switch both ways, a Recent Matches row → Match → Back to My Stats,
// Agent Select forcing Live, the start page, and the page staying put when new data arrives.
import { expect, test, type Page } from "@playwright/test";
import { PUUID, recentMatchId } from "../src/lib/demo/cdv8";
import { myStatsFull, myStatsLongnames, myStatsUnranked } from "../src/lib/demo/mystats";
import { openScene, waitPage } from "./harness";

/** DEV only: a value read from the app's own module instances (the dev server serves each module at one URL). */
const fromApp = <T>(page: Page, expr: string): Promise<T> => page.evaluate(expr) as Promise<T>;
const route = (page: Page) => fromApp<unknown>(page, `import("/src/lib/router.svelte.ts").then(({ router }) => JSON.parse(JSON.stringify(router.route)))`);
const tab = (page: Page, label: string) => page.locator("header.bar .tab", { hasText: label });
const scrollTop = (page: Page) => page.locator("#app .scroll").evaluate((el) => el.scrollTop);
const matchesClosed = (page: Page) => page.evaluate(() => Number(document.documentElement.dataset.matchClosed ?? 0));

test("the tabs: Live → My Stats → Live; the active one has the accent indicator, the Live data is kept", async ({ page }) => {
  await openScene(page, "ingame");
  await expect(page.locator(".live .card")).toHaveCount(10);
  await expect(tab(page, "Live")).toHaveClass(/\bon\b/);
  await expect(tab(page, "My Stats")).toHaveClass(/\boff\b/);
  await expect(page.locator("header.bar .right img")).toHaveCount(1); // the map, on the Live grid

  await tab(page, "My Stats").click();
  await waitPage(page, "loaded");
  expect(await route(page)).toEqual({ name: "mystats" });
  await expect(page.locator(".live")).toHaveCount(0);
  await expect(tab(page, "My Stats")).toHaveClass(/\bon\b/);
  await expect(tab(page, "My Stats")).toHaveAttribute("aria-current", "page");
  await expect(tab(page, "My Stats").locator(".ind")).toHaveCount(1);
  await expect(tab(page, "Live")).toHaveClass(/\boff\b/);
  await expect(tab(page, "Live").locator(".ind")).toHaveCount(0);
  await expect(page.locator("header.bar .right")).toHaveCount(0); // the map belongs to the Live grid

  await tab(page, "Live").click();
  expect(await route(page)).toEqual({ name: "live" });
  await expect(page.locator(".live .card")).toHaveCount(10);
  await expect(tab(page, "Live")).toHaveClass(/\bon\b/);
  await expect(page.locator("header.bar .right img")).toHaveCount(1);
});

test("the inactive tab is a pointer, a button, and reachable by keyboard", async ({ page }) => {
  await openScene(page, "ingame");
  expect(await tab(page, "My Stats").evaluate((el) => [el.tagName, getComputedStyle(el).cursor])).toEqual(["BUTTON", "pointer"]);
  await page.keyboard.press("Tab");
  await page.keyboard.press("Tab");
  await expect(tab(page, "My Stats")).toBeFocused();
  await page.keyboard.press("Enter");
  await waitPage(page, "loaded");
  expect(await route(page)).toEqual({ name: "mystats" });
});

test("clicking My Stats while on it changes nothing, and keeps the scroll position", async ({ page }) => {
  await openScene(page, "mystats");
  await page.locator("#app .scroll").evaluate((el) => (el.scrollTop = 300));
  await tab(page, "My Stats").click();
  expect(await route(page)).toEqual({ name: "mystats" });
  expect(await scrollTop(page)).toBe(300);
});

test("a Recent Matches row opens its match with the user as the focus; the tab stays My Stats; Back returns to My Stats", async ({ page }) => {
  await openScene(page, "mystats");
  const row = page.locator(".mystats .last5 .rrow").nth(1);
  await row.scrollIntoViewIfNeeded();
  const closedBefore = await matchesClosed(page);
  await row.click({ position: { x: 20, y: 20 } });
  await waitPage(page, "loaded");
  expect(await route(page)).toEqual({
    name: "match",
    matchId: recentMatchId("fahad", 1),
    focusPuuid: PUUID.fahad,
    from: "recent",
    back: "mystats",
  });
  await expect(page.locator(".match")).toHaveCount(1);
  await expect(page.locator(".mystats")).toHaveCount(0);
  await expect(tab(page, "My Stats")).toHaveClass(/\bon\b/); // a Match opened from My Stats is under My Stats
  await expect(tab(page, "Live")).toHaveClass(/\boff\b/);
  // The user's own match: their team first, the user's row framed.
  await expect(page.locator(".match .thead .tlabel").first()).toHaveText("Your team");
  await expect(page.locator(".match .srow .selfframe")).toHaveCount(1);

  await page.locator(".match .back").click();
  expect(await route(page)).toEqual({ name: "mystats" });
  await expect(page.locator(".mystats")).toHaveCount(1);
  await expect(page.locator(".match")).toHaveCount(0);
  await expect(tab(page, "My Stats")).toHaveClass(/\bon\b/);
  expect(await matchesClosed(page)).toBe(closedBefore + 1); // leaving the Match page tells Rust (close_match)
  await expect(page.locator(".mystats .last5 .rrow")).toHaveCount(5);
});

test("a Session chip opens its match with the user as the focus; the tab stays My Stats; Back returns to the same scroll position", async ({ page }) => {
  const m = myStatsFull();
  await openScene(page, "mystats");
  const chips = page.locator(".mystats .session .mc");
  await expect(chips).toHaveCount(m.session!.matches.length);
  await page.locator("#app .scroll").evaluate((el) => (el.scrollTop = 150));
  const chip = chips.nth(4);
  await chip.scrollIntoViewIfNeeded();
  const y = await scrollTop(page);
  const closedBefore = await matchesClosed(page);
  await chip.click();
  await waitPage(page, "loaded");
  const want = m.session!.matches[4]!;
  expect(await route(page)).toEqual({ name: "match", matchId: want.matchId, focusPuuid: PUUID.fahad, from: "recent", back: "mystats" });
  await expect(page.locator(".mystats")).toHaveCount(0);
  await expect(tab(page, "My Stats")).toHaveClass(/\bon\b/);
  await expect(tab(page, "Live")).toHaveClass(/\boff\b/);
  // The chip's own match, from the user's side: its map and result as the chip said, their team first, the user's row framed.
  await expect(page.locator(".match .thead .tlabel").first()).toHaveText("Your team");
  await expect(page.locator(".match .srow .selfframe")).toHaveCount(1);
  expect(await scrollTop(page), "the Match starts at the top").toBe(0);

  await page.locator(".match .back").click();
  expect(await route(page)).toEqual({ name: "mystats" });
  await expect(page.locator(".mystats .session .mc")).toHaveCount(m.session!.matches.length);
  expect(await scrollTop(page)).toBe(y);
  expect(await matchesClosed(page)).toBe(closedBefore + 1);
});

test("every kind of Session chip opens a Match page that agrees with it: the first, a loss, the newest and a draw", async ({ page }) => {
  const opens = async (scene: string, i: number, mark: string) => {
    await openScene(page, scene);
    await page.locator(".mystats .session .mc").nth(i).click();
    await waitPage(page, "loaded");
    // The Match header's result chip is the user's team's: the chip clicked says the same thing.
    await expect(page.locator(".match .mhead .chip"), `${scene} chip ${i}`).toHaveText(mark);
  };
  const full = myStatsFull().session!.matches;
  for (const i of [0, full.findIndex((c) => c.result === "L"), full.length - 1]) await opens("mystats", i, full[i]!.result);
  const long = myStatsLongnames().session!.matches; // the longest-names scene's session has draws
  const d = long.findIndex((c) => c.result === "D");
  expect(d).toBeGreaterThanOrEqual(0);
  await opens("mystats-longnames", d, "D");
});

test("Enter and Space on a focused Session chip open its match (keyboard)", async ({ page }) => {
  for (const key of ["Enter", " "]) {
    await openScene(page, "mystats");
    const chip = page.locator(".mystats .session .mc").first();
    await chip.focus();
    await page.keyboard.press(key);
    await waitPage(page, "loaded");
    expect(await route(page), key).toMatchObject({ name: "match", matchId: myStatsFull().session!.matches[0]!.matchId, from: "recent", back: "mystats" });
    await page.locator(".match .back").click();
    await expect(page.locator(".mystats")).toHaveCount(1);
  }
});

test("Session chips are reachable by Tab, in order, and a placement session (no stored RR) opens too", async ({ page }) => {
  await openScene(page, "mystats-unranked");
  const chips = page.locator(".mystats .session .mc");
  await expect(chips).toHaveCount(3);
  await chips.first().focus();
  await page.keyboard.press("Tab");
  await expect(chips.nth(1)).toBeFocused();
  await page.keyboard.press("Tab");
  await expect(chips.nth(2)).toBeFocused();
  await page.keyboard.press("Enter");
  await waitPage(page, "loaded");
  expect(await route(page)).toMatchObject({ name: "match", matchId: myStatsUnranked().session!.matches[2]!.matchId, back: "mystats" });
});

test("Enter on a focused Recent Matches row opens its match (keyboard)", async ({ page }) => {
  await openScene(page, "mystats");
  await page.locator(".mystats .last5 .rrow").first().focus();
  await page.keyboard.press("Enter");
  await waitPage(page, "loaded");
  expect(await route(page)).toMatchObject({ name: "match", from: "recent", back: "mystats" });
});

test("the Live tab from a Match opened on My Stats goes to the Live grid", async ({ page }) => {
  await openScene(page, "mystats");
  await page.locator(".mystats .last5 .rrow").first().click({ position: { x: 20, y: 20 } });
  await waitPage(page, "loaded");
  await tab(page, "Live").click();
  expect(await route(page)).toEqual({ name: "live" });
  await expect(page.locator(".live .card")).toHaveCount(10);
});

test("a Match opened from Details still goes Back to Details, and the tab there is Live", async ({ page }) => {
  await openScene(page, "details");
  await expect(tab(page, "Live")).toHaveClass(/\bon\b/);
  await page.locator(".details .recent .rrow").first().click({ position: { x: 20, y: 20 } });
  await waitPage(page, "loaded");
  await expect(tab(page, "Live")).toHaveClass(/\bon\b/);
  await page.locator(".match .back").click();
  expect(await route(page)).toMatchObject({ name: "details" });
});

test("entering Agent Select brings the user back to Live, from My Stats and from a Match opened there", async ({ page }) => {
  const enterAgentSelect = (p: Page) =>
    fromApp<void>(
      p,
      `import("/src/lib/stores.ts").then(({ applyLive, live }) => {
        let s; live.subscribe((v) => (s = v))();
        applyLive({ ...s, stage: "pregame" });
      })`,
    );
  await openScene(page, "mystats"); // the scene's live stage is In Game
  await enterAgentSelect(page);
  expect(await route(page)).toEqual({ name: "live" });
  await expect(page.locator(".live")).toHaveCount(1);
  await expect(page.locator(".live .dashed")).toHaveCount(5); // Agent Select's empty right column

  await openScene(page, "mystats");
  await page.locator(".mystats .last5 .rrow").first().click({ position: { x: 20, y: 20 } });
  await waitPage(page, "loaded");
  await enterAgentSelect(page);
  expect(await route(page)).toEqual({ name: "live" });
  await expect(page.locator(".match")).toHaveCount(0);
});

test("a later stage change that is not Agent Select leaves My Stats alone", async ({ page }) => {
  await openScene(page, "mystats");
  await fromApp<void>(
    page,
    `import("/src/lib/stores.ts").then(({ applyLive, live }) => {
      let s; live.subscribe((v) => (s = v))();
      applyLive({ ...s, stage: "menus" });
      applyLive({ ...s, stage: "closed" });
    })`,
  );
  expect(await route(page)).toEqual({ name: "mystats" });
});

test("the start page: closed opens My Stats, any other stage opens Live", async ({ page }) => {
  await openScene(page, "mystats-closed");
  expect(await route(page)).toEqual({ name: "mystats" });
  await expect(tab(page, "My Stats")).toHaveClass(/\bon\b/);
  // From there the Live tab shows the user's own card only (VALORANT is closed).
  await tab(page, "Live").click();
  await expect(page.locator(".live .card")).toHaveCount(1);

  for (const scene of ["lobby2", "pregame", "ingame"]) {
    await openScene(page, scene);
    expect(await route(page), scene).toEqual({ name: "live" });
  }
  await openScene(page, "closed"); // the Live scenes that name their route keep it
  expect(await route(page)).toEqual({ name: "live" });
});

const applyLive = (page: Page, state: object) =>
  fromApp<void>(page, `import("/src/lib/stores.ts").then(({ applyLive }) => applyLive(${JSON.stringify(state)}))`);

test("the start page waits for Rust's first verdict: an unsettled closed state keeps Live, the first settled one decides", async ({ page }) => {
  // VALORANT closed, verdict made: My Stats (the unsettled `closed` before it changed nothing).
  await openScene(page, "closed-unsettled");
  expect(await route(page)).toEqual({ name: "live" });
  await expect(page.locator(".live")).toHaveCount(1);
  await expect(page.locator(".mystats")).toHaveCount(0);
  await applyLive(page, { stage: "closed", mapId: null, players: [], settled: false });
  expect(await route(page), "still unsettled").toEqual({ name: "live" });
  await applyLive(page, { stage: "closed", mapId: null, players: [], settled: true });
  expect(await route(page)).toEqual({ name: "mystats" });
  await expect(page.locator(".mystats")).toHaveCount(1);

  // VALORANT already running: the unsettled closed state comes first, the Lobby is the verdict. Never My Stats.
  await openScene(page, "closed-unsettled");
  await applyLive(page, { stage: "menus", mapId: null, players: [], settled: true });
  expect(await route(page)).toEqual({ name: "live" });
  await applyLive(page, { stage: "closed", mapId: null, players: [], settled: true }); // the game quits later: once only
  expect(await route(page)).toEqual({ name: "live" });
  await expect(page.locator(".mystats")).toHaveCount(0);

  // A user who went to My Stats before the verdict stays there whatever it says.
  await openScene(page, "closed-unsettled");
  await tab(page, "My Stats").click();
  await applyLive(page, { stage: "menus", mapId: null, players: [], settled: true });
  expect(await route(page)).toEqual({ name: "mystats" });
});

test("Back from a Match returns My Stats to the scroll position it was left at, and the Match starts at the top", async ({ page }) => {
  await openScene(page, "mystats");
  await page.locator("#app .scroll").evaluate((el) => (el.scrollTop = 300));
  const row = page.locator(".mystats .last5 .rrow").nth(2);
  await row.scrollIntoViewIfNeeded();
  const y = await scrollTop(page);
  expect(y, "the page was scrolled").toBeGreaterThan(100);
  await row.click({ position: { x: 20, y: 20 } });
  await waitPage(page, "loaded");
  await expect(page.locator(".match")).toHaveCount(1);
  expect(await scrollTop(page), "the Match starts at the top").toBe(0);
  await page.locator(".match .back").click();
  await expect(page.locator(".mystats")).toHaveCount(1);
  expect(await scrollTop(page)).toBe(y);
  // Leaving by the tab and coming back is a fresh visit: the top.
  await tab(page, "Live").click();
  await tab(page, "My Stats").click();
  expect(await scrollTop(page)).toBe(0);
});

test("Back from a Match returns Player Details to its scroll position, Show more kept", async ({ page }) => {
  await openScene(page, "details-more");
  await page.locator("#app .scroll").evaluate((el) => (el.scrollTop = 200));
  const row = page.locator(".details .recent .rrow").nth(5);
  await row.scrollIntoViewIfNeeded();
  const y = await scrollTop(page);
  expect(y, "the page was scrolled").toBeGreaterThan(100);
  await row.click({ position: { x: 20, y: 20 } });
  await waitPage(page, "loaded");
  expect(await scrollTop(page)).toBe(0);
  await page.locator(".match .back").click();
  await expect(page.locator(".details")).toHaveCount(1);
  expect(await route(page)).toMatchObject({ name: "details", more: true });
  await expect(page.locator(".details .recent .rrow")).toHaveCount(8);
  expect(await scrollTop(page)).toBe(y);
});

test("new data replaces the page in place: the scroll position stays, no skeleton in between", async ({ page }) => {
  await openScene(page, "mystats");
  await page.locator("#app .scroll").evaluate((el) => (el.scrollTop = 420));
  const next = myStatsFull();
  next.rank.netRr = 999;
  next.session = null;
  await page.evaluate(
    `import("/src/lib/stores.ts").then(({ mystats }) => mystats.set(${JSON.stringify(next)}))`,
  );
  await expect(page.locator(".mystats .rank .cells .cv").first()).toHaveText("+999");
  await expect(page.locator(".mystats .session .big")).toHaveText("—");
  expect(await scrollTop(page)).toBe(420);
  await expect(page.locator(".mystats")).toHaveAttribute("aria-busy", "false");
  await expect(page.locator(".mystats .sk")).toHaveCount(0);
  // The answer going back to "unknown" (null) keeps the frames, never an empty page.
  await page.evaluate(`import("/src/lib/stores.ts").then(({ mystats }) => mystats.set(null))`);
  await expect(page.locator(".mystats .sech")).toHaveCount(9);
  await expect(page.locator(".mystats .rank .tier")).toHaveText("—");
});

test("the first answer lands on the skeleton: nothing answered → shapes, then the data", async ({ page }) => {
  await openScene(page, "mystats-loading");
  await expect(page.locator(".mystats")).toHaveAttribute("aria-busy", "true");
  await page.evaluate(`import("/src/lib/stores.ts").then(({ mystats }) => mystats.set(${JSON.stringify(myStatsFull())}))`);
  await expect(page.locator(".mystats")).toHaveAttribute("aria-busy", "false");
  await expect(page.locator(".mystats .rank .tier")).toHaveText("Ascendant 1");
  await expect(page.locator(".mystats .sk")).toHaveCount(0);
});
