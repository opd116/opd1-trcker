<!-- A tier icon slot: skeleton while loading (undefined), "—" when missing or unranked (null / tier < 3). -->
<script lang="ts">
  import { assetUrl } from "../lib/assets";
  import { DASH } from "../lib/format";
  import { isRankedTier } from "../lib/meta";
  import Skeleton from "./Skeleton.svelte";

  interface Props {
    tier: number | null | undefined;
    /** Icon edge in px. */
    px: number;
  }

  let { tier, px }: Props = $props();

  /** Tier whose icon failed to load; a new tier gets a fresh attempt. */
  let failedTier = $state<number | null>(null);
</script>

<span class="slot" style:width="{px}px" style:height="{px}px">
  {#if tier === undefined}
    <Skeleton w="{px}px" h="{px}px" r="50%" />
  {:else if isRankedTier(tier) && failedTier !== tier}
    <img src={assetUrl("tier", tier)} width={px} height={px} alt="" decoding="async" onerror={() => (failedTier = tier)} />
  {:else}
    <span class="none" data-text>{DASH}</span>
  {/if}
</span>

<style>
  .slot {
    display: inline-flex;
    flex: none;
    align-items: center;
    justify-content: center;
  }

  img {
    width: 100%;
    height: 100%;
    object-fit: contain;
  }

  .none {
    color: var(--text-3);
    font-size: var(--fs-lg);
    line-height: 1;
  }
</style>
