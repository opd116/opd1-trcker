<!--
  Encounters (spec §6.6, v8:277-313): every match shared with the user, newest first, no cap. The player's side, the
  user's side ("You"), and the result and score from the user's side ("Your result"). A click opens Match Details.
  The header names the player the way the hero does (shortName; a hidden player never by their real name, R18).
-->
<script lang="ts">
  import { assetUrl } from "../../lib/assets";
  import { shortName } from "../../lib/details";
  import { dateShort, score } from "../../lib/format";
  import { isRankedTier, mapName, tierLabel } from "../../lib/meta";
  import { meta, now } from "../../lib/stores";
  import { encounterMapTip } from "../../lib/tips";
  import { tip } from "../../lib/tooltip.svelte";
  import type { DisplayName, EncounterDetailRow } from "../../lib/types";
  import ChevronRight from "../icons/ChevronRight.svelte";
  import ResultChip from "../ResultChip.svelte";
  import Skel from "../Skel.svelte";

  interface Props {
    rows: EncounterDetailRow[];
    /** The Details player's name (the card's: absent while loading, null when missing). */
    name: DisplayName | null | undefined;
    onopen: (matchId: string) => void;
  }

  let { rows, name, onopen }: Props = $props();

  const short = $derived(name === undefined ? undefined : name === null ? "—" : shortName(name, $meta));

  function onkeydown(e: KeyboardEvent, matchId: string) {
    if (e.key !== "Enter" && e.key !== " ") return;
    e.preventDefault();
    onopen(matchId);
  }
</script>

{#snippet side(agentId: string | null, tier: number | null)}
  {@const t = isRankedTier(tier) ? tier : null}
  {@const tname = t === null ? null : tierLabel($meta, t)}
  <div class="ewho">
    {#if agentId}<img class="eagent" src={assetUrl("agent", agentId)} alt="" />{:else}<div class="eagent"></div>{/if}
    {#if t !== null}<img class="etier" src={assetUrl("tier", t)} alt="" />{:else}<div class="etier"></div>{/if}
    <span class={["tname", t === null && "dash"]}>{#if tname === undefined}<Skel w={60} h={10} />{:else}{tname ?? "—"}{/if}</span>
  </div>
{/snippet}

<section class="sec encs">
  <div class="sech">Encounters</div>
  <div class="panel">
    <div class="ehead">
      <span>Map</span><span class="ename" data-ellipsis>{#if short === undefined}<Skel w={60} h={10} />{:else}{short}{/if}</span><span
      ></span><span>You</span><span>Your result</span><span></span>
    </div>
    {#each rows as e (e.matchId)}
      {@const map = mapName($meta, e.mapId)}
      <div class="erow" role="button" tabindex="0" onclick={() => onopen(e.matchId)} onkeydown={(ev) => onkeydown(ev, e.matchId)}>
        <div class="emap" use:tip={encounterMapTip(e, $now)}>
          {#if e.mapId}<img class="mimg" src={assetUrl("mapList", e.mapId)} alt="" />{:else}<div class="mimg"></div>{/if}
          <div class="mcol">
            <span class={["mname", map === null && "dash"]}>{#if map === undefined}<Skel w={60} h={10} />{:else}{map ?? "—"}{/if}</span>
            <span class="date">{dateShort(e.startMs, $now)}</span>
          </div>
        </div>
        {@render side(e.themAgentId, e.themTier)}
        <span class="eside">{e.with ? "With" : "Against"}</span>
        {@render side(e.selfAgentId, e.selfTier)}
        <div class="eres"><ResultChip result={e.result} size={20} /><span class="sc">{score(e.roundsWon, e.roundsLost)}</span></div>
        <ChevronRight />
      </div>
    {/each}
  </div>
</section>

<style>
  .sec {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .sech {
    font-size: 14px;
    font-weight: 600;
  }

  .panel {
    background: var(--card);
    border-radius: 8px;
    overflow: hidden;
  }

  .ehead,
  .erow {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 144px 58px 144px 80px 10px;
    column-gap: 12px;
    align-items: center;
    padding: 0 14px 0 8px;
  }

  .ehead {
    height: 30px;
    font-size: 12px;
    font-weight: 500;
    color: var(--text2);
    border-bottom: 1px solid var(--line);
    white-space: nowrap;
  }

  /* The design's own ellipsis span (spec §8.3 check 5 allows it). */
  .ename {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .erow {
    height: 50px;
    cursor: pointer;
  }

  .erow + .erow {
    border-top: 1px solid var(--line);
  }

  .erow:hover {
    background: var(--hover-row);
  }

  .emap {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }

  /* An unknown map keeps the box, with the image's own (no) background. */
  .mimg {
    display: block;
    width: 56px;
    height: 32px;
    flex: none;
    object-fit: cover;
    border-radius: 4px;
  }

  .mcol {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .mname {
    font-size: 14px;
    font-weight: 600;
    white-space: nowrap;
  }

  .mname.dash {
    color: var(--text2);
  }

  .date {
    font-size: 12px;
    font-weight: 500;
    color: var(--text2);
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }

  .ewho {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }

  .eagent {
    display: block;
    width: 26px;
    height: 26px;
    flex: none;
    border-radius: 5px;
    background: var(--raised);
  }

  .etier {
    display: block;
    width: 20px;
    height: 20px;
    flex: none;
  }

  /* The design's ellipsis stays as a safety net only: `Ascendant 3` fits the 144px column in every font. */
  .tname {
    min-width: 0;
    font-size: 13px;
    font-weight: 500;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* A missing rank's —, as everywhere else a rank is missing. */
  .tname.dash {
    color: var(--text2);
  }

  .eside {
    font-size: 13px;
    font-weight: 600;
    color: var(--text2);
    white-space: nowrap;
  }

  .eres {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .sc {
    font-size: 14px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
</style>
