<!--
  Match Details header (spec §6.10, v8:365-378): the map, `{mode} · {date} · {time} · {duration}` (the date has no year
  for a current-year match opened from an encounter row, R140), then team 0's result
  chip, the score and the halves (tooltip `First half · Second half`). A no-team mode (Deathmatch) has no chip, `—` for
  the score and no halves. `view: null` = the loading header (a 150×56 box and a 120×18 bar).
-->
<script lang="ts">
  import { assetUrl } from "../../lib/assets";
  import { halvesText, whenText, type MatchFrom } from "../../lib/details";
  import { score } from "../../lib/format";
  import { mapName } from "../../lib/meta";
  import { meta, now } from "../../lib/stores";
  import { HALVES_TIP } from "../../lib/tips";
  import { tip } from "../../lib/tooltip.svelte";
  import type { MatchDetailsView } from "../../lib/types";
  import ResultChip from "../ResultChip.svelte";
  import Skel from "../Skel.svelte";

  // `from` (the route's) picks the date form of the second line (R140); the loading header has no such line.
  let { view, from = "recent" }: { view: MatchDetailsView | null; from?: MatchFrom } = $props();

  const teams = $derived(view && view.teams.length >= 2 ? { a: view.teams[0]!, b: view.teams[1]! } : null);
  const map = $derived(view ? mapName($meta, view.mapId) : null);
  const halves = $derived(view ? halvesText(view) : null);
</script>

{#if view}
  <div class="mhead">
    {#if view.mapId}<img class="mmap" src={assetUrl("mapList", view.mapId)} alt="" />{:else}<div class="mmap"></div>{/if}
    <div class="mtext">
      <span class={["mtitle", map === null && "dash"]}>{#if map === undefined}<Skel w={120} h={18} />{:else}{map ?? "—"}{/if}</span>
      <span class="mwhen">{whenText(view, from, $now)}</span>
    </div>
    <div class="mres">
      {#if teams?.a.result}<ResultChip result={teams.a.result} size={28} />{/if}
      <div class="mscol">
        <span class="mscore">{teams ? score(teams.a.roundsWon, teams.b.roundsWon) : "—"}</span>
        {#if halves}<span class="mhalves" use:tip={HALVES_TIP}>{halves}</span>{/if}
      </div>
    </div>
  </div>
{:else}
  <div class="mhead" aria-hidden="true">
    <Skel w={150} h={56} r={5} block />
    <div class="mtext"><Skel w={120} h={18} block /></div>
  </div>
{/if}

<style>
  .mhead {
    display: flex;
    align-items: center;
    gap: 16px;
    background: var(--card);
    border-radius: 8px;
    padding: 12px 22px 12px 12px;
    margin-bottom: 14px;
  }

  /* An unknown map keeps the box, with the image's own (no) background. */
  .mmap {
    display: block;
    width: 150px;
    height: 56px;
    flex: none;
    object-fit: cover;
    border-radius: 5px;
  }

  .mtext {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .mtitle {
    font-size: 20px;
    font-weight: 600;
  }

  .mtitle.dash {
    color: var(--text2);
  }

  .mwhen {
    font-size: 12px;
    font-weight: 500;
    color: var(--text2);
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }

  .mres {
    display: flex;
    align-items: center;
    gap: 14px;
  }

  .mscol {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
  }

  .mscore {
    font-size: 28px;
    line-height: 32px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }

  .mhalves {
    font-size: 12px;
    font-weight: 500;
    color: var(--text2);
    font-variant-numeric: tabular-nums;
  }
</style>
