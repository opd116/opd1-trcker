<!-- K/D, HS% (last 5 competitive) and Win% (this act) (spec §3.7). Scope lives in the tooltips, never on screen. -->
<script lang="ts">
  import { kd2, pct0 } from "../lib/format";
  import { LAST5_SCOPE, winTip } from "../lib/tips";
  import { tip } from "../lib/tooltip.svelte";
  import type { PlayerCard } from "../lib/types";
  import Skel from "./Skel.svelte";

  let { card }: { card: PlayerCard } = $props();

  // A loading block (value absent) has no tooltip: its scope explains a number that is not there yet. Same rule as
  // Win% (no tip while its games are unknown) and the top bar's Average party rank.
  const scope = (v: number | null | undefined) => (v === undefined ? "" : LAST5_SCOPE);

  const blocks = $derived([
    { label: "K/D", value: card.kd, text: kd2(card.kd), tip: scope(card.kd) },
    { label: "HS%", value: card.hsPct, text: pct0(card.hsPct), tip: scope(card.hsPct) },
    { label: "Win%", value: card.actWinrate, text: pct0(card.actWinrate), tip: winTip(card) },
  ]);
</script>

<div class="stats">
  {#each blocks as b (b.label)}
    <div class="stat" use:tip={b.tip}>
      {#if b.value === undefined}
        <!-- Loading: one bar, bottom-aligned in the block (spec §3.7). -->
        <Skel w={54} h={18} block />
      {:else}
        <span class="k">{b.label}</span>
        <span class="v">{b.text}</span>
      {/if}
    </div>
  {/each}
</div>

<style>
  .stats {
    display: flex;
    flex: 1;
    justify-content: space-between;
    gap: 12px;
  }

  .stat {
    display: flex;
    flex-direction: row-reverse;
    align-items: baseline;
    gap: 5px;
  }

  .k {
    font-size: 12px;
    line-height: 14px;
    font-weight: 500;
    color: var(--text2);
  }

  .v {
    font-size: 17px;
    line-height: 22px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
</style>
