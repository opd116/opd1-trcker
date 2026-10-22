// Playwright (spec §8.1): Chromium at 1280×800 (the v2 default window; the minimum is 1180×760), DPR 1, UTC, en-US, against the DEV demo feed.
// The page clock is fixed by tests/harness.ts (2026-09-29T21:59:00Z). fit-dpr.spec.ts overrides the DPR to 1.5
// with test.use; there is no second project, so no two projects write the same reference PNG.
import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./tests",
  // Design reference PNGs are regenerated from the design file on every run (tests/design-ref.ts); git-ignored.
  snapshotPathTemplate: "{testDir}/ref/{arg}{ext}",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  reporter: [["list"]],
  timeout: 60_000,
  expect: {
    timeout: 10_000,
    // Visual baselines (tests/visual-baseline.spec.ts): committed PNGs, compared with `toHaveScreenshot`. The name has no
    // `{platform}` / `{projectName}` suffix because only one environment ever renders them: the pinned Playwright image
    // (`update_baselines` run of .github/workflows/build.yml). They are separate from `snapshotPathTemplate` above, whose
    // design-file references are git-ignored and regenerated on every run.
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
