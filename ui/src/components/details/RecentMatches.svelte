<!--
  Recent Competitive Matches (spec §6.4, v8:244-275): newest first, a row per match; a click opens its Match Details.
  A match with no stats row shows — in its stat line (R102); its rank icon does not depend on that (R81).
  `rows: null` = the Details skeleton (§6.9: the header row plus 3 rows of a map box and a bar); `[]` = no history
  (§5.6: one 52px row with — in the Map column, no Show more).
-->
<script lang="ts">
  import { assetUrl } from "../../lib/assets";
  import { ago, kd2, pct0, score } from "../../lib/format";
  import { isRankedTier, mapName } from "../../lib/meta";
  import { meta, now } from "../../lib/stores";
  import { rankTip, recentTimeTip } from "../../lib/tips";
  import { tip } from "../../lib/tooltip.svelte";
  import type { MatchSummary } from "../../lib/types";
  import ChevronRight from "../icons/ChevronRight.svelte";
  import ResultChip from "../ResultChip.svelte";
  import Skel from "../Skel.svelte";
  import ShowMore from "./ShowMore.svelte";

  interface Props {
    rows: MatchSummary[] | null;
    showMore: boolean;
    onopen: (matchId: string) => void;
  }

  let { rows, showMore, onopen }: Props = $props();

  const num = (n: number | null) => (n === null ? "—" : String(n));

  function onkeydown(e: KeyboardEvent, matchId: string) {
    if (e.key !== "Enter" && e.key !== " ") return;
    e.preventDefault();
    onopen(matchId);
  }
</script>

<section class="sec recent">
  <div class="sech">Recent Competitive Matches</div>
  <div class="panel">
    <div class="rhead">
      <span>Map</span><span></span><span></span><span>Score</span><span class="c">K / D / A</span><span class="r">K/D</span><span class="r">HS%</span><span></span>
    </div>
    {#if rows === null}
      {#each [0, 1, 2] as i (i)}
        <div class="rrow sk-row" aria-hidden="true">
          <div class="rmap"><Skel w={64} h={36} r={4} block /><Skel w={90} h={13} block /></div>
        </div>
      {/each}
    {:else if rows.length === 0}
      <div class="rrow none"><span class="nodash">—</span></div>
    {:else}
      {#each rows as m (m.matchId)}
        {@const map = mapName($meta, m.mapId)}
        <div class="rrow" role="button" tabindex="0" onclick={() => onopen(m.matchId)} onkeydown={(e) => onkeydown(e, m.matchId)}>
          <div class="rmap">
            {#if m.mapId}<img class="mimg" src={assetUrl("mapList", m.mapId)} alt="" />{:else}<div class="mimg mbox"></div>{/if}
            <div class="mcol">
              <span class={["mname", !map && map !== undefined && "dash"]} data-ellipsis
                >{#if map === undefined}<Skel w={60} h={10} />{:else}{map ?? "—"}{/if}</span
              >
              <span class="ago" use:tip={recentTimeTip(m.startMs)}>{ago(m.startMs, $now)}</span>
            </div>
          </div>
          {#if m.agentId}<img class="ragent" src={assetUrl("agent", m.agentId)} alt="" />{:else}<div class="ragent"></div>{/if}
          {#if isRankedTier(m.tier)}
            <img class="rrank" src={assetUrl("tier", m.tier)} alt="" use:tip={rankTip($meta, m.tier)} />
          {:else}
            <div class="rrank"></div>
          {/if}
          <div class="rscore"><ResultChip result={m.result} size={20} /><span class="sc">{score(m.roundsWon, m.roundsLost)}</span></div>
          <div class="kda"><span class="r">{num(m.kills)}</span><span class="sl">/</span><span class="c">{num(m.deaths)}</span><span class="sl">/</span><span>{num(m.assists)}</span></div>
          <span class="rkd">{kd2(m.kd)}</span>
          <span class="rhs">{pct0(m.hsPct)}</span>
          <ChevronRight />
        </div>
      {/each}
    {/if}
  </div>
  {#if showMore}<ShowMore />{/if}
</section>

<style>
  .sec {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .sech {
    font-size: 14px;
    font-weight: 600;
  }

  .panel {
    background: var(--card);
    border-radius: 8px;
    overflow: hidden;
  }

  .rhead,
  .rrow {
    display: grid;
    grid-template-columns: 176px 28px 24px 84px minmax(0, 1fr) 48px 44px 10px;
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

  .rrow.sk-row,
  .rrow.none {
    cursor: default;
  }

  .rrow.sk-row:hover,
  .rrow.none:hover {
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

  /* An unknown map keeps the box, with the image's own (no) background: a --raised box would read as a skeleton. */
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

  .mname.dash {
    color: var(--text2);
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

  .rrank {
    display: block;
    width: 24px;
    height: 24px;
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
  .rhs {
    text-align: right;
    font-size: 14px;
    font-variant-numeric: tabular-nums;
  }

  .rkd {
    font-weight: 600;
  }

  .rhs {
    font-weight: 500;
  }
</style>
