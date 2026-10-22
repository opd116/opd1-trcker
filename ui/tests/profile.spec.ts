// The v2 profile drawer (design TrackerApp.dc.html 949–1128) against the DEV demo feed: the dialog's name and focus,
// Escape order (note popover first, then the drawer), focus return, the save + note flow, the self / other variants,
// the hidden and empty edge scenes, and the encounter → scoreboard hand-off. 1280×800 (playwright.config.ts).
import { expect, test, type Page } from "@playwright/test";
import { PUUID } from "../src/lib/demo/cdv8";
import { routeMedia } from "./harness";

const MARA = PUUID.mara!;
const FAHAD = PUUID.fahad!;
const ZEPHYR = PUUID.zephyr!;

const dialog = (page: Page) => page.getByRole("dialog");

async function open(page: Page, query: string): Promise<void> {
  await routeMedia(page);
  await page.goto(`/?${query}`);
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

test("focus returns to the card that opened the drawer", async ({ page }) => {
  await routeMedia(page);
  await page.goto("/?scene=ingame");
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
  await page.goto(`/?scene=ingame&profile=${MARA}`);
  await expect(dialog(page)).toBeVisible();
  await page.mouse.click(100, 400);
  await expect(dialog(page)).toHaveCount(0);
});

test("a hidden player: no tag, the hidden style, no name in the label", async ({ page }) => {
  await open(page, "scene=details-hidden");
  const d = dialog(page);
  await expect(d).toHaveAttribute("aria-label", "Hidden player profile");
  await expect(d.getByText("No stored matches with this player before this one.")).toBeVisible();
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
});
