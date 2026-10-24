<!--
  Competitive · BEST TIMES TO PLAY (design v2 TrackerApp.dc.html 772–788): seven weekday tiles, Monday first (tinted
  green from 60%, red to 47%; hatched with a dashed edge under 8 games, dashed and empty with none), four time-of-day
  rows (bar coloured like the tables; a dashed empty track with no games), and the legend.
-->
<script lang="ts">
  import { daypartRow, weekdayTile } from "../../lib/compv2";
  import { tip } from "../../lib/tooltip.svelte";
  import type { WinCell } from "../../lib/types";
  import Skel from "../Skel.svelte";

  let {
    weekdays,
    dayparts,
  }: { weekdays: readonly WinCell[] | null | undefined; dayparts: readonly WinCell[] | null | undefined } = $props();

  const loading = $derived(weekdays === undefined);
  const tiles = $derived([0, 1, 2, 3, 4, 5, 6].map((i) => weekdayTile(weekdays?.[i], i)));
  const rows = $derived([0, 1, 2, 3].map((i) => daypartRow(dayparts?.[i], i)));
</script>

<section class="card" aria-label="Best times to play" aria-busy={loading}>
  <div class="hdr">
    <span class="micro">BEST TIMES TO PLAY</span>
    <span class="lab">win% · games</span>
  </div>
  <div class="week">
    {#each tiles as t, i (i)}
      <div class="day" use:tip={loading ? "" : t.tip}>
        <span class="dname">{t.d}</span>
        {#if loading}
          <span class="tile skel"></span>
        {:else}
          <span class={["tile", t.dashed && "dashed"]} style:background={t.bg}>
            <span class="twr" class:empty={t.empty}>{t.wr}</span>
            <span class="tg">{t.g}</span>
          </span>
        {/if}
      </div>
    {/each}
  </div>
  <div class="parts">
    {#each rows as r (r.d)}
      <div class="part" use:tip={loading ? "" : r.tip}>
        <div class="pname"><span class="pd">{r.d}</span><span class="ph">{r.h}</span></div>
        {#if loading}
          <Skel w={80} h={6} block /><Skel w={36} h={12} block /><Skel w={50} h={12} block />
        {:else}
          <span class={["ptrack", r.empty && "empty"]}><span class="pbar" style:width="{r.w}%" style:background={r.bar}></span></span>
          <span class="pwr" class:empty={r.empty}>{r.wr}</span>
          <span class="pg">{r.g}</span>
        {/if}
      </div>
    {/each}
  </div>
  <div class="legend">
    <span class="li"><span class="hatch"></span>Under 8 games: limited evidence</span>
    <span class="li"><span class="dash">—</span>No games played</span>
  </div>
</section>

<style>
  .card {
    background: var(--card);
    border: 1px solid var(--line);
    border-radius: 12px;
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    min-width: 0;
  }

  .hdr {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
  }

  .micro {
    font: 500 10.5px/1 var(--font-mono);
    letter-spacing: 0.06em;
    color: var(--text-muted);
  }

  .lab {
    font: 500 12.5px/1 var(--font);
    color: var(--text-muted);
  }

  .week {
    display: grid;
    grid-template-columns: repeat(7, minmax(0, 1fr));
    gap: 4px;
  }

  .day {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    cursor: default;
  }

  .dname {
    font: 600 12px/1 var(--font);
    color: var(--text-2);
  }

  .tile {
    width: 100%;
    height: 50px;
    box-sizing: border-box;
    border-radius: 5px;
    border: 1px solid transparent;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 5px;
  }

  .tile.dashed {
    border: 1px dashed var(--line-strong);
  }

  .tile.skel {
    background: var(--skel);
  }

  .twr {
    font: 700 13.5px/1 var(--font);
    color: var(--text);
    font-variant-numeric: tabular-nums;
  }

  .tg {
    font: 500 11px/1 var(--font);
    color: var(--text-2);
  }

  .empty {
    color: var(--text-muted);
  }

  .parts {
    display: flex;
    flex-direction: column;
    gap: 7px;
  }

  .part {
    display: grid;
    grid-template-columns: minmax(0, 1.3fr) minmax(0, 1fr) 46px 62px;
    column-gap: 8px;
    align-items: center;
    height: 26px;
    cursor: default;
  }

  .pname {
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
  }

  .pd {
    font: 600 12.5px/1 var(--font);
  }

  .ph {
    font: 500 11px/1 var(--font);
    color: var(--text-muted);
    white-space: nowrap;
  }

  .ptrack {
    height: 6px;
    box-sizing: border-box;
    border-radius: 3px;
    background: var(--button-hover);
    overflow: hidden;
  }

  .ptrack.empty {
    background: transparent;
    border: 1px dashed var(--line-strong);
  }

  .pbar {
    display: block;
    height: 100%;
  }

  .pwr {
    font: 700 13.5px/1 var(--font);
    text-align: right;
    color: var(--text);
    font-variant-numeric: tabular-nums;
  }

  .pwr.empty {
    color: var(--text-muted);
  }

  .pg {
    font: 500 12px/1 var(--font);
    text-align: right;
    color: var(--text-muted);
    white-space: nowrap;
  }

  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 14px;
    padding-top: 8px;
    border-top: 1px solid var(--line);
    font: 500 12px/1.3 var(--font);
    color: var(--text-muted);
  }

  .li {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .hatch {
    width: 14px;
    height: 10px;
    box-sizing: border-box;
    border-radius: 2px;
    border: 1px dashed var(--text-disabled);
    background: repeating-linear-gradient(135deg, rgba(255, 255, 255, 0.1) 0 2px, transparent 2px 4px);
  }

  .dash {
    font-weight: 700;
    color: var(--text-2);
  }
</style>
