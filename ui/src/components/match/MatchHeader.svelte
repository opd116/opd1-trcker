<!--
  Match Details header (spec §6.10, v8:365-378): the map, `{mode} · {date} · {time} · {duration}` (the date has no year
  for a current-year match opened from an encounter row, R140), then team 0's result
  chip, the score and the halves (tooltip `First half · Second half`). A no-team mode (Deathmatch) has no chip, `—` for
  the score and no halves. `view: null` = the loading header (a 150×56 box and a 120×18 bar). A view with no team at
  all (`emptyMatch`, the call failed twice) is the settled empty header: the map box, `—` for the map, the date line
  and the score, nothing invented.
-->
<script lang="ts">
  import { assetUrl } from "../../lib/assets";
  import { retryImg } from "../../lib/imgRetry";
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
    {#if view.mapId}<img {@attach retryImg} class="mmap" src={assetUrl("mapList", view.mapId)} alt="" />{:else}<div class="mmap"></div>{/if}
    <div class="mtext">
      <span class="mtitle">{#if map === undefined}<Skel w={120} h={18} />{:else}{map ?? "—"}{/if}</span>
      <span class="mwhen">{view.teams.length === 0 ? "—" : whenText(view, from, $now)}</span>
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
    gap: 20px;
    background: var(--card);
    border: 1px solid var(--line);
    border-radius: 10px;
    padding: 16px 24px 16px 16px;
    margin-bottom: 16px;
  }

  /* An unknown map keeps the box, with the image's own (no) background. */
  .mmap {
    display: block;
    width: 150px;
    height: 72px;
    flex: none;
    object-fit: cover;
    border-radius: 5px;
  }

  .mtext {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 7px;
  }

  .mtitle {
    font-size: 24px;
    font-weight: 600;
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
    gap: 18px;
    padding-left: 24px;
    border-left: 1px solid var(--line);
  }

  .mscol {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 5px;
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
