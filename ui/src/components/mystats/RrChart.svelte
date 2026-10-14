<!--
  The RR progression chart (spec §9.2): one inline SVG line of `absoluteRr` over the act, oldest → newest, evenly
  spaced by match; no chart library. Tier boundaries are hairline gridlines and each tier band is named in the left
  gutter (a band too thin for a 12px label goes unnamed); above Immortal 1's floor, where there are no tiers, 3–4
  round-number lines are labelled `N RR` instead. The newest point is marked. One transparent layer over the plot
  takes the pointer and picks the point nearest it: its crosshair and dot show and the tooltip is that point's
  `+18 RR · Ascendant 2 · 47 RR · Sep 29`. The keyboard gets the same pick: the plot is one tab stop (a slider over the
  points); focus picks the newest point, ←/→ move it, Home/End jump to the ends, and leaving the chart clears it.
  Fewer than 2 points: an empty frame with `—`. `series: undefined` = the skeleton.
-->
<script lang="ts">
  import { dateShort, DASH } from "../../lib/format";
  import { bandName, chartModel, keyTarget, tierOfAbsolute, valueIndex, type ChartBox, type ChartPick, type ChartPoint } from "../../lib/mystats";
  import { meta, now } from "../../lib/stores";
  import { ACT_SCOPE, rrPointTip } from "../../lib/tips";
  import { retip, tip, tipState } from "../../lib/tooltip.svelte";
  import type { RrPoint } from "../../lib/types";
  import Panel from "./Panel.svelte";

  let { series }: { series: RrPoint[] | undefined } = $props();

  /** The plot's room: tier names on the left, the end dates under it. */
  const PAD = { padL: 96, padR: 18, padT: 16, padB: 30 };
  /** The tooltip's anchor box around the hovered point: the bubble sits under it, centred on the point. */
  const ANCHOR_H = 28;

  let w = $state(0);
  let h = $state(0);
  /**
   * The picked point (the nearest to the pointer, or the keyboard's), or null. It is kept by its match, not by its
   * index: a `mystats` push that inserts older points must not move the pick to another match.
   */
  let pick = $state<ChartPick | null>(null);
  let anchor = $state<SVGRectElement | null>(null);
  /** The focusable wrapper (the one tab stop). */
  let stop = $state<HTMLElement | null>(null);
  /** The slice the pointer last picked, null while it is off the plot: the early return of `move`, apart from the keyboard's pick. */
  let pointerSlice: number | null = null;
  /** The keyboard has the chart (focused with `:focus-visible`, or used a key on it): the pointer leaving does not clear its pick. Cleared on blur. */
  let keyPick = false;

  const box = $derived<ChartBox>({ w, h, ...PAD });
  const model = $derived(series === undefined ? null : chartModel(series, box));
  const step = $derived(model && model.points.length > 1 ? (model.plot.x1 - model.plot.x0) / (model.points.length - 1) : 0);
  const last = $derived(model ? model.points[model.points.length - 1]! : null);
  const first = $derived(model ? model.points[0]! : null);
  /** The picked point's index (its match's, in the series as it is now), or null with no pick. */
  const hover = $derived(model && pick ? valueIndex(model.points, pick) : null);
  const hot = $derived(model && hover !== null ? model.points[hover]! : null);
  const textOf = (p: ChartPoint): string =>
    rrPointTip(p.delta, bandName($meta, tierOfAbsolute(p.point.absoluteRr)) ?? null, p.rr, p.point.startMs, $now);
  /** The tip of the hovered point; the anchor keeps the first point's text while nothing is hovered (it never shows). */
  const tipText = $derived.by(() => {
    const p = hot ?? first;
    return p ? textOf(p) : "";
  });
  /** The slider's value for assistive tech: the picked point, or the newest while nothing is picked. */
  const valueNow = $derived(model ? valueIndex(model.points, pick) : 0);
  const valueText = $derived(model ? textOf(model.points[valueNow]!) : "");

  // The plot went away (a series of fewer than 2 points, a box too small): nothing stays picked. The focused wrapper is
  // gone with it, and a removed element sends no blur.
  $effect(() => {
    if (model) return;
    pick = null;
    pointerSlice = null;
    keyPick = false;
  });

  const pickAt = (i: number): void => {
    pick = { ms: model!.points[i]!.point.startMs, idx: i };
  };

  /**
   * One `pointermove` for the whole plot: the nearest point by x. A new point restarts the tip (the engine saw no
   * `mouseover`). The same slice again changes nothing, unless the pick is elsewhere (the keyboard's, or the series
   * moved under the pointer) or the pointer has just entered: it then takes the pick and shows its own tip.
   */
  function move(e: PointerEvent): void {
    if (!model || !first || step <= 0) return;
    const x = e.clientX - (e.currentTarget as Element).getBoundingClientRect().left + (model.plot.x0 - step / 2);
    const i = Math.max(0, Math.min(model.points.length - 1, Math.round((x - first.x) / step)));
    if (i === pointerSlice && i === hover) return;
    pointerSlice = i;
    pickAt(i);
    if (anchor) retip(anchor);
  }

  /** The pointer leaves the plot: the pick goes, unless the keyboard has the chart (its pick stays until blur). */
  function leave(): void {
    pointerSlice = null;
    if (!keyPick) pick = null;
  }

  /** Keyboard focus (never a mouse click's) picks the newest point and shows its tip. */
  function focus(e: FocusEvent): void {
    if (!model || !(e.currentTarget as Element).matches(":focus-visible")) return;
    keyPick = true;
    pickAt(model.points.length - 1);
    if (anchor && stop) retip(anchor, stop, false);
  }

  function blur(): void {
    keyPick = false;
    pick = null;
  }

  /** ←/→ move the pick (clamped), Home/End jump to the ends; any other key is left alone. A handled key shows its point's tip again. */
  function keydown(e: KeyboardEvent): void {
    if (!model) return;
    const next = keyTarget(e, valueNow, model.points.length - 1);
    if (next === null) return;
    e.preventDefault();
    keyPick = true;
    // The same point with its bubble up (an arrow at either end) changes nothing: no flicker.
    if (next === hover && tipState.tip) return;
    pickAt(next);
    if (anchor && stop) retip(anchor, stop, false);
  }
</script>

<Panel title="RR Progression" name="rrchart" scope={ACT_SCOPE} grow>
  <div class="chart" bind:clientWidth={w} bind:clientHeight={h}>
    {#if series === undefined}
      <div class="skel" aria-hidden="true"></div>
    {:else if model === null}
      <div class="none">{DASH}</div>
    {:else if first && last}
      <!-- The one tab stop: a slider over the points (←/→, Home/End); its value text is the picked point's tip. -->
      <div
        class="stop"
        bind:this={stop}
        role="slider"
        tabindex="0"
        aria-label="RR progression"
        aria-orientation="horizontal"
        aria-valuemin={0}
        aria-valuemax={model.points.length - 1}
        aria-valuenow={valueNow}
        aria-valuetext={valueText}
        onfocus={focus}
        onblur={blur}
        onkeydown={keydown}
      >
        <svg width={w} height={h} viewBox="0 0 {w} {h}" role="img" aria-label="RR progression">
          {#each model.grid as g, i (i)}
            <line class="grid" x1={model.plot.x0} x2={model.plot.x1} y1={g.y} y2={g.y} />
            {#if g.label !== null}
              <text class="rrmark" x={model.plot.x0 - 12} y={g.y} text-anchor="end" dominant-baseline="central">{g.label}</text>
            {/if}
          {/each}
          {#each model.bands as b, i (i)}
            {@const name = bandName($meta, b.tier, true)}
            {#if b.labelY !== null && typeof name === "string"}
              <text class="band" x={model.plot.x0 - 12} y={b.labelY} text-anchor="end" dominant-baseline="central">{name}</text>
            {/if}
          {/each}
          <path class="area" d={model.area} />
          <path class="line" d={model.line} />
          {#if hot}
            <line class="xh" x1={hot.x} x2={hot.x} y1={model.plot.y0} y2={model.plot.y1} />
            <circle class="hot" cx={hot.x} cy={hot.y} r="4" />
          {/if}
          <circle class="end" cx={last.x} cy={last.y} r="4" />
          <text class="date" x={model.plot.x0} y={h - 10} text-anchor="start">{dateShort(first.point.startMs, $now)}</text>
          <text class="date" x={model.plot.x1} y={h - 10} text-anchor="end">{dateShort(last.point.startMs, $now)}</text>
          <!-- The tooltip's trigger: a box around the hovered point, never hit itself (the layer below is). -->
          <rect
            bind:this={anchor}
            class="anchor"
            x={(hot ?? first).x - Math.max(step, 12) / 2}
            y={(hot ?? first).y - ANCHOR_H / 2}
            width={Math.max(step, 12)}
            height={ANCHOR_H}
            use:tip={tipText}
          />
          <rect
            class="hit"
            role="presentation"
            x={model.plot.x0 - step / 2}
            y={model.plot.y0}
            width={model.plot.x1 - model.plot.x0 + step}
            height={model.plot.y1 - model.plot.y0}
            onpointermove={move}
            onpointerleave={leave}
          />
        </svg>
      </div>
    {/if}
  </div>
</Panel>

<style>
  /* The wrapper sets the chart's size and never its own: the SVG is out of flow, so the row's height is the Rank
     card's, and the chart follows it. */
  .chart {
    position: relative;
    flex: 1;
    min-height: 220px;
  }

  /* The tab stop is the chart's whole box. Its ring is inset (the panel clips) and follows the panel's corners, like
     the last row of a table: the global 2px-outside ring would be cut off. */
  .stop {
    position: absolute;
    inset: 0;
    border-radius: 8px;
  }

  .stop:focus-visible {
    outline-offset: -2px;
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

  /* Tier names, RR marks and the end dates: the quiet labels of the frame (--text2: --text3 is too dim at 12px on
     the card colour). */
  .band,
  .rrmark,
  .date {
    fill: var(--text2);
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

  /* The newest point and the hovered one: an 8px dot with a 2px ring in the panel colour, so it stays whole where it
     meets the line. Neither takes the pointer: the layer above them does. */
  .end,
  .hot {
    fill: var(--accent);
    stroke: var(--card);
    stroke-width: 2;
    pointer-events: none;
  }

  .xh {
    stroke: var(--line2);
    stroke-width: 1;
    pointer-events: none;
  }

  .anchor {
    fill: transparent;
    pointer-events: none;
  }

  .hit {
    fill: transparent;
  }
</style>
