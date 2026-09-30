<!-- K/D, HS% (last 5 competitive) and Win% (this act) (spec §3.7). Scope lives in the tooltips, never on screen. -->
<script lang="ts">
  import { kd2, pct0 } from "../lib/format";
  import { LAST5_SCOPE, winTip } from "../lib/tips";
  import { tip } from "../lib/tooltip.svelte";
  import type { PlayerCard } from "../lib/types";
  import Skel from "./Skel.svelte";

  let { card }: { card: PlayerCard } = $props();

  const blocks = $derived([
    { label: "K/D", value: card.kd, text: kd2(card.kd), tip: LAST5_SCOPE },
    { label: "HS%", value: card.hsPct, text: pct0(card.hsPct), tip: LAST5_SCOPE },
    { label: "Win%", value: card.actWinrate, text: pct0(card.actWinrate), tip: winTip(card) },
  ]);
</script>

<div class="stats">
  {#each blocks as b (b.label)}
    <div class="stat" use:tip={b.tip}>
      {#if b.value === undefined}
        <!-- Loading: one bar, bottom-aligned in the block (spec §3.7). -->
        <Skel w={44} h={30} block />
      {:else}
        <span class="k">{b.label}</span>
        <span class="v">{b.text}</span>
      {/if}
    </div>
  {/each}
</div>

<style>
  .stats {
    align-self: end;
    display: flex;
  }

  .stat {
    width: 76px;
    display: flex;
    flex-direction: column;
    justify-content: flex-end;
  }

  .k {
    font-size: 12px;
    line-height: 14px;
    font-weight: 500;
    color: var(--text2);
  }

  .v {
    font-size: 20px;
    line-height: 24px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
</style>
