// The v2 global layout assertions (fit-v2.spec.ts, visual-baseline.spec.ts) as functions. Each returns a list of
// problems (empty = pass), so a failing test prints what is wrong and where. Selectors are the components' own class
// names (Svelte keeps them; its `svelte-…` scoping classes are dropped from the labels).
import type { Page } from "@playwright/test";

// ── the type scale ───────────────────────────────────────────────────────────────────────────────────────────────
/**
 * Every font size the v2 design file sets (`.superpowers/design/v2/TrackerApp.dc.html`, its `font:` shorthands and
 * `font-size` overrides), in px. Nothing on screen may use a size the design does not have.
 */
export const DESIGN_SIZES = [
  9, 10, 10.5, 11, 11.5, 12, 12.5, 13, 13.5, 14, 14.5, 15, 15.5, 16, 17, 18, 19, 20, 21, 22, 24, 25, 26, 28, 30, 32, 34, 36, 46, 54, 76,
] as const;

/**
 * The sizes under the 12px body floor, by family, exactly as the design uses them (CLAUDE.md "Design rules": the
 * uppercase Geist Mono micro-labels are 10.5px, the badges 9–10px):
 *   Geist Mono  9 (the nav rail's count badge), 10 (YOU / MVP badges, W/L chips, the compact table heads), 10.5 (the
 *               micro-labels), 11 and 11.5 (small figures)
 *   Barlow      11 and 11.5 (chips, sub-captions, the nav labels); 10 only for a bare arrow glyph (the sort and RR
 *               arrows: no letter or digit)
 *   Saira Condensed never below 12 (it is the display face)
 */
export const SMALL_SIZES: Record<string, readonly number[]> = {
  "Geist Mono": [9, 10, 10.5, 11, 11.5],
  Barlow: [11, 11.5],
  "Saira Condensed": [],
};
/** The one Barlow size below 11 the design uses, for glyph-only text. */
export const GLYPH_SIZE = 10;

/** The three bundled families (fonts.test.ts checks the files; this checks what renders). */
export const FAMILIES = ["Barlow", "Saira Condensed", "Geist Mono"] as const;
/**
 * The design's ASCII art face (TrackerApp.dc.html 895: `'Cascadia Mono', Consolas, … 'Geist Mono'`), a Windows system
 * font: allowed for the art, never loaded by the app.
 */
export const ART_FAMILY = "Cascadia Mono";
/** The ASCII art itself: sized to fit its frame (`canvasFontPx`, `tileFontPx`), so it is artwork, not type. */
export const ART_SELECTOR = "pre.canvas, .art pre";

/**
 * Every rendered text under #app: its size is one the design uses; below 12px only the design's small sizes for that
 * family (see SMALL_SIZES); never below 9px; and its family is one of the three bundled ones (or the ASCII studio's
 * art face). The ASCII art itself (ART_SELECTOR) is scaled to its frame and is not type.
 */
export async function checkType(page: Page): Promise<string[]> {
  return page.evaluate(
    ({ sizes, small, glyph, families, art }) => {
      const out = new Set<string>();
      const app = document.getElementById("app")!;
      const walker = document.createTreeWalker(app, NodeFilter.SHOW_TEXT);
      for (let n = walker.nextNode(); n; n = walker.nextNode()) {
        const text = n.textContent?.trim();
        if (!text) continue;
        const el = n.parentElement!;
        if (el.getClientRects().length === 0 || el.closest(art)) continue;
        const s = getComputedStyle(el);
        if (s.visibility === "hidden") continue;
        const px = parseFloat(s.fontSize);
        const family = s.fontFamily.split(",")[0]!.replace(/["']/g, "").trim();
        const short = text.length > 40 ? `${text.slice(0, 40)}…` : text;
        if (!families.includes(family)) out.add(`"${short}" is in ${family}`);
        if (!sizes.some((d) => Math.abs(d - px) < 0.01)) out.add(`"${short}" is ${px}px (not in the design's scale)`);
        if (px < 9) out.add(`"${short}" is ${px}px (under the 9px badge floor)`);
        else if (px < 12) {
          const okSizes = small[family] ?? [];
          const glyphOnly = !/[\p{L}\p{N}]/u.test(text);
          const ok = okSizes.some((d) => Math.abs(d - px) < 0.01) || (family === "Barlow" && glyphOnly && Math.abs(px - glyph) < 0.01);
          if (!ok) out.add(`"${short}" is ${px}px ${family} (the design has no such small text)`);
        }
      }
      return [...out];
    },
    { sizes: [...DESIGN_SIZES], small: SMALL_SIZES, glyph: GLYPH_SIZE, families: [...FAMILIES, ART_FAMILY] as string[], art: ART_SELECTOR },
  );
}

/** #app uses Barlow, and the three families are loaded (bundled woff2, never a CDN). */
export async function checkFonts(page: Page): Promise<string[]> {
  return page.evaluate((families) => {
    const out: string[] = [];
    const ff = getComputedStyle(document.getElementById("app")!).fontFamily;
    if (!ff.replace(/["']/g, "").startsWith("Barlow")) out.push(`#app font-family is ${ff}`);
    for (const f of families) {
      const loaded = [...document.fonts].some((face) => face.family.replace(/["']/g, "") === f && face.status === "loaded");
      if (!loaded) out.push(`${f} is not loaded`);
    }
    return out;
  }, [...FAMILIES] as string[]);
}

// ── the window ───────────────────────────────────────────────────────────────────────────────────────────────────
/** #app fills the viewport exactly and the document never scrolls (either axis). */
export async function checkAppBox(page: Page): Promise<string[]> {
  return page.evaluate(() => {
    const out: string[] = [];
    const [w, h] = [window.innerWidth, window.innerHeight];
    const r = document.getElementById("app")!.getBoundingClientRect();
    if (r.x !== 0 || r.y !== 0 || r.width !== w || r.height !== h) out.push(`#app is ${r.x},${r.y} ${r.width}×${r.height} in ${w}×${h}`);
    const se = document.scrollingElement!;
    if (se.scrollWidth > w) out.push(`document scrollWidth ${se.scrollWidth} > ${w}`);
    if (se.scrollHeight > h) out.push(`document scrollHeight ${se.scrollHeight} > ${h}`);
    return out;
  });
}

/**
 * Scroll containers the design itself lets scroll sideways: the ASCII studio's canvas backdrop (TrackerApp.dc.html 904,
 * `overflow:auto`: art wider than the panel scrolls).
 */
export const SIDEWAYS_SCROLLERS = [".backdrop"];

/**
 * Nothing scrolls sideways (SIDEWAYS_SCROLLERS aside): no element that can scroll horizontally has content wider than
 * itself. With `noScroll` nothing scrolls vertically either (the Live board at the design's 1280×800 baseline).
 */
export async function checkScrolling(page: Page, noScroll = false): Promise<string[]> {
  return page.evaluate(
    ({ noScroll, sideways }) => {
      const label = (el: Element) => el.tagName.toLowerCase() + [...el.classList].filter((c) => !/^s(velte)?-/.test(c)).map((c) => `.${c}`).join("");
      const out: string[] = [];
      const can = (v: string) => v === "auto" || v === "scroll";
      for (const el of document.querySelectorAll<HTMLElement>("#app, #app *")) {
        const s = getComputedStyle(el);
        if (can(s.overflowX) && el.scrollWidth > el.clientWidth && !sideways.some((sel) => el.matches(sel))) {
          out.push(`${label(el)} scrolls sideways (${el.scrollWidth} > ${el.clientWidth})`);
        }
        if (noScroll && can(s.overflowY) && el.scrollHeight > el.clientHeight) out.push(`${label(el)} scrolls (${el.scrollHeight} > ${el.clientHeight})`);
      }
      return out;
    },
    { noScroll, sideways: SIDEWAYS_SCROLLERS },
  );
}

// ── clipping and overlap, from the text's own geometry ──────────────────────────────────────────────────────────
/**
 * The text checks, measured on the glyphs rather than on boxes (the design sets most text at `line-height: 1`, so a
 * line box is smaller than the font's content area and a box measurement would cut every name):
 *
 * - every text node's ink box: horizontally its laid-out extent (range rects, one per line), vertically the actual ink
 *   (`measureText`'s actual ascent / descent around the baseline), so a cut descender counts and empty leading does not;
 * - clipping: walking up from the text to the nearest scroll container (what lies past its edge is reached by
 *   scrolling), every ancestor that clips (`overflow` not `visible`, `overflow-clip-margin` counted) holds the ink box.
 *   The one exception is a truncation the design asks for: the clipping element elides with an ellipsis
 *   (`text-overflow: ellipsis`) or clamps its lines (`line-clamp`), and it, or an element around or inside it on the
 *   text's path, is marked `data-ellipsis`. Such a box may cut the text sideways (behind its ellipsis), hide the lines
 *   past its clamp, and trim the descender tails below the baseline (the design's truncating text is `line-height: 1`,
 *   so its own clip box does exactly that), but never the letters' bodies: above, no more than the half-leading;
 * - overlap: the visible parts of two different texts (ink box ∩ every clipping ancestor ∩ the viewport) never meet.
 *   Texts on different layers (inside a different positioned, z-indexed overlay: the profile drawer over the screen,
 *   a popover) are not compared.
 */
export async function checkText(page: Page): Promise<{ clip: string[]; overlap: string[] }> {
  return page.evaluate(() => {
    const label = (el: Element) => el.tagName.toLowerCase() + [...el.classList].filter((c) => !/^s(velte)?-/.test(c)).map((c) => `.${c}`).join("");
    const app = document.getElementById("app")!;
    const ctx = document.createElement("canvas").getContext("2d")!;
    type Box = { left: number; right: number; top: number; bottom: number };
    const meet = (a: Box, b: Box): Box => ({
      left: Math.max(a.left, b.left),
      right: Math.min(a.right, b.right),
      top: Math.max(a.top, b.top),
      bottom: Math.min(a.bottom, b.bottom),
    });
    const empty = (b: Box) => b.right - b.left <= 0.5 || b.bottom - b.top <= 0.5;
    const clip: string[] = [];
    const texts: Array<{ node: Node; el: Element; text: string; layer: Element | null; boxes: Box[] }> = [];

    /** The nearest positioned ancestor with a z-index: the overlay layer the text paints in (null: the page). */
    const layerOf = (el: Element): Element | null => {
      for (let a: Element | null = el; a && a !== app; a = a.parentElement) {
        const s = getComputedStyle(a);
        if ((s.position === "absolute" || s.position === "fixed") && s.zIndex !== "auto") return a;
      }
      return null;
    };
    const transform = (t: string, how: string) =>
      how === "uppercase" ? t.toUpperCase() : how === "lowercase" ? t.toLowerCase() : how === "capitalize" ? t.replace(/\b\p{L}/gu, (c) => c.toUpperCase()) : t;

    const walker = document.createTreeWalker(app, NodeFilter.SHOW_TEXT);
    for (let n = walker.nextNode(); n; n = walker.nextNode()) {
      const raw = n.textContent ?? "";
      const text = raw.trim();
      if (!text) continue;
      const el = n.parentElement!;
      const s = getComputedStyle(el);
      if (s.visibility === "hidden" || el.getClientRects().length === 0) continue;
      ctx.font = `${s.fontStyle} ${s.fontWeight} ${s.fontSize} ${s.fontFamily}`;
      const m = ctx.measureText(transform(text, s.textTransform));
      const range = document.createRange();
      range.selectNodeContents(n);
      const rects = [...range.getClientRects()].filter((r) => r.width > 0 && r.height > 0);
      if (rects.length === 0) continue;
      const ellipsisEl = el.closest("[data-ellipsis]");
      /** A truncation the design asks for: `a` elides with an ellipsis or clamps its lines, and is marked. */
      const truncates = (a: HTMLElement, as: CSSStyleDeclaration) =>
        !!ellipsisEl && (ellipsisEl.contains(a) || a.contains(ellipsisEl)) && (as.textOverflow === "ellipsis" || clamps(as));
      const clamps = (as: CSSStyleDeclaration) => (as.webkitLineClamp || "none") !== "none";
      // The design sets truncating text at `line-height: 1`: its line box is shorter than the font's content area by
      // this much on each side, and the truncating box clips there (descender tails included), as the design's own does.
      const lh = parseFloat(s.lineHeight);
      const leading = Number.isFinite(lh) ? Math.max(0, (m.fontBoundingBoxAscent + m.fontBoundingBoxDescent - lh) / 2) : 0;
      const boxes: Box[] = [];
      let reported = false;
      for (const r of rects) {
        const baseline = r.top + m.fontBoundingBoxAscent;
        const ink: Box = { left: r.left, right: r.right, top: baseline - m.actualBoundingBoxAscent, bottom: baseline + m.actualBoundingBoxDescent };
        let visible: Box = meet(ink, { left: 0, top: 0, right: window.innerWidth, bottom: window.innerHeight });
        let stopX = false;
        let stopY = false;
        let hiddenLine = false;
        for (let a: HTMLElement | null = el as HTMLElement; a && a !== app.parentElement; a = a.parentElement) {
          const as = getComputedStyle(a);
          if (as.display === "inline" || as.display === "contents") continue;
          const b = a.getBoundingClientRect();
          // overflow:clip paints past the box by overflow-clip-margin (room for an underline), but an ellipsis still
          // elides the glyphs at the box itself.
          const mx = as.overflowX === "clip" && as.textOverflow !== "ellipsis" ? parseFloat(as.getPropertyValue("overflow-clip-margin")) || 0 : 0;
          const my = as.overflowY === "clip" ? parseFloat(as.getPropertyValue("overflow-clip-margin")) || 0 : 0;
          const box: Box = {
            left: as.overflowX === "visible" ? -Infinity : b.left + parseFloat(as.borderLeftWidth) - mx,
            right: as.overflowX === "visible" ? Infinity : b.right - parseFloat(as.borderRightWidth) + mx,
            top: as.overflowY === "visible" ? -Infinity : b.top + parseFloat(as.borderTopWidth) - my,
            bottom: as.overflowY === "visible" ? Infinity : b.bottom - parseFloat(as.borderBottomWidth) + my,
          };
          visible = meet(visible, box);
          const scrollX = (as.overflowX === "auto" || as.overflowX === "scroll") && a.scrollWidth > a.clientWidth;
          const scrollY = (as.overflowY === "auto" || as.overflowY === "scroll") && a.scrollHeight > a.clientHeight;
          const ok = truncates(a, as);
          // A line a clamp hides altogether is the truncation itself.
          if (ok && clamps(as) && ink.top + leading >= box.bottom - 0.5) hiddenLine = true;
          if (!reported && !hiddenLine && !stopX && !scrollX && (ink.left < box.left - 0.5 || ink.right > box.right + 0.5) && !ok) {
            clip.push(`"${text}" is cut sideways by ${label(a)}${as.textOverflow === "ellipsis" || clamps(as) ? " (a truncation without data-ellipsis)" : ""}`);
            reported = true;
          }
          // A truncating box may trim what hangs below the baseline (descender tails) and, above, no more than the
          // half-leading its `line-height` takes off; the letters' bodies stay whole.
          const cutTop = ink.top < box.top - (ok ? leading + 0.5 : 0.5);
          const cutBottom = ink.bottom > box.bottom + 0.5 && !(ok && box.bottom >= baseline - 0.5);
          if (!reported && !hiddenLine && !stopY && !scrollY && (cutTop || cutBottom)) {
            const by = Math.max(box.top - ink.top, ink.bottom - box.bottom).toFixed(1);
            clip.push(`"${text}" is cut vertically by ${label(a)} (${by}px)${!ok && (as.textOverflow === "ellipsis" || clamps(as)) ? " (a truncation without data-ellipsis)" : ""}`);
            reported = true;
          }
          if (scrollX) stopX = true;
          if (scrollY) stopY = true;
        }
        if (!empty(visible)) boxes.push(visible);
      }
      if (boxes.length > 0) texts.push({ node: n, el, text, layer: layerOf(el), boxes });
    }

    const overlap: string[] = [];
    for (let i = 0; i < texts.length; i++) {
      for (let j = i + 1; j < texts.length; j++) {
        const [a, b] = [texts[i]!, texts[j]!];
        if (a.layer !== b.layer) continue;
        if (a.boxes.some((x) => b.boxes.some((y) => !empty(meet(x, y))))) {
          overlap.push(`"${a.text.slice(0, 30)}" (${label(a.el)}) overlaps "${b.text.slice(0, 30)}" (${label(b.el)})`);
        }
      }
    }
    return { clip, overlap };
  });
}

/** Text clipping (`checkText`) and the text decorations (`checkDecorations`). */
export async function checkClipping(page: Page): Promise<string[]> {
  return [...(await checkText(page)).clip, ...(await checkDecorations(page))];
}

/** Every element that draws a text decoration paints it whole (`decorationInk`), once per element. */
async function checkDecorations(page: Page): Promise<string[]> {
  const labels = await page.evaluate(() => {
    const label = (el: Element) => el.tagName.toLowerCase() + [...el.classList].filter((c) => !/^s(velte)?-/.test(c)).map((c) => `.${c}`).join("");
    const found: string[] = [];
    /** Fully on screen: inside the viewport (with the 10px band the probe shoots under it) and every scroll container. */
    const shown = (el: HTMLElement) => {
      const r = el.getBoundingClientRect();
      if (r.top < 0 || r.bottom + 12 > window.innerHeight) return false;
      for (let a = el.parentElement; a; a = a.parentElement) {
        const s = getComputedStyle(a);
        if (s.overflowY !== "auto" && s.overflowY !== "scroll") continue;
        const b = a.getBoundingClientRect();
        if (r.top < b.top || r.bottom + 12 > b.bottom) return false;
      }
      return true;
    };
    for (const el of document.querySelectorAll<HTMLElement>("#app *")) {
      if (getComputedStyle(el).textDecorationLine === "none" || !el.textContent?.trim() || el.getClientRects().length === 0) continue;
      if (!shown(el)) continue; // scrolled out of view: nothing to shoot
      el.dataset.decoProbe = String(found.length);
      found.push(`"${el.textContent.trim()}" (${label(el)})`);
    }
    return found;
  });
  const out: string[] = [];
  for (const [i, what] of labels.entries()) {
    const { ink, unclipped, width } = await decorationInk(page, `[data-deco-probe="${i}"]`);
    // A 1px dotted line lights about half the pixels under the span; a quarter is a safe floor.
    if (unclipped < Math.floor(width / 4)) out.push(`${what}: its text decoration does not paint (${unclipped}px of ink)`);
    else if (ink < unclipped) out.push(`${what}: a clip eats ${unclipped - ink} of ${unclipped} decoration pixels`);
  }
  await page.evaluate(() => document.querySelectorAll("[data-deco-probe]").forEach((el) => el.removeAttribute("data-deco-probe")));
  return out;
}

/**
 * Make the next frame a full re-raster of the viewport, so a screenshot shows what a first paint draws and never a
 * tile patched up from the previous state (a background change of the root invalidates the whole viewport; set and
 * put back in one task, so it is safe under the fake clock).
 */
async function repaintFully(page: Page): Promise<void> {
  await page.evaluate(() => {
    const root = document.documentElement;
    const before = root.style.getPropertyValue("background-color");
    root.style.setProperty("background-color", "rgb(1, 2, 3)");
    void root.offsetHeight;
    if (before) root.style.setProperty("background-color", before);
    else root.style.removeProperty("background-color");
  });
}

/**
 * Ink probe for a text decoration that hangs below its text. Ink outside an `overflow:hidden` box shows up in no DOM
 * rect, so a clipped underline passes every box measurement. For the first match of `sel` this screenshots the box
 * plus a 10px band under it with and without `text-decoration` and counts the pixels that differ: once as rendered
 * (`ink`) and once with the clip pushed 14px out (`unclipped`); `ink < unclipped` means a clip eats part of it.
 * Every shot is a from-scratch raster (`repaintFully`): an incremental repaint can leave a stale antialiased row.
 */
export async function decorationInk(page: Page, sel: string): Promise<{ ink: number; unclipped: number; width: number }> {
  const loc = page.locator(sel).first();
  const r = await loc.boundingBox();
  if (!r) throw new Error(`no box for ${sel}`);
  const clip = { x: Math.max(0, Math.floor(r.x) - 1), y: Math.max(0, Math.floor(r.y) - 2), width: Math.ceil(r.width) + 2, height: Math.ceil(r.height) + 12 };
  const count = async (extra: string): Promise<number> => {
    const base = await page.addStyleTag({ content: extra || "/* as rendered */" });
    await repaintFully(page);
    const withDeco = await page.screenshot({ clip });
    const off = await page.addStyleTag({ content: `${sel}{text-decoration:none!important}` });
    await repaintFully(page);
    const without = await page.screenshot({ clip });
    await off.evaluate((el) => (el as Element).remove());
    await base.evaluate((el) => (el as Element).remove());
    return page.evaluate(
      async ([a, b]) => {
        const px = async (b64: string) => {
          const bytes = Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
          const bmp = await createImageBitmap(new Blob([bytes], { type: "image/png" }));
          const cv = new OffscreenCanvas(bmp.width, bmp.height);
          const ctx = cv.getContext("2d")!;
          ctx.drawImage(bmp, 0, 0);
          return ctx.getImageData(0, 0, bmp.width, bmp.height).data;
        };
        const [da, db] = [await px(a!), await px(b!)];
        let n = 0;
        for (let i = 0; i < da.length; i += 4) {
          if (Math.abs(da[i]! - db[i]!) + Math.abs(da[i + 1]! - db[i + 1]!) + Math.abs(da[i + 2]! - db[i + 2]!) > 24) n++;
        }
        return n;
      },
      [withDeco.toString("base64"), without.toString("base64")],
    );
  };
  const ink = await count("");
  const unclipped = await count(`${sel}{overflow:clip!important;overflow-clip-margin:14px!important}`);
  return { ink, unclipped, width: r.width };
}

// ── overlap ──────────────────────────────────────────────────────────────────────────────────────────────────────
/**
 * No text sits on another (CLAUDE.md: "No text may overlap or crowd numbers"): in every row-like box (a flex row or a
 * grid) the in-flow children that hold text are pairwise disjoint, and no two texts' visible glyphs meet (`checkText`:
 * this also catches a label that spills out of its column onto the next one).
 */
export async function checkOverlap(page: Page): Promise<string[]> {
  return [...(await checkRowBoxes(page)), ...(await checkText(page)).overlap];
}

async function checkRowBoxes(page: Page): Promise<string[]> {
  return page.evaluate(() => {
    const label = (el: Element) => el.tagName.toLowerCase() + [...el.classList].filter((c) => !/^s(velte)?-/.test(c)).map((c) => `.${c}`).join("");
    const out: string[] = [];
    const hit = (a: DOMRect, b: DOMRect) =>
      Math.min(a.right, b.right) - Math.max(a.left, b.left) > 0.5 && Math.min(a.bottom, b.bottom) - Math.max(a.top, b.top) > 0.5;
    for (const box of document.querySelectorAll<HTMLElement>("#app *")) {
      const s = getComputedStyle(box);
      const rowish = (s.display.endsWith("flex") && !s.flexDirection.startsWith("column")) || s.display.endsWith("grid");
      if (!rowish) continue;
      const kids = [...box.children]
        .filter((k): k is HTMLElement => k instanceof HTMLElement && !!k.textContent?.trim())
        .filter((k) => {
          const ks = getComputedStyle(k);
          return ks.position !== "absolute" && ks.position !== "fixed" && ks.visibility !== "hidden" && ks.display !== "none";
        })
        .map((k) => [k, k.getBoundingClientRect()] as const)
        .filter(([, r]) => r.width > 0 && r.height > 0);
      for (let i = 0; i < kids.length; i++)
        for (let j = i + 1; j < kids.length; j++)
          if (hit(kids[i]![1], kids[j]![1])) out.push(`in ${label(box)}: ${label(kids[i]![0])} "${kids[i]![0].textContent!.trim().slice(0, 30)}" overlaps ${label(kids[j]![0])} "${kids[j]![0].textContent!.trim().slice(0, 30)}"`);
    }
    return out;
  });
}

// ── hidden names ─────────────────────────────────────────────────────────────────────────────────────────────────
/**
 * Hidden-name integrity (R96, a regression check): with `real` the fixture names a scene hides, nothing under #app
 * (text or attributes: aria labels, tooltips, data attributes) carries any of them, and no name drawn in the hidden
 * style (the italic agent name / `Hidden player`) has a `#tag` beside it.
 */
export async function checkHiddenIntegrity(page: Page, real: string[]): Promise<string[]> {
  return page.evaluate((names) => {
    const out: string[] = [];
    const html = document.getElementById("app")!.outerHTML;
    for (const n of names) if (html.includes(n)) out.push(`the page contains the hidden name "${n}"`);
    for (const el of document.querySelectorAll("#app .hidden, #app .hid")) {
      const unit = el.parentElement!;
      if (/#[A-Za-z0-9]/.test(unit.textContent ?? "")) out.push(`a hidden name "${el.textContent?.trim()}" shows a #tag`);
    }
    return out;
  }, real);
}

/** The fixture names hidden per scene (spec §8.2): Agent Select hides Nova#3341 until she locks. */
export function hiddenRealNames(scene: string): string[] {
  return scene.startsWith("pregame") ? ["Nova", "3341"] : [];
}

// ── the full set ─────────────────────────────────────────────────────────────────────────────────────────────────
/**
 * Every check, each problem prefixed with its check's name. `noScroll`: nothing may scroll vertically either (the Live
 * board at the design's baseline size).
 */
export async function fitChecks(page: Page, scene: string, opts: { noScroll?: boolean } = {}): Promise<string[]> {
  const text = await checkText(page);
  return [
    ...(await checkAppBox(page)).map((p) => `app: ${p}`),
    ...(await checkScrolling(page, opts.noScroll)).map((p) => `scroll: ${p}`),
    ...[...text.clip, ...(await checkDecorations(page))].map((p) => `clip: ${p}`),
    ...[...(await checkRowBoxes(page)), ...text.overlap].map((p) => `overlap: ${p}`),
    ...(await checkType(page)).map((p) => `type: ${p}`),
    ...(await checkFonts(page)).map((p) => `fonts: ${p}`),
    ...(await checkHiddenIntegrity(page, hiddenRealNames(scene))).map((p) => `hidden: ${p}`),
  ];
}
