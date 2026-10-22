// R106 focus ring on the v2 screens: keyboard-only (`:focus-visible`), never left behind by a mouse click, and whole on
// every side: a row inside a clipping panel (the board, the History list, Saved, the drawer's encounter rows) and a
// control touching the window's edge (the nav rail items, the title bar's buttons) draw it inset (offset −2px),
// anything else 2px outside, where nothing may clip it. The probe
// compares the element with and without its ring and counts the pixels that differ in a 4px band along each edge
// (inside for an inset ring, outside for an outer one): a side that is clipped has none.
import { expect, test, type Locator, type Page } from "@playwright/test";
import { PUUID } from "../src/lib/demo/cdv8";
import { openScene } from "./harness";

type Sides = { top: number; right: number; bottom: number; left: number; w: number; h: number };

/** How many pixels the ring paints in the 4px band inside (or, with `outer`, just outside) each straight edge of `el`. */
async function ringSides(page: Page, el: Locator, outer: boolean): Promise<Sides> {
  const r = await el.boundingBox();
  if (!r) throw new Error("no box");
  const vp = page.viewportSize()!;
  const x0 = Math.round(r.x);
  const y0 = Math.round(r.y);
  const w = Math.round(r.width);
  const h = Math.round(r.height);
  const clip = { x: Math.max(0, x0 - 6), y: Math.max(0, y0 - 6) };
  const box = { ...clip, width: Math.min(vp.width - clip.x, x0 + w + 6 - clip.x), height: Math.min(vp.height - clip.y, y0 + h + 6 - clip.y) };
  const withRing = await page.screenshot({ clip: box });
  const off = await page.addStyleTag({ content: ":focus-visible{outline:none!important}" });
  const without = await page.screenshot({ clip: box });
  await off.evaluate((s) => (s as Element).remove());
  return page.evaluate(
    async ([a, b, geo]) => {
      const px = async (b64: string) => {
        const bytes = Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
        const bmp = await createImageBitmap(new Blob([bytes], { type: "image/png" }));
        const cv = new OffscreenCanvas(bmp.width, bmp.height);
        const ctx = cv.getContext("2d")!;
        ctx.drawImage(bmp, 0, 0);
        return { d: ctx.getImageData(0, 0, bmp.width, bmp.height).data, bw: bmp.width, bh: bmp.height };
      };
      const [A, B] = [await px(a!), await px(b!)];
      const diff = (x: number, y: number) => {
        if (x < 0 || y < 0 || x >= A.bw || y >= A.bh) return false; // outside the window: nothing can paint there
        const i = (y * A.bw + x) * 4;
        return Math.abs(A.d[i]! - B.d[i]!) + Math.abs(A.d[i + 1]! - B.d[i + 1]!) + Math.abs(A.d[i + 2]! - B.d[i + 2]!) > 24;
      };
      const { x, y, w, h, outer } = geo as { x: number; y: number; w: number; h: number; outer: boolean };
      const out = { top: 0, right: 0, bottom: 0, left: 0, w, h };
      const at = (edge: "top" | "right" | "bottom" | "left", along: number, k: number): [number, number] => {
        const d = outer ? -1 - k : k;
        return edge === "top" ? [x + along, y + d] : edge === "bottom" ? [x + along, y + h - 1 - d] : edge === "left" ? [x + d, y + along] : [x + w - 1 - d, y + along];
      };
      const c = 6; // skip the rounded corners
      for (const edge of ["top", "bottom", "left", "right"] as const) {
        const len = edge === "top" || edge === "bottom" ? w : h;
        for (let along = c; along < len - c; along++) {
          for (let k = 0; k < 4; k++) {
            const [qx, qy] = at(edge, along, k);
            if (diff(qx, qy)) {
              out[edge]++;
              break;
            }
          }
        }
      }
      return out;
    },
    [withRing.toString("base64"), without.toString("base64"), { x: x0 - box.x, y: y0 - box.y, w, h, outer }] as const,
  );
}

/** Every side's straight part carries the ring (at least 80% of it). */
function expectWhole(s: Sides, what: string) {
  for (const side of ["top", "bottom"] as const) expect(s[side], `${what}: ${side} edge`).toBeGreaterThanOrEqual(Math.floor((s.w - 12) * 0.8));
  for (const side of ["left", "right"] as const) expect(s[side], `${what}: ${side} edge`).toBeGreaterThanOrEqual(Math.floor((s.h - 12) * 0.8));
}

const focusVisible = (l: Locator) => l.evaluate((el) => el.matches(":focus-visible"));
const outlineOf = (l: Locator) =>
  l.evaluate((el) => {
    const c = getComputedStyle(el);
    return { style: c.outlineStyle, width: c.outlineWidth, offset: parseFloat(c.outlineOffset) };
  });

/** Keyboard-focus `el` (Tab first, so a script focus counts as keyboard modality) and check its ring is whole. */
async function expectRing(page: Page, el: Locator, what: string): Promise<void> {
  // The tooltip a focused trigger shows after 450 ms must not land in the shots.
  const noTip = await page.addStyleTag({ content: "#tooltip{display:none!important}" });
  await el.scrollIntoViewIfNeeded();
  await el.focus();
  await expect(el).toBeFocused();
  expect(await focusVisible(el), `${what}: :focus-visible`).toBe(true);
  const o = await outlineOf(el);
  expect(o.style, what).toBe("solid");
  expect(o.width, what).toBe("2px");
  expectWhole(await ringSides(page, el, o.offset >= 0), what);
  await noTip.evaluate((s) => (s as Element).remove());
}

type Target = {
  name: string;
  scene: string;
  params?: Record<string, string>;
  el: (page: Page) => Locator;
  /** An inset ring (rows inside a clipping list): offset −2px. */
  inset?: boolean;
};

const MARA = PUUID.mara!;
const FAHAD = PUUID.fahad!;
const rail = (label: string) => (page: Page) => page.locator(`nav.rail button[aria-label="${label}"]`);
const titleButton = (label: string) => (page: Page) => page.locator(`#app > header button[aria-label="${label}"]`);
const scoreboardRows = (page: Page) => page.getByRole("dialog").getByRole("button", { name: /Open scoreboard$/ });

const TARGETS: Target[] = [
  { name: "nav rail: Live (the first item)", scene: "ingame", el: rail("Live"), inset: true },
  { name: "nav rail: About (the last item)", scene: "ingame", el: rail("About"), inset: true },
  { name: "nav rail: the rank widget", scene: "ingame", el: rail("Your rank: open Competitive") },
  { name: "title bar: Minimize", scene: "ingame", el: titleButton("Minimize"), inset: true },
  { name: "title bar: Close (the window's right edge)", scene: "ingame", el: titleButton("Close"), inset: true },
  { name: "board: the first row of a team", scene: "ingame", el: (p) => p.locator("main section.team").first().locator("[data-row]").first(), inset: true },
  { name: "board: the last row of a team", scene: "ingame", el: (p) => p.locator("main section.team").first().locator("[data-row]").last(), inset: true },
  { name: "history: the first row", scene: "closed", params: { screen: "history" }, el: (p) => p.locator("main .row[data-row]").first(), inset: true },
  { name: "history: the last row (below the fold)", scene: "closed", params: { screen: "history" }, el: (p) => p.locator("main .row[data-row]").last(), inset: true },
  { name: "saved: the first row", scene: "closed", params: { screen: "saved" }, el: (p) => p.locator("main .row[data-row]").first(), inset: true },
  { name: "saved: the last row", scene: "closed", params: { screen: "saved" }, el: (p) => p.locator("main .row[data-row]").last(), inset: true },
  { name: "drawer: the first encounter row", scene: "ingame", params: { profile: MARA }, el: (p) => scoreboardRows(p).first(), inset: true },
  { name: "drawer: the last encounter row", scene: "ingame", params: { profile: MARA }, el: (p) => scoreboardRows(p).last(), inset: true },
  { name: "drawer: the user's first recent row", scene: "ingame", params: { profile: FAHAD }, el: (p) => scoreboardRows(p).first(), inset: true },
  { name: "drawer: the user's last recent row", scene: "ingame", params: { profile: FAHAD }, el: (p) => scoreboardRows(p).last(), inset: true },
  { name: "scoreboard: Back", scene: "match", el: (p) => p.locator("main .match .back") },
];

for (const t of TARGETS) {
  test(`a whole keyboard ring: ${t.name}`, async ({ page }) => {
    await openScene(page, t.scene, t.params);
    await page.keyboard.press("Tab"); // keyboard modality: the script focus below is a :focus-visible one
    const el = t.el(page);
    await expectRing(page, el, t.name);
    expect((await outlineOf(el)).offset, `${t.name}: offset`).toBe(t.inset ? -2 : 2);
  });
}

test("a mouse click leaves no ring (on a row, a nav item and Back)", async ({ page }) => {
  await openScene(page, "closed", { screen: "history" });
  const nav = page.locator('nav.rail button[aria-label="History"]');
  await nav.click();
  expect(await focusVisible(nav)).toBe(false);
  expect((await outlineOf(nav)).style).toBe("none");
  const row = page.locator("main .row[data-row]").first();
  await row.click();
  const back = page.locator("main .match .back");
  await expect(back).toBeVisible();
  await back.click();
  await expect(row).toBeVisible();
  for (const el of [row, nav]) {
    if (await el.evaluate((e) => e === document.activeElement)) expect(await focusVisible(el)).toBe(false);
  }
});
