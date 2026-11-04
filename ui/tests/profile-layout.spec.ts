import { expect, test, type Locator, type Page } from "@playwright/test";
import { detailsFor } from "../src/lib/demo/cdv8";
import { fitChecks } from "./checks";
import { openScene, settle } from "./harness";

const SECTIONS = ["Top teammates", "Recent matches", "Map performance"];

/** Read the actual section geometry, not CSS declarations or an implementation-specific layout key. */
async function layoutProblems(dialog: Locator, encounters: boolean, narrow: boolean): Promise<string[]> {
  return dialog.evaluate((panel, { encounters, narrow }) => {
    const content = panel.querySelector<HTMLElement>(".profile-content")!;
    const sections = [...content.querySelectorAll<HTMLElement>(":scope > section")];
    const boxes = new Map(sections.map((section) => [section.getAttribute("aria-label"), section.getBoundingClientRect()]));
    const mates = boxes.get("Top teammates")!;
    const recent = boxes.get("Recent matches")!;
    const maps = boxes.get("Map performance")!;
    const encounter = boxes.get("Encounters");
    const loadout = content.querySelector<HTMLElement>(".loadout-wrap")!.getBoundingClientRect();
    const problems: string[] = [];
    const gap = (a: DOMRect, b: DOMRect, label: string) => {
      const value = b.top - a.bottom;
      if (value < 12 || value > 20) problems.push(`${label}: gap ${value}`);
    };
    if (narrow) {
      const flow = [...sections.map((section) => section.getBoundingClientRect()), loadout];
      for (let i = 1; i < flow.length; i++) {
        if (Math.abs(flow[i]!.left - flow[0]!.left) > 1) problems.push(`narrow section ${i} is in a different column`);
        gap(flow[i - 1]!, flow[i]!, `narrow section ${i}`);
      }
    } else {
      gap(mates, maps, "teammates → maps");
      if (encounters && encounter) {
        if (!(encounter.left < mates.left && recent.left < mates.left)) problems.push("encounters/recent are not left of teammates");
        if (Math.abs(mates.left - maps.left) > 1) problems.push("maps do not align with teammates");
        gap(encounter, recent, "encounters → recent");
      } else {
        if (!(mates.left < recent.left && maps.left < recent.left)) problems.push("teammates/maps are not left of recent");
      }
      if (Math.abs(mates.width - recent.width) > 1) problems.push("panels are not half-width peers");
      const bottom = Math.max(...[...boxes.values()].map((box) => box.bottom));
      const loadoutGap = loadout.top - bottom;
      if (loadoutGap < 12 || loadoutGap > 20) problems.push(`loadout overlaps or leaves a gap: ${loadoutGap}`);
    }
    const contentBox = content.getBoundingClientRect();
    if (Math.abs(contentBox.bottom - loadout.bottom) > 2) problems.push(`layout does not own its full height: ${contentBox.bottom} vs ${loadout.bottom}`);
    const body = panel.querySelector<HTMLElement>(".body")!;
    if (body.scrollWidth > body.clientWidth + 1) problems.push("main profile scrolls horizontally");
    for (const element of content.querySelectorAll<HTMLElement>("*")) {
      const style = getComputedStyle(element);
      if ((style.overflowY === "auto" || style.overflowY === "scroll") && element.scrollHeight > element.clientHeight + 1) {
        problems.push(`nested vertical scroller: ${element.className}`);
      }
    }
    return problems;
  }, { encounters, narrow });
}

async function expectLayout(page: Page, encounters: boolean, narrow = false): Promise<void> {
  const dialog = page.getByRole("dialog");
  await expect.poll(() => dialog.evaluate((panel) => panel.clientWidth < 600)).toBe(narrow);
  await expect.poll(() => layoutProblems(dialog, encounters, narrow)).toEqual([]);
  expect(await dialog.locator(".profile-content > section").evaluateAll((nodes) => nodes.map((node) => node.getAttribute("aria-label"))))
    .toEqual(encounters ? ["Encounters", ...SECTIONS] : SECTIONS);
}

for (const size of [{ width: 700, height: 860 }, { width: 680, height: 760 }]) {
  for (const scene of ["details-more", "details-hidden"] as const) {
    test(`profile owns its complete layout and six rows at ${size.width}, ${scene}`, async ({ page }, info) => {
      await page.setViewportSize(size);
      await openScene(page, scene);
      await expectLayout(page, scene === "details-more");
      const dialog = page.getByRole("dialog");
      const rows = dialog.locator(".recent-section .row");
      await expect(rows).toHaveCount(6);
      await rows.last().scrollIntoViewIfNeeded();
      await expect(rows.last()).toBeInViewport();
      await expect.poll(() => dialog.locator(".body").evaluate((body) => body.scrollTop)).toBeGreaterThan(0);
      const loadout = dialog.locator(".loadout-wrap");
      await loadout.scrollIntoViewIfNeeded();
      await expect(loadout).toBeInViewport();
      await expect(loadout.locator(".tile")).toHaveCount(4);
      await page.screenshot({ path: info.outputPath("profile-bottom.png") });
    });
  }
}

for (const size of [{ width: 700, height: 860 }, { width: 680, height: 760 }, { width: 700, height: 860, drawerWidth: 500 }]) {
  for (const scene of ["details-more", "details-self"] as const) {
    test(`real Tab traversal preserves profile actions at app ${size.width}, drawer ${"drawerWidth" in size ? size.drawerWidth : size.width}, ${scene}`, async ({ page }) => {
      await page.setViewportSize(size);
      await openScene(page, scene);
      const dialog = page.getByRole("dialog");
      if ("drawerWidth" in size) {
        // The native app's minimum is 680px. Exercise the drawer's container query,
        // not an unsupported 500px app viewport that remains min-width:680px.
        await dialog.evaluate((panel, width) => { panel.style.width = `${width}px`; }, size.drawerWidth);
        await expectLayout(page, scene === "details-more", true);
      }
      const focusable = dialog.locator('a[href], button:not([disabled]), textarea, input, select, [tabindex]:not([tabindex="-1"])');
      const count = await focusable.count();
      // Another player's encounter rows and the user's own six recent rows are real actions.
      await expect(dialog.locator(scene === "details-self" ? ".recent-section [role=button]" : ".encounter-section [role=button]"))
        .toHaveCount(scene === "details-self" ? 6 : 3);
      await dialog.focus();
      for (let i = 0; i < count; i++) {
        await page.keyboard.press("Tab");
        await expect(focusable.nth(i)).toBeFocused();
        await expect(focusable.nth(i)).toBeInViewport();
      }
      await page.keyboard.press("Tab");
      await expect(focusable.first()).toBeFocused();
      await page.keyboard.press("Shift+Tab");
      await expect(focusable.last()).toBeFocused();
      expect(await dialog.locator('[tabindex]').evaluateAll((elements) => elements.filter((element) => Number(element.getAttribute("tabindex")) > 0).length)).toBe(0);
    });
  }
}

test("profile section focus and complete layout survive both breakpoint directions", async ({ page }, info) => {
  await page.setViewportSize({ width: 700, height: 860 });
  await openScene(page, "details-self");
  const row = page.getByRole("dialog").locator(".recent-section [role=button]").nth(4);
  await row.focus();
  for (const width of [500, 601, 599, 680, 700]) {
    await page.getByRole("dialog").evaluate((panel, width) => { panel.style.width = `${width}px`; }, width);
    await expectLayout(page, false, width < 600);
    await expect(row).toBeFocused();
    await row.scrollIntoViewIfNeeded();
    await expect(row).toBeInViewport();
    if (width === 500) await page.screenshot({ path: info.outputPath("500px-drawer-in-700x860-app.png") });
  }
});

test("delayed details and changed section heights reflow without overlap", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.setViewportSize({ width: 700, height: 860 });
  await openScene(page, "details-loading");
  const details = detailsFor("mara");
  await page.evaluate(async ({ path, payload }) => {
    const { openDetails } = await import(path);
    openDetails.event(payload);
  }, { path: "/src/lib/stores.ts", payload: details });
  await expect(page.getByRole("dialog")).toHaveAttribute("aria-busy", "false");
  await settle(page);
  await expectLayout(page, true);
  // A real later details event removes encounters and changes the teammates card height.
  await page.evaluate(async ({ path, payload }) => {
    const { openDetails } = await import(path);
    openDetails.event(payload);
  }, { path: "/src/lib/stores.ts", payload: { ...details, encounters: [], topTeammates: details.topTeammates.slice(0, 1) } });
  await expect(page.getByRole("dialog").locator(".encounter-section")).toHaveCount(0);
  await expectLayout(page, false);
  expect(errors).toEqual([]);
});

test.describe("actual Windows 150% display scale", () => {
  test.use({ viewport: { width: 680, height: 760 }, deviceScaleFactor: 1.5 });
  for (const scene of ["details-more", "details-hidden", "ingame-remembered"] as const) {
    test(`DPR 1.5 geometry and durable screenshot: ${scene}`, async ({ page }, info) => {
      await openScene(page, scene);
      expect(await page.evaluate(() => devicePixelRatio)).toBe(1.5);
      if (scene !== "ingame-remembered") await expectLayout(page, scene === "details-more");
      // Capture the unmodified UI before the ink checker temporarily changes clipping/decoration for its pixel probes.
      await page.screenshot({ path: info.outputPath(`680x760-dpr1.5-${scene}.png`) });
      expect(await fitChecks(page, scene)).toEqual([]);
      await info.attach("actual-viewport", {
        body: JSON.stringify({ viewport: page.viewportSize(), dpr: await page.evaluate(() => devicePixelRatio), scene }),
        contentType: "application/json",
      });
    });
  }
});
