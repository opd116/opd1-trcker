<!--
  Player Details (spec §6.1–§6.9, v8:163-356): Back, the hero, then Recent Competitive Matches (+ Show more) and
  Encounters on the left, Top 3 Teammates and Loadout on the right. It scrolls vertically inside App's one scroll
  container. The data is `invoke('player_details')`, kept in the `openDetails` slot (boot.ts) so a `details` event for
  the same player replaces it in place (spec §2.4), keeping `more` and the scroll position; an event always wins over
  an answer still in flight, and leaving for Live forgets the payload.
-->
<script lang="ts">
  import BackButton from "../components/details/BackButton.svelte";
  import Encounters from "../components/details/Encounters.svelte";
  import Hero from "../components/details/Hero.svelte";
  import Loadout from "../components/details/Loadout.svelte";
  import RecentMatches from "../components/details/RecentMatches.svelte";
  import TopTeammates from "../components/details/TopTeammates.svelte";
  import { getPlayerDetails } from "../lib/api";
  import { recentShown, type MatchFrom } from "../lib/details";
  import { go } from "../lib/router.svelte";
  import { live, openDetails } from "../lib/stores";

  let { puuid, more }: { puuid: string; more: boolean } = $props();

  /** The open details, only when they are this player's (a stale answer for another player is never shown). */
  const d = $derived($openDetails?.card.puuid === puuid ? $openDetails : null);
  const shown = $derived(d ? recentShown(d.recent, more) : null);

  // Fetch on open and whenever the player changes. Details already open for this player (Back from Match) stay on
  // screen and are replaced in place when the answer comes; another player's are cleared first (the skeleton shows).
  $effect(() => {
    openDetails.open(puuid, getPlayerDetails).catch((err: unknown) => console.error("player_details failed", err));
  });

  const openMatch = (from: MatchFrom) => (matchId: string) =>
    go({ name: "match", matchId, focusPuuid: puuid, from, back: { puuid, more } });
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
