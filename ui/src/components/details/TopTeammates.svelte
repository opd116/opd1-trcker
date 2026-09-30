<!--
  Top 3 Teammates (spec §6.7, v8:317-335): full names that wrap, never truncate; the live agent and rank for a teammate
  whose card on the live screen shows their name (never for a hidden live card); the shared match count. Not clickable.
  The backend applies the hidden-name rules (R18, R93). `mates: null` = the Details skeleton (§6.9: 3 rows of a bar);
  `[]` = one row with —.
-->
<script lang="ts">
  import { assetUrl } from "../../lib/assets";
  import { liveTeammate } from "../../lib/details";
  import { count } from "../../lib/format";
  import { meta } from "../../lib/stores";
  import { liveTip } from "../../lib/tips";
  import { tip } from "../../lib/tooltip.svelte";
  import type { LiveState, TeammateSummary } from "../../lib/types";
  import Name from "../Name.svelte";
  import Skel from "../Skel.svelte";

  let { mates, live }: { mates: TeammateSummary[] | null; live: LiveState | null } = $props();

  /** `14 games` → the number (its own span) and the word; `1 game` (spec §4.3 `count`). */
  const parts = (n: number) => {
    const [num, word] = count(n, "game").split(" ");
    return { num: num!, word: word! };
  };
</script>

<section class="sec mates">
  <div class="sech">Top 3 Teammates</div>
  <div class="panel">
    {#if mates === null}
      {#each [0, 1, 2] as i (i)}
        <div class="mate" aria-hidden="true"><Skel w={120} h={13} block /></div>
      {/each}
    {:else if mates.length === 0}
      <div class="mate"><span class="nodash">—</span></div>
    {:else}
      {#each mates as m (m.puuid)}
        {@const lv = liveTeammate(m.puuid, live)}
        {@const c = parts(m.count)}
        <div class="mate">
          <Name name={m.name} size="row" tipMode="none" wrap />
          {#if lv}
            <div class="mlive" use:tip={liveTip($meta, lv.agentId)}>
              {#if lv.agentId}<img class="lagent" src={assetUrl("agent", lv.agentId)} alt="" />{:else}<div class="lagent"></div>{/if}
              {#if lv.tier !== null}<img class="ltier" src={assetUrl("tier", lv.tier)} alt="" />{:else}<div class="ltier"></div>{/if}
            </div>
          {/if}
          <span class="mcount"><span class="v">{c.num}</span>{c.word}</span>
        </div>
      {/each}
    {/if}
  </div>
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

  .mate {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 46px;
    padding: 0 14px;
  }

  .mate + .mate {
    border-top: 1px solid var(--line);
  }

  .nodash {
    font-size: 14px;
    font-weight: 500;
    color: var(--text2);
  }

  .mlive {
    flex: none;
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .lagent {
    display: block;
    width: 26px;
    height: 26px;
    border-radius: 5px;
    background: var(--raised);
  }

  .ltier {
    display: block;
    width: 22px;
    height: 22px;
  }

  .mcount {
    flex: none;
    width: 62px;
    text-align: right;
    font-size: 12px;
    font-weight: 500;
    color: var(--text2);
    white-space: nowrap;
  }

  .mcount .v {
    font-size: 15px;
    font-weight: 600;
    color: var(--text);
    font-variant-numeric: tabular-nums;
    margin-right: 4px;
  }
</style>
