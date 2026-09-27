<!-- The one shared hover bubble. Mounted on <body> by the `tip` attachment on first use. -->
<script lang="ts">
  import type { Attachment } from "svelte/attachments";
  import { hideTip, tipState } from "../lib/tooltip.svelte";

  const GAP = 6;
  const EDGE = 8;

  /** Centre above the anchor, flip below near the top, clamp to the window. */
  const place: Attachment<HTMLElement> = (el) => {
    const anchor = tipState.rect;
    void tipState.lines.length; // re-place when the text changes
    if (!anchor) return;
    const b = el.getBoundingClientRect();
    const maxX = window.innerWidth - b.width - EDGE;
    const x = Math.max(EDGE, Math.min(maxX, anchor.left + anchor.width / 2 - b.width / 2));
    let y = anchor.top - b.height - GAP;
    if (y < EDGE) y = anchor.bottom + GAP;
    el.style.transform = `translate(${Math.round(x)}px, ${Math.round(y)}px)`;
    el.style.visibility = "visible";
  };
</script>

<!-- Escape dismisses the bubble without moving the pointer (WCAG 1.4.13). -->
<svelte:window
  onkeydown={(e) => {
    if (e.key === "Escape" && tipState.rect) hideTip();
  }}
/>

{#if tipState.rect && tipState.lines.length > 0}
  <div class="tip" role="tooltip" {@attach place}>
    {#each tipState.lines as line, i (i)}
      <div class={i === 0 ? "title" : "line"}>{line}</div>
    {/each}
  </div>
{/if}

<style>
  .tip {
    position: fixed;
    left: 0;
    top: 0;
    z-index: 1000;
    visibility: hidden;
    max-width: 280px;
    padding: 7px 10px 8px;
    background: var(--surface-3);
    border: 1px solid var(--line-strong);
    border-radius: var(--r-sm);
    box-shadow: 0 8px 24px rgb(5 8 12 / 0.45);
    pointer-events: none;
    font-size: var(--fs-sm);
    line-height: var(--lh-body);
    font-variant-numeric: tabular-nums;
  }

  .title {
    color: var(--text-1);
    font-weight: var(--fw-strong);
  }

  .line {
    color: var(--text-2);
  }
</style>
