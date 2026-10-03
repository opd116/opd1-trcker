<!--
  The current session (spec §9.3): Net RR (big, signed, coloured), W / L, when it started, and every result of the
  session as W / L / D chips, oldest → newest. The header's tooltip says what a session is; `Started` carries the date
  and time. `session: null` (no match in the last 6 hours) or an unknown page: the frame with `—`. `undefined`: the
  skeleton.
-->
<script lang="ts">
  import { DASH, span } from "../../lib/format";
  import { signedRr, signTone } from "../../lib/mystats";
  import { now } from "../../lib/stores";
  import { SESSION_SCOPE, whenTip } from "../../lib/tips";
  import { tip } from "../../lib/tooltip.svelte";
  import type { Result, SessionSummary, StreakMark } from "../../lib/types";
  import ResultChip from "../ResultChip.svelte";
  import Skel from "../Skel.svelte";
  import Panel from "./Panel.svelte";

  let { session }: { session: SessionSummary | null | undefined } = $props();

  const RESULT: Record<StreakMark, Result> = { W: "win", L: "loss", D: "draw" };
</script>

<Panel title="Session" name="session" scope={SESSION_SCOPE}>
  {#if session === undefined}
    <div class="sess" aria-hidden="true">
      <div class="cell first"><Skel w={44} h={12} block /><Skel w={72} h={28} block /></div>
      <div class="cell"><Skel w={44} h={12} block /><Skel w={72} h={22} block /></div>
      <div class="cell"><Skel w={44} h={12} block /><Skel w={72} h={22} block /></div>
      <div class="cell streak"><Skel w={200} h={20} block /></div>
    </div>
  {:else if session === null}
    <div class="sess">
      <div class="cell first"><span class="lab">Net RR</span><span class="big dim">{DASH}</span></div>
      <div class="cell"><span class="lab">W / L</span><span class="val dim">{DASH}</span></div>
      <div class="cell"><span class="lab">Started</span><span class="val dim">{DASH}</span></div>
    </div>
  {:else}
    <div class="sess">
      <div class="cell first" use:tip={SESSION_SCOPE}>
        <span class="lab">Net RR</span>
        <span class={["big", signTone(session.netRr)]}>{signedRr(session.netRr)}</span>
      </div>
      <div class="cell" use:tip={SESSION_SCOPE}>
        <span class="lab">W / L</span>
        <span class="val"><span class="w">{session.wins}</span><span class="sep">{" / "}</span><span class="l">{session.losses}</span></span>
      </div>
      <div class="cell" use:tip={whenTip(session.startedMs)}>
        <span class="lab">Started</span>
        <span class="val"><span class="num">{span($now - session.startedMs)}</span> <span class="unit">ago</span></span>
      </div>
      <div class="cell streak">
        <div class="chips">
          {#each session.streak as m, i (i)}
            <ResultChip result={RESULT[m]} size={20} />
          {/each}
        </div>
      </div>
    </div>
  {/if}
</Panel>

<style>
  .sess {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 12px 0;
    padding: 14px 18px;
  }

  .cell {
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 2px;
    padding: 0 22px;
    border-left: 1px solid var(--line);
    min-height: 56px;
  }

  .cell.first {
    padding-left: 0;
    border-left: 0;
  }

  .cell.streak {
    flex: 1;
    min-width: 260px;
  }

  .lab {
    font-size: 12px;
    line-height: 14px;
    font-weight: 500;
    color: var(--text2);
    white-space: nowrap;
  }

  .big {
    font-size: 28px;
    line-height: 32px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  .val {
    font-size: 22px;
    line-height: 27px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  .big.win {
    color: var(--win);
  }

  .big.loss {
    color: var(--loss);
  }

  .dim {
    color: var(--text2);
  }

  .big.flat {
    color: var(--text);
  }

  .w {
    color: var(--win);
  }

  .l {
    color: var(--loss);
  }

  .sep {
    color: var(--text3);
    font-weight: 500;
  }

  .unit {
    font-size: 14px;
    font-weight: 500;
    color: var(--text2);
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
</style>
