<!--
  The drawer's rank card (prototype 988–1010): current rank (only the exact RR shows; the next-tier figure is not
  shown here at all, per the owner screenshot), peak (act + time since that act ended; tooltip: the act's name and
  dates), last act (its `Final rank` subtitle is removed by the same screenshot); then the stats row, each stat's
  scope in its tooltip. The Record cell draws the typed W/L counts (`41W – 24L`, win/loss tokens, neutral zeros); six
  stats wrap to a 3×2 grid below ~720px so a `999W – 999L` record never overflows.
-->
<script lang="ts">
  import { assetUrl } from "../../lib/assets";
  import { DASH } from "../../lib/format";
  import { retryImg } from "../../lib/imgRetry";
  import { isRankedTier } from "../../lib/meta";
  import { peakCell, prevCell, rankCell, type StatCell } from "../../lib/profile";
  import { meta } from "../../lib/stores";
  import { peakTip } from "../../lib/tips";
  import { tip } from "../../lib/tooltip.svelte";
  import type { PlayerCard } from "../../lib/types";
  import Skel from "../Skel.svelte";
  import RecordValue from "./RecordValue.svelte";

  let { card, stats, note = "" }: { card: PlayerCard | null; stats: StatCell[] | null; note?: string } = $props();

  const cur = $derived(card ? rankCell(card.rank, $meta) : undefined);
  const peak = $derived(card ? peakCell(card.peak, $meta) : undefined);
  const prev = $derived(card ? prevCell(card.prevActTier, $meta) : undefined);
  /**
   * The current rank's exact RR alone. This drawer never shows the RR to the next tier (the approved prototype drops
   * it), so the RR is composed from the card here rather than taken from `rankCell`, whose `sub` also carries the
   * next-tier figure for the screens that still want it. Unknown rank → nothing; unranked → `No ranked games`;
   * unknown RR with a known tier → `— RR`.
   */
  const curRr = $derived.by(() => {
    const r = card?.rank;
    if (r === undefined || r === null) return "";
    if (!isRankedTier(r.tier)) return "No ranked games";
    return r.rr === null ? `${DASH} RR` : `${r.rr} RR`;
  });
  /** Loading: the five stats every player has (the user's Net RR joins once the card says who this is). */
  const cells: StatCell[] = $derived(stats ?? ["K/D", "Headshot %", "Win rate", "Record", "Avg ACS"].map((label) => ({ label, value: undefined, color: "", tip: "" })));
</script>

<div class="rc">
  <div class="ranks">
    <div class="cell main">
      {#if cur === undefined}
        <Skel w={32} h={32} r="50%" block />
        <div class="col"><span class="lbl">CURRENT RANK</span><Skel w={120} h={18} block /><Skel w={110} h={11} block /></div>
      {:else}
        {#if cur.icon !== null}<img {@attach retryImg} class="i46" src={assetUrl("tier", cur.icon)} alt="" />{:else}<span class="i46"></span>{/if}
        <div class="col">
          <span class="lbl">CURRENT RANK</span>
          <span class="nm big" style:color={cur.color}>{cur.name}</span>
          {#if curRr}<span class="sub" data-ellipsis>{curRr}</span>{/if}
        </div>
      {/if}
    </div>
    <div class="cell" use:tip={card?.peak ? peakTip(card.peak) : ""}>
      {#if peak === undefined}
        <Skel w={32} h={32} r="50%" block />
        <div class="col"><span class="lbl">PEAK</span><Skel w={100} h={15} block /><Skel w={90} h={11} block /></div>
      {:else}
        {#if peak.icon !== null}<img {@attach retryImg} class="i32" src={assetUrl("tier", peak.icon)} alt="" />{:else}<span class="i32"></span>{/if}
        <div class="col">
          <span class="lbl">PEAK</span>
          <span class="nm" style:color={peak.color}>{peak.name}</span>
          {#if peak.sub}<span class="sub" data-ellipsis>{peak.sub}</span>{/if}
        </div>
      {/if}
    </div>
    <div class="cell">
      {#if prev === undefined}
        <Skel w={32} h={32} r="50%" block />
        <div class="col"><span class="lbl">LAST ACT</span><Skel w={100} h={15} block /><Skel w={70} h={11} block /></div>
      {:else}
        {#if prev.icon !== null}<img {@attach retryImg} class="i32 dim" src={assetUrl("tier", prev.icon)} alt="" />{:else}<span class="i32"></span>{/if}
        <div class="col">
          <span class="lbl">LAST ACT</span>
          <span class="nm" style:color={prev.color}>{prev.name}</span>
          {#if prev.sub && prev.sub !== "Final rank"}<span class="sub" data-ellipsis>{prev.sub}</span>{/if}
        </div>
      {/if}
    </div>
  </div>
  <div class="stats" class:has-six={cells.length === 6}>
    {#each cells as s, i (s.label)}
      <div class={["stat", i > 0 && "sep"]} use:tip={s.tip}>
        <span class="sl">{s.label}</span>
        {#if s.value === undefined}<Skel w={52} h={22} block />{:else if "record" in s}<RecordValue record={s.record ?? null} />{:else}<span
            class="sv"
            style:color={s.color}>{s.value}</span
          >{/if}
      </div>
    {/each}
  </div>
  {#if note.trim()}
    <div class="overview-note"><span class="lbl">NOTE</span><p>{note}</p></div>
  {/if}
</div>

<style>
  .rc {
    flex: none;
    border: 1px solid var(--line-card);
    border-radius: 10px;
    overflow: hidden;
    background: var(--card);
  }

  .ranks {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
  }

  .cell {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 10px;
    min-width: 0;
    cursor: default;
  }

  .cell.main {
    gap: 8px;
    border-left: none;
  }

  .i46 {
    width: 32px;
    height: 32px;
    display: block;
    flex: none;
  }

  .i32 {
    width: 32px;
    height: 32px;
    display: block;
    flex: none;
  }

  .dim {
    opacity: 0.85;
  }

  .col {
    display: flex;
    flex-direction: column;
    gap: 5px;
    min-width: 0;
  }

  .lbl {
    font: 500 10.5px/1 var(--font-mono);
    letter-spacing: 0.06em;
    color: var(--text-label);
  }

  .nm {
    font: 700 17px/1 var(--font);
    white-space: nowrap;
  }

  .nm.big {
    font-size: 17px;
  }

  .sub {
    font: 500 12.5px/1 var(--font);
    color: var(--text-3);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .stats {
    border-top: 1px solid var(--line-card);
    padding: 10px 12px 12px;
    display: flex;
  }

  .stat {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 5px;
    cursor: default;
  }

  .stat.sep {
    padding-left: 10px;
  }

  .sl {
    font: 500 12.5px/1 var(--font);
    color: var(--text-muted);
    white-space: nowrap;
  }

  .sv {
    font: 600 22px/1 var(--font);
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }

  .overview-note {
    display: grid;
    grid-template-columns: 36px minmax(0, 1fr);
    gap: 8px;
    padding: 10px 12px;
    border-top: 1px solid var(--line-card);
    background: var(--surface);
  }

  .overview-note p {
    margin: 0;
    font: 500 13px/1.45 var(--font);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    color: var(--text-2);
  }

  /* Six stats only fit one row from ~720px up; below that they wrap to a 3×2 grid (approved prototype). */
  @container profile (width < 720px) {
    .has-six { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 12px; }
    .has-six .stat:nth-child(3n + 1) { padding-left: 0; }
  }

  @container profile (width < 620px) {
    .nm { white-space: normal; line-height: 1.1; }
    .cell { align-items: flex-start; }
  }
</style>
