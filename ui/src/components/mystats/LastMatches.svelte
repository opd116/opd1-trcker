<!--
  Recent Matches on My Stats (spec §9.3): the user's newest 5 competitive matches in the Player Details row style
  (§6.4): map picture, map name and time ago, agent, result chip and score, K / D / A, K/D, ACS, HS% and the RR the match
  gave (signed, coloured), then the chevron. A row opens that match's Match Details with the user as the focus player
  (`onopen`; a row without it is plain: no pointer, no click, and no chevron). A match with no stats row shows `—`
  in its stat line (R102).
  `rows: undefined` = the skeleton (the header row and 5 rows of a map box and a bar); `[]` or null = one `—` row.
-->
<script lang="ts">
  import { assetUrl } from "../../lib/assets";
  import { ago, kd2, pct0, score } from "../../lib/format";
  import { rowKeydown } from "../../lib/details";
  import { mapName } from "../../lib/meta";
  import { signedRr, signTone } from "../../lib/mystats";
  import { meta, now } from "../../lib/stores";
  import { ACS_TIP, RECENT_SCOPE, recentTimeTip } from "../../lib/tips";
  import { tip } from "../../lib/tooltip.svelte";
  import type { MatchSummary } from "../../lib/types";
  import ChevronRight from "../icons/ChevronRight.svelte";
  import ResultChip from "../ResultChip.svelte";
  import Skel from "../Skel.svelte";
  import Panel from "./Panel.svelte";

  interface Props {
    rows: MatchSummary[] | null | undefined;
    onopen?: (matchId: string) => void;
  }

  let { rows, onopen }: Props = $props();

  const num = (n: number | null) => (n === null ? "—" : String(n));
  const acs = (n: number | null) => (n === null ? "—" : String(Math.round(n)));
</script>

{#snippet cells(m: MatchSummary)}
  {@const map = mapName($meta, m.mapId)}
  <div class="rmap">
    {#if m.mapId}<img class="mimg" src={assetUrl("mapList", m.mapId)} alt="" />{:else}<div class="mimg mbox"></div>{/if}
    <div class="mcol">
      <span class="mname" data-ellipsis
        >{#if map === undefined}<Skel w={60} h={10} />{:else}{map ?? "—"}{/if}</span
      >
      <span class="ago" use:tip={recentTimeTip(m.startMs)}>{ago(m.startMs, $now)}</span>
    </div>
  </div>
  {#if m.agentId}<img class="ragent" src={assetUrl("agent", m.agentId)} alt="" />{:else}<div class="ragent"></div>{/if}
  <div class="rscore"><ResultChip result={m.result} size={20} /><span class={["sc", (m.roundsWon === null || m.roundsLost === null) && "dim"]}>{score(m.roundsWon, m.roundsLost)}</span></div>
  <div class="kda">
    <span class={["r", m.kills === null && "dim"]}>{num(m.kills)}</span><span class="sl">/</span><span class={["c", m.deaths === null && "dim"]}>{num(m.deaths)}</span><span
      class="sl">/</span
    ><span class:dim={m.assists === null}>{num(m.assists)}</span>
  </div>
  <span class={["rkd", m.kd === null && "dim"]}>{kd2(m.kd)}</span>
  <span class={["racs", m.acs === null && "dim"]}>{acs(m.acs)}</span>
  <span class={["rhs", m.hsPct === null && "dim"]}>{pct0(m.hsPct)}</span>
  <span class={["rrr", signTone(m.rrEarned)]}>{signedRr(m.rrEarned)}</span>
{/snippet}

<Panel title="Recent Matches" name="last5" scope={RECENT_SCOPE}>
  <div class="rhead">
    <span>Map</span><span></span><span>Score</span><span class="c">K / D / A</span><span class="r">K/D</span><span class="r"
      ><span use:tip={ACS_TIP}>ACS</span></span
    ><span class="r">HS%</span><span class="r">RR</span><span></span>
  </div>
  {#if rows === undefined}
    {#each [0, 1, 2, 3, 4] as i (i)}
      <div class="rrow sk-row" aria-hidden="true">
        <div class="rmap"><Skel w={64} h={36} r={4} block /><Skel w={90} h={13} block /></div>
      </div>
    {/each}
  {:else if !rows || rows.length === 0}
    <div class="rrow none"><span class="nodash">—</span></div>
  {:else}
    {#each rows as m (m.matchId)}
      {#if onopen}
        <div class="rrow" role="button" tabindex="0" onclick={() => onopen(m.matchId)} onkeydown={rowKeydown(() => onopen(m.matchId))}>
          {@render cells(m)}<ChevronRight />
        </div>
      {:else}
        <!-- Not clickable (no way to open it): no chevron either, its column stays empty. -->
        <div class="rrow plain">{@render cells(m)}<span></span></div>
      {/if}
    {/each}
  {/if}
</Panel>

<style>
  /* Map | agent | score | K / D / A | K/D | ACS | HS% | RR | chevron. The Details row (§6.4) with ACS and RR added
     and the rank icon dropped: the page is about the user, whose rank is the rank card's. Same inset (8px / 14px) and
     the same map column (176px), so the agent icons stand where the Details rows' do. */
  .rhead,
  .rrow {
    display: grid;
    grid-template-columns: 176px 28px 84px minmax(0, 1fr) 56px 56px 56px 64px 10px;
    column-gap: 12px;
    align-items: center;
    padding: 0 14px 0 8px;
  }

  .rhead {
    height: 30px;
    font-size: 12px;
    font-weight: 500;
    color: var(--text2);
    border-bottom: 1px solid var(--line);
    white-space: nowrap;
  }

  .c {
    text-align: center;
  }

  .r {
    text-align: right;
  }

  .rrow {
    height: 52px;
    cursor: pointer;
  }

  .rrow + .rrow {
    border-top: 1px solid var(--line);
  }

  .rrow:hover {
    background: var(--hover-row);
  }

  .rrow:focus-visible {
    outline-offset: -2px;
  }

  .rrow:last-child {
    border-radius: 0 0 8px 8px;
  }

  .rrow.sk-row,
  .rrow.none,
  .rrow.plain {
    cursor: default;
  }

  .rrow.sk-row:hover,
  .rrow.none:hover,
  .rrow.plain:hover {
    background: none;
  }

  .nodash {
    font-size: 14px;
    font-weight: 500;
    color: var(--text2);
  }

  .rmap {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }

  .mimg {
    display: block;
    width: 64px;
    height: 36px;
    flex: none;
    object-fit: cover;
    border-radius: 4px;
  }

  .mcol {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .mname {
    font-size: 14px;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .ago {
    font-size: 12px;
    font-weight: 500;
    color: var(--text2);
    white-space: nowrap;
    align-self: flex-start;
  }

  .ragent {
    display: block;
    width: 28px;
    height: 28px;
    border-radius: 5px;
    background: var(--raised);
  }

  .rscore {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .sc {
    font-size: 14px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  .kda {
    display: grid;
    grid-template-columns: 24px 14px 18px 14px 24px;
    justify-content: center;
    align-items: center;
    font-size: 14px;
    font-weight: 500;
    font-variant-numeric: tabular-nums;
  }

  .sl {
    text-align: center;
    color: var(--text3);
  }

  .rkd,
  .racs,
  .rhs,
  .rrr {
    text-align: right;
    font-size: 14px;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  .rkd,
  .rrr {
    font-weight: 600;
  }

  .dim,
  .rrr.dim {
    color: var(--text2);
  }

  .racs,
  .rhs {
    font-weight: 500;
  }

  .rrr.win {
    color: var(--win);
  }

  .rrr.loss {
    color: var(--loss);
  }
</style>
