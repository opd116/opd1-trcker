<!--
  My Stats (spec §9, CLAUDE.md "My Stats"): the user's own account over the current act, built by Rust from the DB.
  Sections in order: Rank beside the RR chart; Session; Recent Matches; Maps; Agents; Win rate by weekday beside by
  time of day; Insights. It scrolls vertically inside App's one scroll container (like Details), never sideways.
  The data is the `mystats` store (the `my_stats` answer at startup, then Rust's `mystats` events): `undefined` until
  something answered = every section draws its skeleton; `null` = Rust does not know the user or the act yet = the
  frames with `—`. A row of Recent Matches and a chip of the Session open their Match Details (the user as focus
  player, `MyStats.puuid`) with Back returning here.
-->
<script lang="ts">
  import Insights from "../components/mystats/Insights.svelte";
  import LastMatches from "../components/mystats/LastMatches.svelte";
  import RankCard from "../components/mystats/RankCard.svelte";
  import RrChart from "../components/mystats/RrChart.svelte";
  import SessionPanel from "../components/mystats/SessionPanel.svelte";
  import StatTable from "../components/mystats/StatTable.svelte";
  import WinCells from "../components/mystats/WinCells.svelte";
  import { go } from "../lib/router.svelte";
  import { mystats, refreshMyStats } from "../lib/stores";

  const s = $derived($mystats);
  /** Every section's value: undefined while loading, null when Rust knows nothing, else the data. */
  const pick = <T,>(f: (m: NonNullable<typeof s>) => T): T | null | undefined => (s === undefined ? undefined : s === null ? null : f(s));

  /** The user's id is part of the stats (`MyStats.puuid`): a row or a session chip opens its Match Details whenever the page has data. */
  const open = $derived(
    s
      ? (matchId: string) => go({ name: "match", matchId, focusPuuid: s.puuid, from: "recent", back: "mystats" })
      : undefined,
  );

  // Nothing to show yet (the first answer was null, or never came): ask again now the page is open.
  $effect(() => {
    void refreshMyStats();
  });
</script>

<div class="mystats" aria-busy={s === undefined}>
  <div class="top">
    <RankCard rank={pick((m) => m.rank)} />
    <RrChart series={s === undefined ? undefined : (s?.rrSeries ?? [])} />
  </div>
  <SessionPanel session={pick((m) => m.session)} onopen={open} />
  <LastMatches rows={pick((m) => m.last5)} onopen={open} />
  <StatTable kind="map" rows={pick((m) => m.maps)} />
  <StatTable kind="agent" rows={pick((m) => m.agents)} />
  <div class="pair">
    <WinCells kind="weekday" cells={pick((m) => m.byWeekday)} />
    <WinCells kind="daypart" cells={pick((m) => m.byDaypart)} />
  </div>
  <Insights insights={pick((m) => m.insights)} />
</div>

<style>
  .mystats {
    display: flex;
    flex-direction: column;
    gap: 20px;
  }

  /* The rank card is the row's height; the chart beside it fills the rest of the width and that height. */
  .top {
    display: grid;
    grid-template-columns: 330px minmax(0, 1fr);
    gap: 16px;
    align-items: stretch;
  }

  /* 7 weekday columns and 4 time-of-day columns, one column width each. */
  .pair {
    display: grid;
    grid-template-columns: 7fr 4fr;
    gap: 16px;
    align-items: stretch;
  }
</style>
