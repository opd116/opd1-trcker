<!--
  Match Details (spec §6.10, v8:358-419): Back to the player's Details, the header, the scoreboard. It scrolls
  vertically inside App's one scroll container. The data is `invoke('match_details', { matchId, focusPuuid })`; the
  backend sets the perspective, the order and every name (R74, R93), the page only renders it. Until the answer comes
  (or if it never does) the skeleton shows; a newer request always wins over an older one ({#await} keeps only the
  latest promise). A rejected call is retried once after 1s; when that fails too the page settles on `emptyMatch`
  (`—` in the header, the scoreboard's column row only, no text) and stops being busy.
-->
<script lang="ts">
  import BackButton from "../components/details/BackButton.svelte";
  import MatchHeader from "../components/match/MatchHeader.svelte";
  import MatchSkeleton from "../components/match/MatchSkeleton.svelte";
  import Scoreboard from "../components/match/Scoreboard.svelte";
  import { getMatchDetails } from "../lib/api";
  import { DETAILS_RETRY_MS, retryOnce } from "../lib/boot";
  import { emptyMatch, type MatchFrom } from "../lib/details";
  import type { MatchDetailsView } from "../lib/types";

  // `from` (recent row or encounter row) is part of the route. The backend decides the perspective; the page only
  // hands `from` to the header, whose date reads differently for an encounter row (R140).
  let { matchId, focusPuuid, from }: { matchId: string; focusPuuid: string; from: MatchFrom } = $props();

  // Never rejects: a failure ends as the settled empty view.
  const request = $derived(
    retryOnce(() => getMatchDetails(matchId, focusPuuid), DETAILS_RETRY_MS).catch((err: unknown) => {
      console.error("match_details failed", err);
      return emptyMatch(matchId);
    }),
  );
</script>

{#snippet page(view: MatchDetailsView | null)}
  <div class="match" aria-busy={view === null}>
    <BackButton />
    {#if view}
      <MatchHeader {view} {from} />
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
{/await}
