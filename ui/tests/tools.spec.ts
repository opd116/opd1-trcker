// The tool screens (design v2, TrackerApp.dc.html 862–948 + `aboutInfo` 2040): the ASCII studio, Settings and About
// against the DEV demo commands. Layout sizes from the design, the studio's controls end to end (filter, effects,
// painting, copy, save, delete, the gallery), Settings' switches and radiogroups, and About's rows.
import { expect, test, type Page } from "@playwright/test";
import { openScene } from "./harness";
test.use({ viewport: { width: 700, height: 860 } });

const SIZES = [
  [700, 860],
  [680, 760],
] as const;

const open = (page: Page, screen: string) => openScene(page, "closed", { screen });
const box = async (page: Page, sel: string) => (await page.locator(sel).first().boundingBox())!;
const canvasText = (page: Page) => page.locator("pre.canvas").evaluate((el) => el.firstChild?.textContent ?? "");

for (const [w, h] of SIZES) {
  test(`ascii layout at ${w}×${h}: compact reflow (one column, two-column controls), the page scrolls, no sideways overflow`, async ({ page }) => {
    await page.setViewportSize({ width: w, height: h });
    await open(page, "ascii");
    // At the compact widths the studio reflows: the editor stacks (one column, auto height) and its controls run in two
    // columns at the editor's full width, instead of the 440px / 300px two-pane layout the wide window uses.
    expect(await page.locator(".editor").evaluate((el) => getComputedStyle(el).gridTemplateColumns.split(" ").length)).toBe(1);
    expect(await page.locator(".controls").evaluate((el) => getComputedStyle(el).gridTemplateColumns.split(" ").length)).toBe(2);
    expect((await box(page, ".controls")).width).toBeCloseTo((await box(page, ".editor")).width, 0);
    const [sw, cw, sh, ch] = await page.locator("main .page").evaluate((el) => [el.scrollWidth, el.clientWidth, el.scrollHeight, el.clientHeight]);
    expect(sw).toBe(cw);
    expect(sh).toBeGreaterThan(ch);
    const lines = (await canvasText(page)).split("\n");
    expect(lines).toHaveLength(15);
    expect(new Set(lines.map((l) => l.length)).size).toBe(1);
    // Six presets, 160px tiles.
    await expect(page.locator(".gallery .tile")).toHaveCount(6);
    expect((await box(page, ".gallery .tile")).height).toBe(160);
    const doc = await page.evaluate(() => [document.documentElement.scrollWidth, document.documentElement.scrollHeight]);
    expect(doc).toEqual([w, h]);
  });
}

test("ascii: the input keeps a-z 0-9 space ! ? . - only, at most 10, with a counter", async ({ page }) => {
  await open(page, "ascii");
  const input = page.getByLabel("Text", { exact: true });
  await input.fill("");
  await input.pressSequentially("gg_wp#!abcdefgh");
  await expect(input).toHaveValue("ggwp!abcde");
  await expect(page.locator(".field .count")).toHaveText("10 / 10");
  expect(await page.locator(".field .count").evaluate((el) => getComputedStyle(el).color)).toBe("rgb(255, 198, 144)");
});

test("ascii: fill, effect and tool radiogroups (click and arrow keys)", async ({ page }) => {
  await open(page, "ascii");
  const effect = page.getByRole("radiogroup", { name: "Effect" });
  await expect(effect.getByRole("radio", { name: "Shadow" })).toHaveAttribute("aria-checked", "true");
  await effect.getByRole("radio", { name: "Shadow" }).focus();
  await page.keyboard.press("ArrowLeft");
  await expect(effect.getByRole("radio", { name: "Wide" })).toHaveAttribute("aria-checked", "true");
  await expect(effect.getByRole("radio", { name: "Wide" })).toBeFocused();
  expect(await canvasText(page)).not.toContain("░");
  const fill = page.getByRole("radiogroup", { name: "Fill character" });
  await fill.getByRole("radio", { name: "Hash" }).click();
  expect(await canvasText(page)).toContain("##");
  expect(await canvasText(page)).not.toContain("█");
  // Picking a brush switches to Draw.
  await page.getByRole("radiogroup", { name: "Tool" }).getByRole("radio", { name: "Erase" }).click();
  await page.getByRole("radiogroup", { name: "Brush" }).getByRole("radio", { name: "Plus" }).click();
  await expect(page.getByRole("radiogroup", { name: "Tool" }).getByRole("radio", { name: "Draw" })).toHaveAttribute("aria-checked", "true");
});

test("ascii: drag to paint, erase, keyboard paint, Clear", async ({ page }) => {
  await open(page, "ascii");
  const clear = page.getByRole("button", { name: "Clear drawing" });
  await expect(clear).toBeDisabled();
  await page.getByRole("radiogroup", { name: "Brush" }).getByRole("radio", { name: "Plus" }).click();
  const b = await box(page, "pre.canvas");
  // The top-left corner is blank in CLUTCH: drag a stroke along the first row.
  await page.mouse.move(b.x + 2, b.y + 2);
  await page.mouse.down();
  await page.mouse.move(b.x + b.width * 0.1, b.y + 2, { steps: 8 });
  await page.mouse.up();
  const first = (await canvasText(page)).split("\n")[0]!;
  expect(first.startsWith("++")).toBe(true);
  await expect(clear).toBeEnabled();
  // Erase the first cell.
  await page.getByRole("radiogroup", { name: "Tool" }).getByRole("radio", { name: "Erase" }).click();
  await page.mouse.click(b.x + 2, b.y + 2);
  expect((await canvasText(page)).split("\n")[0]![0]).toBe(" ");
  // Keyboard: the canvas is one tab stop; arrows move the cursor, Space paints at it.
  await page.getByRole("radiogroup", { name: "Brush" }).getByRole("radio", { name: "At sign" }).click();
  await page.locator("pre.canvas").focus();
  await page.keyboard.press("ArrowUp");
  for (let i = 0; i < 6; i++) await page.keyboard.press("ArrowUp");
  await page.keyboard.press("Space");
  const rows = (await canvasText(page)).split("\n");
  expect(rows[0]![rows[0]!.length >> 1]).toBe("@");
  await clear.click();
  expect(await canvasText(page)).not.toMatch(/[+@]/);
  await expect(clear).toBeDisabled();
});

test("ascii: Copy puts the trimmed art on the clipboard; the button and the status say so for 2.4 s", async ({ page, context }) => {
  await context.grantPermissions(["clipboard-read", "clipboard-write"]);
  await open(page, "ascii");
  const copy = page.getByRole("button", { name: "Copy to clipboard" });
  await copy.click();
  await expect(page.locator(".actions .primary")).toHaveText("Copied ✓");
  // Windows hands the text back with CRLF line ends.
  const clip = (await page.evaluate(() => navigator.clipboard.readText())).replace(/\r\n/g, "\n");
  const lines = clip.split("\n");
  expect(lines).toHaveLength(6);
  expect(clip).toContain("█");
  await expect(page.getByRole("status")).toHaveText(`Copied ${Math.max(...lines.map((l) => l.length))} × 6 characters.`);
  await expect(page.locator(".actions .primary")).toHaveText("Copy to clipboard", { timeout: 4000 });
  await expect(page.getByRole("status")).toHaveText("");
  // A tile's Copy.
  await page.getByRole("button", { name: "Copy GG" }).click();
  await expect(page.getByRole("button", { name: "Copy GG" })).toHaveText("Copied ✓");
  await expect(page.getByRole("status")).toHaveText("Copied from the gallery.");
  expect(await page.evaluate(() => navigator.clipboard.readText())).toContain("▓▓");
});

test("ascii: a tile opens in the editor (text art → the controls; a drawing → paint)", async ({ page }) => {
  await open(page, "ascii");
  await page.getByRole("button", { name: "Open GG in the editor" }).click();
  await expect(page.getByLabel("Text", { exact: true })).toHaveValue("GG");
  await expect(page.getByRole("radiogroup", { name: "Effect" }).getByRole("radio", { name: "Wide" })).toHaveAttribute("aria-checked", "true");
  await expect(page.getByRole("radiogroup", { name: "Fill character" }).getByRole("radio", { name: "Dark shade" })).toHaveAttribute("aria-checked", "true");
  await page.getByRole("button", { name: "Open Crosshair in the editor" }).click();
  await expect(page.getByLabel("Text", { exact: true })).toHaveValue("");
  expect(await canvasText(page)).toContain("━━━━   •   ━━━━");
  await expect(page.getByRole("button", { name: "Clear drawing" })).toBeEnabled();
});

test("ascii: Save stores the drawing first in the gallery, marked as yours; Delete takes two clicks", async ({ page }) => {
  await open(page, "ascii");
  await page.getByLabel("Text", { exact: true }).fill("ace");
  await page.getByRole("button", { name: "Save", exact: true }).click();
  await expect(page.locator(".actions .secondary")).toHaveText("Saved ✓");
  await expect(page.getByRole("status")).toHaveText("Saved to the gallery below.");
  const tiles = page.locator(".gallery .tile");
  await expect(tiles).toHaveCount(7);
  await expect(tiles.first().locator(".title")).toHaveText("ACE");
  await expect(tiles.first().locator(".mine")).toHaveAttribute("data-tip", "Saved by you");
  // Saving the same drawing again keeps one copy.
  await page.locator(".actions .secondary").click();
  await expect(tiles).toHaveCount(7);
  const del = page.getByRole("button", { name: "Delete ACE" });
  await expect(del).toHaveAttribute("data-tip", "Delete");
  await del.click();
  await expect(page.getByRole("button", { name: "Confirm delete ACE" })).toHaveText("Delete?");
  await page.getByRole("button", { name: "Confirm delete ACE" }).click();
  await expect(tiles).toHaveCount(6);
});

for (const [w, h] of SIZES) {
  test(`settings at ${w}×${h}: intentionally empty, no unavailable controls`, async ({ page }) => {
    await page.setViewportSize({ width: w, height: h });
    await open(page, "settings");
    await expect(page.locator("main h1")).toHaveText("Settings");
    await expect(page.locator("main button, main input, main [role=switch], main [role=radio]")).toHaveCount(0);
    await expect(page.locator("main")).toHaveText("Settings");
    const [sw, cw] = await page.locator("main").evaluate((el) => [el.scrollWidth, el.clientWidth]);
    expect(sw).toBe(cw);
  });
}

test("R206: the title bar has no always-on-top pin; Settings stays empty and the stored defaults hold", async ({ page }) => {
  await open(page, "settings");
  // R206 removed the pin from both shell bars: the title bar carries only the window buttons.
  const labels = await page
    .locator("#app > header")
    .getByRole("button")
    .evaluateAll((els) => els.map((e) => e.getAttribute("aria-label")));
  expect(labels).toEqual(["Minimize", "Maximize", "Close"]);
  await expect(page.locator('#app > header button[aria-label="Keep on top during matches"]')).toHaveCount(0);
  // The postponed Settings page still shows only its title, and the stored defaults are the documented seven.
  const readSettings = () =>
    page.evaluate(`import('/src/lib/appdata.svelte.ts').then(({settings}) => { let value; settings.subscribe(v => value = v)(); return value; })`);
  expect(await readSettings()).toEqual({ highlight: true, enemyFirst: false, smurf: "cautious", savedAlert: true, sound: false, boot: true, tray: true });
  await expect(page.locator("main")).toHaveText("Settings");
});

test("about: wordmark and the six rows from app_info and the status", async ({ page }) => {
  await open(page, "about");
  await expect(page.locator("main h1")).toHaveText("About");
  await expect(page.locator("main .brand .word")).toHaveText("OPD1");
  await expect(page.locator("main .brand .mark")).toHaveText("TRCKER");
  await expect(page.locator("dl.info")).toHaveAttribute("aria-busy", "false");
  const rows = await page.locator("dl.info .row").evaluateAll((els) => els.map((e) => [e.querySelector("dt")!.textContent!.trim(), e.querySelector("dd")!.textContent!.trim()]));
  expect(rows).toEqual([
    ["VERSION", "0.4.0 · 2a5a381"],
    ["BUILD", "Oct 19, 2026"],
    ["RIOT CLIENT", "98.0.1 · connected"],
    ["VALORANT", "11.08"],
    ["GAME DATA", "valorant-api.com"],
    ["LOCAL DATABASE", "1,284 matches · 48 MB"],
  ]);
  await expect(page.getByText("SIL Open Font License 1.1", { exact: false })).toBeVisible();
});
