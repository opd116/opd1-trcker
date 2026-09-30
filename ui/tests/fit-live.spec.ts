// Live page fit and per-scene assertions (spec §8.3–§8.6) at exactly 1000×800, DPR 1, against the DEV demo feed.
import { mkdirSync } from "node:fs";
import { join } from "node:path";
import { expect, test, type Locator, type Page } from "@playwright/test";
import {
  checkAppBox,
  checkCardOverlap,
  checkClipping,
  checkFont,
  checkFontFloor,
  checkLiveNoOverflow,
  checkNoStatusText,
  decorationInk,
  liveChecks,
} from "./checks";
import { ACCENTS, FONTS, fontSlug, openScene, pointerAway, SHOTS_DIR, watchFontCdn, type ThemeParams } from "./harness";

const LIVE_SCENES = [
  "closed", "closed-empty", "boot", "lobby1", "lobby2", "lobby3", "lobby4", "lobby5", "lobby2-loading", "lobby2-unranked",
  "pregame", "pregame-nomap", "pregame-loading", "ingame", "ingame-longnames", "ingame-parties4", "ingame-partial",
  "ingame-unranked", "ingame-loading", "ingame-nometa",
];

mkdirSync(SHOTS_DIR, { recursive: true });
const shot = (page: Page, name: string) => page.screenshot({ path: join(SHOTS_DIR, `${name}.png`) });

type Box = { x: number; y: number; width: number; height: number };
const box = async (l: Locator): Promise<Box> => {
  const b = await l.boundingBox();
  if (!b) throw new Error(`no box for ${l}`);
  return b;
};
/** ±0.5px (spec §8.4). */
const near = (actual: number, want: number, what: string) => expect(Math.abs(actual - want), `${what}: ${actual} vs ${want}`).toBeLessThanOrEqual(0.5);
const cardY = (i: number) => 59 + 147.8 * i;
const left = (page: Page) => page.locator(".live .col").nth(0);
const right = (page: Page) => page.locator(".live .col").nth(1);
const cardNamed = (page: Page, text: string | RegExp) => page.locator(".live .card").filter({ hasText: text });
const css = (l: Locator, prop: string) => l.evaluate((el, p) => getComputedStyle(el).getPropertyValue(p), prop);
/** A CSS variable resolved to rgb() through a probe element, so it compares with computed colours. */
const varRgb = (page: Page, name: string) =>
  page.evaluate((n) => {
    const p = document.createElement("div");
    p.style.color = `var(${n})`;
    document.body.appendChild(p);
    const c = getComputedStyle(p).color;
    p.remove();
    return c;
  }, name);

/** What each scene puts in the grid (spec §5.1–§5.7): cards, full skeleton cards, dashed slots. */
function slots(scene: string): [number, number, number] {
  if (scene === "closed") return [1, 0, 0];
  if (scene === "closed-empty") return [0, 0, 0];
  if (scene === "boot") return [0, 1, 0];
  if (scene.startsWith("lobby")) return [Number(scene[5]), 0, 0];
  if (scene === "pregame-loading") return [0, 5, 5];
  if (scene.startsWith("pregame")) return [5, 0, 5];
  if (scene === "ingame-loading") return [0, 10, 0];
  return [10, 0, 0];
}

for (const scene of LIVE_SCENES) {
  for (const font of FONTS) {
    test(`fit: ${scene} [${font}]`, async ({ page }) => {
      await openScene(page, scene, { font });
      const [cards, skeletons, dashed] = slots(scene);
      await expect(page.locator(".live .card")).toHaveCount(cards);
      await expect(page.locator(".live .skcard")).toHaveCount(skeletons);
      await expect(page.locator(".live .dashed")).toHaveCount(dashed);
      expect(await liveChecks(page, scene, font)).toEqual([]);
      await shot(page, `${scene}__${fontSlug(font)}`);
    });
  }
}

test("geometry: ingame", async ({ page }) => {
  await openScene(page, "ingame");
  const bar = await box(page.locator("header.bar"));
  near(bar.y, 0, "top bar y");
  near(bar.height, 49, "top bar height");

  const ind = page.locator("header.bar .ind");
  const ib = await box(ind);
  near(ib.height, 2, "tab indicator height");
  near(ib.y + ib.height, 49, "tab indicator bottom");
  expect(await css(ind, "background-color")).toBe(await varRgb(page, "--accent"));

  const map = await box(page.locator("header.bar .map img"));
  near(map.width, 116, "map width");
  near(map.height, 32, "map height");
  near(map.x + map.width, 982, "map right edge");

  for (const [c, x] of [[left(page), 10], [right(page), 505]] as const) {
    const cards = c.locator(".card");
    await expect(cards).toHaveCount(5);
    for (let i = 0; i < 5; i++) {
      const b = await box(cards.nth(i));
      near(b.x, x, `card ${x}/${i} x`);
      near(b.y, cardY(i), `card ${x}/${i} y`);
      near(b.width, 485, `card ${x}/${i} width`);
      near(b.height, 139.8, `card ${x}/${i} height`);
    }
  }

  // Card-relative parts, on Mara (right column, row 0: encounters and a likely party).
  const mara = cardNamed(page, "Mara");
  const o = await box(mara);
  const rel = async (sel: string) => {
    const b = await box(mara.locator(sel).first());
    return { x: b.x - o.x, y: b.y - o.y, w: b.width, h: b.height, r: b.x + b.width - o.x, b: b.y + b.height - o.y };
  };
  const agent = await rel(".who .agent");
  near(agent.x, 56, "agent x");
  near(agent.y, 12, "agent y");
  near(agent.w, 40, "agent w");
  near(agent.h, 40, "agent h");
  const ricon = await rel(".rank img");
  near(ricon.x, 313, "rank icon x");
  near(ricon.y, 12, "rank icon y");
  near(ricon.w, 40, "rank icon w");
  near(ricon.h, 40, "rank icon h");
  for (const [sel, x] of [[".peak", 56], [".prev", 313]] as const) {
    const r = await rel(sel);
    near(r.y, 62, `${sel} y`);
    near(r.h, 18, `${sel} h`);
    near(r.x, x, `${sel} x`);
  }
  const stats = mara.locator(".stat");
  await expect(stats).toHaveCount(3);
  for (let i = 0; i < 3; i++) {
    const b = await box(stats.nth(i));
    near(b.width, 76, `stat ${i} width`);
    near(b.x - o.x, 56 + 76 * i, `stat ${i} x`);
  }
  near((await rel(".stats")).b, 127.8, "stat trio bottom");
  const tiles = mara.locator(".tile");
  await expect(tiles).toHaveCount(5);
  for (let i = 0; i < 5; i++) {
    const b = await box(tiles.nth(i));
    near(b.width, 22, `tile ${i} w`);
    near(b.height, 22, `tile ${i} h`);
    near(b.y - o.y, 95.8, `tile ${i} y`);
  }
  near((await box(tiles.first())).x - o.x, 313, "first tile x");
  const lastTile = await box(tiles.last());
  near(lastTile.x + lastTile.width - o.x, 443, "last tile right");
  const arrow = await rel(".arrow");
  near(arrow.y, 120.8, "arrow top");
  near(arrow.b, 127.8, "arrow bottom");
  const slot = await rel(".encslot");
  near(slot.x, 261, "⟲ slot x");
  near(slot.r, 297, "⟲ slot right");
  const tab = await rel(".ptab");
  near(tab.x, 464, "party tab x");
  near(tab.r, 485, "party tab right");
  near(tab.y, 22, "party tab y");
  near(tab.b, 42, "party tab bottom");
  const stripe = await rel(".sline");
  near(stripe.x, 482, "stripe x");
  near(stripe.r, 485, "stripe right");
});

test("geometry: ingame [Underline]", async ({ page }) => {
  await openScene(page, "ingame", { last5: "Underline" });
  const fahad = cardNamed(page, "Fahad");
  const o = await box(fahad);
  const tiles = fahad.locator(".tile");
  for (let i = 0; i < 5; i++) {
    const b = await box(tiles.nth(i));
    near(b.y - o.y, 105.8, `tile ${i} y`);
    near(b.y + b.height - o.y, 127.8, `tile ${i} bottom`);
  }
  await expect(fahad.locator(".arrow")).toHaveCount(0);
  await expect(fahad.locator(".uline")).toHaveCount(1);
  await expect(tiles.last().locator(".uline")).toHaveCount(1);
  // The bar hangs 6px below the tile (v8:125) and stays fully inside the card, so nothing clips it.
  const bar = await box(fahad.locator(".uline"));
  near(bar.y - o.y, 105.8 + 22 + 6 - 2, "underline top");
  near(bar.height, 2, "underline height");
  expect(bar.y + bar.height).toBeLessThanOrEqual(o.y + o.height);
});

test("pregame: 5 cards + 5 dashed slots", async ({ page }) => {
  await openScene(page, "pregame");
  await expect(left(page).locator(".card")).toHaveCount(5);
  const slots = right(page).locator(".dashed");
  await expect(slots).toHaveCount(5);
  await expect(right(page).locator(".card")).toHaveCount(0);
  const card = await box(left(page).locator(".card").first());
  for (let i = 0; i < 5; i++) {
    const s = slots.nth(i);
    expect(await s.innerText()).toBe("");
    expect(await css(s, "border-top-style")).toBe("dashed");
    const b = await box(s);
    near(b.x, 505, `slot ${i} x`);
    near(b.y, cardY(i), `slot ${i} y`);
    near(b.width, card.width, `slot ${i} width`);
    near(b.height, card.height, `slot ${i} height`);
  }
  // R99: a hover looks exactly like no pick; only locked agents show an icon.
  await expect(cardNamed(page, "Fahad").locator(".who img")).toHaveCount(1);
  await expect(cardNamed(page, "Sable").locator(".who img")).toHaveCount(1);
  await expect(cardNamed(page, "VesperUnbreakabl").locator(".who img")).toHaveCount(0);
  await expect(cardNamed(page, "Drake").locator(".who img")).toHaveCount(0);
  const hidden = cardNamed(page, "Hidden Player");
  await expect(hidden).toHaveCount(1);
  await expect(hidden.locator(".who img")).toHaveCount(0);
  const hid = hidden.locator(".nm .hid");
  await expect(hid).toHaveText("Hidden Player");
  expect(await css(hid, "font-style")).toBe("italic");
});

for (let n = 1; n <= 5; n++) {
  test(`lobby${n}: n cards, APR iff n ≥ 2, only self outlined`, async ({ page }) => {
    await openScene(page, `lobby${n}`);
    const cards = left(page).locator(".card");
    await expect(cards).toHaveCount(n);
    expect(await right(page).locator(":scope > *").count()).toBe(0);
    await expect(page.locator("header.bar .apr")).toHaveCount(n >= 2 ? 1 : 0);
    await expect(page.locator(".live .self")).toHaveCount(1);
    await expect(cards.first().locator(".self")).toHaveCount(1);
    await expect(page.locator(".live .stripe, .live .ptab, .live .enc")).toHaveCount(0);
    for (let i = 0; i < n; i++) {
      const b = await box(cards.nth(i));
      near(b.x, 10, `card ${i} x`);
      near(b.y, cardY(i), `card ${i} y`);
      near(b.width, 485, `card ${i} width`);
      near(b.height, 139.8, `card ${i} height`);
    }
  });
}

test("closed: one card, empty right side", async ({ page }) => {
  await openScene(page, "closed");
  await expect(page.locator(".live .card")).toHaveCount(1);
  await expect(left(page).locator(".card .self")).toHaveCount(1);
  expect(await right(page).locator(":scope > *").count()).toBe(0);
  expect(await page.locator("header.bar .right > *").count()).toBe(0);
  const b = await box(page.locator(".live .card"));
  near(b.x, 10, "x");
  near(b.y, 59, "y");
  near(b.width, 485, "width");
  near(b.height, 139.8, "height");
});

test("closed-empty: no card, no skeleton", async ({ page }) => {
  await openScene(page, "closed-empty");
  await expect(page.locator(".live .card, .live .skcard, .live .dashed")).toHaveCount(0);
  expect(await page.locator(".live .col > *").count()).toBe(0);
  expect(await page.locator("header.bar .right > *").count()).toBe(0);
});

test("lobby2-loading / lobby2-unranked: APR skeleton / APR empty box + —", async ({ page }) => {
  await openScene(page, "lobby2-loading");
  const apr = page.locator("header.bar .apr");
  expect((await apr.innerText()).trim()).toBe("Avg party rank");
  const sk = apr.locator(".sk");
  await expect(sk).toHaveCount(2);
  const circle = await box(sk.nth(0));
  near(circle.width, 32, "circle w");
  near(circle.height, 32, "circle h");
  expect(await css(sk.nth(0), "border-radius")).toBe("50%");
  expect(await css(sk.nth(0), "background-color")).toBe(await varRgb(page, "--skel"));
  const bar = await box(sk.nth(1));
  near(bar.width, 52, "bar w");
  near(bar.height, 12, "bar h");
  await expect(apr.locator("img")).toHaveCount(0);

  await openScene(page, "lobby2-unranked");
  const apr2 = page.locator("header.bar .apr");
  expect((await apr2.innerText()).replace(/\s+/g, " ").trim()).toBe("Avg party rank —");
  await expect(apr2.locator("img, .sk")).toHaveCount(0);
  const empty = await box(apr2.locator(".ebox"));
  near(empty.width, 32, "empty box w");
  near(empty.height, 32, "empty box h");
});

for (const font of FONTS) {
  test(`ingame-longnames: the 123 ⟲ count doesn't intersect its tag; no Peak row clips [${font}]`, async ({ page }) => {
    await openScene(page, "ingame-longnames", { font });
    const names = page.locator(".live .card .nm .n");
    const count = await names.count();
    expect(count).toBeGreaterThanOrEqual(8);
    for (let i = 0; i < count; i++) {
      const n = names.nth(i);
      const truncated = await n.evaluate((el) => el.scrollWidth > el.clientWidth);
      expect(truncated, `name ${i} truncated`).toBe(true);
      expect(await css(n, "text-overflow")).toBe("ellipsis");
      const tag = n.locator("xpath=following-sibling::span[1]");
      await expect(tag).toHaveText("#WWWWW");
      expect(await tag.evaluate((el) => el.scrollWidth <= el.clientWidth)).toBe(true);
    }
    const zephyr = page.locator(".live .card").filter({ has: page.locator(".enc", { hasText: "123" }) });
    await expect(zephyr).toHaveCount(1);
    const enc = await box(zephyr.locator(".enc"));
    const tag = await box(zephyr.locator(".nm .t"));
    // Not just disjoint: never crowded either (a 5px gap, like the name–tag gap).
    expect(tag.x + tag.width + 5, "tag right + 5 ≤ ⟲ left").toBeLessThanOrEqual(enc.x + 0.5);
    const peaks = page.locator(".live .card .peak");
    for (let i = 0; i < (await peaks.count()); i++) {
      const clip = await peaks.nth(i).evaluate((row) =>
        [row, ...row.querySelectorAll("*")].some((el) => el.scrollWidth > el.clientWidth),
      );
      expect(clip, `peak row ${i} clips`).toBe(false);
    }
    expect(await checkClipping(page)).toEqual([]);
    expect(await checkCardOverlap(page)).toEqual([]);
  });
}

test('ingame-nometa: skeleton bars for the map name, tier names and hidden-agent name; no "undefined"/"null" text', async ({ page }) => {
  await openScene(page, "ingame-nometa");
  const text = await page.locator("#app").innerText();
  expect(text).not.toMatch(/undefined|null/);
  const html = await page.locator("#app").innerHTML();
  expect(html).not.toMatch(/undefined|>null</);
  const mapBar = page.locator("header.bar .map .sk");
  await expect(mapBar).toHaveCount(1);
  near((await box(mapBar)).width, 52, "map name bar w");
  near((await box(mapBar)).height, 12, "map name bar h");
  await expect(page.locator("header.bar .map img")).toHaveCount(1);
  const cards = page.locator(".live .card");
  await expect(cards).toHaveCount(10);
  for (let i = 0; i < 10; i++) {
    const tier = cards.nth(i).locator(".rank .rtext .sk").first();
    near((await box(tier)).width, 84, `card ${i} tier bar w`);
    near((await box(tier)).height, 13, `card ${i} tier bar h`);
    expect((await cards.nth(i).locator(".rank .rtext").innerText()).replace(/\s+/g, " ").trim()).toMatch(/^\d+ RR$/);
  }
  const cypher = cards.filter({ has: page.locator(".nm svg") }).filter({ hasNot: page.locator(".nm .t") });
  await expect(cypher).toHaveCount(1);
  const bar = cypher.locator(".nm .sk");
  near((await box(bar)).width, 120, "hidden agent bar w");
  near((await box(bar)).height, 13, "hidden agent bar h");
  expect((await cypher.locator(".nm").innerText()).trim()).toBe("");
});

test("last5 order: Fahad renders L W W L W and the rightmost is the newest style; 2-match player has 3 dashed tiles first", async ({ page }) => {
  await openScene(page, "ingame");
  const tiles = cardNamed(page, "Fahad").locator(".tile");
  expect(await tiles.allInnerTexts()).toEqual(["L", "W", "W", "L", "W"]);
  const newest = tiles.last();
  expect(await css(newest, "background-color")).toBe(await varRgb(page, "--rn-w-bg"));
  expect(await css(newest, "color")).toBe(await varRgb(page, "--on-solid"));
  expect(await css(tiles.nth(1), "background-color")).toBe(await varRgb(page, "--r-w-bg"));

  await openScene(page, "ingame-unranked");
  const kilo = cardNamed(page, "Kilo").locator(".tile");
  expect(await kilo.allInnerTexts()).toEqual(["", "", "", "W", "L"]);
  for (let i = 0; i < 3; i++) expect(await css(kilo.nth(i), "border-top-style")).toBe("dashed");
  const zephyr = cardNamed(page, "Zephyr");
  expect(await zephyr.locator(".tile").allInnerTexts()).toEqual(["", "", "", "", ""]);
  await expect(zephyr.locator(".arrow")).toHaveCount(1);
  expect((await zephyr.locator(".stat .v").allInnerTexts()).map((s) => s.trim())).toEqual(["—", "—", "—"]);
});

async function stripeColours(page: Page) {
  return page.locator(".live .card").evaluateAll((cards) =>
    cards.map((card) => {
      const line = card.querySelector<HTMLElement>(".sline");
      if (!line) return null;
      const s = getComputedStyle(line);
      const confirmed = s.backgroundImage === "none";
      const colour = confirmed ? s.backgroundColor : (s.backgroundImage.match(/rgb\([^)]+\)/)?.[0] ?? "");
      return { confirmed, gradient: s.backgroundImage.includes("repeating-linear-gradient"), colour };
    }),
  );
}

test("parties4: 4 distinct stripe colours = --party-0…3, likely = gradient, confirmed = solid, none on self/self-party", async ({ page }) => {
  await openScene(page, "ingame-parties4");
  const marks = await stripeColours(page);
  expect(marks[0]).toBeNull(); // Fahad (self)
  const want = await Promise.all([0, 1, 2, 3].map((g) => varRgb(page, `--party-${g}`)));
  // Roster order: Fahad, Sable, Vesper, Drake, Nova | Mara, Kilo, Zephyr, Rook, Cypher. Groups from the scene.
  const groups = [null, 1, 1, 0, 0, 2, 2, 3, 3, null];
  const confirmed = [null, false, false, true, true, false, false, true, true, null];
  marks.forEach((m, i) => {
    if (groups[i] === null) return expect(m, `card ${i}`).toBeNull();
    expect(m!.colour, `card ${i} colour`).toBe(want[groups[i]!]);
    expect(m!.confirmed, `card ${i} confirmed`).toBe(confirmed[i]);
    expect(m!.gradient, `card ${i} gradient`).toBe(!confirmed[i]);
  });
  expect(new Set(want).size).toBe(4);
});

/** Spec §1.6, verbatim. */
const PARTY_ROWS: Record<string, string[]> = {
  "#e3a54f": ["#7fa6e8", "#b596e0", "#b0c46c", "#db8fb4"],
  "#4fc3ae": ["#7fa6e8", "#b596e0", "#b0c46c", "#db8fb4"],
  "#7fb1ef": ["#5fbfa9", "#c99ad8", "#b0c46c", "#db8fb4"],
  "#e58a9a": ["#7fa6e8", "#5fbfa9", "#b0c46c", "#b596e0"],
};
const hexRgb = (h: string) => `rgb(${[1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16)).join(", ")})`;

for (const accent of ACCENTS) {
  test(`party colours: ingame-parties4 [${accent}]`, async ({ page }) => {
    await openScene(page, "ingame-parties4", { accent });
    const marks = await stripeColours(page);
    const byGroup = [marks[3], marks[1], marks[5], marks[7]].map((m) => m!.colour);
    expect(byGroup).toEqual(PARTY_ROWS[accent]!.map(hexRgb));
    await shot(page, `ingame-parties4__${accent.slice(1)}`);
  });
}

const OPTIONS: Array<[string, ThemeParams]> = [
  ["Underline", { last5: "Underline" }],
  ["Solid", { fill: "Solid" }],
  ["Accent-Red", { results: "Accent / Red" }],
  ["Accent-Gray", { results: "Accent / Gray" }],
  ["Cool", { tone: "Cool" }],
  ["Warm", { tone: "Warm" }],
  ...ACCENTS.map((a): [string, ThemeParams] => [a, { accent: a }]),
];

for (const [label, params] of OPTIONS) {
  test(`options: ingame [${label}]`, async ({ page }) => {
    await openScene(page, "ingame", params);
    const problems = [
      ...(await checkAppBox(page)),
      ...(await checkLiveNoOverflow(page)),
      ...(await checkFontFloor(page)),
      ...(await checkClipping(page)),
      ...(await checkCardOverlap(page)),
      ...(await checkNoStatusText(page)),
      ...(await checkFont(page, "Barlow")),
    ];
    expect(problems).toEqual([]);
    await shot(page, `ingame__${label.replace("#", "")}`);
  });
}

test("the ⟲ slot stays 36px wide for 1–2 digit counts in all 3 fonts (design geometry)", async ({ page }) => {
  for (const font of FONTS) {
    await openScene(page, "ingame", { font });
    // Stretch every visible badge to 2 digits: the widest count the design's 36px slot must still hold.
    const widths = await page.locator(".live .card .enc").evaluateAll((els) =>
      els.map((el) => {
        el.lastChild!.textContent = "88";
        return (el.parentElement as HTMLElement).getBoundingClientRect().width;
      }),
    );
    expect(widths.length).toBeGreaterThan(0);
    for (const w of widths) expect(Math.abs(w - 36), `${font}: slot ${w}`).toBeLessThanOrEqual(0.5);
  }
});

test("tier name right edge ≤ 463 for Ascendant 3 in all 3 fonts", async ({ page }) => {
  for (const font of FONTS) {
    await openScene(page, "ingame-longnames", { font });
    const edges = await page.locator(".live .card").evaluateAll((cards) =>
      cards.map((c) => {
        const t = c.querySelector(".rank .tier")!;
        return { text: t.textContent, right: t.getBoundingClientRect().right - c.getBoundingClientRect().left };
      }),
    );
    expect(edges).toHaveLength(10);
    for (const e of edges) {
      expect(e.text).toBe("Ascendant 3");
      expect(e.right, `${font}`).toBeLessThanOrEqual(463);
    }
  }
});

test("no request to fonts.googleapis.com / fonts.gstatic.com", async ({ page }) => {
  const hits = watchFontCdn(page);
  for (const font of FONTS) await openScene(page, "ingame", { font });
  expect(hits).toEqual([]);
});

// F7 (fix round 1): CLAUDE.md "Card indicators" defines the remembered style as eye-off + dotted-underlined name +
// tag. The name span clips (`overflow:hidden`, for the ellipsis), and the underline hangs below the text, so the
// clip box needs room for it. Check 5 sees only horizontal overflow, hence this pixel probe: the underline must
// paint, and the clip must not eat any of it (same pixels with the clip pushed out). The name component is one
// piece; the hero and row sizes are probed by putting the card's `.nm` into those sizes.
for (const font of FONTS) {
  test(`remembered name: the dotted underline is drawn at card, hero and row size [${font}]`, async ({ page }) => {
    for (const scene of ["ingame", "ingame-longnames"]) {
      await openScene(page, scene, { font });
      await pointerAway(page);
      await expect(page.locator(".live .card .nm .rem")).toHaveCount(1);
      for (const size of ["card", "hero", "row"] as const) {
        await page.evaluate((sz) => {
          const nm = document.querySelector(".live .card .nm:has(.rem)")!;
          nm.classList.remove("size-card", "size-hero", "size-row");
          nm.classList.add(`size-${sz}`);
        }, size);
        const { ink, unclipped, width } = await decorationInk(page, ".live .card .nm .rem");
        // A 1px dotted line lights about half the pixels under the span; a quarter is a safe floor.
        expect(unclipped, `${scene} ${size}: the underline itself paints (${width}px name)`).toBeGreaterThanOrEqual(Math.floor(width / 4));
        expect(ink, `${scene} ${size}: the clip eats ${unclipped - ink} of ${unclipped} underline pixels`).toBe(unclipped);
      }
    }
  });
}

// F3/F12 (task 17 review): the level line has three states. Absent (still loading, R23) → the skeleton card's own
// 48×10 bar inline in the Lv line, so the name column keeps its height; withheld (null, R100) → `Lv —`.
test("level: absent = a 48×10 bar in the Lv line (name column keeps its height); withheld = Lv —", async ({ page }) => {
  await openScene(page, "ingame-partial");
  const cards = page.locator(".live .card");
  const absent = cards.nth(2); // Vesper: `level` key deleted
  const bar = absent.locator(".lv .sk");
  await expect(bar).toHaveCount(1);
  const b = await box(bar);
  near(b.width, 48, "level bar width");
  near(b.height, 10, "level bar height");
  expect(await css(bar, "border-radius")).toBe("3px");
  expect(await css(bar, "background-color")).toBe(await varRgb(page, "--skel"));
  expect((await absent.locator(".lv").innerText()).trim()).toBe("");
  const known = cards.nth(0); // Fahad: a number
  expect((await known.locator(".lv").innerText()).replace(/\s+/g, " ").trim()).toMatch(/^Lv \d+$/);
  near((await box(absent.locator(".ncol"))).height, (await box(known.locator(".ncol"))).height, "name column height");
  near((await box(absent.locator(".lv"))).height, (await box(known.locator(".lv"))).height, "Lv line height");

  await openScene(page, "ingame-unranked");
  const withheld = cardNamed(page, "Zephyr"); // level: null
  expect((await withheld.locator(".lv").innerText()).replace(/\s+/g, " ").trim()).toBe("Lv —");
  await expect(withheld.locator(".lv .sk")).toHaveCount(0);
});

// F5/F11: the party stripe and tab come before the grid in the DOM (design v8:44-49; markup order is asserted in
// PlayerCard.test.ts) and z-index paints them above it: a hit test on them finds them, not the grid.
test("party stripe and tab paint above the card grid", async ({ page }) => {
  await openScene(page, "ingame-parties4");
  const sable = cardNamed(page, "Sable");
  const hit = async (sel: string, dx: number, dy: number) => {
    const b = await box(sable.locator(sel).first());
    return page.evaluate(([x, y, s]) => document.elementFromPoint(x as number, y as number)?.closest(s as string) != null, [b.x + dx, b.y + dy, sel] as const);
  };
  const tab = await box(sable.locator(".ptab"));
  expect(await hit(".ptab", tab.width / 2, tab.height / 2), "tab hit").toBe(true);
  const stripe = await box(sable.locator(".stripe"));
  expect(await hit(".stripe", stripe.width / 2, stripe.height - 20), "stripe hit").toBe(true);
});

// F8 (task 17 review, ruled in fix round 2: keep): the agent box is one look in every stage. Lobby and Closed have no
// agent to show, and the card stays identical to every other stage's (CLAUDE.md "the card is identical everywhere";
// R99 calls this state "the empty agent slot"): the same empty 40×40 `--raised` div that Agent Select draws before a
// lock, never an image and never a skeleton shape. The cards around it are populated (no `.sk` anywhere), so the box
// reads as a slot beside real values, not as a loading card.
type BoxLook = { tag: string; w: number; h: number; bg: string; radius: string; cls: string };
const emptyAgentBoxes = (page: Page): Promise<BoxLook[]> =>
  page.locator(".live .card .who .agent").evaluateAll((els) =>
    els
      .filter((el) => el.tagName === "DIV")
      .map((el) => {
        const r = el.getBoundingClientRect();
        const cs = getComputedStyle(el);
        return { tag: el.tagName, w: r.width, h: r.height, bg: cs.backgroundColor, radius: cs.borderTopLeftRadius, cls: el.className.replace(/ svelte-\S+/g, "") };
      }),
  );

test("the empty agent box is one look in every stage (Agent Select before a lock = Lobby = Closed)", async ({ page }) => {
  await openScene(page, "pregame");
  const reference = (await emptyAgentBoxes(page))[0]; // an ally who has not locked (R99)
  expect(reference, "pregame has an unpicked ally").toBeDefined();
  near(reference!.w, 40, "reference w");
  near(reference!.h, 40, "reference h");
  expect(reference!.bg).toBe(await varRgb(page, "--raised"));
  expect(reference!.radius).toBe("6px");

  for (const scene of ["lobby1", "lobby3", "lobby5", "closed"]) {
    await openScene(page, scene);
    const cards = page.locator(".live .card");
    const n = await cards.count();
    expect(n, scene).toBeGreaterThan(0);
    // Every card is populated: no skeleton shape, so the box is not read as one.
    await expect(page.locator(".live .card .sk"), `${scene}: no skeleton shape inside a card`).toHaveCount(0);
    await expect(page.locator(".live .card .who img"), `${scene}: no agent image`).toHaveCount(0);
    const boxes = await emptyAgentBoxes(page);
    expect(boxes.length, `${scene}: every card has the empty box`).toBe(n);
    for (const b of boxes) expect(b, scene).toEqual(reference);
  }
});
