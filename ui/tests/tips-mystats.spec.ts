// Every My Stats tooltip of spec §9.6, hovered in turn, with the engine's placement rules (spec §4.1): each appears with the
// right text after 450ms, inside #app, centred on its trigger and flipped above it near the bottom; the places that have
// none show none. Playwright's fake clock is paused once the page has settled, so the delay is stepped with clock.runFor.
import { expect, test, type Locator, type Page } from "@playwright/test";
import { myStatsFull, myStatsImmortal, myStatsRrNull, myStatsUnranked } from "../src/lib/demo/mystats";
import { dateLong, dateShort, rrDelta, time12 } from "../src/lib/format";
import { chartModel, DAYPARTS, rrInTier, WEEKDAYS_LONG } from "../src/lib/mystats";
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
  return readTip(page, trigger);
}

/** Step the clock past the delay and read the tip (null = none), checking its placement against `trigger`'s box. */
async function readTip(page: Page, trigger: Locator): Promise<string | null> {
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

/** The chart point `i`'s tip: `+18 RR · Ascendant 2 · 47 RR · Sep 29`; the first point has no change (spec §9.6). */
function pointTip(series: ReturnType<typeof myStatsFull>["rrSeries"], i: number): string {
  const p = series[i]!;
  const delta = i === 0 ? null : p.absoluteRr - series[i - 1]!.absoluteRr;
  return [delta === null ? null : `${rrDelta(delta)} RR`, tierName(p.absoluteRr), `${rrInTier(p.absoluteRr)} RR`, dateShort(p.startMs, now)]
    .filter(Boolean)
    .join(" · ");
}

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

/** The chart's points in the page's own coordinates: the layer over the plot takes the pointer, so a point is a spot. */
async function chartPoints(page: Page, series: ReturnType<typeof myStatsFull>["rrSeries"]) {
  const chart = sec(page, "rrchart");
  await chart.scrollIntoViewIfNeeded();
  await page.waitForTimeout(60);
  const svg = (await chart.locator("svg").boundingBox())!;
  const model = chartModel(series, { w: svg.width, h: svg.height, padL: 96, padR: 18, padT: 16, padB: 30 })!;
  const spot = (i: number, dx = 0, dy = 0) => ({ x: svg.x + model.points[i]!.x + dx, y: svg.y + model.points[i]!.y + dy });
  return { svg, model, spot, anchor: chart.locator(".anchor") };
}

test("tips: mystats, the chart: each point says its change, tier, RR and day; the first has no change", async ({ page }) => {
  const m = myStatsFull();
  await openScene(page, "mystats", {}, { clock: "manual" });
  const { spot, anchor } = await chartPoints(page, m.rrSeries);
  const point = (i: number) => pointTip(m.rrSeries, i);
  const last = m.rrSeries.length - 1;
  for (const i of [0, 1, 17, 30, last - 1, last]) {
    await page.mouse.move(spot(i).x, spot(i).y);
    expect(await readTip(page, anchor), `chart point ${i}`).toBe(point(i));
    await pointerAway(page);
    await expect(tip(page), `chart point ${i}: cleared`).toHaveCount(0);
  }
  expect(point(0)).toMatch(/^Diamond \d · \d+ RR · Aug 26$/);
  expect(point(last)).toMatch(/^[+−0][\d]* RR · Ascendant 1 · 21 RR · Sep 29$/);
});

test("tips: mystats, the chart: a pointer anywhere over the plot picks the point nearest by x; moving on restarts the tip", async ({ page }) => {
  const m = myStatsFull();
  await openScene(page, "mystats", {}, { clock: "manual" });
  const { svg, model, spot, anchor } = await chartPoints(page, m.rrSeries);
  const step = (model.plot.x1 - model.plot.x0) / (model.points.length - 1);
  const tipAt = async (x: number, y: number) => {
    await page.mouse.move(x, y);
    return readTip(page, anchor);
  };
  const text = (i: number) => {
    const p = m.rrSeries[i]!;
    return `${rrDelta(p.absoluteRr - m.rrSeries[i - 1]!.absoluteRr)} RR · ${tierName(p.absoluteRr)} · ${rrInTier(p.absoluteRr)} RR · ${dateShort(p.startMs, now)}`;
  };
  // A hair left of the midpoint between points 10 and 11 is point 10, a hair right of it is 11 — at any height on the plot.
  const mid = spot(10, step / 2);
  const top = svg.y + model.plot.y0 + 4;
  const bottom = svg.y + model.plot.y1 - 4;
  expect(await tipAt(mid.x - 2, top), "left of the midpoint, top").toBe(text(10));
  expect(await tipAt(mid.x + 2, bottom), "right of the midpoint, bottom").toBe(text(11));
  expect(await tipAt(mid.x + 2, spot(11).y), "same point again: still there").toBe(text(11));
  // Moving to another point takes the bubble away at once and shows the new one after the delay, like any trigger.
  await page.mouse.move(spot(14).x, spot(14).y);
  await expect(tip(page), "the old bubble is gone before the delay").toHaveCount(0);
  expect(await readTip(page, anchor)).toBe(text(14));
  // Staying on the same point (a wiggle inside its slice) keeps the bubble.
  await page.mouse.move(spot(14, 2).x, spot(14).y + 3);
  await page.clock.runFor(100);
  await expect(tip(page)).toHaveCount(1);
  // Off the plot: the gutter has no tip.
  await page.mouse.move(svg.x + 20, spot(14).y);
  await expect(tip(page)).toHaveCount(0);
  await page.clock.runFor(600);
  await expect(tip(page)).toHaveCount(0);
});

test("tips: mystats-immortal, a point above the floor says its RR counted from Immortal 1", async ({ page }) => {
  const m = myStatsImmortal();
  await openScene(page, "mystats-immortal", {}, { clock: "manual" });
  const { spot, anchor } = await chartPoints(page, m.rrSeries);
  const last = m.rrSeries.length - 1;
  const p = m.rrSeries[last]!;
  expect(p.absoluteRr).toBeGreaterThanOrEqual(2400);
  await page.mouse.move(spot(last).x, spot(last).y);
  const delta = p.absoluteRr - m.rrSeries[last - 1]!.absoluteRr;
  expect(await readTip(page, anchor)).toBe(`${rrDelta(delta)} RR · Immortal · ${p.absoluteRr - 2400} RR · ${dateShort(p.startMs, now)}`);
});

test("tips: mystats, a hovered point shows its crosshair and dot; none while the pointer is off the plot", async ({ page }) => {
  const m = myStatsFull();
  await openScene(page, "mystats", {}, { clock: "manual" });
  const { svg, model, spot } = await chartPoints(page, m.rrSeries);
  const chart = sec(page, "rrchart");
  await expect(chart.locator(".xh")).toHaveCount(0);
  await expect(chart.locator(".hot")).toHaveCount(0);
  await page.mouse.move(spot(10).x, spot(10).y);
  await expect(chart.locator(".xh")).toHaveCount(1);
  await expect(chart.locator(".hot")).toHaveCount(1);
  const x1 = async () => Number(await chart.locator(".xh").getAttribute("x1"));
  expect(Math.abs((await x1()) - model.points[10]!.x)).toBeLessThan(0.1);
  expect(Math.abs(Number(await chart.locator(".hot").getAttribute("cx")) - model.points[10]!.x)).toBeLessThan(0.1);
  expect(Math.abs(Number(await chart.locator(".hot").getAttribute("cy")) - model.points[10]!.y)).toBeLessThan(0.1);
  // The crosshair spans the plot's height; the next point takes it over (never two at once).
  expect(Number(await chart.locator(".xh").getAttribute("y1"))).toBeCloseTo(model.plot.y0, 1);
  expect(Number(await chart.locator(".xh").getAttribute("y2"))).toBeCloseTo(model.plot.y1, 1);
  await page.mouse.move(spot(11).x, spot(11).y);
  await expect(chart.locator(".xh")).toHaveCount(1);
  expect(Math.abs((await x1()) - model.points[11]!.x)).toBeLessThan(0.1);
  await page.mouse.move(svg.x + 10, svg.y + 10); // the gutter, off the plot
  await expect(chart.locator(".xh")).toHaveCount(0);
  await expect(chart.locator(".hot")).toHaveCount(0);
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

// ── keyboard (spec §4.1, §9.2): the chart is one tab stop; focus, arrows, Home/End and Escape drive its tip ──────────

/** Focus the My Stats tab and Tab once: the next stop is the chart's. */
async function tabToChart(page: Page): Promise<Locator> {
  await page.getByRole("button", { name: "My Stats" }).focus();
  await page.keyboard.press("Tab");
  const stop = sec(page, "rrchart").locator(".stop");
  await expect(stop).toBeFocused();
  return stop;
}

/** Where the crosshair stands: the x of the picked point, or null when none is picked. */
const crossX = async (page: Page): Promise<number | null> => {
  const xh = sec(page, "rrchart").locator(".xh");
  return (await xh.count()) === 0 ? null : Number(await xh.getAttribute("x1"));
};

test("keyboard: Tab from the My Stats tab lands on the chart; it shows the newest point's tip (the bubble has its id; the point is announced by aria-valuetext, not aria-describedby)", async ({ page }) => {
  const m = myStatsFull();
  await openScene(page, "mystats", {}, { clock: "manual" });
  const { anchor, model } = await chartPoints(page, m.rrSeries);
  const last = m.rrSeries.length - 1;
  const stop = await tabToChart(page);
  await expect(stop).toHaveAttribute("role", "slider");
  await expect(stop).toHaveAttribute("aria-label", "RR progression");
  expect(await stop.evaluate((el) => el.matches(":focus-visible"))).toBe(true);
  // The pick and its crosshair are there at once; the tip follows the delay, like a hover's.
  expect(Math.abs((await crossX(page))! - model.points[last]!.x)).toBeLessThan(0.1);
  await expect(tip(page)).toHaveCount(0);
  await expect(stop).not.toHaveAttribute("aria-describedby", /.+/);
  expect(await readTip(page, anchor)).toBe(pointTip(m.rrSeries, last));
  await expect(tip(page)).toHaveAttribute("id", "tooltip");
  await expect(stop).not.toHaveAttribute("aria-describedby", /.+/); // the slider says its point itself (aria-valuetext)
  await expect(stop).toHaveAttribute("aria-valuenow", String(last));
  await expect(stop).toHaveAttribute("aria-valuetext", pointTip(m.rrSeries, last));
  await expect(stop).toHaveAttribute("aria-valuemax", String(last));
});

test("keyboard: ←/→ move the picked point (clamped), Home and End jump to the ends, each showing that point's tip", async ({ page }) => {
  const m = myStatsFull();
  await openScene(page, "mystats", {}, { clock: "manual" });
  const { anchor, model } = await chartPoints(page, m.rrSeries);
  const last = m.rrSeries.length - 1;
  const stop = await tabToChart(page);
  expect(await readTip(page, anchor)).toBe(pointTip(m.rrSeries, last));

  const at = async (i: number, what: string) => {
    expect(Math.abs((await crossX(page))! - model.points[i]!.x), `${what}: crosshair`).toBeLessThan(0.1);
    await expect(stop, `${what}: value`).toHaveAttribute("aria-valuenow", String(i));
  };
  // ArrowLeft: the old bubble goes at once (and its description with it), the previous point's shows after the delay.
  await page.keyboard.press("ArrowLeft");
  await expect(tip(page)).toHaveCount(0);
  await expect(stop).not.toHaveAttribute("aria-describedby", /.+/);
  await at(last - 1, "ArrowLeft");
  expect(await readTip(page, anchor)).toBe(pointTip(m.rrSeries, last - 1));
  await expect(stop).not.toHaveAttribute("aria-describedby", /.+/);
  // ArrowRight comes back; at the end another ArrowRight changes nothing and does not flicker the bubble.
  await page.keyboard.press("ArrowRight");
  await at(last, "ArrowRight");
  expect(await readTip(page, anchor)).toBe(pointTip(m.rrSeries, last));
  await page.keyboard.press("ArrowRight");
  await expect(tip(page)).toHaveCount(1);
  await at(last, "ArrowRight at the end");
  // Home and End.
  await page.keyboard.press("Home");
  await at(0, "Home");
  expect(await readTip(page, anchor)).toBe(pointTip(m.rrSeries, 0));
  expect(pointTip(m.rrSeries, 0)).toMatch(/^Diamond \d · \d+ RR · Aug 26$/); // the first point has no change
  await page.keyboard.press("ArrowLeft");
  await at(0, "ArrowLeft at the start");
  await page.keyboard.press("End");
  await at(last, "End");
  expect(await readTip(page, anchor)).toBe(pointTip(m.rrSeries, last));
  // A few steps in a row: the tip belongs to the last one.
  for (let k = 0; k < 4; k++) await page.keyboard.press("ArrowLeft");
  await at(last - 4, "4 × ArrowLeft");
  expect(await readTip(page, anchor)).toBe(pointTip(m.rrSeries, last - 4));
  // Home and End are the chart's while it has the focus: they did not scroll the page.
  expect(await page.locator("#app .scroll").evaluate((el) => el.scrollTop)).toBe(0);
});

test("keyboard: Escape hides the tip and does nothing else; the next arrow shows one again", async ({ page }) => {
  const m = myStatsFull();
  await openScene(page, "mystats", {}, { clock: "manual" });
  const { anchor, model } = await chartPoints(page, m.rrSeries);
  const last = m.rrSeries.length - 1;
  const stop = await tabToChart(page);
  expect(await readTip(page, anchor)).toBe(pointTip(m.rrSeries, last));
  await page.keyboard.press("Escape");
  await expect(tip(page)).toHaveCount(0);
  await expect(stop).not.toHaveAttribute("aria-describedby", /.+/);
  // Nothing else happened: the chart keeps the focus and its pick, and no tip comes back by itself.
  await expect(stop).toBeFocused();
  expect(Math.abs((await crossX(page))! - model.points[last]!.x)).toBeLessThan(0.1);
  await page.clock.runFor(1000);
  await expect(tip(page)).toHaveCount(0);
  // Escape with a tip still pending cancels it as well.
  await page.keyboard.press("ArrowLeft");
  await page.clock.runFor(200);
  await page.keyboard.press("Escape");
  await page.clock.runFor(1000);
  await expect(tip(page)).toHaveCount(0);
  // An arrow after Escape: the tip is back, at the new point.
  await page.keyboard.press("ArrowLeft");
  expect(await readTip(page, anchor)).toBe(pointTip(m.rrSeries, last - 2));
});

test("keyboard: Escape also hides a hover's tip", async ({ page }) => {
  await openScene(page, "mystats", {}, { clock: "manual" });
  const trigger = sec(page, "last5").locator(".rrow").nth(0).locator(".ago");
  await trigger.scrollIntoViewIfNeeded();
  await page.waitForTimeout(60);
  await trigger.hover();
  await page.clock.runFor(460);
  await expect(tip(page)).toHaveText("9:14 PM");
  await page.keyboard.press("Escape");
  await expect(tip(page)).toHaveCount(0);
});

test("keyboard: leaving the chart (Tab, Shift+Tab) clears the pick, its crosshair and the tip; coming back picks the newest again", async ({ page }) => {
  const m = myStatsFull();
  await openScene(page, "mystats", {}, { clock: "manual" });
  const { anchor } = await chartPoints(page, m.rrSeries);
  const last = m.rrSeries.length - 1;
  const stop = await tabToChart(page);
  await page.keyboard.press("ArrowLeft");
  expect(await readTip(page, anchor)).toBe(pointTip(m.rrSeries, last - 1));
  await page.keyboard.press("Tab");
  await expect(sec(page, "last5").locator(".rrow").first()).toBeFocused();
  await expect(tip(page)).toHaveCount(0);
  expect(await crossX(page)).toBeNull();
  await expect(stop).not.toHaveAttribute("aria-describedby", /.+/);
  await expect(stop).toHaveAttribute("aria-valuenow", String(last)); // unpicked: the newest
  await page.clock.runFor(1000);
  await expect(tip(page)).toHaveCount(0);
  await page.keyboard.press("Shift+Tab");
  await expect(stop).toBeFocused();
  expect(await readTip(page, anchor)).toBe(pointTip(m.rrSeries, last));
});

test("keyboard: a click on the chart takes the focus but no ring, no keyboard pick and no tip through the focus; the pointer behaves as before", async ({ page }) => {
  const m = myStatsFull();
  await openScene(page, "mystats", {}, { clock: "manual" });
  const { svg, model, spot } = await chartPoints(page, m.rrSeries);
  const stop = sec(page, "rrchart").locator(".stop");
  await page.mouse.click(spot(10).x, spot(10).y);
  await expect(stop).toBeFocused();
  expect(await stop.evaluate((el) => el.matches(":focus-visible"))).toBe(false);
  expect(await stop.evaluate((el) => getComputedStyle(el).outlineStyle)).toBe("none");
  // The pick is the pointer's (point 10), not the keyboard's newest point; the click hid the tip and the focus brings none.
  expect(Math.abs((await crossX(page))! - model.points[10]!.x)).toBeLessThan(0.1);
  await page.clock.runFor(1000);
  await expect(tip(page)).toHaveCount(0);
  await expect(stop).not.toHaveAttribute("aria-describedby", /.+/);
  // Leaving the plot clears the pointer's pick, as it does without focus.
  await page.mouse.move(svg.x + 10, svg.y + 10);
  expect(await crossX(page)).toBeNull();
});

test("keyboard: a hover while the chart has the keyboard takes the tip; leaving the plot keeps the keyboard's pick", async ({ page }) => {
  const m = myStatsFull();
  await openScene(page, "mystats", {}, { clock: "manual" });
  const { svg, model, spot, anchor } = await chartPoints(page, m.rrSeries);
  const last = m.rrSeries.length - 1;
  await tabToChart(page);
  expect(await readTip(page, anchor)).toBe(pointTip(m.rrSeries, last));
  await page.mouse.move(spot(5).x, spot(5).y);
  expect(await readTip(page, anchor)).toBe(pointTip(m.rrSeries, 5));
  await page.mouse.move(svg.x + 10, svg.y + 10); // the gutter: the pointer leaves the plot
  await expect(tip(page)).toHaveCount(0);
  // The keyboard still has the chart: the pick (where the pointer left it) stays until the focus goes.
  expect(Math.abs((await crossX(page))! - model.points[5]!.x)).toBeLessThan(0.1);
  await page.keyboard.press("ArrowRight");
  expect(await readTip(page, anchor)).toBe(pointTip(m.rrSeries, 6));
});

test("keyboard: a pointer entering over the slice the keyboard picked still shows its own tip", async ({ page }) => {
  const m = myStatsFull();
  await openScene(page, "mystats", {}, { clock: "manual" });
  const { spot, anchor } = await chartPoints(page, m.rrSeries);
  const last = m.rrSeries.length - 1;
  await tabToChart(page); // the keyboard picked the newest point
  expect(await readTip(page, anchor)).toBe(pointTip(m.rrSeries, last));
  await page.keyboard.press("Escape");
  await expect(tip(page)).toHaveCount(0);
  // The pointer comes in over that very slice: it is the pointer's turn, so its tip shows (the pick did not change, the tip did).
  await page.mouse.move(spot(last).x, spot(last).y);
  expect(await readTip(page, anchor)).toBe(pointTip(m.rrSeries, last));
  // A wiggle inside the slice keeps the bubble, as for any hover.
  await page.mouse.move(spot(last, 1).x, spot(last).y + 4);
  await page.clock.runFor(100);
  await expect(tip(page)).toHaveCount(1);
});

test("keyboard: the chart prevents only the keys it handles; modified keys, ↑ ↓ and Page keys are left to the page", async ({ page }) => {
  const m = myStatsFull();
  await openScene(page, "mystats", {}, { clock: "manual" });
  await chartPoints(page, m.rrSeries);
  const stop = await tabToChart(page);
  const last = m.rrSeries.length - 1;
  await page.evaluate(() => {
    const w = window as unknown as { __keys: string[] };
    w.__keys = [];
    // The document listener runs after the chart's own (bubbling): `defaultPrevented` says what the chart did.
    document.addEventListener("keydown", (e) => {
      if (["Control", "Shift"].includes(e.key)) return;
      w.__keys.push(`${e.ctrlKey ? "Ctrl+" : ""}${e.shiftKey ? "Shift+" : ""}${e.key}:${e.defaultPrevented ? "prevented" : "left"}`);
    });
  });
  for (const k of ["ArrowLeft", "ArrowRight", "Home", "End", "Control+ArrowLeft", "Shift+ArrowLeft", "Control+Home", "ArrowUp", "ArrowDown", "PageUp", "Escape"]) {
    await page.keyboard.press(k);
  }
  const seen = await page.evaluate(() => (window as unknown as { __keys: string[] }).__keys);
  expect(seen).toEqual([
    "ArrowLeft:prevented",
    "ArrowRight:prevented",
    "Home:prevented",
    "End:prevented",
    "Ctrl+ArrowLeft:left",
    "Shift+ArrowLeft:left",
    "Ctrl+Home:left",
    "ArrowUp:left",
    "ArrowDown:left",
    "PageUp:left",
    "Escape:left",
  ]);
  // The ignored keys moved nothing: End left the pick on the newest point.
  await expect(stop).toHaveAttribute("aria-valuenow", String(last));
});

test("keyboard: a mystats push that inserts older points keeps the pick on its own match, not on its index", async ({ page }) => {
  const m = myStatsFull();
  await openScene(page, "mystats", {}, { clock: "manual" });
  const { svg, anchor } = await chartPoints(page, m.rrSeries);
  const stop = await tabToChart(page);
  await page.keyboard.press("Home");
  for (let k = 0; k < 10; k++) await page.keyboard.press("ArrowRight");
  await expect(stop).toHaveAttribute("aria-valuenow", "10");
  const first = m.rrSeries[0]!;
  const older = [4, 3, 2, 1].map((h) => ({ ...first, startMs: first.startMs - h * 3_600_000 }));
  const next = [...older, ...m.rrSeries];
  await page.evaluate(
    `import("/src/lib/stores.ts").then(({ mystats }) => { let m; mystats.subscribe((v) => (m = v))(); mystats.set({ ...m, rrSeries: ${JSON.stringify(next)} }); })`,
  );
  // The same match, four places further right.
  await expect(stop).toHaveAttribute("aria-valuenow", "14");
  await expect(stop).toHaveAttribute("aria-valuemax", String(next.length - 1));
  await expect(stop).toHaveAttribute("aria-valuetext", pointTip(m.rrSeries, 10));
  expect(pointTip(next, 14)).toBe(pointTip(m.rrSeries, 10));
  const model = chartModel(next, { w: svg.width, h: svg.height, padL: 96, padR: 18, padT: 16, padB: 30 })!;
  expect(Math.abs((await crossX(page))! - model.points[14]!.x)).toBeLessThan(0.1);
  // The next key moves from where the pick now stands.
  await page.keyboard.press("ArrowLeft");
  await expect(stop).toHaveAttribute("aria-valuenow", "13");
  expect(await readTip(page, anchor)).toBe(pointTip(next, 13));
});

test("keyboard: when the plot goes away the pick goes with it and the chart is no tab stop", async ({ page }) => {
  const m = myStatsFull();
  await openScene(page, "mystats", {}, { clock: "manual" });
  await chartPoints(page, m.rrSeries);
  const stop = await tabToChart(page);
  await page.keyboard.press("ArrowLeft");
  await expect(stop).toHaveAttribute("aria-valuenow", String(m.rrSeries.length - 2));
  // A push with a single point: nothing to draw a line through.
  await page.evaluate(
    `import("/src/lib/stores.ts").then(({ mystats }) => { let m; mystats.subscribe((v) => (m = v))(); mystats.set({ ...m, rrSeries: ${JSON.stringify(m.rrSeries.slice(0, 1))} }); })`,
  );
  await expect(sec(page, "rrchart").locator(".stop")).toHaveCount(0);
  await expect(sec(page, "rrchart").locator(".none")).toHaveText("—");
  await expect(tip(page)).toHaveCount(0);
  // The series comes back: the chart starts with no pick (not at the old index), and a fresh focus picks the newest.
  await page.evaluate(
    `import("/src/lib/stores.ts").then(({ mystats }) => { let m; mystats.subscribe((v) => (m = v))(); mystats.set({ ...m, rrSeries: ${JSON.stringify(m.rrSeries)} }); })`,
  );
  await expect(sec(page, "rrchart").locator(".stop")).toHaveCount(1);
  expect(await crossX(page)).toBeNull();
  await expect(sec(page, "rrchart").locator(".stop")).toHaveAttribute("aria-valuenow", String(m.rrSeries.length - 1));
});

test("keyboard: Tab order on My Stats is DOM order: the two tabs, the chart, then the five Recent Matches rows (the cells are not stops)", async ({ page }) => {
  await openScene(page, "mystats");
  const seen: string[] = [];
  for (let i = 0; i < 8; i++) {
    await page.keyboard.press("Tab");
    seen.push(
      await page.evaluate(() => {
        const a = document.activeElement as HTMLElement | null;
        if (!a) return "none";
        if (a.matches("header .tab")) return `tab:${a.textContent}`;
        if (a.matches(".rrchart .stop")) return "chart";
        if (a.matches(".last5 .rrow")) return "row";
        return `other:${a.tagName}.${a.className}`;
      }),
    );
  }
  expect(seen).toEqual(["tab:Live", "tab:My Stats", "chart", "row", "row", "row", "row", "row"]);
});

test("keyboard: with no plot to read (nothing to chart, no answer yet, loading) the chart is not a tab stop", async ({ page }) => {
  for (const scene of ["mystats-unranked", "mystats-new", "mystats-loading"]) {
    await openScene(page, scene);
    await expect(page.locator(".mystats .rrchart .stop"), scene).toHaveCount(0);
    await expect(page.locator(".mystats .rrchart [tabindex]"), scene).toHaveCount(0);
  }
});
