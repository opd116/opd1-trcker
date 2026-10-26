<script lang="ts">
  import { isTauri } from "@tauri-apps/api/core";
  import ProfileDrawer from "./components/profile/ProfileDrawer.svelte";
  import GlanceShell from "./components/shell/GlanceShell.svelte";
  import NavRail from "./components/shell/NavRail.svelte";
  import TitleBar from "./components/shell/TitleBar.svelte";
  import Tooltip from "./components/Tooltip.svelte";
  import { tauriBridge } from "./lib/api";
  import { keyAction, runKeyAction } from "./lib/boot";
  import { retryOnChange, retryOnNextUpdate } from "./lib/boundary";
  import { cardOpenGuard } from "./lib/clickGuard";
  import { router } from "./lib/router.svelte";
  import { live, mystats } from "./lib/stores";
  import { reportError } from "./lib/uilog";
  import About from "./screens/About.svelte";
  import Ascii from "./screens/Ascii.svelte";
  import Competitive from "./screens/Competitive.svelte";
  import History from "./screens/History.svelte";
  import Live from "./screens/Live.svelte";
  import Saved from "./screens/Saved.svelte";
  import Settings from "./screens/Settings.svelte";
  import Skins from "./screens/Skins.svelte";

  // Ctrl+Shift+L / Ctrl+Shift+D (no UI). Only meaningful with the Rust side present.
  function onkeydown(e: KeyboardEvent) {
    const action = keyAction(e);
    if (!action || !isTauri()) return;
    e.preventDefault();
    runKeyAction(action, tauriBridge, (text) => navigator.clipboard.writeText(text)).catch((err) =>
      reportError(`key:${action}`, err),
    );
  }

  // A click that opens a page or the drawer must not have its double-click's second click land on what replaced it.
  // Capture phase, so it is gone before any handler on the new page sees it.
  function swallowDoubleClick(e: MouseEvent) {
    if (!cardOpenGuard.swallows()) return;
    e.stopPropagation();
    e.preventDefault();
  }

  /** One boundary per page: a crash on one page never follows the user to the next. A scoreboard is its own page. */
  const pageKey = $derived(router.matchId === null ? router.screen : `${router.screen}:match:${router.matchId}`);

  // #app fills the window; glance mode drops the full layout's minimum size (the window is a 420px column then).
  $effect(() => {
    document.getElementById("app")?.toggleAttribute("data-glance", router.glance);
  });
</script>

<svelte:window {onkeydown} onclickcapture={swallowDoubleClick} />

{#if router.glance}
  <!-- GlanceShell holds the body's own boundary, so its title bar (drag, Exit glance, Close) survives a body crash. -->
  <svelte:boundary onerror={(err) => reportError("glance", err)}>
    <GlanceShell />
    {#snippet failed(_error, reset)}
      <div class="blank" {@attach retryOnNextUpdate(reset, live)}></div>
    {/snippet}
  </svelte:boundary>
{:else}
  <TitleBar />
  <div class="row">
    <NavRail />
    <main class="main">
      {#key pageKey}
        <svelte:boundary onerror={(err) => reportError("page", err)}>
          {#if router.screen === "live"}
            <Live live={$live} />
          {:else if router.screen === "saved"}
            <Saved />
          {:else if router.screen === "history"}
            <History />
          {:else if router.screen === "comp"}
            <Competitive />
          {:else if router.screen === "skins"}
            <Skins />
          {:else if router.screen === "ascii"}
            <Ascii />
          {:else if router.screen === "settings"}
            <Settings />
          {:else}
            <About />
          {/if}
          <!-- A quiet empty page: no text. The title bar and the nav rail stay. A crashed page tries again on the next
               update of its data: Competitive on the next My Stats update, any other page on the next live update. -->
          {#snippet failed(_error, reset)}
            <div class="blank" {@attach retryOnNextUpdate(reset, router.screen === "comp" ? mystats : live)}></div>
          {/snippet}
        </svelte:boundary>
      {/key}
      {#if router.profile !== null}
        <!-- The drawer covers <main> only (the design's `position:absolute; inset:0` inside main). A crashed drawer takes
             no room from the page under it, and tries again on the next live update or when another player opens. -->
        <svelte:boundary onerror={(err) => reportError("profile", err)}>
          <ProfileDrawer puuid={router.profile} />
          {#snippet failed(_error, reset)}
            <div class="gone" {@attach retryOnNextUpdate(reset, live)} {@attach retryOnChange(reset, () => router.profile)}></div>
          {/snippet}
        </svelte:boundary>
      {/if}
    </main>
  </div>
{/if}
<Tooltip />

<style>
  /* #app is the mount target from index.html: it fills the window. */
  :global(#app) {
    position: relative;
    width: 100%;
    height: 100%;
    min-width: 1180px;
    min-height: 760px;
    overflow: hidden;
    background: var(--bg);
    color: var(--text);
    font-family: var(--font);
    display: flex;
    flex-direction: column;
    -webkit-font-smoothing: antialiased;
  }

  :global(#app[data-glance]) {
    min-width: 0;
    min-height: 0;
  }

  .row {
    flex: 1;
    min-height: 0;
    display: flex;
  }

  .main {
    flex: 1;
    min-width: 0;
    min-height: 0;
    position: relative;
    display: flex;
    flex-direction: column;
  }

  .blank {
    flex: 1;
    min-height: 0;
  }

  .gone {
    display: none;
  }
</style>
