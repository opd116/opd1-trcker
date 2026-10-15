import { test, expect } from '@playwright/test';
import { SCENES } from '../src/lib/demo/scenes';

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
      return {w:app.clientWidth,h:app.clientHeight,width:page.clientWidth,scrollWidth:page.scrollWidth,height:page.clientHeight,scrollHeight:page.scrollHeight,small:small.map(e => e.tagName)};
    });
    expect(geometry.w).toBe(1000); expect(geometry.h).toBe(800);
    expect(geometry.scrollWidth).toBeLessThanOrEqual(geometry.width);
    if (scene.route.name === 'live') expect(geometry.scrollHeight).toBeLessThanOrEqual(geometry.height);
    expect(geometry.small).toEqual([]);
    expect(errors).toEqual([]);
    if (shots.has(name)) {
      await page.evaluate(async () => {await Promise.race([Promise.all(Array.from(document.images).map(img => img.complete ? Promise.resolve() : new Promise(resolve => {img.onload = resolve; img.onerror = resolve;}))),new Promise(resolve => setTimeout(resolve,10000))]);});
      await page.screenshot({path:process.env.FRESH_REVIEW_DIR ? `${process.env.FRESH_REVIEW_DIR}/fresh-${name}.png` : test.info().outputPath(`fresh-${name}.png`)});
    }
  });
}

test('fresh: player, expanded history, match and tabs are navigable', async ({page}) => {
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
  await expect(frame.getByRole('button',{name:'Open Fahad',exact:true})).toBeVisible();
  await page.getByLabel('Layout',{exact:true}).selectOption('teams');
  await expect(frame.getByRole('button',{name:'Open player 1 on your team',exact:true})).toBeVisible();
  await expect(frame.locator('.player')).toHaveCount(10);
  await page.getByLabel('Preview',{exact:true}).selectOption('pregame');
  await expect(frame.locator('.player')).toHaveCount(5);
});
