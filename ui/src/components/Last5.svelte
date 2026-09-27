<!-- Last 5 competitive results, newest first: W/L (tile colour + fill), RR change, time since. -->
<script lang="ts">
  import { resultTip } from "../lib/cardTips";
  import { DASH, rrDelta, timeAgo } from "../lib/format";
  import { now } from "../lib/stores";
  import { tip } from "../lib/tooltip.svelte";
  import type { Last5Entry } from "../lib/types";
  import Skeleton from "./Skeleton.svelte";

  interface Props {
    items: Last5Entry[] | null | undefined;
    /** "fixed": compact tiles packed to the right. "fill": five equal columns. */
    fit: "fixed" | "fill";
  }

  let { items, fit }: Props = $props();
</script>

<ol class="last5 {fit}" aria-label="Last 5 competitive matches">
  {#if items === undefined}
    {#each [0, 1, 2, 3, 4] as i (i)}
      <li class="slot"><Skeleton w="100%" h="100%" /></li>
    {/each}
  {:else if items === null || items.length === 0}
    <li class="none" data-text>{DASH}</li>
  {:else}
    {#each items.slice(0, 5) as m, i (i)}
      <li
        class="slot tile"
        class:win={m.win}
        class:loss={!m.win}
        data-stat
        {@attach tip(resultTip(m))}
      >
        <span class="d num" data-text>{rrDelta(m.rrEarned)}</span>
        <span class="t num" data-text>{timeAgo(m.startMs, $now)}</span>
      </li>
    {/each}
  {/if}
</ol>

<style>
  .last5 {
    display: grid;
    gap: var(--tile-gap, 4px);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .fixed {
    grid-auto-flow: column;
    grid-auto-columns: var(--tile-w, 36px);
    justify-content: end;
  }

  .fill {
    grid-template-columns: repeat(5, minmax(0, 1fr));
  }

  .slot {
    display: grid;
    height: var(--tile-h, 38px);
  }

  .tile {
    align-content: center;
    justify-items: center;
    row-gap: 1px;
    border-radius: var(--r-xs);
    box-shadow: inset 0 2px 0 var(--c);
    background: var(--bg-c);
  }

  .win {
    --c: var(--win);
    --bg-c: var(--win-bg);
  }

  .loss {
    --c: var(--loss);
    --bg-c: var(--loss-bg);
  }

  .d {
    color: var(--c);
    font-size: var(--tile-fs, var(--fs-md));
    font-weight: var(--fw-strong);
    line-height: 1.1;
  }

  .t {
    color: var(--text-3);
    font-size: var(--fs-xs);
    line-height: 1.1;
  }

  .none {
    display: flex;
    align-items: center;
    height: var(--tile-h, 38px);
    color: var(--text-3);
  }
</style>
