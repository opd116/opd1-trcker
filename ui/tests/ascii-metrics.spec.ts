import { expect, test } from "@playwright/test";
import { CANVAS_H, canvasFontPx, composeCanvas, INITIAL, rawCanvasW } from "../src/lib/ascii";
import { openScene, openScreen } from "./harness";

for (const width of [680, 700]) {
  for (const family of ["shipped", '"Courier New", monospace', '"Geist Mono"']) {
    test(`ordinary ASCII fits actual glyph advances at ${width}: ${family}`, async ({ page }, info) => {
      await page.setViewportSize({ width, height: 860 });
      await openScene(page, "closed", { screen: "ascii" });
      if (family !== "shipped") await page.addStyleTag({ content: `pre.canvas{font-family:${family}!important}` });
      await page.evaluate(() => document.fonts.ready);
      await expect.poll(() => page.locator("pre.canvas").evaluate((pre) => {
        const frame = pre.parentElement!;
        const style = getComputedStyle(frame);
        const usable = frame.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight);
        return { clipped: pre.getBoundingClientRect().width > usable + 1, sideways: frame.scrollWidth > frame.clientWidth + 1 };
      })).toEqual({ clipped: false, sideways: false });
      expect(await page.locator("pre.canvas").textContent()).toBe(composeCanvas(INITIAL.text, INITIAL.char, INITIAL.effect, {}).out);
      await page.screenshot({ path: info.outputPath("ascii-fitted.png") });
    });
  }

  test(`pointer and keyboard still paint exact logical cells after fitting at ${width}`, async ({ page }) => {
    await page.setViewportSize({ width, height: 860 });
    await openScene(page, "closed", { screen: "ascii" });
    const pre = page.locator("pre.canvas");
    const canvas = composeCanvas(INITIAL.text, INITIAL.char, INITIAL.effect, {});
    await expect.poll(() => pre.evaluate((element) => element.parentElement!.scrollWidth - element.parentElement!.clientWidth)).toBeLessThanOrEqual(1);
    await pre.scrollIntoViewIfNeeded();
    const box = (await pre.boundingBox())!;
    const cell = { x: 2, y: 1 };
    expect(canvas.out.split("\n")[cell.y]![cell.x]).toBe(" ");
    await page.mouse.click(box.x + (cell.x + .5) * box.width / canvas.W, box.y + (cell.y + .5) * box.height / CANVAS_H);
    await expect.poll(async () => (await pre.textContent())!.split("\n")[cell.y]![cell.x]).toBe(INITIAL.brush);
    // The first ArrowLeft starts from the model's centre; reach the blank top-left corner and paint with Space.
    for (let i = 0; i < canvas.cx + 1; i++) await page.keyboard.press("ArrowLeft");
    for (let i = 0; i < canvas.cy + 1; i++) await page.keyboard.press("ArrowUp");
    await page.keyboard.press("Space");
    await expect.poll(async () => (await pre.textContent())![0]).toBe(INITIAL.brush);
    await expect(page.getByRole("button", { name: "Clear drawing" })).toBeEnabled();
  });
}

test("very wide saved ASCII preserves all cells and intentional horizontal scrolling", async ({ page }) => {
  await page.setViewportSize({ width: 680, height: 760 });
  await openScene(page, "closed");
  const raw = "#".repeat(240);
  await page.evaluate(async ({ path, raw }) => {
    const { state } = await import(path);
    state.ascii.push({ id: 91, title: "Wide preserved drawing", raw, createdMs: Date.now() });
  }, { path: "/src/lib/demo/v2/store.ts", raw });
  await openScreen(page, "ASCII");
  await page.getByRole("button", { name: "Open Wide preserved drawing in the editor", exact: true }).click();
  const pre = page.locator("pre.canvas");
  // The existing model already chooses 7px for this unusually wide drawing; fitting must not shrink it further.
  const preferred = canvasFontPx(rawCanvasW(raw));
  expect(preferred).toBe(7);
  await expect.poll(() => pre.evaluate((element) => parseFloat(getComputedStyle(element).fontSize))).toBe(preferred);
  await expect.poll(() => pre.evaluate((element) => element.parentElement!.scrollWidth - element.parentElement!.clientWidth)).toBeGreaterThan(1);
  expect(await pre.textContent()).toContain(raw);
});
