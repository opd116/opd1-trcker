// Our Live page against a reference render of the user's design file (tests/design-ref.ts), pixel for pixel, at a
// per-pixel YIQ threshold of 0.2. The reference PNGs are regenerated from the design on every run (beforeAll)
// into tests/ref/ (git-ignored). Serial, so no two workers write the same reference PNG.
//
// What differs on purpose, and nothing else:
//   - the ⟲ glyph (COMPARE_STYLE hides it in both renders; the count beside it is compared);
//   - R125: the card's Prev row, masked in both renders (design-ref.ts). The masked rects must match exactly.
import { readFileSync, writeFileSync } from "node:fs";
import { expect, test } from "@playwright/test";
import {
  COMPARE_STYLE,
  designPrevRows,
  ensureCdnCache,
  findDesignDir,
  MASK_COLOR,
  openDesign,
  ourPrevRows,
  rectGap,
  rectsOf,
  type DesignProps,
  type Rect,
} from "./design-ref";
import { openScene, type ThemeParams } from "./harness";

test.describe.configure({ mode: "serial" });

/**
 * At most 0.02% of the 800 000 pixels (160 px) may differ. Measured baseline with the two hides above: 20–40 px in
 * each of the 12 In Game cases (IBM Plex Sans 20, Barlow 36, Archivo 40) and 0 in `ingame-loading`. All of it is
 * Rook's dotted underline (R128: our remembered name draws it, the design's mock clips it away by accident), which
 * is a deliberate, stable difference, so it is left in the comparison rather than masked. 160 px is 4× the worst
 * case and still fails a lost self outline (~1.2k px), a missing arrow row (~1.4k px), a Peak weight change or
 * one card's tier name. It replaces the plan's first 1% (8 000 px), which was set before the baseline was known.
 */
const MAX_DIFF_RATIO = 0.0002;

type Case = { label: string; file: string; scene: string; params: ThemeParams; props: DesignProps };

const opt = (label: string, file: string, params: ThemeParams, props: DesignProps): Case => ({ label, file, scene: "ingame", params, props });

const CASES: Case[] = [
  opt("ingame [default]", "ingame-default", {}, {}),
  opt("ingame [Underline]", "ingame-underline", { last5: "Underline" }, { last5Style: "Underline" }),
  opt("ingame [Solid]", "ingame-solid", { fill: "Solid" }, { resultFill: "Solid" }),
  opt("ingame [Accent / Red]", "ingame-accent-red", { results: "Accent / Red" }, { results: "Accent / Red" }),
  opt("ingame [Accent / Gray]", "ingame-accent-gray", { results: "Accent / Gray" }, { results: "Accent / Gray" }),
  opt("ingame [Cool]", "ingame-cool", { tone: "Cool" }, { tone: "Cool" }),
  opt("ingame [Warm]", "ingame-warm", { tone: "Warm" }, { tone: "Warm" }),
  ...(["#4fc3ae", "#7fb1ef", "#e58a9a"] as const).map((a) => opt(`ingame [${a}]`, `ingame-accent-${a.slice(1)}`, { accent: a }, { accent: a })),
  opt("ingame [IBM Plex Sans]", "ingame-ibm-plex-sans", { font: "IBM Plex Sans" }, { font: "IBM Plex Sans" }),
  opt("ingame [Archivo]", "ingame-archivo", { font: "Archivo" }, { font: "Archivo" }),
  { label: "ingame-loading", file: "ingame-loading", scene: "ingame-loading", params: {}, props: { loading: true } },
];

const DESIGN_DIR = findDesignDir();

test.beforeAll(async ({ browser }, testInfo) => {
  test.skip(!DESIGN_DIR, "design file not on this machine");
  test.skip(!(await ensureCdnCache()), "design CDN cache empty");
  const context = await browser.newContext({
    viewport: { width: 1000, height: 800 },
    deviceScaleFactor: 1,
    timezoneId: "UTC",
    locale: "en-US",
  });
  for (const c of CASES) {
    const page = await context.newPage();
    await openDesign(page, DESIGN_DIR!, c.props);
    const mask = designPrevRows(page);
    await page.screenshot({ path: testInfo.snapshotPath(`${c.file}.png`), style: COMPARE_STYLE, mask: [mask], maskColor: MASK_COLOR });
    writeFileSync(testInfo.snapshotPath(`${c.file}.prev.json`), JSON.stringify(await rectsOf(mask)));
    await page.close();
  }
  await context.close();
});

for (const c of CASES) {
  test(`design-ref: ${c.label}`, async ({ page }, testInfo) => {
    await openScene(page, c.scene, c.params);
    await page.mouse.move(5, 5);
    const mask = ourPrevRows(page);
    // The R125 mask covers exactly the design's Prev cells, no more: same rects in both renders (none when loading).
    const designRects = JSON.parse(readFileSync(testInfo.snapshotPath(`${c.file}.prev.json`), "utf8")) as Rect[];
    expect(designRects).toHaveLength(c.scene === "ingame" ? 10 : 0);
    const ours = await rectsOf(mask);
    expect(rectGap(ours, designRects), `R125 mask rects ${JSON.stringify(ours)} vs the design's ${JSON.stringify(designRects)}`).toBeLessThanOrEqual(0.5);
    expect(await page.screenshot({ style: COMPARE_STYLE, mask: [mask], maskColor: MASK_COLOR })).toMatchSnapshot(`${c.file}.png`, {
      maxDiffPixelRatio: MAX_DIFF_RATIO,
      threshold: 0.2,
    });
  });
}
