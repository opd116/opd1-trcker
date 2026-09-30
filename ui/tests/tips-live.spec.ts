// Every card and top-bar tooltip of spec §4.2, hovered in turn (spec §8.5 `tips`), plus the engine's timing and
// placement rules (spec §4.1). Playwright's fake clock is installed before load and paused once the page has
// settled, so the 450ms delay is stepped with clock.runFor and never depends on real timers.
import { expect, test, type Locator, type Page } from "@playwright/test";
import { openScene, pointerAway, varRgb } from "./harness";

const card = (page: Page, text: string | RegExp) => page.locator(".live .card").filter({ hasText: text });
const tip = (page: Page) => page.locator("#app [role=tooltip]");

type Item = { what: string; trigger: (p: Page) => Locator; text: string | null };

/** Hover the trigger, step the clock past 450ms and read the tip (null = no tip). Then check its placement. */
async function hoverTip(page: Page, trigger: Locator): Promise<string | null> {
  await trigger.hover();
  await page.clock.runFor(460);
  if ((await tip(page).count()) === 0) return null;
  const t = tip(page);
  const [tr, rr, app] = await Promise.all([trigger.boundingBox(), t.boundingBox(), page.locator("#app").boundingBox()]);
  if (!tr || !rr || !app) throw new Error("no box");
  // Inside #app.
  expect(rr.x).toBeGreaterThanOrEqual(app.x);
  expect(rr.y).toBeGreaterThanOrEqual(app.y);
  expect(rr.x + rr.width).toBeLessThanOrEqual(app.x + app.width);
  expect(rr.y + rr.height).toBeLessThanOrEqual(app.y + app.height);
  // Centred on the trigger, clamped to 136–864.
  const want = Math.max(136, Math.min(864, tr.x + tr.width / 2 - app.x));
  expect(Math.abs(rr.x + rr.width / 2 - app.x - want)).toBeLessThanOrEqual(1);
  // Below the trigger, or above it when below would pass 720.
  if (tr.y + tr.height - app.y + 8 > 720) expect(Math.abs(rr.y + rr.height - (tr.y - app.y - 8))).toBeLessThanOrEqual(1);
  else expect(Math.abs(rr.y - (tr.y + tr.height - app.y + 8))).toBeLessThanOrEqual(1);
  return t.evaluate((el) => {
    const wl = el.querySelector(".wl");
    if (!wl) return el.textContent;
    const row = [...wl.children].map((s) => s.textContent).join(" ");
    return `${row}\n${el.querySelector(".scope")?.textContent ?? ""}`;
  });
}

async function runItems(page: Page, items: Item[]): Promise<void> {
  for (const it of items) {
    const trigger = it.trigger(page);
    await expect(trigger, it.what).toHaveCount(1);
    expect(await hoverTip(page, trigger), it.what).toBe(it.text);
    // Moving off a trigger clears the tip immediately (no clock step).
    await pointerAway(page);
    await expect(tip(page), `${it.what}: cleared`).toHaveCount(0);
  }
}

const A4 = "VALORANT 2026 · Act 4\nJun 24 – Aug 25, 2026";
const stat = (p: Page, who: string, i: number) => card(p, who).locator(".stat").nth(i);
const tile = (p: Page, who: string, i: number) => card(p, who).locator(".tile").nth(i);

const INGAME: Item[] = [
  { what: "visible name, truncated", trigger: (p) => card(p, "VesperUnbreakabl").locator(".nm"), text: "VesperUnbreakabl#EUW7" },
  { what: "visible name, not truncated", trigger: (p) => card(p, "Fahad").locator(".nm"), text: null },
  { what: "remembered name", trigger: (p) => card(p, "Rook").locator(".nm"), text: "Hidden name · known from your match\nSplit · Aug 30, 2026" },
  { what: "hidden agent name", trigger: (p) => card(p, "Cypher").locator(".nm"), text: "Hidden name" },
  { what: "⟲ Vesper", trigger: (p) => card(p, "VesperUnbreakabl").locator(".enc"), text: "1 with · 0 against" },
  { what: "⟲ Mara", trigger: (p) => card(p, "Mara").locator(".enc"), text: "1 with · 2 against" },
  { what: "⟲ Rook", trigger: (p) => card(p, "Rook").locator(".enc"), text: "0 with · 1 against" },
  { what: "party stripe, confirmed", trigger: (p) => card(p, "Drake").locator(".stripe"), text: "Party · confirmed" },
  { what: "party tab, confirmed", trigger: (p) => card(p, "Drake").locator(".ptab"), text: "Party · confirmed" },
  { what: "party stripe, likely", trigger: (p) => card(p, "Mara").locator(".stripe"), text: "Party · likely" },
  { what: "party tab, likely", trigger: (p) => card(p, "Kilo").locator(".ptab"), text: "Party · likely" },
  { what: "peak, past act (V)", trigger: (p) => card(p, "Fahad").locator(".pgroup"), text: A4 },
  { what: "peak, past act across a new year", trigger: (p) => card(p, "Sable").locator(".pgroup"), text: "VALORANT 2025 · Act 6\nNov 12, 2025 – Jan 6, 2026" },
  { what: "peak, episode act", trigger: (p) => card(p, "Rook").locator(".pgroup"), text: "Episode 9 · Act 3\nOct 22, 2024 – Jan 8, 2025" },
  { what: "peak, current act (bottom row)", trigger: (p) => card(p, "Nova").locator(".pgroup"), text: "V26 A5 · Current act\nAug 26 – Oct 20, 2026" },
  { what: "K/D", trigger: (p) => stat(p, "Fahad", 0), text: "Last 5 competitive" },
  { what: "HS%", trigger: (p) => stat(p, "Fahad", 1), text: "Last 5 competitive" },
  { what: "Win%", trigger: (p) => stat(p, "Fahad", 2), text: "31W · 22L · 53 games\nThis act" },
  { what: "Win%, bottom row flips above", trigger: (p) => stat(p, "Cypher", 2), text: "2W · 1L · 3 games\nThis act" },
  { what: "last-5 tile 1 (oldest)", trigger: (p) => tile(p, "Fahad", 0), text: "−21 RR · 23h" },
  { what: "last-5 tile 2", trigger: (p) => tile(p, "Fahad", 1), text: "+17 RR · 22h" },
  { what: "last-5 tile 3", trigger: (p) => tile(p, "Fahad", 2), text: "+21 RR · 4h" },
  { what: "last-5 tile 4", trigger: (p) => tile(p, "Fahad", 3), text: "−19 RR · 2h" },
  { what: "last-5 tile 5 (newest)", trigger: (p) => tile(p, "Fahad", 4), text: "+18 RR · 45m" },
  { what: "last-5 draw", trigger: (p) => tile(p, "VesperUnbreakabl", 1), text: "0 RR · 22h" },
  { what: "last-5 empty tile", trigger: (p) => tile(p, "Cypher", 0), text: null },
  { what: "last-5 bottom row flips above", trigger: (p) => tile(p, "Nova", 4), text: "+18 RR · 45m" },
  { what: "current rank", trigger: (p) => card(p, "Fahad").locator(".rank"), text: null },
  { what: "prev", trigger: (p) => card(p, "Fahad").locator(".prev"), text: null },
  { what: "level", trigger: (p) => card(p, "Fahad").locator(".lv"), text: null },
  { what: "map group", trigger: (p) => p.locator("header.bar .map"), text: null },
];

test("tips: ingame", async ({ page }) => {
  await openScene(page, "ingame", {}, { clock: "manual" });
  await runItems(page, INGAME);
});

test("tips: ingame-longnames", async ({ page }) => {
  await openScene(page, "ingame-longnames", {}, { clock: "manual" });
  const W = "WWWWWWWWWWWWWWWW";
  await runItems(page, [
    { what: "visible name, truncated (self)", trigger: (p) => p.locator(".live .card").first().locator(".nm"), text: `${W}#WWWWW` },
    { what: "visible name, truncated (enemy)", trigger: (p) => p.locator(".live .col").nth(1).locator(".card").first().locator(".nm"), text: `${W}#WWWWW` },
    // A cut remembered name: its own tip, then the full Name#TAG under it (v0.2.0 polish). Rook, not cut, has the plain tip (ingame).
    { what: "remembered long name (cut)", trigger: (p) => p.locator(".live .card").filter({ has: p.locator(".nm svg") }).filter({ has: p.locator(".nm .t") }).locator(".nm"), text: `Hidden name · known from your match\nSplit · Aug 30, 2026\n${W}#WWWWW` },
    { what: "3-digit ⟲", trigger: (p) => p.locator(".live .enc", { hasText: "123" }), text: "70 with · 53 against" },
    { what: "peak V26 A4", trigger: (p) => p.locator(".live .card").first().locator(".pgroup"), text: A4 },
    { what: "peak E10 A3", trigger: (p) => p.locator(".live .card").filter({ has: p.locator(".pgroup", { hasText: "E10 A3" }) }).locator(".pgroup"), text: "Episode 10 · Act 3\nMar 5 – Apr 30, 2025" },
  ]);
});

const card0 = (p: Page) => p.locator(".live .card").first();

test("tips: lobby3", async ({ page }) => {
  await openScene(page, "lobby3", {}, { clock: "manual" });
  await runItems(page, [
    { what: "average party rank", trigger: (p) => p.locator("header.bar .apr"), text: "Average party rank\nFrom 2 ranked players" },
    { what: "self Win%", trigger: (p) => card0(p).locator(".stat").nth(2), text: "31W · 22L · 53 games\nThis act" },
    { what: "self peak", trigger: (p) => card0(p).locator(".pgroup"), text: A4 },
    { what: "unranked member's K/D", trigger: (p) => p.locator(".live .card").nth(2).locator(".stat").nth(0), text: "Last 5 competitive" },
  ]);
});

test("tips: pregame", async ({ page }) => {
  await openScene(page, "pregame", {}, { clock: "manual" });
  await runItems(page, [
    { what: "Hidden Player", trigger: (p) => card(p, "Hidden Player").locator(".nm"), text: "Hidden name" },
    { what: "hovering ally (no agent) peak", trigger: (p) => card(p, "Drake").locator(".pgroup"), text: "VALORANT 2026 · Act 2\nMar 4 – Apr 28, 2026" },
    { what: "hidden player's Win%", trigger: (p) => card(p, "Hidden Player").locator(".stat").nth(2), text: "25W · 16L · 41 games\nThis act" },
  ]);
});

test("lobby2: the APR tip is `Average party rank\\nFrom 1 ranked player`", async ({ page }) => {
  await openScene(page, "lobby2", {}, { clock: "manual" });
  expect(await hoverTip(page, page.locator("header.bar .apr"))).toBe("Average party rank\nFrom 1 ranked player");
});

test("the name tip appears at ≥ 450ms and not at 400ms", async ({ page }) => {
  await openScene(page, "ingame-longnames", {}, { clock: "manual" });
  await page.locator(".live .card").first().locator(".nm").hover();
  await page.clock.runFor(400);
  await expect(tip(page)).toHaveCount(0);
  await page.clock.runFor(50);
  await expect(tip(page)).toHaveCount(1);
  await expect(tip(page)).toHaveText("WWWWWWWWWWWWWWWW#WWWWW");
});

test("the Win% tip shows W · L · games / This act in --win / --loss", async ({ page }) => {
  await openScene(page, "ingame", {}, { clock: "manual" });
  await stat(page, "Fahad", 2).hover();
  await page.clock.runFor(460);
  const t = tip(page);
  await expect(t.locator(".wl > span")).toHaveText(["31W", "·", "22L", "·", "53 games"]);
  await expect(t.locator(".scope")).toHaveText("This act");
  const colour = (sel: string) => t.locator(sel).evaluate((el) => getComputedStyle(el).color);
  expect(await colour(".w")).toBe(await varRgb(page, "--win"));
  expect(await colour(".l")).toBe(await varRgb(page, "--loss"));
});

// F4/F13 (task 17 review), spec §2.3: no APR tip while a rank is still loading (a count would be false), and with
// 2+ players and nobody ranked the tip says so: `From 0 ranked players` explains the empty box and —.
test("APR tip: none while a rank is loading", async ({ page }) => {
  await openScene(page, "lobby2-loading", {}, { clock: "manual" });
  await expect(page.locator("header.bar .apr .sk")).toHaveCount(2);
  expect(await hoverTip(page, page.locator("header.bar .apr"))).toBeNull();
});

test("APR tip: `From 0 ranked players` when nobody is ranked", async ({ page }) => {
  await openScene(page, "lobby2-unranked", {}, { clock: "manual" });
  await expect(page.locator("header.bar .apr .ebox")).toHaveCount(1);
  expect(await hoverTip(page, page.locator("header.bar .apr"))).toBe("Average party rank\nFrom 0 ranked players");
});

// Task 17 review minor 3: a stat block that is still loading carries no tooltip, like the loading APR and a Win%
// whose games are unknown: the scope explains a number that is not there yet. ingame-partial: Kilo is loading all
// three stats; Cypher is loading only K/D, so his HS% beside it keeps its tip.
test("tips: ingame-partial, a loading K/D, HS% or Win% has no tip", async ({ page }) => {
  await openScene(page, "ingame-partial", {}, { clock: "manual" });
  await expect(card(page, "Kilo").locator(".stat .sk")).toHaveCount(3);
  await expect(card(page, "Cypher").locator(".stat .sk")).toHaveCount(1);
  await runItems(page, [
    { what: "loading K/D", trigger: (p) => stat(p, "Kilo", 0), text: null },
    { what: "loading HS%", trigger: (p) => stat(p, "Kilo", 1), text: null },
    { what: "loading Win%", trigger: (p) => stat(p, "Kilo", 2), text: null },
    { what: "loading K/D beside a known HS%", trigger: (p) => stat(p, "Cypher", 0), text: null },
    { what: "known HS% beside a loading K/D", trigger: (p) => stat(p, "Cypher", 1), text: "Last 5 competitive" },
  ]);
});
