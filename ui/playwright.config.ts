// Playwright (spec §8.1): Chromium at exactly 1000×800, DPR 1, UTC, en-US, against the DEV demo feed.
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
  expect: { timeout: 10_000 },
  use: {
    baseURL: "http://127.0.0.1:4173",
    viewport: { width: 1000, height: 800 },
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
