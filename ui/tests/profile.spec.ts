// The v2 profile drawer (design TrackerApp.dc.html 949–1128) against the DEV demo feed: the dialog's name and focus,
// Escape order (note popover first, then the drawer), focus return, the save + note flow, the self / other variants,
// the hidden and empty edge scenes, and the encounter → scoreboard hand-off. 1280×800 (playwright.config.ts).
import { expect, test, type Page } from "@playwright/test";
import { PUUID } from "../src/lib/demo/cdv8";
import { checkText } from "./checks";
import { gotoApp, routeMedia, screenNav } from "./harness";
test.use({ viewport: { width: 700, height: 860 } });

const MARA = PUUID.mara!;
const FAHAD = PUUID.fahad!;
const ZEPHYR = PUUID.zephyr!;

const dialog = (page: Page) => page.getByRole("dialog");

/** A drawer row opened its scoreboard: History is the screen, the board loaded, the user's own row is there, Back names the player. */
async function expectScoreboard(page: Page, back: string): Promise<void> {
  await expect(screenNav(page, "History")).toHaveClass(/active/);
  await expect(page.locator('main .match[aria-busy="false"]')).toHaveCount(1);
  await expect(page.locator("main .match .srow.self")).toHaveCount(1);
  await expect(page.locator("main .match .back")).toHaveText(back);
}

async function open(page: Page, query: string): Promise<void> {
  await routeMedia(page);
  await gotoApp(page, `/?${query}`);
  await expect(dialog(page)).toBeVisible();
}

test("another player: name, header actions, encounters, loadout, maps, recent rows", async ({ page }) => {
  await open(page, `scene=ingame&profile=${MARA}`);
  const d = dialog(page);
  await expect(d).toHaveAttribute("aria-label", "Mara profile");
  await expect(d).toHaveAttribute("aria-modal", "true");
  await expect(d).toBeFocused();
  await expect(d.getByText("#7788")).toBeVisible();
  await expect(d.getByRole("button", { name: "Saved" })).toHaveAttribute("aria-pressed", "true");
  await expect(d.getByRole("button", { name: "Competitive stats" })).toHaveCount(0);
  await expect(d.getByText("CURRENT RANK")).toBeVisible();
  for (const l of ["K/D", "Headshot %", "Win rate", "Record", "Avg ACS"]) await expect(d.getByText(l, { exact: true })).toBeVisible();
  await expect(d.getByText("Net RR", { exact: true })).toHaveCount(0);
  await expect(d.getByRole("button", { name: /Open scoreboard$/ }).first()).toBeVisible();
  await expect(d.getByText("LOADOUT")).toBeVisible();
  await expect(d.getByText("Reaver Vandal")).toBeVisible();
  await expect(d.getByText("RECENT MATCHES")).toBeVisible();
  // Another player's recent rows are not links: the only row buttons are the encounters.
  const encounters = await d.getByRole("button", { name: /Open scoreboard$/ }).count();
  expect(encounters).toBe(3);
  // Nothing scrolls sideways.
  const overflow = await d.evaluate((el) => (el.lastElementChild as HTMLElement).scrollWidth - (el.lastElementChild as HTMLElement).clientWidth);
  expect(overflow).toBeLessThanOrEqual(0);
});

/**
 * The recent-match map label must fit its own ink vertically even when the `data-ellipsis` marker the fit checks honour
 * is taken off the rendered span (R205 regression: the room comes from the label's line-height, not from the marker),
 * and a deliberately long name must elide horizontally instead of overflowing its column. Restores the marker.
 */
async function expectRecentMapLabelFits(page: Page, w: number, h: number, dpr1 = false): Promise<void> {
  await page.setViewportSize({ width: w, height: h });
  await open(page, "scene=details-more");
  const d = dialog(page);
  await expect(d).toHaveAttribute("aria-busy", "false");
  const names = d.locator(".rm .mn");
  await expect(names.first()).toBeVisible();
  await page.evaluate(() => document.querySelectorAll<HTMLElement>(".rm .mn").forEach((el) => el.removeAttribute("data-ellipsis")));
  const clip = (await checkText(page)).clip.filter((m) => m.includes("span.mn"));
  const probe = names.first();
  const original = (await probe.textContent()) ?? "";
  await probe.evaluate((el) => {
    el.textContent = "Ascent".repeat(10);
  });
  const long = await probe.evaluate((el) => ({ elides: getComputedStyle(el).textOverflow === "ellipsis", cut: el.scrollWidth > el.clientWidth }));
  const page_ = await page.evaluate(() => ({ sw: document.documentElement.scrollWidth, cw: document.documentElement.clientWidth }));
  await probe.evaluate((el, t) => {
    el.textContent = t as string;
  }, original);
  await page.evaluate(() => document.querySelectorAll<HTMLElement>(".rm .mn").forEach((el) => el.setAttribute("data-ellipsis", "")));
  const where = `${w}×${h}${dpr1 ? " DPR 1.5" : ""}`;
  expect(clip, `${where}: map label is cut vertically without the marker`).toEqual([]);
  expect(long, `${where}: a long map name elides`).toEqual({ elides: true, cut: true });
  expect(page_.sw, `${where}: the long name never widens the page`).toBe(page_.cw);
}

test("recent-match map labels fit their ink without the truncation marker (line-height, not data-ellipsis)", async ({ page }) => {
  for (const [w, h] of [
    [700, 860],
    [680, 760],
  ] as const)
    await expectRecentMapLabelFits(page, w, h);
});

test("recent-match map labels fit their ink at 680×760, DPR 1.5", async ({ page }) => {
  await expectRecentMapLabelFits(page, 680, 760, true);
});

test("the user: personal notes, Competitive stats, Net RR, clickable recent rows", async ({ page }) => {
  await open(page, `scene=ingame&profile=${FAHAD}`);
  const d = dialog(page);
  await expect(d).toHaveAttribute("aria-label", "Fahad profile");
  await expect(d.getByText("YOU", { exact: true })).toBeVisible();
  await expect(d.getByRole("button", { name: /Save player|Saved/ })).toHaveCount(0);
  await expect(d.getByText("Net RR", { exact: true })).toBeVisible();
  await expect(d.getByRole("textbox", { name: "Personal notes" })).toBeVisible();
  await d.getByRole("textbox", { name: "Personal notes" }).fill("Warm up first");
  await expect(d.getByRole("textbox", { name: "Personal notes" })).toHaveValue("Warm up first");
  const rows = d.getByRole("button", { name: /Open scoreboard$/ });
  await expect(rows).toHaveCount(6);
  await rows.first().click();
  await expect(dialog(page)).toHaveCount(0);
  await expectScoreboard(page, "Fahad");
});

test("save + note popover: a new save's × cancels it; Escape closes the popover before the drawer", async ({ page }) => {
  await open(page, `scene=ingame&profile=${ZEPHYR}`);
  const d = dialog(page);
  const save = d.getByRole("button", { name: "Save player" });
  await save.click();
  const note = d.getByRole("textbox", { name: "Note on Zephyr" });
  await expect(note).toBeFocused();
  await expect(d.getByRole("button", { name: "Saved" })).toHaveAttribute("aria-expanded", "true");
  await d.getByRole("button", { name: "Cancel" }).click();
  await expect(d.getByRole("button", { name: "Save player" })).toHaveAttribute("aria-pressed", "false");

  // Saved again, with a note: Escape closes the popover only, and focus goes back to the button.
  await d.getByRole("button", { name: "Save player" }).click();
  await d.getByRole("textbox", { name: "Note on Zephyr" }).fill("Lurks B");
  await page.keyboard.press("Escape");
  await expect(d.getByRole("textbox", { name: "Note on Zephyr" })).toHaveCount(0);
  await expect(d.getByRole("button", { name: "Saved" })).toBeFocused();
  // The popover opened on a saved player offers "Remove from saved".
  await d.getByRole("button", { name: "Saved" }).click();
  await expect(d.getByRole("textbox", { name: "Note on Zephyr" })).toHaveValue("Lurks B");
  await expect(d.getByRole("button", { name: "Remove from saved" })).toBeVisible();
  await page.keyboard.press("Escape");
  await page.keyboard.press("Escape");
  await expect(dialog(page)).toHaveCount(0);
});

test("a new save's Cancel puts back the note the player already had, instead of deleting it", async ({ page }) => {
  await open(page, `scene=ingame&profile=${MARA}`);
  const d = dialog(page);
  const OLD = "Entry every round, dry-peeks A long";
  const stored = () => page.evaluate(`import("/src/lib/demo/v2/store.ts").then(({ state }) => state.marks.notes[${JSON.stringify(MARA)}] ?? null)`);
  // Removed from saved: the note stays stored (as Rust keeps it).
  await d.getByRole("button", { name: "Saved" }).click();
  await d.getByRole("button", { name: "Remove from saved" }).click();
  await d.getByRole("button", { name: "Save player" }).click();
  const note = d.getByRole("textbox", { name: "Note on Mara" });
  await expect(note).toHaveValue(OLD);
  await note.fill(`${OLD}, then lurks`);
  await d.getByRole("button", { name: "Cancel" }).click();
  await expect(d.getByRole("button", { name: "Save player" })).toHaveAttribute("aria-pressed", "false");
  // Past the note's 400ms write: what was typed is undone, the older note is still stored.
  await page.waitForTimeout(800);
  expect(await stored()).toBe(OLD);
  await d.getByRole("button", { name: "Save player" }).click();
  await expect(note).toHaveValue(OLD);
});

test("focus returns to the card that opened the drawer", async ({ page }) => {
  await routeMedia(page);
  await gotoApp(page, "/?scene=ingame");
  const card = page.getByRole("button").filter({ hasText: "Sable" }).first();
  await card.focus();
  await page.keyboard.press("Enter");
  await expect(dialog(page)).toHaveAttribute("aria-label", "Sable profile");
  await expect(dialog(page)).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(dialog(page)).toHaveCount(0);
  await expect(card).toBeFocused();
});

test("the scrim and × close the drawer", async ({ page }) => {
  await open(page, `scene=ingame&profile=${MARA}`);
  await dialog(page).getByRole("button", { name: "Close profile" }).click();
  await expect(dialog(page)).toHaveCount(0);
  // The 700px panel fills <main> at the compact window: use a wide window so a click beside the drawer lands on the scrim.
  await page.setViewportSize({ width: 1280, height: 800 });
  await gotoApp(page, `/?scene=ingame&profile=${MARA}`);
  await expect(dialog(page)).toBeVisible();
  await page.mouse.click(100, 400);
  await expect(dialog(page)).toHaveCount(0);
});

test("the flagged smurf player shows no automated smurf decoration (R205)", async ({ page }) => {
  await open(page, `scene=ingame&profile=${PUUID.wisp}`);
  const d = dialog(page);
  await expect(d).toBeVisible();
  // The compact drawer deliberately carries no smurf badge/panel; the underlying signal data still exists.
  await expect(d.locator('[aria-label="Automated smurf signal"], .smurf')).toHaveCount(0);
  await expect(d).not.toContainText(/smurf/i);
});

test("a hidden player: no tag, the hidden style, no name in the label", async ({ page }) => {
  await open(page, "scene=details-hidden");
  const d = dialog(page);
  await expect(d).toHaveAttribute("aria-label", "Hidden player profile");
  await expect(d.getByRole("region", { name: "Encounters", exact: true })).toHaveCount(0);
  await expect(d.getByRole("region", { name: "Top teammates", exact: true })).toBeVisible();
  await expect(d.locator("header").getByText(/#/)).toHaveCount(0);
});

test("loading: skeletons, then the settled-empty state draws —", async ({ page }) => {
  await open(page, "scene=details-loading");
  await expect(dialog(page)).toHaveAttribute("aria-busy", "true");
  await expect(dialog(page).getByText("Mara", { exact: true })).toBeVisible();
  await open(page, "scene=details-empty");
  const d = dialog(page);
  await expect(d).toHaveAttribute("aria-busy", "false");
  await expect(d.getByText("—").first()).toBeVisible();
});

test("an encounter row opens the scoreboard from the user's side", async ({ page }) => {
  await open(page, `scene=ingame&profile=${MARA}`);
  await dialog(page).getByRole("button", { name: /Open scoreboard$/ }).first().press("Enter");
  await expect(dialog(page)).toHaveCount(0);
  await expectScoreboard(page, "Mara");
});
