<!--
  Match Details (spec §6.10, v8:358-419): Back to the player's Details, the header, the scoreboard. It scrolls
  vertically inside App's one scroll container. The data is `invoke('match_details', { matchId, focusPuuid })`, kept in
  the `openMatch` slot (boot.ts) so a `match` event for this match replaces it in place (R150: Rust found names for the
  rows that showed agent names); the backend sets the perspective, the order and every name (R74, R93), the page only
  renders it. An event always wins over an answer still in flight. Leaving the page closes the slot, so an event after
  that changes nothing, and tells Rust (`close_match`, like `close_details`), which forgets the open match so nothing is
  rebuilt or emitted for a page nobody sees. Another match replacing this one sends this page's `close_match` first and
  the new page's `match_details` after it, which opens the new match again. Until the answer comes (or if it never
  does) the skeleton shows. A rejected call is retried once after 1s; when that fails too the page settles on
  `emptyMatch` (`—` in the header, the scoreboard's column row only, no text) and stops being busy.
-->
<script lang="ts">
  import BackButton from "../components/details/BackButton.svelte";
  import MatchHeader from "../components/match/MatchHeader.svelte";
  import MatchSkeleton from "../components/match/MatchSkeleton.svelte";
  import Scoreboard from "../components/match/Scoreboard.svelte";
  import { closeMatch, getMatchDetails } from "../lib/api";
  import { DETAILS_RETRY_MS, DETAILS_TIMEOUT_MS } from "../lib/boot";
  import { emptyMatch, type MatchFrom } from "../lib/details";
  import { openMatch } from "../lib/stores";

  // `from` (recent row or encounter row) is part of the route. The backend decides the perspective; the page only
  // hands `from` to the header, whose date reads differently for an encounter row (R140).
  let { matchId, focusPuuid, from }: { matchId: string; focusPuuid: string; from: MatchFrom } = $props();

  /** The open view, only when it is this match's (an answer or event for another match is never shown). */
  const view = $derived($openMatch?.matchId === matchId ? $openMatch : null);

  // Fetch on open and whenever the match or the perspective changes; a failure ends as the settled empty view.
  $effect(() => {
    openMatch
      .open(matchId, (id) => getMatchDetails(id, focusPuuid), { retryMs: DETAILS_RETRY_MS, timeoutMs: DETAILS_TIMEOUT_MS, settle: emptyMatch })
      .catch((err: unknown) => console.error("match_details failed", err));
  });

  // Leaving the page (Back, the Live tab, Agent Select's switch to Live) unmounts it: the slot forgets the view, so a
  // `match` event that comes later changes nothing, and Rust forgets the open match (`close_match`). No dependency, so
  // this runs on unmount only. When another match replaces this page, this teardown runs before the new page's effect,
  // so its `close_match` goes out before the new `match_details` (which opens the new match on the Rust side).
  $effect(() => () => {
    openMatch.close();
    closeMatch(matchId).catch((err: unknown) => console.error("close_match failed", err));
  });
</script>

<div class="match" aria-busy={view === null}>
  <BackButton />
  {#if view}
    <MatchHeader {view} {from} />
    <Scoreboard {view} />
  {:else}
    <MatchSkeleton />
  {/if}
</div>
