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
    const label = (el: Element) => `${el.tagName.toLowerCase()}.${[...el.classList].filter((c) => !c.startsWith("svelte-")).join(".")}`;
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
 */
export async function checkClipping(page: Page): Promise<string[]> {
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
 * anywhere in its DOM, attributes included. `real` = the fixture names hidden in this scene.
 */
export async function checkHiddenIntegrity(page: Page, real: string[]): Promise<string[]> {
  return page.evaluate((names) => {
    const out: string[] = [];
    for (const card of document.querySelectorAll(".live .card")) {
      if (!card.querySelector(".nm .hid")) continue;
      const html = card.outerHTML;
      if (/#[A-Za-z0-9]/.test(card.textContent ?? "")) out.push("a hidden card shows a #tag");
      for (const n of names) if (html.includes(n)) out.push(`a hidden card contains "${n}"`);
    }
    return out;
  }, real);
}

/** The fixture names hidden per scene (spec §8.2): Agent Select hides Nova#3341 until she locks. */
export function hiddenRealNames(scene: string): string[] {
  return scene.startsWith("pregame") ? ["Nova", "3341"] : [];
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
