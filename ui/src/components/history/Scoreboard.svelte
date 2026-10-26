<!--
  The scoreboard (design v2 589–657, logic `scoreboard()` 1627–1674): the History screen with a match open. Back
  (labelled by where it was opened from) and Esc call `backFromMatch`. The hero: the map splash, the map, the mode /
  date / duration line, VICTORY / DEFEAT / DRAW with the score and the halves, and YOUR RR when the user's RR change for
  that match is known (their own history row, else My Stats). Then one table per team, rows by ACS, with YOU,
  MATCH MVP / TEAM MVP and a bookmark on everyone but the user. The data is the router's `openMatch` slot (it fetched
  `match_details` when the match opened); a `match` event replaces it in place (names that arrived later).
  Ported from the old Match page: hidden names (the backend's DisplayName), `—` for a player without a stats row (the
  rank icon stays), a no-team mode (Deathmatch) as one `Players` block, and the settled-empty view (`emptyMatch`, no
  teams). The design's party gutter is kept as empty space: `match_details` has no party field.
-->
<script lang="ts">
  import { matchHistory } from "../../lib/api";
  import { savedSet, toggleSaved } from "../../lib/appdata.svelte";
  import { assetUrl } from "../../lib/assets";
  import { pct0, rrDelta } from "../../lib/format";
  import { pmText } from "../../lib/details";
  import { byAcs, endMs, halvesLine, heroMeta, kdaText, kdOf, mvpOf, RESULT_LETTER, RESULT_WORD_BIG, stamp, yourRrOf } from "../../lib/historyv2";
  import { retryImg } from "../../lib/imgRetry";
  import { isRankedTier, mapName } from "../../lib/meta";
  import { backFromMatch, matchOpenedByKeyboard, router, scrollHost } from "../../lib/router.svelte";
  import { nameParts } from "../../lib/savedv2";
  import { live, meta, mystats, now, openMatch } from "../../lib/stores";
  import { tierColor, tierName } from "../../lib/theme";
  import { ACS_TIP, ADR_TIP, PM_TIP } from "../../lib/tips";
  import { tip } from "../../lib/tooltip.svelte";
  import type { DisplayName, MatchDetailsView, MatchTeamView, Result } from "../../lib/types";
  import { reportError } from "../../lib/uilog";
  import Skel from "../Skel.svelte";
  import { cachedRow } from "./cache";
  import PlayerName from "./PlayerName.svelte";

  let { matchId }: { matchId: string } = $props();

  /** The open view, only when it is this match's. */
  const view = $derived<MatchDetailsView | null>($openMatch?.matchId === matchId ? $openMatch : null);
  const teams = $derived(view?.teams ?? []);
  const noTeams = $derived(teams.length < 2);
  /** `emptyMatch`: `match_details` failed twice, nothing to show but the frame. */
  const settledEmpty = $derived(view !== null && teams.length === 0);

  // ── Back ──
  /**
   * Opening the scoreboard from the keyboard destroyed the row that had the focus: it starts on Back instead of on
   * nothing. (A click leaves the focus alone, so no ring appears for the mouse.)
   */
  let backBtn = $state<HTMLButtonElement | null>(null);
  $effect(() => {
    if (backBtn && matchOpenedByKeyboard()) backBtn.focus({ preventScroll: true });
  });

  function nameOf(puuid: string): DisplayName | null {
    for (const t of teams) for (const p of t.players) if (p.puuid === puuid) return p.name;
    return $live?.players.find((p) => p.puuid === puuid)?.name ?? null;
  }

  const backLabel = $derived.by(() => {
    switch (router.matchFrom) {
      case "recap":
      case "lastMatch":
        return "Live";
      case "comp":
        return "Competitive";
      case "encounter":
      case "profile": {
        const n = router.encFrom ? nameOf(router.encFrom.puuid) : null;
        return n ? nameParts(n, $meta).name : "Profile";
      }
      default:
        return "Match history";
    }
  });

  const crumb = $derived.by(() => {
    if (router.matchFrom === "recap") return "Just finished";
    if (!view || settledEmpty) return "";
    const s = stamp(endMs(view), $now);
    return `${s.dayLabel} · ${s.hm}`;
  });

  function onWindowKey(e: KeyboardEvent) {
    if (e.key !== "Escape" || e.defaultPrevented || router.profile !== null) return;
    e.preventDefault();
    backFromMatch();
  }

  // ── hero ──
  const us = $derived(teams[0] ?? null);
  const them = $derived(teams[1] ?? null);
  const result = $derived<Result | null>(noTeams ? null : (us?.result ?? null));
  const halves = $derived(view ? halvesLine(view) : null);
  const map = $derived(view && view.mapId ? mapName($meta, view.mapId) : null);

  /** The user's RR change and rank after it, for their own competitive match: their history row, else My Stats. */
  let fetchedRow = $state<{ id: string; rrEarned: number | null; tierAfter: number | null } | null>(null);
  const yourRr = $derived.by(() => {
    if (!view || view.perspective !== "self" || view.queueId !== "competitive") return null;
    return yourRrOf(matchId, cachedRow(matchId) ?? (fetchedRow?.id === matchId ? fetchedRow : null), $mystats);
  });

  // Not in what is loaded: ask the newest page of the user's competitive history once (a local DB read).
  let asked = false;
  $effect(() => {
    if (asked || !view || view.perspective !== "self" || view.queueId !== "competitive" || cachedRow(matchId)) return;
    asked = true;
    matchHistory("competitive", 0, 50)
      .then((p) => {
        const r = p.rows.find((x) => x.matchId === matchId);
        if (r) fetchedRow = { id: matchId, rrEarned: r.rrEarned, tierAfter: r.tierAfter };
      })
      .catch((err: unknown) => reportError("match_history", err));
  });

  // ── tables ──
  const mvp = $derived(view ? mvpOf(view) : { match: null, team: new Set<string>() });

  function teamLabel(i: number): string {
    if (noTeams) return "PLAYERS";
    if (i !== 0) return "ENEMY TEAM";
    if (view?.perspective === "self") return "YOUR TEAM";
    const f = teams.flatMap((t) => t.players).find((p) => p.isFocus);
    return f ? `${nameParts(f.name, $meta).name}'s team` : "YOUR TEAM";
  }

  const teamColor = (i: number) => (noTeams ? "var(--accent)" : i === 0 ? "var(--ally)" : "var(--enemy)");
  const RES_COLOR = { win: "var(--win)", loss: "var(--loss)", draw: "var(--text-dim)" } as const;
  const RES_BG = { win: "rgba(95,207,143,.15)", loss: "rgba(239,106,91,.15)", draw: "rgba(138,143,151,.15)" } as const;
  const PM_COLOR = { win: "var(--win)", loss: "var(--loss)", flat: "var(--text-dim)" } as const;
  const num = (n: number | null) => (n === null ? "—" : String(Math.round(n)));
</script>

<svelte:window onkeydown={onWindowKey} />

{#snippet colHead()}
  <span class="h r" use:tip={ACS_TIP}>ACS</span>
  <span class="h c">K / D / A</span>
  <span class="h r" use:tip={PM_TIP}>+/–</span>
  <span class="h r">K/D</span>
  <span class="h r" use:tip={ADR_TIP}>ADR</span>
  <span class="h r">HS%</span>
  <span></span>
{/snippet}

{#snippet table(t: MatchTeamView, i: number)}
  <div class="team">
    <div class="thead">
      <div class="gut"><div class="tbar" style:background={teamColor(i)}></div></div>
      <div class="tname">
        {#if !noTeams && t.result}
          <span class="tres" style:background={RES_BG[t.result]} style:color={RES_COLOR[t.result]}>{RESULT_LETTER[t.result]}</span>
        {/if}
        <span class="tlabel" style:color={teamColor(i)} data-ellipsis>{teamLabel(i)}</span>
        {#if !noTeams}<span class="tscore">{t.roundsWon ?? "—"}</span>{/if}
      </div>
      {@render colHead()}
    </div>
    {#each byAcs(t.players) as p, j (p.puuid)}
      {@const pm = pmText(p.kills, p.deaths)}
      {@const saved = $savedSet.has(p.puuid)}
      <div class={["srow", p.isSelf && "self", p.isFocus && !p.isSelf && "focus", j === t.players.length - 1 && "last"]}>
        <div class="selfbar"></div>
        <div class="gut"></div>
        <div class="who">
          {#if p.agentId}<img {@attach retryImg} class="agent" src={assetUrl("agent", p.agentId)} alt="" />{:else}<div class="agent"></div>{/if}
          {#if isRankedTier(p.tier)}
            <img {@attach retryImg} class="rank" src={assetUrl("tier", p.tier)} alt="" use:tip={tierName(p.tier, $meta)} />
          {:else}
            <div class="rank"></div>
          {/if}
          <PlayerName name={p.name} size="board" />
          {#if p.isSelf}<span class="you">YOU</span>{/if}
          {#if mvp.match === p.puuid}<span class="mvp">MATCH MVP</span>{:else if mvp.team.has(p.puuid)}<span class="mvp">TEAM MVP</span>{/if}
        </div>
        <span class="acs">{num(p.acs)}</span>
        <span class="kda">{kdaText(p.kills, p.deaths, p.assists)}</span>
        <span class="stat r" style:color={PM_COLOR[pm.tone]}>{pm.text}</span>
        <span class="stat r">{kdOf(p.kills, p.deaths)}</span>
        <span class="stat r">{num(p.adr)}</span>
        <span class="stat r">{pct0(p.hsPct)}</span>
        {#if p.isSelf}
          <span></span>
        {:else}
          <button
            type="button"
            class={["save", saved && "on"]}
            aria-pressed={saved}
            aria-label={saved ? "Saved" : "Save player"}
            use:tip={saved ? "Saved" : "Save player"}
            onclick={() => toggleSaved(p.puuid)}
          >
            <svg width="11" height="13" viewBox="0 0 10 12" fill={saved ? "currentColor" : "none"} stroke="currentColor" stroke-width="1.2" stroke-linejoin="round" aria-hidden="true"
              ><path d="M1 1h8v10L5 8.2 1 11z"></path></svg
            >
          </button>
        {/if}
      </div>
    {/each}
  </div>
{/snippet}

<div class="match" aria-busy={view === null} use:scrollHost>
  <div class="top">
    <button type="button" class="back" bind:this={backBtn} onclick={() => backFromMatch()}>
      <svg width="7" height="12" viewBox="0 0 7 12" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" aria-hidden="true"
        ><path d="M5.5 1.5L1.5 6l4 4.5"></path></svg
      ><span class="blabel" data-ellipsis>{backLabel}</span>
    </button>
    {#if crumb}<span class="crumb">{crumb}</span>{/if}
  </div>

  {#if view}
    <div class="hero">
      {#if view.mapId}<img {@attach retryImg} class="splash" src={assetUrl("mapSplash", view.mapId)} alt="" />{/if}
      <div class="shade"></div>
      <div class="hin">
        <div class="hl">
          <span class="hmap">{#if map === undefined}<Skel w={160} h={40} />{:else}{map ?? "—"}{/if}</span>
          <span class="hmeta">{settledEmpty ? "—" : heroMeta(view)}</span>
        </div>
        <div class="grow"></div>
        {#if !noTeams && us && them}
          <div class="hscore">
            {#if result}<span class="hres" style:color={RES_COLOR[result]}>{RESULT_WORD_BIG[result]}</span>{/if}
            {#if us.roundsWon !== null && them.roundsWon !== null}
              <span class="big">{us.roundsWon}<span class="colon">:</span>{them.roundsWon}</span>
            {/if}
            {#if halves}<span class="halves">{halves}</span>{/if}
          </div>
        {/if}
        {#if yourRr}
          <div class="vdiv"></div>
          <div class="yrr">
            <span class="ylabel">YOUR RR</span>
            <span class="yval" style:color={yourRr.rr > 0 ? "var(--win)" : yourRr.rr < 0 ? "var(--loss)" : "var(--text-dim)"}>{rrDelta(yourRr.rr)}</span>
            {#if isRankedTier(yourRr.tier)}
              <div class="yrank">
                <img {@attach retryImg} class="yicon" src={assetUrl("tier", yourRr.tier)} alt="" /><span style:color={tierColor(yourRr.tier)}
                  >{tierName(yourRr.tier, $meta)}</span
                >
              </div>
            {/if}
          </div>
        {/if}
      </div>
    </div>

    {#if settledEmpty}
      <div class="team">
        <div class="thead">
          <div class="gut"></div>
          <div class="tname"></div>
          {@render colHead()}
        </div>
      </div>
    {:else}
      {#each teams as t, i (`${i}:${t.teamId}`)}{@render table(t, i)}{/each}
    {/if}
  {:else}
    <!-- Loading: the hero and two team tables as neutral shapes. -->
    <div class="hero skel" aria-hidden="true">
      <div class="hin">
        <div class="hl"><Skel w={180} h={40} block /><Skel w={260} h={11} block /></div>
        <div class="grow"></div>
        <div class="hscore"><Skel w={130} h={46} block /></div>
      </div>
    </div>
    {#each [0, 1] as i (i)}
      <div class="team" aria-hidden="true">
        <div class="thead"><div class="gut"></div><div class="tname"><Skel w={120} h={14} /></div></div>
        {#each [0, 1, 2, 3, 4] as j (j)}
          <div class={["srow", j === 4 && "last"]}>
            <div class="selfbar"></div>
            <div class="gut"></div>
            <div class="who"><Skel w={32} h={32} r={6} block /><Skel w={20} h={20} r="50%" block /><Skel w={110} h={13} block /></div>
            {#each [0, 1, 2, 3, 4, 5] as k (k)}<span class={k === 1 ? "c" : "r"}><Skel w={k === 1 ? 70 : 30} h={13} /></span>{/each}
            <span></span>
          </div>
        {/each}
      </div>
    {/each}
  {/if}
</div>

<style>
  .match {
    flex: 1;
    min-height: 0;
    overflow: auto;
    display: flex;
    flex-direction: column;
    padding: 18px 24px 18px;
    gap: 12px;
  }

  .top {
    display: flex;
    align-items: center;
    gap: 12px;
    flex: none;
  }

  .back {
    height: 30px;
    padding: 0 12px 0 9px;
    border-radius: 7px;
    background: var(--card-head);
    color: var(--text-soft);
    border: 1px solid var(--line-control);
    font: 600 12.5px/1 var(--font);
    cursor: pointer;
    display: flex;
    align-items: center;
    gap: 7px;
  }

  .back:hover {
    background: var(--button-hover);
    border-color: var(--line-hover);
  }

  .blabel {
    max-width: 240px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .crumb {
    font: 500 10.5px/1 var(--font-mono);
    letter-spacing: 0.06em;
    color: var(--text-label);
  }

  .hero {
    position: relative;
    flex: none;
    height: 132px;
    border-radius: 12px;
    overflow: hidden;
    border: 1px solid var(--line);
    background: var(--card);
  }

  .splash {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
    opacity: 0.4;
  }

  .shade {
    position: absolute;
    inset: 0;
    background: linear-gradient(90deg, #101215 0%, rgba(16, 18, 21, 0.75) 50%, rgba(16, 18, 21, 0.5) 100%);
  }

  .hin {
    position: relative;
    height: 100%;
    display: flex;
    align-items: center;
    gap: 28px;
    padding: 0 28px;
  }

  .hl {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-width: 0;
  }

  .hmap {
    font: 700 46px/0.9 var(--font-display);
    letter-spacing: 0.05em;
    text-transform: uppercase;
    white-space: nowrap;
  }

  .hmeta {
    font: 500 10.5px/1 var(--font-mono);
    letter-spacing: 0.06em;
    color: var(--text-muted);
    white-space: nowrap;
  }

  .grow {
    flex: 1;
  }

  .hscore {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 8px;
  }

  .hres {
    font: 700 13px/1 var(--font-display);
    letter-spacing: 0.16em;
  }

  .big {
    font: 700 54px/0.85 var(--font-display);
    white-space: nowrap;
  }

  .colon {
    color: var(--line-strong);
    margin: 0 10px;
  }

  .halves {
    font: 500 10.5px/1 var(--font-mono);
    letter-spacing: 0.08em;
    color: var(--text-dim);
  }

  .vdiv {
    width: 1px;
    height: 70px;
    background: rgba(255, 255, 255, 0.1);
  }

  .yrr {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 120px;
  }

  .ylabel {
    font: 500 10.5px/1 var(--font-mono);
    letter-spacing: 0.06em;
    color: var(--text-dim);
  }

  .yval {
    font: 700 34px/0.9 var(--font-display);
  }

  .yrank {
    display: flex;
    align-items: center;
    gap: 6px;
    font: 600 13px/1 var(--font);
    white-space: nowrap;
  }

  .yicon {
    width: 18px;
    height: 18px;
    display: block;
  }

  .team {
    flex: none;
    background: var(--card);
    border: 1px solid var(--line);
    border-radius: 10px;
    overflow: hidden;
  }

  .thead,
  .srow {
    display: grid;
    grid-template-columns: 18px minmax(0, 1fr) 64px 120px 56px 56px 56px 56px 32px;
    column-gap: 14px;
    align-items: center;
    padding-right: 10px;
  }

  .thead {
    height: 36px;
    background: var(--card-head);
    border-bottom: 1px solid var(--line);
  }

  .gut {
    height: 100%;
    display: flex;
  }

  .tbar {
    width: 3px;
    height: 100%;
  }

  .tname {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }

  .tres {
    flex: none;
    width: 22px;
    height: 22px;
    border-radius: 4px;
    display: flex;
    align-items: center;
    justify-content: center;
    font: 700 10.5px/1 var(--font-mono);
  }

  .tlabel {
    font: 700 15px/1 var(--font-display);
    letter-spacing: 0.12em;
    text-transform: uppercase;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .tscore {
    flex: none;
    font: 700 16px/1 var(--font);
  }

  .h {
    font: 500 10.5px/1 var(--font-mono);
    letter-spacing: 0.06em;
    color: var(--text-label);
    white-space: nowrap;
  }

  .r {
    text-align: right;
  }

  .c {
    text-align: center;
  }

  .srow {
    position: relative;
    height: 46px;
    border-bottom: 1px solid #1b1e22;
  }

  .srow.last {
    border-bottom: none;
  }

  .srow.self {
    background: rgba(157, 140, 255, 0.06);
  }

  .selfbar {
    position: absolute;
    left: 0;
    top: 0;
    bottom: 0;
    width: 2px;
  }

  .self .selfbar,
  .focus .selfbar {
    background: var(--accent);
  }

  .who {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }

  .agent {
    width: 32px;
    height: 32px;
    border-radius: 6px;
    display: block;
    background: var(--skel);
    flex: none;
  }

  .rank {
    width: 20px;
    height: 20px;
    display: block;
    flex: none;
  }

  .you {
    flex: none;
    font: 700 10px/1 var(--font-mono);
    letter-spacing: 0.06em;
    color: var(--on-accent);
    background: var(--accent);
    padding: 3px 5px;
    border-radius: 3px;
  }

  .mvp {
    flex: none;
    font: 700 10px/1 var(--font-mono);
    letter-spacing: 0.08em;
    color: var(--mvp);
    border: 1px solid rgba(240, 199, 94, 0.45);
    padding: 2px 5px;
    border-radius: 3px;
    white-space: nowrap;
  }

  .acs {
    font: 700 16px/1 var(--font);
    text-align: right;
    font-variant-numeric: tabular-nums;
  }

  .kda {
    font: 500 14px/1 var(--font);
    color: var(--text-2);
    text-align: center;
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }

  .stat {
    font: 600 14px/1 var(--font);
    color: var(--text-2);
    font-variant-numeric: tabular-nums;
  }

  .save {
    width: 28px;
    height: 28px;
    border-radius: 6px;
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    color: var(--text-dim);
    background: transparent;
    border: none;
    padding: 0;
  }

  .save:hover {
    background: var(--chip-hover);
  }

  .save.on {
    color: var(--accent);
  }
</style>
