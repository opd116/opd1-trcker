// The tool screens (design v2, TrackerApp.dc.html 862–948 + `aboutInfo` 2040): the ASCII studio, Settings and About
// against the DEV demo commands. Layout sizes from the design, the studio's controls end to end (filter, effects,
// painting, copy, save, delete, the gallery), Settings' switches and radiogroups, and About's rows.
import { expect, test, type Page } from "@playwright/test";
import { openScene } from "./harness";

const SIZES = [
  [1280, 800],
  [1180, 760],
] as const;

const open = (page: Page, screen: string) => openScene(page, "closed", { screen });
const box = async (page: Page, sel: string) => (await page.locator(sel).first().boundingBox())!;
const canvasText = (page: Page) => page.locator("pre.canvas").evaluate((el) => el.firstChild?.textContent ?? "");

for (const [w, h] of SIZES) {
  test(`ascii layout at ${w}×${h}: 440px editor (300px controls), the page scrolls, no sideways overflow`, async ({ page }) => {
    await page.setViewportSize({ width: w, height: h });
    await open(page, "ascii");
    expect((await box(page, ".editor")).height).toBe(440);
    expect((await box(page, ".controls")).width).toBe(300);
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
  test(`settings layout at ${w}×${h}: the 780px column, four groups, the design's words`, async ({ page }) => {
    await page.setViewportSize({ width: w, height: h });
    await open(page, "settings");
    expect((await box(page, "main .col")).width).toBe(780);
    await expect(page.locator("main h1")).toHaveText("Settings");
    await expect(page.locator("section.group")).toHaveCount(4);
    await expect(page.getByRole("switch")).toHaveCount(7);
    expect((await box(page, '[role="switch"]')).width).toBe(42);
    await expect(page.getByText("Flags only when 3 of 4 signals agree", { exact: false })).toBeVisible();
    const [sw, cw] = await page.locator("main .page").evaluate((el) => [el.scrollWidth, el.clientWidth]);
    expect(sw).toBe(cw);
  });
}

test("settings: switches flip and persist through updateSettings; Play a sound waits for the alert", async ({ page }) => {
  await open(page, "settings");
  const enemy = page.getByRole("switch", { name: "Enemy team first" });
  await expect(enemy).toHaveAttribute("aria-checked", "false");
  await enemy.click();
  await expect(enemy).toHaveAttribute("aria-checked", "true");
  // Keyboard: Space on a focused switch.
  await enemy.focus();
  await page.keyboard.press("Space");
  await expect(enemy).toHaveAttribute("aria-checked", "false");

  const alert = page.getByRole("switch", { name: "Saved player in my match" });
  const sound = page.getByRole("switch", { name: "Play a sound with alerts" });
  await expect(sound).not.toHaveAttribute("aria-disabled", "true");
  await alert.click();
  await expect(sound).toHaveAttribute("aria-disabled", "true");
  await expect(sound).toHaveAttribute("data-tip", "Turn on “Saved player in my match” first");
  const before = await sound.getAttribute("aria-checked");
  await sound.click({ force: true });
  await expect(sound).toHaveAttribute("aria-checked", before!);
  expect(await sound.evaluate((el) => getComputedStyle(el).opacity)).toBe("0.5");
  await alert.click();
  await expect(sound).not.toHaveAttribute("aria-disabled", "true");

  // The pin in the title bar and the setting are one value.
  const onTop = page.getByRole("switch", { name: "Keep on top during matches" });
  const pin = page.locator('#app > header button[aria-label="Keep on top during matches"]');
  await expect(onTop).toHaveAttribute("aria-checked", "true");
  await expect(pin).toHaveAttribute("aria-pressed", "true");
  await onTop.click();
  await expect(pin).toHaveAttribute("aria-pressed", "false");
  await pin.click();
  await expect(onTop).toHaveAttribute("aria-checked", "true");
});

test("settings: smurf warnings radiogroup (arrow keys) and its option table", async ({ page }) => {
  await open(page, "settings");
  const g = page.getByRole("radiogroup", { name: "Smurf warnings" });
  await expect(g.getByRole("radio", { name: "Cautious" })).toHaveAttribute("aria-checked", "true");
  await expect(g.getByRole("radio", { name: "Cautious" })).toHaveAttribute("tabindex", "0");
  await expect(g.getByRole("radio", { name: "Off" })).toHaveAttribute("tabindex", "-1");
  await g.getByRole("radio", { name: "Cautious" }).focus();
  await page.keyboard.press("ArrowRight");
  await expect(g.getByRole("radio", { name: "Eager" })).toHaveAttribute("aria-checked", "true");
  await expect(page.locator(".table .r.on .name")).toHaveText("Eager");
  await g.getByRole("radio", { name: "Off" }).click();
  await expect(page.locator(".table .r.on .name")).toHaveText("Off");
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
