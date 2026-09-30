// Player Details and Match Details against a reference render of the user's design file (tests/design-ref.ts), pixel
// for pixel at a per-pixel YIQ threshold of 0.2, the same gate as visual-live. The design is clicked through like a
// user (Mara's card, Show more, recent row 0, encounter row 0, Cypher's card); ours opens the matching scene. "Bottom"
// sets the page's scroll container to scrollTop = scrollHeight in both renders; only details-more is taller than the
// window, so it is the one bottom case (details and match fit in 800px: their bottom frame is their top frame, Task 18
// review, Minor 12), and each bottom case asserts that both renders really scrolled. The reference PNGs are
// regenerated on every run (beforeAll).
//
// What differs on purpose, and nothing else:
//   - the ⟲ glyph (COMPARE_STYLE hides it in both renders; the count beside it is compared). In the hero the count
//     follows the glyph (the card right-aligns it), and the design's glyph is a system-font character where ours is
//     the spec §3.5 13px SVG, so the count lands 1px apart: the measured baseline is 28 px in the three Details cases
//     (all of it that one digit), 0 px in details-more (bottom), in details-hidden (Cypher has no ⟲) and in both Match
//     cases;
//   - details-more: the recent rows' "time ago" labels, hidden in both renders. The design mock hard-codes
//     `1d / 1d / 3d ago` for rows 4, 5 and 7 (v8:451), which its own dates (v8:472) do not give at the page clock:
//     ours derive `22h / 23h / 2d ago` from the start times. The first three rows (all that `details` shows) agree.
//   - match-enc (the Match page opened from Mara's first encounter row) is compared too, top only (it fits the window):
//     the header date has no year in the current year, as the design writes it (v8:642, v8:685; R140).
//   - details-hidden: Cypher, whose name is hidden on the live screen: the hero's eye-off icon and italic agent name
//     (v8:188-190), compared like the rest.
import { expect, test, type Page } from "@playwright/test";
import { COMPARE_STYLE, ensureCdnCache, findDesignDir, openDesign, type DesignStep } from "./design-ref";
import { openScene, settle } from "./harness";

test.describe.configure({ mode: "serial" });

/** 0.02% of the 800 000 pixels (160 px), as visual-live. */
const MAX_DIFF_RATIO = 0.0002;

const AGO_HIDE = '[data-tip$=" AM"],[data-tip$=" PM"]{visibility:hidden!important}';

type Case = { label: string; file: string; scene: string; steps: DesignStep[]; bottom: boolean; style: string };

const CASES: Case[] = [
  { label: "details (top)", file: "details-top", scene: "details", steps: ["mara"], bottom: false, style: COMPARE_STYLE },
  { label: "details-more (top)", file: "details-more-top", scene: "details-more", steps: ["mara", "more"], bottom: false, style: `${COMPARE_STYLE}\n${AGO_HIDE}` },
  { label: "details-more (bottom)", file: "details-more-bottom", scene: "details-more", steps: ["mara", "more"], bottom: true, style: `${COMPARE_STYLE}\n${AGO_HIDE}` },
  { label: "details-hidden (top)", file: "details-hidden-top", scene: "details-hidden", steps: ["cypher"], bottom: false, style: COMPARE_STYLE },
  { label: "match (top)", file: "match-top", scene: "match", steps: ["mara", "recent"], bottom: false, style: COMPARE_STYLE },
  { label: "match-enc (top)", file: "match-enc-top", scene: "match-enc", steps: ["mara", "encounter"], bottom: false, style: COMPARE_STYLE },
];

/**
 * Scroll every rendered scroll container of the page (the design's raw <x-dc> template copy excluded) to its end.
 * Returns the largest scrollTop reached: 0 means the page fits and "bottom" would repeat the top frame.
 */
async function toBottom(page: Page, selector: string): Promise<number> {
  const top = await page.evaluate((sel) => {
    let max = 0;
    for (const el of document.querySelectorAll<HTMLElement>(sel)) {
      if (el.closest("x-dc")) continue;
      el.scrollTop = el.scrollHeight;
      max = Math.max(max, el.scrollTop);
    }
    return max;
  }, selector);
  await settle(page);
  return top;
}

/** The design's scrollTop for each bottom case (beforeAll), to hold ours to the same distance. */
const designScroll = new Map<string, number>();

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
    await openDesign(page, DESIGN_DIR!, {}, c.steps);
    if (c.bottom) designScroll.set(c.file, await toBottom(page, "[data-screen-label]"));
    await page.screenshot({ path: testInfo.snapshotPath(`${c.file}.png`), style: c.style });
    await page.close();
  }
  await context.close();
});

for (const c of CASES) {
  test(`design-ref: ${c.label}`, async ({ page }) => {
    await openScene(page, c.scene);
    await page.mouse.move(5, 5);
    if (c.bottom) {
      // A different frame from the top case in both renders, scrolled by the same distance.
      const ours = await toBottom(page, "#app .scroll");
      expect(designScroll.get(c.file), "the design scrolled").toBeGreaterThan(0);
      expect(ours, "ours scrolled").toBeGreaterThan(0);
      expect(Math.abs(ours - designScroll.get(c.file)!), "the same scroll distance").toBeLessThanOrEqual(1);
    }
    expect(await page.screenshot({ style: c.style })).toMatchSnapshot(`${c.file}.png`, { maxDiffPixelRatio: MAX_DIFF_RATIO, threshold: 0.2 });
  });
}
