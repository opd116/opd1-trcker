<!--
  VALORANT closed or in its menus (design v2 TrackerApp.dc.html 266–350): the radar with its copy and the three
  checks, "Launch VALORANT" (closed) or the searching pill (menus, while matchmaking), and the right column: your
  party (menus), the last match, today's session and your rank. A card whose data is unknown is left out.
-->
<script lang="ts">
  import { launchValorant } from "../../lib/api";
  import { appStatus } from "../../lib/appdata.svelte";
  import { assetUrl } from "../../lib/assets";
  import { sessionOpen } from "../../lib/compv2";
  import { queueLabel } from "../../lib/historyv2";
  import { pct0, rrDelta } from "../../lib/format";
  import { retryImg } from "../../lib/imgRetry";
  import {
    agoCaps,
    clock,
    durationCaps,
    kdaText,
    nameView,
    rankRrText,
    resultColor,
    resultScore,
    rrColor,
    selfFirst,
    SESSION_CHIPS,
    sessionChip,
    sessionSpanMs,
    waitChecks,
  } from "../../lib/livev2";
  import { mapName } from "../../lib/meta";
  import { openMatch } from "../../lib/router.svelte";
  import { meta, mystats } from "../../lib/stores";
  import { tierColor, tierName, tierShort } from "../../lib/theme";
  import { tip } from "../../lib/tooltip.svelte";
  import type { LiveState } from "../../lib/types";
  import { cachedRow } from "../history/cache";
  import Dot from "../shell/Dot.svelte";
  import Radar from "./Radar.svelte";
  import Shimmer from "./Shimmer.svelte";

  let { live }: { live: LiveState | null } = $props();

  const menus = $derived(live?.stage === "menus");
  const checks = $derived(waitChecks($appStatus, live?.stage ?? null));
  const queue = $derived(menus ? ($appStatus?.queue ?? null) : null);
  const party = $derived(menus && live ? selfFirst(live.players).slice(0, 5) : []);
  const ms = $derived($mystats ?? null);
  const last = $derived(ms?.last5[0] ?? null);
  const rank = $derived(ms && typeof ms.rank.tier === "number" ? ms.rank : null);

  // A clock for "45 MIN AGO" and the queue timer: every second while searching, else every 30 s. The session length
  // is the session's own span (`sessionSpanMs`), so it does not tick.
  let nowMs = $state(Date.now());
  $effect(() => {
    nowMs = Date.now();
    const id = setInterval(() => (nowMs = Date.now()), queue ? 1000 : 30_000);
    return () => clearInterval(id);
  });
  // TODAY is the session still going: one that ended 6 h ago is no longer today's (the snapshot can be hours old).
  const session = $derived(ms?.session && sessionOpen(ms.session, nowMs) ? ms.session : null);

  let launching = $state(false);
  let launchError = $state<string | null>(null);
  async function launch() {
    launching = true;
    launchError = null;
    try {
      await launchValorant();
    } catch (err) {
      launchError = err instanceof Error ? err.message : String(err);
    } finally {
      launching = false;
    }
  }

  function openLast() {
    if (!ms || !last) return;
    openMatch(last.matchId, { from: "lastMatch", focusPuuid: ms.puuid });
  }

  function lastKey(e: KeyboardEvent) {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      openLast();
    }
  }
</script>

<div class="wait">
  <div class="main">
    <Radar />
    <div class="copy">
      {#if menus}
        <h1>In the lobby</h1>
        <p>Queue up. This page switches to the live match as soon as agent select starts.</p>
      {:else}
        <h1>Waiting for VALORANT</h1>
        <p>Start the game and keep this window open. Players load on their own when you reach agent select.</p>
      {/if}
    </div>
    <div class="checks">
      {#each checks as c, i (c.label)}
        <div class="check" class:bb={i < checks.length - 1}>
          <span class="cdot" style:background={c.ok ? "var(--win)" : "var(--text-disabled)"}></span>
          <span class="clabel">{c.label}</span>
          <div class="fill"></div>
          <span class="cval" style:color={c.ok ? "var(--text-muted)" : "var(--text-3)"}>{c.value}</span>
        </div>
      {/each}
    </div>
    {#if !menus}
      <button type="button" class="primary" onclick={launch} disabled={launching}>Launch VALORANT</button>
      {#if launchError}<span class="err" role="alert">Couldn’t start VALORANT: {launchError}</span>{/if}
    {:else if queue}
      <div class="search" role="timer">
        <Dot color="var(--win)" pulse />
        <span class="s-label">Searching · {queueLabel(queue.queueId)}</span>
        <span class="s-time">{clock(nowMs - queue.searchingSinceMs)}</span>
      </div>
    {/if}
  </div>

  <div class="side">
    {#if menus && party.length > 0}
      <div class="card pad">
        <div class="cap"><span class="mono">YOUR PARTY</span><span class="mono n">{party.length} / 5</span></div>
        <div class="party" class:dense={party.length >= 4}>
          {#each party as p (p.puuid)}
            {@const nm = nameView(p.name, $meta)}
            <div class="member">
              <img {@attach retryImg} class="art" src={assetUrl("cardSmall", p.cardId)} alt="" />
              <div class="m-col">
                {#if nm.style === "loading"}<Shimmer w={90} h={13} />{:else}<span class="m-name"
                    ><span class={["m-n", nm.style]} data-ellipsis>{nm.text}</span>
                    {#if nm.tag}<span class="m-tag">{nm.tag}</span>{/if}</span
                  >{/if}
                <span class="mono">LV {p.level ?? "—"}{p.isSelf ? " · YOU" : ""}</span>
              </div>
              {#if p.rank === undefined}
                <Shimmer w={24} h={24} r={12} /><span class="m-rank"><Shimmer w={34} h={13} /></span>
              {:else if p.rank === null}
                <span class="m-icon"></span><span class="m-rank dim">—</span>
              {:else}
                <img {@attach retryImg} class="m-icon" src={assetUrl("tier", p.rank.tier >= 3 ? p.rank.tier : 0)} alt="" />
                <span class="m-rank" style:color={tierColor(p.rank.tier)}>{tierShort(p.rank.tier, $meta)}</span>
              {/if}
            </div>
          {/each}
        </div>
      </div>
    {/if}

    {#if ms && last}
      <div class="card">
        <div class="lm-hero">
          <img {@attach retryImg} src={assetUrl("mapSplash", last.mapId)} alt="" />
          <div class="lm-fade"></div>
          <div class="lm-cap mono">LAST MATCH · {agoCaps(nowMs - last.startMs)}</div>
          <div class="lm-title">
            <span class="lm-map">{mapName($meta, last.mapId) ?? "—"}</span>
            <span class="lm-res" style:color={resultColor(last.result)}>{resultScore(last)}</span>
          </div>
        </div>
        <div class="lm-stats">
          <div class="st"><span class="mono">K/D/A</span><span class="sv">{kdaText(last.kills, last.deaths, last.assists)}</span></div>
          <div class="st"><span class="mono">ACS</span><span class="sv">{last.acs === null ? "—" : Math.round(last.acs)}</span></div>
          <div class="st"><span class="mono">HS%</span><span class="sv">{pct0(last.hsPct)}</span></div>
          <div class="st"><span class="mono">RR</span><span class="sv" style:color={rrColor(last.rrEarned)}>{rrDelta(last.rrEarned)}</span></div>
        </div>
        <!-- data-match-id: Back from the scoreboard puts a keyboard user's focus here again (router `backFromMatch`). -->
        <div class="lm-open" role="button" tabindex="0" data-match-id={last.matchId} onclick={openLast} onkeydown={lastKey}>
          View scoreboard<svg width="7" height="12" viewBox="0 0 7 12" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" aria-hidden="true"><path d="M1.5 1.5L5.5 6l-4 4.5"></path></svg>
        </div>
      </div>
    {/if}

    {#if session && session.matches.length > 0}
      <div class="card pad col">
        <div class="cap"><span class="mono">TODAY</span><span class="mono n">{session.matches.length} {session.matches.length === 1 ? "MATCH" : "MATCHES"} · {durationCaps(sessionSpanMs(session, (id) => cachedRow(id)?.durationMs))}</span></div>
        <div class="today">
          <span class="net" style:color={rrColor(session.netRr)}>{rrDelta(session.netRr)} RR</span>
          <span class="wl"><span class="w">{session.wins}</span> – <span class="l">{session.losses}</span></span>
        </div>
        <div class="chips">
          {#each session.matches.slice(-SESSION_CHIPS) as m (m.matchId)}
            {@const c = sessionChip(m, $meta)}
            <div class="chip" class:draw={c.letter === "D"} use:tip={c.tip} style:background={c.bg} style:color={c.fg}>{c.letter}</div>
          {/each}
        </div>
      </div>
    {/if}

    {#if rank}
      <div class="card pad rank">
        <img {@attach retryImg} class="rk-icon" src={assetUrl("tier", rank.tier! >= 3 ? rank.tier! : 0)} alt="" />
        <div class="rk-col">
          <div class="rk-top">
            <span class="rk-name" style:color={tierColor(rank.tier)}>{tierName(rank.tier, $meta)}</span>
            <span class="mono dim">{rankRrText(rank.tier, rank.rr)}</span>
          </div>
          <div class="rk-track"><div style:width="{Math.min(100, Math.max(0, rank.rr ?? 0))}%" style:background={tierColor(rank.tier)}></div></div>
        </div>
      </div>
    {/if}
  </div>
</div>

<style>
  .wait {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: minmax(0, 1fr) 340px;
    gap: 12px;
  }

  .main {
    background: var(--card);
    border: 1px solid var(--line);
    border-radius: 12px;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 26px;
    padding: 40px;
    min-height: 0;
  }

  .copy {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
  }

  h1 {
    margin: 0;
    font: 700 36px/1 var(--font-display);
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }

  p {
    margin: 0;
    font: 500 15px/1.5 var(--font);
    color: var(--text-muted);
    text-align: center;
    max-width: 440px;
    text-wrap: pretty;
  }

  .checks {
    width: 100%;
    max-width: 440px;
    display: flex;
    flex-direction: column;
    border: 1px solid var(--line);
    border-radius: 9px;
    overflow: hidden;
  }

  .check {
    display: flex;
    align-items: center;
    gap: 12px;
    height: 42px;
    padding: 0 14px;
    background: var(--drawer);
  }

  .check.bb {
    border-bottom: 1px solid var(--line);
  }

  .cdot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex: none;
  }

  .clabel {
    font: 600 13px/1 var(--font);
    color: var(--text);
  }

  .fill {
    flex: 1;
  }

  .cval {
    font: 500 10.5px/1 var(--font-mono);
    letter-spacing: 0.06em;
  }

  .primary {
    height: 36px;
    padding: 0 18px;
    border-radius: 8px;
    background: var(--accent);
    color: var(--on-accent);
    border: none;
    font: 600 13.5px/1 var(--font);
    cursor: pointer;
  }

  .primary:hover {
    background: var(--accent-hover);
  }

  .primary:disabled {
    opacity: 0.6;
    cursor: default;
  }

  .err {
    margin-top: -14px;
    font: 500 13px/1.4 var(--font);
    color: var(--error-text);
    text-align: center;
    max-width: 440px;
  }

  .search {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 36px;
    padding: 0 16px;
    border-radius: 18px;
    background: rgba(157, 140, 255, 0.08);
    border: 1px solid rgba(157, 140, 255, 0.3);
  }

  .s-label {
    font: 600 13px/1 var(--font);
    color: var(--accent-pale);
  }

  .s-time {
    font: 600 12.5px/1 var(--font);
    color: var(--text);
    font-variant-numeric: tabular-nums;
  }

  .side {
    display: flex;
    flex-direction: column;
    gap: 12px;
    min-height: 0;
    overflow: auto;
  }

  .card {
    flex: none;
    background: var(--card);
    border: 1px solid var(--line);
    border-radius: 12px;
    overflow: hidden;
  }

  .pad {
    padding: 16px;
  }

  .col {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .mono {
    font: 500 10.5px/1 var(--font-mono);
    letter-spacing: 0.06em;
    color: var(--text-muted);
  }

  .mono.n {
    letter-spacing: 0.08em;
    color: var(--text-label);
  }

  .mono.dim {
    letter-spacing: 0;
    color: var(--text-dim);
  }

  .cap {
    display: flex;
    justify-content: space-between;
  }

  .pad > .cap {
    margin-bottom: 12px;
  }

  .col > .cap {
    margin-bottom: 0;
  }

  .party {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  /* [C] 4–5 members: the design shows 2; a tighter stack keeps the right column inside 760px. */
  .party.dense {
    gap: 4px;
  }

  .member {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 40px;
  }

  .art {
    width: 36px;
    height: 36px;
    border-radius: 6px;
    object-fit: cover;
    display: block;
    background: var(--skel);
    flex: none;
  }

  .m-col {
    display: flex;
    flex-direction: column;
    gap: 5px;
    flex: 1;
    min-width: 0;
  }

  .m-name {
    display: flex;
    align-items: baseline;
    gap: 4px;
    min-width: 0;
    white-space: nowrap;
  }

  .m-n {
    font: 600 14px/1 var(--font);
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .m-n.hidden {
    font-style: italic;
    font-weight: 500;
    color: var(--text-muted);
  }

  /* The dotted underline hangs below the clipped box: widen the paint clip (as PlayerRow does). */
  .m-n.remembered {
    text-decoration: underline dotted var(--text-faint);
    text-underline-offset: 3px;
    overflow: clip;
    overflow-clip-margin: 6px;
  }

  .m-tag {
    flex: none;
    font: 500 12px/1 var(--font);
    color: var(--text-label);
  }

  .m-icon {
    width: 24px;
    height: 24px;
    display: block;
    flex: none;
  }

  .m-rank {
    font: 600 14px/1 var(--font);
    width: 44px;
    flex: none;
    white-space: nowrap;
  }

  .m-rank.dim {
    color: var(--text-muted);
  }

  .lm-hero {
    position: relative;
    height: 84px;
  }

  .lm-hero img {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
    opacity: 0.45;
  }

  .lm-fade {
    position: absolute;
    inset: 0;
    background: linear-gradient(180deg, rgba(19, 21, 25, 0.1), var(--card));
  }

  .lm-cap {
    position: absolute;
    left: 16px;
    top: 14px;
  }

  .lm-title {
    position: absolute;
    left: 16px;
    bottom: 10px;
    display: flex;
    align-items: baseline;
    gap: 10px;
  }

  .lm-map {
    font: 700 26px/1 var(--font-display);
    letter-spacing: 0.05em;
    text-transform: uppercase;
  }

  .lm-res {
    font: 700 22px/1 var(--font-display);
  }

  .lm-stats {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    padding: 12px 16px 14px;
    gap: 8px;
  }

  .st {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }

  .st .mono {
    color: var(--text-label);
  }

  .sv {
    font: 600 15px/1 var(--font);
    font-variant-numeric: tabular-nums;
  }

  .lm-open {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 38px;
    padding: 0 16px;
    border-top: 1px solid var(--line);
    font: 600 12.5px/1 var(--font);
    color: var(--accent-text);
    cursor: pointer;
  }

  .lm-open:hover {
    background: var(--card-head);
  }

  .lm-open:focus-visible {
    outline-offset: -2px;
  }

  .today {
    display: flex;
    align-items: baseline;
    gap: 14px;
  }

  .net {
    font: 700 30px/1 var(--font-display);
  }

  .wl {
    font: 600 17px/1 var(--font);
    color: var(--text-muted);
  }

  .w {
    color: var(--win);
  }

  .l {
    color: var(--loss);
  }

  .chips {
    display: flex;
    gap: 4px;
  }

  .chip {
    flex: 1;
    height: 22px;
    border-radius: 3px;
    display: flex;
    align-items: center;
    justify-content: center;
    font: 700 10px/1 var(--font-mono);
  }

  .chip.draw {
    border: 1px solid var(--line-strong);
  }

  .rank {
    display: flex;
    align-items: center;
    gap: 14px;
  }

  .rk-icon {
    width: 44px;
    height: 44px;
    display: block;
    flex: none;
  }

  .rk-col {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .rk-top {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
  }

  .rk-name {
    font: 600 20px/1 var(--font-display);
  }

  .rk-track {
    height: 4px;
    border-radius: 2px;
    background: var(--track);
    overflow: hidden;
  }

  .rk-track > div {
    height: 100%;
  }
</style>
