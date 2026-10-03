// Every My Stats tooltip of spec §9.6, hovered in turn, with the engine's placement rules (spec §4.1): each appears with the
// right text after 450ms, inside #app, centred on its trigger and flipped above it near the bottom; the places that have
// none show none. Playwright's fake clock is paused once the page has settled, so the delay is stepped with clock.runFor.
import { expect, test, type Locator, type Page } from "@playwright/test";
import { myStatsFull, myStatsRrNull, myStatsUnranked } from "../src/lib/demo/mystats";
import { dateLong, dateShort, rrDelta, time12 } from "../src/lib/format";
import { DAYPARTS, WEEKDAYS_LONG } from "../src/lib/mystats";
import { NOW, openScene, pointerAway, TIER_LABELS } from "./harness";

// The expected strings are written in the browser's zone (UTC, spec §8.1): make this process's the same.
process.env.TZ = "UTC";

const tip = (page: Page) => page.locator("#app [role=tooltip]");

type Item = { what: string; trigger: (p: Page) => Locator; text: string | null };

/**
 * Bring the trigger into view first and let that scroll's event land (a scroll hides tips), then hover it, step the
 * clock past 450ms and read the tip (null = no tip). Then check its placement against the trigger.
 */
async function hoverTip(page: Page, trigger: Locator): Promise<string | null> {
  await trigger.scrollIntoViewIfNeeded();
  await page.waitForTimeout(60);
  await trigger.hover();
  await page.clock.runFor(460);
  if ((await tip(page).count()) === 0) return null;
  const t = tip(page);
  const [tr, rr, app] = await Promise.all([trigger.boundingBox(), t.boundingBox(), page.locator("#app").boundingBox()]);
  if (!tr || !rr || !app) throw new Error("no box");
  expect(rr.x).toBeGreaterThanOrEqual(app.x);
  expect(rr.y).toBeGreaterThanOrEqual(app.y);
  expect(rr.x + rr.width).toBeLessThanOrEqual(app.x + app.width);
  expect(rr.y + rr.height).toBeLessThanOrEqual(app.y + app.height);
  const want = Math.max(136, Math.min(864, tr.x + tr.width / 2 - app.x));
  expect(Math.abs(rr.x + rr.width / 2 - app.x - want)).toBeLessThanOrEqual(1);
  if (tr.y + tr.height - app.y + 8 > 720) expect(Math.abs(rr.y + rr.height - (tr.y - app.y - 8))).toBeLessThanOrEqual(1);
  else expect(Math.abs(rr.y - (tr.y + tr.height - app.y + 8))).toBeLessThanOrEqual(1);
  return t.evaluate((el) => {
    const wl = el.querySelector(".wl");
    if (!wl) return el.textContent;
    return `${[...wl.children].map((s) => s.textContent).join(" ")}\n${el.querySelector(".scope")?.textContent ?? ""}`;
  });
}

async function runItems(page: Page, items: Item[]): Promise<void> {
  for (const it of items) {
    const trigger = it.trigger(page);
    await expect(trigger, it.what).toHaveCount(1);
    expect(await hoverTip(page, trigger), it.what).toBe(it.text);
    await pointerAway(page);
    await expect(tip(page), `${it.what}: cleared`).toHaveCount(0);
  }
}

const now = NOW.getTime();
const when = (ms: number) => `${dateLong(ms)} · ${time12(ms)}`;
const tierName = (abs: number) => (abs >= 2400 ? "Immortal" : TIER_LABELS[Math.floor(abs / 100) - 3]!);
const wlTip = (w: number, l: number, g: number, scope = "This act") => `${w}W · ${l}L · ${g} ${g === 1 ? "game" : "games"}\n${scope}`;

const sec = (p: Page, name: string) => p.locator(`.mystats .${name}`);
const head = (p: Page, name: string) => sec(p, name).locator(".sech span");

test("tips: mystats, section headers name their scope; no scope is printed", async ({ page }) => {
  await openScene(page, "mystats", {}, { clock: "manual" });
  await runItems(page, [
    { what: "Rank header", trigger: (p) => head(p, "rank"), text: "This act" },
    { what: "RR Progression header", trigger: (p) => head(p, "rrchart"), text: "This act" },
    { what: "Session header", trigger: (p) => head(p, "session"), text: "Current session" },
    { what: "Recent Matches header", trigger: (p) => head(p, "last5"), text: "Last 5 competitive" },
    { what: "Maps header", trigger: (p) => head(p, "maps"), text: "This act" },
    { what: "Agents header", trigger: (p) => head(p, "agents"), text: "This act" },
    { what: "Weekday header", trigger: (p) => head(p, "weekday"), text: "This act" },
    { what: "Time of day header", trigger: (p) => head(p, "daypart"), text: "This act" },
    { what: "Insights header (each row has its own)", trigger: (p) => head(p, "insights"), text: null },
  ]);
  // The scopes live in the tooltips only: nothing on the page says them.
  const words = await page.locator(".mystats").innerText();
  expect(words).not.toMatch(/this act|last 5|last five|current session/i);
});

test("tips: mystats, rank card", async ({ page }) => {
  const m = myStatsFull();
  await openScene(page, "mystats", {}, { clock: "manual" });
  const r = (p: Page) => sec(p, "rank");
  await runItems(page, [
    { what: "progress bar", trigger: (p) => r(p).locator(".bar"), text: "79 RR to next tier" },
    { what: "W / L", trigger: (p) => r(p).locator(".wl"), text: "This act" },
    { what: "Net RR", trigger: (p) => r(p).locator(".cells .cell").nth(0), text: "This act" },
    { what: "Avg win", trigger: (p) => r(p).locator(".cells .cell").nth(1), text: "This act" },
    { what: "Avg loss", trigger: (p) => r(p).locator(".cells .cell").nth(2), text: "This act" },
    { what: "newest rank change: its full date", trigger: (p) => r(p).locator(".cago").nth(0), text: when(m.rank.changes[0]!.startMs) },
    { what: "oldest listed rank change", trigger: (p) => r(p).locator(".cago").nth(3), text: when(m.rank.changes[3]!.startMs) },
    { what: "tier and RR have none", trigger: (p) => r(p).locator(".name"), text: null },
    { what: "rank change name has none", trigger: (p) => r(p).locator(".cname").nth(0), text: null },
  ]);
});

test("tips: mystats, the chart: each point says its change, tier and day; the first has no change", async ({ page }) => {
  const m = myStatsFull();
  await openScene(page, "mystats", {}, { clock: "manual" });
  const hit = (p: Page, i: number) => sec(p, "rrchart").locator(".hit").nth(i);
  const point = (i: number) => {
    const p = m.rrSeries[i]!;
    const delta = i === 0 ? null : p.absoluteRr - m.rrSeries[i - 1]!.absoluteRr;
    return [delta === null ? null : `${rrDelta(delta)} RR`, tierName(p.absoluteRr), dateShort(p.startMs, now)].filter(Boolean).join(" · ");
  };
  const last = m.rrSeries.length - 1;
  const items: Item[] = [0, 1, 17, 30, last - 1, last].map((i) => ({ what: `chart point ${i}`, trigger: (p: Page) => hit(p, i), text: point(i) }));
  await runItems(page, items);
  expect(point(0)).toMatch(/^Diamond \d · Aug 26$/);
  expect(point(last)).toMatch(/^[+−0][\d]* RR · Ascendant 1 · Sep 29$/);
});

test("tips: mystats, a hovered point shows its crosshair and dot; no other point does", async ({ page }) => {
  await openScene(page, "mystats", {}, { clock: "manual" });
  const pts = sec(page, "rrchart").locator(".pt");
  const op = (i: number, cls: string) => pts.nth(i).locator(cls).evaluate((el) => getComputedStyle(el).opacity);
  expect(await op(10, ".xh")).toBe("0");
  await pts.nth(10).locator(".hit").hover();
  expect(await op(10, ".xh")).toBe("1");
  expect(await op(10, ".hot")).toBe("1");
  expect(await op(11, ".xh")).toBe("0");
  expect(await op(9, ".hot")).toBe("0");
});

test("tips: mystats, session", async ({ page }) => {
  const m = myStatsFull();
  await openScene(page, "mystats", {}, { clock: "manual" });
  const c = (p: Page, i: number) => sec(p, "session").locator(".cell").nth(i);
  await runItems(page, [
    { what: "Net RR", trigger: (p) => c(p, 0), text: "Current session" },
    { what: "W / L", trigger: (p) => c(p, 1), text: "Current session" },
    { what: "Started: its full date and time", trigger: (p) => c(p, 2), text: when(m.session!.startedMs) },
    { what: "a streak chip", trigger: (p) => sec(p, "session").locator(".chip").nth(3), text: null },
  ]);
});

test("tips: mystats, Recent Matches", async ({ page }) => {
  await openScene(page, "mystats", {}, { clock: "manual" });
  const row = (p: Page, i: number) => sec(p, "last5").locator(".rrow").nth(i);
  await runItems(page, [
    { what: "time 0", trigger: (p) => row(p, 0).locator(".ago"), text: "9:14 PM" },
    { what: "time 2", trigger: (p) => row(p, 2).locator(".ago"), text: "5:20 PM" },
    { what: "ACS header", trigger: (p) => sec(p, "last5").locator(".rhead span", { hasText: "ACS" }).last(), text: "Average combat score" },
    { what: "map name", trigger: (p) => row(p, 0).locator(".mname"), text: null },
    { what: "RR", trigger: (p) => row(p, 0).locator(".rrr"), text: null },
    { what: "score", trigger: (p) => row(p, 0).locator(".rscore"), text: null },
    { what: "agent", trigger: (p) => row(p, 0).locator(".ragent"), text: null },
  ]);
});

test("tips: mystats, Maps and Agents: Win% is the W · L · games tip, the rest have none", async ({ page }) => {
  const m = myStatsFull();
  await openScene(page, "mystats", {}, { clock: "manual" });
  const items: Item[] = [];
  for (const [name, rows] of [["maps", m.maps], ["agents", m.agents]] as const) {
    for (const i of [0, rows.length - 1]) {
      const r = rows[i]!;
      const cell = (p: Page, n: number) => sec(p, name).locator(".trow").nth(i).locator(".num").nth(n);
      items.push({ what: `${name} row ${i} Win%`, trigger: (p) => cell(p, 2), text: wlTip(r.wins, r.losses, r.games) });
      items.push({ what: `${name} row ${i} Games`, trigger: (p) => cell(p, 0), text: null });
      items.push({ what: `${name} row ${i} Net RR`, trigger: (p) => cell(p, 3), text: null });
    }
    items.push({ what: `${name} ACS header`, trigger: (p) => sec(p, name).locator(".thead span", { hasText: "ACS" }).last(), text: "Average combat score" });
    items.push({ what: `${name} name`, trigger: (p) => sec(p, name).locator(".trow .n").first(), text: null });
  }
  await runItems(page, items);
});

test("tips: mystats, weekday and time-of-day cells say W · L · games and their slice; an empty one says nothing", async ({ page }) => {
  const m = myStatsFull();
  await openScene(page, "mystats", {}, { clock: "manual" });
  const cell = (p: Page, kind: string, i: number) => sec(p, kind).locator(".cell").nth(i);
  const items: Item[] = [];
  for (const i of [0, 3, 6]) {
    const c = m.byWeekday[i]!;
    items.push({ what: `weekday ${i}`, trigger: (p) => cell(p, "weekday", i), text: wlTip(c.wins, c.games - c.wins, c.games, `This act · ${WEEKDAYS_LONG[i]}`) });
  }
  for (const i of [0, 2, 3]) {
    const c = m.byDaypart[i]!;
    items.push({ what: `daypart ${i}`, trigger: (p) => cell(p, "daypart", i), text: wlTip(c.wins, c.games - c.wins, c.games, `This act · ${DAYPARTS[i]!.hours}`) });
  }
  await runItems(page, items);
  expect(m.byDaypart[2]!.games).toBeGreaterThan(0);
});

test("tips: mystats-unranked, an empty weekday cell has no tip, a played one has", async ({ page }) => {
  const cell = (p: Page, kind: string, i: number) => sec(p, kind).locator(".cell").nth(i);
  await openScene(page, "mystats-unranked", {}, { clock: "manual" });
  const u = myStatsUnranked();
  const empty = u.byWeekday.findIndex((c) => c.games === 0);
  const full = u.byWeekday.findIndex((c) => c.games > 0);
  await runItems(page, [
    { what: "an empty weekday cell", trigger: (p) => cell(p, "weekday", empty), text: null },
    { what: "a played weekday cell", trigger: (p) => cell(p, "weekday", full), text: wlTip(u.byWeekday[full]!.wins, u.byWeekday[full]!.games - u.byWeekday[full]!.wins, u.byWeekday[full]!.games, `This act · ${WEEKDAYS_LONG[full]}`) },
  ]);
});

test("tips: mystats, insights say the sample behind them", async ({ page }) => {
  const m = myStatsFull();
  await openScene(page, "mystats", {}, { clock: "manual" });
  const row = (p: Page, i: number) => sec(p, "insights").locator(".irow").nth(i);
  await runItems(
    page,
    m.insights.map((ins, i): Item => ({ what: `insight ${ins.kind}`, trigger: (p) => row(p, i), text: `From ${ins.matches} ${ins.matches === 1 ? "match" : "matches"}` })),
  );
});

test("tips: mystats-unranked, an unranked rank, a missing bar and an empty chart have no tip", async ({ page }) => {
  await openScene(page, "mystats-unranked", {}, { clock: "manual" });
  await runItems(page, [
    { what: "unranked rank", trigger: (p) => sec(p, "rank").locator(".head .name"), text: null },
    { what: "no progress bar to hover", trigger: (p) => sec(p, "rank").locator(".barrow"), text: null },
    { what: "the empty chart", trigger: (p) => sec(p, "rrchart").locator(".none"), text: null },
    { what: "a no-RR session", trigger: (p) => sec(p, "session").locator(".cell").nth(0), text: "Current session" },
  ]);
});

test("tips: mystats-rrnull, `— RR` has no tip and there is no bar to explain", async ({ page }) => {
  expect(myStatsRrNull().rank.rrToNext).toBeNull();
  await openScene(page, "mystats-rrnull", {}, { clock: "manual" });
  await runItems(page, [{ what: "— RR", trigger: (p) => sec(p, "rank").locator(".rr"), text: null }]);
});

test("tips: mystats-new, an empty frame has no tip, but its header still names the scope", async ({ page }) => {
  await openScene(page, "mystats-new", {}, { clock: "manual" });
  await runItems(page, [
    { what: "the empty rank card", trigger: (p) => sec(p, "rank").locator(".head .name"), text: null },
    { what: "its header still names the scope", trigger: (p) => head(p, "rank"), text: "This act" },
  ]);
});

test("scrolling the My Stats container hides an open tip", async ({ page }) => {
  await openScene(page, "mystats", {}, { clock: "manual" });
  const trigger = sec(page, "last5").locator(".rrow").nth(0).locator(".ago");
  await trigger.scrollIntoViewIfNeeded();
  await page.waitForTimeout(60);
  await trigger.hover();
  await page.clock.runFor(460);
  await expect(tip(page)).toHaveText("9:14 PM");
  await page.locator("#app .scroll").evaluate((el) => (el.scrollTop += 40));
  await expect(tip(page)).toHaveCount(0);
});
