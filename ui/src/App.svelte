<script lang="ts">
  type Tab = "live" | "my-stats";

  let activeTab = $state<Tab>("live");
</script>

<main>
  <div class="tabs" role="tablist" aria-label="Screens">
    <button
      type="button"
      role="tab"
      aria-selected={activeTab === "live"}
      class:active={activeTab === "live"}
      onclick={() => (activeTab = "live")}
    >
      Live
    </button>
    <button
      type="button"
      role="tab"
      aria-selected={activeTab === "my-stats"}
      class:active={activeTab === "my-stats"}
      onclick={() => (activeTab = "my-stats")}
    >
      My Stats
    </button>
  </div>

  <section class="panel" role="tabpanel">
    {#if activeTab === "live"}
      <div class="live"></div>
    {:else}
      <div class="my-stats"></div>
    {/if}
  </section>
</main>

<style>
  main {
    display: flex;
    flex-direction: column;
    height: 100vh;
    box-sizing: border-box;
  }

  .tabs {
    display: flex;
    flex-shrink: 0;
    border-bottom: 1px solid #2a2a2a;
  }

  .tabs button {
    flex: 1;
    padding: 12px 0;
    font-size: 14px;
    font-family: inherit;
    background: transparent;
    color: inherit;
    border: none;
    border-bottom: 2px solid transparent;
    cursor: pointer;
  }

  .tabs button.active {
    border-bottom-color: currentColor;
  }

  .panel {
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }

  .live,
  .my-stats {
    height: 100%;
  }
</style>
