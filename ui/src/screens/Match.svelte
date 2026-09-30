<!--
  Match Details (spec §6.10, v8:358-419): Back to the player's Details, the header, the scoreboard. It scrolls
  vertically inside App's one scroll container. The data is `invoke('match_details', { matchId, focusPuuid })`; the
  backend sets the perspective, the order and every name (R74, R93), the page only renders it. Until the answer comes
  (or if it never does) the skeleton shows; a newer request always wins over an older one ({#await} keeps only the
  latest promise).
-->
<script lang="ts">
  import BackButton from "../components/details/BackButton.svelte";
  import MatchHeader from "../components/match/MatchHeader.svelte";
  import MatchSkeleton from "../components/match/MatchSkeleton.svelte";
  import Scoreboard from "../components/match/Scoreboard.svelte";
  import { getMatchDetails } from "../lib/api";
  import type { MatchDetailsView } from "../lib/types";

  // `from` (recent row or encounter row) is part of the route; the backend decides the perspective, so the page
  // itself does not read it.
  let { matchId, focusPuuid }: { matchId: string; focusPuuid: string; from: "recent" | "encounter" } = $props();

  const request = $derived(
    getMatchDetails(matchId, focusPuuid).catch((err: unknown) => {
      console.error("match_details failed", err);
      throw err;
    }),
  );
</script>

{#snippet page(view: MatchDetailsView | null)}
  <div class="match" aria-busy={view === null}>
    <BackButton />
    {#if view}
      <MatchHeader {view} />
      <Scoreboard {view} />
    {:else}
      <MatchSkeleton />
    {/if}
  </div>
{/snippet}

{#await request}
  {@render page(null)}
{:then view}
  {@render page(view)}
{:catch}
  {@render page(null)}
{/await}
