<!-- Top bar (spec §2.2): wordmark and tabs. The right group (map, average party rank) arrives with the Live
     screen (Task 17): it is rendered on the Live grid route only and left out on Details and Match. -->
<script lang="ts">
  import { go } from "../lib/router.svelte";
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
</style>
