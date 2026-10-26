<!--
  Collection · one skin (design v2 TrackerApp.dc.html 845–856): the skin level's art on a radial tint of its edition
  colour (a missing level or an image that fails to load: the dashed "ARTWORK UNAVAILABLE" frame), the name clamped to
  two lines with its tooltip, and the footer: edition swatch (tooltip: the edition), VP and ≈ SAR (tooltip: the rate).
-->
<script lang="ts">
  import { assetUrl } from "../../lib/assets";
  import { IMG_RETRY_MS, retryImg } from "../../lib/imgRetry";
  import { artTint, editionColor, editionLabel, priceTip, sarText, vpText } from "../../lib/skins";
  import { tip } from "../../lib/tooltip.svelte";
  import type { OwnedSkin } from "../../lib/types";

  /** `name`: the full name ("Reaver Vandal", `skinName`). */
  let { skin, name }: { skin: OwnedSkin; name: string } = $props();

  /**
   * R163: Rust answers 503 while a valorant-api download fails (offline, a cold start), and `retryImg` asks again on
   * its backoff. The frame says "unavailable" only once those retries are spent; until then the art just isn't drawn.
   */
  let errors = $state(0);
  const broken = $derived(errors > IMG_RETRY_MS.length);
  const hasArt = $derived(!!skin.levelId && !broken);
</script>

<div class="card">
  <div class="art" style:background="radial-gradient(120% 100% at 50% 40%,{artTint(skin.edition)},transparent 72%)">
    {#if hasArt}
      <img
        {@attach retryImg}
        src={assetUrl("skinLevel", skin.levelId)}
        alt={name}
        class:waiting={errors > 0}
        onload={() => (errors = 0)}
        onerror={() => errors++}
      />
    {:else}
      <div class="noart">
        <span class="na1">ARTWORK UNAVAILABLE</span>
        <span class="na2">Not in the local asset cache</span>
      </div>
    {/if}
  </div>
  <div class="body">
    <span class="name" data-ellipsis use:tip={name}>{name}</span>
    <div class="foot" use:tip={priceTip(skin.vp)}>
      <span class="vpw">
        <span class="sw" style:background={editionColor(skin.edition)} use:tip={skin.edition ? editionLabel(skin.edition) : "No edition"}></span>
        <span class="vp">{vpText(skin.vp)}</span>
      </span>
      <span class="sar">{sarText(skin.vp)}</span>
    </div>
  </div>
</div>

<style>
  .card {
    border-radius: 9px;
    overflow: hidden;
    background: var(--card-head);
    border: 1px solid var(--line-card);
    display: flex;
    flex-direction: column;
  }

  .art {
    height: 76px;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 12px 16px;
  }

  .art img.waiting {
    visibility: hidden;
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
    box-sizing: border-box;
    border-radius: 6px;
    border: 1px dashed var(--line-hover);
    background: repeating-linear-gradient(135deg, transparent 0 6px, rgba(255, 255, 255, 0.03) 6px 12px);
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 5px;
    text-align: center;
  }

  .na1 {
    font: 500 10.5px/1 var(--font-mono);
    letter-spacing: 0.04em;
    color: var(--text-muted);
  }

  .na2 {
    font: 500 11.5px/1 var(--font);
    color: var(--text-tag);
  }

  .body {
    padding: 9px 12px 11px;
    display: flex;
    flex-direction: column;
    gap: 7px;
    border-top: 1px solid var(--line);
  }

  .name {
    font: 600 13.5px/1.25 var(--font);
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    min-height: 34px;
  }

  .foot {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 8px;
    padding-top: 8px;
    border-top: 1px solid var(--line);
    cursor: default;
  }

  .vpw {
    display: flex;
    align-items: center;
    gap: 7px;
  }

  .sw {
    width: 9px;
    height: 9px;
    border-radius: 2px;
    flex: none;
  }

  .vp {
    font: 700 13.5px/1 var(--font);
    color: var(--text);
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }

  .sar {
    font: 600 12.5px/1 var(--font);
    color: var(--text-3);
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }
</style>
