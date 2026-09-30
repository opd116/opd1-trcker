// Reference render of the user's design file (test harness only, never shipped). The prototype's own runtime
// (support.js → React from unpkg, Google Fonts, valorant-api images) runs inside Playwright, with every
// network dependency served locally so both renders use identical bytes:
//   - http://design.local/**         → the design dir (the .dc.html with its data-props defaults replaced)
//   - https://unpkg.com/**           → ui/tests/.cache/cdn/ (filled once from the network, byte for byte; SRI stays valid)
//   - fonts.googleapis.com/css2      → @font-face rules pointing at the 12 bundled woff2 files (same as app.css)
//   - fonts.gstatic.com              → aborted
//   - media.valorant-api.com         → the same image cache as the app harness
import { existsSync, readFileSync } from "node:fs";
import { dirname, extname, join, normalize } from "node:path";
import type { Locator, Page, Route } from "@playwright/test";
import { CACHE_DIR, readCache, routeMedia, serveCached, settle, UI_DIR, writeCache } from "./harness";

export const DESIGN_FILE = "OPD1 Tracker v8.dc.html";
const DESIGN_ORIGIN = "http://design.local";
const CDN_CACHE = join(CACHE_DIR, "cdn");
/** The scripts support.js loads (with SRI) before it boots. */
const CDN_SCRIPTS = [
  "https://unpkg.com/react@18.3.1/umd/react.production.min.js",
  "https://unpkg.com/react-dom@18.3.1/umd/react-dom.production.min.js",
];
const FONT_DIR = join(UI_DIR, "src", "assets", "fonts");

export type DesignProps = {
  screen?: "In Game" | "Player Details";
  loading?: boolean;
  tone?: "Neutral" | "Cool" | "Warm";
  accent?: string;
  results?: "Green / Red" | "Accent / Red" | "Accent / Gray";
  resultFill?: "Tinted" | "Solid";
  font?: "Barlow" | "IBM Plex Sans" | "Archivo";
  last5Style?: "Arrow" | "Underline";
};

/** A user click inside the design: click, park the pointer at (5,5), wait 600ms (its tooltip timer is 450ms). */
export type DesignStep = "mara" | "cypher" | "more" | "recent" | "encounter";

/** `OPD1_DESIGN_DIR`, else walk up from ui/ to `.superpowers/design/cd-v8/project`. null when absent. */
export function findDesignDir(): string | null {
  const env = process.env.OPD1_DESIGN_DIR;
  if (env) return existsSync(join(env, DESIGN_FILE)) ? env : null;
  for (let dir = UI_DIR; ; dir = dirname(dir)) {
    const cand = join(dir, ".superpowers", "design", "cd-v8", "project");
    if (existsSync(join(cand, DESIGN_FILE))) return cand;
    if (dirname(dir) === dir) return null;
  }
}

/**
 * Make sure the CDN cache holds React and ReactDOM: fill missing entries from the network (Node's fetch
 * returns the decoded bytes the browser would hash for SRI). false = no network and an empty cache.
 */
export async function ensureCdnCache(): Promise<boolean> {
  for (const url of CDN_SCRIPTS) {
    if (readCache(CDN_CACHE, url)) continue;
    try {
      const res = await fetch(url);
      if (!res.ok) return false;
      writeCache(CDN_CACHE, url, {
        body: Buffer.from(await res.arrayBuffer()),
        type: res.headers.get("content-type") ?? "text/javascript",
      });
    } catch {
      return false;
    }
  }
  return true;
}

const TYPES: Record<string, string> = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".png": "image/png",
  ".jpg": "image/jpeg",
  ".woff2": "font/woff2",
};

const unescapeAttr = (s: string) => s.replace(/&quot;/g, '"').replace(/&amp;/g, "&");
const escapeAttr = (s: string) => s.replace(/&/g, "&amp;").replace(/"/g, "&quot;");

/** The .dc.html with each given prop's `default` replaced in its `data-props` JSON. */
export function patchedDesign(dir: string, props: DesignProps): string {
  const html = readFileSync(join(dir, DESIGN_FILE), "utf8");
  return html.replace(/data-props="([^"]*)"/, (_m, raw: string) => {
    const spec = JSON.parse(unescapeAttr(raw)) as Record<string, { default?: unknown }>;
    for (const [k, v] of Object.entries(props)) {
      if (!spec[k]) throw new Error(`design has no prop ${k}`);
      spec[k].default = v;
    }
    return `data-props="${escapeAttr(JSON.stringify(spec))}"`;
  });
}

/** app.css's own @font-face rules, with the bundled files served from design.local. */
function fontCss(): string {
  const css = readFileSync(join(UI_DIR, "src", "app.css"), "utf8");
  const faces = css.match(/@font-face\s*{[^}]*}/g) ?? [];
  return faces.map((f) => f.replace(/url\("\.\/assets\/fonts\/([^"]+)"\)/g, `url("${DESIGN_ORIGIN}/__fonts/$1")`)).join("\n");
}

async function serveDesign(route: Route, dir: string, props: DesignProps): Promise<void> {
  const path = decodeURIComponent(new URL(route.request().url()).pathname);
  if (path.startsWith("/__fonts/")) {
    const file = join(FONT_DIR, normalize(path.slice("/__fonts/".length)));
    if (!file.startsWith(FONT_DIR) || !existsSync(file)) return route.fulfill({ status: 404 });
    return route.fulfill({ status: 200, body: readFileSync(file), headers: { "content-type": "font/woff2" } });
  }
  if (path === `/${DESIGN_FILE}`) {
    return route.fulfill({ status: 200, body: patchedDesign(dir, props), headers: { "content-type": TYPES[".html"]! } });
  }
  const file = join(dir, normalize(path));
  if (!file.startsWith(dir) || !existsSync(file)) return route.fulfill({ status: 404 });
  return route.fulfill({
    status: 200,
    body: readFileSync(file),
    headers: { "content-type": TYPES[extname(file)] ?? "application/octet-stream" },
  });
}

/** Load the design with `props` as its option defaults, optionally clicking through `steps` like a user. */
export async function openDesign(page: Page, dir: string, props: DesignProps, steps: DesignStep[] = []): Promise<void> {
  await page.route(`${DESIGN_ORIGIN}/**`, (route) => serveDesign(route, dir, props));
  // From the cache (ensureCdnCache() filled React and ReactDOM); anything else is fetched once, then kept.
  await page.route("https://unpkg.com/**", (route) => serveCached(route, CDN_CACHE, true).then(() => {}));
  await page.route("https://fonts.googleapis.com/css2**", (route) =>
    route.fulfill({ status: 200, body: fontCss(), headers: { "content-type": "text/css; charset=utf-8", "access-control-allow-origin": "*" } }),
  );
  await page.route("https://fonts.gstatic.com/**", (route) => route.abort());
  await routeMedia(page);

  await page.goto(`${DESIGN_ORIGIN}/${encodeURIComponent(DESIGN_FILE)}`);
  // The runtime renders the template with React; the raw <x-dc> copy stays hidden.
  await page.waitForFunction(() => [...document.querySelectorAll("#app")].some((a) => !a.closest("x-dc") && a.children.length > 0));
  await settle(page);
  for (const step of steps) {
    await clickStep(page, step);
  }
}

async function clickStep(page: Page, step: DesignStep): Promise<void> {
  // The rendered app (React writes `cursor: pointer`, with a space); the hidden raw <x-dc> template has none.
  const app = page.locator("#app").filter({ has: page.locator("div[style*='cursor: pointer']") });
  switch (step) {
    case "mara":
      // The only pointer div holding Mara's Riot ID is her card (the template's whitespace sits between name and tag).
      await app.locator("div[style*='cursor: pointer']").filter({ hasText: /Mara\s*#7788/ }).click();
      break;
    case "cypher":
      // Cypher is hidden (v8:466): his card's name is the italic agent name, the only `Cypher` text on In Game.
      await app.locator("div[style*='cursor: pointer']").filter({ has: page.getByText("Cypher", { exact: true }) }).click();
      break;
    case "more":
      await app.getByText("Show more", { exact: true }).click();
      break;
    case "recent":
      await app.getByText("45m ago", { exact: true }).click();
      break;
    case "encounter":
      // Mara's first encounter row (Lotus, Against): its date span, as the only exact `Sep 12` on her Details page.
      await app.getByText("Sep 12", { exact: true }).click();
      break;
  }
  await page.mouse.move(5, 5);
  await page.waitForTimeout(600);
  await settle(page);
}

/**
 * Both renders hide the ⟲ glyph, and only the glyph. Since v0.2.0 (R92) we draw the same character in the same
 * `'Segoe UI Symbol','Segoe UI',sans-serif` span as the design, so on Windows the two glyphs are identical. This
 * comparison runs on Linux, which has no Segoe UI Symbol: each render would fall back to whatever font the machine
 * happens to have for U+27F2, which proves nothing about the app, so the glyph stays out of the pixel diff (its
 * own look is pinned by fit-live's badge test). The count beside it is compared: the same font, right-aligned in
 * the same 36px slot, so it lands on the same pixels.
 */
export const COMPARE_STYLE = '[style*="Segoe UI Symbol"], .enc .glyph{visibility:hidden!important}';

/**
 * R125 (a user-delegated change to the design; spec §3.6 and §8.4): the card's Prev row reads icon · Prev · name,
 * where the design has Prev · icon · name. That grid cell (card-relative x 313, y 62, 158×18) is the one approved
 * deviation, so both renders mask exactly it, in one colour. visual-live asserts that the two sets of masked
 * rects are identical, so the mask can never grow past the row.
 */
export const designPrevRows = (page: Page) => page.locator("xpath=//div[span[1][normalize-space(.)='Prev']]");
export const ourPrevRows = (page: Page) => page.locator(".live .card .prev");
export const MASK_COLOR = "#ff00ff";

export type Rect = [x: number, y: number, w: number, h: number];

/** The rects of every rendered element `loc` matches. */
export const rectsOf = (loc: Locator): Promise<Rect[]> =>
  loc.evaluateAll((els) =>
    els
      .map((e) => e.getBoundingClientRect())
      .filter((r) => r.width > 0 && r.height > 0)
      .map((r) => [r.x, r.y, r.width, r.height] as Rect),
  );

/**
 * The largest coordinate gap between two rect lists of equal length (Infinity when the lengths differ). The rows
 * sit at fractional y (59 + 147.8i) and the two layouts round them a few hundredths apart.
 */
export const rectGap = (a: Rect[], b: Rect[]): number =>
  a.length !== b.length ? Infinity : Math.max(0, ...a.flatMap((r, i) => r.map((v, k) => Math.abs(v - b[i]![k]!))));
