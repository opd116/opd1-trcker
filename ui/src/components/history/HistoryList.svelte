<!--
  Match History list (design v2 523–588, logic 1741–1769): the title and the Queue tablist (Competitive / All modes,
  counts from `match_history`), then one scrolling card: a sticky column header, day groups (label, W / L chips, the
  day's result strip oldest → newest, the RR pill summed over its competitive rows only) and 56px match rows. A row
  opens its scoreboard from the user's side. Pages of 50 load as the list nears its end. Non-competitive rows have no
  rank and no RR (`—`). Empty states: nothing stored at all, or nothing in the chosen queue.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { matchHistory } from "../../lib/api";
  import { assetUrl } from "../../lib/assets";
  import { kd2 } from "../../lib/format";
  import {
    endMs,
    footText,
    groupDays,
    hsCell,
    isComp,
    kdaText,
    kdTone,
    RESULT_LETTER,
    rowAria,
    rrCell,
    stamp,
    whenText,
  } from "../../lib/historyv2";
  import { retryImg } from "../../lib/imgRetry";
  import { agentName, isRankedTier, mapName } from "../../lib/meta";
  import { openMatch, scrollHost } from "../../lib/router.svelte";
  import { live, meta, mystats, now } from "../../lib/stores";
  import { tierName } from "../../lib/theme";
  import { tip } from "../../lib/tooltip.svelte";
  import type { HistoryPage, HistoryQueue, HistoryRow } from "../../lib/types";
  import { reportError } from "../../lib/uilog";
  import Skel from "../Skel.svelte";
  import { historyCache } from "./cache";

  const PAGE = 50;
  const TABS: { q: HistoryQueue; label: string }[] = [
    { q: "competitive", label: "Competitive" },
    { q: "all", label: "All modes" },
  ];

  let queue = $state<HistoryQueue>(historyCache.queue);
  let data = $state<HistoryPage | null>(historyCache.pages[historyCache.queue] ?? null);
  let busy = false;
  /** Bumped by every queue switch and refresh: an answer for an older request is dropped. */
  let gen = 0;

  function put(q: HistoryQueue, p: HistoryPage) {
    historyCache.pages[q] = p;
    if (q === queue) data = p;
  }

  /** Fetch the first `n` rows of `q` again (at least one page), replacing what is shown. */
  async function refresh(q: HistoryQueue, n = PAGE) {
    const g = ++gen;
    busy = true;
    try {
      const p = await matchHistory(q, 0, Math.max(PAGE, n));
      if (g === gen) put(q, p);
    } catch (err) {
      reportError("match_history", err);
    } finally {
      if (g === gen) busy = false;
    }
  }

  async function more() {
    const d = data;
    if (busy || !d || d.rows.length >= d.total) return;
    const q = queue;
    const g = gen;
    busy = true;
    try {
      const p = await matchHistory(q, d.rows.length, PAGE);
      if (g === gen && q === queue) put(q, { ...p, rows: [...d.rows, ...p.rows.filter((r) => !d.rows.some((x) => x.matchId === r.matchId))] });
    } catch (err) {
      reportError("match_history", err);
    } finally {
      if (g === gen) busy = false;
    }
  }

  function pick(q: HistoryQueue) {
    if (q === queue) return;
    queue = q;
    historyCache.queue = q;
    data = historyCache.pages[q] ?? null;
    if (scroller) scroller.scrollTop = 0;
    void refresh(q, data?.rows.length ?? PAGE);
  }

  // Mount: draw what the cache has, then ask again (the user may have finished a match since).
  onMount(() => {
    void refresh(queue, data?.rows.length ?? PAGE);
  });

  // A match that just ended lands in the list: ask again whenever the live stage changes.
  let lastStage: string | null | undefined;
  $effect(() => {
    const st = $live?.stage ?? null;
    if (lastStage !== undefined && st !== lastStage) void refresh(queue, data?.rows.length ?? PAGE);
    lastStage = st;
  });

  let scroller = $state<HTMLElement | null>(null);
  let sentinel = $state<HTMLElement | null>(null);
  // Load the next page when the end of the list comes within 400px.
  $effect(() => {
    if (!scroller || !sentinel || typeof IntersectionObserver === "undefined") return;
    const io = new IntersectionObserver((es) => es.some((e) => e.isIntersecting) && void more(), { root: scroller, rootMargin: "400px" });
    io.observe(sentinel);
    return () => io.disconnect();
  });

  const days = $derived(data ? groupDays(data.rows, $now) : []);
  const allTotal = $derived(data?.allTotal ?? null);
  const count = (q: HistoryQueue) => (data ? String(q === "competitive" ? data.competitiveTotal : data.allTotal) : "");

  /** The user's own puuid: the scoreboard takes their side. */
  const selfPuuid = $derived($mystats?.puuid ?? $live?.players.find((p) => p.isSelf)?.puuid ?? "");

  function open(r: HistoryRow) {
    openMatch(r.matchId, { from: "history", focusPuuid: selfPuuid });
  }

  function onKey(e: KeyboardEvent, r: HistoryRow) {
    if (e.target !== e.currentTarget) return;
    if (e.key !== "Enter" && e.key !== " ") return;
    e.preventDefault();
    open(r);
  }

  const RES_COLOR = { win: "var(--win)", loss: "var(--loss)", draw: "var(--text-dim)" } as const;
  const RES_BG = { win: "rgba(95,207,143,.16)", loss: "rgba(239,106,91,.16)", draw: "rgba(138,143,151,.16)" } as const;
  const KD_COLOR = { hi: "var(--text-hi)", lo: "var(--text-lo)", mid: "var(--text-mid)", none: "var(--text-label)" } as const;
  const RR_COLOR = { win: "var(--win)", loss: "var(--loss)", flat: "var(--text-label)" } as const;
</script>

<div class="page">
  <div class="head">
    <h1 class="title">Match history</h1>
    <div class="grow"></div>
    <div class="tabs" role="tablist" aria-label="Queue">
      {#each TABS as t (t.q)}
        <button type="button" role="tab" class={["tab", queue === t.q && "on"]} aria-selected={queue === t.q} onclick={() => pick(t.q)}
          >{t.label}<span class="cnt">{count(t.q)}</span></button
        >
      {/each}
    </div>
  </div>

  {#if data && data.rows.length > 0}
    <div class="card" bind:this={scroller} use:scrollHost>
      <div class="cols">
        <span>RESULT</span><span>MAP</span><span>AGENT</span><span>K / D / A</span><span class="r">K/D</span><span class="r">ACS</span><span class="r"
          >HS%</span
        ><span class="r">RR</span><span></span>
      </div>
      {#each days as d (d.label)}
        <div class="day">
          <span class="dlabel">{d.label}</span>
          <span class="chip w">{d.wins}W</span>
          <span class="chip l">{d.losses}L</span>
          <div class="strip" aria-hidden="true">
            {#each d.strip as s, i (i)}<span style:background={RES_COLOR[s]}></span>{/each}
          </div>
          <div class="grow"></div>
          {#if d.rr !== null}
            <span class={["rrpill", d.rr >= 0 ? "up" : "down"]}><span class="arrow">{d.rr >= 0 ? "▲" : "▼"}</span>{Math.abs(d.rr)} RR</span>
          {/if}
        </div>
        {#each d.rows as r (r.matchId)}
          {@const map = mapName($meta, r.mapId)}
          {@const hs = hsCell(r)}
          {@const rr = rrCell(r)}
          {@const tone = kdTone(r.kd)}
          <div class="row" data-row="1" tabindex="0" role="button" aria-label={rowAria(r, map ?? null)} onclick={() => open(r)} onkeydown={(e) => onKey(e, r)}>
            <div class="bar" style:background={RES_COLOR[r.result]}></div>
            <div class="res">
              <span class="badge" style:background={RES_BG[r.result]} style:color={RES_COLOR[r.result]}>{RESULT_LETTER[r.result]}</span>
              <span class="score">{r.roundsWon !== null && r.roundsLost !== null ? `${r.roundsWon}–${r.roundsLost}` : "—"}</span>
            </div>
            <div class="map">
              {#if r.mapId}<img {@attach retryImg} class="mimg" src={assetUrl("mapList", r.mapId)} alt="" />{:else}<div class="mimg"></div>{/if}
              <div class="mtext">
                <span class="mname">{map ?? "—"}</span>
                <span class="when" use:tip={stamp(endMs(r), $now).exact}>{whenText(r, $now)}</span>
              </div>
            </div>
            {#if r.agentId}
              {@const an = agentName($meta, r.agentId) ?? ""}
              <img {@attach retryImg} class="agent" src={assetUrl("agent", r.agentId)} alt={an} use:tip={an} />
            {:else}
              <div class="agent"></div>
            {/if}
            <span class="kda">{kdaText(r.kills, r.deaths, r.assists)}</span>
            <span class="num kd" style:color={KD_COLOR[tone]}>{kd2(r.kd)}</span>
            <span class="num">{r.acs === null ? "—" : Math.round(r.acs)}</span>
            <span class="num" style:color={r.hsPct === null ? "var(--text-label)" : "var(--text)"} use:tip={hs.tip}>{hs.text}</span>
            <div class="rank">
              {#if isComp(r) && isRankedTier(r.tierAfter)}
                <img
                  {@attach retryImg}
                  class="ricon"
                  src={assetUrl("tier", r.tierAfter)}
                  alt=""
                  use:tip={`Rank after this match: ${tierName(r.tierAfter, $meta)}`}
                />
              {/if}
              <span class="rr" style:color={RR_COLOR[rr.tone]}>{rr.text}</span>
            </div>
            <svg width="7" height="12" viewBox="0 0 7 12" fill="none" stroke="var(--text-ghost)" stroke-width="1.5" stroke-linecap="round" aria-hidden="true"
              ><path d="M1.5 1.5L5.5 6l-4 4.5"></path></svg
            >
          </div>
        {/each}
      {/each}
      <div class="foot" bind:this={sentinel}>{footText(data.rows.length, data.total)}</div>
    </div>
  {:else if data && allTotal === 0}
    <div class="empty">
      <div class="ebox">
        <div class="eicon">
          <svg width="24" height="24" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" aria-hidden="true"
            ><path d="M2.5 10a7.5 7.5 0 1 0 15 0a7.5 7.5 0 1 0-15 0M10 6v4.5l3 2"></path></svg
          >
        </div>
        <span class="etitle">No matches stored yet</span>
        <span class="etext">Matches are saved when they end while OPD1 is running. Finish one and the full scoreboard shows up here.</span>
        <span class="emono">OLDER MATCHES CAN’T BE FETCHED FROM RIOT</span>
      </div>
    </div>
  {:else if data}
    <div class="empty">
      <div class="ebox q">
        <span class="etitle small">No {queue === "competitive" ? "competitive matches" : "matches"} stored</span>
        <span class="etext soft">OPD1 stores every match that ends while it’s running. Finish {queue === "competitive" ? "a competitive match" : "one"} and it appears here.</span>
      </div>
    </div>
  {:else}
    <!-- First load: the card's shape, no text. -->
    <div class="card" aria-busy="true">
      <div class="cols"></div>
      <div class="day"><Skel w={90} h={14} /></div>
      {#each [0, 1, 2, 3, 4, 5] as i (i)}
        <div class="row skel" aria-hidden="true">
          <span></span>
          <Skel w={70} h={18} block />
          <div class="map"><Skel w={84} h={36} r={5} block /><Skel w={80} h={13} block /></div>
          <Skel w={34} h={34} r={6} block />
          <Skel w={80} h={13} block />
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .page {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    padding: 18px 24px 18px;
    gap: 14px;
  }

  .head {
    display: flex;
    align-items: flex-end;
    gap: 16px;
    flex: none;
  }

  .title {
    margin: 0;
    font: 700 32px/1 var(--font-display);
    letter-spacing: 0.03em;
    text-transform: uppercase;
  }

  .grow {
    flex: 1;
  }

  .tabs {
    display: flex;
    padding: 3px;
    border-radius: 8px;
    background: var(--surface);
    border: 1px solid var(--track);
  }

  .tab {
    height: 28px;
    padding: 0 12px;
    border-radius: 6px;
    display: flex;
    align-items: center;
    gap: 7px;
    font: 600 13px/1 var(--font);
    color: var(--text-muted);
    background: transparent;
    border: none;
    cursor: pointer;
  }

  .tab:hover {
    color: var(--text);
  }

  .tab.on {
    color: var(--text);
    background: var(--button-hover);
  }

  .cnt {
    font: 600 12px/1 var(--font);
    color: var(--text-tag);
  }

  .tab.on .cnt {
    color: #c9c2ff;
  }

  .card {
    flex: 1;
    min-height: 0;
    overflow: auto;
    display: flex;
    flex-direction: column;
    background: var(--card);
    border: 1px solid var(--line);
    border-radius: 10px;
  }

  .cols,
  .row {
    display: grid;
    grid-template-columns: 112px minmax(0, 1fr) 34px 112px 48px 48px 48px 84px 10px;
    column-gap: 16px;
    align-items: center;
    padding: 0 14px 0 17px;
  }

  .cols {
    position: sticky;
    top: 0;
    z-index: 2;
    flex: none;
    height: 36px;
    background: var(--card-head);
    border-bottom: 1px solid var(--line-media);
    font: 500 10.5px/1 var(--font-mono);
    letter-spacing: 0.06em;
    color: var(--text-muted);
  }

  .r {
    text-align: right;
  }

  .day {
    flex: none;
    display: flex;
    align-items: center;
    gap: 10px;
    height: 42px;
    padding: 0 40px 0 17px;
    background: var(--sunken);
    border-bottom: 1px solid var(--line);
  }

  .dlabel {
    font: 700 15px/1 var(--font-display);
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: var(--text);
    margin-right: 6px;
  }

  .chip {
    display: inline-flex;
    align-items: center;
    height: 22px;
    padding: 0 8px;
    border-radius: 5px;
    font: 700 12.5px/1 var(--font);
  }

  .chip.w {
    background: rgba(95, 207, 143, 0.14);
    color: var(--win-text);
  }

  .chip.l {
    background: rgba(239, 106, 91, 0.14);
    color: var(--loss-text);
  }

  .strip {
    display: flex;
    gap: 2px;
    margin-left: 4px;
  }

  .strip span {
    width: 4px;
    height: 14px;
    border-radius: 1px;
  }

  .rrpill {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 26px;
    padding: 0 10px;
    border-radius: 13px;
    font: 700 14px/1 var(--font);
    white-space: nowrap;
  }

  .rrpill.up {
    background: rgba(95, 207, 143, 0.1);
    border: 1px solid rgba(95, 207, 143, 0.35);
    color: var(--win-text);
  }

  .rrpill.down {
    background: rgba(239, 106, 91, 0.1);
    border: 1px solid rgba(239, 106, 91, 0.35);
    color: var(--loss-text);
  }

  .arrow {
    font-size: 10px;
  }

  .row {
    position: relative;
    flex: none;
    height: 56px;
    border-bottom: 1px solid var(--line-soft);
    cursor: pointer;
  }

  .row:hover {
    background: var(--row-hover);
  }

  .row.skel {
    cursor: default;
  }

  .row.skel:hover {
    background: none;
  }

  .bar {
    position: absolute;
    left: 0;
    top: 0;
    bottom: 0;
    width: 3px;
  }

  .res {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .badge {
    width: 24px;
    height: 24px;
    flex: none;
    border-radius: 5px;
    display: flex;
    align-items: center;
    justify-content: center;
    font: 700 11.5px/1 var(--font-mono);
  }

  .score {
    font: 700 19px/1 var(--font);
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }

  .map {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
  }

  .mimg {
    width: 84px;
    height: 36px;
    object-fit: cover;
    border-radius: 5px;
    display: block;
    flex: none;
    background: var(--skel);
  }

  .mtext {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
  }

  .mname {
    font: 600 15px/1 var(--font);
  }

  .when {
    font: 500 12.5px/1 var(--font);
    color: var(--text-label);
    white-space: nowrap;
    align-self: flex-start;
  }

  .agent {
    width: 34px;
    height: 34px;
    border-radius: 6px;
    display: block;
    background: var(--skel);
  }

  .kda {
    font: 500 15px/1 var(--font);
    color: var(--text-soft);
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }

  .num {
    font: 600 15px/1 var(--font);
    text-align: right;
    font-variant-numeric: tabular-nums;
  }

  .rank {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 7px;
  }

  .ricon {
    width: 20px;
    height: 20px;
    display: block;
  }

  .rr {
    font: 700 17px/1 var(--font);
    min-width: 32px;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }

  .foot {
    flex: none;
    padding: 14px 17px 16px;
    font: 500 12.5px/1.45 var(--font);
    color: var(--text-label);
  }

  .empty {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    border: 1px dashed var(--line-control);
    border-radius: 12px;
  }

  .ebox {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 16px;
    max-width: 420px;
    text-align: center;
  }

  .ebox.q {
    gap: 12px;
  }

  .eicon {
    width: 56px;
    height: 56px;
    border-radius: 14px;
    border: 1px solid var(--line-control);
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-muted);
  }

  .etitle {
    font: 700 24px/1 var(--font-display);
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }

  .etitle.small {
    font-size: 22px;
  }

  .etext {
    font: 500 14px/1.55 var(--font);
    color: var(--text-dim);
    text-wrap: pretty;
  }

  .etext.soft {
    color: var(--text-muted);
  }

  .emono {
    font: 500 10.5px/1 var(--font-mono);
    letter-spacing: 0.08em;
    color: var(--text-label);
  }
</style>
