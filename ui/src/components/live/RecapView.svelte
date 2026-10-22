<!--
  The match recap (design v2 TrackerApp.dc.html 372–447): after the user's own match, until they queue again
  (`appStatus.recapMatchId`). Hero (result, score, your numbers, TEAM MVP), the two teams by ACS with a bookmark per
  player, and the right column: your rank with this match's RR, the session, and the full scoreboard button.
  The design's PEOPLE card ("You beat Mara for the first time in 4 meetings") is left out: after the match the live
  cards are gone and nothing on the wire says who you met before, so it would have to be invented.
  The view comes from `match_details(recapMatchId, self)`; leaving the recap closes it in Rust unless the full
  scoreboard took it over.
-->
<script lang="ts">
  import { closeMatch, getMatchDetails } from "../../lib/api";
  import { marks, toggleSaved } from "../../lib/appdata.svelte";
  import { assetUrl } from "../../lib/assets";
  import { queueLabel } from "../../lib/historyv2";
  import { durationMin, rrDelta } from "../../lib/format";
  import { retryImg } from "../../lib/imgRetry";
  import { byAcs, kdaText, nameView, nextTierText, recapModel, rrColor, SESSION_CHIPS, sessionChip } from "../../lib/livev2";
  import { mapName } from "../../lib/meta";
  import { openMatch, router } from "../../lib/router.svelte";
  import { meta, mystats } from "../../lib/stores";
  import { tierColor, tierName } from "../../lib/theme";
  import { tip } from "../../lib/tooltip.svelte";
  import type { LiveState, MatchDetailsView } from "../../lib/types";
  import { reportError } from "../../lib/uilog";
  import Shimmer from "./Shimmer.svelte";
  import WaitingView from "./WaitingView.svelte";

  let { matchId, selfPuuid, live }: { matchId: string; selfPuuid: string | null; live: LiveState | null } = $props();

  let view = $state<MatchDetailsView | null>(null);
  let failed = $state(false);

  $effect(() => {
    const id = matchId;
    const focus = selfPuuid;
    view = null;
    failed = false;
    if (!focus) return;
    let alive = true;
    getMatchDetails(id, focus)
      .then((v) => {
        if (alive) view = v;
      })
      .catch((err: unknown) => {
        if (!alive) return;
        failed = true;
        reportError("recap", err);
      });
    return () => {
      alive = false;
      // "Open full scoreboard" re-opens this same match as the History scoreboard: leave it open then.
      if (router.matchId !== id) closeMatch(id).catch((err: unknown) => reportError("recap close", err));
    };
  });

  const ms = $derived($mystats ?? null);
  const model = $derived(view ? recapModel(view, ms) : null);
  const rank = $derived(ms && typeof ms.rank.tier === "number" ? ms.rank : null);
  const session = $derived(ms?.session ?? null);

  /** The rank bar: the RR before this match, then this match's change (green up to the RR now, red past it). */
  const bar = $derived.by(() => {
    if (!rank || rank.rr === null) return null;
    const clamp = (n: number) => Math.max(0, Math.min(100, n));
    const now = clamp(rank.rr);
    const d = model?.rr ?? null;
    if (d === null || d === 0) return { base: now, gain: 0, loss: 0 };
    if (d > 0) return { base: clamp(rank.rr - d), gain: now - clamp(rank.rr - d), loss: 0 };
    return { base: now, gain: 0, loss: clamp(rank.rr - d) - now };
  });

  const caption = $derived.by(() => {
    if (!view) return "";
    const parts = ["MATCH OVER", (mapName($meta, view.mapId) ?? "").toUpperCase(), queueLabel(view.queueId).toUpperCase()];
    if (view.durationMs !== null) parts.push(durationMin(view.durationMs).toUpperCase());
    return parts.filter(Boolean).join(" · ");
  });

  function openFull() {
    if (!selfPuuid) return;
    openMatch(matchId, { from: "recap", focusPuuid: selfPuuid });
  }
</script>

{#if failed || !selfPuuid}
  <WaitingView {live} />
{:else}
  <div class="recap">
    <div class="left">
      {#if !view || !model}
        <div class="hero sk"><Shimmer w={260} h={64} r={6} /><Shimmer w={360} h={20} /></div>
        <div class="teams">
          <div class="team sk-team"></div>
          <div class="team sk-team"></div>
        </div>
      {:else}
        <div class="hero">
          <img {@attach retryImg} class="splash" src={assetUrl("mapSplash", view.mapId)} alt="" />
          <div class="hero-fade"></div>
          <div class="hero-in">
            <span class="mono">{caption}</span>
            <div class="res">
              <span class="word" style:color={model.color}>{model.word}</span>
              {#if model.ours !== null && model.theirs !== null}<span class="score">{model.ours}<span class="colon">:</span>{model.theirs}</span>{/if}
            </div>
            <div class="stats">
              {#each model.stats as s (s.l)}
                <div class="stat"><span class="mono dim">{s.l}</span><span class="sv" style:color={s.c}>{s.v}</span></div>
              {/each}
              {#if model.mvp}<div class="mvp-wrap"><span class="mvp">TEAM MVP</span></div>{/if}
            </div>
          </div>
        </div>
        <div class="teams">
          {#each view.teams.slice(0, 2) as t, ti (t.teamId)}
            {@const rows = byAcs(t.players)}
            <div class="team">
              <div class="t-head">
                <span class="t-label" style:color={ti === 0 ? "var(--ally)" : "var(--enemy)"}>{ti === 0 ? "YOUR TEAM" : "ENEMY TEAM"}</span>
                <span class="t-score">{t.roundsWon ?? "—"}</span>
                <div class="fill"></div>
                <span class="mono col acs">ACS</span>
                <span class="mono col kda">K / D / A</span>
                <span class="sp"></span>
              </div>
              {#each rows as r, i (r.puuid)}
                {@const nm = nameView(r.name, $meta)}
                {@const saved = $marks.saved.includes(r.puuid)}
                <div class="t-row" class:bb={i < rows.length - 1} class:self={r.isSelf}>
                  <span class="t-agent">{#if r.agentId}<img {@attach retryImg} src={assetUrl("agent", r.agentId)} alt="" />{/if}</span>
                  {#if typeof r.tier === "number"}<img {@attach retryImg} class="t-rank" src={assetUrl("tier", r.tier >= 3 ? r.tier : 0)} alt="" use:tip={tierName(r.tier, $meta)} />{:else}<span class="t-rank"></span>{/if}
                  <span class={["t-name", nm.style]} data-ellipsis use:tip={nm.style === "remembered" ? nm.tip : ""}>{nm.text}</span>
                  {#if i === 0 && r.acs !== null}<span class="t-mvp">{ti === 0 ? "MVP" : "TOP"}</span>{/if}
                  <div class="fill"></div>
                  <span class="t-acs">{r.acs === null ? "—" : Math.round(r.acs)}</span>
                  <span class="t-kda">{kdaText(r.kills, r.deaths, r.assists, " / ")}</span>
                  {#if r.isSelf}
                    <span class="sp"></span>
                  {:else}
                    <button
                      type="button"
                      class="save"
                      class:on={saved}
                      aria-pressed={saved}
                      aria-label={saved ? `Saved: ${nm.text}` : `Save ${nm.text}`}
                      use:tip={saved ? "Saved" : "Save player"}
                      onclick={() => toggleSaved(r.puuid)}
                      ><svg width="11" height="13" viewBox="0 0 10 12" fill={saved ? "currentColor" : "none"} stroke="currentColor" stroke-width="1.2" stroke-linejoin="round" aria-hidden="true"
                        ><path d="M1 1h8v10L5 8.2 1 11z"></path></svg
                      ></button
                    >
                  {/if}
                </div>
              {/each}
            </div>
          {/each}
        </div>
      {/if}
    </div>

    <div class="right">
      {#if rank}
        {@const next = nextTierText(rank.tier, rank.rrToNext, rank.avgWin, $meta)}
        <div class="card r-rank">
          <span class="mono">RANK</span>
          <div class="rk">
            <img {@attach retryImg} src={assetUrl("tier", rank.tier! >= 3 ? rank.tier! : 0)} alt="" />
            <div class="rk-col">
              <span class="rk-name" style:color={tierColor(rank.tier)}>{tierName(rank.tier, $meta)}</span>
              <span class="rk-rr">{rank.rr ?? "—"} RR{#if model?.rr !== null && model?.rr !== undefined}&nbsp;<span style:color={rrColor(model.rr)}>{rrDelta(model.rr)}</span>{/if}</span>
            </div>
          </div>
          {#if bar}
            <div class="rk-track">
              <div style:left="0" style:width="{bar.base}%" style:background={tierColor(rank.tier)}></div>
              {#if bar.gain > 0}<div style:left="{bar.base}%" style:width="{bar.gain}%" style:background="var(--win)"></div>{/if}
              {#if bar.loss > 0}<div style:left="{bar.base}%" style:width="{bar.loss}%" style:background="var(--loss)"></div>{/if}
            </div>
          {/if}
          {#if next}<span class="rk-next">{next}</span>{/if}
        </div>
      {/if}

      {#if session && session.matches.length > 0}
        <div class="card r-session">
          <div class="cap"><span class="mono">SESSION</span><span class="mono n">{session.matches.length} {session.matches.length === 1 ? "MATCH" : "MATCHES"}</span></div>
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

      <div class="fill"></div>
      <button type="button" class="primary" onclick={openFull}>Open full scoreboard</button>
      <span class="mono clears">CLEARS WHEN YOU QUEUE AGAIN</span>
    </div>
  </div>
{/if}

<style>
  .recap {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: minmax(0, 1fr) 330px;
    gap: 12px;
  }

  .left,
  .right {
    display: flex;
    flex-direction: column;
    gap: 12px;
    min-height: 0;
  }

  .mono {
    font: 500 10.5px/1 var(--font-mono);
    letter-spacing: 0.06em;
    color: var(--text-muted);
  }

  .mono.dim {
    color: var(--text-dim);
  }

  .mono.n {
    color: var(--text-label);
  }

  .hero {
    position: relative;
    flex: none;
    height: 206px;
    border-radius: 12px;
    overflow: hidden;
    border: 1px solid var(--line);
    background: var(--card);
  }

  .hero.sk {
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 18px;
    padding: 20px 24px;
  }

  .splash {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
    opacity: 0.42;
  }

  .hero-fade {
    position: absolute;
    inset: 0;
    background: linear-gradient(90deg, #0f1114 0%, rgba(15, 17, 20, 0.82) 45%, rgba(15, 17, 20, 0.35) 100%);
  }

  .hero-in {
    position: relative;
    height: 100%;
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    padding: 20px 24px;
  }

  .res {
    display: flex;
    align-items: flex-end;
    gap: 24px;
  }

  .word {
    font: 700 76px/0.8 var(--font-display);
    letter-spacing: 0.02em;
  }

  .score {
    font: 700 46px/0.8 var(--font-display);
    color: var(--text);
  }

  .colon {
    color: var(--line-strong);
    margin: 0 8px;
  }

  .stats {
    display: flex;
    gap: 30px;
  }

  .stat {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .sv {
    font: 700 20px/1 var(--font);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  .mvp-wrap {
    display: flex;
    align-items: center;
  }

  .mvp {
    font: 600 10px/1 var(--font-mono);
    letter-spacing: 0.06em;
    color: var(--on-accent);
    background: var(--mvp);
    padding: 5px 8px;
    border-radius: 4px;
  }

  .teams {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }

  .team {
    background: var(--card);
    border: 1px solid var(--line);
    border-radius: 12px;
    overflow: hidden;
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .t-head {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 36px;
    padding: 0 14px;
    border-bottom: 1px solid var(--line);
    background: var(--card-head);
    flex: none;
  }

  .t-label {
    font: 700 14px/1 var(--font-display);
    letter-spacing: 0.12em;
  }

  .t-score {
    font: 700 15px/1 var(--font);
    color: var(--text);
  }

  .col {
    color: var(--text-label);
    text-align: right;
  }

  .acs {
    width: 36px;
  }

  .kda {
    width: 66px;
    white-space: nowrap;
  }

  .sp {
    width: 28px;
    flex: none;
  }

  .fill {
    flex: 1;
  }

  .t-row {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 44px;
    padding: 0 8px 0 14px;
    flex: none;
  }

  .t-row.bb {
    border-bottom: 1px solid #1b1e22;
  }

  .t-row.self {
    background: var(--accent-faint);
  }

  .t-agent {
    width: 28px;
    height: 28px;
    border-radius: 5px;
    background: var(--skel);
    overflow: hidden;
    flex: none;
  }

  .t-agent img {
    width: 100%;
    height: 100%;
    display: block;
  }

  .t-rank {
    width: 18px;
    height: 18px;
    display: block;
    flex: none;
  }

  .t-name {
    font: 600 13.5px/1 var(--font);
    color: var(--text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }

  .self .t-name {
    color: var(--accent-pale);
  }

  .t-name.hidden {
    font-style: italic;
    font-weight: 500;
    color: var(--text-muted);
    padding-right: 2px;
  }

  .t-name.remembered {
    text-decoration: underline dotted var(--text-faint);
    text-underline-offset: 3px;
    overflow: clip;
    overflow-clip-margin: 6px;
  }

  .t-mvp {
    flex: none;
    font: 600 10px/1 var(--font-mono);
    letter-spacing: 0.08em;
    color: var(--mvp);
    border: 1px solid rgba(240, 199, 94, 0.45);
    padding: 2px 4px;
    border-radius: 3px;
  }

  .t-acs {
    font: 600 14px/1 var(--font);
    width: 36px;
    text-align: right;
    flex: none;
    font-variant-numeric: tabular-nums;
  }

  .t-kda {
    font: 500 12.5px/1 var(--font);
    color: var(--text-muted);
    width: 66px;
    text-align: right;
    white-space: nowrap;
    flex: none;
  }

  .save {
    width: 28px;
    height: 28px;
    flex: none;
    border-radius: 6px;
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    color: var(--text-dim);
    background: transparent;
    border: 0;
    padding: 0;
  }

  .save.on {
    color: var(--accent);
  }

  .save:hover {
    background: var(--chip-hover);
  }

  .card {
    flex: none;
    background: var(--card);
    border: 1px solid var(--line);
    border-radius: 12px;
    padding: 18px;
    display: flex;
    flex-direction: column;
  }

  .r-rank {
    gap: 16px;
  }

  .rk {
    display: flex;
    align-items: center;
    gap: 14px;
  }

  .rk img {
    width: 54px;
    height: 54px;
    display: block;
  }

  .rk-col {
    display: flex;
    flex-direction: column;
    gap: 7px;
  }

  .rk-name {
    font: 600 24px/1 var(--font-display);
  }

  .rk-rr {
    font: 500 11px/1 var(--font-mono);
    color: var(--text-muted);
  }

  .rk-track {
    position: relative;
    height: 6px;
    border-radius: 3px;
    background: var(--track);
    overflow: hidden;
  }

  .rk-track > div {
    position: absolute;
    top: 0;
    bottom: 0;
  }

  .rk-next {
    font: 500 13px/1.4 var(--font);
    color: var(--text-dim);
  }

  .r-session {
    gap: 12px;
  }

  .cap {
    display: flex;
    justify-content: space-between;
  }

  .today {
    display: flex;
    align-items: baseline;
    gap: 14px;
  }

  .net {
    font: 700 28px/1 var(--font-display);
  }

  .wl {
    font: 600 16px/1 var(--font);
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

  .primary {
    flex: none;
    height: 36px;
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

  .clears {
    line-height: 1.5;
    letter-spacing: 0.04em;
    color: var(--text-label);
    text-align: center;
  }
</style>
