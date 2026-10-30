<!--
  The Live screen (design v2 TrackerApp.dc.html 141–447), one of four views by the connection and the live stage:
    error   the Riot Client can't be reached (`appStatus.conn === "error"`)
    board   Agent Select / In Game: the match header and the team tables
    recap   after the user's own match, until they queue again (`appStatus.recapMatchId`, menus or closed)
    waiting VALORANT closed or in its menus
  While the connection is lost the last data stays on screen under the lost banner.
-->
<script lang="ts">
  import ErrorView from "../components/live/ErrorView.svelte";
  import LostBanner from "../components/live/LostBanner.svelte";
  import MatchHeader from "../components/live/MatchHeader.svelte";
  import RecapView from "../components/live/RecapView.svelte";
  import TeamTable from "../components/live/TeamTable.svelte";
  import WaitingView from "../components/live/WaitingView.svelte";
  import { appStatus, marks, settings } from "../lib/appdata.svelte";
  import { cardOpenGuard } from "../lib/clickGuard";
  import { boardTeams, countdown, loadingProgress, matchup, rowView, type RowCtx } from "../lib/livev2";
  import { mapName } from "../lib/meta";
  import { openProfile, router, setGlance } from "../lib/router.svelte";
  import { meta, selfId } from "../lib/stores";
  import type { LiveState } from "../lib/types";

  let { live }: { live: LiveState | null } = $props();

  const status = $derived($appStatus);
  const onBoard = $derived(!!live && (live.stage === "pregame" || live.stage === "ingame"));
  const view = $derived(
    status?.conn === "error" ? "error" : onBoard ? "board" : status?.recapMatchId ? "recap" : "waiting",
  );
  const selfPuuid = $derived($selfId);

  // ── board ──
  const ctx: RowCtx = $derived({
    meta: $meta,
    lobby: live?.players ?? [],
    settings: $settings,
    marks: $marks,
    selected: router.lastProfile,
    stage: live?.stage ?? "closed",
  });
  const teams = $derived(
    live && onBoard
      ? boardTeams(live, $settings.enemyFirst).map((t) => ({ ...t, rows: t.cards.map((c, i) => rowView(c, i, t.cards, ctx)) }))
      : [],
  );
  const progress = $derived(live && onBoard ? loadingProgress(live) : null);
  const pg = $derived(live?.stage === "pregame" ? (status?.pregame ?? null) : null);

  // The Agent Select countdown ticks every second while it shows.
  let nowMs = $state(Date.now());
  $effect(() => {
    if (pg?.phaseEndsMs == null) return;
    nowMs = Date.now();
    const id = setInterval(() => (nowMs = Date.now()), 1000);
    return () => clearInterval(id);
  });
  const pregame = $derived(
    live?.stage === "pregame"
      ? {
          left: pg?.phaseEndsMs == null ? null : countdown(pg.phaseEndsMs - nowMs),
          locked: pg?.locked ?? live.players.filter((p) => p.team === "ally" && p.agentId !== null).length,
          total: pg?.total ?? 5,
        }
      : null,
  );

  // Arms the double-click guard first (App swallows clicks for 400 ms), so the second click can't land on the drawer.
  function open(puuid: string, el: HTMLElement) {
    cardOpenGuard.arm();
    openProfile(puuid, el);
  }
</script>

<div class="live">
  {#if status?.conn === "lost" && view !== "error"}
    <LostBanner {status} />
  {/if}
  {#if view === "error" && status}
    <ErrorView {status} />
  {:else if view === "board" && live}
    <MatchHeader
      mapId={live.mapId}
      mapName={live.mapId ? mapName($meta, live.mapId) : null}
      {pregame}
      {progress}
      matchup={matchup(live)}
      onglance={() => setGlance(true)}
    />
    <div class="tables">
      {#each teams as t (t.team)}
        <TeamTable label={t.label} color={t.color} rows={t.rows} placeholders={t.placeholders} onopen={open} />
      {/each}
      {#if live.stage === "pregame"}
        <div class="enemy-ph">
          <span class="ph-label">ENEMY TEAM</span>
          <span class="ph-text">Riot hides the other team during agent select. Ranks, stats and parties load the moment the match starts.</span>
        </div>
      {/if}
    </div>
  {:else if view === "recap" && status?.recapMatchId}
    <RecapView matchId={status.recapMatchId} {selfPuuid} {live} />
  {:else}
    <WaitingView {live} />
  {/if}
</div>

<style>
  .live {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    padding: 18px 24px;
    gap: 10px;
  }

  .tables {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
    overflow: auto;
  }

  .enemy-ph {
    flex: 1;
    min-height: 250px;
    border: 1px dashed var(--line-control);
    border-radius: 10px;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    background: repeating-linear-gradient(135deg, transparent 0 12px, rgba(255, 255, 255, 0.014) 12px 24px);
  }

  .ph-label {
    font: 700 15px/1 var(--font-display);
    letter-spacing: 0.14em;
    color: var(--enemy);
  }

  .ph-text {
    font: 500 14px/1.45 var(--font);
    color: var(--text-muted);
    text-align: center;
    max-width: 380px;
    text-wrap: pretty;
  }
</style>
