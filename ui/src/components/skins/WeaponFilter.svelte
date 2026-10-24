<!--
  Collection · BY WEAPON (design v2 TrackerApp.dc.html 808–822): "All weapons" first, then each weapon the user owns a
  skin for, most first; a 34px row of weapon art, name, bar and count. Selecting a row filters the grid. The list
  scrolls inside its card.
-->
<script lang="ts">
  import { assetUrl } from "../../lib/assets";
  import { retryImg } from "../../lib/imgRetry";
  import { ALL, type WeaponRow } from "../../lib/skins";

  let { rows, selected, onselect }: { rows: WeaponRow[]; selected: string; onselect: (id: string) => void } = $props();

</script>

<section class="card" aria-label="By weapon">
  <div class="hdr">
    <span class="micro">BY WEAPON</span>
    <span class="lab">Select to filter</span>
  </div>
  <div class="list">
    {#each rows as w (w.id)}
      {@const on = selected === w.id}
      <button type="button" class="row" class:on aria-pressed={on} onclick={() => onselect(w.id)}>
        <span class="art">
          {#if w.id !== ALL}
            <!-- R163: a failed load is asked again (`retryImg`); until it loads the slot looks empty (no alt text). -->
            <img {@attach retryImg} src={assetUrl("weaponIcon", w.id)} alt="" />
          {/if}
        </span>
        <span class="name" data-ellipsis>{w.name}</span>
        <span class="track"><span class="bar" style:width="{w.w}%"></span></span>
        <span class="n">{w.n}</span>
      </button>
    {/each}
  </div>
</section>

<style>
  .card {
    flex: 1;
    background: var(--card);
    border: 1px solid var(--line);
    border-radius: 12px;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }

  .hdr {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    padding: 16px 16px 10px;
  }

  .micro {
    font: 500 10.5px/1 var(--font-mono);
    letter-spacing: 0.06em;
    color: var(--text-muted);
  }

  .lab {
    font: 500 12.5px/1 var(--font);
    color: var(--text-muted);
  }

  .list {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 0 8px 10px;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .row {
    flex: none;
    display: grid;
    grid-template-columns: 56px minmax(0, 1fr) 52px 24px;
    column-gap: 10px;
    align-items: center;
    height: 34px;
    padding: 0 8px;
    border-radius: 6px;
    background: transparent;
    border: 1px solid transparent;
    color: var(--text-mid);
    cursor: pointer;
    text-align: left;
    font: inherit;
  }

  .row:hover {
    background: var(--row-hover);
  }

  .row.on {
    background: var(--accent-soft);
    border-color: rgba(157, 140, 255, 0.4);
    color: var(--text);
  }

  .art {
    height: 18px;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .art img {
    max-width: 56px;
    max-height: 18px;
    object-fit: contain;
    display: block;
    opacity: 0.8;
  }

  .name {
    font: 600 13.5px/1 var(--font);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .track {
    height: 5px;
    border-radius: 3px;
    background: var(--button-hover);
    overflow: hidden;
  }

  .bar {
    display: block;
    height: 100%;
    background: var(--accent);
    border-radius: 3px;
  }

  .n {
    font: 600 13.5px/1 var(--font);
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
</style>
