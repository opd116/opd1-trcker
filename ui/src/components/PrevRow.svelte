<!-- Previous act rank (spec §3.6): greyed icon + tier name, or —. No tooltip. -->
<script lang="ts">
  import { assetUrl } from "../lib/assets";
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
    {#if isRankedTier(tier)}
      <img class="pi" src={assetUrl("tier", tier)} alt="" />
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

  .lbl {
    width: 50px;
    flex: none;
  }

  .pi {
    display: block;
    width: 18px;
    height: 18px;
    flex: none;
    margin-right: 6px;
    filter: grayscale(0.7);
    opacity: 0.75;
  }

  .pname {
    font-size: 13px;
    font-weight: 500;
    color: var(--text2);
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .dash {
    font-size: 13px;
  }
</style>
