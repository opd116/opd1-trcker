<!--
  Last 5 competitive results (spec §3.8; hero §6.2): oldest → newest left to right, empty slots on the left, the
  newest the rightmost tile. Arrow style: the newest tile is filled and an arrow runs under the row. Underline
  style: the newest tile keeps the plain box and gets a bar under it.
-->
<script lang="ts">
  import { last5Tiles } from "../lib/live";
  import { boxStyle } from "../lib/resultBox";
  import { now } from "../lib/stores";
  import { theme } from "../lib/themeStore.svelte";
  import { last5Tip } from "../lib/tips";
  import { tip } from "../lib/tooltip.svelte";
  import type { Last5Entry } from "../lib/types";
  import Skel from "./Skel.svelte";

  let { entries, size }: { entries: Last5Entry[] | null | undefined; size: "card" | "hero" } = $props();

  const tiles = $derived(last5Tiles(entries));
  const arrow = $derived(theme.last5Style === "Arrow");
</script>

{#if tiles === "skeleton"}
  <div class={["l5", `size-${size}`]}><Skel w={size === "hero" ? 154 : 130} h={size === "hero" ? 26 : 22} r={4} block /></div>
{:else}
  <div class={["l5", `size-${size}`]}>
    <div class="l5col">
      <div class="tiles">
        <!-- Keyed by slot: the tiles are positional, and two entries may share a startMs (bad or placeholder data). -->
        {#each tiles as t, i (i)}
          {#if t.entry}
            <div class="tile" style={boxStyle(t.key, t.newest && arrow)} use:tip={last5Tip(t.entry, $now)}>{t.key}{#if t.newest && !arrow}<div class="uline"></div>{/if}</div>
          {:else}
            <div class="tile" style={boxStyle("E")}></div>
          {/if}
        {/each}
      </div>
      {#if arrow}
        <div class="arrow">
          <div class="aline"></div>
          <svg width="5" height="7" viewBox="0 0 5 7" fill="none" stroke="currentColor" stroke-width="1.2" aria-hidden="true"
            ><path d="M0.6 0.6L4.2 3.5L0.6 6.4" /></svg
          >
        </div>
      {/if}
    </div>
  </div>
{/if}

<style>
  .l5 {
    align-self: end;
    justify-self: start;
  }

  .l5col {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .tiles {
    display: flex;
    gap: 5px;
  }

  .tile {
    position: relative;
    width: 22px;
    height: 22px;
    box-sizing: border-box;
    border-radius: 4px;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 12px;
    font-weight: 700;
  }

  .uline {
    position: absolute;
    left: 5px;
    right: 5px;
    bottom: -6px;
    height: 2px;
    border-radius: 1px;
    background: var(--text);
  }

  .arrow {
    position: relative;
    height: 7px;
    color: var(--arrow-col);
  }

  .aline {
    position: absolute;
    left: 0;
    right: 2px;
    top: 3px;
    height: 1px;
    background: currentColor;
  }

  .arrow svg {
    position: absolute;
    right: 0;
    top: 0;
    display: block;
  }

  /* Hero (spec §6.2): 26px tiles, 13px letters, 6px gaps. */
  .size-hero .l5col {
    gap: 4px;
  }

  .size-hero .tiles {
    gap: 6px;
  }

  .size-hero .tile {
    width: 26px;
    height: 26px;
    font-size: 13px;
  }

  .size-hero .uline {
    left: 6px;
    right: 6px;
  }
</style>
