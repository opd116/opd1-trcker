// Spec §8.1/§8.6: the tight edges must hold on a scaled Windows display too (125–150%). Checks 2, 5 and 6 at DPR 1.5.
import { expect, test } from "@playwright/test";
import { checkCardOverlap, checkClipping, checkLiveNoOverflow } from "./checks";
import { FONTS, openScene } from "./harness";

test.use({ deviceScaleFactor: 1.5 });

for (const scene of ["ingame-longnames", "pregame"]) {
  for (const font of FONTS) {
    test(`dpr1.5: ${scene} [${font}]`, async ({ page }) => {
      await openScene(page, scene, { font });
      expect(await page.evaluate(() => window.devicePixelRatio)).toBe(1.5);
      const problems = [
        ...(await checkLiveNoOverflow(page)).map((p) => `2: ${p}`),
        ...(await checkClipping(page)).map((p) => `5: ${p}`),
        ...(await checkCardOverlap(page)).map((p) => `6: ${p}`),
      ];
      expect(problems).toEqual([]);
    });
  }
}
