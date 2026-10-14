<!-- Peak row (spec §3.6): icon + tier name + the act it was reached in. The tooltip covers the whole group [C]. -->
<script lang="ts">
  import { assetUrl } from "../lib/assets";
  import { retryImg } from "../lib/imgRetry";
  import { peakLabel } from "../lib/format";
  import { tierLabel } from "../lib/meta";
  import { meta } from "../lib/stores";
  import { peakTip } from "../lib/tips";
  import { tip } from "../lib/tooltip.svelte";
  import type { Peak } from "../lib/types";
  import Skel from "./Skel.svelte";

  let { peak }: { peak: Peak | null | undefined } = $props();

  const name = $derived(peak ? tierLabel($meta, peak.tier) : null);
</script>

<div class="peak">
  {#if peak === undefined}
    <!-- Loading: the whole row, label included, is one bar (spec §5.4). -->
    <Skel w={176} h={10} block />
  {:else}
    <span class="lbl">Peak</span>
    {#if peak === null}
      <span class="dash">—</span>
    {:else}
      <div class="pgroup" use:tip={peakTip(peak)}>
        <img {@attach retryImg} class="pi" src={assetUrl("tier", peak.tier)} alt="" />
        <span class="pname">{#if name === undefined}<Skel w={60} h={10} />{:else}{name ?? "—"}{/if}</span>
        <span class="when">{peakLabel(peak)}</span>
      </div>
    {/if}
  {/if}
</div>

<style>
  .peak {
    display: flex;
    align-items: center;
    min-width: 0;
    white-space: nowrap;
    font-size: 12px;
    font-weight: 500;
    color: var(--text2);
  }

  .lbl {
    width: 30px;
    flex: none;
  }

  .pgroup {
    display: flex;
    align-items: center;
    min-width: 0;
  }

  .pi {
    display: block;
    width: 18px;
    height: 18px;
    flex: none;
    margin-right: 4px;
  }

  .pname {
    font-size: 12px;
    font-weight: 500;
    color: var(--text2);
    margin-right: 6px;
  }

  /* The ellipsis is only a safety net: ingame-longnames proves it never triggers (spec §3.6). */
  .when {
    font-variant-numeric: tabular-nums;
    overflow: hidden;
    text-overflow: ellipsis;
  }


  .dash {
    font-size: 13px;
  }
</style>
