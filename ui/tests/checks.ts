// Spec §8.3 global assertions as functions. Each returns a list of problems (empty = pass), so a failing test
// prints what is wrong and where. Selectors are the components' own class names (Svelte keeps them).
import type { Page } from "@playwright/test";

/** 1. #app is exactly 0,0,1000,800 and the document never scrolls. */
export async function checkAppBox(page: Page): Promise<string[]> {
  return page.evaluate(() => {
    const out: string[] = [];
    const r = document.getElementById("app")!.getBoundingClientRect();
    if (r.x !== 0 || r.y !== 0 || r.width !== 1000 || r.height !== 800) out.push(`#app is ${r.x},${r.y} ${r.width}×${r.height}`);
    const se = document.scrollingElement!;
    if (se.scrollWidth > 1000) out.push(`document scrollWidth ${se.scrollWidth}`);
    if (se.scrollHeight > 800) out.push(`document scrollHeight ${se.scrollHeight}`);
    return out;
  });
}

/**
 * 2. Live scenes: nothing inside the Live grid overflows its box (name spans excepted); no scrollbar anywhere.
 * The Underline style's bar is left out of the measurement: the design hangs it 6px below the newest tile
 * (v8:125, spec §3.8), so it overflows its tile by construction. It is decoration inside the card, never
 * clipped (fit-live's Underline geometry test asserts it stays inside the card).
 */
export async function checkLiveNoOverflow(page: Page): Promise<string[]> {
  return page.evaluate(() => {
    const out: string[] = [];
    const grid = document.querySelector(".live");
    if (!grid) return ["no .live grid"];
    const hide = document.createElement("style");
    hide.textContent = ".uline{display:none!important}";
    document.head.appendChild(hide);
    const label = (el: Element) => `${el.tagName.toLowerCase()}.${[...el.classList].filter((c) => !/^s(velte)?-/.test(c)).join(".")}`;
    for (const el of [grid, ...grid.querySelectorAll("*")]) {
      if (!(el instanceof HTMLElement)) continue;
      if (el.closest("[data-ellipsis]")) continue;
      if (el.scrollHeight > el.clientHeight + 1 || el.scrollWidth > el.clientWidth + 1) {
        out.push(`${label(el)} overflows: ${el.scrollWidth}×${el.scrollHeight} in ${el.clientWidth}×${el.clientHeight}`);
      }
    }
    for (const el of document.querySelectorAll<HTMLElement>("#app, #app *")) {
      const s = getComputedStyle(el);
      const scrolls = (v: string) => v === "scroll" || v === "auto";
      if ((scrolls(s.overflowY) && el.scrollHeight > el.clientHeight) || (scrolls(s.overflowX) && el.scrollWidth > el.clientWidth)) {
        out.push(`${label(el)} scrolls`);
      }
    }
    hide.remove();
    return out;
  });
}

/** 4. Every rendered text is at least 12px. */
export async function checkFontFloor(page: Page): Promise<string[]> {
  return page.evaluate(() => {
    const out: string[] = [];
    const walker = document.createTreeWalker(document.getElementById("app")!, NodeFilter.SHOW_TEXT);
    for (let n = walker.nextNode(); n; n = walker.nextNode()) {
      if (!n.textContent?.trim()) continue;
      const px = parseFloat(getComputedStyle(n.parentElement!).fontSize);
      if (px < 12) out.push(`"${n.textContent.trim()}" is ${px}px`);
    }
    return out;
  });
}

/**
 * 5. Nothing clips except the `data-ellipsis` spans. A truncated allowed span ends in an ellipsis, and a truncated
 * Riot-ID name keeps its tag fully visible. The Peak and Prev rows are strict.
 * Vertically, nothing clips at all, the allowed spans included: every text's glyph box (its font's ascent to
 * descent) lies inside each clipping ancestor's paint box (`overflow-clip-margin` counted), and every text
 * decoration paints whole (`checkDecorations`: the underline hangs below the glyph box, where no DOM rect reaches).
 */
export async function checkClipping(page: Page): Promise<string[]> {
  return [...(await checkHorizontal(page)), ...(await checkVertical(page)), ...(await checkDecorations(page))];
}

async function checkVertical(page: Page): Promise<string[]> {
  return page.evaluate(() => {
    const out: string[] = [];
    const app = document.getElementById("app")!;
    /**
     * The page's one scroll container (the Details/Match `.scroll`, App.svelte's child of #app) does not clip its
     * content: what lies past its edge is reached by scrolling. Its ancestors clip the container's own box, never that
     * content, so the walk up from a text stops there (task 17 re-review: `auto` used to count as a clip, and every
     * text below the fold of a Details page was reported as cut by `.scroll`, then by `#app`). Only that element: an
     * element with only `overflow-x: hidden` computes `overflow-y: auto` too, and a cut under it must still count
     * (Task 18 review, Minor 9; check 3 reports any other element that scrolls).
     */
    const host = app.querySelector(":scope > .scroll");
    const scrolls = (el: HTMLElement) => {
      if (el !== host) return false;
      const y = getComputedStyle(el).overflowY;
      return y === "auto" || y === "scroll"; // made non-scrolling (a test does), it clips like any box
    };
    /** Where `el` stops painting its content, or null when it does not clip in y. */
    const clipBox = (el: HTMLElement): { top: number; bottom: number } | null => {
      const s = getComputedStyle(el);
      if (s.overflowY === "visible") return null;
      const r = el.getBoundingClientRect();
      // overflow:clip may paint past the padding box by overflow-clip-margin; hidden/auto/scroll never do.
      const m = s.overflowY === "clip" ? parseFloat(s.getPropertyValue("overflow-clip-margin")) || 0 : 0;
      return { top: r.top + parseFloat(s.borderTopWidth) - m, bottom: r.bottom - parseFloat(s.borderBottomWidth) + m };
    };
    const walker = document.createTreeWalker(app, NodeFilter.SHOW_TEXT);
    for (let n = walker.nextNode(); n; n = walker.nextNode()) {
      if (!n.textContent?.trim()) continue;
      const range = document.createRange();
      range.selectNodeContents(n);
      const g = range.getBoundingClientRect();
      if (g.height === 0) continue;
      for (let el = n.parentElement; el && el !== app.parentElement; el = el.parentElement) {
        if (scrolls(el)) break;
        const c = clipBox(el);
        if (c && (g.top < c.top - 0.5 || g.bottom > c.bottom + 0.5)) {
          out.push(`"${n.textContent.trim()}" is cut vertically by ${el.tagName.toLowerCase()}.${[...el.classList].filter((k) => !/^s(velte)?-/.test(k)).join(".")}`);
          break;
        }
      }
    }
    return out;
  });
}

/**
 * Every element that draws a text decoration (today: the remembered name's dotted underline, R128) paints it
 * whole: `decorationInk` below, once per element, tagged for the probe and untagged after.
 */
async function checkDecorations(page: Page): Promise<string[]> {
  const labels = await page.evaluate(() => {
    const found: string[] = [];
    for (const el of document.querySelectorAll<HTMLElement>("#app *")) {
      if (getComputedStyle(el).textDecorationLine === "none" || !el.textContent?.trim() || el.getClientRects().length === 0) continue;
      el.dataset.decoProbe = String(found.length);
      found.push(el.textContent.trim());
    }
    return found;
  });
  const out: string[] = [];
  for (const [i, text] of labels.entries()) {
    const { ink, unclipped, width } = await decorationInk(page, `[data-deco-probe="${i}"]`);
    // A 1px dotted line lights about half the pixels under the span; a quarter is a safe floor.
    if (unclipped < Math.floor(width / 4)) out.push(`"${text}": its text decoration does not paint (${unclipped}px of ink)`);
    else if (ink < unclipped) out.push(`"${text}": a clip eats ${unclipped - ink} of ${unclipped} decoration pixels`);
  }
  await page.evaluate(() => document.querySelectorAll("[data-deco-probe]").forEach((el) => el.removeAttribute("data-deco-probe")));
  return out;
}

async function checkHorizontal(page: Page): Promise<string[]> {
  return page.evaluate(() => {
    const out: string[] = [];
    const seen = new Set<Element>();
    const walker = document.createTreeWalker(document.getElementById("app")!, NodeFilter.SHOW_TEXT);
    for (let n = walker.nextNode(); n; n = walker.nextNode()) {
      if (!n.textContent?.trim()) continue;
      // The nearest box that can be measured (inline spans report 0×0).
      let el: HTMLElement | null = n.parentElement;
      while (el && getComputedStyle(el).display === "inline") el = el.parentElement;
      if (!el || seen.has(el) || el.getClientRects().length === 0) continue;
      seen.add(el);
      if (el.scrollWidth <= el.clientWidth) continue;
      const text = el.textContent!.trim();
      if (!el.hasAttribute("data-ellipsis")) {
        out.push(`"${text}" clips: ${el.scrollWidth} > ${el.clientWidth}`);
        continue;
      }
      if (getComputedStyle(el).textOverflow !== "ellipsis") out.push(`"${text}" is cut without an ellipsis`);
      const tag = el.nextElementSibling as HTMLElement | null;
      if (el.classList.contains("n") && tag?.classList.contains("t")) {
        const wrap = el.parentElement!.getBoundingClientRect();
        const t = tag.getBoundingClientRect();
        if (tag.scrollWidth > tag.clientWidth || t.right > wrap.right + 0.5) out.push(`tag "${tag.textContent}" is cut`);
      }
    }
    return out;
  });
}

/**
 * Ink probe for a text decoration that hangs below its text (fix round 1, F7; check 5 runs it on every decorated
 * element). Ink outside an `overflow:hidden` box never shows up in `scrollHeight` or in any DOM rect, so a clipped
 * underline passes every box measurement. For the first match of `sel` this screenshots the box plus a 10px band under it twice, with and
 * without `text-decoration`, and counts the pixels that differ: that is the decoration's ink. It counts once as
 * rendered and once with the clip pushed 14px out (`overflow:clip` + `overflow-clip-margin`, which keeps the
 * text-overflow ellipsis, so the decoration's extent is the same in both), so `ink < unclipped` means the clip is
 * eating part of the decoration (a font that puts the underline half inside the box still shows some ink, which
 * is why a plain "some ink" floor is not enough). The caller keeps the pointer away so no tooltip is in the crop.
 */
export async function decorationInk(page: Page, sel: string): Promise<{ ink: number; unclipped: number; width: number }> {
  const loc = page.locator(sel).first();
  // Details and Match scroll: bring the element (and the 10px band under it) into the viewport first, since a
  // screenshot clip can't reach past it. Only when it is outside: a Live page never scrolls, and Playwright's own
  // scrollIntoViewIfNeeded made this probe flaky there (under 8 workers: 1 failure in 24 runs; with this guard 0 in 72).
  await loc.evaluate((el) => {
    const r = el.getBoundingClientRect();
    if (r.top < 0 || r.bottom + 12 > window.innerHeight) el.scrollIntoView({ block: "center" });
  });
  const r = await loc.boundingBox();
  if (!r) throw new Error(`no box for ${sel}`);
  const clip = { x: Math.floor(r.x) - 1, y: Math.floor(r.y) - 2, width: Math.ceil(r.width) + 2, height: Math.ceil(r.height) + 12 };
  const count = async (extra: string): Promise<number> => {
    const base = await page.addStyleTag({ content: extra || "/* as rendered */" });
    const withDeco = await page.screenshot({ clip });
    const off = await page.addStyleTag({ content: `${sel}{text-decoration:none!important}` });
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

/**
 * R125/R126 (spec §3.6 Prev): the widest Prev row must fit its 158px column unclipped. For every card's Prev row
 * that shows a tier name, each of `labels` is written into the name in turn and the name and the row are measured
 * (the icon is a fixed 18px, so the text is all that varies). The rows' own text is put back afterwards.
 */
export async function checkPrevNames(page: Page, labels: string[]): Promise<string[]> {
  return page.evaluate((names) => {
    const out: string[] = [];
    const rows = [...document.querySelectorAll<HTMLElement>(".live .card .prev")].filter((r) => r.querySelector(".pname"));
    if (rows.length === 0) return ["no Prev row with a tier name"];
    rows.forEach((row, ri) => {
      const name = row.querySelector<HTMLElement>(".pname")!;
      const own = name.textContent;
      for (const n of names) {
        name.textContent = n;
        if (name.scrollWidth > name.clientWidth || row.scrollWidth > row.clientWidth) {
          out.push(`row ${ri}: "Prev ${n}" clips (${name.scrollWidth} > ${name.clientWidth})`);
        }
      }
      name.textContent = own;
    });
    return out;
  }, labels);
}

/**
 * 6. Within each card the name wrapper, ⟲ slot, Lv line, rank text, Peak row, Prev row, the 3 stat blocks and the
 * last-5 block are pairwise disjoint, and the party tab touches no text.
 */
export async function checkCardOverlap(page: Page): Promise<string[]> {
  return page.evaluate(() => {
    const out: string[] = [];
    const cards = [...document.querySelectorAll(".live .card")];
    const hit = (a: DOMRect, b: DOMRect) =>
      Math.min(a.right, b.right) - Math.max(a.left, b.left) > 0.5 && Math.min(a.bottom, b.bottom) - Math.max(a.top, b.top) > 0.5;
    cards.forEach((card, ci) => {
      const parts: Array<[string, DOMRect]> = [];
      const need = (sel: string, n = 1) => {
        const els = [...card.querySelectorAll(sel)];
        if (els.length !== n) out.push(`card ${ci}: ${els.length} × ${sel}, want ${n}`);
        els.forEach((e, i) => parts.push([n > 1 ? `${sel}[${i}]` : sel, e.getBoundingClientRect()]));
      };
      need(".nm");
      need(".encslot");
      need(".lv");
      need(".rtext");
      need(".peak");
      need(".prev");
      need(".stat", 3);
      need(".l5");
      for (let i = 0; i < parts.length; i++)
        for (let j = i + 1; j < parts.length; j++)
          if (hit(parts[i]![1], parts[j]![1])) out.push(`card ${ci}: ${parts[i]![0]} overlaps ${parts[j]![0]}`);
      const tab = card.querySelector(".ptab");
      if (tab) {
        const tr = tab.getBoundingClientRect();
        const walker = document.createTreeWalker(card, NodeFilter.SHOW_TEXT);
        for (let n = walker.nextNode(); n; n = walker.nextNode()) {
          if (!n.textContent?.trim()) continue;
          const range = document.createRange();
          range.selectNodeContents(n);
          for (const r of range.getClientRects()) if (hit(tr, r)) out.push(`card ${ci}: party tab covers "${n.textContent.trim()}"`);
        }
      }
    });
    return out;
  });
}

/** 7. No status words anywhere. */
export async function checkNoStatusText(page: Page): Promise<string[]> {
  const text = await page.locator("#app").innerText();
  const m = text.match(/offline|last updated|beta|saved data|loading|unavailable|searching|queue/i);
  return m ? [`status text "${m[0]}"`] : [];
}

/** 8. #app uses the selected family and that family is loaded. */
export async function checkFont(page: Page, family: string): Promise<string[]> {
  return page.evaluate((f) => {
    const out: string[] = [];
    const ff = getComputedStyle(document.getElementById("app")!).fontFamily;
    if (!ff.replace(/["']/g, "").startsWith(f)) out.push(`font-family is ${ff}`);
    if (!document.fonts.check(`600 16px "${f}"`)) out.push(`${f} 600 is not loaded`);
    return out;
  }, family);
}

/**
 * 9. Hidden-name integrity (regression check only, R96): a card drawn in the hidden style carries no real name
 * anywhere in its DOM, attributes included. `real` = the fixture names hidden in this scene. A hidden card is one
 * whose name has the eye-off icon and no tag: that covers `Hidden Player`, the agent name, and an agent name that
 * is still a meta-pending bar (no `.hid` yet). A remembered name has the eye too, but keeps its tag.
 */
export async function checkHiddenIntegrity(page: Page, real: string[]): Promise<string[]> {
  return page.evaluate((names) => {
    const out: string[] = [];
    for (const card of document.querySelectorAll(".live .card")) {
      const nm = card.querySelector(".nm");
      if (!nm?.querySelector("svg") || nm.querySelector(".t")) continue;
      const html = card.outerHTML;
      if (/#[A-Za-z0-9]/.test(card.textContent ?? "")) out.push("a hidden card shows a #tag");
      for (const n of names) if (html.includes(n)) out.push(`a hidden card contains "${n}"`);
    }
    // Details and Match (R93, R18): every name drawn in a hidden style (eye-off, no tag) shows no tag and no real name
    // in its own unit: the hero, a Top 3 row, a scoreboard row. A player hidden on the live screen is hidden on every
    // page, so the whole Details or Match DOM (attributes included) must not carry any of `real`.
    const page = document.querySelector("#app .details, #app .match");
    if (page) {
      const what = page.classList.contains("details") ? "Details" : "Match";
      for (const nm of page.querySelectorAll(".nm")) {
        if (!nm.querySelector("svg") || nm.querySelector(".t")) continue;
        const unit = nm.closest(".hero, .mate, .srow") ?? nm;
        const label = `a hidden ${unit === nm ? "name" : (unit.classList[0] ?? "unit")} in ${what}`;
        const own = unit.classList.contains("hero") ? nm : unit; // the hero also holds the rank and stats
        if (/#[A-Za-z0-9]/.test(own.textContent ?? "")) out.push(`${label} shows a #tag`);
        for (const n of names) if (unit.outerHTML.includes(n)) out.push(`${label} contains "${n}"`);
      }
      for (const n of names) if (page.outerHTML.includes(n)) out.push(`the ${what} page contains "${n}"`);
    }
    return out;
  }, real);
}

/** The fixture names hidden per scene (spec §8.2): Agent Select hides Nova#3341 until she locks. */
export function hiddenRealNames(scene: string): string[] {
  return scene.startsWith("pregame") ? ["Nova", "3341"] : [];
}

/**
 * 3. Details / Match: the one scroll container scrolls vertically only. Its scrollWidth fits its width, and nothing
 * inside it is a scroll container of its own (no nested scrollbar, in either axis).
 */
export async function checkScrollX(page: Page): Promise<string[]> {
  return page.evaluate(() => {
    const out: string[] = [];
    const sc = document.querySelectorAll<HTMLElement>("#app .scroll");
    if (sc.length !== 1) return [`${sc.length} scroll containers, want 1`];
    const el = sc[0]!;
    // overflow-y:auto makes overflow-x compute to auto too: fine as long as nothing is wider than the container.
    if (el.scrollWidth > el.clientWidth) out.push(`.scroll scrollWidth ${el.scrollWidth} > ${el.clientWidth}`);
    for (const d of el.querySelectorAll<HTMLElement>("*")) {
      const s = getComputedStyle(d);
      const can = (v: string) => v === "auto" || v === "scroll";
      if ((can(s.overflowY) && d.scrollHeight > d.clientHeight) || (can(s.overflowX) && d.scrollWidth > d.clientWidth)) {
        out.push(`${d.tagName.toLowerCase()}.${[...d.classList].filter((c) => !/^s(velte)?-/.test(c)).join(".")} scrolls inside .scroll`);
      }
    }
    return out;
  });
}

/** Checks 1, 3, 4, 5, 7, 8, 9 (the Details / Match set; spec §8.3, check 9 a regression check only, R96). */
export async function detailsChecks(page: Page, scene: string, family: string): Promise<string[]> {
  const scroller = page.locator("#app .scroll");
  const top = await scroller.evaluate((el) => el.scrollTop);
  const out = [
    ...(await checkAppBox(page)).map((p) => `1: ${p}`),
    ...(await checkScrollX(page)).map((p) => `3: ${p}`),
    ...(await checkFontFloor(page)).map((p) => `4: ${p}`),
    ...(await checkClipping(page)).map((p) => `5: ${p}`),
    ...(await checkNoStatusText(page)).map((p) => `7: ${p}`),
    ...(await checkFont(page, family)).map((p) => `8: ${p}`),
    ...(await checkHiddenIntegrity(page, hiddenRealNames(scene))).map((p) => `9: ${p}`),
  ];
  // The decoration probe scrolls its target into view; leave the page where it was.
  await scroller.evaluate((el, t) => (el.scrollTop = t), top);
  return out;
}

/** Checks 1, 2, 4, 5, 6, 7, 8, 9 (the Live set). */
export async function liveChecks(page: Page, scene: string, family: string): Promise<string[]> {
  return [
    ...(await checkAppBox(page)).map((p) => `1: ${p}`),
    ...(await checkLiveNoOverflow(page)).map((p) => `2: ${p}`),
    ...(await checkFontFloor(page)).map((p) => `4: ${p}`),
    ...(await checkClipping(page)).map((p) => `5: ${p}`),
    ...(await checkCardOverlap(page)).map((p) => `6: ${p}`),
    ...(await checkNoStatusText(page)).map((p) => `7: ${p}`),
    ...(await checkFont(page, family)).map((p) => `8: ${p}`),
    ...(await checkHiddenIntegrity(page, hiddenRealNames(scene))).map((p) => `9: ${p}`),
  ];
}

/**
 * My Stats, no overlap and no crowding (spec §9.7, the page's version of check 6): inside each row-like group of the
 * page, the direct children that are on screen do not overlap, and two on the same line keep at least 8px between them
 * (the grids use 12px; a number that widened its column would eat into it). Text that overflows its own box is check 5.
 */
export async function checkMyStatsLayout(page: Page): Promise<string[]> {
  return page.evaluate(() => {
    const out: string[] = [];
    const groups = [
      ".rank .head",
      ".rank .name",
      ".rank .cells",
      ".rank .cell",
      ".rank .crow",
      ".session .cell",
      ".last5 .rhead",
      ".last5 .rrow",
      ".last5 .rmap",
      ".last5 .rscore",
      ".maps .thead",
      ".maps .trow",
      ".agents .thead",
      ".agents .trow",
      ".weekday .cell",
      ".daypart .cell",
      ".insights .irow",
    ];
    // Cells that sit edge to edge by design (the K / D / A grid, the session's padded cells, the columns of the two cell
    // panels) only must not overlap: their padding keeps the content apart.
    const butting = [".last5 .kda", ".session .sess", ".weekday .cells", ".daypart .cells"];
    const label = (el: Element) => `${el.tagName.toLowerCase()}.${[...el.classList].filter((c) => !/^s(velte)?-/.test(c)).join(".")}`;
    for (const sel of [...groups, ...butting]) {
      const gap = !butting.includes(sel);
      document.querySelectorAll(sel).forEach((group, gi) => {
        const kids = [...group.children]
          .map((el) => [el, el.getBoundingClientRect()] as const)
          .filter(([, r]) => r.width > 0 && r.height > 0);
        for (let i = 0; i < kids.length; i++) {
          for (let j = i + 1; j < kids.length; j++) {
            const [a, ra] = kids[i]!;
            const [b, rb] = kids[j]!;
            const x = Math.min(ra.right, rb.right) - Math.max(ra.left, rb.left); // > 0: they overlap horizontally
            const y = Math.min(ra.bottom, rb.bottom) - Math.max(ra.top, rb.top);
            if (x > 0.5 && y > 0.5) out.push(`${sel}[${gi}]: ${label(a)} overlaps ${label(b)}`);
            else if (gap && y > 0.5 && x <= 0 && -x < 7.5) out.push(`${sel}[${gi}]: ${label(a)} is ${(-x).toFixed(1)}px from ${label(b)}`);
          }
        }
      });
    }
    return out;
  });
}

/** Checks 1, 3, 4, 5, 7, 8 and the My Stats layout check (9, spec §9.7). */
export async function myStatsChecks(page: Page, family: string): Promise<string[]> {
  const scroller = page.locator("#app .scroll");
  const top = await scroller.evaluate((el) => el.scrollTop);
  const out = [
    ...(await checkAppBox(page)).map((p) => `1: ${p}`),
    ...(await checkScrollX(page)).map((p) => `3: ${p}`),
    ...(await checkFontFloor(page)).map((p) => `4: ${p}`),
    ...(await checkClipping(page)).map((p) => `5: ${p}`),
    ...(await checkNoStatusText(page)).map((p) => `7: ${p}`),
    ...(await checkFont(page, family)).map((p) => `8: ${p}`),
    ...(await checkMyStatsLayout(page)).map((p) => `9: ${p}`),
  ];
  // The decoration probe scrolls its target into view; leave the page where it was.
  await scroller.evaluate((el, t) => (el.scrollTop = t), top);
  return out;
}
