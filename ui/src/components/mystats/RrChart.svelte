<!--
  The RR progression chart (spec §9.2): one inline SVG line of `absoluteRr` over the act, oldest → newest, evenly
  spaced by match; no chart library. Tier boundaries are hairline gridlines and each tier band is named in the left
  gutter (a band too thin for a 12px label goes unnamed). The newest point is marked; hovering a point shows its
  crosshair and a tooltip `+18 RR · Ascendant 2 · Sep 29`. Fewer than 2 points: an empty frame with `—`.
  `series: undefined` = the skeleton.
-->
<script lang="ts">
  import { dateShort, DASH } from "../../lib/format";
  import { bandName, chartModel, tierOfAbsolute, type ChartBox } from "../../lib/mystats";
  import { meta, now } from "../../lib/stores";
  import { ACT_SCOPE, rrPointTip } from "../../lib/tips";
  import { tip } from "../../lib/tooltip.svelte";
  import type { RrPoint } from "../../lib/types";
  import Panel from "./Panel.svelte";

  let { series }: { series: RrPoint[] | undefined } = $props();

  /** The plot's room: tier names on the left, the end dates under it. */
  const PAD = { padL: 96, padR: 18, padT: 16, padB: 30 };
  /** A point's hover target: as tall as a finger-sized box, as wide as its slice of the line. */
  const HIT_H = 28;

  let w = $state(0);
  let h = $state(0);

  const box = $derived<ChartBox>({ w, h, ...PAD });
  const model = $derived(series === undefined ? null : chartModel(series, box));
  const step = $derived(model && model.points.length > 1 ? (model.plot.x1 - model.plot.x0) / (model.points.length - 1) : 0);
  const last = $derived(model ? model.points[model.points.length - 1]! : null);
  const first = $derived(model ? model.points[0]! : null);
</script>

<Panel title="RR Progression" name="rrchart" scope={ACT_SCOPE} grow>
  <div class="chart" bind:clientWidth={w} bind:clientHeight={h}>
    {#if series === undefined}
      <div class="skel" aria-hidden="true"></div>
    {:else if model === null}
      <div class="none">{DASH}</div>
    {:else if first && last}
      <svg width={w} height={h} viewBox="0 0 {w} {h}" role="img" aria-label="RR progression">
        {#each model.grid as g, i (i)}
          <line class="grid" x1={model.plot.x0} x2={model.plot.x1} y1={g.y} y2={g.y} />
        {/each}
        {#each model.bands as b, i (i)}
          {@const name = bandName($meta, b.tier, true)}
          {#if b.labelY !== null && typeof name === "string"}
            <text class="band" x={model.plot.x0 - 12} y={b.labelY} text-anchor="end" dominant-baseline="central">{name}</text>
          {/if}
        {/each}
        <path class="area" d={model.area} />
        <path class="line" d={model.line} />
        {#each model.points as p, i (i)}
          <g class="pt">
            <line class="xh" x1={p.x} x2={p.x} y1={model.plot.y0} y2={model.plot.y1} />
            <circle class="hot" cx={p.x} cy={p.y} r="4" />
            <rect
              class="hit"
              x={Math.max(p.x - step / 2, 0)}
              y={p.y - HIT_H / 2}
              width={step}
              height={HIT_H}
              use:tip={rrPointTip(p.delta, bandName($meta, tierOfAbsolute(p.point.absoluteRr)) ?? null, p.point.startMs, $now)}
            />
          </g>
        {/each}
        <circle class="end" cx={last.x} cy={last.y} r="4" />
        <text class="date" x={model.plot.x0} y={h - 10} text-anchor="start">{dateShort(first.point.startMs, $now)}</text>
        <text class="date" x={model.plot.x1} y={h - 10} text-anchor="end">{dateShort(last.point.startMs, $now)}</text>
      </svg>
    {/if}
  </div>
</Panel>

<style>
  /* The wrapper sets the chart's size and never its own: the SVG is out of flow, so the row's height is the Rank
     card's, and the chart follows it. */
  .chart {
    position: relative;
    flex: 1;
    min-height: 260px;
  }

  svg {
    position: absolute;
    inset: 0;
    display: block;
    overflow: visible;
  }

  .none {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 14px;
    font-weight: 500;
    color: var(--text2);
  }

  .skel {
    position: absolute;
    inset: 16px 18px;
    border-radius: 6px;
    background: var(--skel);
  }

  .grid {
    stroke: var(--line);
    stroke-width: 1;
  }

  .band,
  .date {
    fill: var(--text3);
    font-size: 12px;
    font-weight: 500;
  }

  .area {
    fill: var(--accent);
    fill-opacity: 0.09;
  }

  .line {
    fill: none;
    stroke: var(--accent);
    stroke-width: 2;
    stroke-linejoin: round;
    stroke-linecap: round;
  }

  /* The newest point: an 8px dot with a 2px ring in the panel colour, so it stays whole where it meets the line. It
     never takes the pointer: its point's hit area below does (and shows the tooltip). */
  .end,
  .hot {
    fill: var(--accent);
    stroke: var(--card);
    stroke-width: 2;
    pointer-events: none;
  }

  .hit {
    fill: transparent;
  }

  .xh {
    stroke: var(--line2);
    stroke-width: 1;
  }

  .xh,
  .hot {
    opacity: 0;
  }

  .xh {
    pointer-events: none;
  }

  .pt:hover .xh,
  .pt:hover .hot {
    opacity: 1;
  }
</style>
