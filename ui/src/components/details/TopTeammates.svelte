<!--
  Top 3 Teammates (spec §6.7, v8:317-335): full names that wrap, never truncate; the live agent and rank for a teammate
  whose card on the live screen shows their name (never for a hidden live card); the shared match count. Not clickable.
  The backend applies the hidden-name rules (R18, R93). `mates: null` = the Details skeleton (§6.9: 3 rows of a bar);
  `[]` = one row with —.
-->
<script lang="ts">
  import { assetUrl } from "../../lib/assets";
  import { liveTeammate } from "../../lib/details";
  import { plural } from "../../lib/format";
  import { meta } from "../../lib/stores";
  import { liveTip } from "../../lib/tips";
  import { tip } from "../../lib/tooltip.svelte";
  import type { LiveState, TeammateSummary } from "../../lib/types";
  import Name from "../Name.svelte";
  import Skel from "../Skel.svelte";
  import Section from "./Section.svelte";

  let { mates, live }: { mates: TeammateSummary[] | null; live: LiveState | null } = $props();
</script>

<Section title="Top 3 Teammates" name="mates">
  {#if mates === null}
    {#each [0, 1, 2] as i (i)}
      <div class="mate" aria-hidden="true"><Skel w={120} h={13} block /></div>
    {/each}
  {:else if mates.length === 0}
    <div class="mate"><span class="nodash">—</span></div>
  {:else}
    {#each mates as m (m.puuid)}
      {@const lv = liveTeammate(m.puuid, live)}
      <div class="mate">
        <Name name={m.name} size="row" tipMode="none" wrap />
        {#if lv}
          <div class="mlive" use:tip={liveTip($meta, lv.agentId)}>
            {#if lv.agentId}<img class="lagent" src={assetUrl("agent", lv.agentId)} alt="" />{:else}<div class="lagent"></div>{/if}
            {#if lv.tier !== null}<img class="ltier" src={assetUrl("tier", lv.tier)} alt="" />{:else}<div class="ltier"></div>{/if}
          </div>
        {/if}
        <!-- `14 games`, `1 game` (spec §4.3 `count`), the number in its own span. -->
        <span class="mcount"><span class="v">{m.count}</span>{plural(m.count, "game")}</span>
      </div>
    {/each}
  {/if}
</Section>

<style>
  /* The design's row is `min-height:46px; padding:0 14px` (v8:321). [C] The 46px is kept as 30px + 8px above and
     below, so a wrapped 2–3-line name keeps clear of the row lines (Task 18 review, Minor 5) while a one-line row
     (at most 30px of content: the 26px agent icon) is exactly the design's 46px. */
  .mate {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 30px;
    padding: 8px 14px;
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
