// Visual regression baselines (spec §8.7) for the v2 screens: the main states against committed PNGs in
// ui/tests/baselines/, at 1280×800 (the glance column at 420×800), DPR 1, the fixed page clock. Pixels depend on the
// browser build and its fonts, so the PNGs are made only in the pinned Playwright image (the manual `Build` run with
// `update_baselines`, job `baselines-render`) and compared only there (CI sets VISUAL_BASELINES=1). On a developer
// machine the whole file skips: a local Chrome differs from the image's, and a local run must never write or compare
// the committed PNGs. A state whose PNG has not been generated yet skips too (it is not a failure): the v2 set is
// generated once by that manual run, and a state added later skips until the next one.
import { existsSync } from "node:fs";
import { expect, test, type Page } from "@playwright/test";
import { PUUID } from "../src/lib/demo/cdv8";
import { RECAP_MATCH_ID } from "../src/lib/demo/v2/status";
import { checkFonts } from "./checks";
import { openScene, pointerAway, SIZES } from "./harness";

test.skip(process.env.VISUAL_BASELINES !== "1", "visual baselines are rendered and compared only in the pinned Playwright image (VISUAL_BASELINES=1)");

type Baseline = {
  /** The PNG is `<name>.png`. */
  name: string;
  /** What the state shows (the test title). */
  what: string;
  scene: string;
  params?: Record<string, string>;
  size?: keyof typeof SIZES;
  /** At least this many images (`<img>` plus CSS background pictures) must be on screen and loaded: never vacuous. */
  images: number;
};

const BASELINES: Baseline[] = [
  { name: "live-ingame", what: "Live, in game (two teams of five)", scene: "ingame", images: 22 },
  { name: "live-pregame", what: "Live, Agent Select (your team, the enemy placeholder)", scene: "pregame", images: 9 },
  { name: "live-waiting", what: "Live, waiting for VALORANT (closed)", scene: "closed", images: 3 },
  { name: "live-lobby", what: "Live, in the lobby with a party of 3, in queue", scene: "lobby3", params: { queue: "1" }, images: 8 },
  { name: "live-recap", what: "Live, the recap of the user's last match", scene: "closed", params: { recap: "1" }, images: 23 },
  { name: "live-glance", what: "Live, glance mode (420px column)", scene: "ingame", params: { glance: "1" }, size: "glance", images: 23 },
  { name: "profile", what: "the profile drawer (Mara) over Live", scene: "ingame", params: { profile: PUUID.mara! }, images: 71 },
  { name: "saved", what: "Saved players", scene: "closed", params: { screen: "saved" }, images: 5 },
  { name: "history", what: "Match history", scene: "closed", params: { screen: "history" }, images: 34 },
  {
    name: "scoreboard",
    what: "a scoreboard opened from History",
    scene: "closed",
    params: { screen: "history", match: RECAP_MATCH_ID, puuid: PUUID.fahad! },
    images: 22,
  },
  { name: "comp", what: "Competitive", scene: "mystats", images: 18 },
  { name: "skins", what: "Collection", scene: "closed", params: { screen: "skins" }, images: 33 },
  { name: "ascii", what: "ASCII studio", scene: "closed", params: { screen: "ascii" }, images: 1 },
  { name: "settings", what: "Settings", scene: "closed", params: { screen: "settings" }, images: 1 },
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

for (const b of BASELINES) {
  test(`baseline: ${b.name} (${b.what})`, async ({ page }, testInfo) => {
    // Not generated yet: skipped, not failed. A run with --update-snapshots (the `update_baselines` job) writes it.
    const png = testInfo.snapshotPath(`${b.name}.png`, { kind: "screenshot" });
    const updating = testInfo.config.updateSnapshots === "all" || testInfo.config.updateSnapshots === "changed";
    test.skip(!existsSync(png) && !updating, `${b.name}.png has not been generated yet (run the Build workflow with update_baselines)`);

    await page.setViewportSize(SIZES[b.size ?? "default"]);
    await openScene(page, b.scene, b.params);
    expect(await checkFonts(page)).toEqual([]);

    // All pictures are in before the shot: a row whose icon is still on its way would be a different image.
    const { bad, count } = await imageReport(page);
    expect(bad, "images that did not load").toEqual([]);
    expect(count, "images on screen").toBeGreaterThanOrEqual(b.images);

    // No tooltip, no hover colour, no focus ring: the pointer rests where nothing reacts.
    await pointerAway(page);
    await expect(page.locator("#tooltip")).toHaveCount(0);

    // The viewport only, never the full page: the scrolling screens scroll inside it.
    await expect(page).toHaveScreenshot(`${b.name}.png`);
  });
}
