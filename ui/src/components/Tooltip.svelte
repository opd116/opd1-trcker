<!-- The one shared hover bubble (spec §4.1), inside #app. The engine is lib/tooltip.svelte.ts. -->
<script lang="ts">
  import { parseWl, tipState, wlRow } from "../lib/tooltip.svelte";

  const view = $derived(tipState.tip);
  const wl = $derived(view ? parseWl(view.text) : null);
</script>

{#if view}
  <div class="tip" role="tooltip" style:left="{view.x}px" style:top="{view.top}px" style:transform={view.transform}>{#if wl}{@const [w, d1, l, d2, g] = wlRow(wl)}<span class="wl"><span class="w">{w}</span><span class="dot">{d1}</span><span class="l">{l}</span><span class="dot">{d2}</span><span>{g}</span></span><div class="scope">This act</div>{:else}{view.text}{/if}</div>
{/if}

<style>
  .tip {
    position: absolute;
    z-index: 50;
    width: max-content;
    max-width: 260px;
    padding: 7px 10px;
    background: var(--tip-bg);
    border: 1px solid var(--line2);
    border-radius: 6px;
    box-shadow: 0 6px 18px rgba(0, 0, 0, 0.45);
    font-size: 13px;
    line-height: 18px;
    font-weight: 500;
    color: var(--text);
    white-space: pre-line;
    pointer-events: none;
  }

  .wl {
    display: flex;
    gap: 6px;
    align-items: baseline;
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }

  .w {
    color: var(--win);
    font-weight: 600;
  }

  .l {
    color: var(--loss);
    font-weight: 600;
  }

  .dot {
    color: var(--text3);
  }

  .scope {
    color: var(--text2);
  }
</style>
