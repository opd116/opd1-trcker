<!-- Top bar (spec §2.2–§2.3): wordmark, tabs and, on the Live grid route only, the right group: the Average
     Party Rank in the Lobby (2+ players), the map in Agent Select and In Game. Empty on Details and Match. -->
<script lang="ts">
  import { go, type Route } from "../lib/router.svelte";
  import type { LiveState } from "../lib/types";
  import AvgPartyRank from "./AvgPartyRank.svelte";
  import MapGroup from "./MapGroup.svelte";

  let { route, live }: { route: Route; live: LiveState | null } = $props();
</script>

<header class="bar">
  <div class="word">OPD1 TRCKER</div>
  <nav class="tabs">
    <!-- Live is always the active tab, including on Details and Match; clicking it goes to the Live grid. -->
    <button type="button" class="tab on" aria-current="page" onclick={() => go({ name: "live" })}>
      Live<span class="ind"></span>
    </button>
    <!-- My Stats is inert until that page exists: no handler, default cursor. -->
    <span class="tab off">My Stats</span>
  </nav>
  <div class="spacer"></div>
  {#if route.name === "live"}
    <div class="right">
      {#if live?.stage === "menus"}
        {#if live.players.length >= 2}
          <AvgPartyRank players={live.players} />
        {/if}
      {:else if live?.stage === "pregame" || live?.stage === "ingame"}
        <MapGroup mapId={live.mapId} />
      {/if}
    </div>
  {/if}
</header>

<style>
  .bar {
    height: 48px;
    flex: none;
    display: flex;
    align-items: center;
    gap: 30px;
    padding: 0 18px;
    border-bottom: 1px solid var(--line);
  }

  .word {
    font-size: 14px;
    font-weight: 700;
    letter-spacing: 0.18em;
    white-space: nowrap;
  }

  .tabs {
    display: flex;
    gap: 24px;
    align-self: stretch;
  }

  .tab {
    position: relative;
    display: flex;
    align-items: center;
  }

  button.tab {
    margin: 0;
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    font-size: 14px;
    font-weight: 600;
    cursor: pointer;
  }

  .off {
    font-size: 14px;
    font-weight: 500;
    color: var(--text2);
    cursor: default;
  }

  .ind {
    position: absolute;
    left: 0;
    right: 0;
    bottom: -1px;
    height: 2px;
    border-radius: 1px;
    background: var(--accent);
  }

  .spacer {
    flex: 1;
  }

  .right {
    display: flex;
    align-items: center;
    gap: 12px;
  }
</style>
