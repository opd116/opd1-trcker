import { expect, test } from '@playwright/test';
import { openScene } from './harness';

for (const size of [{width:680,height:760},{width:700,height:860},{width:800,height:1000},{width:1280,height:800}]) {
  test(`compact roster fits ${size.width}x${size.height}`, async ({page}, info) => {
    await page.setViewportSize(size);
    await openScene(page,'ingame');
    const rows=page.locator('.compact-player[data-row]');
    await expect(rows).toHaveCount(10);
    const first=await rows.first().boundingBox();
    const last=await rows.last().boundingBox();
    expect(first!.height).toBeGreaterThanOrEqual(48);
    expect(last!.y+last!.height).toBeLessThanOrEqual(size.height-10);
    expect(size.height-(last!.y+last!.height)).toBeLessThanOrEqual(22);
    expect(await page.evaluate(()=>document.documentElement.scrollWidth)).toBe(size.width);
    await page.getByRole('button',{name:'Rank view'}).click();
    await expect(page.locator('.rank-view')).toHaveAttribute('aria-pressed','true');
    await expect(page.locator('.peak-act').first()).toBeVisible();
    await expect(page.locator('.peak-power')).toHaveCount(0);
    const transition=await page.locator('.peak-rank').first().evaluate(el=>getComputedStyle(el).transitionDuration);
    expect(transition).not.toBe('0s');
    await expect(page.locator('.peak-rank').first()).toHaveCSS('opacity','1');
    await expect(page.locator('.current-rank').first()).toHaveCSS('opacity','0');
    await page.screenshot({path:info.outputPath('peak.png')});
    await page.getByRole('button',{name:'Rank view'}).click();
    await rows.first().click();
    await expect(page.locator('.overlay')).toBeVisible();
  });
}

test('grouped navigation restores every page, settings has no options',async({page})=>{
  await page.setViewportSize({width:700,height:860});
  await openScene(page,'ingame');
  const nav=page.getByRole('navigation',{name:'Screens'});
  for (const item of ['History','Saved players','Competitive']) {
    await nav.getByRole('button',{name:/Records/}).click();
    await nav.getByRole('button',{name:item,exact:true}).click();
    await expect(page.locator('main.main')).not.toBeEmpty();
  }
  for (const item of ['Skins','ASCII']) {
    await nav.getByRole('button',{name:'Tools',exact:true}).click();
    await nav.getByRole('button',{name:item,exact:true}).click();
    await expect(page.locator('main.main')).not.toBeEmpty();
  }
  await nav.getByRole('button',{name:'Settings',exact:true}).click();
  await expect(page.locator('.settings-placeholder h1')).toHaveText('Settings');
  await expect(page.locator('main input,main select,main button')).toHaveCount(0);
  await nav.getByRole('button',{name:'Tools',exact:true}).click();
  await page.keyboard.press('Escape');
  await expect(nav.getByRole('button',{name:'Tools',exact:true})).toBeFocused();
  await expect(page.locator('.nav-menu')).toHaveCount(0);
});

test('reduced motion keeps rank view functional without movement',async({page})=>{
  await page.emulateMedia({reducedMotion:'reduce'});
  await page.setViewportSize({width:700,height:860});
  await openScene(page,'ingame');
  await page.getByRole('button',{name:'Rank view'}).click();
  expect(await page.locator('.peak-rank').first().evaluate(el=>getComputedStyle(el).transitionDuration)).toBe('0s');
  await expect(page.locator('.peak-rank').first()).toHaveCSS('opacity','1');
});

for (const width of [680,700]) {
  for (const screen of ['history','saved','comp','skins','ascii','about','settings']) {
    test(`${screen} fits compact width ${width}`,async({page},info)=>{
      await page.setViewportSize({width,height:860});
      const errors:string[]=[];
      page.on('pageerror',error=>errors.push(error.message));
      await openScene(page,'ingame',{screen});
      const overflow=await page.evaluate(()=>{
        const bad:string[]=[];
        for(const el of document.querySelectorAll<HTMLElement>('main.main *')) {
          const style=getComputedStyle(el);
          if ((style.overflowX==='auto'||style.overflowX==='scroll')&&el.scrollWidth>el.clientWidth+1) bad.push(`${el.tagName}.${el.className}`);
        }
        if(document.documentElement.scrollWidth>window.innerWidth)bad.push('document');
        return bad;
      });
      expect(overflow).toEqual([]);
      expect(errors).toEqual([]);
      await page.screenshot({path:info.outputPath(`${screen}.png`)});
    });
  }
}

for (const scene of ['ingame-longnames','ingame-partial','ingame-unranked','ingame-loading','ingame-rr-unknown','pregame']) {
  test(`compact data states: ${scene}`,async({page},info)=>{
    await page.setViewportSize({width:680,height:760});
    const errors:string[]=[];
    page.on('pageerror',error=>errors.push(error.message));
    await openScene(page,scene);
    const overflow=await page.locator('.tables').evaluate(el=>({x:el.scrollWidth-el.clientWidth,y:el.scrollHeight-el.clientHeight}));
    expect(overflow.x).toBeLessThanOrEqual(1);
    expect(overflow.y).toBeLessThanOrEqual(1);
    await page.getByRole('button',{name:'Rank view'}).click();
    await expect(page.locator('.peak-rank').first()).toHaveCSS('opacity','1');
    expect(errors).toEqual([]);
    await page.screenshot({path:info.outputPath(`${scene}.png`)});
  });
}

test('rank transition reverses safely on quick toggles',async({page})=>{
  await page.setViewportSize({width:700,height:860});
  await openScene(page,'ingame');
  const toggle=page.getByRole('button',{name:'Rank view'});
  await toggle.click();await toggle.click();await toggle.click();
  await expect(page.locator('.peak-rank').first()).toHaveCSS('opacity','1');
  await expect(page.locator('.current-rank').first()).toHaveCSS('opacity','0');
  await toggle.click();
  await expect(page.locator('.current-rank').first()).toHaveCSS('opacity','1');
  await expect(page.locator('.peak-rank').first()).toHaveCSS('opacity','0');
});

for (const width of [680,700]) {
  test(`profile and scoreboard fit ${width} and return correctly`,async({page},info)=>{
    await page.setViewportSize({width,height:860});
    await openScene(page,'ingame');
    const mara=page.locator('.compact-player[data-row]').nth(5);
    await mara.click();
    const panel=page.getByRole('dialog');
    await expect(panel).toBeVisible();
    await expect(panel.locator('.overview-note p')).toHaveText('Entry every round, dry-peeks A long');
    expect(await panel.evaluate(el=>el.scrollWidth-el.clientWidth)).toBeLessThanOrEqual(1);
    await page.screenshot({path:info.outputPath('profile.png')});
    await page.keyboard.press('Escape');
    await expect(mara).toBeFocused();
    const nav=page.getByRole('navigation',{name:'Screens'});
    await nav.getByRole('button',{name:/Records/}).click();
    await nav.getByRole('button',{name:'History',exact:true}).click();
    await page.locator('main [data-match-id]').first().click();
    await expect(page.locator('.match[aria-busy="false"]')).toBeVisible();
    const overflow=await page.locator('.match').evaluate(el=>el.scrollWidth-el.clientWidth);
    expect(overflow).toBeLessThanOrEqual(1);
    await page.screenshot({path:info.outputPath('scoreboard.png')});
    await page.locator('.match .back').click();
    await expect(page.locator('main [data-match-id]').first()).toBeVisible();
  });
}

test('live keeps the last data under the lost banner at the 680x760 minimum',async({page},info)=>{
  await page.setViewportSize({width:680,height:760});
  const errors:string[]=[];
  page.on('pageerror',error=>errors.push(error.message));
  await openScene(page,'ingame',{variant:'lost'});
  await expect(page.locator('.lost')).toBeVisible();
  await expect(page.locator('.lost .head')).toHaveText('Lost connection to the Riot Client');
  await expect(page.locator('.compact-player[data-row]')).toHaveCount(10);
  // All ten rows are actually on screen: the last row's bottom stays inside the 760px viewport.
  const rows=page.locator('.compact-player[data-row]');
  const lastRow=await rows.last().boundingBox();
  expect(lastRow!.y+lastRow!.height,'lost 680x760: last roster row bottom inside the viewport').toBeLessThanOrEqual(760);
  // The banner and its Retry control stay inside the window; nothing under main overflows sideways.
  const lost=await page.locator('.lost').boundingBox();
  const retry=await page.locator('.lost button').boundingBox();
  expect(lost!.x).toBeGreaterThanOrEqual(0);
  expect(lost!.x+lost!.width).toBeLessThanOrEqual(680);
  expect(retry!.x).toBeGreaterThanOrEqual(0);
  expect(retry!.x+retry!.width).toBeLessThanOrEqual(680);
  expect(retry!.y).toBeGreaterThanOrEqual(lost!.y);
  expect(retry!.y+retry!.height).toBeLessThanOrEqual(lost!.y+lost!.height);
  const overflow=await page.evaluate(()=>{
    const bad:string[]=[];
    for(const el of document.querySelectorAll<HTMLElement>('main.main *')) {
      const style=getComputedStyle(el);
      if ((style.overflowX==='auto'||style.overflowX==='scroll')&&el.scrollWidth>el.clientWidth+1) bad.push(`${el.tagName}.${el.className}`);
      if ((style.overflowY==='auto'||style.overflowY==='scroll')&&el.scrollHeight>el.clientHeight+1) bad.push(`${el.tagName}.${el.className}(y)`);
    }
    if(document.documentElement.scrollWidth>window.innerWidth)bad.push('document');
    if(document.documentElement.scrollHeight>window.innerHeight+1)bad.push('document(y)');
    return bad;
  });
  expect(overflow,'lost 680x760: no horizontal or vertical overflow').toEqual([]);
  expect(errors).toEqual([]);
  await page.screenshot({path:info.outputPath('lost-680x760.png')});
});

test('glance is the 420 square with its hover detail',async({page},info)=>{
  await page.setViewportSize({width:420,height:420});
  const errors:string[]=[];
  page.on('pageerror',error=>errors.push(error.message));
  await openScene(page,'ingame',{glance:'1'});
  const box=await page.locator('div.glance').boundingBox();
  expect(box!.width).toBe(420);
  expect(box!.height).toBe(420);
  expect(await page.evaluate(()=>document.documentElement.scrollWidth)).toBe(420);
  const rows=page.locator('.glance-body [data-row]');
  await expect(rows).toHaveCount(10);
  // Every row sits inside the visible scroller, before the hover and during it (the inspecting state compacts cells).
  const scroller=page.locator('.glance-body .scroll');
  async function expectRowsInsideScroller(when:string){
    const s=await scroller.boundingBox();
    const boxes=await rows.evaluateAll(els=>els.map(el=>{const r=el.getBoundingClientRect();return {t:r.top,b:r.bottom};}));
    for(const [i,r] of boxes.entries()){
      expect(r.t,`${when}: row ${i} top inside the scroller`).toBeGreaterThanOrEqual(s!.y-1);
      expect(r.b,`${when}: row ${i} bottom inside the scroller`).toBeLessThanOrEqual(s!.y+s!.height+1);
    }
    expect(await scroller.evaluate(el=>el.scrollHeight-el.clientHeight),`${when}: scroller does not scroll`).toBeLessThanOrEqual(1);
  }
  await expectRowsInsideScroller('before hover');
  await rows.first().hover();
  const detail=page.locator('#glance-player-detail');
  await expect(detail).toBeVisible();
  await expectRowsInsideScroller('during hover');
  const detailBox=await detail.boundingBox();
  expect(detailBox!.x).toBeGreaterThanOrEqual(box!.x);
  expect(detailBox!.x+detailBox!.width).toBeLessThanOrEqual(box!.x+box!.width);
  expect(detailBox!.y+detailBox!.height).toBeLessThanOrEqual(box!.y+box!.height);
  expect(errors).toEqual([]);
  await page.screenshot({path:info.outputPath('glance-420.png')});
});

for (const size of [{width:700,height:860},{width:680,height:760}]) {
test(`reference party curves and arrows keep distinct group colors without boxes at ${size.width}`,async({page},info)=>{
  await page.setViewportSize(size);
  const errors:string[]=[];
  page.on('pageerror',error=>errors.push(error.message));
  await openScene(page,'ingame-parties4');
  // R209: both evidence states have connected strokes; confidence stays in the tooltip/data.
  const solidLinks=page.locator('.compact-team').first().locator('.party-link');
  const otherLinks=page.locator('.compact-team').last().locator('.party-link');
  await expect(solidLinks.first()).toHaveCSS('border-left-style','solid');
  await expect(otherLinks.first()).toHaveCSS('border-left-style','solid');
  // R208's supplied image supersedes the mistaken no-arrow interpretation of R207.
  for(const [links,style] of [[solidLinks,'solid'],[otherLinks,'solid']] as const){
    const paint=await links.first().evaluate(node=>({
      arrow:getComputedStyle(node,'::before').content,
      horizontal:parseFloat(getComputedStyle(node).borderTopWidth),
      style:getComputedStyle(node).borderTopStyle,
      radius:parseFloat(getComputedStyle(node).borderTopLeftRadius),
      arrowStyle:getComputedStyle(node,'::before').borderTopStyle,
      arrowWidth:parseFloat(getComputedStyle(node,'::before').width),
    }));
    expect(paint.arrow).toBe('""');
    expect(paint.horizontal).toBeGreaterThan(0);
    expect(paint.style).toBe(style);
    expect(paint.radius).toBe(8);
    expect(paint.arrowStyle).toBe('solid');
    expect(paint.arrowWidth).toBe(6);
  }
  const geometry=await page.locator('.party-link').evaluateAll(nodes=>nodes.map(node=>{
    const css=getComputedStyle(node), arrow=getComputedStyle(node,'::before');
    const portrait=node.closest('.compact-player')!.querySelector('.portrait')!.getBoundingClientRect();
    return {
      right:parseFloat(css.borderRightWidth),top:parseFloat(css.borderTopWidth),bottom:parseFloat(css.borderBottomWidth),
      starts:!node.classList.contains('to-top'),ends:!node.classList.contains('to-bottom'),
      endpoint:Math.abs(node.getBoundingClientRect().right-portrait.left),
      arrowRight:parseFloat(arrow.borderRightWidth),arrowBottom:parseFloat(arrow.borderBottomWidth),arrowLeft:parseFloat(arrow.borderLeftWidth),
    };
  }));
  for(const item of geometry){
    expect(item.right).toBe(0); // No phantom fourth side, including likely/dashed links.
    expect(item.top).toBe(item.starts?2:0);
    expect(item.bottom).toBe(item.ends?2:0);
    expect(item.endpoint).toBeLessThanOrEqual(1);
    expect(item.arrowRight).toBe(2);
    expect(item.arrowBottom).toBe(0);
    expect(item.arrowLeft).toBe(0);
  }
  const colors=await page.locator('.party-link').evaluateAll(nodes=>nodes.map(node=>getComputedStyle(node).borderLeftColor));
  expect(new Set(colors).size).toBe(4);
  const enemyColors=await page.locator('.compact-team').last().locator('.party-link').evaluateAll(nodes=>nodes.map(node=>getComputedStyle(node).borderLeftColor));
  expect(enemyColors).toHaveLength(4);
  expect(enemyColors[0]).toBe(enemyColors[1]);
  expect(enemyColors[2]).toBe(enemyColors[3]);
  expect(enemyColors[0]).not.toBe(enemyColors[2]);
  expect(errors).toEqual([]);
  await page.screenshot({path:info.outputPath('parties-restored.png')});
});
}

test('current rank keeps its numeric peak-gap badge beside the tier icon',async({page},info)=>{
  await page.setViewportSize({width:700,height:860});
  const errors:string[]=[];
  page.on('pageerror',error=>errors.push(error.message));
  await openScene(page,'ingame');
  // Current view: the gap badge is visible beside the tier icon; peak layer is behind.
  await expect(page.locator('.current-rank').first()).toHaveCSS('opacity','1');
  await expect(page.locator('.peak-rank').first()).toHaveCSS('opacity','0');
  // The badge is numeric and matches the fixture's own deltas (peak tier − current tier): ally Fahad 24−21=3,
  // Sable 22−20=2, Vesper 21−19=2, Drake 22−21=1, Nova 22−22 (none); enemy Mara 25−24=1, Kilo 24−23=1,
  // wisp/Rook equal (none), Cypher 22−21=1. Seven badges, in DOM order.
  const badges=await page.locator('.compact-player[data-row] .peak-gap').evaluateAll(els=>els.map(e=>(e.textContent??'').trim()));
  expect(badges,'peak-gap badges match the fixture deltas').toEqual(['3','2','2','1','1','1','1']);
  for(const text of badges) expect(text,'peak-gap badge is numeric').toMatch(/^\d+$/);
  await expect(page.locator('.compact-player[data-row]').first().locator('.peak-gap')).toBeVisible();
  expect(errors).toEqual([]);
  await page.screenshot({path:info.outputPath('peak-gap-badge.png')});
});

test('Peak shows rank name and actual act age with a downward gap toward Current, not division cells',async({page})=>{
  await page.setViewportSize({width:680,height:760});
  await openScene(page,'ingame');
  const up=await page.locator('.current-rank .peak-gap').allTextContents();
  await page.getByRole('button',{name:'Rank view'}).click();
  await expect(page.locator('.peak-rank').first()).toHaveCSS('opacity','1');
  await expect(page.locator('.peak-details>b').first()).toHaveText('Immortal 1');
  await expect(page.locator('.peak-act').first()).toHaveText(/^(?:E\d+|V\d+) A\d+ · \d+(?:m|h|d|w|mo|y) ago$/);
  await expect(page.locator('.peak-power')).toHaveCount(0);
  expect(await page.locator('.peak-rank .current-gap').allTextContents()).toEqual(up);
  await expect(page.locator('.current-gap path').first()).toHaveAttribute('d','m2 8 4 4 4-4M6 12V2');
  await expect(page.locator('.peak-rank').first()).toHaveAttribute('aria-label',/Current Ascendant 1 · 3 divisions below peak/);
});

test('an enemy trio and duo keep separate colors and restored connectors for all five players',async({page},info)=>{
  await page.setViewportSize({width:680,height:760});
  await openScene(page,'ingame-trio-duo');
  const enemy=page.locator('.compact-team').last();
  const links=enemy.locator('.party-link');
  await expect(links).toHaveCount(5);
  const paint=await links.evaluateAll(nodes=>nodes.map(node=>({color:getComputedStyle(node).borderLeftColor,style:getComputedStyle(node).borderLeftStyle,arrow:getComputedStyle(node,'::before').content})));
  expect(paint.slice(0,3).every(item=>item.color===paint[0]!.color&&item.style==='solid')).toBe(true);
  expect(paint.slice(3).every(item=>item.color===paint[3]!.color&&item.style==='solid')).toBe(true);
  expect(paint[0]!.color).not.toBe(paint[3]!.color);
  expect(paint.every(item=>item.arrow==='""')).toBe(true);
  // The middle member gets a branch from the same continuous spine, not another rounded box.
  const middle=links.nth(1);
  await expect(middle).toHaveClass(/to-top/);
  await expect(middle).toHaveClass(/to-bottom/);
  const middlePaint=await middle.evaluate(node=>({
    top:getComputedStyle(node).borderTopWidth,bottom:getComputedStyle(node).borderBottomWidth,right:getComputedStyle(node).borderRightWidth,
    branch:getComputedStyle(node,'::after').borderTopWidth,arrowTop:getComputedStyle(node,'::before').top,
    height:node.getBoundingClientRect().height,
  }));
  expect([middlePaint.top,middlePaint.bottom,middlePaint.right]).toEqual(['0px','0px','0px']);
  expect(middlePaint.branch).toBe('2px');
  expect(parseFloat(middlePaint.arrowTop)).toBeCloseTo(middlePaint.height/2,0);
  await page.screenshot({path:info.outputPath('enemy-trio-duo.png')});
});

for(const size of [{width:700,height:860},{width:680,height:760}]) {
  test(`profile sections prioritize encounters and teammates in compact panels at ${size.width}`,async({page},info)=>{
    await page.setViewportSize(size);
    await openScene(page,'details-more');
    const dialog=page.getByRole('dialog');
    await expect(dialog).toHaveAttribute('aria-busy','false');
    expect(await dialog.locator('.profile-content>section').evaluateAll(nodes=>nodes.map(node=>node.getAttribute('aria-label')))).toEqual(['Encounters','Top teammates','Recent matches','Map performance']);
    const enc=dialog.getByRole('region',{name:'Encounters',exact:true});
    const mates=dialog.getByRole('region',{name:'Top teammates',exact:true});
    const recent=dialog.getByRole('region',{name:'Recent matches',exact:true});
    const maps=dialog.getByRole('region',{name:'Map performance',exact:true});
    const eb=await enc.boundingBox(), mb=await mates.boundingBox(), rb=await recent.boundingBox(), mapsb=await maps.boundingBox();

    // Column placement WITH encounters:
    // Left: Encounters -> Recent matches; Right: Top teammates -> Map performance
    expect(eb!.x).toBeLessThan(mb!.x);
    expect(rb!.x).toBeLessThan(mb!.x);
    expect(mb!.x).toBeGreaterThan(eb!.x);
    expect(mapsb!.x).toBeGreaterThan(eb!.x);
    expect(rb!.y).toBeGreaterThan(eb!.y);
    expect(mapsb!.y).toBeGreaterThan(mb!.y);

    // Original regression: Map performance top minus Top teammates bottom is 12–20px
    const gap = mapsb!.y - (mb!.y + mb!.height);
    expect(gap).toBeGreaterThanOrEqual(12);
    expect(gap).toBeLessThanOrEqual(20);

    expect(Math.abs(rb!.width-mb!.width)).toBeLessThan(1);
    expect(eb!.width).toBeLessThan(size.width*.55);
    await expect(enc.getByRole('button',{name:/Open scoreboard$/})).toHaveCount(3);
    await expect(recent.locator('.row')).toHaveCount(6);

    // No nested scrolling on card elements
    const cardOverflow = await dialog.locator('.card').evaluateAll(cards => cards.map(c => c.scrollHeight - c.clientHeight));
    expect(cardOverflow.every(diff => diff <= 1)).toBe(true);

    await page.screenshot({path:info.outputPath('profile-sections-top.png')});
    await recent.locator('.row').last().scrollIntoViewIfNeeded();
    await expect(recent.locator('.row').last()).toBeInViewport();
    const over=await dialog.locator('.body').evaluate(el=>el.scrollWidth-el.clientWidth);
    expect(over).toBe(0);
    await page.screenshot({path:info.outputPath('profile-sections-recent.png')});
  });

  test(`without encounters teammates move to the first panel at ${size.width}`,async({page},info)=>{
    await page.setViewportSize(size);
    await openScene(page,'details-hidden');
    const dialog=page.getByRole('dialog');
    await expect(dialog).toHaveAttribute('aria-busy','false');
    await expect(dialog.getByRole('region',{name:'Encounters',exact:true})).toHaveCount(0);
    expect(await dialog.locator('.profile-content>section').evaluateAll(nodes=>nodes.map(node=>node.getAttribute('aria-label')))).toEqual(['Top teammates','Recent matches','Map performance']);
    const mates=dialog.getByRole('region',{name:'Top teammates',exact:true});
    const recent=dialog.getByRole('region',{name:'Recent matches',exact:true});
    const maps=dialog.getByRole('region',{name:'Map performance',exact:true});
    const mb=await mates.boundingBox(), rb=await recent.boundingBox(), mapsb=await maps.boundingBox();

    // Column placement WITHOUT encounters:
    // Left: Top teammates -> Map performance; Right: Recent matches
    expect(mb!.x).toBeLessThan(rb!.x);
    expect(mapsb!.x).toBeLessThan(rb!.x);
    expect(rb!.x).toBeGreaterThan(mb!.x);
    expect(mapsb!.y).toBeGreaterThan(mb!.y);

    // Original regression: Map performance top minus Top teammates bottom is 12–20px
    const gap = mapsb!.y - (mb!.y + mb!.height);
    expect(gap).toBeGreaterThanOrEqual(12);
    expect(gap).toBeLessThanOrEqual(20);

    // No nested scrolling
    const cardOverflow = await dialog.locator('.card').evaluateAll(cards => cards.map(c => c.scrollHeight - c.clientHeight));
    expect(cardOverflow.every(diff => diff <= 1)).toBe(true);

    await page.screenshot({path:info.outputPath('profile-without-encounters.png')});
  });
}

test('profile preserves focus across resize breakpoint and maintains reading order',async({page})=>{
  await page.setViewportSize({width:700,height:860});
  await openScene(page,'details-more');
  const dialog=page.getByRole('dialog');
  await expect(dialog).toHaveAttribute('aria-busy','false');

  // Focus an interactive button inside the drawer
  const saveBtn=dialog.locator('button.save');
  await saveBtn.focus();
  await expect(saveBtn).toBeFocused();

  // Resize across the 600px breakpoint to 500px
  await page.setViewportSize({width:500,height:860});
  await expect(saveBtn).toBeFocused();

  // Verify single-column DOM order remains intact
  expect(await dialog.locator('.profile-content>section').evaluateAll(nodes=>nodes.map(node=>node.getAttribute('aria-label')))).toEqual(['Encounters','Top teammates','Recent matches','Map performance']);

  // Resize back to 700px and verify focus remains
  await page.setViewportSize({width:700,height:860});
  await expect(saveBtn).toBeFocused();
});

test('typical Live stats are white after broadening the neutral range',async({page})=>{
  await openScene(page,'ingame');
  const sable=page.locator('.compact-player[data-row]').nth(1);
  for(const selector of ['.win .reading','.combat']) {
    const colors=await sable.locator(selector).evaluateAll(nodes=>nodes.map(node=>getComputedStyle(node).color));
    expect(colors.every(color=>color==='rgb(255, 255, 255)')).toBe(true);
  }
});

test('neither normal nor Glance shell exposes the removed always-on-top feature',async({page})=>{
  await page.setViewportSize({width:700,height:860});
  await openScene(page,'ingame');
  await expect(page.locator('.bar .pin')).toHaveCount(0);
  await expect(page.getByRole('button',{name:'Keep on top during matches'})).toHaveCount(0);
  await page.getByRole('button',{name:'Open glance mode',exact:true}).click();
  await page.setViewportSize({width:420,height:420});
  await expect(page.locator('div.glance')).toBeVisible();
  await expect(page.locator('.bar .pin')).toHaveCount(0);
  await expect(page.locator('.glance-body [data-row]')).toHaveCount(10);
});
