<!-- Top bar (spec §2.2–§2.3): wordmark, tabs and, on the Live grid route only, the right group: the Average
     Party Rank in the Lobby (2+ players), the map in Agent Select and In Game. Empty on My Stats, Details and Match.
     The active tab is My Stats on My Stats and on a Match opened from it, Live everywhere else. -->
<script lang="ts">
  import { activeTab, go, type Route } from "../lib/router.svelte";
  import type { LiveState } from "../lib/types";
  import AvgPartyRank from "./AvgPartyRank.svelte";
  import MapGroup from "./MapGroup.svelte";

  let { route, live }: { route: Route; live: LiveState | null } = $props();

  const TABS = [
    { id: "live", label: "Live" },
    { id: "mystats", label: "My Stats" },
  ] as const;
  const active = $derived(activeTab(route));

  // A tab already showing its own page stays put (no scroll reset); from a Match or Details it goes to the page.
  const open = (id: (typeof TABS)[number]["id"]) => {
    if (id === "mystats" && route.name === "mystats") return;
    go({ name: id });
  };
</script>

<header class="bar">
  <div class="word">OPD1 TRCKER</div>
  <nav class="tabs">
    {#each TABS as t (t.id)}
      <button type="button" class={["tab", active === t.id ? "on" : "off"]} aria-current={active === t.id ? "page" : undefined} onclick={() => open(t.id)}>
        {t.label}{#if active === t.id}<span class="ind"></span>{/if}
      </button>
    {/each}
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
    height: 63px;
    flex: none;
    display: flex;
    align-items: center;
    gap: 42px;
    padding: 0 20px;
    border-bottom: 1px solid var(--line);
  }

  .word {
    font-size: 16px;
    font-weight: 700;
    letter-spacing: 0.12em;
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

  /* 4px of padding, taken back by a -4px margin (the layout is unchanged), so the inset focus ring below has room and
     never runs through the letters. */
  button.tab {
    margin: 0 -4px;
    padding: 0 4px;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    font-size: 14px;
    cursor: pointer;
  }

  button.on {
    font-weight: 600;
    color: var(--accent);
  }

  /* R106 focus ring, inset: this tab touches the top edge of #app (overflow hidden), where the global 2px-outside
     ring would be cut off. Keyboard focus only; the mouse look is untouched. */
  button.tab:focus-visible {
    outline-offset: -2px;
  }

  /* The tab of the page not shown: the design's inactive look, a little brighter under the pointer [C]. */
  button.off {
    font-weight: 500;
    color: var(--text2);
  }

  button.off:hover {
    color: var(--text-soft);
  }

  .ind {
    position: absolute;
    left: 4px;
    right: 4px;
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
