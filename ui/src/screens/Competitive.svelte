<!--
  Competitive (design v2 TrackerApp.dc.html 658–792; replaces My Stats, R189): the user's own account over the current
  act, from the `mystats` store (Rust's `my_stats` snapshot and `mystats` events). Header; RANK NOW beside RR
  PROGRESSION; TODAY'S SESSION; MAPS, AGENTS and BEST TIMES. `undefined` = every card's skeleton; `null` (Rust does not
  know the user or the act yet) = the frames with `—`. A chart point or a session chip opens its scoreboard (History's
  match view, Back returns here). The snapshot's insights have no slot in the v2 design and are not shown.
-->
<script lang="ts">
  import BestTimes from "../components/comp/BestTimes.svelte";
  import RankNow from "../components/comp/RankNow.svelte";
  import RrProgression from "../components/comp/RrProgression.svelte";
  import SessionStrip from "../components/comp/SessionStrip.svelte";
  import StatCard from "../components/comp/StatCard.svelte";
  import { knownMatches, rrChart, subtitle } from "../lib/compv2";
  import { openMatch, scrollHost } from "../lib/router.svelte";
  import { myStatsForSelf, now, refreshMyStats } from "../lib/stores";

  const s = $derived($myStatsForSelf);
  const known = $derived(s ? knownMatches(s) : new Map());
  /** The session's matches are shaded on the chart when any of them is among its points. */
  const shaded = $derived(!!s?.session && (rrChart(s.rrSeries, known, s.session)?.todayLeftPct ?? null) !== null);
  const open = $derived(s ? (matchId: string) => openMatch(matchId, { from: "comp", focusPuuid: s.puuid }) : undefined);
  const pick = <T,>(f: (m: NonNullable<typeof s>) => T): T | null | undefined => (s === undefined ? undefined : s === null ? null : f(s));

  // Nothing to show yet (the first answer was null, or never came): ask again now the page is open.
  $effect(() => {
    void refreshMyStats();
  });
</script>

<!-- `mystats`: the shared test harness waits on `.mystats[aria-busy]` for the Competitive scenes. -->
<div class="screen mystats" use:scrollHost aria-busy={s === undefined}>
  <header class="head">
    <h1>Competitive</h1>
    <span class="sub">{subtitle(s, $now)}</span>
  </header>
  <div class="top">
    <RankNow stats={s} {known} />
    <RrProgression stats={s} {known} onopen={open} />
  </div>
  <SessionStrip session={pick((m) => m.session)} {known} {shaded} onopen={open} />
  <div class="three">
    <StatCard kind="map" rows={pick((m) => m.maps)} />
    <StatCard kind="agent" rows={pick((m) => m.agents)} />
    <BestTimes weekdays={pick((m) => m.byWeekday)} dayparts={pick((m) => m.byDaypart)} />
  </div>
</div>

<style>
  .screen {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    overflow-x: hidden;
    display: flex;
    flex-direction: column;
    padding: 18px 24px 20px;
    gap: 12px;
  }

  .head {
    display: flex;
    flex-direction: column;
    gap: 9px;
    flex: none;
  }

  h1 {
    margin: 0;
    font: 700 32px/1 var(--font-display);
    letter-spacing: 0.03em;
    text-transform: uppercase;
  }

  .sub {
    font: 500 13px/1 var(--font);
    color: var(--text-muted);
  }

  .top {
    flex: none;
    display: grid;
    grid-template-columns: 340px minmax(0, 1fr);
    gap: 12px;
  }

  .three {
    flex: none;
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 12px;
  }
</style>
