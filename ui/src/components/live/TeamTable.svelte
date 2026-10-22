<!-- One team of the live board (design v2 TrackerApp.dc.html 177–257): the header strip and its rows. -->
<script lang="ts">
  import type { RowView } from "../../lib/livev2";
  import { tip } from "../../lib/tooltip.svelte";
  import PlayerRow from "./PlayerRow.svelte";

  interface Props {
    label: string;
    color: string;
    rows: RowView[];
    placeholders: number;
    onopen: (puuid: string, el: HTMLElement) => void;
  }

  let { label, color, rows, placeholders, onopen }: Props = $props();
  const total = $derived(rows.length + placeholders);
</script>

<section class="team" aria-label={label === "YOUR TEAM" ? "Your team" : "Enemy team"}>
  <div class="head">
    <div class="edge"><div style:background={color}></div></div>
    <span class="label" style:color>{label}</span>
    <span class="col">RANK</span>
    <span class="col help" use:tip={"Competitive win rate, current act"}>WIN%</span>
    <span class="col help" use:tip={"Kills per death, last 5 competitive"}>K/D</span>
    <span class="col help" use:tip={"Share of hits that landed on the head, last 5 competitive"}>HS%</span>
    <div></div>
  </div>
  {#each rows as row, i (row.puuid)}
    <PlayerRow {row} border={i < total - 1} {onopen} />
  {/each}
  {#each { length: placeholders } as _, i (i)}
    <PlayerRow row={null} border={rows.length + i < total - 1} />
  {/each}
</section>

<style>
  .team {
    flex: none;
    background: var(--card);
    border: 1px solid var(--line);
    border-radius: 10px;
    overflow: hidden;
  }

  .head {
    display: grid;
    grid-template-columns: 24px minmax(0, 2.2fr) minmax(0, 1.9fr) 72px 64px 64px 20px;
    column-gap: 12px;
    align-items: center;
    height: 38px;
    padding-right: 10px;
    background: var(--card-head);
    border-bottom: 1px solid var(--line);
  }

  .edge {
    height: 100%;
    display: flex;
  }

  .edge > div {
    width: 3px;
    height: 100%;
  }

  .label {
    font: 700 15px/1 var(--font-display);
    letter-spacing: 0.1em;
  }

  .col {
    font: 500 10.5px/1 var(--font-mono);
    letter-spacing: 0.06em;
    color: var(--text-label);
  }

  .help {
    cursor: help;
    justify-self: start;
  }
</style>
