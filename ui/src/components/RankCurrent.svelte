<!-- Current rank (spec §3.6): icon + tier name + exact RR. No tooltip. -->
<script lang="ts">
  import { assetUrl } from "../lib/assets";
  import { retryImg } from "../lib/imgRetry";
  import { rr } from "../lib/format";
  import { isRankedTier, tierLabel } from "../lib/meta";
  import { meta } from "../lib/stores";
  import type { Rank } from "../lib/types";
  import Skel from "./Skel.svelte";

  let { rank }: { rank: Rank | null | undefined } = $props();

  const ranked = $derived(rank && isRankedTier(rank.tier) ? rank : null);
  const name = $derived(ranked ? tierLabel($meta, ranked.tier) : null);
</script>

<div class="rank">
  {#if rank === undefined}
    <!-- Loading: the skeleton card's own rank shapes (spec §5.4). -->
    <Skel w={40} h={40} r="50%" block />
    <div class="rtext skel"><Skel w={84} h={13} block /><Skel w={40} h={10} block /></div>
  {:else if !ranked}
    <!-- [C] Unranked or missing: the box keeps the layout, then —. -->
    <div class="icon"></div>
    <span class="rtext dash">—</span>
  {:else}
    <img {@attach retryImg} class="icon" src={assetUrl("tier", ranked.tier)} alt="" />
    <div class="rtext">
      <span class="tier">{#if name === undefined}<Skel w={84} h={13} />{:else}{name ?? "—"}{/if}</span>
      <span class="rr"><span class="v">{rr(ranked.rr)}</span> RR</span>
    </div>
  {/if}
</div>

<style>
  .rank {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }

  .icon {
    display: block;
    width: 40px;
    height: 40px;
    flex: none;
  }

  .rtext {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .skel {
    gap: 7px;
  }

  .tier {
    font-size: 15px;
    font-weight: 600;
    white-space: nowrap;
  }

  .rr {
    font-size: 12px;
    font-weight: 500;
    color: var(--text2);
    white-space: nowrap;
  }

  .v {
    font-size: 13px;
    font-weight: 600;
    color: var(--text);
    font-variant-numeric: tabular-nums;
  }

  .dash {
    font-size: 15px;
    font-weight: 600;
    color: var(--text2);
  }
</style>
