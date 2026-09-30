<!--
  Player Details while `player_details` is on its way (spec §6.9 [C]): the hero's grey shapes, then the Recent, Top 3
  and Loadout sections in their loading state. Section headers are labels, not status text. No Encounters (whether
  there are any is not known yet).
-->
<script lang="ts">
  import Hero from "./Hero.svelte";
  import Loadout from "./Loadout.svelte";
  import RecentMatches from "./RecentMatches.svelte";
  import TopTeammates from "./TopTeammates.svelte";

  const inert = () => {};
</script>

<Hero details={null} />
<!-- The same body grid as Details (spec §6.3). -->
<div class="body">
  <div class="bcol"><RecentMatches rows={null} showMore={false} onopen={inert} /></div>
  <div class="bcol"><TopTeammates mates={null} live={null} /><Loadout loadout={null} /></div>
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
