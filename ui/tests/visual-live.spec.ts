// Our Live page against a reference render of the user's design file (tests/design-ref.ts), pixel for pixel:
// at most 1% of the 800 000 pixels may differ, at a per-pixel YIQ threshold of 0.2. The reference PNGs are
// regenerated from the design on every run (beforeAll) into tests/ref/ (git-ignored). Serial, so no two
// workers write the same reference PNG.
import { expect, test } from "@playwright/test";
import { COMPARE_STYLE, ensureCdnCache, findDesignDir, openDesign, type DesignProps } from "./design-ref";
import { openScene, type ThemeParams } from "./harness";

test.describe.configure({ mode: "serial" });

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
    await page.screenshot({ path: testInfo.snapshotPath(`${c.file}.png`), style: COMPARE_STYLE });
    await page.close();
  }
  await context.close();
});

for (const c of CASES) {
  test(`design-ref: ${c.label}`, async ({ page }) => {
    await openScene(page, c.scene, c.params);
    await page.mouse.move(5, 5);
    expect(await page.screenshot({ style: COMPARE_STYLE })).toMatchSnapshot(`${c.file}.png`, {
      maxDiffPixelRatio: 0.01,
      threshold: 0.2,
    });
  });
}
