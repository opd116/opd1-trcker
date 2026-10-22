// The v2 Live screen (design v2 .superpowers/design/v2/TrackerApp.dc.html 29–447) against the DEV demo feed: the
// board's sizes at both window sizes, Agent Select, loading, the hidden-name looks, keyboard and tooltips, the waiting,
// error, lost and recap views, and glance mode. The page clock is pinned at the demo's NOW (harness), so the demo's
// countdowns read as the design's (0:42, 1:24, 42 s, 8S).
import { expect, test, type Page } from "@playwright/test";
import { openScene, pointerAway } from "./harness";

const box = async (page: Page, sel: string) => (await page.locator(sel).first().boundingBox())!;
const tipText = (page: Page) => page.locator("#tooltip").innerText();

for (const [w, h] of [
  [1280, 800],
  [1180, 760],
] as const) {
  test(`in game at ${w}×${h}: header, two teams of five 56px rows, no sideways overflow`, async ({ page }) => {
    await page.setViewportSize({ width: w, height: h });
    await openScene(page, "ingame");
    const head = await box(page, "main .live > .head");
    expect(head.height).toBe(62);
    expect(head.x).toBe(76 + 24);
    const teams = page.locator("main section.team");
    await expect(teams).toHaveCount(2);
    await expect(teams.nth(0)).toHaveAttribute("aria-label", "Your team");
    const rows = page.locator("main [data-row]");
    await expect(rows).toHaveCount(10);
    for (let i = 0; i < 10; i++) expect(Math.round((await rows.nth(i).boundingBox())!.height)).toBe(56);
    // The user's row comes first and carries YOU.
    await expect(rows.nth(0).locator(".you")).toHaveText("YOU");
    const sw = await page.evaluate(() => document.documentElement.scrollWidth);
    expect(sw).toBe(w);
    // Nothing but a name, a note or a rank name may be cut off sideways.
    const cut = await page.evaluate(() =>
      [...document.querySelectorAll<HTMLElement>("main .live *")]
        .filter((e) => e.scrollWidth > e.clientWidth + 1 && getComputedStyle(e).overflowX !== "visible")
        // An <img> still decoding can report its alt text's width for a moment: not layout.
        .filter((e) => !e.matches(".n, .note-t, .rank-name, .text, .tables, img"))
        .map((e) => `${e.tagName}.${e.className} in ${e.parentElement?.className} (${e.scrollWidth}>${e.clientWidth})`),
    );
    expect(cut).toEqual([]);
  });
}

test("agent select: chip, countdown, lock count, your team only, the enemy placeholder, picking tiles", async ({ page }) => {
  await openScene(page, "pregame");
  await expect(page.locator(".head .chip")).toHaveText("AGENT SELECT");
  await expect(page.locator(".head .timer")).toHaveText("0:42");
  // The lock count agrees with the rows: 5 allies, 3 of them "Picking…" (a hover, an undecided ally, a hidden player).
  await expect(page.locator(".head .locked")).toHaveText("2 OF 5 LOCKED");
  await expect(page.locator("main section.team")).toHaveCount(1);
  await expect(page.locator(".enemy-ph .ph-label")).toHaveText("ENEMY TEAM");
  await expect(page.locator("main [data-row] .agent .q")).toHaveCount(3);
  await expect(page.locator("main [data-row] .n.hidden")).toHaveText(["Hidden player"]);
  // The enemy side of the matchup is unknown.
  await expect(page.locator(".matchup .side").nth(1).locator(".wr")).toHaveText("?");
});

test("loading: LOADING PLAYERS 0 / 10 and ten skeleton rows", async ({ page }) => {
  await openScene(page, "ingame-loading");
  await expect(page.locator(".head .loading .ld-n")).toHaveText("0 / 10");
  await expect(page.locator("main .row.skel")).toHaveCount(10);
  await expect(page.locator("main [data-row]")).toHaveCount(0);
});

test("partial cards: a loading cell shimmers, the rest of the row is drawn", async ({ page }) => {
  await openScene(page, "ingame-partial");
  await expect(page.locator(".head .loading .ld-n")).toHaveText(/\d \/ 10/);
  await expect(page.locator("main [data-row] .sk").first()).toBeVisible();
});

test("hidden names: the agent name in italic without a tag; a remembered name is dotted with its source on hover", async ({ page }) => {
  await openScene(page, "ingame", {}, { clock: "manual" });
  const hidden = page.locator("main [data-row] .n.hidden");
  await expect(hidden).toHaveText(["Cypher"]);
  expect(await hidden.evaluate((e) => getComputedStyle(e).fontStyle)).toBe("italic");
  await expect(hidden.locator("xpath=following-sibling::span")).toHaveCount(0);
  const rem = page.locator("main [data-row] .n.remembered").first();
  expect(await rem.evaluate((e) => getComputedStyle(e).textDecorationStyle)).toBe("dotted");
  await rem.hover();
  await page.clock.runFor(500);
  expect(await tipText(page)).toMatch(/^Hidden name · known from/);
  await pointerAway(page);
});

test("tooltips: peak (with its tier image), played before (key/value), party, win/loss bar", async ({ page }) => {
  await openScene(page, "ingame", {}, { clock: "manual" });
  const hover = async (sel: string) => {
    await pointerAway(page);
    await page.clock.runFor(100);
    await page.locator(sel).first().hover();
    await page.clock.runFor(500);
  };
  await hover("main [data-row] .peak");
  await expect(page.locator("#tooltip img")).toHaveCount(1);
  expect(await tipText(page)).toMatch(/1mo ago|This act/);
  await hover("main [data-row] .met");
  expect(await tipText(page)).toMatch(/^Played before\nWith you\n\d+\nAgainst you\n\d+$/);
  await hover("main [data-row] .gutter:has(.pdot)");
  expect(await tipText(page)).toMatch(/^(Probably q|Q)ueued together\n/);
  await hover("main [data-row] .wr-cell .big");
  expect(await tipText(page)).toMatch(/^\d+W\n\d+L$/);
});

test("keyboard: Tab reaches the rows, Enter opens the profile and keeps the row marked", async ({ page }) => {
  await openScene(page, "ingame");
  const rows = page.locator("main [data-row]");
  await rows.nth(0).focus();
  await page.keyboard.press("Tab");
  await expect(rows.nth(1)).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(rows.nth(1)).toHaveClass(/\bsel\b/);
  await page.keyboard.press("Escape");
  await expect(rows.nth(1)).toHaveClass(/\bsel\b/);
});

test("waiting (closed): copy, checks, Launch VALORANT; last match, today and rank cards", async ({ page }) => {
  await openScene(page, "closed");
  await expect(page.locator(".wait h1")).toHaveText("Waiting for VALORANT");
  await expect(page.locator(".wait .cval")).toHaveText(["RUNNING · 98.0.1", "NOT RUNNING", "WAITING FOR GAME"]);
  await expect(page.locator(".wait .primary")).toHaveText("Launch VALORANT");
  await expect(page.locator(".wait .lm-cap")).toHaveText(/^LAST MATCH · \d+ (MIN|H|D) AGO$/);
  await expect(page.locator(".wait .rk-name")).toBeVisible();
  // The last match opens its scoreboard on the History screen.
  await page.locator(".wait .lm-open").click();
  await expect(page.locator('nav.rail button[aria-label="History"]')).toHaveAttribute("aria-current", "page");
});

test("waiting (menus): the party and the queue timer", async ({ page }) => {
  await openScene(page, "lobby3", { queue: "1" });
  await expect(page.locator(".wait h1")).toHaveText("In the lobby");
  await expect(page.locator(".wait .s-time")).toHaveText("1:24");
  await expect(page.locator(".wait .party .member")).toHaveCount(3);
  await expect(page.locator(".wait .cap .n").first()).toHaveText("3 / 5");
});

test("error: code, lockfile path, Retry now and the auto-retry countdown; no folder picker", async ({ page }) => {
  await openScene(page, "closed", { variant: "error" });
  await expect(page.locator(".err h1")).toHaveText("Can’t reach the Riot Client");
  await expect(page.locator(".err .c")).toHaveText("LOCKFILE_NOT_FOUND");
  await expect(page.locator(".err .path")).toContainText("lockfile");
  await expect(page.locator(".err .auto")).toHaveText("AUTO-RETRY IN 8S");
  await expect(page.locator(".err button")).toHaveText(["Retry now"]);
});

test("lost: the banner over the last data", async ({ page }) => {
  await openScene(page, "ingame", { variant: "lost" });
  await expect(page.locator(".lost .text")).toHaveText("Showing data from 42 s ago. Reconnecting, attempt 2 of 5.");
  await expect(page.locator("main [data-row]")).toHaveCount(10);
});

test("recap: hero, both teams by ACS with bookmarks, the full scoreboard", async ({ page }) => {
  await openScene(page, "closed", { recap: "1" });
  await expect(page.locator(".recap .word")).toHaveText(/VICTORY|DEFEAT|DRAW/);
  await expect(page.locator(".recap .team")).toHaveCount(2);
  const acs = await page.locator(".recap .team").first().locator(".t-acs").allInnerTexts();
  const nums = acs.map(Number).filter((n) => !Number.isNaN(n));
  expect(nums).toEqual([...nums].sort((a, b) => b - a));
  await expect(page.locator(".recap .save").first()).toBeVisible();
  await page.locator(".recap .primary").click();
  await expect(page.locator('nav.rail button[aria-label="History"]')).toHaveAttribute("aria-current", "page");
});

test("glance: enemy team first, 52px rows, the saved-player callout", async ({ page }) => {
  await page.setViewportSize({ width: 420, height: 800 });
  await openScene(page, "ingame", { glance: "1" });
  const teams = page.locator(".glance-body section.team");
  await expect(teams).toHaveCount(2);
  await expect(teams.nth(0)).toHaveAttribute("aria-label", "Enemy team");
  const row = page.locator(".glance-body [data-row]").first();
  expect(Math.round((await row.boundingBox())!.height)).toBe(52);
  await expect(page.locator(".glance-body .callout .c-head")).toHaveText(/ is on (the enemy|your) team$/);
  const sw = await page.evaluate(() => document.documentElement.scrollWidth);
  expect(sw).toBe(420);
});
