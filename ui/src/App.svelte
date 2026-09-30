<script lang="ts">
  import { isTauri } from "@tauri-apps/api/core";
  import Tooltip from "./components/Tooltip.svelte";
  import TopBar from "./components/TopBar.svelte";
  import { tauriBridge } from "./lib/api";
  import { keyAction, runKeyAction } from "./lib/boot";
  import { router, scrollHost } from "./lib/router.svelte";
  import { live } from "./lib/stores";
  import Details from "./screens/Details.svelte";
  import Live from "./screens/Live.svelte";
  import Match from "./screens/Match.svelte";

  // Ctrl+Shift+L / Ctrl+Shift+D (no UI). Only meaningful with the Rust side present.
  function onkeydown(e: KeyboardEvent) {
    const action = keyAction(e);
    if (!action || !isTauri()) return;
    e.preventDefault();
    runKeyAction(action, tauriBridge, (text) => navigator.clipboard.writeText(text)).catch((err) =>
      console.error(`${action} failed`, err),
    );
  }
</script>

<svelte:window {onkeydown} />

<TopBar route={router.route} live={$live} />
{#if router.route.name === "live"}
  <Live live={$live} />
{:else}
  <!-- One scroll container for Details and Match (spec §6.1); `go` resets it to the top. -->
  <div class="scroll" use:scrollHost>
    {#if router.route.name === "details"}
      <Details />
    {:else}
      <Match />
    {/if}
  </div>
{/if}
<Tooltip />

<style>
  /* #app is the mount target from index.html (spec §2.1). */
  :global(#app) {
    position: relative;
    width: 1000px;
    height: 800px;
    overflow: hidden;
    background: var(--bg);
    color: var(--text);
    font-family: var(--font);
    display: flex;
    flex-direction: column;
    -webkit-font-smoothing: antialiased;
  }

  .scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    scrollbar-width: thin;
    scrollbar-color: var(--scroll) transparent;
    scrollbar-gutter: stable;
    padding: 14px 8px 22px 16px;
  }
</style>
