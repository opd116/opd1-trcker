<script lang="ts">
  import { assetUrl } from '../../lib/assets';
  import { retryImg } from '../../lib/imgRetry';
  import type { Matchup } from '../../lib/livev2';
  let { mapId, mapName, matchup, pregame, progress, peak, ontoggle, onglance }: {
    mapId:string|null; mapName:string|null|undefined; matchup:Matchup[];
    pregame:{left:string|null;locked:number;total:number}|null;progress:{loaded:number;total:number}|null;
    peak:boolean;ontoggle:()=>void;onglance:()=>void;
  } = $props();
  const balance = $derived.by(() => {
    const values = matchup.map(side => side.hidden ? null : parseFloat(side.wr));
    if (values.length !== 2 || values.some(value => value === null || !Number.isFinite(value))) return null;
    const [a,b] = values as number[];
    return a + b > 0 ? a / (a + b) * 100 : 50;
  });
</script>
<div class="compact-match">
  <div class="map">{#if mapId}<img {@attach retryImg} src={assetUrl('mapList',mapId)} alt=""/>{/if}<b>{mapName ?? '—'}</b></div>
  <div class="balance">
    {#if pregame}<div class="pregame"><span>Agent select</span><b>{pregame.left ?? ''}</b><span>{pregame.locked}/{pregame.total} locked</span></div>{:else}<div class="team-rates">{#each matchup as side}<span>{side.wr} <small>{side.label === 'YOUR TEAM' ? 'Your team' : 'Enemy team'}</small></span>{/each}</div>{/if}
    {#if progress}<div class="load-track" role="progressbar" aria-label="Loading players" aria-valuenow={progress.loaded} aria-valuemin={0} aria-valuemax={progress.total}><i style:width={`${progress.loaded/Math.max(1,progress.total)*100}%`}></i></div>{:else if balance !== null}<div class="comparison" title="Team average win rates, not a prediction of match outcome."><span style:width={`${balance}%`}></span></div>{/if}
  </div>
  <div class="actions"><button class="glance" aria-label="Open glance mode" onclick={onglance}><svg viewBox="0 0 16 16" aria-hidden="true"><rect x="1.5" y="2" width="13" height="12" rx="1.5"/><path d="M10 2v12"/></svg>Glance</button><button class="rank-view" aria-pressed={peak} onclick={ontoggle}><span>Rank view</span><b>{peak ? 'Peak' : 'Current'}</b></button></div>
</div>
<style>
  .compact-match{height:58px;flex:none;display:flex;align-items:center;gap:16px;min-width:0}.map{position:relative;width:150px;height:56px;flex:none;overflow:hidden;border:1px solid var(--line);border-radius:8px;display:flex;align-items:center;padding:12px}.map img{position:absolute;inset:0;width:100%;height:100%;object-fit:cover;opacity:.55}.map b{position:relative;font:700 23px/1 var(--font);text-shadow:0 2px 8px #000}
  .balance{flex:1;min-width:0}.team-rates,.pregame{display:flex;justify-content:space-between;align-items:baseline;gap:8px;font:600 16px/20px var(--font)}small,.pregame span{font:500 11px/1.2 var(--font);color:var(--text-muted)}.team-rates>span:last-child{text-align:right}.comparison,.load-track{height:5px;background:#505052;margin-top:6px;border-radius:2px;overflow:hidden}.comparison span{display:block;width:50%;height:100%;background:#b4becd}.load-track i{display:block;height:100%;background:var(--accent)}
  .actions{width:114px;display:flex;flex-direction:column;gap:4px;flex:none}button{font:500 12px/1 var(--font);border:1px solid var(--line-control);background:#101010;border-radius:6px;min-height:25px;cursor:pointer;color:var(--text);display:flex;align-items:center;gap:5px;padding:4px 10px}.glance{justify-content:center;background:#22090d;border-color:var(--accent)}.glance svg{width:13px;height:13px;fill:none;stroke:currentColor;stroke-width:1.4}.rank-view{justify-content:space-between}.rank-view span{font-size:11px;color:var(--text-muted)}.rank-view[aria-pressed='true'] b{color:var(--accent-text)}button:hover{background:var(--row-hover)}
</style>
