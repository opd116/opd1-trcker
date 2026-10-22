<!--
  The live board's header card (design v2 TrackerApp.dc.html 153–175): the map tile, the Agent Select chip with its
  countdown and lock count, LOADING PLAYERS n / m while cards are still loading, the two teams' average win rates,
  and the Glance mode button. The design draws it at 1280 px; with every block shown at once (Agent Select while the
  players load) it is wider than that, so the map tile and the loading bar give up width first (down to what their
  text needs), then the matchup is cut at its right edge. The Glance mode button never shrinks.
-->
<script lang="ts">
  import { assetUrl } from "../../lib/assets";
  import { retryImg } from "../../lib/imgRetry";
  import type { Matchup } from "../../lib/livev2";
  import Shimmer from "./Shimmer.svelte";

  interface Props {
    mapId: string | null;
    /** undefined while meta is pending; null when the map is unknown. */
    mapName: string | null | undefined;
    /** Agent Select only. */
    pregame: { left: string | null; locked: number; total: number } | null;
    progress: { loaded: number; total: number } | null;
    matchup: Matchup[];
    onglance: () => void;
  }

  let { mapId, mapName, pregame, progress, matchup, onglance }: Props = $props();
</script>

<div class="head">
  <div class="map">
    {#if mapId}<img {@attach retryImg} src={assetUrl("mapList", mapId)} alt="" />{/if}
    <div class="fade"></div>
    {#if mapName === undefined}
      <span class="map-sk"><Shimmer w={90} h={18} /></span>
    {:else}
      <span class="map-name">{mapName ?? "—"}</span>
    {/if}
  </div>
  {#if pregame}
    <div class="vr short"></div>
    <div class="select">
      <span class="chip">AGENT SELECT</span>
      {#if pregame.left !== null}<span class="timer" role="timer">{pregame.left}</span>{/if}
      <span class="locked">{pregame.locked} OF {pregame.total} LOCKED</span>
    </div>
  {/if}
  <div class="vr"></div>
  {#if progress}
    <div class="loading" role="progressbar" aria-label="Loading players" aria-valuemin={0} aria-valuemax={progress.total} aria-valuenow={progress.loaded}>
      <div class="ld-top"><span>LOADING PLAYERS</span><span class="ld-n">{progress.loaded} / {progress.total}</span></div>
      <div class="ld-track">
        <div class="ld-fill" style:width="{Math.round((progress.loaded / Math.max(1, progress.total)) * 100)}%"></div>
        <div class="ld-sheen"></div>
      </div>
    </div>
    <div class="vr short"></div>
  {/if}
  <div class="matchup">
    {#each matchup as m, i (m.label)}
      {#if i > 0}<span class="vs">VS</span>{/if}
      <div class="side" style:opacity={m.hidden ? 0.35 : 1}>
        <span class="side-bar" style:background={m.color}></span>
        <div class="side-col">
          <span class="side-label" style:color={m.color}>{m.label}</span>
          <span class="side-wr"><span class="wr">{m.wr}</span><span class="wr-l">win rate</span></span>
        </div>
      </div>
    {/each}
  </div>
  <button type="button" class="glance" aria-label="Open glance mode" onclick={onglance}
    ><svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true"
      ><rect x="1.5" y="2" width="13" height="12" rx="1.5"></rect><path d="M10 2v12"></path></svg
    >Glance mode</button
  >
</div>

<style>
  .head {
    height: 62px;
    flex: none;
    display: flex;
    align-items: center;
    gap: 20px;
    padding: 0 10px;
    background: var(--card);
    border: 1px solid var(--line);
    border-radius: 10px;
  }

  .map {
    position: relative;
    flex: 0 4 210px;
    min-width: 128px;
    height: 44px;
    border-radius: 7px;
    overflow: hidden;
    background: var(--row-hover);
  }

  .map img {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
    opacity: 0.75;
  }

  .fade {
    position: absolute;
    inset: 0;
    background: linear-gradient(90deg, rgba(12, 13, 16, 0.92) 15%, rgba(12, 13, 16, 0.2));
  }

  .map-name,
  .map-sk {
    position: absolute;
    left: 14px;
    top: 50%;
    transform: translateY(-50%);
  }

  .map-name {
    font: 700 22px/1 var(--font-display);
    letter-spacing: 0.08em;
    text-transform: uppercase;
    white-space: nowrap;
    max-width: calc(100% - 16px);
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .vr {
    width: 1px;
    height: 38px;
    background: var(--track);
    flex: none;
  }

  .vr.short {
    height: 30px;
  }

  .select {
    display: flex;
    align-items: center;
    gap: 14px;
    flex: none;
  }

  .chip {
    font: 700 12px/1 var(--font-display);
    letter-spacing: 0.14em;
    color: var(--accent-text);
    padding: 6px 9px;
    border: 1px solid rgba(157, 140, 255, 0.4);
    border-radius: 5px;
    background: rgba(157, 140, 255, 0.08);
    white-space: nowrap;
  }

  .timer {
    font: 700 28px/1 var(--font-display);
    font-variant-numeric: tabular-nums;
  }

  .locked {
    font: 500 10.5px/1 var(--font-mono);
    letter-spacing: 0.08em;
    color: var(--text-muted);
    white-space: nowrap;
  }

  .loading {
    display: flex;
    flex-direction: column;
    gap: 6px;
    flex: 0 2 180px;
    min-width: 150px;
  }

  .ld-top {
    display: flex;
    justify-content: space-between;
    gap: 6px;
    white-space: nowrap;
    font: 500 10.5px/1 var(--font-mono);
    letter-spacing: 0.08em;
    color: var(--text-dim);
  }

  .ld-n {
    color: var(--text);
  }

  .ld-track {
    position: relative;
    height: 4px;
    border-radius: 2px;
    background: var(--track);
    overflow: hidden;
  }

  .ld-fill {
    position: absolute;
    left: 0;
    top: 0;
    bottom: 0;
    background: var(--accent);
    border-radius: 2px;
    transition: width 0.3s;
  }

  .ld-sheen {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 30%;
    background: linear-gradient(90deg, transparent, rgba(255, 255, 255, 0.35), transparent);
    animation: opdShimmer 1.3s ease-in-out infinite;
  }

  .matchup {
    display: flex;
    align-items: center;
    gap: 18px;
    /* Shrinks only once the map tile and the loading bar are down to their minimum. */
    flex: 0 0.01 auto;
    min-width: 0;
    overflow: hidden;
  }

  .vs,
  .side {
    flex: none;
  }

  .vs {
    font: 700 13px/1 var(--font-display);
    letter-spacing: 0.14em;
    color: var(--text-ghost);
  }

  .side {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .side-bar {
    width: 3px;
    height: 32px;
    border-radius: 2px;
  }

  .side-col {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .side-label {
    font: 700 12px/1 var(--font-display);
    letter-spacing: 0.12em;
    white-space: nowrap;
  }

  .side-wr {
    display: flex;
    align-items: baseline;
    gap: 5px;
    white-space: nowrap;
  }

  .wr {
    font: 700 19px/1 var(--font);
    color: var(--text);
  }

  .wr-l {
    font: 500 12px/1 var(--font);
    color: var(--text-label);
  }

  .glance {
    height: 34px;
    padding: 0 12px;
    border-radius: 8px;
    background: var(--button);
    color: var(--text);
    border: 1px solid var(--line-control);
    font: 600 13px/1 var(--font);
    cursor: pointer;
    display: flex;
    align-items: center;
    gap: 8px;
    flex: none;
    /* Pushed to the right end (no spacer element, so a crowded header does not spend a gap on it). */
    margin-left: auto;
  }

  .glance:hover {
    background: var(--button-hover);
    border-color: var(--line-hover);
  }
</style>
