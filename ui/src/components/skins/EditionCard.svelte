<!--
  Collection · editions (design v2 TrackerApp.dc.html 825–840): a 10px bar split by edition, then one chip per edition
  (swatch, name, count). A chip toggles the grid's edition filter; while one is on, the others fade to 45%.
-->
<script lang="ts">
  import { ALL, type EditionRow } from "../../lib/skins";
  import type { Edition } from "../../lib/types";

  let {
    rows,
    selected,
    onselect,
  }: { rows: EditionRow[]; selected: Edition | typeof ALL; onselect: (e: Edition | typeof ALL) => void } = $props();

  const op = (e: Edition) => (selected === ALL || selected === e ? 1 : 0.45);
</script>

<section class="card" aria-label="Editions">
  <div class="stack" aria-hidden="true">
    {#each rows as t (t.edition)}
      {#if t.n > 0}<div class="seg" style:flex={t.n} style:background={t.color} style:opacity={op(t.edition)}></div>{/if}
    {/each}
  </div>
  <div class="chips">
    {#each rows as t (t.edition)}
      {@const on = selected === t.edition}
      <button
        type="button"
        class="chip"
        class:on
        aria-pressed={on}
        style:opacity={op(t.edition)}
        onclick={() => onselect(on ? ALL : t.edition)}
      >
        <span class="sw" style:background={t.color}></span>
        <span class="nm">{t.name}</span>
        <span class="ct">{t.n}</span>
      </button>
    {/each}
  </div>
</section>

<style>
  .card {
    flex: none;
    background: var(--card);
    border: 1px solid var(--line);
    border-radius: 12px;
    padding: 14px 18px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .stack {
    display: flex;
    height: 10px;
    border-radius: 5px;
    overflow: hidden;
    gap: 2px;
  }

  .seg {
    height: 100%;
  }

  .chips {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }

  .chip {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 34px;
    padding: 0 12px 0 10px;
    border-radius: 8px;
    background: var(--card-head);
    border: 1px solid var(--line-media);
    cursor: pointer;
    font: inherit;
  }

  .chip:hover {
    border-color: var(--line-hover);
  }

  .chip.on {
    background: var(--accent-soft);
    border-color: rgba(157, 140, 255, 0.5);
  }

  .sw {
    width: 9px;
    height: 9px;
    border-radius: 2px;
  }

  .nm {
    font: 600 13px/1 var(--font);
    color: var(--text);
  }

  .ct {
    font: 700 14px/1 var(--font);
    color: var(--text);
    font-variant-numeric: tabular-nums;
  }
</style>
