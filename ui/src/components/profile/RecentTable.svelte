<!--
  RECENT MATCHES (R209 compact half-width panel): the newest 6 stored competitive matches. Result + score, map + when (exact in
  the tooltip), agent, K / D / A, and the rank during the match. The recent-match RR and IN YOUR MATCH columns the
  owner screenshot removed do not show (their data stays available in `profile_extras`). The user's own rows open the
  scoreboard; another player's are not links.
-->
<script lang="ts">
  import { assetUrl } from "../../lib/assets";
  import { rowKeydown } from "../../lib/details";
  import { retryImg } from "../../lib/imgRetry";
  import { RECENT_ROWS, recentRow } from "../../lib/profile";
  import { openMatch } from "../../lib/router.svelte";
  import { meta, now } from "../../lib/stores";
  import { tip } from "../../lib/tooltip.svelte";
  import type { MatchSummary, ProfileExtras } from "../../lib/types";
  import Skel from "../Skel.svelte";

  interface Props {
    /** null while the details load. */
    rows: MatchSummary[] | null;
    /**
     * Accepted from the drawer, but the IN YOUR MATCH column that consumed it is owner-removed, so nothing here
     * renders it yet.
     */
    extras: ProfileExtras | null | undefined;
    isSelf: boolean;
    puuid: string;
  }

  let { rows, isSelf, puuid }: Props = $props();

  const views = $derived(rows ? rows.slice(0, RECENT_ROWS).map((m) => recentRow(m, $meta, $now)) : null);
  const open = (matchId: string) => openMatch(matchId, { from: "profile", focusPuuid: puuid });
</script>

{#snippet cells(m: NonNullable<typeof views>[number])}
  <div class="c-res">
    <span class="res" style:background={m.resStyle.bg} style:color={m.resStyle.fg}>{m.res}</span><span class="score">{m.score}</span>
  </div>
  <div class="c-map">
    {#if m.mapId}<img {@attach retryImg} class="map" src={assetUrl("mapList", m.mapId)} alt="" />{:else}<span class="map"></span>{/if}
    <div class="mcol">
      <span class="mn" data-ellipsis>{#if m.map === undefined}<Skel w={56} h={12} />{:else}{m.map ?? "—"}{/if}</span>
      <span class="when" use:tip={m.whenTip}>{m.when}</span>
    </div>
  </div>
  <div class="performance">
    {#if m.agentId}<img {@attach retryImg} class="ag" src={assetUrl("agent", m.agentId)} alt={m.agent} use:tip={m.agent} />{:else}<span class="ag"></span>{/if}
    <span class="kda" use:tip={'K / D / A'}>{m.kda}</span>
  </div>
  <div class="c-rank" use:tip={m.rankName}>
    {#if m.tier !== null}<img {@attach retryImg} src={assetUrl("tier", m.tier >= 3 ? m.tier : 0)} alt="" />{/if}<span style:color={m.rankColor}>{m.rankShort}</span>
  </div>
{/snippet}

<div class="rm">
  <span class="lbl mlabel">RECENT MATCHES</span>
  <div class="card">
    {#if views === null}
      {#each [0, 1, 2] as i (i)}
        <div class="row" aria-hidden="true">
          <Skel w={54} h={20} block /><Skel w={90} h={28} r={4} block /><Skel w={50} h={20} block /><span></span>
        </div>
      {/each}
    {:else if views.length === 0}
      <div class="row empty"><span>—</span></div>
    {:else if isSelf}
      {#each views as m (m.matchId)}
        <div class="row link" data-row="1" role="button" tabindex="0" aria-label={m.aria} onclick={() => open(m.matchId)} onkeydown={rowKeydown(() => open(m.matchId))}>
          {@render cells(m)}
          <svg width="7" height="12" viewBox="0 0 7 12" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" aria-hidden="true"
            ><path d="M1.5 1.5L5.5 6l-4 4.5" /></svg
          >
        </div>
      {/each}
    {:else}
      {#each views as m (m.matchId)}
        <div class="row">
          {@render cells(m)}
          <span></span>
        </div>
      {/each}
    {/if}
  </div>
</div>

<style>
  .rm {
    flex: none;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .lbl,
  .mlabel {
    font: 500 10.5px/1 var(--font-mono);
    letter-spacing: 0.06em;
    color: var(--text-label);
  }

  .card {
    border: 1px solid var(--line);
    border-radius: 10px;
    overflow: hidden;
    background: var(--card);
  }

  .row {
    display: grid;
     grid-template-columns: 62px minmax(0, 1fr) 62px 8px;
    column-gap: 6px;
    align-items: center;
    min-height: 58px;
    padding: 0 10px;
    border-bottom: 1px solid var(--line-soft);
    color: var(--text-disabled);
  }

  .row:last-child {
    border-bottom: none;
  }

  .row.link {
    cursor: pointer;
  }

  .row.link:hover {
    background: var(--row-hover);
  }

  .row.empty {
    display: flex;
    color: var(--text-muted);
    font: 500 14px/1 var(--font);
  }

  .c-res {
    display: flex;
    align-items: center;
    gap: 5px;
  }

  .res {
    width: 22px;
    height: 22px;
    flex: none;
    border-radius: 4px;
    display: flex;
    align-items: center;
    justify-content: center;
    font: 700 11px/1 var(--font-mono);
  }

  .score {
    font: 700 15px/1 var(--font);
    color: var(--text);
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }

  .c-map {
    display: flex;
    align-items: center;
    position: relative;
    height: 48px;
    min-height: 48px;
    overflow: hidden;
    border-radius: 4px;
    min-width: 0;
  }

  .map {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
    border-radius: 4px;
    display: block;
    flex: none;
    background: var(--skel);
  }

  .mcol {
    position: relative;
    width: 100%;
    height: 100%;
    justify-content: center;
    padding: 5px 7px;
    background: linear-gradient(90deg, rgba(12, 11, 12, .9), rgba(12, 11, 12, .4));
    display: flex;
    flex-direction: column;
    gap: 5px;
    min-width: 0;
  }

  .mn {
    font: 600 13.5px/18px var(--font);
    color: var(--text);
  }

  .when {
    font: 500 12px/1 var(--font);
    color: var(--text-2);
    white-space: nowrap;
  }

  .ag {
    width: 28px;
    height: 28px;
    border-radius: 5px;
    display: block;
    background: var(--skel);
  }

  .kda {
    font: 500 14px/1 var(--font);
    color: var(--text-soft);
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }

  .c-rank {
    display: flex;
    align-items: center;
    gap: 3px;
    min-width: 0;
    font: 700 13.5px/1 var(--font);
    white-space: nowrap;
  }

  .c-rank img {
    width: 18px;
    height: 18px;
    display: block;
    flex: none;
  }

  .row:nth-child(even) { background-color: var(--surface); }
  .row.link:hover { background-color: var(--row-hover); }
  .mn { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .kda { text-align: center; }
  /* R210: compact panel, natural vertical flow without internal scrolling, map with 48px headroom */
  .row{row-gap:7px;padding:8px 10px}
  .c-map{grid-column:1 / -1;grid-row:1;height:48px;min-height:48px}
  .c-res{grid-column:1;grid-row:2}
  .performance{grid-column:2;grid-row:2;display:flex;align-items:center;gap:5px;min-width:0}
  .performance .ag{width:22px;height:22px;flex:none}
  .c-rank{grid-column:3;grid-row:2}
  .row>svg,.row>span:last-child{grid-column:4;grid-row:2}
</style>
