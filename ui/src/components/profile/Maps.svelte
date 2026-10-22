<!--
  The act's most played maps (prototype 1077–1086; `profile_extras.maps`): map strip, win-rate bar, WIN%, record.
  `pad` lines the rows up with the loadout tiles beside them (prototype `mapsPad`).
-->
<script lang="ts">
  import { assetUrl } from "../../lib/assets";
  import { retryImg } from "../../lib/imgRetry";
  import { mapRows } from "../../lib/profile";
  import { meta } from "../../lib/stores";
  import { tip } from "../../lib/tooltip.svelte";
  import type { StatRow } from "../../lib/types";
  import Skel from "../Skel.svelte";

  /** `rows` undefined while `profile_extras` is pending; [] when it failed or has none. */
  let { rows, pad }: { rows: StatRow[] | undefined; pad: boolean } = $props();

  const views = $derived(rows ? mapRows(rows, $meta) : null);
</script>

<div class={["maps", pad && "pad"]}>
  {#if views === null}
    {#each [0, 1, 2, 3, 4] as i (i)}
      <div class="row" aria-hidden="true"><Skel w={104} h={28} r={5} block /><Skel w={180} h={6} block /><span></span><span></span></div>
    {/each}
  {:else}
    {#each views as m (m.id)}
      <div class="row" use:tip={m.tip}>
        <div class="strip">
          <img {@attach retryImg} src={assetUrl("mapList", m.id)} alt="" />
          <div class="shade"></div>
          <span class="mn">{#if m.map === undefined}<Skel w={50} h={11} />{:else}{m.map ?? "—"}{/if}</span>
        </div>
        <div class="bar"><div style:width={`${m.wr}%`} style:background={m.barColor}></div></div>
        <span class="wr" style:color={m.textColor}>{m.wr}%</span>
        <span class="rec">{m.record}</span>
      </div>
    {/each}
  {/if}
</div>

<style>
  .maps {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }

  .maps.pad {
    padding-top: 20px;
  }

  .row {
    display: grid;
    grid-template-columns: 104px minmax(0, 1fr) 40px 44px;
    column-gap: 10px;
    align-items: center;
    cursor: default;
  }

  .strip {
    position: relative;
    height: 28px;
    border-radius: 5px;
    overflow: hidden;
    background: var(--skel);
  }

  .strip img {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
    opacity: 0.6;
  }

  .shade {
    position: absolute;
    inset: 0;
    background: linear-gradient(90deg, rgba(12, 13, 16, 0.9) 25%, rgba(12, 13, 16, 0.3));
  }

  .mn {
    position: relative;
    display: flex;
    align-items: center;
    height: 100%;
    padding-left: 9px;
    font: 600 13px/1 var(--font);
    color: var(--text);
  }

  .bar {
    height: 6px;
    border-radius: 3px;
    background: var(--button-hover);
    overflow: hidden;
  }

  .bar div {
    height: 100%;
    border-radius: 3px;
  }

  .wr {
    font: 600 14px/1 var(--font);
    text-align: right;
    font-variant-numeric: tabular-nums;
  }

  .rec {
    font: 500 13px/1 var(--font);
    color: var(--text-3);
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
</style>
