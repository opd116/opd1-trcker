// Playwright (spec §8.1): Chromium at 1280×800 (the v2 default window; the minimum is 1180×760), DPR 1, UTC, en-US, against the DEV demo feed.
// The page clock is fixed by tests/harness.ts (2026-09-29T21:59:00Z). fit-v2.spec.ts reruns a few states at DPR 1.5
// with test.use; there is no second project, so no two projects write the same baseline PNG.
import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./tests",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  reporter: [["list"]],
  timeout: 60_000,
  expect: {
    timeout: 10_000,
    // Visual baselines (tests/visual-baseline.spec.ts): committed PNGs, compared with `toHaveScreenshot`. The name has no
    // `{platform}` / `{projectName}` suffix because only one environment ever renders them: the pinned Playwright image
    // (`update_baselines` run of .github/workflows/build.yml).
    toHaveScreenshot: {
      pathTemplate: "{testDir}/baselines/{arg}{ext}",
      maxDiffPixelRatio: 0.002,
      animations: "disabled",
      caret: "hide",
      scale: "css",
    },
  },
  use: {
    baseURL: "http://127.0.0.1:4173",
    viewport: { width: 1280, height: 800 },
    deviceScaleFactor: 1,
    timezoneId: "UTC",
    locale: "en-US",
  },
  projects: [{ name: "chromium", use: { browserName: "chromium" } }],
  // R14: always a fresh DEV server on a fixed port.
  webServer: {
    command: "npm run dev -- --port 4173 --strictPort",
    url: "http://127.0.0.1:4173",
    reuseExistingServer: false,
    timeout: 60_000,
  },
});
