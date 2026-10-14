<!--
  Previous act rank (spec §3.6, R125/R126): icon · `Prev` · tier name, or `Prev —`. No tooltip.
  The icon sits centred in a 40px column, the width of the current-rank icon above it, so the two icons share one
  centre line; the label starts right after that column and the name follows it. With no prev tier the column
  stays, empty. The name's ellipsis is only a safety net: `Prev` + `Ascendant 3` fits in all three fonts (fit-live).
-->
<script lang="ts">
  import { assetUrl } from "../lib/assets";
  import { retryImg } from "../lib/imgRetry";
  import { isRankedTier, tierLabel } from "../lib/meta";
  import { meta } from "../lib/stores";
  import Skel from "./Skel.svelte";

  let { tier }: { tier: number | null | undefined } = $props();

  const name = $derived(isRankedTier(tier) ? tierLabel($meta, tier) : null);
</script>

<div class="prev">
  {#if tier === undefined}
    <!-- Loading: the whole row is one bar (spec §5.4). -->
    <Skel w={124} h={10} block />
  {:else}
    <span class="lbl">Prev</span>
    <div class="icol">
      {#if isRankedTier(tier)}<img {@attach retryImg} class="pi" src={assetUrl("tier", tier)} alt="" />{/if}
    </div>
    {#if isRankedTier(tier)}
      <span class="pname">{#if name === undefined}<Skel w={60} h={10} />{:else}{name ?? "—"}{/if}</span>
    {:else}
      <span class="dash">—</span>
    {/if}
  {/if}
</div>

<style>
  .prev {
    display: flex;
    align-items: center;
    min-width: 0;
    white-space: nowrap;
    font-size: 12px;
    font-weight: 500;
    color: var(--text2);
  }

  /* The rank icon's column (spec §3.6 Current): 11px + 18px icon + 11px. */
  .icol {
    width: 22px;
    flex: none;
    display: flex;
    justify-content: center;
  }

  .pi {
    display: block;
    width: 18px;
    height: 18px;
    flex: none;
    opacity: 0.8;
  }

  .lbl {
    flex: none;
    width: 30px;
  }

  .pname {
    min-width: 0;
    font-size: 12px;
    font-weight: 500;
    color: var(--text2);
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .dash {
    font-size: 13px;
  }
</style>
