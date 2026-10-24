<!--
  RECENT MATCHES (prototype 1087–1126): the newest 6 stored competitive matches. Result + score, map + when (exact in
  the tooltip), agent, K / D / A, the rank during the match, RR, and IN YOUR MATCH: players of the current live match
  who were in that game (`profile_extras.liveMates`, drawn from their live cards). The user's own rows open the
  scoreboard; another player's are not links.
-->
<script lang="ts">
  import { assetUrl } from "../../lib/assets";
  import { rowKeydown } from "../../lib/details";
  import { retryImg } from "../../lib/imgRetry";
  import { liveMateChips, RECENT_ROWS, recentRow } from "../../lib/profile";
  import { openMatch } from "../../lib/router.svelte";
  import { live, meta, now } from "../../lib/stores";
  import { tip } from "../../lib/tooltip.svelte";
  import type { MatchSummary, ProfileExtras } from "../../lib/types";
  import Skel from "../Skel.svelte";

  interface Props {
    /** null while the details load. */
    rows: MatchSummary[] | null;
    /** undefined while pending, null when it failed. */
    extras: ProfileExtras | null | undefined;
    isSelf: boolean;
    puuid: string;
  }

  let { rows, extras, isSelf, puuid }: Props = $props();

  const views = $derived(
    rows
      ? rows.slice(0, RECENT_ROWS).map((m) => ({ ...recentRow(m, $meta, $now), with: liveMateChips(m.matchId, extras ?? null, $live, puuid, $meta) }))
      : null,
  );
  const open = (matchId: string) => openMatch(matchId, { from: "profile", focusPuuid: puuid });
</script>

{#snippet cells(m: NonNullable<typeof views>[number])}
  <div class="c-res">
    <span class="res" style:background={m.resStyle.bg} style:color={m.resStyle.fg}>{m.res}</span><span class="score">{m.score}</span>
  </div>
  <div class="c-map">
    {#if m.mapId}<img {@attach retryImg} class="map" src={assetUrl("mapList", m.mapId)} alt="" />{:else}<span class="map"></span>{/if}
    <div class="mcol">
      <span class="mn">{#if m.map === undefined}<Skel w={56} h={12} />{:else}{m.map ?? "—"}{/if}</span>
      <span class="when" use:tip={m.whenTip}>{m.when}</span>
    </div>
  </div>
  {#if m.agentId}<img {@attach retryImg} class="ag" src={assetUrl("agent", m.agentId)} alt={m.agent} use:tip={m.agent} />{:else}<span class="ag"></span>{/if}
  <span class="kda">{m.kda}</span>
  <div class="c-rank" use:tip={m.rankName}>
    {#if m.tier !== null}<img {@attach retryImg} src={assetUrl("tier", m.tier >= 3 ? m.tier : 0)} alt="" />{/if}<span style:color={m.rankColor}>{m.rankShort}</span>
  </div>
  <span class="rr" style:color={m.rrColor}>{m.rr}</span>
  <div class="c-with">
    {#each m.with as w (w.puuid)}
      <span class="wchip" use:tip={w.tip}
        >{#if w.agentId}<img {@attach retryImg} src={assetUrl("agent", w.agentId)} alt="" />{:else}<span class="wx"></span>{/if}<span
          class={["wn", w.hidden && "hid"]} data-ellipsis>{w.name}</span
        ></span
      >
    {/each}
  </div>
{/snippet}

<div class="rm">
  <span class="lbl">RECENT MATCHES</span>
  <div class="card">
    <div class="row head">
      <span>RESULT</span><span>MAP</span><span>AGENT</span><span>K / D / A</span><span>RANK</span><span class="r">RR</span><span
        class="help"
        use:tip={"Players from your current live match who were in this game"}>IN YOUR MATCH</span
      ><span></span>
    </div>
    {#if views === null}
      {#each [0, 1, 2] as i (i)}
        <div class="row" aria-hidden="true">
          <Skel w={70} h={20} block /><Skel w={140} h={28} r={4} block /><Skel w={28} h={28} r={5} block /><Skel w={70} h={13} block /><Skel w={70} h={20} block /><span></span><span></span><span></span>
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

  .lbl {
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
    grid-template-columns: 82px minmax(0, 1fr) 28px 92px 104px 40px minmax(0, 1.2fr) 10px;
    column-gap: 12px;
    align-items: center;
    height: 50px;
    padding: 0 14px;
    border-bottom: 1px solid var(--line-soft);
    color: var(--text-disabled);
  }

  .row:last-child {
    border-bottom: none;
  }

  .row.head {
    height: 32px;
    background: var(--card-head);
    border-bottom: 1px solid var(--line);
    font: 500 10.5px/1 var(--font-mono);
    letter-spacing: 0.06em;
    color: var(--text-label);
  }

  .head .r {
    text-align: right;
  }

  .help {
    cursor: help;
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
    gap: 9px;
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
    gap: 10px;
    min-width: 0;
  }

  .map {
    width: 56px;
    height: 28px;
    object-fit: cover;
    border-radius: 4px;
    display: block;
    flex: none;
    background: var(--skel);
  }

  .mcol {
    display: flex;
    flex-direction: column;
    gap: 5px;
    min-width: 0;
  }

  .mn {
    font: 600 13.5px/1 var(--font);
    color: var(--text);
  }

  .when {
    font: 500 12px/1 var(--font);
    color: var(--text-label);
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
    gap: 7px;
    min-width: 0;
    font: 700 13.5px/1 var(--font);
    white-space: nowrap;
  }

  .c-rank img {
    width: 22px;
    height: 22px;
    display: block;
    flex: none;
  }

  .rr {
    font: 700 15px/1 var(--font);
    text-align: right;
    font-variant-numeric: tabular-nums;
  }

  .c-with {
    display: flex;
    gap: 5px;
    min-width: 0;
    overflow: hidden;
  }

  .wchip {
    flex: 0 1 auto;
    min-width: 0;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 2px 8px 2px 2px;
    border-radius: 5px;
    background: var(--button);
    border: 1px solid var(--line-control);
    cursor: default;
  }

  .wchip img,
  .wx {
    width: 22px;
    height: 22px;
    border-radius: 4px;
    display: block;
    flex: none;
    background: var(--skel);
  }

  .wn {
    min-width: 0;
    font: 600 12.5px/1 var(--font);
    color: var(--text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .wn.hid {
    font-style: italic;
    color: var(--text-muted);
  }
</style>
