<!--
  LOADOUT (prototype 1063–1076): Vandal, Phantom, Operator, Sheriff in the equipped chroma, tinted by edition, the
  edition swatch under the name. R97 tri-state per weapon: absent → a skeleton tile, null → `—`. `wide`: the row spans
  both columns in four tiles (another player with no encounters, prototype `loadCol`).
-->
<script lang="ts">
  import { assetUrl } from "../../lib/assets";
  import { retryImg } from "../../lib/imgRetry";
  import { loadoutTile, WEAPONS } from "../../lib/profile";
  import { tip } from "../../lib/tooltip.svelte";
  import type { Loadout } from "../../lib/types";
  import Skel from "../Skel.svelte";

  let { loadout, wide }: { loadout: Loadout | null; wide: boolean } = $props();

  const tiles = $derived(WEAPONS.map((w) => (loadout === null ? loadoutTile(w.label, undefined) : loadoutTile(w.label, loadout[w.key]))));
</script>

<div class={["lo", wide && "wide"]}>
  <span class="lbl">LOADOUT</span>
  <div class="tiles">
    {#each tiles as t (t.weapon)}
      {#if t.state === "loading"}
        <div class="tile" aria-hidden="true">
          <div class="art"><Skel w={96} h={30} r={5} block /></div>
          <div class="txt"><Skel w={110} h={13} block /><Skel w={80} h={10} block /></div>
        </div>
      {:else if t.state === "none"}
        <div class="tile">
          <div class="art"></div>
          <div class="txt"><span class="skin dash">—</span><span class="wp"><span class="sw"></span>{t.weapon}</span></div>
        </div>
      {:else}
        <div class="tile" use:tip={t.tip}>
          <div class="art" style:background={`linear-gradient(160deg, ${t.tint}, transparent 75%)`}>
            {#if t.chromaId}
              <img {@attach retryImg} src={assetUrl("chroma", t.chromaId)} alt={t.skin} />
            {:else}
              <div class="noart">NO ARTWORK</div>
            {/if}
          </div>
          <div class="txt">
            <span class="skin">{t.skin}</span>
            <span class="wp"><span class="sw" style:background={t.color}></span>{t.weapon} · {t.edition ?? "Standard"}</span>
          </div>
        </div>
      {/if}
    {/each}
  </div>
</div>

<style>
  .lo {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-width: 0;
  }

  .lo.wide {
    order: 1;
    grid-column: 1 / -1;
  }

  .lbl {
    font: 500 10.5px/1 var(--font-mono);
    letter-spacing: 0.06em;
    color: var(--text-label);
  }

  .tiles {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    gap: 8px;
  }

  .wide .tiles {
    grid-template-columns: repeat(4, minmax(0, 1fr));
  }

  .tile {
    border-radius: 8px;
    overflow: hidden;
    background: var(--card);
    border: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .art {
    height: 58px;
    box-sizing: border-box;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 8px 12px;
  }

  .art img {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
    display: block;
  }

  .noart {
    width: 100%;
    height: 100%;
    border-radius: 5px;
    border: 1px dashed var(--line-hover);
    background: repeating-linear-gradient(135deg, transparent 0 6px, rgba(255, 255, 255, 0.03) 6px 12px);
    display: flex;
    align-items: center;
    justify-content: center;
    font: 500 10.5px/1 var(--font-mono);
    letter-spacing: 0.04em;
    color: var(--text-label);
  }

  .txt {
    padding: 8px 10px 10px;
    display: flex;
    flex-direction: column;
    gap: 5px;
    border-top: 1px solid var(--line-soft);
    min-width: 0;
  }

  .skin {
    font: 600 13px/1.2 var(--font);
    color: var(--text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .skin.dash {
    color: var(--text-muted);
  }

  .wp {
    display: flex;
    align-items: center;
    gap: 6px;
    font: 500 11.5px/1 var(--font);
    color: var(--text-3);
    white-space: nowrap;
  }

  .sw {
    width: 7px;
    height: 7px;
    border-radius: 2px;
    background: var(--text-dim);
    flex: none;
  }
</style>
