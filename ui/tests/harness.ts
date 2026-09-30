// Test harness (spec §8.1): clock, routes, scene URLs, settling and the font check. Test-only; never shipped.
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import type { Page, Route } from "@playwright/test";

/** The page clock (spec §8.1): only at 21:59 do the design's `45m / 2h / 4h ago` agree with its clock times. */
export const NOW = new Date("2026-09-29T21:59:00Z");

export const FONTS = ["Barlow", "IBM Plex Sans", "Archivo"] as const;
export type Font = (typeof FONTS)[number];
/** `IBM Plex Sans` → `IBM-Plex-Sans`, for file names. */
export const fontSlug = (f: string) => f.replace(/ /g, "-");

export const ACCENTS = ["#e3a54f", "#4fc3ae", "#7fb1ef", "#e58a9a"] as const;

const HERE = dirname(fileURLToPath(import.meta.url));
export const TESTS_DIR = HERE;
export const UI_DIR = dirname(HERE);
/** git-ignored (ui/tests/.cache/): valorant-api images and the design prototype's CDN scripts. */
export const CACHE_DIR = join(HERE, ".cache");
export const SHOTS_DIR = join(HERE, "shots");

// ── a byte-for-byte response cache (images here, unpkg in design-ref.ts) ─────────
type Cached = { body: Buffer; type: string };

function cachePaths(dir: string, url: string) {
  const key = createHash("sha1").update(url).digest("hex");
  return { bin: join(dir, `${key}.bin`), meta: join(dir, `${key}.json`) };
}

export function readCache(dir: string, url: string): Cached | null {
  const p = cachePaths(dir, url);
  if (!existsSync(p.bin) || !existsSync(p.meta)) return null;
  const meta = JSON.parse(readFileSync(p.meta, "utf8")) as { url: string; type: string };
  return { body: readFileSync(p.bin), type: meta.type };
}

/** Atomic (temp file + rename): parallel workers may fill the same entry. */
export function writeCache(dir: string, url: string, c: Cached): void {
  mkdirSync(dir, { recursive: true });
  const p = cachePaths(dir, url);
  const tmp = `.${process.pid}.${Math.random().toString(36).slice(2)}`;
  writeFileSync(p.bin + tmp, c.body);
  renameSync(p.bin + tmp, p.bin);
  writeFileSync(p.meta + tmp, JSON.stringify({ url, type: c.type }));
  renameSync(p.meta + tmp, p.meta);
}

/**
 * Serve `route` from the cache when the file exists, else fetch it once from the network and keep it.
 * `cors` adds `Access-Control-Allow-Origin: *` (needed by the design's SRI-checked CDN scripts).
 */
export async function serveCached(route: Route, dir: string, cors = false): Promise<boolean> {
  const url = route.request().url();
  const extra: Record<string, string> = cors ? { "access-control-allow-origin": "*" } : {};
  const hit = readCache(dir, url);
  if (hit) {
    await route.fulfill({ status: 200, body: hit.body, headers: { "content-type": hit.type, ...extra } });
    return true;
  }
  try {
    const res = await route.fetch();
    if (!res.ok()) {
      await route.abort();
      return false;
    }
    const body = await res.body();
    const type = res.headers()["content-type"] ?? "application/octet-stream";
    writeCache(dir, url, { body, type });
    await route.fulfill({ status: 200, body, headers: { "content-type": type, ...extra } });
    return true;
  } catch {
    await route.abort().catch(() => {});
    return false;
  }
}

export const MEDIA_CACHE = join(CACHE_DIR, "media");

/** valorant-api images (the DEV `assetUrl` points at its CDN) come from the local cache when present. */
export async function routeMedia(page: Page): Promise<void> {
  await page.route("https://media.valorant-api.com/**", (route) => serveCached(route, MEDIA_CACHE).then(() => {}));
}

/** Every request to a font CDN, recorded (the app must make none: spec §8.1). */
export function watchFontCdn(page: Page): string[] {
  const hits: string[] = [];
  page.on("request", (r) => {
    if (/^https?:\/\/fonts\.(googleapis|gstatic)\.com\//.test(r.url())) hits.push(r.url());
  });
  return hits;
}

// ── scenes ─────────────────────────────────────────────────────────────────────
export type ThemeParams = Partial<Record<"tone" | "accent" | "results" | "fill" | "font" | "last5", string>>;

export function sceneUrl(scene: string, params: ThemeParams & Record<string, string> = {}): string {
  return `/?${new URLSearchParams({ scene, ...params })}`;
}

export type OpenOpts = {
  /**
   * `fixed` (default): Date.now is pinned at NOW and timers run normally.
   * `manual`: Playwright's fake clock, installed at NOW before the page loads; once the page has settled
   * time stops, and only `page.clock.runFor` moves it (tooltip timing tests never depend on real timers).
   */
  clock?: "fixed" | "manual";
};

/** Open a demo scene and wait until fonts and every image (including CSS backgrounds) have loaded. */
export async function openScene(
  page: Page,
  scene: string,
  params: ThemeParams & Record<string, string> = {},
  opts: OpenOpts = {},
): Promise<void> {
  await routeMedia(page);
  if (opts.clock === "manual") await page.clock.install({ time: NOW });
  else await page.clock.setFixedTime(NOW);
  await page.goto(sceneUrl(scene, params));
  await page.waitForFunction(() => document.documentElement.dataset.feed === "demo-feed");
  await settle(page);
  if (opts.clock === "manual") await page.clock.pauseAt(new Date(NOW.getTime() + 30_000));
}

/** Fonts ready, every `<img>` and CSS background image loaded, then two frames for paint. */
export async function settle(page: Page): Promise<void> {
  await page.evaluate(async () => {
    await document.fonts.ready;
    const urls = new Set<string>();
    for (const el of document.querySelectorAll<HTMLElement>("#app *")) {
      const bg = getComputedStyle(el).backgroundImage;
      for (const m of bg.matchAll(/url\("?([^")]+)"?\)/g)) urls.add(m[1]!);
    }
    const wait = (img: HTMLImageElement) =>
      img.complete
        ? Promise.resolve()
        : new Promise<void>((r) => {
            img.addEventListener("load", () => r(), { once: true });
            img.addEventListener("error", () => r(), { once: true });
          });
    const bgs = [...urls].map((u) => {
      const i = new Image();
      i.src = u;
      return wait(i);
    });
    await Promise.all([...[...document.images].map(wait), ...bgs]);
    await new Promise<void>((r) => requestAnimationFrame(() => requestAnimationFrame(() => r())));
  });
}

/** Spec §8.3 check 8's second half: the selected family is actually loaded. */
export async function fontLoaded(page: Page, family: string): Promise<boolean> {
  return page.evaluate((f) => document.fonts.check(`600 16px "${f}"`), family);
}

/** Move the pointer to a spot with no tooltip trigger (top-left of the top bar). */
export async function pointerAway(page: Page): Promise<void> {
  await page.mouse.move(5, 5);
}
