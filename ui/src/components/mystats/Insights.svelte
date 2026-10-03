<!--
  Auto insights (spec §9.5): one row per insight the backend sends, in its order: a dot (green for good news, a
  neutral grey for a weak spot: a note, not an alarm), the finished English text, and the size of the sample behind it as the tooltip (`From 16 matches`).
  Nothing to say (an insight needs enough games) or nothing known: one `—` row. `insights: undefined` = the skeleton.
-->
<script lang="ts">
  import { DASH } from "../../lib/format";
  import { minusSigns } from "../../lib/mystats";
  import { insightTip } from "../../lib/tips";
  import { tip } from "../../lib/tooltip.svelte";
  import type { Insight } from "../../lib/types";
  import Skel from "../Skel.svelte";
  import Panel from "./Panel.svelte";

  let { insights }: { insights: Insight[] | null | undefined } = $props();
</script>

<Panel title="Insights" name="insights">
  {#if insights === undefined}
    {#each [0, 1, 2] as i (i)}
      <div class="irow" aria-hidden="true"><Skel w={8} h={8} r="50%" block /><Skel w={320} h={13} block /></div>
    {/each}
  {:else if !insights || insights.length === 0}
    <div class="irow none"><span class="nodash">{DASH}</span></div>
  {:else}
    {#each insights as i (i.kind)}
      <div class="irow" use:tip={insightTip(i.matches)}>
        <span class={["dot", i.positive ? "good" : "weak"]}></span>
        <span class="text">{minusSigns(i.text)}</span>
      </div>
    {/each}
  {/if}
</Panel>

<style>
  .irow {
    display: flex;
    align-items: flex-start;
    gap: 12px;
    min-height: 44px;
    box-sizing: border-box;
    padding: 12px 18px;
  }

  .irow + .irow {
    border-top: 1px solid var(--line);
  }

  .irow.none {
    align-items: center;
    min-height: 52px;
  }

  .nodash {
    font-size: 14px;
    font-weight: 500;
    color: var(--text2);
  }

  .dot {
    flex: none;
    width: 8px;
    height: 8px;
    margin-top: 6px;
    border-radius: 50%;
  }

  .dot.good {
    background: var(--win);
  }

  .dot.weak {
    background: var(--text2);
  }

  .text {
    min-width: 0;
    font-size: 14px;
    line-height: 20px;
    font-weight: 500;
  }
</style>
