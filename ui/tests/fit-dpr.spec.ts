// Spec §8.1/§8.6: the tight edges must hold on a scaled Windows display too (125–150%). Checks 2, 5 and 6 at DPR 1.5,
// plus the R125/R126 Prev row with every tier name (ingame-longnames holds `Prev` + `Ascendant 3` on every card).
// Details and Match (Task 18): checks 3 and 5 on their long-name scenes, the tightest rows those pages have.
import { expect, test } from "@playwright/test";
import { checkCardOverlap, checkClipping, checkLiveNoOverflow, checkPrevNames, checkScrollX } from "./checks";
import { FONTS, openScene, TIER_LABELS } from "./harness";

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
        ...(await checkPrevNames(page, TIER_LABELS)).map((p) => `R126: ${p}`),
      ];
      expect(problems).toEqual([]);
    });
  }
}

for (const scene of ["details-longnames", "match-longnames", "details-nostats"]) {
  for (const font of FONTS) {
    test(`dpr1.5: ${scene} [${font}]`, async ({ page }) => {
      await openScene(page, scene, { font });
      expect(await page.evaluate(() => window.devicePixelRatio)).toBe(1.5);
      const problems = [...(await checkScrollX(page)).map((p) => `3: ${p}`), ...(await checkClipping(page)).map((p) => `5: ${p}`)];
      expect(problems).toEqual([]);
    });
  }
}
