<!--
  The drawer's rank card (prototype 988–1010): current rank (with the RR to the next tier), peak (act + time since
  that act ended; tooltip: the act's name and dates), last act; then the stats row, each stat's scope in its tooltip.
-->
<script lang="ts">
  import { assetUrl } from "../../lib/assets";
  import { retryImg } from "../../lib/imgRetry";
  import { peakCell, prevCell, rankCell, type StatCell } from "../../lib/profile";
  import { meta } from "../../lib/stores";
  import { peakTip } from "../../lib/tips";
  import { tip } from "../../lib/tooltip.svelte";
  import type { PlayerCard } from "../../lib/types";
  import Skel from "../Skel.svelte";

  let { card, stats }: { card: PlayerCard | null; stats: StatCell[] | null } = $props();

  const cur = $derived(card ? rankCell(card.rank, $meta) : undefined);
  const peak = $derived(card ? peakCell(card.peak, $meta) : undefined);
  const prev = $derived(card ? prevCell(card.prevActTier, $meta) : undefined);
  /** Loading: the five stats every player has (the user's Net RR joins once the card says who this is). */
  const cells = $derived(stats ?? ["K/D", "Headshot %", "Win rate", "Record", "Avg ACS"].map((label) => ({ label, value: undefined, color: "", tip: "" })));
</script>

<div class="rc">
  <div class="ranks">
    <div class="cell main">
      {#if cur === undefined}
        <Skel w={46} h={46} r="50%" block />
        <div class="col"><span class="lbl">CURRENT RANK</span><Skel w={120} h={18} block /><Skel w={110} h={11} block /></div>
      {:else}
        {#if cur.icon !== null}<img {@attach retryImg} class="i46" src={assetUrl("tier", cur.icon)} alt="" />{:else}<span class="i46"></span>{/if}
        <div class="col">
          <span class="lbl">CURRENT RANK</span>
          <span class="nm big" style:color={cur.color}>{cur.name}</span>
          {#if cur.sub}<span class="sub" data-ellipsis>{cur.sub}</span>{/if}
        </div>
      {/if}
    </div>
    <div class="cell" use:tip={card?.peak ? peakTip(card.peak) : ""}>
      {#if peak === undefined}
        <Skel w={32} h={32} r="50%" block />
        <div class="col"><span class="lbl">PEAK</span><Skel w={100} h={15} block /><Skel w={90} h={11} block /></div>
      {:else}
        {#if peak.icon !== null}<img {@attach retryImg} class="i32" src={assetUrl("tier", peak.icon)} alt="" />{:else}<span class="i32"></span>{/if}
        <div class="col">
          <span class="lbl">PEAK</span>
          <span class="nm" style:color={peak.color}>{peak.name}</span>
          {#if peak.sub}<span class="sub" data-ellipsis>{peak.sub}</span>{/if}
        </div>
      {/if}
    </div>
    <div class="cell">
      {#if prev === undefined}
        <Skel w={32} h={32} r="50%" block />
        <div class="col"><span class="lbl">LAST ACT</span><Skel w={100} h={15} block /><Skel w={70} h={11} block /></div>
      {:else}
        {#if prev.icon !== null}<img {@attach retryImg} class="i32 dim" src={assetUrl("tier", prev.icon)} alt="" />{:else}<span class="i32"></span>{/if}
        <div class="col">
          <span class="lbl">LAST ACT</span>
          <span class="nm" style:color={prev.color}>{prev.name}</span>
          {#if prev.sub}<span class="sub" data-ellipsis>{prev.sub}</span>{/if}
        </div>
      {/if}
    </div>
  </div>
  <div class="stats">
    {#each cells as s, i (s.label)}
      <div class={["stat", i > 0 && "sep"]} use:tip={s.tip}>
        <span class="sl">{s.label}</span>
        {#if s.value === undefined}<Skel w={52} h={22} block />{:else}<span class="sv" style:color={s.color}>{s.value}</span>{/if}
      </div>
    {/each}
  </div>
</div>

<style>
  .rc {
    flex: none;
    border: 1px solid var(--line-card);
    border-radius: 10px;
    overflow: hidden;
    background: var(--card);
  }

  .ranks {
    display: grid;
    grid-template-columns: 1.35fr 1fr 1fr;
  }

  .cell {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 16px 18px;
    border-left: 1px solid var(--line-card);
    min-width: 0;
    cursor: default;
  }

  .cell.main {
    gap: 14px;
    background: var(--nav-active);
    border-left: none;
  }

  .i46 {
    width: 46px;
    height: 46px;
    display: block;
    flex: none;
  }

  .i32 {
    width: 32px;
    height: 32px;
    display: block;
    flex: none;
  }

  .dim {
    opacity: 0.85;
  }

  .col {
    display: flex;
    flex-direction: column;
    gap: 7px;
    min-width: 0;
  }

  .lbl {
    font: 500 10.5px/1 var(--font-mono);
    letter-spacing: 0.06em;
    color: var(--text-label);
  }

  .nm {
    font: 700 17px/1 var(--font);
    white-space: nowrap;
  }

  .nm.big {
    font-size: 21px;
  }

  .sub {
    font: 500 12.5px/1 var(--font);
    color: var(--text-3);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .stats {
    border-top: 1px solid var(--line-card);
    padding: 13px 18px 15px;
    display: flex;
  }

  .stat {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 7px;
    cursor: default;
  }

  .stat.sep {
    padding-left: 14px;
    border-left: 1px solid var(--line-card);
  }

  .sl {
    font: 500 12.5px/1 var(--font);
    color: var(--text-muted);
    white-space: nowrap;
  }

  .sv {
    font: 600 22px/1 var(--font);
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }
</style>
