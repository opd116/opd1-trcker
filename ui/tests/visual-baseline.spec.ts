// Visual regression baselines (spec §8.7): the main scenes against committed PNGs in ui/tests/baselines/, at 1000×800,
// DPR 1, Barlow, the fixed page clock, the default theme. Pixels depend on the browser build and its fonts, so the PNGs are
// made only in the pinned Playwright image (the manual `Build` run with `update_baselines`, job `baselines-render`) and
// compared only there (CI sets VISUAL_BASELINES=1). On a developer machine the whole file skips: this container's Chromium
// differs from the image's, and a local run must never write or compare the committed PNGs.
import { expect, test, type Page } from "@playwright/test";
import { checkFont } from "./checks";
import { openScene, pointerAway } from "./harness";

test.skip(process.env.VISUAL_BASELINES !== "1", "visual baselines are rendered and compared only in the pinned Playwright image (VISUAL_BASELINES=1)");

type Baseline = {
  /** The DEV demo scene (`?scene=`, spec §8.2); the PNG is `<scene>.png`. */
  scene: string;
  /** What the scene shows (the test title). */
  what: string;
  /** At least this many images (`<img>` plus CSS background pictures) must be on screen and loaded: the check is never vacuous. */
  images: number;
};

const BASELINES: Baseline[] = [
  { scene: "ingame", what: "Live, In Game (the design's roster: 10 cards)", images: 40 },
  { scene: "pregame", what: "Live, Agent Select (5 cards, 5 empty slots)", images: 15 },
  { scene: "lobby3", what: "Live, Lobby with 3 players", images: 8 },
  { scene: "details", what: "Player Details (Mara)", images: 30 },
  { scene: "match", what: "Match Details (from a recent row)", images: 15 },
  { scene: "mystats", what: "My Stats, full", images: 12 },
  { scene: "mystats-new", what: "My Stats, new account", images: 0 },
];

/**
 * Every `<img>` and every CSS background picture under `#app` is probed once more (the page cached it, so this is
 * instant): each must be `complete` with a `naturalWidth` above 0. Returns the failures and how many were checked.
 */
async function imageReport(page: Page): Promise<{ bad: string[]; count: number }> {
  return page.evaluate(async () => {
    const bad: string[] = [];
    let count = 0;
    for (const img of document.images) {
      count++;
      if (!(img.complete && img.naturalWidth > 0)) bad.push(`<img> ${img.currentSrc || img.src}`);
    }
    const urls = new Set<string>();
    for (const el of document.querySelectorAll<HTMLElement>("#app *")) {
      for (const m of getComputedStyle(el).backgroundImage.matchAll(/url\("?([^")]+)"?\)/g)) urls.add(m[1]!);
    }
    await Promise.all(
      [...urls].map(
        (u) =>
          new Promise<void>((resolve) => {
            const i = new Image();
            const done = () => {
              count++;
              if (!(i.complete && i.naturalWidth > 0)) bad.push(`background ${u}`);
              resolve();
            };
            i.addEventListener("load", done, { once: true });
            i.addEventListener("error", done, { once: true });
            i.src = u;
          }),
      ),
    );
    return { bad, count };
  });
}

for (const { scene, what, images } of BASELINES) {
  test(`baseline: ${scene} (${what})`, async ({ page }) => {
    await openScene(page, scene, { font: "Barlow" });
    expect(await checkFont(page, "Barlow")).toEqual([]);

    // All pictures are in before the shot: a card whose art or icon is still on its way would be a different image.
    const { bad, count } = await imageReport(page);
    expect(bad, "images that did not load").toEqual([]);
    expect(count, "images on screen").toBeGreaterThanOrEqual(images);

    // No tooltip, no hover colour, no focus ring: the pointer rests where nothing reacts.
    await pointerAway(page);
    await expect(page.locator("#app [role=tooltip]")).toHaveCount(0);

    // The viewport only (1000×800), never the full page: Details, Match and My Stats scroll inside it.
    await expect(page).toHaveScreenshot(`${scene}.png`);
  });
}
