<!--
  The one shared hover bubble (design v2 `#opd-tip`), position: fixed over the whole window. The engine is
  lib/tooltip.svelte.ts; this component renders the view, measures it, and places it with `tipPlace` (above the
  trigger, 8px off, flipped below when there is no room, 6px from the viewport edges). It stays invisible until it
  is placed, then fades in (.08s) as the design's does.
  The plain-text markup is kept on ONE line: `.tip` is `white-space: pre-line`, so a newline or indent between the
  tags would print as a stray blank line inside the bubble. Only the tip text's own `\n` may break a line.
-->
<script lang="ts">
  import { TIP_ID, tipPlace, tipState, wlFill, wlRow } from "../lib/tooltip.svelte";

  const view = $derived(tipState.tip);
  let el = $state<HTMLDivElement | null>(null);
  let pos = $state<{ left: number; top: number } | null>(null);

  // After every new view is in the DOM: measure it and place it. Until then it is laid out but transparent.
  $effect(() => {
    const v = view;
    pos = null;
    if (!v || !el) return;
    pos = tipPlace(v.anchor, el.offsetWidth, el.offsetHeight, window.innerWidth);
  });
</script>

{#if view}
  <div
    bind:this={el}
    class="tip"
    id={TIP_ID}
    role="tooltip"
    style:left="{pos?.left ?? 0}px"
    style:top="{pos?.top ?? 0}px"
    style:opacity={pos ? 1 : 0}
  >{#if view.kind === "img"}<div class="img-row"><img src={view.img} alt="" /><div class="img-col"><span class="img-head" style:color={view.color ?? "var(--text)"}>{view.head}</span><span class="img-sub">{view.sub}</span></div></div>{:else if view.kind === "kv"}<div class="kv-head">{view.head}</div><div class="kv">{#each view.rows as [k, v], i (i)}<span class="kv-k">{k}</span><span class="kv-v">{v}</span>{/each}</div>{:else if view.kind === "wl"}<div class="wl-row"><span class="w">{view.w}W</span><div class="bar"><div class="fill" style:width="{wlFill(view.w, view.l)}%"></div></div><span class="l">{view.l}L</span></div>{:else if view.kind === "legacyWl"}{@const [w, d1, l, d2, g] = wlRow(view)}<span class="wl"><span class="lw">{w}</span><span class="dot">{d1}</span><span class="ll">{l}</span><span class="dot">{d2}</span><span>{g}</span></span><div class="scope">{view.scope}</div>{:else}{view.text}{/if}</div>
{/if}

<style>
  /* TrackerApp.dc.html 1146: the bubble exactly. */
  .tip {
    position: fixed;
    z-index: 99999;
    pointer-events: none;
    transition: opacity 0.08s;
    width: max-content;
    max-width: 280px;
    padding: 7px 10px;
    background: var(--tip-bg);
    border: 1px solid var(--line-tip);
    border-radius: 7px;
    box-shadow: 0 10px 28px rgba(0, 0, 0, 0.5);
    color: var(--text);
    font: 500 12.5px/1.45 var(--font);
    white-space: pre-line;
  }

  /* data-tip-img */
  .img-row {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .img-row img {
    width: 36px;
    height: 36px;
    flex: none;
    display: block;
  }

  .img-col {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .img-head {
    font-weight: 700;
    font-size: 14px;
  }

  .img-sub {
    color: var(--text-2);
  }

  /* data-tip-kv */
  .kv-head {
    font-weight: 700;
    font-size: 13.5px;
    margin-bottom: 6px;
  }

  .kv {
    display: grid;
    grid-template-columns: auto auto;
    column-gap: 18px;
    row-gap: 3px;
  }

  .kv-k {
    color: var(--text-muted);
  }

  .kv-v {
    font-weight: 700;
    text-align: right;
  }

  /* data-tip-wl */
  .wl-row {
    display: flex;
    align-items: center;
    gap: 9px;
    font-weight: 700;
    font-size: 13.5px;
  }

  .w {
    color: var(--win-text);
  }

  .l {
    color: var(--loss-text);
  }

  .bar {
    width: 110px;
    height: 6px;
    border-radius: 3px;
    overflow: hidden;
    background: var(--loss);
  }

  .fill {
    height: 100%;
    background: var(--win);
  }

  /* LEGACY v8 `wl:` row */
  .wl {
    display: flex;
    gap: 6px;
    align-items: baseline;
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }

  .lw {
    color: var(--win);
    font-weight: 600;
  }

  .ll {
    color: var(--loss);
    font-weight: 600;
  }

  .dot {
    color: var(--text-ghost);
  }

  .scope {
    color: var(--text-muted);
  }
</style>
