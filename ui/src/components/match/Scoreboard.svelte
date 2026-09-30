<!--
  Match Details scoreboard (spec §6.10, v8:379-417): per team a header (result chip, rounds, `Your team` /
  `{name}'s team` / `Opponents`, or `Players` in a no-team mode) and its rows by ACS. The focus player's row has a 3px
  accent bar, the user's a frame. Names are the backend's DisplayName, hidden-name rules applied (R74, R93). A player with
  no stats row shows — in every stat, never 0, and keeps the rank icon (R124, R81). Rows are not clickable.
  `view: null` = the loading board (2 team headers of 5 grey rows).
-->
<script lang="ts">
  import { assetUrl } from "../../lib/assets";
  import { pmText, shortName, teamLabel } from "../../lib/details";
  import { pct0 } from "../../lib/format";
  import { isRankedTier } from "../../lib/meta";
  import { meta } from "../../lib/stores";
  import { ACS_TIP, ADR_TIP, PM_TIP, rankTip } from "../../lib/tips";
  import { tip } from "../../lib/tooltip.svelte";
  import type { MatchDetailsView } from "../../lib/types";
  import Name from "../Name.svelte";
  import ResultChip from "../ResultChip.svelte";
  import Skel from "../Skel.svelte";

  let { view }: { view: MatchDetailsView | null } = $props();

  const noTeams = $derived(!view || view.teams.length < 2);
  /** `{name}'s team`: the focus player's short name (undefined while a hidden agent's name waits for meta). */
  const focusShort = $derived.by(() => {
    const f = view?.teams.flatMap((t) => t.players).find((p) => p.isFocus);
    return f ? shortName(f.name, $meta) : "—";
  });

  const num = (n: number | null) => (n === null ? "—" : String(n));
  const int = (n: number | null) => (n === null ? "—" : String(Math.round(n)));
  const TONE = { win: "var(--win)", loss: "var(--loss)", flat: "var(--text2)" } as const;
</script>

<div class="board">
  <div class="shead">
    <span>Player</span><span class="r" use:tip={ACS_TIP}>ACS</span><span class="c">K / D / A</span><span class="r" use:tip={PM_TIP}>+/−</span><span
      class="r"
      use:tip={ADR_TIP}>ADR</span
    ><span class="r">HS%</span>
  </div>
  {#if view}
    {#each view.teams as t, i (`${i}:${t.teamId}`)}
      <div class="thead">
        {#if !noTeams && t.result}<ResultChip result={t.result} size={20} />{/if}
        <span class="trounds">{noTeams ? "—" : num(t.roundsWon)}</span>
        {#if i === 0 && !noTeams && view.perspective === "focus" && focusShort === undefined}
          <span class="tlabel"><Skel w={60} h={10} /></span>
        {:else}
          <span class="tlabel">{teamLabel(view, i, focusShort ?? "—")}</span>
        {/if}
      </div>
      {#each t.players as p (p.puuid)}
        {@const pm = pmText(p.kills, p.deaths)}
        <div class="srow">
          {#if p.isFocus && !p.isSelf}<div class="focusbar"></div>{/if}
          {#if p.isSelf}<div class="selfframe"></div>{/if}
          <div class="pcell">
            {#if p.agentId}<img class="sagent" src={assetUrl("agent", p.agentId)} alt="" />{:else}<div class="sagent"></div>{/if}
            {#if isRankedTier(p.tier)}
              <img class="srank" src={assetUrl("tier", p.tier)} alt="" use:tip={rankTip($meta, p.tier)} />
            {:else}
              <div class="srank"></div>
            {/if}
            <Name name={p.name} size="row" tipMode="none" />
          </div>
          <span class="acs">{int(p.acs)}</span>
          <div class="kda"><span class="r">{num(p.kills)}</span><span class="sl">/</span><span class="c">{num(p.deaths)}</span><span class="sl">/</span><span>{num(p.assists)}</span></div>
          <span class="pm" style:color={TONE[pm.tone]}>{pm.text}</span>
          <span class="adr">{int(p.adr)}</span>
          <span class="hs">{pct0(p.hsPct)}</span>
        </div>
      {/each}
    {/each}
  {:else}
    {#each [0, 1] as i (i)}
      <div class="thead" aria-hidden="true"></div>
      {#each [0, 1, 2, 3, 4] as j (j)}
        <div class="srow" aria-hidden="true">
          <div class="pcell"><Skel w={28} h={28} r={5} block /><Skel w={22} h={22} r="50%" block /><Skel w={120} h={13} block /></div>
          <!-- Each bar sits as its column's numbers do: right, except K / D / A, which is centred. -->
          {#each [0, 1, 2, 3, 4] as k (k)}<span class={["skn", k === 1 && "mid"]}><Skel w={30} h={13} block /></span>{/each}
        </div>
      {/each}
    {/each}
  {/if}
</div>

<style>
  .board {
    background: var(--card);
    border-radius: 8px;
    overflow: hidden;
  }

  .shead,
  .srow {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 52px 104px 48px 52px 48px;
    column-gap: 14px;
    align-items: center;
    padding: 0 18px 0 12px;
  }

  .shead {
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

  .thead {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 36px;
    padding: 0 12px;
    background: var(--team-bg);
  }

  .thead ~ .thead {
    border-top: 1px solid var(--line);
  }

  .trounds {
    font-size: 15px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }

  .tlabel {
    font-size: 12px;
    font-weight: 500;
    color: var(--text2);
  }

  .srow {
    position: relative;
    height: 42px;
    border-top: 1px solid var(--line);
  }

  .focusbar {
    position: absolute;
    left: 0;
    top: 0;
    bottom: 0;
    width: 3px;
    background: var(--accent);
  }

  .selfframe {
    position: absolute;
    inset: 0;
    border: 1px solid var(--self-frame);
    pointer-events: none;
  }

  .pcell {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }

  .sagent {
    display: block;
    width: 28px;
    height: 28px;
    flex: none;
    border-radius: 5px;
    background: var(--raised);
  }

  .srank {
    display: block;
    width: 22px;
    height: 22px;
    flex: none;
  }

  .acs,
  .pm,
  .adr,
  .hs {
    text-align: right;
    font-size: 14px;
    font-variant-numeric: tabular-nums;
  }

  .acs,
  .pm {
    font-weight: 600;
  }

  .adr,
  .hs {
    font-weight: 500;
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

  .skn {
    justify-self: end;
  }

  .skn.mid {
    justify-self: center;
  }
</style>
