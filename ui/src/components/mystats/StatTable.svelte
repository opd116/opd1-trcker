<!--
  The Maps and Agents tables (spec §9.4): one row per map or agent, most played first (the backend's order), with Games,
  W–L, Win%, Net RR, K/D and ACS, numbers right-aligned in tabular figures. An agent row carries its head icon. Win%
  and Net RR take the win / loss colours (above / below 50%, gains / losses); a Win% cell's tooltip is the structured
  W·L·games tip. Names come from meta (R95): a pending meta draws a bar, a missing id `—`.
  `rows: undefined` = the skeleton (3 rows); `[]` or null = one `—` row.
-->
<script lang="ts">
  import { assetUrl } from "../../lib/assets";
  import { DASH, kd2, pct0, score } from "../../lib/format";
  import { agentName, mapName } from "../../lib/meta";
  import { rateTone, signedRr, signTone } from "../../lib/mystats";
  import { meta } from "../../lib/stores";
  import { ACS_TIP, ACT_SCOPE, winCellTip } from "../../lib/tips";
  import { tip } from "../../lib/tooltip.svelte";
  import type { StatRow } from "../../lib/types";
  import Skel from "../Skel.svelte";
  import Panel from "./Panel.svelte";

  interface Props {
    kind: "map" | "agent";
    rows: StatRow[] | null | undefined;
  }

  let { kind, rows }: Props = $props();

  const title = $derived(kind === "map" ? "Maps" : "Agents");
  const nameOf = (id: string) => (kind === "map" ? mapName($meta, id) : agentName($meta, id));
</script>

<Panel {title} name={kind === "map" ? "maps" : "agents"} scope={ACT_SCOPE}>
  <div class="thead">
    <span>{kind === "map" ? "Map" : "Agent"}</span><span class="r">Games</span><span class="r">W–L</span><span class="r">Win%</span><span
      class="r">Net RR</span
    ><span class="r">K/D</span><span class="r"><span use:tip={ACS_TIP}>ACS</span></span>
  </div>
  {#if rows === undefined}
    {#each [0, 1, 2] as i (i)}
      <div class="trow sk-row" aria-hidden="true"><Skel w={110} h={13} block /></div>
    {/each}
  {:else if !rows || rows.length === 0}
    <div class="trow none"><span class="nodash">{DASH}</span></div>
  {:else}
    {#each rows as r (r.id)}
      {@const name = nameOf(r.id)}
      <div class="trow">
        <div class="tname">
          {#if kind === "agent"}
            {#if r.id}<img class="ticon" src={assetUrl("agent", r.id)} alt="" />{:else}<div class="ticon"></div>{/if}
          {/if}
          <span class="n">{#if name === undefined}<Skel w={60} h={10} />{:else}{name ?? DASH}{/if}</span>
        </div>
        <span class="r num">{r.games}</span>
        <span class="r num">{score(r.wins, r.losses)}</span>
        <span class={["r num strong", rateTone(r.winrate)]} use:tip={winCellTip(r.wins, r.losses, r.games)}>{pct0(r.winrate)}</span>
        <span class={["r num strong", signTone(r.netRr)]}>{signedRr(r.netRr)}</span>
        <span class="r num strong">{kd2(r.kd)}</span>
        <span class="r num">{r.acs === null ? DASH : Math.round(r.acs)}</span>
      </div>
    {/each}
  {/if}
</Panel>

<style>
  .thead,
  .trow {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 72px 84px 72px 88px 64px 64px;
    column-gap: 12px;
    align-items: center;
    padding: 0 18px 0 18px;
  }

  .thead {
    height: 30px;
    font-size: 12px;
    font-weight: 500;
    color: var(--text2);
    border-bottom: 1px solid var(--line);
    white-space: nowrap;
  }

  .trow {
    height: 44px;
  }

  .trow + .trow {
    border-top: 1px solid var(--line);
  }

  .r {
    text-align: right;
  }

  .nodash {
    font-size: 14px;
    font-weight: 500;
    color: var(--text2);
  }

  .tname {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }

  .ticon {
    display: block;
    width: 28px;
    height: 28px;
    flex: none;
    border-radius: 5px;
    background: var(--raised);
  }

  .n {
    font-size: 14px;
    font-weight: 600;
    white-space: nowrap;
  }

  .num {
    font-size: 14px;
    font-weight: 500;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  .strong {
    font-weight: 600;
  }

  .win {
    color: var(--win);
  }

  .loss {
    color: var(--loss);
  }
</style>
