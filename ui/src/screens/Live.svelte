<!--
  The one Live page (spec §3.1, §5): the design's In Game layout in every stage. Two columns, each a fixed 5-slot
  track, so a card is always 485 × 139.8px. No scrolling.
-->
<script lang="ts">
  import CardSkeleton from "../components/CardSkeleton.svelte";
  import EmptySlot from "../components/EmptySlot.svelte";
  import PlayerCard from "../components/PlayerCard.svelte";
  import { liveColumns, type SlotItem } from "../lib/live";
  import { go } from "../lib/router.svelte";
  import type { LiveState } from "../lib/types";

  let { live }: { live: LiveState | null } = $props();

  const cols = $derived(liveColumns(live));
  const open = (puuid: string) => go({ name: "details", puuid, more: false });
  const key = (s: SlotItem, i: number) => (s.kind === "card" ? s.card.puuid : `${s.kind}:${i}`);
</script>

{#snippet column(items: SlotItem[])}
  <div class="col">
    {#each items as slot, i (key(slot, i))}
      {#if slot.kind === "card"}
        <PlayerCard card={slot.card} onopen={open} />
      {:else if slot.kind === "skeleton"}
        <CardSkeleton />
      {:else if slot.kind === "dashed"}
        <EmptySlot />
      {/if}
    {/each}
  </div>
{/snippet}

<div class="live">
  {@render column(cols.left)}
  {@render column(cols.right)}
</div>

<style>
  .live {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    gap: 10px;
    padding: 10px;
  }

  .col {
    display: grid;
    grid-template-rows: repeat(5, minmax(0, 1fr));
    gap: 8px;
    min-height: 0;
  }
</style>
