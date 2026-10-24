<!--
  The History screen (design v2 523–657): the match list, or the scoreboard of `router.matchId` (App keys the page by
  the match, so each scoreboard is its own page). Each owns its scroll container (`use:scrollHost`).
-->
<script lang="ts">
  import HistoryList from "../components/history/HistoryList.svelte";
  import Scoreboard from "../components/history/Scoreboard.svelte";
  import { router } from "../lib/router.svelte";
</script>

{#if router.matchId !== null}
  <Scoreboard matchId={router.matchId} />
{:else}
  <HistoryList />
{/if}
