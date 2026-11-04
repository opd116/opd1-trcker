// The tooltip engine's behaviour on the v2 screens (ported from the v8 tips-live / tips-details / interaction specs;
// the tip texts themselves are checked by live-v2, profile, saved-history and the unit tests): the 450 ms delay, a
// click hiding an open tip, keyboard focus showing a trigger's tip (with aria-describedby) and Escape / Tab hiding it,
// a mouse focus never opening one, and a scroll hiding an open tip but not a keyboard one that the focus itself
// scrolled into view. The page clock is Playwright's fake one, so no timing here depends on real timers.
import { expect, test, type Locator, type Page } from "@playwright/test";
import { openScene, pointerAway } from "./harness";
test.use({ viewport: { width: 700, height: 860 } });

const tip = (page: Page) => page.locator("#tooltip");
const editNote = (page: Page) => page.getByRole("button", { name: "Edit note on Mara", exact: true });
const addNote = (page: Page) => page.getByRole("button", { name: "Add a note on Kilo", exact: true });

/** The tip's text, and that it is placed like a hover's: centred on the trigger, 8px above or below it. */
async function readTip(page: Page, trigger: Locator): Promise<string> {
  await expect(tip(page)).toHaveCSS("opacity", "1");
  const [t, b] = [(await tip(page).boundingBox())!, (await trigger.boundingBox())!];
  const vw = page.viewportSize()!.width;
  const centred = Math.abs(t.x + t.width / 2 - (b.x + b.width / 2)) <= 1 || t.x <= 6.5 || t.x + t.width >= vw - 6.5;
  expect(centred, "centred on its trigger (or clamped to the window)").toBe(true);
  const gap = t.y >= b.y + b.height ? t.y - (b.y + b.height) : b.y - (t.y + t.height);
  expect(Math.abs(gap - 8)).toBeLessThanOrEqual(1);
  return tip(page).innerText();
}

test("a hover's tip shows at 450 ms, not at 400 ms; leaving the trigger hides it at once", async ({ page }) => {
  await openScene(page, "ingame", {}, { clock: "manual" });
  const peak = page.locator("main [data-row] .peak-gap").first();
  await peak.hover();
  await page.clock.runFor(400);
  await expect(tip(page)).toHaveCount(0);
  await page.clock.runFor(60);
  await expect(tip(page)).toHaveCount(1);
  expect(await readTip(page, peak)).toMatch(/\d{4}$/);
  await pointerAway(page);
  await expect(tip(page)).toHaveCount(0);
});

test("a click while a tip is open hides it", async ({ page }) => {
  await openScene(page, "ingame", {}, { clock: "manual" });
  const col = page.locator("main .metric.combat").first();
  await col.hover();
  await page.clock.runFor(460);
  await expect(tip(page)).toHaveCount(1);
  await page.mouse.down();
  await page.mouse.up();
  await expect(tip(page)).toHaveCount(0);
});

test("keyboard: a focusable trigger shows its tip on Tab (same delay and placement); Escape, Tab away and Enter hide it", async ({ page }) => {
  await openScene(page, "ingame", { screen: "saved" }, { clock: "manual" });
  const btn = editNote(page);
  await page.keyboard.press("Tab");
  await btn.focus();
  await expect(btn).toBeFocused();
  expect(await btn.evaluate((el) => el.matches(":focus-visible"))).toBe(true);
  await page.clock.runFor(440);
  await expect(tip(page)).toHaveCount(0); // not before 450 ms
  await page.clock.runFor(20);
  expect(await readTip(page, btn)).toBe("Edit note on Mara");
  await expect(tip(page)).toHaveAttribute("id", "tooltip");
  await expect(btn).toHaveAttribute("aria-describedby", "tooltip");

  // Escape: gone, with its description, and nothing else happens (still focused, the editor stays closed).
  await page.keyboard.press("Escape");
  await expect(tip(page)).toHaveCount(0);
  await expect(btn).not.toHaveAttribute("aria-describedby", /.+/);
  await expect(btn).toBeFocused();
  await expect(page.getByRole("textbox", { name: "Edit note on Mara" })).toHaveCount(0);

  // Tab away: the next trigger's tip replaces it, and the old one's description goes.
  await addNote(page).focus();
  await page.clock.runFor(460);
  expect(await readTip(page, addNote(page))).toBe("Add a note on Kilo");
  await expect(btn).not.toHaveAttribute("aria-describedby", /.+/);
  await btn.focus();
  await expect(addNote(page)).not.toHaveAttribute("aria-describedby", /.+/);
  await page.clock.runFor(460);
  await expect(tip(page)).toHaveCount(1);

  // Enter on the note button clicks it: the tip goes with the click and the editor opens.
  await btn.focus();
  await page.clock.runFor(460);
  await expect(tip(page)).toHaveCount(1);
  await page.keyboard.press("Enter");
  await expect(tip(page)).toHaveCount(0);
  await expect(page.getByRole("textbox", { name: "Edit note on Mara" })).toBeFocused();
});

test("keyboard: a mouse-focused trigger opens no tip through its focus, and has no :focus-visible", async ({ page }) => {
  await openScene(page, "ingame", { screen: "saved" }, { clock: "manual" });
  const btn = editNote(page);
  const box = (await btn.boundingBox())!;
  // Press on it (focus), then move off before the delay ends: the hover's tip is cancelled by the move, the focus owes none.
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(5, 5);
  await page.mouse.up();
  await page.clock.runFor(1000);
  await expect(tip(page)).toHaveCount(0);
  await expect(btn).not.toHaveAttribute("aria-describedby", /.+/);
  // The press happened on the button and the release elsewhere: it focuses it but never clicks it open.
  if (await btn.evaluate((el) => el === document.activeElement)) expect(await btn.evaluate((el) => el.matches(":focus-visible"))).toBe(false);
  await expect(page.getByRole("textbox", { name: "Edit note on Mara" })).toHaveCount(0);
});

test("scrolling a list hides an open hover tip", async ({ page }) => {
  await openScene(page, "closed", { screen: "history" }, { clock: "manual" });
  const when = page.locator("main .row[data-row] .when").first();
  await when.hover();
  await page.clock.runFor(460);
  await expect(tip(page)).toHaveCount(1);
  await page.locator("main .card").first().evaluate((el) => (el.scrollTop += 40));
  await expect(tip(page)).toHaveCount(0);
});

test("keyboard: focusing a trigger below the fold scrolls it into view, and that scroll does not cancel its tip", async ({ page }) => {
  await openScene(page, "closed", { screen: "history" }, { clock: "manual" });
  await page.keyboard.press("Tab"); // keyboard modality: the script focus below is a :focus-visible one
  const last = page.locator("main .row[data-row]").last();
  await last.evaluate((el) => el.setAttribute("data-tip", "Oldest match"));
  const scroller = page.locator("main .card").first();
  expect(await scroller.evaluate((el) => el.scrollHeight > el.clientHeight && el.scrollTop === 0)).toBe(true);
  // The browser delivers the scroll event a frame after the focus. Listen first and wait for the event, so the clock
  // only moves once "the scroll came first" is a fact, not a race.
  await scroller.evaluate((el) => {
    const w = window as unknown as { __scrolled: boolean };
    w.__scrolled = false;
    el.addEventListener("scroll", () => (w.__scrolled = true), { once: true });
  });
  await last.focus();
  await expect(last).toBeFocused();
  await expect.poll(() => page.evaluate(() => (window as unknown as { __scrolled: boolean }).__scrolled)).toBe(true);
  expect(await scroller.evaluate((el) => el.scrollTop)).toBeGreaterThan(0);
  await page.clock.runFor(460);
  expect(await readTip(page, last)).toBe("Oldest match");
  // A scroll after it is up hides it, as for any tip.
  await scroller.evaluate((el) => (el.scrollTop -= 30));
  await expect(tip(page)).toHaveCount(0);
});
