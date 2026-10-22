<!--
  Competitive · MAPS / AGENTS (design v2 TrackerApp.dc.html 742–771): the act's most played maps or agents, 30px rows of
  image + name, W–L, a win% bar (≥ 55% green, ≤ 45% red, grey between), WIN% and NET RR (maps) or K/D (agents). Each
  row's tooltip gives games and W–L, plus "limited evidence" under 8 games. At most 7 rows.
-->
<script lang="ts">
  import { assetUrl } from "../../lib/assets";
  import { statRowView } from "../../lib/compv2";
  import { DASH } from "../../lib/format";
  import { retryImg } from "../../lib/imgRetry";
  import { agentName, mapName } from "../../lib/meta";
  import { meta } from "../../lib/stores";
  import { tip } from "../../lib/tooltip.svelte";
  import type { StatRow } from "../../lib/types";
  import Skel from "../Skel.svelte";

  let { kind, rows }: { kind: "map" | "agent"; rows: StatRow[] | null | undefined } = $props();

  const MAX_ROWS = 7;
  const views = $derived((rows ?? []).slice(0, MAX_ROWS).map(statRowView));
  const nameOf = (id: string): string => {
    const n = kind === "map" ? mapName($meta, id) : agentName($meta, id);
    return typeof n === "string" && n ? n : DASH;
  };
</script>

<section class="card" aria-label={kind === "map" ? "Maps" : "Agents"} aria-busy={rows === undefined}>
  <div class="hdr">
    <span class="micro">{kind === "map" ? "MAPS" : "AGENTS"} · CURRENT ACT</span>
    <span class="lab">by games</span>
  </div>
  <div class="grid head">
    <span>{kind === "map" ? "MAP" : "AGENT"}</span><span class="r">W–L</span><span></span><span class="r">WIN%</span><span class="r"
      >{kind === "map" ? "NET RR" : "K/D"}</span
    >
  </div>
  {#if rows === undefined}
    {#each [0, 1, 2, 3, 4] as i (i)}
      <div class="grid row">
        <div class="who"><Skel w={40} h={26} r={4} block /><Skel w={70} h={12} block /></div>
        <Skel w={30} h={12} block /><Skel w={52} h={6} block /><Skel w={30} h={12} block /><Skel w={30} h={12} block />
      </div>
    {/each}
  {:else if views.length === 0}
    <div class="none">{DASH}</div>
  {:else}
    {#each views as v (v.id)}
      {@const name = nameOf(v.id)}
      <div class="grid row" use:tip={`${name}: ${v.tipTail}`}>
        <div class="who">
          {#if kind === "map"}
            <img {@attach retryImg} class="map" src={assetUrl("mapList", v.id)} alt="" />
          {:else}
            <span class="agbox"><img {@attach retryImg} class="agent" src={assetUrl("agent", v.id)} alt="" /></span>
          {/if}
          <span class="name" data-ellipsis>{name}</span>
        </div>
        <span class="wl">{v.wl}</span>
        <span class="track"><span class="bar" style:width="{v.wrPct}%" style:background={v.bar}></span></span>
        <span class="wr">{v.wr}</span>
        {#if kind === "map"}
          <span class="last" style:color={v.netColor}>{v.netRr}</span>
        {:else}
          <span class="last kd">{v.kd}</span>
        {/if}
      </div>
    {/each}
  {/if}
</section>

<style>
  .card {
    background: var(--card);
    border: 1px solid var(--line);
    border-radius: 12px;
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 10px;
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

  .grid {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 40px 52px 38px 46px;
    column-gap: 8px;
    align-items: center;
  }

  .head {
    font: 500 10px/1 var(--font-mono);
    letter-spacing: 0.06em;
    color: var(--text-muted);
    padding-bottom: 2px;
  }

  .r {
    text-align: right;
  }

  .row {
    height: 30px;
    cursor: default;
  }

  .who {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }

  .map {
    width: 40px;
    height: 26px;
    object-fit: cover;
    border-radius: 4px;
    display: block;
    flex: none;
    background: var(--skel);
  }

  .agbox {
    width: 40px;
    height: 26px;
    flex: none;
    display: flex;
  }

  .agent {
    width: 26px;
    height: 26px;
    border-radius: 4px;
    display: block;
    background: var(--skel);
  }

  .name {
    font: 600 13.5px/1 var(--font);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }

  .wl {
    font: 500 13px/1 var(--font);
    color: var(--text-2);
    text-align: right;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  .track {
    height: 6px;
    border-radius: 3px;
    background: var(--button-hover);
    overflow: hidden;
  }

  .bar {
    display: block;
    height: 100%;
    border-radius: 3px;
  }

  .wr {
    font: 600 14px/1 var(--font);
    text-align: right;
    font-variant-numeric: tabular-nums;
  }

  .last {
    font: 700 13.5px/1 var(--font);
    text-align: right;
    font-variant-numeric: tabular-nums;
  }

  .kd {
    font-weight: 600;
    color: var(--text-mid);
  }

  .none {
    padding: 20px 0;
    text-align: center;
    font: 500 14px/1 var(--font);
    color: var(--text-muted);
  }
</style>
