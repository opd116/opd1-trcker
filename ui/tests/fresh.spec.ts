import { test, expect } from '@playwright/test';
import { SCENES } from '../src/lib/demo/scenes';
import { routeMedia } from './harness';

test.beforeEach(async ({page}) => {await routeMedia(page);});

const shots = new Set(['ingame','pregame','lobby2','ingame-longnames','ingame-partial','ingame-remembered','details','match','mystats']);

for (const [name, scene] of Object.entries(SCENES)) {
  test(`fresh ${name}: fits window and renders without runtime errors`, async ({ page }) => {
    const errors: string[] = [];
    page.on('pageerror', e => errors.push(e.message));
    await page.goto(`/design-screen.html?scene=${name}`);
    await expect(page.locator('.fresh-app')).toBeVisible();
    await page.evaluate(() => document.fonts.ready);
    const geometry = await page.locator('.fresh-app').evaluate(app => {
      const page = app.querySelector('.page')!;
      const small = Array.from(app.querySelectorAll('*')).filter(e => e.textContent?.trim() && e.childElementCount === 0 && Number.parseFloat(getComputedStyle(e).fontSize) < 12);
      return {w:app.clientWidth,h:app.clientHeight,width:page.clientWidth,scrollWidth:page.scrollWidth,height:page.clientHeight,scrollHeight:page.scrollHeight,scrollable:page.classList.contains('scroll'),small:small.map(e => e.tagName)};
    });
    expect(geometry.w).toBe(1000); expect(geometry.h).toBe(800);
    expect(geometry.scrollWidth).toBeLessThanOrEqual(geometry.width);
    if (!geometry.scrollable) expect(geometry.scrollHeight).toBeLessThanOrEqual(geometry.height);
    if (scene.route.name === 'match') expect(geometry.scrollHeight).toBeLessThanOrEqual(geometry.height);
    expect(geometry.small).toEqual([]);
    const rowOverflow = await page.locator('.roster button.columns.player').evaluateAll(rows => rows.flatMap(row => {
      const bounds = row.getBoundingClientRect();
      const pageBounds = row.closest('.page')!.getBoundingClientRect();
      const cells = Array.from(row.children).map(cell => cell.getBoundingClientRect());
      const issues: string[] = [];
      const id = row.getAttribute('data-puuid');
      if (bounds.bottom > pageBounds.bottom + 1) issues.push(`${id}: row below page`);
      cells.forEach((cell, i) => {
        if (cell.top < bounds.top - 1 || cell.bottom > bounds.bottom + 1) issues.push(`${id}: cell ${i} outside row height`);
        if (cell.left < bounds.left - 1 || cell.right > bounds.right + 1) issues.push(`${id}: cell ${i} outside row width`);
        if (i > 0 && cell.left < cells[i - 1]!.right - 1) issues.push(`${id}: cells ${i - 1} and ${i} overlap`);
      });
      const identity = row.querySelector('.identity')!.getBoundingClientRect();
      const relationships = row.querySelector('.relationships')!.getBoundingClientRect();
      if (identity.right > relationships.left + 1) issues.push(`${id}: identity overlaps indicators`);
      const tag = row.querySelector('.tag');
      if (tag && tag.scrollWidth > tag.clientWidth) issues.push(`${id}: tag is clipped`);
      return issues;
    }));
    expect(rowOverflow).toEqual([]);
    expect(errors).toEqual([]);
    if (shots.has(name)) {
      await page.evaluate(async () => {await Promise.race([Promise.all(Array.from(document.images).map(img => img.complete ? Promise.resolve() : new Promise(resolve => {img.onload = resolve; img.onerror = resolve;}))),new Promise(resolve => setTimeout(resolve,10000))]);});
      await page.screenshot({path:process.env.FRESH_REVIEW_DIR ? `${process.env.FRESH_REVIEW_DIR}/fresh-${name}.png` : test.info().outputPath(`fresh-${name}.png`)});
    }
  });
}

test('fresh: player, expanded history, match and tabs are navigable', async ({page}) => {
  await page.goto('/design-screen.html?scene=match');
  await page.getByRole('button',{name:'Back',exact:true}).click();
  await expect(page.getByRole('region',{name:'Player overview'})).toBeVisible();
  await page.goto('/design-screen.html?scene=ingame');
  await page.locator(`[data-puuid="${SCENES.ingame.live!.players[0]!.puuid}"]`).click();
  await expect(page.getByRole('region',{name:'Player overview'})).toBeVisible();
  const more = page.getByRole('button',{name:/Show more/i});
  if (await more.count()) await more.click();
  await page.locator('.match-row').first().click();
  await expect(page.getByRole('region',{name:'Scoreboard',exact:true})).toBeVisible();
  await page.getByRole('button',{name:'Back',exact:true}).click();
  await expect(page.getByRole('region',{name:'Player overview'})).toBeVisible();
  await expect(page.getByRole('button',{name:/Show more/i})).toHaveCount(0);
  await page.getByRole('button',{name:'My Stats',exact:true}).click();
  await expect(page.getByRole('heading',{name:'Your progress'})).toBeVisible();
  await page.getByRole('button',{name:/Open Lotus, win/}).click();
  await expect(page.getByRole('region',{name:'Scoreboard',exact:true})).toBeVisible();
  await page.getByRole('button',{name:'Back',exact:true}).click();
  await expect(page.getByRole('heading',{name:'Your progress'})).toBeVisible();
});

test('fresh: hidden identities follow fixture policy in live and details', async ({page}) => {
  const scene = SCENES.ingame;
  await page.goto('/design-screen.html?scene=ingame');
  let checked = 0;
  for (const player of scene.live!.players) {
    if (player.name?.kind === 'hiddenAgent' || player.name?.kind === 'hiddenPlayer') {
      const button = page.locator(`[data-puuid="${player.puuid}"]`);
      await expect(button.locator('.identity')).toHaveClass(/hidden/);
      await expect(button.locator('.tag')).toHaveCount(0);
      checked++;
      await button.click();
      await expect(page.locator('.hero .identity .tag')).toHaveCount(0);
      await page.getByRole('button',{name:'Back',exact:true}).click();
    }
  }
  expect(checked).toBeGreaterThan(0);
  await page.goto('/design-screen.html?scene=ingame-remembered');
  await expect(page.locator('.remembered').first()).toBeVisible();
});

test('fresh: workbench switches the independent roster and team comparison', async ({page}) => {
  await page.goto('/design.html?scene=ingame');
  const frame = page.frameLocator('iframe');
  await expect(frame.locator(`[data-puuid="${SCENES.ingame.live!.players[0]!.puuid}"]`)).toBeVisible();
  await page.getByLabel('Layout',{exact:true}).selectOption('teams');
  await expect(frame.getByRole('button',{name:'Open player 1 on your team',exact:true})).toBeVisible();
  await expect(frame.locator('.player')).toHaveCount(10);
  await page.getByLabel('Preview',{exact:true}).selectOption('pregame');
  await expect(frame.locator('.player')).toHaveCount(5);
});

test('fresh: peak rank is available on hover and keyboard focus without leaving the roster', async ({page}) => {
  await page.goto('/design-screen.html?scene=ingame');
  const player = SCENES.ingame.live!.players[0]!;
  const row = page.locator(`[data-puuid="${player.puuid}"]`);
  const tooltip = row.getByRole('tooltip', {includeHidden:true});
  await expect(tooltip).toBeHidden();
  await row.locator('.peak-badge').hover();
  await expect(tooltip).toBeVisible();
  await expect(tooltip).toHaveText(/^[A-Z]{2}(?: [123])? · .+ · \d+mo$/);
  await expect(tooltip).toContainText(player.peak!.actLabel);
  await expect(row).toHaveAttribute('aria-describedby', `peak-${player.puuid}`);
  await expect(page.locator('.roster button.player')).toHaveCount(10);
  await page.mouse.move(5, 5);
  await expect(tooltip).toBeHidden();
  await row.focus();
  await expect(tooltip).toBeVisible();
  await page.keyboard.press('Enter');
  await expect(page.getByRole('region',{name:'Player overview'})).toBeVisible();
});

test('fresh: bottom-row peak tooltip stays inside the fixed window', async ({page}) => {
  await page.goto('/design-screen.html?scene=ingame');
  const last = page.locator('.roster button.player').last();
  await last.locator('.peak-badge').hover();
  const tooltip = last.getByRole('tooltip');
  await expect(tooltip).toBeVisible();
  const bounds = await tooltip.boundingBox();
  expect(bounds!.y).toBeGreaterThanOrEqual(64);
  expect(bounds!.y + bounds!.height).toBeLessThanOrEqual(800);
});

test('fresh: party symbols share group identity and colors, distinguish confidence, and exclude self party', async ({page}) => {
  await page.goto('/design-screen.html?scene=ingame-parties4');
  const groupColors = new Map<number, string>();
  for (const player of SCENES['ingame-parties4'].live!.players) {
    const row = page.locator(`[data-puuid="${player.puuid}"]`);
    const chip = row.locator('.party');
    if (!player.party || player.isSelf || player.isSelfParty) {
      await expect(chip).toHaveCount(0);
      continue;
    }
    await expect(chip).toHaveText(['●','◆','▲','■'][player.party.group % 4]!);
    await expect(chip).toHaveAttribute('aria-label', `Party ${player.party.group + 1} · ${player.party.confirmed ? 'confirmed' : 'likely'}`);
    const style = await chip.evaluate(el => ({color:getComputedStyle(el).color,border:getComputedStyle(el).borderBottomStyle}));
    expect(style.border).toBe(player.party.confirmed ? 'none' : 'dashed');
    if (groupColors.has(player.party.group)) expect(style.color).toBe(groupColors.get(player.party.group));
    groupColors.set(player.party.group, style.color);
  }
  expect(new Set(groupColors.values()).size).toBe(4);
});

test('fresh: encounter history reserves its slot and preserves long tags with three-digit counts', async ({page}) => {
  await page.goto('/design-screen.html?scene=ingame-longnames');
  for (const player of SCENES['ingame-longnames'].live!.players) {
    const row = page.locator(`[data-puuid="${player.puuid}"]`);
    const badge = row.locator('.encounter');
    const expectedCount = !player.isSelf && !player.isSelfParty && player.encounters ? player.encounters.with + player.encounters.against : 0;
    await expect(row.locator('.encounter-slot')).toBeAttached();
    if (!expectedCount) { await expect(badge).toHaveCount(0); continue; }
    await expect(badge.locator('svg')).toBeVisible();
    await expect(badge.locator('.encounter-count')).toHaveText(String(expectedCount));
    await expect(badge).toHaveAttribute('title', `${player.encounters!.with} with · ${player.encounters!.against} against · Competitive`);
    expect(await badge.evaluate(el => el.scrollWidth <= el.clientWidth)).toBe(true);
  }
});

test('fresh: recent results carry compact text and distinct colors in chronological order with empty slots on the left', async ({page}) => {
  await page.goto('/design-screen.html?scene=ingame-unranked');
  for (const player of SCENES['ingame-unranked'].live!.players) {
    const results = page.locator(`[data-puuid="${player.puuid}"] .result`);
    const chronological = [...(player.last5 ?? [])].reverse();
    const expected = [...Array.from({length:5 - chronological.length}, () => ''), ...chronological.map(match => match.result[0]!.toUpperCase())];
    await expect(results).toHaveText(expected);
    if (chronological.length) await expect(results.last()).toHaveAttribute('aria-label', /Most recent$/);
  }
  const shapes = await Promise.all(['win','loss','draw'].map(async result => page.locator(`.result.${result}`).first().evaluate(el => getComputedStyle(el).color)));
  expect(new Set(shapes).size).toBe(3);
});
