<!--
  Competitive · TODAY'S SESSION (design v2 TrackerApp.dc.html 728–740): net RR, record, start time + length, then one
  chip per match (result, map, RR), newest on the right; a chip opens that match's scoreboard. As many of the newest
  chips as fit (each at least 64px, so the map and RR never crowd); the count says how many there are. No session (the
  last match is over 6 h old): the same strip with `—` values.
-->
<script lang="ts">
  import { exactWhen, hoursMinutes, plural, sessionEnd, type KnownMatch } from "../../lib/compv2";
  import { DASH, rrDelta, time12 } from "../../lib/format";
  import { mapName } from "../../lib/meta";
  import { meta, now } from "../../lib/stores";
  import { tip } from "../../lib/tooltip.svelte";
  import type { SessionSummary } from "../../lib/types";
  import Skel from "../Skel.svelte";

  let {
    session,
    known,
    shaded,
    onopen,
  }: {
    session: SessionSummary | null | undefined;
    known: Map<number, KnownMatch>;
    /** The session's points show as the chart's "Today" region. */
    shaded: boolean;
    onopen?: (matchId: string) => void;
  } = $props();

  const MIN_CHIP = 64;
  const GAP = 6;
  /** `.chips`' padding-left: `clientWidth` includes it, the chips can't use it. */
  const PAD = 22;
  let chipsW = $state(0);
  const fit = $derived(Math.max(1, Math.floor((chipsW - PAD + GAP) / (MIN_CHIP + GAP))));

  const chips = $derived.by(() => {
    const all = session?.matches ?? [];
    return all.slice(Math.max(0, all.length - fit)).map((m) => {
      const map = mapName($meta, m.mapId);
      const mapText = typeof map === "string" && map ? map : DASH;
      const score = known.get(m.startMs)?.score ?? null;
      const word = m.result === "W" ? "won" : m.result === "L" ? "lost" : "drew";
      return {
        id: m.matchId,
        l: m.result,
        map: mapText,
        rr: typeof m.rrEarned === "number" ? rrDelta(m.rrEarned) : DASH,
        tip: `${mapText} · ${word}${score ? ` ${score}` : ""} · ${time12(m.startMs)}`,
        bg: m.result === "W" ? "var(--win-soft)" : m.result === "L" ? "var(--loss-soft)" : "var(--button-hover)",
        fg: m.result === "W" ? "var(--win)" : m.result === "L" ? "var(--loss)" : "var(--text-muted)",
      };
    });
  });

  const net = $derived(session?.netRr ?? null);
  const netColor = $derived(net === null || Math.round(net) === 0 ? "var(--text)" : net > 0 ? "var(--win)" : "var(--loss)");
  const sub = $derived(
    session ? plural(session.matches.length, "match", "matches") + (shaded ? " · shaded on the chart" : "") : "No match in the last 6 h",
  );
</script>

<section class="strip" aria-label="Today's session" aria-busy={session === undefined}>
  <div class="lead">
    <span class="micro">TODAY’S SESSION</span>
    {#if session === undefined}<Skel w={150} h={12} block />{:else}<span class="lab">{sub}</span>{/if}
  </div>
  <div class="blk">
    <span class="lab">RR</span>
    {#if session === undefined}<Skel w={44} h={24} block />{:else}<span class="big" style:color={netColor}>{net === null ? DASH : rrDelta(net)}</span>{/if}
  </div>
  <div class="blk">
    <span class="lab">Record</span>
    {#if session === undefined}<Skel w={44} h={20} block />{:else}<span class="rec">{session ? `${session.wins}–${session.losses}` : DASH}</span>{/if}
  </div>
  <div class="blk">
    <span class="lab">Started</span>
    {#if session === undefined}
      <Skel w={120} h={17} block />
    {:else if session}
      <span class="start" use:tip={exactWhen(session.startedMs)}
        >{time12(session.startedMs)} <span class="dur">· {hoursMinutes(sessionEnd(session, $now) - session.startedMs)}</span></span
      >
    {:else}
      <span class="start">{DASH}</span>
    {/if}
  </div>
  <div class="chips" bind:clientWidth={chipsW}>
    {#each chips as c (c.id)}
      <button
        type="button"
        class="chip"
        use:tip={c.tip}
        aria-label={`${c.tip} · ${c.rr} RR`}
        disabled={!onopen}
        onclick={() => onopen?.(c.id)}
      >
        <span class="res" style:background={c.bg} style:color={c.fg}>{c.l}</span>
        <span class="row"><span class="map" data-ellipsis>{c.map}</span><span class="rr" style:color={c.fg}>{c.rr}</span></span>
      </button>
    {/each}
  </div>
</section>

<style>
  .strip {
    flex: none;
    background: var(--card);
    border: 1px solid var(--line);
    border-radius: 12px;
    padding: 14px 18px;
    display: flex;
    align-items: center;
    gap: 24px;
    min-width: 0;
  }

  .lead {
    display: flex;
    flex-direction: column;
    gap: 8px;
    flex: none;
  }

  .micro {
    font: 500 10.5px/1 var(--font-mono);
    letter-spacing: 0.06em;
    color: var(--text-muted);
  }

  .lab {
    font: 500 12.5px/1 var(--font);
    color: var(--text-muted);
    white-space: nowrap;
  }

  .blk {
    display: flex;
    flex-direction: column;
    gap: 7px;
    padding-left: 22px;
    border-left: 1px solid var(--line);
    flex: none;
  }

  .big {
    font: 700 26px/1 var(--font);
    font-variant-numeric: tabular-nums;
  }

  .rec {
    font: 700 22px/1 var(--font);
    font-variant-numeric: tabular-nums;
  }

  .start {
    font: 600 18px/1 var(--font);
    white-space: nowrap;
    cursor: default;
  }

  .dur {
    font-size: 13px;
    font-weight: 500;
    color: var(--text-muted);
  }

  .chips {
    flex: 1;
    min-width: 0;
    display: flex;
    gap: 6px;
    padding-left: 22px;
    border-left: 1px solid var(--line);
    align-self: stretch;
    align-items: center;
  }

  .chip {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
    align-items: stretch;
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
    border-radius: 4px;
  }

  .chip:disabled {
    cursor: default;
  }

  .res {
    height: 24px;
    border-radius: 4px;
    display: flex;
    align-items: center;
    justify-content: center;
    font: 700 11px/1 var(--font-mono);
  }

  .chip:not(:disabled):hover .res {
    filter: brightness(1.25);
  }

  .row {
    display: flex;
    justify-content: space-between;
    gap: 4px;
    font: 500 12px/1 var(--font);
  }

  .map {
    color: var(--text-2);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }

  .rr {
    font-weight: 700;
    flex: none;
    font-variant-numeric: tabular-nums;
  }
</style>
