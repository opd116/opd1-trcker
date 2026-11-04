<script lang="ts">
  import { assetUrl } from '../../lib/assets';
  import { retryImg } from '../../lib/imgRetry';
  import type { RowView } from '../../lib/livev2';
  import { peakRank } from '../../lib/peakRank';
  import { meta } from '../../lib/stores';
  import { tip } from '../../lib/tooltip.svelte';
  import type { PlayerCard } from '../../lib/types';
  import Shimmer from './Shimmer.svelte';
  let { row, card, peak = false, alternate = false, onopen }: {
    row: RowView | null; card?: PlayerCard; peak?: boolean; alternate?: boolean;
    onopen: (puuid: string, el: HTMLElement) => void;
  } = $props();
  const emblem = $derived(peakRank(card?.peak, $meta));
  const peakActive = $derived(peak);
  const currentGapTip = $derived(row?.peak ? `Current ${row.rank.name} · ${row.peak.delta} ${row.peak.delta === 1 ? 'division' : 'divisions'} below peak` : '');
  const encounters = $derived(row?.met && card?.encounters ? card.encounters.with + card.encounters.against : 0);
  function key(e: KeyboardEvent) {
    if (row && (e.key === 'Enter' || e.key === ' ')) { e.preventDefault(); onopen(row.puuid, e.currentTarget as HTMLElement); }
  }
</script>

{#snippet metricText(text: string)}
  {#if text.endsWith('%')}{text.slice(0,-1)}<small class="percent-unit">%</small>{:else}{text}{/if}
{/snippet}

<div class="compact-player" class:alternate class:self={row?.isSelf} class:selected={row?.selected}
  role="button" tabindex={row ? 0 : -1} aria-label={row ? `${row.aria}${peak ? `. Peak ${emblem?.name ?? (card?.peak === undefined ? 'loading' : 'unavailable')}` : ''}` : 'Loading player'} data-row={row ? '1' : undefined}
  onclick={(e) => { if (row) onopen(row.puuid, e.currentTarget); }} onkeydown={key}>
  <div class="party-gutter" style:--party={row?.party?.color}>
    {#if row?.party}<span class="party-link" class:to-top={row.party.top} class:to-bottom={row.party.bottom} use:tip={row.party.tip}></span>{/if}
  </div>
  <div class="identity">
    <div class="portrait" class:party={!!row?.party} style:--party={row?.party?.color}>
      {#if row?.agentId}<img {@attach retryImg} src={assetUrl('agent', row.agentId)} alt={row.agent ?? ''}/>{/if}
    </div>
    <div class="names">
      <div class="name-line">
        {#if !row || row.name.style === 'loading'}<Shimmer w={90} h={12}/>{:else}
          {#if row.name.style === 'remembered'}<svg class="remembered-icon" viewBox="0 0 18 18" fill="none" stroke="currentColor" aria-label="Remembered hidden name"><path d="m2 2 14 14M7 4a8 8 0 0 1 9 5 10 10 0 0 1-2 3M4 5a10 10 0 0 0-2 4s2 5 7 5a8 8 0 0 0 3-.6"/></svg>{/if}
          <strong class:hidden={row.name.style === 'hidden'} class:remembered={row.name.style === 'remembered'} dir="auto" use:tip={row.name.tip} data-ellipsis>{row.name.text}</strong>
        {/if}
        {#if row?.isSelf}<span class="you">You</span>{/if}
        {#if row?.saved}<span class="saved" use:tip={'Saved by you'}><svg viewBox="0 0 10 12" aria-hidden="true"><path d="M1 1h8v10L5 8.2 1 11z"/></svg>Saved</span>{/if}
      </div>
      <div class="subline">
        {#if row?.name.tag}<span class="tag">{row.name.tag}</span>{:else if row}<span use:tip={`Account level ${row.level ?? '—'}`}>{row.agent ?? '—'}</span>{/if}
        {#if encounters}<span class="encounters" use:tip={{kv:row?.met?.tip ?? ''}} aria-label={row?.met?.aria}><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" aria-hidden="true"><path d="M4 7a9 9 0 1 1-1 8M4 2v5h5"/><circle cx="12" cy="10" r="2.4"/><path d="M8 17a4 4 0 0 1 8 0"/></svg>{encounters}</span>{/if}
      </div>
    </div>
  </div>
  <div class="rank-block" class:peak-active={peakActive}>
    <div class="current-rank" aria-hidden={peakActive}>
      {#if !row || row.rank.state === 'loading'}<Shimmer w={32} h={32}/><Shimmer w={76} h={16}/>{:else}
        <div class="rank-icon-slot">
          {#if row.rank.state !== 'missing'}<img {@attach retryImg} src={assetUrl('tier', row.rank.tier)} alt=""/>{/if}
          {#if row.peak}<span class="peak-gap" use:tip={row.peak.tip}><svg viewBox="0 0 12 14" aria-hidden="true"><path d="m2 6 4-4 4 4M6 2v10"/></svg>{row.peak.delta}</span>{/if}
        </div>
        <div class="rank-detail"><b style:color={row.rank.color}>{row.rank.name}</b><div class="rr"><span class="rr-track"><i style:background={row.rank.color} style:transform={`scaleX(${row.rank.rrPct / 100})`}></i></span><span>{row.rank.rrText}</span></div></div>
      {/if}
    </div>
    <div class="peak-rank" role="img" aria-hidden={!peakActive} style:--peak={emblem?.color} use:tip={emblem?.tip ?? 'Peak rank unavailable'} aria-label={emblem ? `Peak ${emblem.tip}${currentGapTip ? `. ${currentGapTip}` : ''}` : card?.peak === undefined ? 'Peak rank loading' : 'Peak rank unavailable'}>
      {#if card?.peak === undefined}<Shimmer w={32} h={32}/><Shimmer w={58} h={12}/>{:else if emblem}
        <div class="rank-icon-slot">
          <img {@attach retryImg} src={assetUrl('tier', emblem.tier)} alt=""/>
          {#if row?.peak}<span class="current-gap" use:tip={currentGapTip}><svg viewBox="0 0 12 14" aria-hidden="true"><path d="m2 8 4 4 4-4M6 12V2"/></svg>{row.peak.delta}</span>{/if}
        </div>
        <div class="peak-details">
          <b style:color={emblem.color} data-ellipsis>{emblem.name}</b>
          <span class="peak-act" data-ellipsis>{emblem.actAge}</span>
        </div>
      {:else}<span class="missing-peak">—</span>{/if}
    </div>
  </div>
  <div class="metric win">
    {#if row?.wr === undefined}<Shimmer w={30} h={16}/>{:else}
      <span class="reading" style:color={row?.wr?.color ?? 'var(--text-mid)'} use:tip={row?.wr ? `This act · ${row.wr.w} wins · ${row.wr.l} losses` : 'Win rate unavailable'}>{@render metricText(row?.wr?.text ?? '—')}
        {#if row?.streak}<span class="streak" class:losing={!row.streak.win} use:tip={row.streak.tip}><svg viewBox="0 0 14 18" aria-hidden="true"><path d={row.streak.win ? 'M7 15V3M2.5 7.5 7 3l4.5 4.5' : 'M7 3v12M2.5 10.5 7 15l4.5-4.5'}/></svg>{row.streak.n}</span>{/if}
      </span>
    {/if}
  </div>
  <div class="metric combat" style:color={row?.kd?.color ?? 'var(--text-mid)'} use:tip={'Kills per death · last 5 competitive'}>{#if row?.kd}<span>{row.kd.text}</span>{:else}<Shimmer w={30} h={16}/>{/if}</div>
  <div class="metric combat" style:color={row?.hs?.color ?? 'var(--text-mid)'} use:tip={'Headshot percentage · last 5 competitive'}>{#if row?.hs}<span>{@render metricText(row.hs.text)}</span>{:else}<Shimmer w={30} h={16}/>{/if}</div>
  <svg class="chevron" viewBox="0 0 8 12" aria-hidden="true"><path d="m2 2 4 4-4 4"/></svg>
</div>

<style>
  .compact-player{display:grid;grid-template-columns:var(--roster-columns);gap:var(--roster-gap);align-items:center;height:var(--live-row,56px);padding:0 12px 0 0;position:relative;border-bottom:1px solid #1b191a;background:#101010;cursor:pointer;min-width:0}
  /* Keyboard focus stays inside the row (the team panel clips its overflow): inset ring, row as its own stacking
     context, and the positioned ownership rail / party gutter (z-index −1) painting below the ring, not over it. */
  .compact-player:focus-visible{outline:2px solid var(--focus);outline-offset:-2px;isolation:isolate}
  .compact-player:focus-visible:before,.compact-player:focus-visible .party-gutter{z-index:-1}
  .alternate{background:var(--row-alternate)}.compact-player:hover,.selected{background:var(--row-hover)}.self:before{content:'';position:absolute;inset:0 auto 0 0;width:2px;background:var(--accent)}
  .identity{display:flex;gap:8px;align-items:center;min-width:0}.portrait{width:36px;height:36px;flex:none;overflow:hidden;border:1px solid var(--line-media);border-radius:6px;background:var(--skel)}.portrait.party{border:2px solid var(--party)}.portrait img{display:block;width:100%;height:100%;object-fit:contain}
  .names{min-width:0;flex:1}.name-line{display:flex;align-items:center;gap:5px;white-space:nowrap}.name-line strong{font:600 15px/18px var(--font);overflow:hidden;text-overflow:ellipsis;min-width:0}.name-line strong.hidden{font-style:italic}.remembered{text-decoration:underline dotted;text-underline-offset:2px}.remembered-icon{width:13px;height:13px;flex:none}.you{color:var(--accent-text);font:600 11px/1 var(--font)}
  /* Pin the dotted stroke's metrics at fractional Windows display scales, retaining ellipsis and the approved
     2px underline offset / 6px clip margin without widening the name's paint into its neighbours. */
  .name-line strong.remembered{overflow:clip;overflow-clip-margin:6px;text-decoration-thickness:1px}
  .subline{display:flex;gap:5px;align-items:center;min-height:18px;font:500 12px/18px var(--font);color:var(--text-muted)}.tag{white-space:nowrap}.saved{font:600 12px/16px var(--font);display:flex;gap:3px;align-items:center;border:1px solid #484848;border-radius:4px;padding:0 4px;flex:none}.saved svg{width:9px;height:11px;fill:currentColor}.encounters{display:inline-flex;gap:3px;align-items:center;font-size:13px}.encounters svg{width:18px;height:18px}
  /* R208: match the user's reference: rounded spine, small arrows into portraits, no outlined boxes.
     Only active stroke edges get a border; border-style on every side creates phantom dashed boxes. */
  .party-gutter{height:100%;position:relative}
  .party-link{position:absolute;left:8px;top:50%;bottom:50%;width:calc(100% + var(--roster-gap) - 8px);border:0;border-left:2px solid var(--party)}
  .party-link.to-top{top:0}.party-link.to-bottom{bottom:0}
  .party-link.to-bottom:not(.to-top){border-top:2px solid var(--party);border-top-left-radius:8px}
  .party-link.to-top:not(.to-bottom){border-bottom:2px solid var(--party);border-bottom-left-radius:8px}
  .party-link.to-top.to-bottom:after{content:'';position:absolute;left:0;right:0;top:50%;border:0;border-top:2px solid var(--party);transform:translateY(-50%)}
  .party-link:before{content:'';position:absolute;right:1px;top:-1px;width:6px;height:6px;border:0;border-top:2px solid var(--party);border-right:2px solid var(--party);transform:translateY(-50%) rotate(45deg)}
  .party-link.to-top:not(.to-bottom):before{top:auto;bottom:-1px;transform:translateY(50%) rotate(45deg)}
  .party-link.to-top.to-bottom:before{top:50%}
  .rank-block{position:relative;perspective:420px;min-width:0;width:144px}.current-rank{display:grid;grid-template-columns:32px minmax(0,1fr);gap:6px;align-items:center;opacity:1;transform:translate3d(0,0,0);transform-origin:16px center;transition:opacity 220ms ease 90ms,transform 440ms cubic-bezier(.22,1,.36,1)}
  .peak-active .current-rank{opacity:0;transform:translate3d(12px,-4px,-60px) rotateY(-14deg);transition-delay:0ms;pointer-events:none}.rank-icon-slot{width:32px;height:32px;position:relative}.rank-icon-slot img{width:32px;height:32px;object-fit:contain;flex:none}.peak-gap{position:absolute;bottom:-4px;left:-6px;border:1px solid var(--line);border-radius:4px;background:var(--card);font:600 11px/14px var(--font);padding:0 3px}
  .peak-gap,.current-gap{display:flex;align-items:center}.peak-gap svg,.current-gap svg{width:9px;height:11px;fill:none;stroke:currentColor;stroke-width:2;stroke-linecap:round;stroke-linejoin:round}
  .current-gap{position:absolute;bottom:-4px;left:-6px;border:1px solid var(--line);border-radius:4px;background:var(--card);font:600 11px/14px var(--font);padding:0 3px}
  .peak-details{min-width:0;flex:1}.peak-details>b{display:block;font:600 13px/16px var(--font);white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.peak-act{display:block;margin-top:3px;font:500 11px/14px var(--font);color:var(--text-2);white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
  .rank-detail{min-width:0}.rank-detail>b{display:block;font:600 13px/16px var(--font);white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.rr{display:flex;gap:6px;align-items:center;margin-top:3px;font:500 11px/1 var(--font);color:var(--text-2);white-space:nowrap}.rr-track{display:block;width:48px;height:4px;flex:none;background:var(--track);border-radius:2px;overflow:hidden}.rr-track i{display:block;width:100%;height:100%;transform-origin:left}
  .peak-rank{position:absolute;inset:50% 0 auto;margin-top:-16px;min-height:32px;display:flex;align-items:center;gap:6px;opacity:0;transform:translate3d(-12px,4px,45px) rotateY(12deg);transform-origin:16px center;transition:opacity 180ms ease,transform 440ms cubic-bezier(.22,1,.36,1);pointer-events:none}.peak-active .peak-rank{opacity:1;transform:translate3d(0,0,0);transition:opacity 240ms ease 100ms,transform 480ms cubic-bezier(.22,1,.36,1) 60ms;pointer-events:auto}
  .missing-peak{color:var(--text-muted)}
  .metric{text-align:center;font:600 18px/22px var(--font);color:var(--text);min-width:0;display:flex;justify-content:center}.combat{font-size:17px;border-left:1px solid var(--line);padding-left:3px}.reading{position:relative;white-space:nowrap}.streak{position:absolute;right:calc(100% + 4px);top:-8px;display:flex;gap:1px;align-items:center;color:var(--win-text);font:600 13px/16px var(--font)}.streak.losing{color:var(--loss-text)}.streak svg{width:12px;height:16px;fill:none;stroke:currentColor;stroke-width:2;stroke-linecap:round;stroke-linejoin:round}.chevron{width:8px;height:12px;fill:none;stroke:var(--text-muted);stroke-width:1.5}
  .percent-unit{font:500 14px/1 var(--font);margin-left:1px}.combat .percent-unit{font-size:13px}
</style>
