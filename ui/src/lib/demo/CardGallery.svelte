<!--
  DEV-only card gallery for screenshot checks: /?scene=cards&size=sm|md|lg|all&names=long|normal
  sm: the In Game budget (2 columns × 5 cards of ~476×124 inside 1000×800).
  md: Agent Select / Lobby 4–5 budget (3 + 2 cards of ~306×320). names=long swaps in the longest Riot ID.
  lg: Lobby 1–3 budget (3 cards of ~304 wide, plus one 380 wide).
  &w=<px> overrides the card width (sm columns / md and lg cells) to probe minimum widths.
-->
<script lang="ts">
  import PlayerCard from "../../components/PlayerCard.svelte";
  import type { PlayerCard as Card } from "../types";
  import { buildScene, galleryCards, LONGEST_NAME, LONGEST_TAG } from "./fixtures";

  const params = new URLSearchParams(location.search);
  const size = params.get("size") ?? "all";
  const long = params.get("names") !== "normal";
  const w = params.get("w");
  const colW = w ? `${Number(w)}px` : null;

  const ingame = buildScene(long ? "ingame-longnames" : "ingame").players;
  const longest = (c: Card): Card => ({ ...c, name: { kind: "visible", name: LONGEST_NAME, tag: LONGEST_TAG } });
  const pregame = buildScene("pregame").players.map((c) =>
    params.get("names") === "long" && c.name.kind !== "hiddenPlayer" ? longest(c) : c,
  );
  const lobby5 = buildScene("lobby5").players;
  const lobby3 = buildScene("lobby3").players;
  const all = galleryCards();
</script>

{#if size === "sm"}
  <div class="frame sm-frame">
    <div class="chrome"></div>
    <div class="teams" style:--col-w={colW}>
      <div class="col">
        {#each ingame.slice(0, 5) as card (card.puuid)}
          <div class="cell"><PlayerCard {card} size="sm" showTeam /></div>
        {/each}
      </div>
      <div class="col">
        {#each ingame.slice(5) as card (card.puuid)}
          <div class="cell"><PlayerCard {card} size="sm" showTeam /></div>
        {/each}
      </div>
    </div>
  </div>
{:else if size === "md"}
  <div class="frame md-frame">
    <div class="chrome"></div>
    <div class="md-grid" style:--col-w={colW}>
      {#each pregame as card (card.puuid)}
        <PlayerCard {card} size="md" />
      {/each}
    </div>
  </div>
{:else if size === "md-lobby"}
  <div class="frame md-frame">
    <div class="chrome lobby"></div>
    <div class="md-grid lobby" style:--col-w={colW}>
      {#each lobby5 as card (card.puuid)}
        <PlayerCard {card} size="md" />
      {/each}
    </div>
  </div>
{:else if size === "lg"}
  <div class="frame lg-frame">
    <div class="chrome lobby"></div>
    <div class="lg-row" style:--col-w={colW}>
      {#each lobby3 as card (card.puuid)}
        <div class="lg-cell"><PlayerCard {card} size="lg" /></div>
      {/each}
    </div>
  </div>
{:else if size === "lg1"}
  <div class="frame lg-frame">
    <div class="chrome"></div>
    <div class="lg-row one">
      <div class="lg-cell"><PlayerCard card={lobby3[0]} size="lg" /></div>
    </div>
  </div>
{:else}
  <div class="all">
    {#each all as card, i (i)}
      <div class="all-sm"><PlayerCard {card} size="sm" showTeam /></div>
    {/each}
    <div class="all-md">
      {#each all as card, i (i)}
        <div class="all-md-cell"><PlayerCard {card} size="md" /></div>
      {/each}
    </div>
  </div>
{/if}

<style>
  .frame {
    width: 1000px;
    height: 800px;
    overflow: hidden;
    padding: 0 16px 16px;
  }

  /* Stand-in for the tabs bar + map header (Task 8), so card budgets are honest. */
  .chrome {
    height: 104px;
  }

  .chrome.lobby {
    height: 112px;
  }

  .teams {
    display: grid;
    grid-template-columns: repeat(2, var(--col-w, 1fr));
    column-gap: 16px;
  }

  .col {
    display: grid;
    grid-template-rows: repeat(5, 124px);
    row-gap: 8px;
  }

  .cell {
    min-width: 0;
  }

  .md-grid {
    display: grid;
    grid-template-columns: repeat(3, var(--col-w, 1fr));
    grid-template-rows: repeat(2, 320px);
    gap: 12px;
    justify-content: center;
  }

  .md-grid.lobby {
    grid-template-rows: repeat(2, 316px);
  }

  .lg-row {
    display: grid;
    grid-template-columns: repeat(3, var(--col-w, 1fr));
    gap: 16px;
    align-items: start;
  }

  .lg-row.one {
    grid-template-columns: 380px;
    justify-content: center;
  }

  .all {
    width: 1000px;
    padding: 16px;
    display: grid;
    grid-template-columns: 476px 476px;
    gap: 8px 16px;
  }

  .all-sm {
    height: 124px;
  }

  .all-md {
    grid-column: 1 / -1;
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 12px;
    margin-top: 16px;
  }

  .all-md-cell {
    height: 320px;
  }
</style>
