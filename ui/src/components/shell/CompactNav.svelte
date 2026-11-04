<script lang="ts">
  import { savedInMatch } from '../../lib/appdata.svelte';
  import { go, router, type Screen } from '../../lib/router.svelte';
  let menu = $state<'records' | 'tools' | null>(null);
  let records: HTMLButtonElement;
  let tools: HTMLButtonElement;
  function select(screen: Screen) { menu = null; go(screen); }
  function dismiss(e: KeyboardEvent) {
    if (e.key === 'Escape' && menu) { e.preventDefault(); const old = menu; menu = null; (old === 'records' ? records : tools)?.focus(); }
  }
</script>

<svelte:window onkeydown={dismiss} onclick={(e) => { if (!(e.target as Element).closest('.compact-nav')) menu = null; }} />
<nav class="compact-nav" aria-label="Screens">
  <button class:active={router.screen === 'live'} aria-current={router.screen === 'live' ? 'page' : undefined} onclick={() => select('live')}>Live</button>
  <div class="nav-spacer"></div>
  <div class="nav-group">
    <button bind:this={records} class:active={['history','saved','comp'].includes(router.screen)} aria-expanded={menu === 'records'} aria-controls="records-pages" onclick={() => menu = menu === 'records' ? null : 'records'}>Records <svg viewBox="0 0 12 12" aria-hidden="true"><path d="m3 4.5 3 3 3-3"/></svg>{#if $savedInMatch > 0}<span class="nav-count">{$savedInMatch}</span>{/if}</button>
    {#if menu === 'records'}<div class="nav-menu" id="records-pages">
      {#each [{id:'history', label:'History'},{id:'saved', label:'Saved players'},{id:'comp', label:'Competitive'}] as item}<button aria-current={router.screen === item.id ? 'page' : undefined} onclick={() => select(item.id as Screen)}>{item.label}</button>{/each}
    </div>{/if}
  </div>
  <div class="nav-group">
    <button bind:this={tools} class:active={['skins','ascii'].includes(router.screen)} aria-expanded={menu === 'tools'} aria-controls="tools-pages" onclick={() => menu = menu === 'tools' ? null : 'tools'}>Tools <svg viewBox="0 0 12 12" aria-hidden="true"><path d="m3 4.5 3 3 3-3"/></svg></button>
    {#if menu === 'tools'}<div class="nav-menu" id="tools-pages"><button aria-current={router.screen === 'skins' ? 'page' : undefined} onclick={() => select('skins')}>Skins</button><button aria-current={router.screen === 'ascii' ? 'page' : undefined} onclick={() => select('ascii')}>ASCII</button></div>{/if}
  </div>
  <button class:active={router.screen === 'settings'} aria-current={router.screen === 'settings' ? 'page' : undefined} onclick={() => select('settings')}>Settings</button>
  <button class="nav-about" class:active={router.screen === 'about'} aria-label="About" aria-current={router.screen === 'about' ? 'page' : undefined} onclick={() => select('about')}><svg viewBox="0 0 20 20" aria-hidden="true"><circle cx="10" cy="10" r="7"/><path d="M10 9v5m0-8v.2"/></svg></button>
</nav>

<style>
  .compact-nav { height:36px; flex:none; display:flex; align-items:stretch; gap:22px; padding:0 16px; background:var(--bg); border-bottom:1px solid var(--line-chrome); position:relative; z-index:30; }
  button { border:0; border-radius:0; background:transparent; color:var(--text-muted); font:600 14px/1 var(--font); padding:0; cursor:pointer; display:flex; align-items:center; gap:5px; position:relative; }
  button:hover,button.active { color:var(--text); }
  button.active:after { content:''; position:absolute; height:2px; background:var(--accent); bottom:0; left:0; right:0; }
  svg { width:12px; height:12px; fill:none; stroke:currentColor; stroke-width:1.5; }
  .nav-spacer { flex:1; }
  .nav-group { position:relative; display:flex; }
  .nav-menu { position:absolute; top:calc(100% + 5px); right:0; width:168px; padding:5px; border:1px solid var(--line-control); border-radius:6px; background:var(--tip-bg); box-shadow:0 8px 24px #0007; }
  .nav-menu button { width:100%; min-height:36px; padding:8px 10px; border-radius:3px; text-align:left; }
  .nav-menu button:hover,.nav-menu button[aria-current] { background:var(--row-hover); color:var(--text); }
  .nav-count { font-size:11px; color:var(--accent-text); }
  .nav-about svg { width:17px; height:17px; }
</style>
