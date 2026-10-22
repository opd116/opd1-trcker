<!--
  Stored competitive matches with the user (prototype 1030–1048): map, the user's result, `Same team` / `Against you`,
  the date (exact in the tooltip), both players' agent + rank + score, a chevron. A row opens the scoreboard from the
  user's side, focused on this player. None stored: one line across both columns ([C]: the prototype's `noEncText`).
-->
<script lang="ts">
  import { assetUrl } from "../../lib/assets";
  import { rowKeydown } from "../../lib/details";
  import { retryImg } from "../../lib/imgRetry";
  import { encounterView, type NameView } from "../../lib/profile";
  import { openMatch } from "../../lib/router.svelte";
  import { meta, now } from "../../lib/stores";
  import { tip } from "../../lib/tooltip.svelte";
  import type { EncounterDetailRow } from "../../lib/types";
  import Skel from "../Skel.svelte";

  interface Props {
    /** null while the details load. */
    rows: EncounterDetailRow[] | null;
    puuid: string;
    them: NameView | undefined;
    /** While loading: hold the card's place (the live card counts encounters). */
    expected: boolean;
  }

  let { rows, puuid, them, expected }: Props = $props();

  const views = $derived(rows ? rows.map((e) => encounterView(e, them, $meta, $now)) : null);
  const open = (matchId: string) => openMatch(matchId, { from: "encounter", focusPuuid: puuid });
  const tierIcon = (t: number) => assetUrl("tier", t >= 3 ? t : 0);
</script>

{#snippet side(s: { agentId: string | null; tier: number | null; score: string; scoreColor: string; tip: string }, cls: string)}
  <div class="side">
    {#if s.agentId}<img {@attach retryImg} class={["ag", cls]} src={assetUrl("agent", s.agentId)} alt="" use:tip={s.tip} />{:else}<span
        class={["ag", cls]}
        use:tip={s.tip}
      ></span>{/if}
    <span class="rk"
      >{#if s.tier !== null}<img {@attach retryImg} src={tierIcon(s.tier)} alt="" />{/if}<span class="sc" style:color={s.scoreColor}>{s.score}</span></span
    >
  </div>
{/snippet}

{#if views === null}
  {#if expected}
    <div class="card" aria-hidden="true">
      {#each [0, 1] as i (i)}
        <div class="row sk"><Skel w={64} h={40} r={5} block /><div class="skc"><Skel w={90} h={13} block /><Skel w={120} h={11} block /></div></div>
      {/each}
    </div>
  {/if}
{:else if views.length === 0}
  <div class="card none">No stored matches with {them?.ref ?? "this player"} before this one.</div>
{:else}
  <div class="card">
    {#each views as e (e.matchId)}
      <div class="row" data-row="1" role="button" tabindex="0" aria-label={e.aria} onclick={() => open(e.matchId)} onkeydown={rowKeydown(() => open(e.matchId))}>
        {#if e.mapId}<img {@attach retryImg} class="map" src={assetUrl("mapList", e.mapId)} alt="" />{:else}<span class="map"></span>{/if}
        <div class="mid">
          <div class="l1">
            <span class="mn">{#if e.map === undefined}<Skel w={60} h={12} />{:else}{e.map ?? "—"}{/if}</span>
            <span class="res" style:background={e.resStyle.bg} style:color={e.resStyle.fg}>{e.res}</span>
          </div>
          <div class="l2">
            <span class={["sidechip", e.with ? "with" : "vs"]}>{e.with ? "Same team" : "Against you"}</span>
            <span class="date" use:tip={e.dateTip}>{e.date}</span>
          </div>
        </div>
        <div class="who">
          {@render side(e.self, "me")}
          <span class="join">{e.with ? "&" : "vs"}</span>
          {@render side(e.them, e.with ? "mate" : "foe")}
        </div>
        <svg width="7" height="12" viewBox="0 0 7 12" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" aria-hidden="true"
          ><path d="M1.5 1.5L5.5 6l-4 4.5" /></svg
        >
      </div>
    {/each}
  </div>
{/if}

<style>
  .card {
    border: 1px solid var(--line);
    border-radius: 10px;
    overflow: hidden;
    background: var(--card);
  }

  .none {
    grid-column: 1 / -1;
    padding: 14px;
    font: 500 13px/1.45 var(--font);
    color: var(--text-muted);
  }

  .row {
    display: grid;
    grid-template-columns: 64px minmax(0, 1fr) auto 8px;
    column-gap: 12px;
    align-items: center;
    padding: 10px 12px;
    border-bottom: 1px solid var(--line-soft);
    cursor: pointer;
    color: var(--text-disabled);
  }

  .row:last-child {
    border-bottom: none;
  }

  .row:hover {
    background: var(--row-hover);
  }

  .row.sk {
    display: flex;
    gap: 12px;
    cursor: default;
  }

  .row.sk:hover {
    background: none;
  }

  .skc {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .map {
    width: 64px;
    height: 40px;
    object-fit: cover;
    border-radius: 5px;
    display: block;
    background: var(--skel);
  }

  .mid {
    display: flex;
    flex-direction: column;
    gap: 7px;
    min-width: 0;
  }

  .l1 {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .mn {
    font: 600 14px/1 var(--font);
    color: var(--text);
  }

  .res {
    width: 20px;
    height: 20px;
    border-radius: 4px;
    display: flex;
    align-items: center;
    justify-content: center;
    font: 700 10.5px/1 var(--font-mono);
  }

  .l2 {
    display: flex;
    align-items: center;
    gap: 8px;
    white-space: nowrap;
  }

  .sidechip {
    font: 600 11.5px/1 var(--font);
    padding: 2px 6px;
    border-radius: 4px;
    border: 1px solid;
  }

  .sidechip.with {
    color: color-mix(in srgb, var(--ally) 72%, var(--text));
    border-color: color-mix(in srgb, var(--ally) 45%, transparent);
  }

  .sidechip.vs {
    color: color-mix(in srgb, var(--enemy) 80%, var(--text));
    border-color: color-mix(in srgb, var(--enemy) 45%, transparent);
  }

  .date {
    font: 500 12px/1 var(--font);
    color: var(--text-label);
  }

  .who {
    display: flex;
    align-items: flex-start;
    gap: 6px;
  }

  .side {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
  }

  .ag {
    width: 28px;
    height: 28px;
    box-sizing: border-box;
    border-radius: 6px;
    display: block;
    background: var(--skel);
    border: 1px solid;
  }

  .ag.me {
    border-color: color-mix(in srgb, var(--accent) 60%, transparent);
  }

  .ag.mate {
    border-color: color-mix(in srgb, var(--ally) 60%, transparent);
  }

  .ag.foe {
    border-color: color-mix(in srgb, var(--enemy) 60%, transparent);
  }

  .rk {
    display: flex;
    align-items: center;
    gap: 3px;
  }

  .rk img {
    width: 14px;
    height: 14px;
    display: block;
  }

  .sc {
    font: 700 12.5px/1 var(--font);
    font-variant-numeric: tabular-nums;
  }

  .join {
    font: 600 11px/1 var(--font);
    color: var(--text-label);
    width: 14px;
    text-align: center;
    margin-top: 9px;
  }
</style>
