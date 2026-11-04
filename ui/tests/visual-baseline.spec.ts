// R211: the user approved Compact as the sole current design, superseding the v2 visual reference.
// Active PNGs live in baselines/compact/ at BOTH 700×860 and 680×760, plus square 420×420 Glance.
// Root-level v2 PNGs are retained as history, not rewritten or used to judge the approved redesign.
// Render/compare only in the pinned Playwright image (VISUAL_BASELINES=1). Missing active references FAIL;
// they never silently skip CI. Intentional reference generation uses the existing manual workflow.
import { existsSync } from "node:fs";
import { expect, test, type Page } from "@playwright/test";
import { PUUID } from "../src/lib/demo/cdv8";
import { RECAP_MATCH_ID } from "../src/lib/demo/v2/status";
import { checkFonts } from "./checks";
import { COMPACT_SIZES, openScene, pointerAway } from "./harness";

test.skip(process.env.VISUAL_BASELINES !== "1", "visual baselines are rendered and compared only in the pinned Playwright image (VISUAL_BASELINES=1)");

type Baseline = {
  /** The PNG is `<name>.png`. */
  name: string;
  /** What the state shows (the test title). */
  what: string;
  scene: string;
  params?: Record<string, string>;
  size?: "glance";
  peak?: boolean;
  heading?: string;
  /** At least this many images (`<img>` plus CSS background pictures) must be on screen and loaded: never vacuous. */
  images: number;
};

const BASELINES: Baseline[] = [
  { name: "live-ingame", what: "Live, in game (two teams of five)", scene: "ingame", images: 22 },
  { name: "live-pregame", what: "Live, Agent Select (your team, the enemy placeholder)", scene: "pregame", images: 9 },
  { name: "live-peak", what: "Live, Peak rank and act age", scene: "ingame", peak: true, images: 22 },
  // R205 removed the own-rank image from the shell: these fixture minima account for that intentional removal.
  { name: "live-waiting", what: "Live, waiting for VALORANT (closed)", scene: "closed", images: 2 },
  { name: "live-lobby", what: "Live, in the lobby with a party of 3, in queue", scene: "lobby3", params: { queue: "1" }, images: 7 },
  { name: "live-recap", what: "Live, the recap of the user's last match", scene: "closed", params: { recap: "1" }, images: 22 },
  { name: "live-glance", what: "Live, square Glance", scene: "ingame", params: { glance: "1" }, size: "glance", images: 23 },
  { name: "profile", what: "the profile drawer (Mara) over Live", scene: "ingame", params: { profile: PUUID.mara! }, images: 71 },
  { name: "profile-hidden", what: "profile without encounters", scene: "details-hidden", images: 1 },
  { name: "saved", what: "Saved players", scene: "closed", params: { screen: "saved" }, images: 4 },
  { name: "history", what: "Match history", scene: "closed", params: { screen: "history" }, images: 33 },
  {
    name: "scoreboard",
    what: "a scoreboard opened from History",
    scene: "closed",
    params: { screen: "history", match: RECAP_MATCH_ID, puuid: PUUID.fahad! },
    images: 21,
  },
  { name: "comp", what: "Competitive", scene: "mystats", images: 17 },
  { name: "skins", what: "Collection", scene: "closed", params: { screen: "skins" }, images: 32 },
  // These views intentionally contain no raster art. Assert their real content below, not a removed shell image.
  { name: "ascii", what: "ASCII studio", scene: "closed", params: { screen: "ascii" }, images: 0 },
  { name: "settings", what: "Settings", scene: "closed", params: { screen: "settings" }, heading: "Settings", images: 0 },
  { name: "about", what: "About", scene: "closed", params: { screen: "about" }, heading: "About", images: 0 },
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
  const sizes = b.size === "glance" ? [COMPACT_SIZES.glance] : [COMPACT_SIZES.default, COMPACT_SIZES.min];
  for (const size of sizes) test(`baseline: ${b.name} ${size.width}×${size.height} (${b.what})`, async ({ page }, testInfo) => {
    const name = `compact/${b.name}-${size.width}x${size.height}.png`;
    const png = testInfo.snapshotPath(name, { kind: "screenshot" });
    const updating = testInfo.config.updateSnapshots === "all" || testInfo.config.updateSnapshots === "changed";
    if (!updating) expect(existsSync(png), `required Compact reference is missing: ${name}`).toBe(true);

    await page.setViewportSize(size);
    await openScene(page, b.scene, b.params);
    if (b.peak) {
      await page.getByRole("button", { name: "Rank view Current", exact: true }).click();
      await expect(page.locator(".peak-rank").first()).toHaveCSS("opacity", "1");
    }
    if (b.heading) await expect(page.getByRole("heading", { name: b.heading, exact: true })).toBeVisible();
    if (b.name === "ascii") {
      const canvas = page.getByRole("application", { name: /ASCII art/ });
      await expect(canvas).toBeVisible();
      expect((await canvas.textContent())?.trim().length, "the ASCII drawing must not be blank").toBeGreaterThan(0);
    }
    if (b.name === "profile-hidden") {
      await expect(page.getByRole("dialog")).toHaveAttribute("aria-label", "Hidden player profile");
      await expect(page.getByRole("region", { name: "Encounters", exact: true })).toHaveCount(0);
      await expect(page.getByRole("region", { name: "Recent matches", exact: true }).locator(".row")).toHaveCount(6);
    }
    expect(await checkFonts(page)).toEqual([]);

    // All pictures are in before the shot: a row whose icon is still on its way would be a different image.
    const { bad, count } = await imageReport(page);
    expect(bad, "images that did not load").toEqual([]);
    expect(count, "images on screen").toBeGreaterThanOrEqual(b.images);

    // No tooltip, no hover colour, no focus ring: the pointer rests where nothing reacts.
    await pointerAway(page);
    await expect(page.locator("#tooltip")).toHaveCount(0);
    await testInfo.attach("visual-readiness", { body: JSON.stringify({ name, viewport: size, dpr: await page.evaluate(() => devicePixelRatio), loadedImages: count, badImages: bad }), contentType: "application/json" });

    // The viewport only, never the full page: the scrolling screens scroll inside it.
    await expect(page).toHaveScreenshot(name);
  });
}
