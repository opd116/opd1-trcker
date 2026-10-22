// Test harness (spec §8.1): clock, routes, scene URLs, settling. Test-only; never shipped.
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import type { Page, Request } from "@playwright/test";

/** The page clock (spec §8.1): only at 21:59 do the demo's `45 min ago` / `2h` agree with its clock times. */
export const NOW = new Date("2026-09-29T21:59:00Z");

/** The v2 design's three families (R189): Barlow for the UI, Saira Condensed for display figures, Geist Mono for labels. */
export const FONTS = ["Barlow", "Saira Condensed", "Geist Mono"] as const;
export type Font = (typeof FONTS)[number];

/** The v2 design's one accent (violet, R189); there is no accent option any more. */
export const ACCENT = "#9d8cff";

/** The window sizes (R190): the default, the minimum, and the glance column (Live only). */
export const SIZES = {
  default: { width: 1280, height: 800 },
  min: { width: 1180, height: 760 },
  glance: { width: 420, height: 800 },
} as const;

const HERE = dirname(fileURLToPath(import.meta.url));
export const UI_DIR = dirname(HERE);
/** git-ignored (ui/tests/.cache/): valorant-api images. */
export const CACHE_DIR = join(HERE, ".cache");

// ── a byte-for-byte response cache ───────────────────────────────────────────────
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

export const MEDIA_CACHE = join(CACHE_DIR, "media");

/** A test's own answer for a media URL: a status to fail it with (an empty body), or null for the normal answer. */
export type MediaOverride = (url: string) => { status: number } | null;

/** The cached image, else fetched once from the network and kept; null when that fails too. */
async function mediaFor(url: string): Promise<{ body: Buffer; type: string } | null> {
  const hit = readCache(MEDIA_CACHE, url);
  if (hit) return hit;
  try {
    const res = await fetch(url);
    if (!res.ok) return null;
    const got = { body: Buffer.from(await res.arrayBuffer()), type: res.headers.get("content-type") ?? "application/octet-stream" };
    writeCache(MEDIA_CACHE, url, got);
    return got;
  } catch {
    return null;
  }
}

/**
 * Whether the CDN's images are served from the local cache. In CI (the pinned Playwright image, `CI` set) they are: the
 * workflow keeps ui/tests/.cache/media between runs. On a developer machine the suite runs on the installed Chrome
 * (`channel: "chrome"`), which with DevTools request interception on refuses some of the DEV server's module requests
 * under load (`net::ERR_INSUFFICIENT_RESOURCES`) and now and then crashes a page; there the images come straight from
 * the CDN unless a test overrides some of them. `MEDIA_CACHE=1` forces the cache locally.
 */
const INTERCEPT_MEDIA = !!process.env.CI || process.env.MEDIA_CACHE === "1";

/**
 * valorant-api images (the DEV `assetUrl` points at its CDN) come from the local cache (git-ignored, filled from the
 * network on a miss) when INTERCEPT_MEDIA is on. Only those requests are intercepted, through the page's own DevTools
 * session (`Fetch.enable` with the CDN's URL pattern), never the app's own: Playwright's `page.route` would pause every
 * request of the page, the DEV server's few hundred module requests included. `override` lets a test fail chosen URLs
 * (and turns the interception on for that test).
 */
export async function routeMedia(page: Page, override?: MediaOverride): Promise<void> {
  if (!override && !INTERCEPT_MEDIA) return;
  const cdp = await page.context().newCDPSession(page);
  cdp.on("Fetch.requestPaused", (e: { requestId: string; request: { url: string } }) => {
    void (async () => {
      const forced = override?.(e.request.url);
      const got = forced ? null : await mediaFor(e.request.url);
      if (got) {
        await cdp.send("Fetch.fulfillRequest", {
          requestId: e.requestId,
          responseCode: 200,
          responseHeaders: [{ name: "content-type", value: got.type }],
          body: got.body.toString("base64"),
        });
      } else if (forced) {
        await cdp.send("Fetch.fulfillRequest", { requestId: e.requestId, responseCode: forced.status, responseHeaders: [], body: "" });
      } else {
        await cdp.send("Fetch.failRequest", { requestId: e.requestId, errorReason: "Failed" });
      }
    })().catch(() => {}); // the page closed meanwhile
  });
  await cdp.send("Fetch.enable", { patterns: [{ urlPattern: "https://media.valorant-api.com/*", requestStage: "Request" }] });
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
/**
 * A demo URL (demo/feed.ts): `?scene=` plus the v2 parameters (`screen`, `profile`, `match` + `puuid`, `variant`,
 * `recap`, `queue`, `glance`, `empty`, `fail`).
 */
export function sceneUrl(scene: string, params: Record<string, string> = {}): string {
  return `/?${new URLSearchParams({ scene, ...params })}`;
}

export type OpenOpts = {
  /**
   * `fixed` (default): Date.now is pinned at NOW and timers run normally.
   * `manual`: Playwright's fake clock, installed at NOW before the page loads; once the page has settled time stops
   * where the page's clock is then, and only `page.clock.runFor` moves it.
   */
  clock?: "fixed" | "manual";
  /**
   * Wait for the data the screen asks for after it opens (default): until nothing under #app is `aria-busy="true"`.
   * Off for the scenes whose answer never comes (`*-loading`), which are left showing their skeleton.
   */
  waitData?: boolean;
};

/** The DEV demo feed is running (demo/feed.ts marks <html> before it applies the scene). */
export const DEMO_READY = () => document.documentElement.dataset.feed === "demo-feed";
/** The app mounted (main.ts mounts App into #app once every module of its static graph has loaded). */
export const APP_READY = () => (document.getElementById("app")?.childElementCount ?? 0) > 0;

/**
 * Navigate to the app and wait until `ready` holds in the page. The DEV server serves the app as a few hundred
 * separate modules, and with DevTools request interception on (`routeMedia`) Chrome can refuse one of them under load
 * (`net::ERR_INSUFFICIENT_RESOURCES`): the module graph then never finishes and the app never starts. That failure is
 * the browser's, not the app's, so the page is loaded again (up to three attempts); any other failure is reported.
 */
export async function gotoApp(page: Page, url: string, ready: () => boolean = DEMO_READY): Promise<void> {
  for (let attempt = 1; ; attempt++) {
    let refused: string | null = null;
    let wake: () => void = () => {};
    const refusedOnce = new Promise<void>((r) => (wake = r));
    const onFail = (req: Request) => {
      if (/ERR_INSUFFICIENT_RESOURCES/.test(req.failure()?.errorText ?? "") && req.resourceType() !== "image") {
        refused = req.url();
        wake();
      }
    };
    page.on("requestfailed", onFail);
    try {
      await page.goto(url);
      const started = page.waitForFunction(ready);
      started.catch(() => {}); // when the refusal wins the race, the reload below ends this wait
      await Promise.race([started, refusedOnce]);
    } finally {
      page.off("requestfailed", onFail);
    }
    if (refused === null) return;
    if (attempt === 3) throw new Error(`the app did not load in 3 attempts: Chrome refused ${refused} (net::ERR_INSUFFICIENT_RESOURCES)`);
  }
}

/** Open a demo scene and wait until its data, the fonts and every image (including CSS backgrounds) have loaded. */
export async function openScene(page: Page, scene: string, params: Record<string, string> = {}, opts: OpenOpts = {}): Promise<void> {
  await routeMedia(page);
  if (opts.clock === "manual") await page.clock.install({ time: NOW });
  else await page.clock.setFixedTime(NOW);
  await gotoApp(page, sceneUrl(scene, params));
  await page.locator("#app main.main, #app .glance").first().waitFor();
  if (opts.waitData ?? !scene.endsWith("-loading")) await waitIdle(page);
  await settle(page);
  if (opts.clock === "manual") await pauseHere(page);
}

/** Every screen, drawer and scoreboard has its data: nothing under #app is `aria-busy="true"` any more. */
export async function waitIdle(page: Page): Promise<void> {
  await page.waitForFunction(() => document.querySelector('#app [aria-busy="true"]') === null);
}

/**
 * Stop the fake clock at the page's own current time (a beat ahead, for the round trip), not at a fixed offset from
 * NOW: loading and settling take however long they take in real time. `pauseAt` refuses a target in the past, so if
 * the round trip ever outlasts the beat the page's time is read again.
 */
async function pauseHere(page: Page): Promise<void> {
  for (let attempt = 0; ; attempt++) {
    const t = await page.evaluate(() => Date.now());
    try {
      await page.clock.pauseAt(t + 250);
      return;
    } catch (e) {
      if (attempt >= 4 || !/past/i.test(String(e))) throw e;
    }
  }
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

/** A CSS variable resolved to rgb() through a probe element, so it compares with computed colours. */
export function varRgb(page: Page, name: string): Promise<string> {
  return page.evaluate((n) => {
    const p = document.createElement("div");
    p.style.color = `var(${n})`;
    document.body.appendChild(p);
    const c = getComputedStyle(p).color;
    p.remove();
    return c;
  }, name);
}

/** Move the pointer to a spot with no tooltip trigger (the logo at the title bar's left end). */
export async function pointerAway(page: Page): Promise<void> {
  await page.mouse.move(5, 5);
}
