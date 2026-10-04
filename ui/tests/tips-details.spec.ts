// Every Details and Match tooltip of spec §4.2, hovered in turn, with the engine's placement rules (spec §4.1), plus
// the scroll rule: scrolling the Details/Match container hides an open tip ([C]). Playwright's fake clock is paused
// once the page has settled, so the 450ms delay is stepped with clock.runFor.
import { expect, test, type Locator, type Page } from "@playwright/test";
import { PUUID } from "../src/lib/demo/cdv8";
import { openScene, pointerAway } from "./harness";

const tip = (page: Page) => page.locator("#app [role=tooltip]");

type Item = { what: string; trigger: (p: Page) => Locator; text: string | null };

/**
 * Bring the trigger into view first and let that scroll's event land (a scroll hides tips), then hover it, step the
 * clock past 450ms and read the tip (null = no tip). Then check its placement against the trigger.
 */
async function hoverTip(page: Page, trigger: Locator): Promise<string | null> {
  await trigger.scrollIntoViewIfNeeded();
  await page.waitForTimeout(60);
  await trigger.hover();
  return readTip(page, trigger);
}

/** Step the clock past the delay and read the tip (null = none), checking its placement against `trigger`'s box. */
async function readTip(page: Page, trigger: Locator): Promise<string | null> {
  await page.clock.runFor(460);
  if ((await tip(page).count()) === 0) return null;
  const t = tip(page);
  const [tr, rr, app] = await Promise.all([trigger.boundingBox(), t.boundingBox(), page.locator("#app").boundingBox()]);
  if (!tr || !rr || !app) throw new Error("no box");
  expect(rr.x).toBeGreaterThanOrEqual(app.x);
  expect(rr.y).toBeGreaterThanOrEqual(app.y);
  expect(rr.x + rr.width).toBeLessThanOrEqual(app.x + app.width);
  expect(rr.y + rr.height).toBeLessThanOrEqual(app.y + app.height);
  const want = Math.max(136, Math.min(864, tr.x + tr.width / 2 - app.x));
  expect(Math.abs(rr.x + rr.width / 2 - app.x - want)).toBeLessThanOrEqual(1);
  if (tr.y + tr.height - app.y + 8 > 720) expect(Math.abs(rr.y + rr.height - (tr.y - app.y - 8))).toBeLessThanOrEqual(1);
  else expect(Math.abs(rr.y - (tr.y + tr.height - app.y + 8))).toBeLessThanOrEqual(1);
  return t.evaluate((el) => {
    const wl = el.querySelector(".wl");
    if (!wl) return el.textContent;
    return `${[...wl.children].map((s) => s.textContent).join(" ")}\n${el.querySelector(".scope")?.textContent ?? ""}`;
  });
}

async function runItems(page: Page, items: Item[]): Promise<void> {
  for (const it of items) {
    const trigger = it.trigger(page);
    await expect(trigger, it.what).toHaveCount(1);
    expect(await hoverTip(page, trigger), it.what).toBe(it.text);
    await pointerAway(page);
    await expect(tip(page), `${it.what}: cleared`).toHaveCount(0);
  }
}

const hero = (p: Page) => p.locator(".details .hero");
const hstat = (p: Page, i: number) => hero(p).locator(".hstat").nth(i);
const htile = (p: Page, i: number) => hero(p).locator(".hl5 .tile").nth(i);
const recent = (p: Page, i: number) => p.locator(".details .recent .rrow").nth(i);
const erow = (p: Page, i: number) => p.locator(".details .encs .erow").nth(i);
const mate = (p: Page, i: number) => p.locator(".details .mate").nth(i);
const wrow = (p: Page, i: number) => p.locator(".details .wrow").nth(i);

const THIS_ACT = "This act";
const HERO_SCOPE = "Last 20 competitive · this act"; // R116: Mara's heroMatches is 20

test("tips: details (Mara)", async ({ page }) => {
  await openScene(page, "details", {}, { clock: "manual" });
  await runItems(page, [
    { what: "hero name, not truncated", trigger: (p) => hero(p).locator(".nm"), text: null },
    { what: "hero ⟲", trigger: (p) => hero(p).locator(".enc"), text: "1 with · 2 against" },
    { what: "hero peak group", trigger: (p) => hero(p).locator(".hpeak"), text: "VALORANT 2026 · Act 4\nJun 24 – Aug 25, 2026" },
    { what: "hero current rank", trigger: (p) => hero(p).locator(".hrank"), text: null },
    { what: "hero prev", trigger: (p) => hero(p).locator(".hprev"), text: null },
    { what: "hero level", trigger: (p) => hero(p).locator(".hlv"), text: null },
    { what: "hero K/D (R116)", trigger: (p) => hstat(p, 0), text: HERO_SCOPE },
    { what: "hero HS% (R116)", trigger: (p) => hstat(p, 1), text: HERO_SCOPE },
    { what: "hero Win%", trigger: (p) => hstat(p, 2), text: `41W · 24L · 65 games\n${THIS_ACT}` },
    { what: "hero Games", trigger: (p) => hstat(p, 3), text: THIS_ACT },
    { what: "hero W / L", trigger: (p) => hstat(p, 4), text: THIS_ACT },
    { what: "hero last-5 tile 1 (oldest)", trigger: (p) => htile(p, 0), text: "−21 RR · 23h" },
    { what: "hero last-5 tile 2", trigger: (p) => htile(p, 1), text: "+17 RR · 22h" },
    { what: "hero last-5 tile 3", trigger: (p) => htile(p, 2), text: "−13 RR · 4h" },
    { what: "hero last-5 tile 4", trigger: (p) => htile(p, 3), text: "+16 RR · 2h" },
    { what: "hero last-5 tile 5 (newest)", trigger: (p) => htile(p, 4), text: "+18 RR · 45m" },
    { what: "recent time 0", trigger: (p) => recent(p, 0).locator(".ago"), text: "9:14 PM" },
    { what: "recent time 2", trigger: (p) => recent(p, 2).locator(".ago"), text: "5:20 PM" },
    { what: "recent rank icon", trigger: (p) => recent(p, 0).locator(".rrank"), text: "Immortal 1" },
    { what: "recent map name", trigger: (p) => recent(p, 0).locator(".mname"), text: null },
    { what: "recent agent", trigger: (p) => recent(p, 0).locator(".ragent"), text: null },
    { what: "recent score", trigger: (p) => recent(p, 0).locator(".rscore"), text: null },
    { what: "Show more", trigger: (p) => p.locator(".details .more"), text: null },
    { what: "encounter map cell", trigger: (p) => erow(p, 0).locator(".emap"), text: "Sep 12 · 11:18 PM · 41 min" },
    { what: "encounter map cell 3", trigger: (p) => erow(p, 2).locator(".emap"), text: "Aug 21 · 8:05 PM · 36 min" },
    { what: "encounter header name", trigger: (p) => p.locator(".details .encs .ehead .ename"), text: null },
    { what: "Top 3 live icons", trigger: (p) => mate(p, 0).locator(".mlive"), text: "In this match · Breach" },
    { what: "Top 3 name", trigger: (p) => mate(p, 1).locator(".nm"), text: null },
    { what: "Top 3 count", trigger: (p) => mate(p, 1).locator(".mcount"), text: null },
    { what: "loadout well (Vandal)", trigger: (p) => wrow(p, 0).locator(".well"), text: "Premium Edition" },
    { what: "loadout well (Operator)", trigger: (p) => wrow(p, 2).locator(".well"), text: "Ultra Edition" },
    { what: "loadout well (Sheriff)", trigger: (p) => wrow(p, 3).locator(".well"), text: "Exclusive Edition" },
    { what: "loadout skin line", trigger: (p) => wrow(p, 0).locator(".wskin"), text: null },
    { what: "Back", trigger: (p) => p.locator(".details .back"), text: null },
  ]);
});

test("tips: details-hidden, the hero name of a hidden agent", async ({ page }) => {
  await openScene(page, "details-hidden", {}, { clock: "manual" });
  await runItems(page, [{ what: "hidden agent", trigger: (p) => hero(p).locator(".nm"), text: "Hidden name" }]);
});

test("tips: details-hidden, the hero name of a remembered player", async ({ page }) => {
  await openScene(page, "details-hidden", { route: "details", puuid: PUUID.rook! }, { clock: "manual" });
  await runItems(page, [
    { what: "remembered", trigger: (p) => hero(p).locator(".nm"), text: "Hidden name · known from your match\nSplit · Aug 30, 2026" },
    { what: "remembered ⟲", trigger: (p) => hero(p).locator(".enc"), text: "0 with · 1 against" },
  ]);
});

test("tips: details-longnames, the hero name shows the full Riot ID only because it is cut", async ({ page }) => {
  await openScene(page, "details-longnames", {}, { clock: "manual" });
  await runItems(page, [{ what: "hero name, truncated", trigger: (p) => hero(p).locator(".nm"), text: "WWWWWWWWWWWWWWWW#WWWWW" }]);
});

test("tips: details-nostats, a — hero K/D or HS% has no tip; Games and W / L keep their scope; no duration", async ({ page }) => {
  await openScene(page, "details-nostats", {}, { clock: "manual" });
  await runItems(page, [
    { what: "hero K/D null", trigger: (p) => hstat(p, 0), text: null },
    { what: "hero HS% null", trigger: (p) => hstat(p, 1), text: null },
    { what: "hero Win% null (no games known)", trigger: (p) => hstat(p, 2), text: null },
    { what: "hero Games null", trigger: (p) => hstat(p, 3), text: THIS_ACT },
    { what: "hero W / L null", trigger: (p) => hstat(p, 4), text: THIS_ACT },
    { what: "hero peak null", trigger: (p) => hero(p).locator(".hpeak"), text: null },
    { what: "hero empty last-5 tile", trigger: (p) => htile(p, 4), text: null },
    { what: "recent rank, unknown", trigger: (p) => recent(p, 1).locator(".rrank"), text: null },
    { what: "recent rank, unranked (tier 0)", trigger: (p) => recent(p, 2).locator(".rrank"), text: null },
    { what: "encounter map cell, no duration", trigger: (p) => erow(p, 0).locator(".emap"), text: "Sep 12 · 11:18 PM" },
    { what: "loadout well, none", trigger: (p) => wrow(p, 0).locator(".well"), text: null },
  ]);
});

test("tips: details-partial, a hero value still loading has no tip", async ({ page }) => {
  await openScene(page, "details-partial", {}, { clock: "manual" });
  await runItems(page, [
    { what: "hero K/D known (from the details)", trigger: (p) => hstat(p, 0), text: HERO_SCOPE },
    { what: "hero Win% loading", trigger: (p) => hstat(p, 2), text: null },
    { what: "hero Games loading", trigger: (p) => hstat(p, 3), text: null },
    { what: "hero W / L loading", trigger: (p) => hstat(p, 4), text: null },
    { what: "hero name loading", trigger: (p) => hero(p).locator(".nm"), text: null },
    { what: "hero peak loading", trigger: (p) => hero(p).locator(".hpeak"), text: null },
  ]);
});

const srow = (p: Page, has: string) => p.locator(".match .srow").filter({ has: p.locator(has) });

test("tips: match", async ({ page }) => {
  await openScene(page, "match", {}, { clock: "manual" });
  const head = (p: Page, i: number) => p.locator(".match .shead > *").nth(i);
  await runItems(page, [
    { what: "halves", trigger: (p) => p.locator(".match .mhalves"), text: "First half · Second half" },
    { what: "header score", trigger: (p) => p.locator(".match .mscore"), text: null },
    { what: "header chip", trigger: (p) => p.locator(".match .mhead .chip"), text: null },
    { what: "when", trigger: (p) => p.locator(".match .mwhen"), text: null },
    { what: "Player header", trigger: (p) => head(p, 0), text: null },
    { what: "ACS header", trigger: (p) => head(p, 1), text: "Average combat score" },
    { what: "K / D / A header", trigger: (p) => head(p, 2), text: null },
    { what: "+/− header", trigger: (p) => head(p, 3), text: "Kills minus deaths" },
    { what: "ADR header", trigger: (p) => head(p, 4), text: "Average damage per round" },
    { what: "HS% header", trigger: (p) => head(p, 5), text: null },
    { what: "scoreboard rank (focus row)", trigger: (p) => srow(p, ".focusbar").locator(".srank"), text: "Immortal 1" },
    { what: "scoreboard rank, bottom row flips above", trigger: (p) => p.locator(".match .srow").last().locator(".srank"), text: "Ascendant 2" },
    { what: "scoreboard name", trigger: (p) => srow(p, ".focusbar").locator(".nm"), text: null },
    { what: "scoreboard +/−", trigger: (p) => srow(p, ".focusbar").locator(".pm"), text: null },
    { what: "team header", trigger: (p) => p.locator(".match .thead").first(), text: null },
  ]);
});

test("tips: match-dm, a row with no rank has no rank tip", async ({ page }) => {
  await openScene(page, "match-dm", {}, { clock: "manual" });
  await runItems(page, [{ what: "no rank", trigger: (p) => srow(p, ".focusbar").locator(".srank"), text: null }]);
});

test("scrolling the Details container hides an open tip", async ({ page }) => {
  await openScene(page, "details-more", {}, { clock: "manual" });
  const trigger = recent(page, 0).locator(".ago");
  await trigger.hover();
  await page.clock.runFor(460);
  await expect(tip(page)).toHaveText("9:14 PM");
  await page.locator("#app .scroll").evaluate((el) => (el.scrollTop += 40));
  await expect(tip(page)).toHaveCount(0);
});

// ── keyboard focus on a real focusable (spec §4.1) ───────────────────────────────────────────────
// No control in the app is both focusable and a tip trigger yet (the My Stats chart's tip belongs to its inner anchor box),
// so the engine is run on real ones here by giving them a `data-tip`, the one attribute the `tip` action sets.

test("keyboard: a focusable trigger shows its tip on Tab (same delay and placement), and Tab away, Escape and a click hide it", async ({ page }) => {
  await openScene(page, "details", {}, { clock: "manual" });
  const back = page.locator(".details .back");
  await back.evaluate((el) => el.setAttribute("data-tip", "Back to the grid"));
  await page.keyboard.press("Tab"); // the Live tab
  await page.keyboard.press("Tab"); // My Stats
  await page.keyboard.press("Tab"); // Back
  await expect(back).toBeFocused();
  await page.clock.runFor(440);
  await expect(tip(page)).toHaveCount(0); // not before 450 ms
  await page.clock.runFor(20);
  await expect(tip(page)).toHaveText("Back to the grid");
  await expect(tip(page)).toHaveAttribute("id", "tooltip");
  await expect(back).toHaveAttribute("aria-describedby", "tooltip");
  expect(await readTip(page, back)).toBe("Back to the grid"); // placed like a hover's: centred under the trigger

  // Escape: gone, with its description, and nothing else happens (still on the page, still focused).
  await page.keyboard.press("Escape");
  await expect(tip(page)).toHaveCount(0);
  await expect(back).not.toHaveAttribute("aria-describedby", /.+/);
  await expect(back).toBeFocused();
  await expect(page.locator(".details")).toBeVisible();

  // Tab away and back: it shows again; moving on hides it at once.
  await page.keyboard.press("Tab");
  await page.keyboard.press("Shift+Tab");
  await expect(back).toBeFocused();
  expect(await readTip(page, back)).toBe("Back to the grid");
  await page.keyboard.press("Tab");
  await expect(back).not.toBeFocused();
  await expect(tip(page)).toHaveCount(0);
  await expect(back).not.toHaveAttribute("aria-describedby", /.+/);

  // Enter on it clicks it (Back): the tip goes with the click.
  await page.keyboard.press("Shift+Tab");
  await expect(back).toBeFocused();
  await page.clock.runFor(460);
  await expect(tip(page)).toHaveCount(1);
  await page.keyboard.press("Enter");
  await expect(tip(page)).toHaveCount(0);
});

test("keyboard: a mouse-focused element opens no tip through its focus, and has no :focus-visible", async ({ page }) => {
  await openScene(page, "details", {}, { clock: "manual" });
  const hero = page.locator(".details .hero");
  await hero.evaluate((el) => {
    el.setAttribute("data-tip", "Hero");
    el.setAttribute("tabindex", "-1");
  });
  // Press on it (focus), then move off before the delay ends: the hover's tip is cancelled by the move, the focus owes none.
  const box = (await hero.boundingBox())!;
  await page.mouse.move(box.x + box.width - 20, box.y + 8);
  await page.mouse.down();
  await expect(hero).toBeFocused();
  expect(await hero.evaluate((el) => el.matches(":focus-visible"))).toBe(false);
  await page.mouse.up();
  await page.mouse.move(5, 5);
  await page.clock.runFor(1000);
  await expect(tip(page)).toHaveCount(0);
  await expect(hero).not.toHaveAttribute("aria-describedby", /.+/);
});

test("keyboard: focusing a trigger below the fold scrolls it into view, and that scroll does not cancel its tip", async ({ page }) => {
  await openScene(page, "details-more", {}, { clock: "manual" });
  await page.keyboard.press("Tab"); // keyboard modality: the script focus below is a :focus-visible one
  const last = page.locator(".details .encs .erow").last(); // the Encounters section starts below the fold
  await last.evaluate((el) => el.setAttribute("data-tip", "Oldest encounter"));
  const scroller = page.locator("#app .scroll");
  expect(await scroller.evaluate((el) => el.scrollTop)).toBe(0);
  // The browser delivers the scroll event a frame after the focus. Listen first (this call has returned before the
  // focus is sent) and wait for the event, so the clock only moves once "the scroll came first" is a fact, not a race.
  await scroller.evaluate((el) => {
    const w = window as unknown as { __scrolled: boolean };
    w.__scrolled = false;
    el.addEventListener("scroll", () => (w.__scrolled = true), { once: true });
  });
  await last.focus();
  await expect(last).toBeFocused();
  await expect.poll(() => page.evaluate(() => (window as unknown as { __scrolled: boolean }).__scrolled)).toBe(true);
  expect(await scroller.evaluate((el) => el.scrollTop)).toBeGreaterThan(0); // the focus scrolled the page
  expect(await readTip(page, last)).toBe("Oldest encounter");
  // A scroll after it is up hides it, as for any tip.
  await scroller.evaluate((el) => (el.scrollTop -= 30));
  await expect(tip(page)).toHaveCount(0);
});
