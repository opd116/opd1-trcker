<script lang="ts">
  import type { Component } from "svelte";

  type Tab = "live" | "my-stats";

  let activeTab = $state<Tab>("live");

  // DEV-only card gallery (?scene=cards). Dead code in `vite build`.
  let Gallery = $state<Component | null>(null);
  if (import.meta.env.DEV && new URLSearchParams(location.search).get("scene") === "cards") {
    import("./lib/demo/CardGallery.svelte").then((m) => (Gallery = m.default));
  }
</script>

{#if Gallery}
  <Gallery />
{:else}
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
{/if}

<style>
  main {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }

  .tabs {
    display: flex;
    flex-shrink: 0;
    border-bottom: 1px solid var(--line);
  }

  .tabs button {
    flex: 1;
    padding: 12px 0;
    font: inherit;
    font-size: var(--fs-md);
    font-weight: var(--fw-strong);
    background: transparent;
    color: var(--text-3);
    border: none;
    border-bottom: 2px solid transparent;
    cursor: pointer;
  }

  .tabs button.active {
    color: var(--text-1);
    border-bottom-color: var(--accent);
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
