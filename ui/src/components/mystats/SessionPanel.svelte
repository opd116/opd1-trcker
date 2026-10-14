<!--
  The current session (spec §9.3): Net RR (big, signed, coloured), W / L, when it started, and every match of the
  session as a W / L / D chip, oldest → newest. A chip's tooltip is its map, RR change and age (`Ascent · +18 RR · 2h`);
  a click or Enter / Space opens that match's Match Details (`onopen`, like a Recent Matches row; its accessible name
  is the result word and the tip, since its text is only a letter; a chip without `onopen` is plain: no role, no
  pointer, no focus stop). The header's tooltip says what a session is; `Started` carries the date
  and time. `session: null` (no match in the last 6 hours) or an unknown page: the frame with `—`. `undefined`: the
  skeleton.
-->
<script lang="ts">
  import { rowKeydown } from "../../lib/details";
  import { DASH, span } from "../../lib/format";
  import { markResult, signedRr, signTone } from "../../lib/mystats";
  import { meta, now } from "../../lib/stores";
  import { SESSION_SCOPE, sessionChipLabel, sessionChipTip, whenTip } from "../../lib/tips";
  import { tip } from "../../lib/tooltip.svelte";
  import type { SessionSummary } from "../../lib/types";
  import ResultChip from "../ResultChip.svelte";
  import Skel from "../Skel.svelte";
  import Panel from "./Panel.svelte";

  interface Props {
    session: SessionSummary | null | undefined;
    onopen?: (matchId: string) => void;
  }

  let { session, onopen }: Props = $props();
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
          {#each session.matches as m (m.matchId)}
            {#if onopen}
              <div
                class="mc open"
                role="button"
                tabindex="0"
                aria-label={sessionChipLabel(m, $meta, $now)}
                use:tip={sessionChipTip(m, $meta, $now)}
                onclick={() => onopen(m.matchId)}
                onkeydown={rowKeydown(() => onopen(m.matchId))}
              >
                <ResultChip result={markResult[m.result]} size={20} />
              </div>
            {:else}
              <!-- Not clickable (no way to open it): the tip stays, the pointer and the focus stop do not. -->
              <div class="mc" use:tip={sessionChipTip(m, $meta, $now)}>
                <ResultChip result={markResult[m.result]} size={20} />
              </div>
            {/if}
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
    padding: 12px 18px;
  }

  .cell {
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 2px;
    padding: 0 22px;
    border-left: 1px solid var(--line);
    min-height: 48px;
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

  /* The chip's own 20×20 box is the trigger, so the tip centres on it and the ring (the global R106 one at its 2px
     outside offset, spec §9.3) hugs it: 3px outside, inside the 6px gap and the panel's 14px / 18px padding, never
     clipped by the panel's overflow. */
  .mc {
    flex: none;
    display: flex;
    width: 20px;
    height: 20px;
    border-radius: 4px;
  }

  .mc.open {
    cursor: pointer;
  }
</style>
