<!--
  A grid of character radios (TrackerApp.dc.html 866–868 "Fill character", 885–887 "Brush"): each cell shows the
  character in the monospace face, named by its tooltip and aria-label. `lit` says whether the checked cell shows its
  violet frame (the brush's only does while the Draw tool is on). Keyboard: one tab stop, arrows move (radio.ts).
-->
<script lang="ts">
  import { CHAR_NAMES } from "../../lib/ascii";
  import { tip } from "../../lib/tooltip.svelte";
  import { radioKeydown, radioTabIndex } from "../settings/radio";

  let {
    chars,
    value,
    onchange,
    label,
    columns,
    size = "fill",
    lit = true,
  }: {
    chars: readonly string[];
    value: string;
    onchange: (c: string) => void;
    label: string;
    columns: number;
    /** "fill": 34px cells, 17px glyphs; "brush": 32px cells, 15px glyphs. */
    size?: "fill" | "brush";
    lit?: boolean;
  } = $props();

  const sel = $derived(chars.indexOf(value));
</script>

<div
  role="radiogroup"
  aria-label={label}
  class="grid {size}"
  style:grid-template-columns="repeat({columns},1fr)"
  tabindex="-1"
  onkeydown={radioKeydown}
>
  {#each chars as c, i (c)}
    <button
      type="button"
      role="radio"
      class="cell"
      class:lit={lit && c === value}
      class:hi={c === value}
      aria-checked={c === value}
      aria-label={CHAR_NAMES[c] ?? c}
      tabindex={radioTabIndex(i, sel)}
      use:tip={CHAR_NAMES[c] ?? c}
      onclick={() => onchange(c)}>{c}</button
    >
  {/each}
</div>

<style>
  .grid {
    flex: none;
    display: grid;
    gap: 6px;
  }

  .grid:focus {
    outline: none;
  }

  .cell {
    height: 34px;
    border-radius: 7px;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    font: 400 17px/1 "Cascadia Mono", Consolas, "DejaVu Sans Mono", Menlo, var(--font-mono);
    cursor: pointer;
    background: var(--input);
    border: 1px solid var(--line-control);
    color: var(--text-2);
  }

  .brush .cell {
    height: 32px;
    font-size: 15px;
  }

  .cell.hi {
    color: var(--text-hi);
  }

  .cell.lit {
    background: rgba(157, 140, 255, 0.16);
    border-color: rgba(157, 140, 255, 0.6);
  }

  /* The design's hover edge wins, the checked cell's included. */
  .cell:hover {
    border-color: var(--line-strong);
  }
</style>
