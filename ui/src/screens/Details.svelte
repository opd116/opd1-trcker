<!--
  Player Details (spec §6.1–§6.9, v8:163-356): Back, the hero, then Recent Competitive Matches (+ Show more) and
  Encounters on the left, Top 3 Teammates and Loadout on the right. It scrolls vertically inside App's one scroll
  container. The data is `invoke('player_details')`, kept in the `openDetails` slot (boot.ts) so a `details` event for
  the same player replaces it in place (spec §2.4), keeping `more` and the scroll position; an event always wins over
  an answer still in flight, and leaving for Live forgets the payload. A call with no answer after 15s counts as rejected, and a rejected call (not an `invalid …` one) is retried once after
  1s; when that fails too the page settles on its empty state (`settledDetails`: `—` values, no text) and stops being busy.
  Leaving the page tells Rust (`close_details`, R145).
-->
<script lang="ts">
  import BackButton from "../components/details/BackButton.svelte";
  import Encounters from "../components/details/Encounters.svelte";
  import Hero from "../components/details/Hero.svelte";
  import Loadout from "../components/details/Loadout.svelte";
  import RecentMatches from "../components/details/RecentMatches.svelte";
  import TopTeammates from "../components/details/TopTeammates.svelte";
  import { get } from "svelte/store";
  import { closeDetails, getPlayerDetails } from "../lib/api";
  import { DETAILS_RETRY_MS, DETAILS_TIMEOUT_MS } from "../lib/boot";
  import { recentShown, settledDetails, type MatchFrom } from "../lib/details";
  import { openMatch as openScoreboard } from "../lib/router.svelte";
    import { live, openDetails } from "../lib/stores";
  import { reportError } from "../lib/uilog";

  let { puuid, more }: { puuid: string; more: boolean } = $props();

  /** The open details, only when they are this player's (a stale answer for another player is never shown). */
  const d = $derived($openDetails?.card.puuid === puuid ? $openDetails : null);
  const shown = $derived(d ? recentShown(d.recent, more) : null);

  // Fetch on open and whenever the player changes. Details already open for this player (Back from Match) stay on
  // screen and are replaced in place when the answer comes; another player's are cleared first (the skeleton shows).
  $effect(() => {
    openDetails
      .open(puuid, getPlayerDetails, {
        retryMs: DETAILS_RETRY_MS,
        timeoutMs: DETAILS_TIMEOUT_MS,
        // The hero keeps what the live card knew at that moment (read then, so this effect never depends on `live`).
        settle: (p) => settledDetails(p, get(live)?.players.find((c) => c.puuid === p)),
      })
      .catch((err: unknown) => reportError("details", err));
  });

  // Leaving the page (Back, the Live tab, a match row, Agent Select's switch to Live) unmounts it: Rust forgets the open
  // Details, so nothing is rebuilt or emitted for a page nobody sees. No dependency, so this runs on unmount only; a
  // player change on the same page never closes (the next `player_details` replaces the open Details itself). The
  // close carries the player on screen at that moment (read in the teardown, so it is no dependency), R163: Rust
  // ignores a close for anyone else and voids an open of this player still being built.
  $effect(() => () => {
    closeDetails(puuid).catch((err: unknown) => reportError("details close", err));
  });

  // LEGACY v8 page (no longer mounted; the v2 profile drawer replaces it).
  const openMatch = (from: MatchFrom) => (matchId: string) =>
    openScoreboard(matchId, { from: from === "encounter" ? "encounter" : "profile", focusPuuid: puuid });
</script>

<!--
  One tree for loading and loaded: until the answer comes every section gets null and draws its own §6.9 loading state
  (the hero's grey shapes, Recent's header and 3 rows, Top 3's and Loadout's bars; section headers are labels, not
  status text). No Encounters while loading: whether there are any is not known yet.
-->
<div class="details" aria-busy={d === null}>
  <BackButton />
  <Hero details={d} />
  <div class="body">
    <div class="bcol">
      <RecentMatches rows={shown?.rows ?? null} showMore={shown?.showMore ?? false} onopen={openMatch("recent")} />
      {#if d && !d.card.isSelf && d.encounters.length > 0}
        <Encounters rows={d.encounters} name={d.card.name} onopen={openMatch("encounter")} />
      {/if}
    </div>
    <div class="bcol">
      <TopTeammates mates={d?.topTeammates ?? null} live={$live} />
      <Loadout loadout={d?.loadout ?? null} />
    </div>
  </div>
</div>

<style>
  .body {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 296px;
    gap: 16px;
    align-items: start;
  }

  .bcol {
    display: flex;
    flex-direction: column;
    gap: 20px;
    min-width: 0;
  }
</style>
