<script lang="ts">
  import { isTauri } from "@tauri-apps/api/core";
  import type { Attachment } from "svelte/attachments";
  import Tooltip from "./components/Tooltip.svelte";
  import TopBar from "./components/TopBar.svelte";
  import { tauriBridge } from "./lib/api";
  import { keyAction, runKeyAction } from "./lib/boot";
  import { cardOpenGuard } from "./lib/clickGuard";
  import { router, scrollHost } from "./lib/router.svelte";
  import { live, mystats } from "./lib/stores";
  import Details from "./screens/Details.svelte";
  import Live from "./screens/Live.svelte";
  import Match from "./screens/Match.svelte";
  import MyStats from "./screens/MyStats.svelte";

  // Ctrl+Shift+L / Ctrl+Shift+D (no UI). Only meaningful with the Rust side present.
  function onkeydown(e: KeyboardEvent) {
    const action = keyAction(e);
    if (!action || !isTauri()) return;
    e.preventDefault();
    runKeyAction(action, tauriBridge, (text) => navigator.clipboard.writeText(text)).catch((err) =>
      console.error(`${action} failed`, err),
    );
  }

  // A card click opens Details; the second click of a double-click must not land on the page that replaced the grid
  // (Back sits where the top-left card was). Capture phase, so it is gone before any handler on the new page sees it.
  function swallowDoubleClick(e: MouseEvent) {
    if (!cardOpenGuard.swallows()) return;
    e.stopPropagation();
    e.preventDefault();
  }

  /** One boundary per page: a crash on one page never follows the user to the next. "Show more" stays on its page. */
  const pageKey = $derived(
    router.route.name === "live"
      ? "live"
      : router.route.name === "mystats"
        ? "mystats"
        : router.route.name === "details"
          ? `details:${router.route.puuid}`
          : `match:${router.route.matchId}`,
  );

  /**
   * A crashed page tries again on the next live update or the next My Stats update (the likeliest cause is one bad
   * payload).
   */
  const retryOnNextUpdate =
    (reset: () => void): Attachment =>
    () => {
      /** The subscription's own first call is the state that crashed; only a later one is an update. */
      const onUpdate = () => {
        let first = true;
        return () => {
          if (first) first = false;
          else reset();
        };
      };
      const offLive = live.subscribe(onUpdate());
      const offStats = mystats.subscribe(onUpdate());
      return () => {
        offLive();
        offStats();
      };
    };
</script>

<svelte:window {onkeydown} onclickcapture={swallowDoubleClick} />

<TopBar route={router.route} live={$live} />
{#key pageKey}
  <svelte:boundary onerror={(err) => console.error("page crashed", err)}>
    {#if router.route.name === "live"}
      <Live live={$live} />
    {:else}
      <!-- One scroll container for My Stats, Details and Match (spec §6.1, §9.1); `go` resets it to the top. -->
      <div class="scroll" use:scrollHost>
        {#if router.route.name === "mystats"}
          <MyStats />
        {:else if router.route.name === "details"}
          <Details puuid={router.route.puuid} more={router.route.more} />
        {:else}
          <Match matchId={router.route.matchId} focusPuuid={router.route.focusPuuid} from={router.route.from} />
        {/if}
      </div>
    {/if}
    <!-- A quiet empty page: no text, no status. The top bar (and its Live tab) stay. -->
    {#snippet failed(_error, reset)}
      <div class="blank" {@attach retryOnNextUpdate(reset)}></div>
    {/snippet}
  </svelte:boundary>
{/key}
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

  .blank {
    flex: 1;
    min-height: 0;
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
