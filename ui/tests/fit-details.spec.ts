// Player Details and Match Details fit and per-scene assertions (spec §8.3, §8.5, §6) at exactly 1000×800, DPR 1,
// against the DEV demo feed. Both pages scroll vertically only; everything else is as strict as Live.
import { expect, test, type Locator, type Page } from "@playwright/test";
import { encounterMatchId, PUUID, recentMatchId } from "../src/lib/demo/cdv8";
import { checkClipping, checkHiddenIntegrity, detailsChecks, hiddenRealNames } from "./checks";
import { FONTS, fontSlug, openScene, pointerAway, shotFull, varRgb, waitPage } from "./harness";

const DETAILS_SCENES = [
  "details", "details-more", "details-self", "details-hidden", "details-empty", "details-loading", "details-longnames",
  "details-partial", "details-nostats",
];
const MATCH_SCENES = ["match", "match-enc", "match-hidden", "match-dm", "match-loading", "match-longnames", "match-nostats"];

const MARA = PUUID.mara!;
const W16 = "WWWWWWWWWWWWWWWW";

type Box = { x: number; y: number; width: number; height: number };
const box = async (l: Locator): Promise<Box> => {
  const b = await l.boundingBox();
  if (!b) throw new Error(`no box for ${l}`);
  return b;
};
/** ±0.5px (spec §8.4). */
const near = (actual: number, want: number, what: string) => expect(Math.abs(actual - want), `${what}: ${actual} vs ${want}`).toBeLessThanOrEqual(0.5);
const css = (l: Locator, prop: string) => l.evaluate((el, p) => getComputedStyle(el).getPropertyValue(p), prop);
const text = async (l: Locator) => (await l.innerText()).replace(/\s+/g, " ").trim();
const hit = (a: Box, b: Box) =>
  Math.min(a.x + a.width, b.x + b.width) - Math.max(a.x, b.x) > 0.5 && Math.min(a.y + a.height, b.y + b.height) - Math.max(a.y, b.y) > 0.5;
const scroller = (page: Page) => page.locator("#app .scroll");
/** DEV only: a value read from the app's own module instance (the dev server serves each module at one URL). */
const fromApp = <T>(page: Page, expr: string): Promise<T> => page.evaluate(expr) as Promise<T>;
const route = (page: Page) => fromApp<unknown>(page, `import("/src/lib/router.svelte.ts").then(({ router }) => JSON.parse(JSON.stringify(router.route)))`);

/** A truncated Riot-ID name: cut with an ellipsis, its tag whole and inside the wrapper. */
async function truncatedWithTag(nm: Locator, what: string): Promise<void> {
  const r = await nm.evaluate((el) => {
    const n = el.querySelector<HTMLElement>(".n")!;
    const t = el.querySelector<HTMLElement>(".t")!;
    const w = el.getBoundingClientRect();
    return {
      cut: n.scrollWidth > n.clientWidth,
      ellipsis: getComputedStyle(n).textOverflow,
      tagWhole: t.scrollWidth <= t.clientWidth && t.getBoundingClientRect().right <= w.right + 0.5,
      tag: t.textContent,
    };
  });
  expect(r.cut, `${what}: name truncated`).toBe(true);
  expect(r.ellipsis, `${what}: ellipsis`).toBe("ellipsis");
  expect(r.tagWhole, `${what}: tag ${r.tag} whole`).toBe(true);
}

// ── spec §8.3: checks 1, 3, 4, 5, 7, 8, 9 on every scene, in every font; a full-height shot of each ─────────────────
for (const scene of [...DETAILS_SCENES, ...MATCH_SCENES]) {
  for (const font of FONTS) {
    test(`fit: ${scene} [${font}]`, async ({ page }) => {
      await openScene(page, scene, { font });
      await pointerAway(page);
      expect(await detailsChecks(page, scene, font)).toEqual([]);
      await shotFull(page, `${scene}__${fontSlug(font)}`);
    });
  }
}

// ── spec §8.5, Details ──────────────────────────────────────────────────────────────────────────────────────────
test("details: 3 recent rows + Show more, then 8 and no button; encounter headers; ⟲ 3; the live teammate's icons", async ({ page }) => {
  await openScene(page, "details");
  const rows = page.locator(".details .recent .rrow");
  await expect(rows).toHaveCount(3);
  await expect(page.locator(".details .more")).toHaveText("Show more");
  await page.locator(".details .more").click();
  await expect(rows).toHaveCount(8);
  await expect(page.locator(".details .more")).toHaveCount(0);
  expect(await page.locator(".details .encs .ehead > *").allInnerTexts()).toEqual(["Map", "Mara", "", "You", "Your result", ""]);
  await expect(page.locator(".details .encs .erow")).toHaveCount(3);
  await expect(page.locator(".details .hero .enc")).toHaveText("3");
  const mates = page.locator(".details .mate");
  await expect(mates).toHaveCount(3);
  await expect(mates.locator(".mlive")).toHaveCount(1);
  await expect(mates.filter({ hasText: "Kilo" }).locator(".mlive img")).toHaveCount(2);
  // The number is its own span, 4px from the word (v8:332), as `count` writes it: `14 games`, `1 game`.
  expect(await mates.nth(0).locator(".mcount").evaluate((el) => [el.firstElementChild!.textContent, el.lastChild!.textContent])).toEqual(["14", "games"]);
  // The hero reads the design's values for Mara (v8:480) and R116's K/D and HS%.
  expect(await page.locator(".details .hero .hstat .v").allInnerTexts()).toEqual(["1.52", "29%", "63%", "65", "41 / 24"]);
  expect(await text(page.locator(".details .hero .hlv"))).toBe("Lv 402");
});

test("details: every section in the design's order; recent rows newest first; loadout Vandal, Phantom, Operator, Sheriff", async ({ page }) => {
  await openScene(page, "details-more");
  expect(await page.locator(".details .sech").allInnerTexts()).toEqual(["Recent Competitive Matches", "Encounters", "Top 3 Teammates", "Loadout"]);
  expect(await page.locator(".details .recent .rrow .ago").allInnerTexts()).toEqual(["45m ago", "2h ago", "4h ago", "22h ago", "23h ago", "2d ago", "2d ago", "3d ago"]);
  expect(await page.locator(".details .wname").allInnerTexts()).toEqual(["Vandal", "Phantom", "Operator", "Sheriff"]);
  expect(await page.locator(".details .wskin").allInnerTexts()).toEqual(["Reaver · Red", "Oni · Hana", "Elderflame · Blue", "Kuronami · White"]);
  // Row 0: Lotus, Reyna, Immortal 1, W 13–9, 24 / 14 / 5, 1.71, 31%.
  const r0 = page.locator(".details .recent .rrow").first();
  expect(await text(r0.locator(".mname"))).toBe("Lotus");
  expect(await text(r0.locator(".rscore"))).toBe("W 13–9");
  expect(await r0.locator(".kda > span").allInnerTexts()).toEqual(["24", "/", "14", "/", "5"]);
  expect(await text(r0.locator(".rkd"))).toBe("1.71");
  expect(await text(r0.locator(".rhs"))).toBe("31%");
});

test("details-self: no Encounters section and no ⟲; Top 3 is shown", async ({ page }) => {
  await openScene(page, "details-self");
  await expect(page.locator(".details .encs")).toHaveCount(0);
  await expect(page.locator(".details .hero .enc")).toHaveCount(0);
  await expect(page.locator(".details .mate")).toHaveCount(3);
  await expect(page.locator(".details .hero .nm")).toHaveText(/Fahad\s*#OPD1/);
});

test("details-hidden: Cypher's hero name is the agent name in the hidden style; Rook's is remembered, underlined", async ({ page }) => {
  await openScene(page, "details-hidden");
  const nm = page.locator(".details .hero .nm");
  await expect(nm.locator("svg")).toHaveCount(1);
  await expect(nm.locator(".t")).toHaveCount(0);
  await expect(nm.locator(".hid")).toHaveText("Cypher");
  expect(await css(nm.locator(".hid"), "font-style")).toBe("italic");
  expect(await css(nm.locator(".hid"), "font-size")).toBe("22px");
  await expect(page.locator(".details .encs")).toHaveCount(0); // no shared match with the user

  await openScene(page, "details-hidden", { route: "details", puuid: PUUID.rook! });
  await expect(nm.locator("svg")).toHaveCount(1);
  await expect(nm.locator(".n.rem")).toHaveText("Rook");
  await expect(nm.locator(".t")).toHaveText("#4413");
  expect(await css(nm.locator(".rem"), "text-decoration-style")).toBe("dotted");
  expect(await css(nm.locator(".rem"), "text-underline-offset")).toBe("5px");
  // The encounters header names the player as the hero does (the name, never the tag).
  expect(await page.locator(".details .encs .ehead > *").nth(1).innerText()).toBe("Rook");
});

test("details-empty: one 52px — row, no Show more, no Encounters, Top 3 —, every weapon —, hero stats —", async ({ page }) => {
  await openScene(page, "details-empty");
  const rows = page.locator(".details .recent .rrow");
  await expect(rows).toHaveCount(1);
  near((await box(rows.first())).height, 52, "empty recent row height");
  expect(await text(rows.first())).toBe("—");
  await expect(page.locator(".details .more")).toHaveCount(0);
  await expect(page.locator(".details .encs")).toHaveCount(0);
  await expect(page.locator(".details .mate")).toHaveCount(1);
  expect(await text(page.locator(".details .mate"))).toBe("—");
  near((await box(page.locator(".details .mate"))).height, 46, "empty Top 3 row");
  expect(await page.locator(".details .wskin").allInnerTexts()).toEqual(["—", "—", "—", "—"]);
  await expect(page.locator(".details .well img")).toHaveCount(0);
  expect(await page.locator(".details .hero .hstat .v").allInnerTexts()).toEqual(["—", "—", "—", "—", "— / —"]);
});

test("details-longnames: hero name truncates (tag whole); Top 3 names wrap inside their block, clear of the icons and count; header and skins ellipsize", async ({ page }) => {
  for (const font of FONTS) {
    await openScene(page, "details-longnames", { font });
    await truncatedWithTag(page.locator(".details .hero .nm"), `hero [${font}]`);
    const mates = page.locator(".details .mate");
    await expect(mates).toHaveCount(3);
    for (let i = 0; i < 3; i++) {
      const m = mates.nth(i);
      const name = await box(m.locator(".nm"));
      // Never cut: the name and the tag lie whole inside the block (a line of their own when they must).
      const whole = await m.locator(".nm").evaluate((el) => {
        const b = el.getBoundingClientRect();
        return [...el.querySelectorAll<HTMLElement>(".n, .t")].every((s) => {
          const r = s.getBoundingClientRect();
          return s.scrollWidth <= s.clientWidth && r.left >= b.left - 0.5 && r.right <= b.right + 0.5 && r.bottom <= b.bottom + 0.5;
        });
      });
      expect(whole, `mate ${i} name and tag whole [${font}]`).toBe(true);
      expect(hit(name, await box(m.locator(".mcount"))), `mate ${i} name vs count [${font}]`).toBe(false);
      if ((await m.locator(".mlive").count()) > 0) expect(hit(name, await box(m.locator(".mlive"))), `mate ${i} name vs icons [${font}]`).toBe(false);
    }
    // The live teammate's block is the narrowest (the icons take 54px): its 16-character name has to wrap.
    const live = mates.filter({ has: page.locator(".mlive") });
    await expect(live).toHaveCount(1);
    expect(await live.locator(".nm .n").evaluate((el) => el.getClientRects().length > 1 || el.getBoundingClientRect().height > 20), `live mate wraps [${font}]`).toBe(true);
    const head = page.locator(".details .encs .ehead .ename");
    expect(await head.evaluate((el) => el.scrollWidth > el.clientWidth && getComputedStyle(el).textOverflow === "ellipsis"), `header [${font}]`).toBe(true);
    for (const s of await page.locator(".details .wskin").all()) {
      expect(await s.evaluate((el) => el.scrollWidth > el.clientWidth && getComputedStyle(el).textOverflow === "ellipsis"), `skin line [${font}]`).toBe(true);
    }
  }
});

test("details-partial: absent hero fields are text-free bars; an absent weapon is a skeleton row, a null one —", async ({ page }) => {
  await openScene(page, "details-partial");
  const hero = page.locator(".details .hero");
  const bar = async (l: Locator, w: number, h: number, r: string, what: string) => {
    const b = await box(l);
    near(b.width, w, `${what} w`);
    near(b.height, h, `${what} h`);
    expect(await css(l, "border-radius"), `${what} radius`).toBe(r);
    expect(await css(l, "background-color"), `${what} colour`).toBe(await varRgb(page, "--skel"));
  };
  await bar(hero.locator(".nm .sk"), 160, 18, "3px", "name");
  await bar(hero.locator(".hlv .sk"), 48, 10, "3px", "level");
  await bar(hero.locator(".hrank .sk").nth(0), 48, 48, "50%", "rank icon");
  await bar(hero.locator(".hrank .sk").nth(1), 90, 13, "3px", "rank name");
  await bar(hero.locator(".hpeak .sk").nth(0), 28, 28, "50%", "peak icon");
  await bar(hero.locator(".hpeak .sk").nth(1), 90, 13, "3px", "peak name");
  for (const i of [2, 3, 4]) await bar(hero.locator(".hstat").nth(i).locator(".sk"), 44, 27, "3px", `stat ${i}`);
  await bar(hero.locator(".hl5 .sk"), 154, 26, "4px", "last 5");
  for (const sel of [".nm", ".hlv", ".hrank", ".hpeak", ".hl5"]) expect(await text(hero.locator(sel)), sel).toBe("");
  // K/D and HS% come from the details (R116), so they are known; the three act values are bars under their labels.
  expect(await hero.locator(".hstat .k").allInnerTexts()).toEqual(["K/D", "HS%", "Win%", "Games", "W / L"]);
  expect(await hero.locator(".hstat .v").allInnerTexts()).toEqual(["1.52", "29%"]);

  const rows = page.locator(".details .wrow");
  await expect(rows).toHaveCount(4);
  // Sheriff: absent (its name still resolving, R97): a --skel well and a 90×13 bar, no text at all.
  const sheriff = rows.nth(3);
  await bar(sheriff.locator(".sk").nth(0), 124, 48, "5px", "loading well");
  await bar(sheriff.locator(".sk").nth(1), 90, 13, "3px", "loading line");
  expect(await text(sheriff)).toBe("");
  // Operator: null (none): an empty raised well, the weapon name and —.
  const op = rows.nth(2);
  expect(await text(op.locator(".wname"))).toBe("Operator");
  expect(await text(op.locator(".wskin"))).toBe("—");
  await expect(op.locator(".well img")).toHaveCount(0);
  expect(await css(op.locator(".well"), "background-color")).toBe(await varRgb(page, "--raised"));
  expect(await css(op.locator(".well"), "box-shadow")).toBe("none");
  await expect(rows.nth(0).locator(".well img")).toHaveCount(2); // the content-tier icon and the skin
});

test("details-loading: the §6.9 skeleton, section labels only, no other text", async ({ page }) => {
  await openScene(page, "details-loading");
  const d = page.locator(".details");
  await expect(d).toHaveAttribute("aria-busy", "true");
  expect(await d.locator(".sech").allInnerTexts()).toEqual(["Recent Competitive Matches", "Top 3 Teammates", "Loadout"]);
  const words = (await d.innerText()).split(/\s+/).filter(Boolean).join(" ");
  expect(words).toBe("Back Recent Competitive Matches Map Score K / D / A K/D HS% Top 3 Teammates Loadout");
  const agent = d.locator(".hero .hagent");
  near((await box(agent)).width, 56, "agent box");
  expect(await css(agent, "border-radius")).toBe("7px");
  expect(await css(agent, "background-color")).toBe(await varRgb(page, "--skel"));
  await expect(d.locator(".hero .hstat .sk")).toHaveCount(5);
  await expect(d.locator(".recent .rrow")).toHaveCount(3);
  await expect(d.locator(".mate")).toHaveCount(3);
  await expect(d.locator(".wrow")).toHaveCount(4);
  await expect(d.locator("img")).toHaveCount(0);
});

// ── R133: every — cell ──────────────────────────────────────────────────────────────────────────────────────────
test("details-nostats: every unknown value is —, and every unknown image an empty box (R102, R124, R133)", async ({ page }) => {
  await openScene(page, "details-nostats");
  const hero = page.locator(".details .hero");
  expect(await text(hero.locator(".hlv"))).toBe("Lv —");
  expect(await text(hero.locator(".hrank"))).toBe("—");
  await expect(hero.locator(".hrank img")).toHaveCount(0);
  expect(await text(hero.locator(".hpeak"))).toBe("Peak —");
  expect(await text(hero.locator(".hprev"))).toBe("Prev —");
  await expect(hero.locator(".hrank img, .hpeak img, .hprev img")).toHaveCount(0);
  expect(await hero.locator(".hstat .v").allInnerTexts()).toEqual(["—", "—", "—", "—", "— / —"]);
  const wl = hero.locator(".hstat").nth(4).locator(".v > span");
  expect(await wl.nth(0).evaluate((el) => getComputedStyle(el).color)).toBe(await varRgb(page, "--win"));
  expect(await wl.nth(2).evaluate((el) => getComputedStyle(el).color)).toBe(await varRgb(page, "--loss"));
  await expect(hero.locator(".hl5 .tile")).toHaveCount(5);
  expect(await hero.locator(".hl5 .tile").allInnerTexts()).toEqual(["", "", "", "", ""]);

  const rows = page.locator(".details .recent .rrow");
  await expect(rows).toHaveCount(3);
  await expect(page.locator(".details .more")).toHaveCount(1);
  for (let i = 0; i < 3; i++) {
    const r = rows.nth(i);
    expect(await r.locator(".kda > span").allInnerTexts(), `row ${i} K / D / A`).toEqual(["—", "/", "—", "/", "—"]);
    expect(await text(r.locator(".rkd")), `row ${i} K/D`).toBe("—");
    expect(await text(r.locator(".rhs")), `row ${i} HS%`).toBe("—");
  }
  // Row 1: no map, score, agent or rank either. Row 2: unranked in that match (tier 0): the empty box, like null.
  const r1 = rows.nth(1);
  expect(await text(r1.locator(".mname"))).toBe("—");
  expect(await text(r1.locator(".rscore"))).toBe("D —");
  await expect(r1.locator("img")).toHaveCount(0);
  await expect(r1.locator(".rmap .mbox, .ragent, .rrank")).toHaveCount(3);
  await expect(rows.nth(2).locator(".rrank")).toHaveCount(1);
  await expect(rows.nth(2).locator("img.rrank")).toHaveCount(0);

  const enc = page.locator(".details .encs .erow");
  await expect(enc).toHaveCount(2);
  expect(await text(enc.nth(0).locator(".emap .mname"))).toBe("—");
  expect(await enc.nth(0).locator(".ewho .tname").allInnerTexts()).toEqual(["—", "—"]);
  expect(await text(enc.nth(0).locator(".eres"))).toBe("D —");
  await expect(enc.nth(0).locator("img")).toHaveCount(0);
  // Row 1: tier 0 (unranked) and null tiers read — beside the agents' icons.
  expect(await enc.nth(1).locator(".ewho .tname").allInnerTexts()).toEqual(["—", "—"]);
  await expect(enc.nth(1).locator("img")).toHaveCount(3); // the map and the two agents

  expect(await text(page.locator(".details .mate"))).toBe("—");
  expect(await page.locator(".details .wskin").allInnerTexts()).toEqual(["—", "—", "—", "—"]);
});

// ── spec §8.5, Match ────────────────────────────────────────────────────────────────────────────────────────────
async function acsByTeam(page: Page): Promise<number[][]> {
  return page.locator(".match .board").evaluate((b) => {
    const out: number[][] = [];
    for (const el of b.children) {
      if (el.classList.contains("thead")) out.push([]);
      else if (el.classList.contains("srow")) out[out.length - 1]!.push(Number(el.querySelector(".acs")!.textContent) || -1);
    }
    return out;
  });
}

test("match: sorted by ACS; `Mara's team` / `Opponents`; one 3px accent focus bar; halves 7–4 · 6–5; the header", async ({ page }) => {
  await openScene(page, "match");
  const teams = await acsByTeam(page);
  expect(teams.map((t) => t.length)).toEqual([5, 5]);
  for (const t of teams) expect([...t].sort((a, b) => b - a)).toEqual(t);
  expect(await page.locator(".match .thead .tlabel").allInnerTexts()).toEqual(["Mara's team", "Opponents"]);
  const bars = page.locator(".match .srow .focusbar");
  await expect(bars).toHaveCount(1);
  near((await box(bars)).width, 3, "focus bar width");
  expect(await css(bars, "background-color")).toBe(await varRgb(page, "--accent"));
  await expect(page.locator(".match .srow .selfframe")).toHaveCount(0);
  await expect(page.locator(".match .mhalves")).toHaveText("7–4 · 6–5");
  await expect(page.locator(".match .mscore")).toHaveText("13–9");
  await expect(page.locator(".match .mtitle")).toHaveText("Lotus");
  await expect(page.locator(".match .mwhen")).toHaveText("Competitive · Sep 29, 2026 · 9:14 PM · 38 min");
  await expect(page.locator(".match .mhead .chip")).toHaveText("W");
  expect(await page.locator(".match .thead .trounds").allInnerTexts()).toEqual(["13", "9"]);
  expect(await page.locator(".match .shead > *").allInnerTexts()).toEqual(["Player", "ACS", "K / D / A", "+/−", "ADR", "HS%"]);
  // Mara's row: 24 / 14 / 5, +10 in --win.
  const mara = page.locator(".match .srow").filter({ has: page.locator(".focusbar") });
  expect(await mara.locator(".kda > span").allInnerTexts()).toEqual(["24", "/", "14", "/", "5"]);
  await expect(mara.locator(".pm")).toHaveText("+10");
  expect(await css(mara.locator(".pm"), "color")).toBe(await varRgb(page, "--win"));
});

test("match-enc: `Your team`; one self frame and one focus bar, in different rows", async ({ page }) => {
  await openScene(page, "match-enc");
  expect(await page.locator(".match .thead .tlabel").allInnerTexts()).toEqual(["Your team", "Opponents"]);
  await expect(page.locator(".match .srow .selfframe")).toHaveCount(1);
  await expect(page.locator(".match .srow .focusbar")).toHaveCount(1);
  await expect(page.locator(".match .srow").filter({ has: page.locator(".selfframe") }).filter({ has: page.locator(".focusbar") })).toHaveCount(0);
  await expect(page.locator(".match .mwhen")).toHaveText("Competitive · Sep 12, 2026 · 11:18 PM · 41 min");
  expect(await css(page.locator(".match .selfframe"), "border-top-color")).toBe(await varRgb(page, "--self-frame"));
});

// The user's own match (from their own Details): the user's row is both the focus and self; it gets the frame only.
test("the user's own match: `Your team`, one self frame, no focus bar", async ({ page }) => {
  await openScene(page, "details-self");
  await page.locator(".details .recent .rrow").first().click();
  await waitPage(page, "loaded");
  expect(await page.locator(".match .thead .tlabel").allInnerTexts()).toEqual(["Your team", "Opponents"]);
  await expect(page.locator(".match .srow .selfframe")).toHaveCount(1);
  await expect(page.locator(".match .srow .focusbar")).toHaveCount(0);
  await expect(page.locator(".match .srow").filter({ has: page.locator(".selfframe") }).locator(".nm")).toHaveText(/Fahad\s*#OPD1/);
});

test("match-hidden: a hidden-agent row (italic agent name, eye, no tag) and a remembered row (underlined, tag)", async ({ page }) => {
  await openScene(page, "match-hidden");
  const hidden = page.locator(".match .srow .nm").filter({ has: page.locator(".hid") });
  await expect(hidden).toHaveCount(1);
  await expect(hidden.locator(".hid")).toHaveText("Cypher");
  await expect(hidden.locator("svg")).toHaveCount(1);
  await expect(hidden.locator(".t")).toHaveCount(0);
  const rem = page.locator(".match .srow .nm").filter({ has: page.locator(".rem") });
  await expect(rem).toHaveCount(1);
  await expect(rem.locator(".rem")).toHaveText("Drift");
  await expect(rem.locator(".t")).toHaveText("#9981");
  expect(await css(rem.locator(".rem"), "text-underline-offset")).toBe("4px");
});

test("match-dm: one team block labelled Players, no chips, header score — and no halves; no rank, ADR —", async ({ page }) => {
  await openScene(page, "match-dm");
  await expect(page.locator(".match .thead")).toHaveCount(1);
  await expect(page.locator(".match .thead .tlabel")).toHaveText("Players");
  await expect(page.locator(".match .thead .trounds")).toHaveText("—");
  await expect(page.locator(".match .chip")).toHaveCount(0);
  await expect(page.locator(".match .mscore")).toHaveText("—");
  await expect(page.locator(".match .mhalves")).toHaveCount(0);
  await expect(page.locator(".match .mwhen")).toHaveText("Deathmatch · Sep 29, 2026 · 8:40 PM · 9 min");
  await expect(page.locator(".match .srow")).toHaveCount(10);
  await expect(page.locator(".match .srow img.srank")).toHaveCount(0);
  expect(new Set(await page.locator(".match .srow .adr").allInnerTexts())).toEqual(new Set(["—"]));
});

// Spec §8.5 says every scoreboard name "truncates": at the design's column widths (v8:381) the name box is ~490px, and
// the longest Riot ID (16 + 5 W's, ~200px + tag) fits whole in every font. So each name is asserted whole, tag included,
// on one 42px line; a longer name would take the ellipsis (check 5 holds any truncated span to one).
test("match-longnames: every 16-character name shows whole with its tag on one line; no row is taller than 42px; the long team label fits", async ({ page }) => {
  for (const font of FONTS) {
    await openScene(page, "match-longnames", { font });
    const named = page.locator(".match .srow .nm").filter({ has: page.locator(".t") });
    expect(await named.count()).toBeGreaterThanOrEqual(9);
    for (const nm of await named.all()) {
      const r = await nm.evaluate((el) => {
        const n = el.querySelector<HTMLElement>(".n")!;
        const t = el.querySelector<HTMLElement>(".t")!;
        return { name: n.textContent, cut: n.scrollWidth > n.clientWidth, tagCut: t.scrollWidth > t.clientWidth, sameLine: Math.abs(n.getBoundingClientRect().bottom - t.getBoundingClientRect().bottom) < 4 };
      });
      expect(r, `row [${font}]`).toEqual({ name: W16, cut: false, tagCut: false, sameLine: true });
    }
    for (const r of await page.locator(".match .srow").all()) expect(await r.evaluate((el) => el.clientHeight), `row height [${font}]`).toBe(42);
    await expect(page.locator(".match .thead .tlabel").first()).toHaveText(`${W16}'s team`);
  }
});

test("match-loading: the §6.10 skeleton, the scoreboard header only", async ({ page }) => {
  await openScene(page, "match-loading");
  const m = page.locator(".match");
  await expect(m).toHaveAttribute("aria-busy", "true");
  const words = (await m.innerText()).split(/\s+/).filter(Boolean).join(" ");
  expect(words).toBe("Back Player ACS K / D / A +/− ADR HS%");
  const map = m.locator(".mhead .sk").first();
  near((await box(map)).width, 150, "map box w");
  near((await box(map)).height, 56, "map box h");
  expect(await css(map, "border-radius")).toBe("5px");
  await expect(m.locator(".thead")).toHaveCount(2);
  await expect(m.locator(".srow")).toHaveCount(10);
  await expect(m.locator("img")).toHaveCount(0);
});

test("match-nostats: no duration, rounds or halves; no-stats rows are — in every stat and keep their rank (R81, R124, R133)", async ({ page }) => {
  await openScene(page, "match-nostats");
  await expect(page.locator(".match .mwhen")).toHaveText("Competitive · Sep 29, 2026 · 9:14 PM");
  await expect(page.locator(".match .mscore")).toHaveText("—");
  await expect(page.locator(".match .mhalves")).toHaveCount(0);
  await expect(page.locator(".match .mhead .chip")).toHaveText("L"); // Zephyr lost it: the result is known, the score is not
  expect(await page.locator(".match .thead .trounds").allInnerTexts()).toEqual(["—", "—"]);
  const bare = page.locator(".match .srow").filter({ has: page.locator(".acs", { hasText: "—" }) });
  expect(await bare.count()).toBeGreaterThanOrEqual(4);
  const flat = await varRgb(page, "--text2");
  for (const r of await bare.all()) {
    expect(await r.locator(".kda > span").allInnerTexts()).toEqual(["—", "/", "—", "/", "—"]);
    await expect(r.locator(".pm")).toHaveText("—");
    expect(await css(r.locator(".pm"), "color")).toBe(flat);
    await expect(r.locator(".adr")).toHaveText("—");
    await expect(r.locator(".hs")).toHaveText("—");
  }
  // The focus row has no stats row but its rank still shows (R81); another has neither a rank nor an agent.
  const focus = page.locator(".match .srow").filter({ has: page.locator(".focusbar") });
  await expect(focus.locator(".acs")).toHaveText("—");
  await expect(focus.locator("img.srank")).toHaveCount(1);
  expect(await bare.filter({ hasNot: page.locator("img") }).count()).toBe(1);
  // One row lost only its damage: ADR — beside real numbers.
  const adrOnly = page.locator(".match .srow").filter({ has: page.locator(".adr", { hasText: "—" }) }).filter({ hasNot: page.locator(".acs", { hasText: "—" }) });
  await expect(adrOnly).toHaveCount(1);
  expect(await adrOnly.locator(".acs").innerText()).toMatch(/^\d+$/);
});

// ── navigation (spec §2.4, §6.9) ────────────────────────────────────────────────────────────────────────────────
test("details event updates in place keeping more and scroll", async ({ page }) => {
  await openScene(page, "details-more");
  const sc = scroller(page);
  await sc.evaluate((el) => (el.scrollTop = 150));
  expect(await sc.evaluate((el) => el.scrollTop)).toBe(150);
  const rowBefore = await page.locator(".details .recent .rrow").first().elementHandle();
  // A `details` event for the open player (Rust emits one after its sync), through the app's own reducer.
  await fromApp(
    page,
    `Promise.all([import("/src/lib/stores.ts"), import("/src/lib/boot.ts")]).then(([s, b]) => {
      let open; s.openDetails.subscribe((v) => (open = v))();
      s.openDetails.update((o) => b.applyDetailsEvent(o, { ...structuredClone(open), heroKd: 1.99, loadout: { ...open.loadout, vandal: null } }));
    })`,
  );
  await expect(page.locator(".details .hero .hstat .v").first()).toHaveText("1.99");
  await expect(page.locator(".details .wskin").first()).toHaveText("—");
  await expect(page.locator(".details .recent .rrow")).toHaveCount(8);
  expect(await sc.evaluate((el) => el.scrollTop)).toBe(150);
  // Keyed rows: the same DOM node, updated in place.
  expect(await rowBefore!.evaluate((el) => el.isConnected)).toBe(true);
  // An event for another player leaves the page alone.
  await fromApp(
    page,
    `Promise.all([import("/src/lib/stores.ts"), import("/src/lib/boot.ts")]).then(([s, b]) => {
      let open; s.openDetails.subscribe((v) => (open = v))();
      const other = structuredClone(open); other.card.puuid = "a0000000-0000-4000-8000-000000000002"; other.heroKd = 7.77;
      s.openDetails.update((o) => b.applyDetailsEvent(o, other));
    })`,
  );
  await expect(page.locator(".details .hero .hstat .v").first()).toHaveText("1.99");
});

test("Back paths: match → details (more kept) → live", async ({ page }) => {
  await openScene(page, "details");
  await page.locator(".details .more").click();
  await page.locator(".details .recent .rrow").nth(5).click();
  await waitPage(page, "loaded");
  expect(await route(page)).toEqual({
    name: "match",
    matchId: recentMatchId("mara", 5),
    focusPuuid: MARA,
    from: "recent",
    back: { puuid: MARA, more: true },
  });
  await expect(page.locator(".match .mtitle")).toHaveText("Bind");
  await page.locator(".match .back").click();
  await waitPage(page, "loaded");
  await expect(page.locator(".details .recent .rrow")).toHaveCount(8);
  expect(await route(page)).toEqual({ name: "details", puuid: MARA, more: true });

  // An encounter row opens its match from the encounter side; Back returns to the same Details.
  await page.locator(".details .encs .erow").first().click();
  await waitPage(page, "loaded");
  expect(await route(page)).toMatchObject({ name: "match", matchId: encounterMatchId("mara", 0), from: "encounter", back: { puuid: MARA, more: true } });
  await expect(page.locator(".match .thead .tlabel").first()).toHaveText("Your team");
  await page.locator(".match .back").click();
  await waitPage(page, "loaded");
  await page.locator(".details .back").click();
  await expect(page.locator(".live")).toHaveCount(1);
  await expect(page.locator("#app .scroll")).toHaveCount(0);
  expect(await route(page)).toEqual({ name: "live" });
});

test("scroll container resets on navigation", async ({ page }) => {
  await openScene(page, "details-more");
  const sc = scroller(page);
  await sc.evaluate((el) => (el.scrollTop = el.scrollHeight));
  expect(await sc.evaluate((el) => el.scrollTop)).toBeGreaterThan(200);
  await page.locator(".details .encs .erow").last().click();
  await waitPage(page, "loaded");
  expect(await sc.evaluate((el) => el.scrollTop)).toBe(0);
  await page.locator(".match .back").click();
  await waitPage(page, "loaded");
  expect(await sc.evaluate((el) => el.scrollTop)).toBe(0);
  // Show more keeps the place (it is not a navigation). Mara's collapsed page fits the window, so it is given room to
  // scroll for this check.
  await openScene(page, "details");
  await page.addStyleTag({ content: ".details{padding-bottom:400px}" });
  await sc.evaluate((el) => (el.scrollTop = 120));
  expect(await sc.evaluate((el) => el.scrollTop)).toBe(120);
  await page.locator(".details .more").click();
  await expect(page.locator(".details .recent .rrow")).toHaveCount(8);
  expect(await sc.evaluate((el) => el.scrollTop)).toBe(120);
  // The Live tab from a scrolled page, then a card: Details opens at the top.
  await sc.evaluate((el) => (el.scrollTop = 200));
  await page.locator("header.bar .tab.on").click();
  await page.locator(".live .card").filter({ hasText: "Sable" }).click({ position: { x: 20, y: 70 } });
  await waitPage(page, "loaded");
  expect(await scroller(page).evaluate((el) => el.scrollTop)).toBe(0);
  await expect(page.locator(".details .hero .nm")).toHaveText(/Sable\s*#1177/);
});

// ── the checks themselves ───────────────────────────────────────────────────────────────────────────────────────
// Task 17 re-review: check 5's vertical pass counted `overflow-y:auto` as a clip, so every text below the fold of a
// Details page read as cut by `.scroll` (and then by #app). A scroll container is not a clip: its content is reached by
// scrolling. A real cut inside it must still be reported, and a scroll box that stops scrolling (hidden) clips again.
test("check 5: the scroll container is not a clip; a cut inside it, or a non-scrolling box, still is", async ({ page }) => {
  await openScene(page, "details-more");
  await pointerAway(page);
  // Texts wholly below the fold at scrollTop 0 (the encounters): an `auto` clip would report every one of them.
  const below = await page.evaluate(() => {
    const sc = document.querySelector("#app .scroll")!.getBoundingClientRect();
    const walker = document.createTreeWalker(document.querySelector("#app .scroll")!, NodeFilter.SHOW_TEXT);
    const out: string[] = [];
    for (let n = walker.nextNode(); n; n = walker.nextNode()) {
      const r = document.createRange();
      r.selectNodeContents(n);
      if (n.textContent?.trim() && r.getBoundingClientRect().top >= sc.bottom) out.push(n.textContent.trim());
    }
    return out;
  });
  expect(below.length, "texts below the fold").toBeGreaterThan(5);
  expect(await checkClipping(page)).toEqual([]);

  const cut = await page.addStyleTag({ content: ".details .recent .rrow .rkd{display:block;height:8px!important;overflow:hidden}" });
  const inside = await checkClipping(page);
  expect(inside.length).toBeGreaterThan(0);
  for (const p of inside) expect(p).toMatch(/is cut vertically by span\.rkd$/);
  await cut.evaluate((el) => (el as Element).remove());

  await page.addStyleTag({ content: "#app .scroll{overflow-y:hidden!important}" });
  const clipped = await checkClipping(page);
  expect(clipped.filter((p) => /is cut vertically by div\.scroll$/.test(p)).length, clipped.join("\n")).toBeGreaterThanOrEqual(below.length);
});

// Check 5's decoration probe screenshots each decorated text; on a scrolling page it first brings one below the fold
// into view (a screenshot clip cannot reach past the viewport, and would throw).
test("check 5 probes a text decoration below the fold", async ({ page }) => {
  await openScene(page, "details-more");
  await pointerAway(page);
  const last = ".details .encs .erow:last-child .mname";
  expect(await page.locator(last).evaluate((el) => el.getBoundingClientRect().top > window.innerHeight)).toBe(true);
  await page.addStyleTag({ content: `${last}{text-decoration:underline dotted var(--text2);text-underline-offset:4px}` });
  expect(await checkClipping(page)).toEqual([]);
  // It was probed where it is: scrolled into view.
  expect(await page.locator(last).evaluate((el) => el.getBoundingClientRect().bottom <= window.innerHeight)).toBe(true);
});

// R93/R96: check 9 scans Details and Match. Nova is a hidden player in Agent Select; her Details and her own match show
// her in the hidden style, and her real name (a fixture string) appears nowhere in either page, attributes included.
test("check 9: a player hidden on the live screen leaves no trace of the real name on Details or Match (pregame Nova)", async ({ page }) => {
  const real = hiddenRealNames("pregame");
  expect(real).toEqual(["Nova", "3341"]);
  await openScene(page, "pregame", { route: "details", puuid: PUUID.nova! });
  await expect(page.locator(".details .hero .nm .hid")).toHaveText("Hidden Player");
  expect(await checkHiddenIntegrity(page, real)).toEqual([]);
  await page.locator(".details .hero").evaluate((el) => el.setAttribute("data-leak", "Nova#3341"));
  expect(await checkHiddenIntegrity(page, real)).toEqual([
    'a hidden hero in Details contains "Nova"',
    'a hidden hero in Details contains "3341"',
    'the Details page contains "Nova"',
    'the Details page contains "3341"',
  ]);
  await page.locator(".details .hero").evaluate((el) => el.removeAttribute("data-leak"));

  await page.locator(".details .recent .rrow").first().click();
  await waitPage(page, "loaded");
  const own = page.locator(".match .srow").filter({ has: page.locator(".focusbar") });
  await expect(own.locator(".nm .hid")).toHaveText("Hidden Player");
  await expect(page.locator(".match .thead .tlabel").first()).toHaveText("Hidden Player's team");
  expect(await checkHiddenIntegrity(page, real)).toEqual([]);
  await own.evaluate((el) => el.setAttribute("title", "Nova"));
  expect(await checkHiddenIntegrity(page, real)).toEqual(['a hidden srow in Match contains "Nova"', 'the Match page contains "Nova"']);
});
