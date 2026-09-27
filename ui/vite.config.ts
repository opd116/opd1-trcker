/// <reference types="vitest/config" />
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { defineConfig } from "vite";

// Tauri sets this when running `tauri dev` against a LAN host (mobile
// testing). Unused on desktop, harmless to read.
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig({
  plugins: [svelte()],

  // Vite options tailored for Tauri development, applied in `tauri dev` /
  // `tauri build` too since Tauri just shells out to these npm scripts.
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || "127.0.0.1",
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // Tell Vite to ignore watching `src-tauri`.
      ignored: ["**/src-tauri/**"],
    },
  },

  test: {
    // Unit tests only; Playwright specs under tests/ run with `playwright test`.
    include: ["src/**/*.test.ts"],
  },
});
