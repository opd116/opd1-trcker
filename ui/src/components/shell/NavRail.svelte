<!--
  The nav rail (design v2 116–139): 76px. Live, Saved, History, Comp, Skins, ASCII; a spacer; Settings and About; the
  user's own rank at the bottom (opens Competitive). The Live item pulses while a match is live (Agent Select / In
  Game, not while the client is unreachable); Saved carries the number of saved players in the live match.
  Items are buttons (Enter / Space), the current one has aria-current="page".
-->
<script lang="ts">
  import { appStatus, savedInMatch } from "../../lib/appdata.svelte";
  import { assetUrl } from "../../lib/assets";
  import { retryImg } from "../../lib/imgRetry";
  import { go, router, type Screen } from "../../lib/router.svelte";
  import { live, meta, myStatsForSelf } from "../../lib/stores";
  import { tierColor, tierShort } from "../../lib/theme";
  import { tip } from "../../lib/tooltip.svelte";
  import Dot from "./Dot.svelte";

  type Item = { id: Screen; label: string; d: string };

  /** Icon paths: the prototype's NAV / NAVB (2063–2064); About is an info circle in the same stroke style. */
  const MAIN: Item[] = [
    { id: "live", label: "Live", d: "M4 10a6 6 0 1 0 12 0a6 6 0 1 0-12 0M10 1.5v4M10 14.5v4M1.5 10h4M14.5 10h4" },
    { id: "saved", label: "Saved", d: "M5.5 3h9v14l-4.5-3.2L5.5 17z" },
    { id: "history", label: "History", d: "M2.5 10a7.5 7.5 0 1 0 15 0a7.5 7.5 0 1 0-15 0M10 6v4.5l3 2" },
    { id: "comp", label: "Comp", d: "M3 16l4.5-5 3.5 3 6-8M13 6h4v4" },
    { id: "skins", label: "Skins", d: "M3 3h6v6H3zM11 3h6v6h-6zM3 11h6v6H3zM11 11h6v6h-6z" },
    { id: "ascii", label: "ASCII", d: "M7 5L3 10l4 5M13 5l4 5-4 5" },
  ];
  const BOTTOM: Item[] = [
    { id: "settings", label: "Settings", d: "M3 6h4M11 6h6M3 14h8M15 14h2M9 4v4M13 12v4" },
    { id: "about", label: "About", d: "M2.5 10a7.5 7.5 0 1 0 15 0a7.5 7.5 0 1 0-15 0M10 9v4.5M10 6.25v.01" },
  ];

  const matchLive = $derived(
    ($live?.stage === "pregame" || $live?.stage === "ingame") && $appStatus?.conn !== "error",
  );

  /** The user's rank: the live card's (freshest), else My Stats'. My Stats only counts for the account on screen. */
  const self = $derived.by(() => {
    const card = $live?.players.find((p) => p.isSelf);
    if (card?.rank) return { tier: card.rank.tier, rr: card.rank.rr };
    const r = $myStatsForSelf?.rank;
    if (r && r.tier !== null) return { tier: r.tier, rr: r.rr };
    return null;
  });
</script>

{#snippet item(n: Item)}
  {@const on = router.screen === n.id}
  <button type="button" class={["item", on && "on"]} aria-label={n.label} aria-current={on ? "page" : undefined} onclick={() => go(n.id)}>
    <span class="bar" aria-hidden="true"></span>
    <svg width="20" height="20" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"
      ><path d={n.d}></path></svg
    >
    <span class="label">{n.label}</span>
    {#if n.id === "live" && matchLive}<span class="live-dot"><Dot color="var(--win)" pulse /></span>{/if}
    {#if n.id === "saved" && $savedInMatch > 0}<span class="badge" aria-label="{$savedInMatch} saved in this match">{$savedInMatch}</span>{/if}
  </button>
{/snippet}

<nav class="rail" aria-label="Screens">
  {#each MAIN as n (n.id)}{@render item(n)}{/each}
  <div class="spacer"></div>
  {#each BOTTOM as n (n.id)}{@render item(n)}{/each}
  <button type="button" class="rank" aria-label="Your rank: open Competitive" use:tip={"Your rank · open Competitive"} onclick={() => go("comp")}>
    {#if self}
      <img {@attach retryImg} src={assetUrl("tier", self.tier)} alt="" width="28" height="28" />
      <span class="short" style:color={tierColor(self.tier)}>{tierShort(self.tier, $meta)}</span>
      <span class="rr">{self.tier >= 3 ? `${self.rr ?? "—"} RR` : "—"}</span>
    {:else}
      <span class="skel-icon"></span>
      <span class="short dim">—</span>
      <span class="rr">— RR</span>
    {/if}
  </button>
</nav>

<style>
  .rail {
    width: 76px;
    flex: none;
    display: flex;
    flex-direction: column;
    padding: 8px 0 10px;
    gap: 2px;
    background: var(--rail);
    border-right: 1px solid var(--line-chrome);
  }

  .item {
    position: relative;
    height: 58px;
    flex: none;
    width: 100%;
    margin: 0;
    padding: 0;
    border: 0;
    background: transparent;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 6px;
    cursor: pointer;
    color: var(--text-muted);
  }

  /* The items touch the window's left edge: an inset ring. The active item's positioned bar sits below the item's
     content while it is focused (its own stacking context, z-index −1), so it does not paint over the ring. */
  .item:focus-visible {
    outline-offset: -2px;
    isolation: isolate;
  }

  .item:focus-visible .bar {
    z-index: -1;
  }

  .item:hover {
    color: var(--text);
  }

  .item.on {
    color: var(--text);
    background: var(--nav-active);
  }

  .item.on svg {
    color: var(--accent);
  }

  .item svg {
    display: block;
  }

  .bar {
    position: absolute;
    left: 0;
    top: 13px;
    bottom: 13px;
    width: 2px;
    border-radius: 0 2px 2px 0;
    background: transparent;
  }

  .item.on .bar {
    background: var(--accent);
  }

  .label {
    font: 600 11.5px/1 var(--font);
  }

  /* A block with the body's line box, the inline-block dot on its baseline: the prototype's exact spot (16px down). */
  .live-dot {
    position: absolute;
    top: 8px;
    right: 17px;
    display: block;
    font: 400 16px/normal var(--font);
  }

  .badge {
    position: absolute;
    top: 9px;
    right: 15px;
    min-width: 16px;
    height: 14px;
    padding: 0 4px;
    border-radius: 7px;
    background: var(--accent);
    color: var(--on-accent);
    font: 600 9px/14px var(--font-mono);
    text-align: center;
  }

  .spacer {
    flex: 1;
  }

  .rank {
    margin: 8px 10px 0;
    padding: 9px 0 8px;
    border: 0;
    border-top: 1px solid var(--line-chrome);
    background: transparent;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 5px;
    cursor: pointer;
    flex: none;
  }

  .rank img {
    width: 28px;
    height: 28px;
    display: block;
  }

  .skel-icon {
    width: 28px;
    height: 28px;
    border-radius: 50%;
    background: var(--skel);
  }

  .short {
    font: 600 12.5px/1 var(--font);
  }

  .short.dim {
    color: var(--text-disabled);
  }

  .rr {
    font: 500 10.5px/1 var(--font-mono);
    color: var(--text-muted);
  }
</style>
