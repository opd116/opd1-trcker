<!--
  Collection (design v2 TrackerApp.dc.html 793–861): the user's owned skins from `invoke("collection")`. Header with the
  total store value in VP and ≈ SAR; BY WEAPON on the left (filters the grid); the edition card (stacked bar + toggle
  chips) above the skin grid, sorted by value then name. A filter pair with no skins says so. `null` (no refresh was
  attempted yet: the game has not run): the dashed empty state in the design's empty-list style. While the answer is
  pending the frames draw skeletons; a failed answer (a refresh attempt left nothing stored, or the skin catalog could
  not be built) leaves them empty. Either of those two is asked again every minute and when the live stage changes.
  The page fills <main>; the weapon list and the grid scroll inside their cards.
-->
<script lang="ts">
  import { untrack } from "svelte";
  import { collection } from "../lib/api";
  import EditionCard from "../components/skins/EditionCard.svelte";
  import SkinCard from "../components/skins/SkinCard.svelte";
  import WeaponFilter from "../components/skins/WeaponFilter.svelte";
  import { scrollHost } from "../lib/router.svelte";
  import { live } from "../lib/stores";
  import { ALL, editionRows, fmtInt, sar, skinNames, totalVp, VALUE_TIP, visibleSkins, weaponRows } from "../lib/skins";
  import { tip } from "../lib/tooltip.svelte";
  import type { Collection, Edition } from "../lib/types";
  import { reportError } from "../lib/uilog";
  import Skel from "../components/Skel.svelte";

  /** Rust waits 60s before it builds the catalog again after a failure: asking more often brings nothing new. */
  const RETRY_MS = 60_000;

  let coll = $state<Collection | null | undefined>(undefined);
  /** The last answer failed: not "never fetched", so the empty state would be false. */
  let failed = $state(false);
  let weapon = $state<string>(ALL);
  let edition = $state<Edition | typeof ALL>(ALL);
  /** Bumped to ask again. */
  let ask = $state(0);

  $effect(() => {
    void ask;
    let alive = true;
    collection()
      .then((c) => {
        if (!alive) return;
        coll = c;
        failed = false;
      })
      .catch((err) => {
        reportError("collection", err);
        if (alive) failed = true;
      });
    return () => {
      alive = false;
    };
  });

  /** No collection to show yet (never fetched, or the answer failed): asked again. */
  const missing = $derived(failed || coll === null);
  $effect(() => {
    if (!missing) return;
    const id = setInterval(() => ask++, RETRY_MS);
    return () => clearInterval(id);
  });
  // VALORANT started or a match ended: Rust may have fetched the owned list meanwhile.
  let lastStage: string | null | undefined;
  $effect(() => {
    const st = $live?.stage ?? null;
    if (lastStage !== undefined && st !== lastStage && untrack(() => missing)) ask++;
    lastStage = st;
  });

  const items = $derived(coll?.items ?? []);
  const total = $derived(totalVp(items));
  const wRows = $derived(coll ? weaponRows(items, coll.weapons) : []);
  const eRows = $derived(editionRows(items));
  const names = $derived(skinNames(items, coll?.weapons ?? []));
  const list = $derived(visibleSkins(items, weapon, edition, names));
</script>

<div class="screen">
  <header class="head">
    <h1>Collection</h1>
    <span class="flex"></span>
    {#if coll === undefined && !failed}
      <div class="value"><Skel w={140} h={28} block /><Skel w={90} h={14} block /></div>
    {:else if coll}
      <div class="value" use:tip={VALUE_TIP}>
        <span class="vp"><span class="num">{fmtInt(total)}</span><span class="unit">VP</span></span>
        <span class="sar">≈ {fmtInt(sar(total))} SAR</span>
      </div>
    {/if}
  </header>

  {#if coll === null && !failed}
    <div class="empty">
      <div class="emptyin">
        <div class="ewell">
          <svg width="22" height="22" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round"
            ><path d="M3 3h6v6H3zM11 3h6v6h-6zM3 11h6v6H3zM11 11h6v6h-6z"></path></svg
          >
        </div>
        <span class="etitle">No collection yet</span>
        <span class="ebody">Open this page while VALORANT is running to load your collection. Every skin you own shows up here with its store value.</span>
      </div>
    </div>
  {:else}
    <div class="cols">
      <div class="left">
        {#if !coll && failed}
          <section class="card skcard"></section>
        {:else if !coll}
          <section class="card skcard" aria-busy="true">
            {#each [0, 1, 2, 3, 4, 5, 6, 7] as i (i)}<Skel w={240} h={14} block />{/each}
          </section>
        {:else}
          <WeaponFilter rows={wRows} selected={weapon} onselect={(id) => (weapon = id)} />
        {/if}
      </div>
      <div class="right">
        {#if !coll && failed}
          <section class="card tiers empty-tiers"></section>
        {:else if !coll}
          <section class="card tiers" aria-busy="true"><Skel w={400} h={10} block /><Skel w={300} h={34} block /></section>
        {:else}
          <EditionCard rows={eRows} selected={edition} onselect={(e) => (edition = e)} />
        {/if}
        <section class="card gridcard" aria-label="Skins">
          <div class="scroll" use:scrollHost>
            {#if !coll && failed}
              <!-- Nothing to show yet: the frame stays empty (no status text) until an answer comes. -->
            {:else if !coll}
              <div class="grid" aria-busy="true">
                {#each [0, 1, 2, 3, 4, 5, 6, 7] as i (i)}<div class="skelcard"></div>{/each}
              </div>
            {:else if list.length === 0}
              <div class="none">{items.length === 0 ? "No skins owned yet." : "No skins with these filters."}</div>
            {:else}
              <div class="grid">
                {#each list as s (s.skinId)}<SkinCard skin={s} name={names.get(s.skinId) ?? s.name} />{/each}
              </div>
            {/if}
          </div>
        </section>
      </div>
    </div>
  {/if}
</div>

<style>
  .screen {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    padding: 18px 24px 18px;
    gap: 14px;
  }

  .head {
    display: flex;
    align-items: flex-end;
    gap: 16px;
    flex: none;
  }

  h1 {
    margin: 0;
    font: 700 32px/1 var(--font-display);
    letter-spacing: 0.03em;
    text-transform: uppercase;
  }

  .flex {
    flex: 1;
  }

  .value {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 7px;
    cursor: help;
  }

  .vp {
    display: flex;
    align-items: baseline;
    gap: 6px;
  }

  .num {
    font: 700 28px/1 var(--font);
    font-variant-numeric: tabular-nums;
  }

  .unit {
    font: 600 14px/1 var(--font);
    color: var(--text-muted);
  }

  .sar {
    font: 600 14px/1 var(--font);
    color: var(--text-2);
    font-variant-numeric: tabular-nums;
  }

  .cols {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: 290px minmax(0, 1fr);
    gap: 12px;
  }

  .left,
  .right {
    display: flex;
    flex-direction: column;
    gap: 12px;
    min-height: 0;
    min-width: 0;
  }

  .card {
    background: var(--card);
    border: 1px solid var(--line);
    border-radius: 12px;
  }

  .skcard {
    flex: 1;
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 20px;
  }

  .tiers {
    flex: none;
    padding: 14px 18px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  /* The skeleton's height, so the grid card below does not move when the answer comes. */
  .empty-tiers {
    height: 86px;
  }

  .gridcard {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  .scroll {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 12px;
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(178px, 1fr));
    gap: 10px;
  }

  .skelcard {
    height: 160px;
    border-radius: 9px;
    background: var(--skel);
  }

  .none {
    padding: 40px 20px;
    text-align: center;
    font: 500 14px/1.5 var(--font);
    color: var(--text-muted);
  }

  .empty {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    border: 1px dashed var(--line-control);
    border-radius: 12px;
  }

  .emptyin {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 16px;
    max-width: 420px;
    text-align: center;
  }

  .ewell {
    width: 56px;
    height: 56px;
    border-radius: 14px;
    border: 1px solid var(--line-control);
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-muted);
  }

  .etitle {
    font: 700 24px/1 var(--font-display);
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }

  .ebody {
    font: 500 14px/1.55 var(--font);
    color: var(--text-dim);
    text-wrap: pretty;
  }
</style>
