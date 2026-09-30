<!--
  The player card (spec §3.2–§3.10): identical in every Live stage. Grid 40 / 18 / 1fr rows, 1fr + 158px columns.
  Click anywhere opens Player Details (tooltip triggers inside do not stop it).
-->
<script lang="ts">
  import { assetUrl } from "../lib/assets";
  import { level } from "../lib/format";
  import { showEncounters, showParty } from "../lib/live";
  import type { PlayerCard } from "../lib/types";
  import AgentBox from "./AgentBox.svelte";
  import EncounterBadge from "./EncounterBadge.svelte";
  import Last5 from "./Last5.svelte";
  import Name from "./Name.svelte";
  import PartyMark from "./PartyMark.svelte";
  import PeakRow from "./PeakRow.svelte";
  import PrevRow from "./PrevRow.svelte";
  import RankCurrent from "./RankCurrent.svelte";
  import Skel from "./Skel.svelte";
  import StatTrio from "./StatTrio.svelte";

  let { card, onopen }: { card: PlayerCard; onopen?: (puuid: string) => void } = $props();

  const open = () => onopen?.(card.puuid);

  function onkeydown(e: KeyboardEvent) {
    if (e.key !== "Enter" && e.key !== " ") return;
    e.preventDefault();
    open();
  }
</script>

<div class="card" role="button" tabindex="0" onclick={open} {onkeydown}>
  <div class="art">
    <div class="art-img" style:background-image={`url("${assetUrl("cardLarge", card.cardId)}")`}></div>
    <div class="art-fade"></div>
  </div>
  {#if card.party && showParty(card)}
    <PartyMark party={card.party} />
  {/if}
  <div class="grid">
    <div class="who">
      <AgentBox agentId={card.agentId} />
      <div class="ncol">
        <div class="l1">
          <Name name={card.name} size="card" />
          <!-- Always rendered, even empty: the name's width does not depend on the badge (spec §3.3). -->
          <div class="encslot">
            {#if card.encounters && showEncounters(card)}
              <EncounterBadge enc={card.encounters} size="card" />
            {/if}
          </div>
        </div>
        <div class="lv">
          {#if card.level === undefined}<Skel w={48} h={10} />{:else}Lv <span class="v">{level(card.level)}</span>{/if}
        </div>
      </div>
    </div>
    <RankCurrent rank={card.rank} />
    <PeakRow peak={card.peak} />
    <PrevRow tier={card.prevActTier} />
    <StatTrio {card} />
    <Last5 entries={card.last5} size="card" />
  </div>
  {#if card.isSelf}
    <div class="self"></div>
  {/if}
</div>

<style>
  .card {
    position: relative;
    min-height: 0;
    background: var(--card);
    border-radius: 6px;
    overflow: hidden;
    cursor: pointer;
  }

  .card:hover {
    box-shadow: inset 0 0 0 1px var(--line-hover);
  }

  .art {
    position: absolute;
    left: 0;
    top: 0;
    bottom: 0;
    width: 44px;
    overflow: hidden;
  }

  .art-img,
  .art-fade {
    position: absolute;
    inset: 0;
  }

  .art-img {
    background-size: 150% auto;
    background-position: 50% 0;
    background-repeat: no-repeat;
    opacity: 0.8;
  }

  .art-fade {
    background: var(--strip-fade);
  }

  .grid {
    position: relative;
    height: 100%;
    box-sizing: border-box;
    display: grid;
    grid-template-columns: minmax(0, 1fr) 158px;
    grid-template-rows: 40px 18px 1fr;
    column-gap: 16px;
    row-gap: 10px;
    padding: 12px 14px 12px 56px;
  }

  .who {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }

  .ncol {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .l1 {
    display: flex;
    align-items: center;
    min-width: 0;
  }

  /* [C] min-width, not width: a 3+ digit count widens the slot leftwards instead of covering the tag. */
  .encslot {
    min-width: 36px;
    flex: none;
    display: flex;
    justify-content: flex-end;
  }

  .lv {
    align-self: flex-start;
    font-size: 12px;
    font-weight: 500;
    color: var(--text2);
  }

  .v {
    color: var(--text);
    font-variant-numeric: tabular-nums;
  }

  .self {
    position: absolute;
    inset: 0;
    border: 1px solid var(--self-frame);
    border-radius: 6px;
    pointer-events: none;
  }
</style>
