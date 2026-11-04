import { defineConfig } from '@playwright/test';
import base from './playwright.config';

/** New-design acceptance suite; legacy visual goldens stay untouched pending review. */
export default defineConfig({
  ...base,
  testMatch:'compact-redesign.spec.ts',
  outputDir:'test-results/compact',
  use:{...base.use,baseURL:'http://127.0.0.1:4187',viewport:{width:700,height:860}},
  webServer:{command:'npm run dev -- --port 4187 --strictPort',url:'http://127.0.0.1:4187',reuseExistingServer:false},
});
