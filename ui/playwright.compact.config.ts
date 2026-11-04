import { defineConfig } from '@playwright/test';
import base from './playwright.config';

/** Compact functional acceptance; R211 visual references are compared separately in the pinned CI image. */
export default defineConfig({
  ...base,
  testMatch:['compact-redesign.spec.ts','profile-layout.spec.ts','ascii-metrics.spec.ts'],
  outputDir:'test-results/compact',
  use:{...base.use,baseURL:'http://127.0.0.1:4187',viewport:{width:700,height:860}},
  webServer:{command:'npm run dev -- --port 4187 --strictPort',url:'http://127.0.0.1:4187',reuseExistingServer:false},
});
