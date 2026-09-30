// R106 focus ring, v0.2.0 polish: it is keyboard-only (`:focus-visible`), and inside a clipping ancestor it is inset so
// no side is cut. Rows in a `.panel { overflow:hidden }` (Recent, Encounters) and the Live tab at the top of #app.
// The probe compares the element with and without its ring, and counts the pixels that differ in a 4px band inside
// each edge: a side the ring is clipped on has none.
import { expect, test, type Locator, type Page } from "@playwright/test";
import { openScene } from "./harness";

type Sides = { top: number; right: number; bottom: number; left: number; w: number; h: number };

/** How many pixels the ring paints in a 4px band inside each edge of `el`'s border box (which must be on screen). */
async function ringSides(page: Page, el: Locator): Promise<Sides> {
  const r = await el.boundingBox();
  if (!r) throw new Error("no box");
  const x0 = Math.round(r.x);
  const y0 = Math.round(r.y);
  const w = Math.round(r.width);
  const h = Math.round(r.height);
  const clip = { x: Math.max(0, x0 - 4), y: Math.max(0, y0 - 4), width: Math.min(1000, w + 8), height: Math.min(800, h + 8) };
  const withRing = await page.screenshot({ clip });
  const off = await page.addStyleTag({ content: ":focus-visible{outline:none!important}" });
  const without = await page.screenshot({ clip });
  await off.evaluate((s) => (s as Element).remove());
  const inner = { x: x0 - clip.x, y: y0 - clip.y, w, h };
  return page.evaluate(
    async ([a, b, box]) => {
      const px = async (b64: string) => {
        const bytes = Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
        const bmp = await createImageBitmap(new Blob([bytes], { type: "image/png" }));
        const cv = new OffscreenCanvas(bmp.width, bmp.height);
        const ctx = cv.getContext("2d")!;
        ctx.drawImage(bmp, 0, 0);
        return { d: ctx.getImageData(0, 0, bmp.width, bmp.height).data, bw: bmp.width };
      };
      const [A, B] = [await px(a!), await px(b!)];
      const diff = (x: number, y: number) => {
        const i = (y * A.bw + x) * 4;
        return Math.abs(A.d[i]! - B.d[i]!) + Math.abs(A.d[i + 1]! - B.d[i + 1]!) + Math.abs(A.d[i + 2]! - B.d[i + 2]!) > 24;
      };
      const { x, y, w, h } = box as { x: number; y: number; w: number; h: number };
      const out = { top: 0, right: 0, bottom: 0, left: 0, w, h };
      for (let dx = 0; dx < w; dx++) for (let k = 0; k < 4; k++) {
        if (diff(x + dx, y + k)) { out.top++; break; }
      }
      for (let dx = 0; dx < w; dx++) for (let k = 0; k < 4; k++) {
        if (diff(x + dx, y + h - 1 - k)) { out.bottom++; break; }
      }
      for (let dy = 0; dy < h; dy++) for (let k = 0; k < 4; k++) {
        if (diff(x + k, y + dy)) { out.left++; break; }
      }
      for (let dy = 0; dy < h; dy++) for (let k = 0; k < 4; k++) {
        if (diff(x + w - 1 - k, y + dy)) { out.right++; break; }
      }
      return out;
    },
    [withRing.toString("base64"), without.toString("base64"), inner] as const,
  );
}

/** Every side of the ring is there: at least 80% of each edge's columns or rows (less the rounded corners) carry ring pixels. */
function expectFullRing(s: Sides, what: string) {
  for (const side of ["top", "bottom"] as const) expect(s[side], `${what}: ${side} edge`).toBeGreaterThanOrEqual(Math.floor((s.w - 20) * 0.8));
  for (const side of ["left", "right"] as const) expect(s[side], `${what}: ${side} edge`).toBeGreaterThanOrEqual(Math.floor((s.h - 20) * 0.8));
}

const focusVisible = (l: Locator) => l.evaluate((el) => el.matches(":focus-visible"));
const outline = (l: Locator) =>
  l.evaluate((el) => {
    const c = getComputedStyle(el);
    return { style: c.outlineStyle, width: c.outlineWidth, offset: c.outlineOffset };
  });

test("the Live tab's ring is inset and whole (it touches the top edge of #app)", async ({ page }) => {
  await openScene(page, "ingame");
  await page.keyboard.press("Tab");
  const tab = page.locator(".tab.on");
  await expect(tab).toBeFocused();
  expect(await focusVisible(tab)).toBe(true);
  expect(await outline(tab)).toEqual({ style: "solid", width: "1px", offset: "-2px" });
  const box = await tab.boundingBox();
  expect(box!.y).toBeLessThanOrEqual(0.5); // the reason: nothing above it for an outside ring
  expectFullRing(await ringSides(page, tab), "Live tab");
});

test("the Live tab's layout and indicator did not move for the ring's room", async ({ page }) => {
  await openScene(page, "ingame");
  const word = await page.locator(".word").boundingBox();
  const tab = await page.locator(".tab.on").boundingBox();
  const off = await page.locator(".tab.off").boundingBox();
  const ind = await page.locator(".tab.on .ind").boundingBox();
  // The button box is the text box plus 4px of padding a side (taken back by -4px margins), so the text of Live
  // still starts 30px after the wordmark (the bar's gap) and My Stats 24px after the end of Live's text.
  const textLeft = tab!.x + 4;
  const textRight = tab!.x + tab!.width - 4;
  expect(Math.abs(textLeft - (word!.x + word!.width + 30))).toBeLessThanOrEqual(0.5);
  expect(Math.abs(off!.x - (textRight + 24))).toBeLessThanOrEqual(0.5);
  // The accent indicator spans the text, not the padding.
  expect(Math.abs(ind!.x - textLeft)).toBeLessThanOrEqual(0.5);
  expect(Math.abs(ind!.x + ind!.width - textRight)).toBeLessThanOrEqual(0.5);
});

test("a mouse click leaves no ring on the tab (keyboard only)", async ({ page }) => {
  await openScene(page, "ingame");
  const tab = page.locator(".tab.on");
  await tab.click();
  expect(await focusVisible(tab)).toBe(false);
  expect((await outline(tab)).style).toBe("none");
});

test("Recent rows: an inset, whole ring on the first and the last row (the panel clips)", async ({ page }) => {
  await openScene(page, "details-more"); // 8 rows: the last one touches the panel's rounded corners
  const rows = page.locator(".details .recent .rrow");
  await expect(rows).toHaveCount(8);
  for (const [name, row] of [["first", rows.first()], ["last", rows.last()]] as const) {
    await row.scrollIntoViewIfNeeded();
    await row.focus();
    expect(await focusVisible(row), name).toBe(true);
    expect((await outline(row)).offset, name).toBe("-2px");
    expectFullRing(await ringSides(page, row), `recent ${name} row`);
  }
});

test("Encounter rows: an inset, whole ring on the first and the last row", async ({ page }) => {
  await openScene(page, "details");
  const rows = page.locator(".details .encs .erow");
  await expect(rows).toHaveCount(3);
  for (const [name, row] of [["first", rows.first()], ["last", rows.last()]] as const) {
    await row.scrollIntoViewIfNeeded();
    await row.focus();
    expect(await focusVisible(row), name).toBe(true);
    expect((await outline(row)).offset, name).toBe("-2px");
    expectFullRing(await ringSides(page, row), `encounter ${name} row`);
  }
});

test("rows that are not clipped keep the 2px outside ring; the mouse look is unchanged", async ({ page }) => {
  await openScene(page, "ingame");
  const card = page.locator(".live .card").first();
  await card.focus();
  expect(await focusVisible(card)).toBe(true);
  expect((await outline(card)).offset).toBe("2px");
  await openScene(page, "details");
  const back = page.locator(".details .back");
  await back.focus();
  expect(await focusVisible(back)).toBe(true);
  expect((await outline(back)).offset).toBe("2px");
});
