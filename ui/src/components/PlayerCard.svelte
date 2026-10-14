<!--
  The player card (spec §3.2–§3.10): identical in every Live stage. Grid 40 / 18 / 1fr rows, 1fr + 158px columns.
  Click anywhere opens Player Details (tooltip triggers inside do not stop it).
-->
<script lang="ts">
  import { assetUrl } from "../lib/assets";
  import { retryBg } from "../lib/imgRetry";
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

  // The art's URL as a string (R163): every `live` event builds new card objects, and an attachment keyed on the
  // object would be torn down, cleared and reloaded each time. A `$derived` string that did not change keeps it.
  const artUrl = $derived(card.cardId ? assetUrl("cardLarge", card.cardId) : "");

  function onkeydown(e: KeyboardEvent) {
    if (e.key !== "Enter" && e.key !== " ") return;
    e.preventDefault();
    open();
  }
</script>

<div class="card" role="button" tabindex="0" onclick={open} {onkeydown}>
  <div class="art">
    <div class="art-img" {@attach artUrl ? retryBg(artUrl) : undefined}></div>
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
      </div>
      <div class="lv">
        {#if card.level === undefined}<Skel w={48} h={10} />{:else}Lv <span class="v">{level(card.level)}</span>{/if}
      </div>
    </div>
    <div class="ranking">
      <RankCurrent rank={card.rank} />
      <div class="history">
        <PrevRow tier={card.prevActTier} />
        <PeakRow peak={card.peak} />
      </div>
    </div>
    <div class="performance">
      <StatTrio {card} />
      <Last5 entries={card.last5} size="card" />
    </div>
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
    width: 38px;
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
    opacity: 0.5;
  }

  .art-fade {
    background: var(--strip-fade);
  }

  .grid {
    position: relative;
    height: 100%;
    box-sizing: border-box;
    display: grid;
    grid-template-rows: 28px minmax(0, 1fr) 32px;
    row-gap: 6px;
    padding: 8px 14px 8px 48px;
  }

  .ranking {
    display: grid;
    grid-template-columns: 164px minmax(0, 1fr);
    gap: 12px;
    align-items: center;
    min-width: 0;
  }

  .history {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
    border-left: 1px solid var(--line);
    padding-left: 12px;
  }

  .performance {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    border-top: 1px solid var(--line);
    padding-top: 6px;
    min-width: 0;
  }

  .who {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    padding-right: 16px;
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
    flex: none;
    min-width: 44px;
    text-align: right;
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
