<!--
  Competitive · RR PROGRESSION (design v2 TrackerApp.dc.html 691–726, logic `extraVals2`): the act's last 40 RR points
  as an area + line over even columns, a line every 100 RR labelled with the tier that starts there (short name, tier
  colour; "N RR" lines above Immortal 1's floor), the session shaded as "Today", the end dot, and four dates under it.
  Hovering a column shows its crosshair, dot and the 220px card (when, result · map · score, RR moved → where it
  landed). The plot is one tab stop (a slider over the points, as the old RrChart): focus picks the newest point, ←/→
  move, Home/End jump, Enter opens the match. A point opens its scoreboard when the snapshot knows its match (`last5` or
  the session, matched by start time); others are not clickable.
-->
<script lang="ts">
  import {
    axisLabels,
    netLabel,
    netTip,
    pointAfter,
    pointText,
    pointWhen,
    rrChart,
    type KnownMatch,
    type RrChartPoint,
  } from "../../lib/compv2";
  import { DASH, rrDelta } from "../../lib/format";
  import { mapName } from "../../lib/meta";
  import { keyTarget, valueIndex, type ChartPick } from "../../lib/mystats";
  import { meta, now } from "../../lib/stores";
  import { tip } from "../../lib/tooltip.svelte";
  import type { MyStats } from "../../lib/types";

  let {
    stats,
    known,
    onopen,
  }: { stats: MyStats | null | undefined; known: Map<number, KnownMatch>; onopen?: (matchId: string) => void } = $props();

  const view = $derived(stats ? rrChart(stats.rrSeries, known, stats.session, $meta) : null);
  const axis = $derived(view ? axisLabels(view.points, $now) : []);
  const last = $derived(view ? view.points[view.points.length - 1]! : null);
  const title = $derived(
    view === null
      ? "RR PROGRESSION"
      : view.total > view.n
        ? `RR PROGRESSION · LAST ${view.n} OF ${view.total} MATCHES`
        : `RR PROGRESSION · ${view.n} ${view.n === 1 ? "MATCH" : "MATCHES"}`,
  );

  let pick = $state<ChartPick | null>(null);
  /** The keyboard has the chart: the pointer leaving keeps its pick (cleared on blur). */
  let keyPick = false;

  const wrapped = $derived(view ? view.points.map((p) => ({ point: p })) : []);
  const hover = $derived(view && pick ? valueIndex(wrapped, pick) : null);
  const hot = $derived(view && hover !== null ? view.points[hover]! : null);
  const valueNow = $derived(view ? valueIndex(wrapped, pick) : 0);

  const mapOf = (p: RrChartPoint): string | null => {
    const m = p.match ? mapName($meta, p.match.mapId) : null;
    return typeof m === "string" && m ? m : null;
  };
  const valueText = $derived(view ? pointText(view.points[valueNow]!, view.n, mapOf(view.points[valueNow]!), $meta) : "");

  const pickAt = (i: number) => {
    if (view) pick = { ms: view.points[i]!.startMs, idx: i };
  };

  function leave() {
    if (!keyPick) pick = null;
  }

  function focus(e: FocusEvent) {
    if (!view || !(e.currentTarget as Element).matches(":focus-visible")) return;
    keyPick = true;
    pickAt(view.points.length - 1);
  }

  function blur() {
    keyPick = false;
    pick = null;
  }

  function keydown(e: KeyboardEvent) {
    if (!view) return;
    if ((e.key === "Enter" || e.key === " ") && !e.ctrlKey && !e.altKey && !e.metaKey) {
      const p = view.points[valueNow];
      if (p?.match && onopen) {
        e.preventDefault();
        onopen(p.match.matchId);
      }
      return;
    }
    const next = keyTarget(e, valueNow, view.points.length - 1);
    if (next === null) return;
    e.preventDefault();
    keyPick = true;
    pickAt(next);
  }

  const resBg = (r: string | null) => (r === "W" ? "var(--win-soft)" : r === "L" ? "var(--loss-soft)" : "var(--button-hover)");
  const resFg = (r: string | null) => (r === "W" ? "var(--win)" : r === "L" ? "var(--loss)" : "var(--text-muted)");
  const netColor = (n: number) => (n > 0 ? "var(--win)" : n < 0 ? "var(--loss)" : "var(--text)");
</script>

<section class="card" aria-label="RR progression" aria-busy={stats === undefined}>
  <div class="hdr">
    <span class="micro">{title}</span>
    <span class="flex"></span>
    {#if view}
      <span class="net" style:color={netColor(view.net)} use:tip={netTip(view, stats?.rank.netRr ?? null)}>{netLabel(view.net)}</span>
      <span class="hint">over these {view.n} · hover a point</span>
    {/if}
  </div>

  {#if stats === undefined}
    <div class="plotwrap"><div class="skel" aria-hidden="true"></div></div>
    <div class="axis"></div>
  {:else if view === null || last === null}
    <div class="plotwrap"><div class="none">{DASH}</div></div>
    <div class="axis"></div>
  {:else}
    <div
      class="plotwrap"
      role="slider"
      tabindex="0"
      aria-label="RR progression"
      aria-orientation="horizontal"
      aria-valuemin={0}
      aria-valuemax={view.n - 1}
      aria-valuenow={valueNow}
      aria-valuetext={valueText}
      onmouseleave={leave}
      onfocus={focus}
      onblur={blur}
      onkeydown={keydown}
    >
      {#each view.bands as b, i (i)}
        <div class="band" style:top="{b.topPct.toFixed(1)}%">
          <span class="blabel" style:color={b.color}>{b.label}</span>
          <span class="bline"></span>
        </div>
      {/each}
      <div class="plot">
        {#if view.todayLeftPct !== null}
          <div class="today" style:left="{view.todayLeftPct.toFixed(2)}%"><span>Today</span></div>
        {/if}
        <svg viewBox="0 0 800 200" preserveAspectRatio="none" aria-hidden="true">
          <defs>
            <linearGradient id="opdRrFill" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0" style="stop-color: var(--accent); stop-opacity: 0.22"></stop>
              <stop offset="1" style="stop-color: var(--accent); stop-opacity: 0"></stop>
            </linearGradient>
          </defs>
          <path d={view.area} fill="url(#opdRrFill)"></path>
          <path class="line" d={view.line} vector-effect="non-scaling-stroke"></path>
        </svg>
        <div class="enddot" style:left="{last.xPct.toFixed(2)}%" style:top="{last.yPct.toFixed(2)}%"></div>
        <div class="cols">
          {#each view.points as p (p.i)}
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div
              class={["col", p.match && onopen && "open"]}
              onmouseenter={() => {
                if (hover !== p.i) pickAt(p.i);
              }}
              onclick={() => {
                if (p.match && onopen) onopen(p.match.matchId);
              }}
            ></div>
          {/each}
        </div>
        {#if hot}
          {@const map = mapOf(hot)}
          <div class="xh" style:left="{hot.xPct.toFixed(2)}%"></div>
          <div class="hotdot" style:left="{hot.xPct.toFixed(2)}%" style:top="{hot.yPct.toFixed(2)}%"></div>
          <div
            class="hcard"
            style:left="{hot.xPct.toFixed(2)}%"
            style:transform="translateX({hot.i > view.n / 2 ? 'calc(-100% - 12px)' : '12px'})"
          >
            <span class="when">{pointWhen(hot, view.n)}</span>
            {#if hot.result || map || hot.match?.score}
              <div class="res">
                {#if hot.result}
                  <span class="chip" style:background={resBg(hot.result)} style:color={resFg(hot.result)}>{hot.result}</span>
                {/if}
                {#if map}<span class="map" data-ellipsis>{map}</span>{/if}
                {#if hot.match?.score}<span class="score">{hot.match.score}</span>{/if}
              </div>
            {/if}
            <span class="after">
              {#if hot.delta !== null}
                <span class="delta" style:color={netColor(hot.delta)}>{rrDelta(hot.delta)} RR</span> →
              {/if}
              {pointAfter(hot, $meta)}
            </span>
          </div>
        {/if}
      </div>
    </div>
    <div class="axis">
      {#each axis as x, i (i)}<span>{x}</span>{/each}
    </div>
  {/if}
</section>

<style>
  .card {
    background: var(--card);
    border: 1px solid var(--line);
    border-radius: 12px;
    padding: 16px 18px 14px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    min-width: 0;
  }

  .hdr {
    display: flex;
    align-items: baseline;
    gap: 12px;
    white-space: nowrap;
  }

  .micro {
    font: 500 10.5px/1 var(--font-mono);
    letter-spacing: 0.06em;
    color: var(--text-muted);
  }

  .flex {
    flex: 1;
  }

  .net {
    font: 700 13.5px/1 var(--font);
    cursor: help;
    font-variant-numeric: tabular-nums;
  }

  .hint {
    font: 500 12.5px/1 var(--font);
    color: var(--text-muted);
  }

  .plotwrap {
    position: relative;
    flex: 1;
    min-height: 200px;
    border-radius: 6px;
  }

  .plotwrap:focus-visible {
    outline-offset: 4px;
  }

  .skel {
    position: absolute;
    inset: 0 6px 0 56px;
    border-radius: 6px;
    background: var(--skel);
  }

  .none {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    font: 500 14px/1 var(--font);
    color: var(--text-muted);
  }

  .band {
    position: absolute;
    left: 0;
    right: 0;
    display: flex;
    align-items: center;
    gap: 10px;
    pointer-events: none;
  }

  .blabel {
    width: 46px;
    flex: none;
    font: 600 12px/1 var(--font);
    text-align: right;
    transform: translateY(-50%);
    white-space: nowrap;
  }

  .bline {
    flex: 1;
    height: 1px;
    background: var(--line-media);
    transform: translateY(-50%);
  }

  .plot {
    position: absolute;
    left: 56px;
    right: 6px;
    top: 0;
    bottom: 0;
  }

  .today {
    position: absolute;
    top: 0;
    bottom: 0;
    right: 0;
    background: var(--accent-faint);
    border-left: 1px dashed rgba(157, 140, 255, 0.45);
    pointer-events: none;
  }

  .today span {
    position: absolute;
    top: 4px;
    left: 6px;
    font: 600 11.5px/1 var(--font);
    color: #c9c2ff;
  }

  svg {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    overflow: visible;
    pointer-events: none;
  }

  .line {
    fill: none;
    stroke: var(--accent);
    stroke-width: 2;
    stroke-linejoin: round;
  }

  .enddot,
  .hotdot {
    position: absolute;
    width: 10px;
    height: 10px;
    margin: -5px 0 0 -5px;
    border-radius: 50%;
    pointer-events: none;
    box-sizing: border-box;
  }

  .enddot {
    background: var(--accent);
    box-shadow: 0 0 0 4px rgba(157, 140, 255, 0.18);
  }

  .hotdot {
    background: var(--text);
    border: 2px solid var(--accent);
  }

  .cols {
    position: absolute;
    inset: 0;
    display: flex;
  }

  .col {
    flex: 1;
    height: 100%;
    cursor: crosshair;
  }

  .col.open {
    cursor: pointer;
  }

  .xh {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 1px;
    background: rgba(236, 235, 231, 0.25);
    pointer-events: none;
  }

  .hcard {
    position: absolute;
    top: 22px;
    width: 220px;
    box-sizing: border-box;
    padding: 10px 12px;
    border-radius: 8px;
    background: var(--tip-bg);
    border: 1px solid var(--line-tip);
    box-shadow: 0 10px 24px rgba(0, 0, 0, 0.5);
    display: flex;
    flex-direction: column;
    gap: 7px;
    pointer-events: none;
    z-index: 2;
  }

  .when {
    font: 500 12px/1 var(--font);
    color: var(--text-muted);
    white-space: nowrap;
  }

  .res {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }

  .chip {
    width: 20px;
    height: 20px;
    flex: none;
    border-radius: 4px;
    display: flex;
    align-items: center;
    justify-content: center;
    font: 700 10.5px/1 var(--font-mono);
  }

  .map {
    font: 600 14px/1 var(--font);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }

  .score {
    font: 700 14px/1 var(--font);
    color: var(--text-mid);
    font-variant-numeric: tabular-nums;
    flex: none;
  }

  .after {
    font: 500 13px/1.3 var(--font);
    color: var(--text-mid);
  }

  .delta {
    font-weight: 700;
  }

  .axis {
    display: flex;
    justify-content: space-between;
    padding-left: 56px;
    min-height: 12px;
    font: 500 12px/1 var(--font);
    color: var(--text-muted);
  }
</style>
