// My Stats fit and per-scene assertions (spec §9.7, and §8.3 as the page uses it) at exactly 1000×800, DPR 1, against the
// DEV demo feed. The page scrolls vertically only; nothing may overlap, clip or crowd a number, in every font.
import { expect, test, type Locator, type Page } from "@playwright/test";
import { myStatsFull, myStatsImmortal, myStatsLongnames, myStatsRrNull, myStatsUnranked } from "../src/lib/demo/mystats";
import { ago, DASH, kd2, pct0, rrDelta, score, span } from "../src/lib/format";
import { WEEKDAYS } from "../src/lib/mystats";
import type { MyStats } from "../src/lib/types";
import { checkMyStatsLayout, myStatsChecks } from "./checks";
import { FONTS, fontSlug, NOW, openScene, pointerAway, SHOTS_DIR, shotFull, varRgb } from "./harness";
import { mkdirSync } from "node:fs";

const SCENES = [
  "mystats", "mystats-new", "mystats-loading", "mystats-unranked", "mystats-immortal", "mystats-rrnull", "mystats-longnames", "mystats-closed",
];

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
const texts = async (l: Locator) => (await l.allInnerTexts()).map((t) => t.replace(/\s+/g, " ").trim());
/** SVG text has no innerText. */
const svgTexts = async (l: Locator) => (await l.allTextContents()).map((t) => t.trim());
const hit = (a: Box, b: Box) =>
  Math.min(a.x + a.width, b.x + b.width) - Math.max(a.x, b.x) > 0.5 && Math.min(a.y + a.height, b.y + b.height) - Math.max(a.y, b.y) > 0.5;
const route = (page: Page) =>
  page.evaluate(`import("/src/lib/router.svelte.ts").then(({ router }) => JSON.parse(JSON.stringify(router.route)))`) as Promise<unknown>;

const page_ = (p: Page) => p.locator(".mystats");
const ago_ = (ms: number) => ago(ms, NOW.getTime());
/** `+18` / `−15` / `0` / `—`, as the page writes a signed RR. */
const rr = (n: number | null) => rrDelta(n);

// ── spec §9.7: checks 1, 3, 4, 5, 7, 8 and the layout check on every scene, in every font; a shot of each ──────────────
for (const scene of SCENES) {
  for (const font of FONTS) {
    test(`fit: ${scene} [${font}]`, async ({ page }) => {
      await openScene(page, scene, { font });
      await pointerAway(page);
      expect(await myStatsChecks(page, font)).toEqual([]);
      await shotFull(page, `${scene}__${fontSlug(font)}`);
    });
  }
}

// The option matrix (spec §8.6) on the full page: every tone, result palette, fill and accent still reads, and fits.
const OPTIONS: Array<[string, Record<string, string>]> = [
  ["cool", { tone: "Cool" }],
  ["warm", { tone: "Warm" }],
  ["solid", { fill: "Solid" }],
  ["accent-red", { results: "Accent / Red" }],
  ["accent-gray", { results: "Accent / Gray" }],
  ["teal", { accent: "#4fc3ae" }],
  ["blue", { accent: "#7fb1ef" }],
  ["pink", { accent: "#e58a9a" }],
];
for (const [label, params] of OPTIONS) {
  test(`fit: mystats [${label}]`, async ({ page }) => {
    await openScene(page, "mystats", params);
    await pointerAway(page);
    expect(await myStatsChecks(page, "Barlow")).toEqual([]);
    await shotFull(page, `mystats__${label}`);
    // The chart line and the progress bar follow the accent; wins and losses follow the result palette.
    const accent = params.accent ?? "#e3a54f";
    const hex = (c: string) => `rgb(${[1, 3, 5].map((i) => parseInt(c.slice(i, i + 2), 16)).join(", ")})`;
    expect(await css(page.locator(".mystats .rrchart .line"), "stroke")).toBe(hex(accent));
    expect(await css(page.locator(".mystats .rank .fill"), "background-color")).toBe(hex(accent));
    expect(await css(page.locator(".mystats .rank .wl .w"), "color")).toBe(await varRgb(page, "--win"));
    expect(await css(page.locator(".mystats .rank .wl .l"), "color")).toBe(await varRgb(page, "--loss"));
  });
}

// The first screen of each scene, exactly 1000×800 (the window the user has), next to the full-height shot above.
for (const scene of SCENES) {
  test(`viewport shot: ${scene}`, async ({ page }) => {
    await openScene(page, scene);
    await pointerAway(page);
    mkdirSync(SHOTS_DIR, { recursive: true });
    await page.screenshot({ path: `${SHOTS_DIR}/${scene}__viewport.png` });
    const size = page.viewportSize();
    expect(size).toEqual({ width: 1000, height: 800 });
  });
}

// ── the full page ───────────────────────────────────────────────────────────────────────────────────────────────────
test("mystats: the sections, in the spec's order, each with its header", async ({ page }) => {
  await openScene(page, "mystats");
  expect(await texts(page.locator(".mystats .sech"))).toEqual([
    "Rank", "RR Progression", "Session", "Recent Matches", "Maps", "Agents", "Win Rate by Weekday", "Win Rate by Time of Day", "Insights",
  ]);
  await expect(page.locator(".mystats")).toHaveAttribute("aria-busy", "false");
  // The tab: My Stats is the active one, with the accent indicator; the right group of the top bar is empty.
  await expect(page.locator("header.bar .tab.on")).toHaveText("My Stats");
  await expect(page.locator("header.bar .tab.on .ind")).toHaveCount(1);
  await expect(page.locator("header.bar .tab.off")).toHaveText("Live");
  await expect(page.locator("header.bar .right")).toHaveCount(0);
  // Scrolls vertically (longer than the window), in the same container Details uses.
  expect(await page.locator("#app .scroll").evaluate((el) => el.scrollHeight > el.clientHeight)).toBe(true);
});

test("mystats: the rank beside the chart, the same height, filling the width", async ({ page }) => {
  await openScene(page, "mystats");
  const rank = await box(page.locator(".mystats .rank"));
  const chart = await box(page.locator(".mystats .rrchart"));
  near(rank.y, chart.y, "same top");
  near(rank.height, chart.height, "same height");
  expect(chart.x).toBeGreaterThan(rank.x + rank.width);
  const all = await box(page.locator(".mystats"));
  near(chart.x + chart.width, all.x + all.width, "chart ends at the content's right edge");
});

test("mystats: the rank card reads the data", async ({ page }) => {
  const m = myStatsFull();
  await openScene(page, "mystats");
  const r = page.locator(".mystats .rank");
  expect(await text(r.locator(".tier"))).toBe("Ascendant 1");
  expect(await text(r.locator(".rr"))).toBe("21 RR");
  await expect(r.locator(".head img.icon")).toHaveCount(1);
  expect(await text(r.locator(".wl .val"))).toBe(`${m.rank.wins} / ${m.rank.losses}`);
  expect(await css(r.locator(".wl .w"), "color")).toBe(await varRgb(page, "--win"));
  expect(await css(r.locator(".wl .l"), "color")).toBe(await varRgb(page, "--loss"));
  // The progress bar: the accent, 21% of the track.
  const bar = await box(r.locator(".bar"));
  const fill = await box(r.locator(".bar .fill"));
  near(fill.width / bar.width, 0.21, "progress");
  near(bar.height, 4, "bar height");
  expect(await css(r.locator(".bar .fill"), "background-color")).toBe(await varRgb(page, "--accent"));
  expect(await css(r.locator(".bar"), "background-color")).toBe(await varRgb(page, "--line2"));
  // Net RR, Avg win, Avg loss: signed, coloured.
  expect(await texts(r.locator(".cells .lab"))).toEqual(["Net RR", "Avg win", "Avg loss"]);
  expect(await texts(r.locator(".cells .cv"))).toEqual([rr(m.rank.netRr), rr(m.rank.avgWin), rr(m.rank.avgLoss)]);
  const tone = ["--win", "--win", "--loss"];
  for (let i = 0; i < 3; i++) expect(await css(r.locator(".cells .cv").nth(i), "color")).toBe(await varRgb(page, tone[i]!));
  // The three figures are apart by hairlines (the Details hero's and the Session's way), not bordered tiles.
  const line = await varRgb(page, "--line");
  for (let i = 0; i < 3; i++) {
    const cell = r.locator(".cells .cell").nth(i);
    for (const side of ["top", "right", "bottom"]) {
      expect(await css(cell, `border-${side}-width`), `cell ${i} ${side}`).toBe("0px");
    }
    expect(await css(cell, "border-radius"), `cell ${i} radius`).toBe("0px");
    expect(await css(cell, "border-left-width"), `cell ${i} divider`).toBe(i === 0 ? "0px" : "1px");
    if (i > 0) expect(await css(cell, "border-left-color"), `cell ${i} divider colour`).toBe(line);
  }
  expect(await css(r.locator(".cells .cell").nth(0), "padding-left")).toBe("0px");
  // The latest changes, newest first, at most 4.
  const rows = r.locator(".crow");
  await expect(rows).toHaveCount(Math.min(4, m.rank.changes.length));
  const names = ["Diamond 3", "Ascendant 1"];
  for (let i = 0; i < (await rows.count()); i++) {
    const c = m.rank.changes[i]!;
    expect(await text(rows.nth(i).locator(".cname"))).toBe(c.tier === 21 ? names[1] : names[0]);
    expect(await text(rows.nth(i).locator(".cago"))).toBe(ago_(c.startMs));
    expect(await rows.nth(i).locator(".arrow").evaluate((el, up) => el.classList.contains(up ? "up" : "down"), c.up)).toBe(true);
    expect(await css(rows.nth(i).locator(".arrow"), "color")).toBe(await varRgb(page, c.up ? "--win" : "--loss"));
  }
  expect(await text(r.locator(".chg .lab"))).toBe("Rank changes");
});

test("mystats: the chart is an SVG line over the act with tier lines, tier names, the newest point and the two end dates", async ({ page }) => {
  const m = myStatsFull();
  await openScene(page, "mystats");
  const c = page.locator(".mystats .rrchart");
  await expect(c.locator("svg")).toHaveCount(1);
  await expect(c.locator("canvas, img")).toHaveCount(0); // no chart library, no bitmap
  // One hover layer over the plot (and the tip's anchor box): no hit rect per point.
  await expect(c.locator("svg .hit")).toHaveCount(1);
  await expect(c.locator("svg rect")).toHaveCount(2);
  expect(m.rrSeries.length).toBeGreaterThan(10);
  expect(await css(c.locator(".line"), "stroke")).toBe(await varRgb(page, "--accent"));
  expect(await css(c.locator(".line"), "stroke-width")).toBe("2px");
  expect(await css(c.locator(".end"), "fill")).toBe(await varRgb(page, "--accent"));
  // Grid: hairlines in --line, one per tier boundary inside the range; labels in --text2, 12px.
  const grid = c.locator(".grid");
  expect(await grid.count()).toBeGreaterThan(0);
  expect(await css(grid.first(), "stroke")).toBe(await varRgb(page, "--line"));
  const labels = c.locator(".band");
  expect(await labels.count()).toBeGreaterThan(0);
  for (const l of await labels.all()) {
    expect(await css(l, "fill")).toBe(await varRgb(page, "--text2"));
    expect(await css(l, "font-size")).toBe("12px");
  }
  for (const d of await c.locator(".date").all()) {
    expect(await css(d, "fill")).toBe(await varRgb(page, "--text2"));
    expect(await css(d, "font-size")).toBe("12px");
  }
  expect(await svgTexts(labels)).toContain("Ascendant 1");
  expect(await svgTexts(c.locator(".date"))).toEqual(["Aug 26", "Sep 29"]);
  // The tier names sit in the left gutter, whole and apart from each other; the dates under the plot, apart.
  const svg = await box(c.locator("svg"));
  const plotLeft = svg.x + 96;
  const lb: Box[] = [];
  for (const l of await labels.all()) lb.push(await box(l));
  for (const b of lb) expect(b.x + b.width, "tier name ends before the plot").toBeLessThanOrEqual(plotLeft + 0.5);
  for (const b of lb) expect(b.x, "tier name inside the chart").toBeGreaterThanOrEqual(svg.x);
  for (let i = 0; i < lb.length; i++) for (let j = i + 1; j < lb.length; j++) expect(hit(lb[i]!, lb[j]!), `tier names ${i} and ${j}`).toBe(false);
  const d = c.locator(".date");
  expect(hit(await box(d.nth(0)), await box(d.nth(1)))).toBe(false);
  // The line runs left to right over the plot, the newest point at its right end, inside the SVG.
  const end = await box(c.locator(".end"));
  near(end.x + end.width / 2, svg.x + svg.width - 18, "newest point at the right end of the plot");
  expect(end.y).toBeGreaterThanOrEqual(svg.y);
  expect(end.y + end.height).toBeLessThanOrEqual(svg.y + svg.height);
});

test("mystats: the session reads the data", async ({ page }) => {
  const m = myStatsFull();
  await openScene(page, "mystats");
  const s = page.locator(".mystats .session");
  expect(await texts(s.locator(".lab"))).toEqual(["Net RR", "W / L", "Started"]);
  expect(await text(s.locator(".big"))).toBe(rr(m.session!.netRr));
  expect(await css(s.locator(".big"), "font-size")).toBe("28px");
  expect(await text(s.locator(".val").nth(0))).toBe(`${m.session!.wins} / ${m.session!.losses}`);
  expect(await text(s.locator(".val").nth(1))).toBe(`${span(NOW.getTime() - m.session!.startedMs)} ago`);
  // One chip per match, oldest to newest, W / L / D: the plain 20×20 boxes, 6px apart, each a button that opens its match.
  const chips = s.locator(".chips .chip");
  await expect(chips).toHaveCount(m.session!.matches.length);
  expect(await texts(chips)).toEqual(m.session!.matches.map((x) => x.result));
  const btns = s.locator(".chips .mc");
  await expect(btns).toHaveCount(m.session!.matches.length);
  expect(await btns.evaluateAll((els) => els.map((e) => [e.getAttribute("role"), e.getAttribute("tabindex"), getComputedStyle(e).cursor]))).toEqual(
    m.session!.matches.map(() => ["button", "0", "pointer"]),
  );
  const first = await box(chips.nth(0));
  const second = await box(chips.nth(1));
  expect([first.width, first.height]).toEqual([20, 20]);
  near(second.x - (first.x + first.width), 6, "gap between chips");
  expect((await box(btns.nth(0))).width).toBe(20);
  // The button's accessible name is the result and the tip, not the bare letter.
  const w = { W: "Win", L: "Loss", D: "Draw" } as const;
  for (const i of [0, m.session!.matches.length - 1]) {
    expect(await btns.nth(i).getAttribute("aria-label")).toMatch(new RegExp(`^${w[m.session!.matches[i]!.result]} · .+ · \\d+[mhdwy]`));
  }
});

test("mystats: Recent Matches are the Details rows with ACS and RR, newest first", async ({ page }) => {
  const m = myStatsFull();
  await openScene(page, "mystats");
  const t = page.locator(".mystats .last5");
  expect(await texts(t.locator(".rhead > span"))).toEqual(["Map", "", "Score", "K / D / A", "K/D", "ACS", "HS%", "RR", ""]);
  const rows = t.locator(".rrow");
  await expect(rows).toHaveCount(5);
  expect(await texts(rows.locator(".ago"))).toEqual(m.last5.map((x) => ago_(x.startMs)));
  expect(await texts(rows.locator(".rscore"))).toEqual(m.last5.map((x) => `${x.result === "win" ? "W" : x.result === "loss" ? "L" : "D"} ${score(x.roundsWon, x.roundsLost)}`));
  expect(await texts(rows.locator(".rkd"))).toEqual(m.last5.map((x) => kd2(x.kd)));
  expect(await texts(rows.locator(".racs"))).toEqual(m.last5.map((x) => String(Math.round(x.acs!))));
  expect(await texts(rows.locator(".rhs"))).toEqual(m.last5.map((x) => pct0(x.hsPct)));
  expect(await texts(rows.locator(".rrr"))).toEqual(m.last5.map((x) => rr(x.rrEarned)));
  // The RR is coloured by sign: +18 green, −19 red.
  expect(await css(rows.nth(0).locator(".rrr"), "color")).toBe(await varRgb(page, "--win"));
  expect(await css(rows.nth(1).locator(".rrr"), "color")).toBe(await varRgb(page, "--loss"));
  // The Details row: 52px, plus the 1px separator on every row but the first.
  for (let i = 0; i < 5; i++) near((await box(rows.nth(i))).height, i === 0 ? 52 : 53, `row ${i} height`);
  for (const r of await rows.all()) expect(await css(r, "cursor")).toBe("pointer");
});

test("mystats: Recent Matches rows share the Details rows' inset, map column and agent-icon column exactly", async ({ page }) => {
  /** Offsets inside the panel: where the row's content starts, the agent icon, the chevron's right edge, the header's first label. */
  const measure = async (scene: string, panel: string) => {
    await openScene(page, scene);
    const p = page.locator(panel);
    await p.scrollIntoViewIfNeeded();
    const pb = await box(p);
    const row = p.locator(".rrow").first();
    const rb = await box(row);
    const agent = await box(row.locator(".ragent"));
    const map = await box(row.locator(".mimg"));
    const chevron = await box(row.locator("svg").last());
    const head = await box(p.locator(".rhead > span").first());
    return {
      padL: await css(row, "padding-left"),
      padR: await css(row, "padding-right"),
      headPadL: await css(p.locator(".rhead"), "padding-left"),
      rowLeft: rb.x - pb.x,
      rowRight: pb.x + pb.width - (rb.x + rb.width),
      map: map.x - pb.x,
      agent: agent.x - pb.x,
      agentSize: [agent.width, agent.height],
      chevronRight: pb.x + pb.width - (chevron.x + chevron.width),
      head: head.x - pb.x,
      rowHeight: rb.height,
    };
  };
  const stats = await measure("mystats", ".mystats .last5 .panel");
  const details = await measure("details", ".details .recent .panel");
  expect(stats, "My Stats rows against Details rows").toEqual(details);
  expect(stats.padL).toBe("8px");
  expect(stats.agentSize).toEqual([28, 28]);
});

for (const kind of ["maps", "agents"] as const) {
  test(`mystats: the ${kind} table, most played first, with Win% and Net RR coloured`, async ({ page }) => {
    const m = myStatsFull();
    const rows = kind === "maps" ? m.maps : m.agents;
    await openScene(page, "mystats");
    const t = page.locator(`.mystats .${kind}`);
    expect(await texts(t.locator(".thead > span"))).toEqual([kind === "maps" ? "Map" : "Agent", "Games", "W–L", "Win%", "Net RR", "K/D", "ACS"]);
    const trs = t.locator(".trow");
    await expect(trs).toHaveCount(rows.length);
    for (let i = 0; i < rows.length; i++) {
      const r = rows[i]!;
      const cells = trs.nth(i).locator(".num");
      expect(await texts(cells), `row ${i}`).toEqual([String(r.games), score(r.wins, r.losses), pct0(r.winrate), rr(r.netRr), kd2(r.kd), String(Math.round(r.acs!))]);
      const pctColour = Math.round(r.winrate * 100) > 50 ? "--win" : Math.round(r.winrate * 100) < 50 ? "--loss" : "--text";
      expect(await css(cells.nth(2), "color"), `row ${i} Win%`).toBe(await varRgb(page, pctColour));
      const rrColour = (r.netRr ?? 0) > 0 ? "--win" : (r.netRr ?? 0) < 0 ? "--loss" : "--text";
      expect(await css(cells.nth(3), "color"), `row ${i} Net RR`).toBe(await varRgb(page, rrColour));
      // Numbers are right-aligned in tabular figures.
      expect(await css(cells.nth(0), "text-align")).toBe("right");
      expect(await css(cells.nth(0), "font-variant-numeric")).toContain("tabular-nums");
    }
    if (kind === "agents") await expect(trs.locator("img.ticon")).toHaveCount(rows.length);
    else await expect(trs.locator("img")).toHaveCount(0);
    // The header row is in --text2, 12px (like Details' column headers).
    expect(await css(t.locator(".thead"), "color")).toBe(await varRgb(page, "--text2"));
    expect(await css(t.locator(".thead"), "font-size")).toBe("12px");
  });
}

test("mystats: weekday (Mon … Sun) and time-of-day cells: win rate, a bar of that height, a label", async ({ page }) => {
  const m = myStatsFull();
  await openScene(page, "mystats");
  const check = async (kind: "weekday" | "daypart", cells: MyStats["byWeekday"] | MyStats["byDaypart"], labels: readonly string[]) => {
    const p = page.locator(`.mystats .${kind}`);
    const cs = p.locator(".cell");
    await expect(cs).toHaveCount(labels.length);
    expect(await texts(cs.locator(".lab"))).toEqual([...labels]);
    for (let i = 0; i < labels.length; i++) {
      const c = cells[i]!;
      const cell = cs.nth(i);
      expect(await text(cell.locator(".pct")), `${kind} ${labels[i]}`).toBe(c.winrate === null ? DASH : pct0(c.winrate));
      const track = await box(cell.locator(".track"));
      if (c.winrate === null) {
        await expect(cell.locator(".fill")).toHaveCount(0);
        expect(await css(cell.locator(".pct"), "color")).toBe(await varRgb(page, "--text2"));
      } else {
        near((await box(cell.locator(".fill"))).height / track.height, c.winrate, `${kind} ${labels[i]} bar`);
      }
    }
  };
  await check("weekday", m.byWeekday, WEEKDAYS);
  await check("daypart", m.byDaypart, ["Morning", "Afternoon", "Evening", "Night"]);
  // Side by side, one column width each (7 and 4 columns), ending on one line.
  const [a, b] = [await box(page.locator(".mystats .weekday")), await box(page.locator(".mystats .daypart"))];
  near(a.y, b.y, "same top");
  near(a.height, b.height, "same height");
  // 7 and 4 columns of one width, give or take the panels' own side padding.
  const wd = (await box(page.locator(".mystats .weekday .cell").first())).width;
  const dp = (await box(page.locator(".mystats .daypart .cell").first())).width;
  expect(Math.abs(wd - dp), `cell widths ${wd} and ${dp}`).toBeLessThanOrEqual(4);
});

test("mystats: insights: a dot (good green, a weak spot neutral grey, never red), the finished text with real minus signs", async ({ page }) => {
  const m = myStatsFull();
  await openScene(page, "mystats");
  const rows = page.locator(".mystats .insights .irow");
  await expect(rows).toHaveCount(m.insights.length);
  expect(await texts(rows.locator(".text"))).toEqual(m.insights.map((i) => i.text.replace(/(^|[\s(])-(?=\d)/g, "$1−")));
  for (let i = 0; i < m.insights.length; i++) {
    expect(await css(rows.nth(i).locator(".dot"), "background-color")).toBe(await varRgb(page, m.insights[i]!.positive ? "--win" : "--text2"));
  }
  expect(m.insights.some((i) => !i.positive), "the demo has a weak spot to check").toBe(true);
  const loss = await varRgb(page, "--loss");
  for (const d of await rows.locator(".dot").all()) expect(await css(d, "background-color")).not.toBe(loss);
  expect(await texts(rows.locator(".text"))).toContainEqual(expect.stringMatching(/−\d+ RR on losses/));
  expect((await texts(rows.locator(".text"))).join("\n")).not.toMatch(/(^|\s)-\d/);
});

// ── the other states ────────────────────────────────────────────────────────────────────────────────────────────────
test("mystats-new: Rust knows nothing yet: every frame stays, with —, no skeleton and no image", async ({ page }) => {
  await openScene(page, "mystats-new");
  const p = page_(page);
  await expect(p).toHaveAttribute("aria-busy", "false");
  expect(await texts(p.locator(".sech"))).toHaveLength(9);
  await expect(p.locator(".sk")).toHaveCount(0);
  await expect(p.locator("img")).toHaveCount(0);
  await expect(p.locator("svg")).toHaveCount(0);
  expect(await text(p.locator(".rank .tier"))).toBe("—");
  expect(await texts(p.locator(".rank .cells .cv"))).toEqual(["—", "—", "—"]);
  expect(await text(p.locator(".rank .wl .val"))).toBe("— / —");
  expect(await text(p.locator(".rrchart .none"))).toBe("—");
  expect(await texts(p.locator(".session .val, .session .big"))).toEqual(["—", "—", "—"]);
  // Every unknown value is the one dim tone: not the text colour, and not the win / loss colours.
  const dim = await varRgb(page, "--text2");
  for (const el of await p.locator(".rank .cells .cv, .rank .wl .w, .rank .wl .l, .rank .wl .dim, .session .val, .session .big").all()) {
    expect(await css(el, "color")).toBe(dim);
  }
  expect(await p.locator(".rank .wl .val .dim").count()).toBe(2);
  await expect(p.locator(".rank .wl .w, .rank .wl .l")).toHaveCount(0);
  expect(await text(p.locator(".last5 .rrow"))).toBe("—");
  expect(await text(p.locator(".maps .trow"))).toBe("—");
  expect(await text(p.locator(".agents .trow"))).toBe("—");
  expect(await texts(p.locator(".weekday .pct, .daypart .pct"))).toEqual(Array(11).fill("—"));
  await expect(p.locator(".weekday .fill, .daypart .fill")).toHaveCount(0);
  expect(await text(p.locator(".insights .irow"))).toBe("—");
  // One-line "none" rows are the Details empty row's 52px; the chart frame keeps its size.
  near((await box(p.locator(".last5 .rrow"))).height, 52, "empty recent row");
});

test("mystats-loading: neutral skeleton shapes only; section headers and column labels are the only text", async ({ page }) => {
  await openScene(page, "mystats-loading");
  const p = page_(page);
  await expect(p).toHaveAttribute("aria-busy", "true");
  await expect(p.locator("img")).toHaveCount(0);
  expect(await text(p)).toBe(
    "Rank RR Progression Session Recent Matches Map Score K / D / A K/D ACS HS% RR Maps Map Games W–L Win% Net RR K/D ACS Agents Agent Games W–L Win% Net RR K/D ACS Win Rate by Weekday Win Rate by Time of Day Insights",
  );
  expect(await css(p.locator(".rrchart .skel"), "background-color")).toBe(await varRgb(page, "--skel"));
  expect(await p.locator(".sk").count()).toBeGreaterThan(30);
  for (const s of await p.locator(".sk").all()) expect(await css(s, "background-color")).toBe(await varRgb(page, "--skel"));
});

test("loading → loaded: the rank card, the session and Recent Matches do not move", async ({ page }) => {
  for (const font of FONTS) {
    const at = async (scene: string) => {
      await openScene(page, scene, { font });
      const p = page_(page);
      return {
        rank: await box(p.locator(".rank")),
        session: await box(p.locator(".session .sech")),
        last5: await box(p.locator(".last5 .sech")),
        last5Panel: await box(p.locator(".last5 .panel")),
        maps: await box(p.locator(".maps .sech")),
      };
    };
    const loading = await at("mystats-loading");
    const loaded = await at("mystats");
    near(loading.rank.height, loaded.rank.height, `rank card height [${font}]`);
    near(loading.session.y, loaded.session.y, `Session header y [${font}]`);
    near(loading.last5.y, loaded.last5.y, `Recent Matches header y [${font}]`);
    near(loading.last5Panel.height, loaded.last5Panel.height, `Recent Matches panel height [${font}]`);
    near(loading.maps.y, loaded.maps.y, `Maps header y [${font}]`);
  }
});

test("mystats-unranked: no rank (—), nothing to chart, no RR anywhere, no insight", async ({ page }) => {
  const m = myStatsUnranked();
  await openScene(page, "mystats-unranked");
  const p = page_(page);
  expect(await text(p.locator(".rank .tier"))).toBe("—");
  await expect(p.locator(".rank img")).toHaveCount(0);
  expect(await text(p.locator(".rank .wl .val"))).toBe(`${m.rank.wins} / ${m.rank.losses}`);
  expect(await texts(p.locator(".rank .cells .cv"))).toEqual(["—", "—", "—"]);
  await expect(p.locator(".rank .bar")).toHaveCount(0);
  expect(await text(p.locator(".rank .crow"))).toBe("—");
  expect(await text(p.locator(".rrchart .none"))).toBe("—");
  await expect(p.locator(".rrchart svg")).toHaveCount(0);
  expect(await text(p.locator(".session .big"))).toBe("—"); // a session with no RR change stored
  expect(await texts(p.locator(".last5 .rrr"))).toEqual(["—", "—", "—"]);
  // The unknown RR of a match, the session's net RR and the table's Net RR are the one dim tone.
  const dim = await varRgb(page, "--text2");
  expect(await css(p.locator(".session .big"), "color")).toBe(dim);
  for (const el of await p.locator(".last5 .rrr").all()) expect(await css(el, "color")).toBe(dim);
  for (const el of await p.locator(".maps .trow .num:nth-child(5), .agents .trow .num:nth-child(5)").all()) {
    expect(await css(el, "color")).toBe(dim);
  }
  expect((await texts(p.locator(".maps .trow")))).toHaveLength(m.maps.length);
  for (const t of await texts(p.locator(".maps .trow .num:nth-child(5), .agents .trow .num:nth-child(5)"))) expect(t).toBe("—");
  expect(await text(p.locator(".insights .irow"))).toBe("—");
});

test("mystats-immortal: no progress bar and the chart climbs from the 2400 floor; mystats-rrnull: a tier with — RR", async ({ page }) => {
  const imm = myStatsImmortal();
  await openScene(page, "mystats-immortal");
  let p = page_(page);
  expect(await text(p.locator(".rank .tier"))).toBe("Immortal 1");
  expect(await text(p.locator(".rank .rr"))).toBe(`${imm.rank.rr} RR`);
  await expect(p.locator(".rank .bar")).toHaveCount(0);
  expect(await svgTexts(p.locator(".rrchart .band"))).toEqual(["Immortal+"]);
  // Above the floor there are no tiers: 3–4 round-number lines, labelled `N RR` (counted from Immortal 1's floor), apart
  // from the band name and from each other, in the gutter, in the frame's quiet colour.
  const marks = p.locator(".rrchart .rrmark");
  const labels = await svgTexts(marks);
  expect(labels.length).toBeGreaterThanOrEqual(3);
  expect(labels.length).toBeLessThanOrEqual(4);
  for (const l of labels) expect(l).toMatch(/^[1-9]\d* RR$/);
  const values = labels.map((l) => parseInt(l, 10));
  for (let i = 1; i < values.length; i++) expect(values[i]!).toBeGreaterThan(values[i - 1]!); // lowest line first
  for (const v of values) expect(v % 5).toBe(0);
  const svgBox = await box(p.locator(".rrchart svg"));
  const all = [...(await marks.all()), ...(await p.locator(".rrchart .band").all())];
  const boxes: Box[] = [];
  for (const l of all) {
    expect(await css(l, "fill")).toBe(await varRgb(page, "--text2"));
    expect(await css(l, "font-size")).toBe("12px");
    const b = await box(l);
    expect(b.x + b.width, "label ends before the plot").toBeLessThanOrEqual(svgBox.x + 96 + 0.5);
    boxes.push(b);
  }
  for (let i = 0; i < boxes.length; i++) for (let j = i + 1; j < boxes.length; j++) expect(hit(boxes[i]!, boxes[j]!), `labels ${i} and ${j}`).toBe(false);
  expect(await p.locator(".rrchart .grid").count()).toBe(labels.length); // above the floor every line is an RR line
  expect(await text(p.locator(".rank .crow"))).toBe("—"); // all of it is above 2400 (Immortal 1): no tier change to list

  await openScene(page, "mystats");
  const withBar = await box(page_(page).locator(".rank"));

  await openScene(page, "mystats-rrnull");
  p = page_(page);
  expect(await text(p.locator(".rank .tier"))).toBe("Ascendant 1");
  expect(await text(p.locator(".rank .rr"))).toBe("— RR"); // same markup and place as a number (R154's look)
  await expect(p.locator(".rank .rr .v")).toHaveText("—");
  await expect(p.locator(".rank .bar")).toHaveCount(0);
  const rrnull = myStatsRrNull();
  expect(rrnull.rank.tier).toBe(21);
  // The bar's row keeps its height when there is no bar: the card is the size it is with one.
  near((await box(p.locator(".rank"))).height, withBar.height, "rank card height with an unknown RR");
});

test("mystats-longnames: the widest numbers and names fit; the streak wraps; the same page, not another", async ({ page }) => {
  const m = myStatsLongnames();
  for (const font of FONTS) {
    await openScene(page, "mystats-longnames", { font });
    const p = page_(page);
    expect(await text(p.locator(".maps .trow .n").first())).toBe("Basic Training");
    expect(await texts(p.locator(".agents .trow .n")).then((a) => a.slice(0, 2))).toEqual(["Brimstone", "Deadlock"]);
    expect(await text(p.locator(".maps .trow").first())).toContain("128");
    expect(await text(p.locator(".rank .tier"))).toBe("Ascendant 3");
    expect(await text(p.locator(".session .big"))).toBe(rr(m.session!.netRr));
    // 30 chips wrap onto more than one line, inside the panel.
    const chips = p.locator(".session .chip");
    await expect(chips).toHaveCount(m.session!.matches.length);
    const ys = new Set((await chips.evaluateAll((els) => els.map((e) => Math.round(e.getBoundingClientRect().y)))) as number[]);
    expect(ys.size, `chip lines [${font}]`).toBeGreaterThan(1);
    expect(await checkMyStatsLayout(page)).toEqual([]);
  }
});

test("mystats-closed: VALORANT closed at startup opens My Stats on its own (R71 ended)", async ({ page }) => {
  await openScene(page, "mystats-closed");
  expect(await route(page)).toEqual({ name: "mystats" });
  await expect(page.locator(".mystats")).toHaveCount(1);
  await expect(page.locator(".live")).toHaveCount(0);
  await expect(page.locator("header.bar .tab.on")).toHaveText("My Stats");
});

test("scenes with no data of their own still have a My Stats page (the full demo)", async ({ page }) => {
  await openScene(page, "ingame", { route: "mystats" });
  await expect(page.locator(".mystats .rank .tier")).toHaveText("Ascendant 1");
});
