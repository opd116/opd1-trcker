<!--
  Win rate by weekday (Mon … Sun) or by time of day (Morning … Night) (spec §9.5): one column per bucket with its win
  rate, a small bar and its label. A bucket with no games reads `—` over an empty bar. The bar's fill is the win rate,
  coloured above / below the even mark (the tick at 50%). The tooltip is the structured W·L·games tip, its second
  row naming the slice (`This act · Monday`, `This act · 5 PM – 10 PM`); a bucket with no games has none.
  `cells: undefined` = the skeleton; null = every bucket empty.
-->
<script lang="ts">
  import { cellViews, DAYPARTS, WEEKDAYS, WEEKDAYS_LONG } from "../../lib/mystats";
  import { ACT_SCOPE, winCellTip } from "../../lib/tips";
  import { tip } from "../../lib/tooltip.svelte";
  import type { WinCell } from "../../lib/types";
  import Skel from "../Skel.svelte";
  import Panel from "./Panel.svelte";

  interface Props {
    kind: "weekday" | "daypart";
    cells: readonly WinCell[] | null | undefined;
  }

  let { kind, cells }: Props = $props();

  const labels = $derived(kind === "weekday" ? WEEKDAYS : DAYPARTS.map((d) => d.label));
  const scopes = $derived(kind === "weekday" ? WEEKDAYS_LONG : DAYPARTS.map((d) => d.hours));
  const views = $derived(cellViews(cells ?? null, labels, scopes));
</script>

<Panel title={kind === "weekday" ? "Win Rate by Weekday" : "Win Rate by Time of Day"} name={kind} scope={ACT_SCOPE} grow>
  <div class="cells" style:grid-template-columns="repeat({views.length}, minmax(0, 1fr))">
    {#each views as c (c.label)}
      {#if cells === undefined}
        <div class="cell" aria-hidden="true">
          <Skel w={32} h={14} block />
          <Skel w={24} h={60} r={4} block />
          <Skel w={28} h={12} block />
        </div>
      {:else}
        <div class="cell" use:tip={winCellTip(c.wins, c.losses, c.games, c.scope)}>
          <span class={["pct", c.tone, c.rate === null && "none"]}>{c.text}</span>
          <div class="track">
            {#if c.rate !== null}<div class={["fill", c.tone]} style:height="{c.fill * 100}%"></div>{/if}
            <div class="mid"></div>
          </div>
          <span class="lab">{c.label}</span>
        </div>
      {/if}
    {/each}
  </div>
</Panel>

<style>
  .cells {
    flex: 1;
    display: grid;
    align-items: end;
    padding: 18px 14px 14px;
  }

  .cell {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }

  .pct {
    font-size: 14px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  .pct.none {
    color: var(--text2);
    font-weight: 500;
  }

  .pct.win {
    color: var(--win);
  }

  .pct.loss {
    color: var(--loss);
  }

  .track {
    position: relative;
    width: 24px;
    height: 60px;
    display: flex;
    align-items: flex-end;
    border-radius: 4px;
    background: var(--line);
    overflow: hidden;
  }

  .fill {
    width: 100%;
    background: var(--text3);
  }

  .fill.win {
    background: var(--win);
  }

  .fill.loss {
    background: var(--loss);
  }

  /* The even mark: a hairline at 50% over the fill. */
  .mid {
    position: absolute;
    left: 0;
    right: 0;
    top: 50%;
    height: 1px;
    background: var(--card);
    opacity: 0.7;
  }

  .lab {
    font-size: 12px;
    font-weight: 500;
    color: var(--text2);
    white-space: nowrap;
  }
</style>
