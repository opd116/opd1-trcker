<script lang="ts">
  import type { RowView } from '../../lib/livev2';
  import type { PlayerCard } from '../../lib/types';
  import CompactPlayerRow from './CompactPlayerRow.svelte';
  let { label, color, rows, cards, placeholders, peak = false, onopen }: {
    label:string; color:string; rows:RowView[]; cards:PlayerCard[]; placeholders:number; peak?:boolean;
    onopen:(puuid:string,el:HTMLElement)=>void;
  } = $props();
</script>
<section class="compact-team" aria-label={label === 'YOUR TEAM' ? 'Your team' : 'Enemy team'}>
  <div class="team-head"><span></span><b style:color>{label === 'YOUR TEAM' ? 'Your team' : 'Enemy team'}</b><span class="rank-heading">{peak ? 'Peak rank' : 'Rank & RR'}</span><span>Win</span><span>K/D</span><span>HS%</span><span></span></div>
  {#each rows as row,i (row.puuid)}<CompactPlayerRow {row} card={cards[i]} {peak} alternate={i%2===0} {onopen}/>{/each}
  {#each {length:placeholders} as _,i}<CompactPlayerRow row={null} {peak} alternate={i%2===0} {onopen}/>{/each}
</section>
<style>
  .compact-team{flex:none;min-width:0;overflow:hidden;border-radius:6px;background:var(--card)}
  .team-head{height:26px;display:grid;grid-template-columns:var(--roster-columns);column-gap:var(--roster-gap);align-items:center;padding-right:12px;background:#090809;border-bottom:1px solid var(--line);font:600 14px/18px var(--font);color:var(--text-muted);text-align:center}
  .team-head>b{text-align:left;font-size:12px}.rank-heading{text-align:left;padding-left:38px;font-size:12px}
</style>
