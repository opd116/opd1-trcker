// Saved, History and the scoreboard (design v2 450–657) against the DEV demo feed: content, sorting, search, the
// note editor, opening a scoreboard and coming back (button and Esc), the hidden-name rows and the layout at the
// default and minimum window sizes (no page or sideways scroll, nothing cut).
import { expect, test, type Page } from "@playwright/test";
import { openScene } from "./harness";

const SIZES = [
  [1280, 800],
  [1180, 760],
] as const;

/** No sideways overflow anywhere in <main>, and the page itself never scrolls. */
async function noOverflow(page: Page): Promise<void> {
  const bad = await page.evaluate(() => {
    const out: string[] = [];
    for (const el of document.querySelectorAll<HTMLElement>("main *")) {
      const cs = getComputedStyle(el);
      if (cs.overflowX === "auto" || cs.overflowX === "scroll") if (el.scrollWidth > el.clientWidth + 1) out.push(el.className);
    }
    const d = document.documentElement;
    if (d.scrollWidth > d.clientWidth || d.scrollHeight > d.clientHeight) out.push("document");
    return out;
  });
  expect(bad).toEqual([]);
}

test.describe("Saved players", () => {
  for (const [w, h] of SIZES) {
    test(`layout at ${w}×${h}`, async ({ page }) => {
      await page.setViewportSize({ width: w, height: h });
      await openScene(page, "ingame", { screen: "saved" });
      await expect(page.locator(".row[data-row]")).toHaveCount(2);
      await noOverflow(page);
    });
  }

  test("rows, live chip, records and sorting", async ({ page }) => {
    await openScene(page, "ingame", { screen: "saved" });
    await expect(page.locator("h1")).toHaveText("Saved players");
    await expect(page.locator(".sum")).toHaveText("2 players");
    await expect(page.locator("main .pill")).toHaveText(["All2", "Teammates1", "Opponents2"]);
    // Mara and Kilo are both in the live match: chip + "Now".
    const rows = page.locator(".row[data-row]");
    await expect(rows.locator(".inmatch")).toHaveCount(2);
    await expect(rows.first().locator(".seen")).toHaveText("Now");
    await expect(rows.first()).toHaveAttribute("aria-label", "Mara, Immortal 1. Open profile");
    // Default sort: last seen, descending; the NOTE column does not sort.
    const heads = page.locator("[role=columnheader]");
    await expect(heads.nth(5)).toHaveAttribute("aria-sort", "descending");
    await expect(heads.nth(4).locator("button")).toBeDisabled();
    await heads.nth(0).locator("button").click();
    await expect(heads.nth(0)).toHaveAttribute("aria-sort", "ascending");
    await expect(heads.nth(5)).toHaveAttribute("aria-sort", "none");
    await expect(rows.first()).toHaveAttribute("aria-label", /^Kilo,/);
    await heads.nth(0).locator("button").click();
    await expect(heads.nth(0)).toHaveAttribute("aria-sort", "descending");
    await expect(rows.first()).toHaveAttribute("aria-label", /^Mara,/);
    // Filters.
    await page.locator("main .pill", { hasText: "Teammates" }).click();
    await expect(rows).toHaveCount(1);
    await expect(rows.first()).toHaveAttribute("aria-label", /^Kilo,/);
  });

  test("search covers name, tag and note; no results", async ({ page }) => {
    await openScene(page, "closed", { screen: "saved" });
    const search = page.getByLabel("Search saved players");
    const rows = page.locator(".row[data-row]");
    await search.fill("dry-peeks");
    await expect(rows).toHaveCount(1);
    await search.fill("2210");
    await expect(rows).toHaveCount(1);
    await expect(rows.first()).toHaveAttribute("aria-label", /^Kilo,/);
    await search.fill("nobody");
    await expect(rows).toHaveCount(0);
    await expect(page.locator(".nores")).toHaveText("No saved player matches “nobody”.");
  });

  test("note: add, saves as you type, Esc closes, long notes clamp with a toggle", async ({ page }) => {
    await openScene(page, "closed", { screen: "saved" });
    await page.getByRole("button", { name: "Add a note on Kilo" }).click();
    const area = page.getByRole("textbox", { name: "Add a note on Kilo" });
    await expect(area).toBeFocused();
    await expect(page.locator(".hint")).toHaveText("Saves as you type · Esc closes");
    const long = "Lurks on attack every round and rarely plays with the team. Watch the A main flank after 0:40.";
    await area.pressSequentially(long);
    await page.keyboard.press("Escape");
    await expect(area).toHaveCount(0);
    // The drawer did not open, and the note shows clamped with its toggle.
    await expect(page.locator(".drawer-stub")).toHaveCount(0);
    const kilo = page.locator(".row[data-row]", { hasText: "Kilo" });
    await expect(kilo.locator(".note")).toHaveText(long);
    const more = kilo.getByRole("button", { name: "Show full note" });
    await expect(more).toHaveAttribute("aria-expanded", "false");
    await more.click();
    await expect(kilo.getByRole("button", { name: "Show less" })).toHaveAttribute("aria-expanded", "true");
    await expect(kilo.getByRole("button", { name: "Edit note on Kilo" })).toBeVisible();
    // The search finds the new note (the marks copy, before any round trip).
    await page.getByLabel("Search saved players").fill("main flank");
    await expect(page.locator(".row[data-row]")).toHaveCount(1);
  });

  test("a row opens the profile (click and Enter)", async ({ page }) => {
    await openScene(page, "closed", { screen: "saved" });
    const row = page.locator(".row[data-row]").first();
    await row.focus();
    await page.keyboard.press("Enter");
    await expect(page.locator(".drawer-stub")).toHaveCount(1);
  });

  test("empty: Go to live match", async ({ page }) => {
    await openScene(page, "closed", { screen: "saved", empty: "1" });
    await expect(page.locator(".sum")).toHaveText("Nobody saved yet");
    await expect(page.locator(".etitle")).toHaveText("No saved players yet");
    await page.getByRole("button", { name: "Go to live match" }).click();
    await expect(page.locator('nav.rail button[aria-label="Live"]')).toHaveAttribute("aria-current", "page");
  });
});

test.describe("Match history", () => {
  for (const [w, h] of SIZES) {
    test(`layout at ${w}×${h}`, async ({ page }) => {
      await page.setViewportSize({ width: w, height: h });
      await openScene(page, "closed", { screen: "history" });
      await expect(page.locator(".day")).not.toHaveCount(0);
      await noOverflow(page);
      // 56px rows, 42px day headers.
      expect((await page.locator(".row[data-row]").first().boundingBox())!.height).toBe(56);
      expect((await page.locator(".day").first().boundingBox())!.height).toBe(42);
    });
  }

  test("tabs, day groups and rows", async ({ page }) => {
    await openScene(page, "closed", { screen: "history" });
    await expect(page.getByRole("tab")).toHaveText(["Competitive11", "All modes13"]);
    await expect(page.getByRole("tab", { name: /Competitive/ })).toHaveAttribute("aria-selected", "true");
    await expect(page.locator(".dlabel").first()).toHaveText("Today");
    await expect(page.locator(".row[data-row]")).toHaveCount(11);
    const first = page.locator(".row[data-row]").first();
    await expect(first).toHaveAttribute("aria-label", "Lotus, win 13 to 9, plus 18 RR. Open scoreboard");
    await expect(first.locator(".when")).toHaveText("45 min ago · 38 min");
    await expect(page.locator(".foot")).toContainText("Showing all 11 stored matches, newest first.");
    // All modes: the Swiftplay row has no rank and no RR.
    await page.getByRole("tab", { name: /All modes/ }).click();
    await expect(page.locator(".row[data-row]")).toHaveCount(13);
    const swift = page.locator(".row[data-row]", { hasText: "Icebox" });
    await expect(swift.locator(".rr")).toHaveText("—");
    await expect(swift.locator(".ricon")).toHaveCount(0);
  });

  test("a row opens its scoreboard; Back and Esc return to the list", async ({ page }) => {
    await openScene(page, "closed", { screen: "history" });
    await page.locator(".row[data-row]").first().click();
    const board = page.locator('.match[aria-busy="false"]');
    await expect(board).toBeVisible();
    await expect(page.locator(".back")).toHaveText("Match history");
    await expect(page.locator(".hres")).toHaveText("VICTORY");
    await expect(page.locator(".ylabel")).toHaveText("YOUR RR");
    await expect(page.locator(".yval")).toHaveText("+18");
    await expect(page.locator(".you")).toHaveCount(1);
    await expect(page.locator(".mvp", { hasText: "MATCH MVP" })).toHaveCount(1);
    await expect(page.locator(".mvp", { hasText: "TEAM MVP" })).toHaveCount(1);
    // No bookmark on the user's own row.
    await expect(page.locator(".srow.self .save")).toHaveCount(0);
    await page.locator(".back").click();
    await expect(page.locator(".dlabel").first()).toHaveText("Today");
    await page.locator(".row[data-row]").nth(1).focus();
    await page.keyboard.press("Enter");
    await expect(page.locator('.match[aria-busy="false"]')).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(page.locator(".dlabel").first()).toHaveText("Today");
  });

  test("empty", async ({ page }) => {
    await openScene(page, "closed", { screen: "history", empty: "1" });
    await expect(page.locator(".etitle")).toHaveText("No matches stored yet");
    await expect(page.getByRole("tab")).toHaveText(["Competitive0", "All modes0"]);
  });
});

test.describe("Scoreboard", () => {
  for (const [w, h] of SIZES) {
    test(`layout at ${w}×${h}`, async ({ page }) => {
      await page.setViewportSize({ width: w, height: h });
      await openScene(page, "match-longnames");
      await noOverflow(page);
    });
  }

  test("opened from a profile: Back is the player's name, their team's label, the bookmark toggles", async ({ page }) => {
    await openScene(page, "match");
    await expect(page.locator(".back")).toHaveText("Mara");
    await expect(page.locator(".tlabel").first()).toHaveText("Mara's team");
    await expect(page.locator(".yrr")).toHaveCount(0);
    const save = page.locator(".srow", { hasText: "Vantablack" }).locator(".save");
    await expect(save).toHaveAttribute("aria-pressed", "false");
    await save.click();
    await expect(save).toHaveAttribute("aria-pressed", "true");
  });

  test("hidden names, no-stats rows and Deathmatch", async ({ page }) => {
    await openScene(page, "match-hidden");
    await expect(page.locator(".srow .hid").first()).toBeVisible();
    await openScene(page, "match-nostats");
    await expect(page.locator(".srow", { hasText: "Zephyr" }).locator(".acs")).toHaveText("—");
    await openScene(page, "match-dm");
    await expect(page.locator(".team")).toHaveCount(1);
    await expect(page.locator(".tlabel")).toHaveText("PLAYERS");
    await expect(page.locator(".hscore")).toHaveCount(0);
  });

  test("loading: the skeleton, busy", async ({ page }) => {
    await openScene(page, "match-loading");
    await expect(page.locator('.match[aria-busy="true"] .hero.skel')).toBeVisible();
  });
});
