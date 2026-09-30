<!-- Average Party Rank in the Lobby top bar (spec §2.3) [C], built from the card's label/value and rank parts. -->
<script lang="ts">
  import { assetUrl } from "../lib/assets";
  import { avgPartyRank } from "../lib/live";
  import { tierLabel } from "../lib/meta";
  import { meta } from "../lib/stores";
  import { aprTip } from "../lib/tips";
  import { tip } from "../lib/tooltip.svelte";
  import type { PlayerCard } from "../lib/types";
  import Skel from "./Skel.svelte";

  let { players }: { players: PlayerCard[] } = $props();

  const apr = $derived(avgPartyRank(players));
  const name = $derived(apr?.tier != null ? tierLabel($meta, apr.tier) : null);
</script>

{#if apr}
  <!-- No tip while loading: there is no count to explain yet. -->
  <div class="apr" use:tip={apr.loading ? "" : aprTip(apr.n)}>
    <span class="lbl">Avg party rank</span>
    <div class="val">
      {#if apr.loading}
        <Skel w={32} h={32} r="50%" block />
        <Skel w={52} h={12} block />
      {:else if apr.tier === null}
        <div class="ebox"></div>
        <span class="dash">—</span>
      {:else}
        <img src={assetUrl("tier", apr.tier)} alt="" />
        {#if name === undefined}<Skel w={52} h={12} block />{:else}<span class="tname">{name ?? "—"}</span>{/if}
      {/if}
    </div>
  </div>
{/if}

<style>
  .apr {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .lbl {
    font-size: 12px;
    font-weight: 500;
    color: var(--text2);
    white-space: nowrap;
  }

  .val {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  img,
  .ebox {
    display: block;
    width: 32px;
    height: 32px;
    flex: none;
  }

  .tname {
    font-size: 14px;
    font-weight: 600;
    white-space: nowrap;
  }

  .dash {
    font-size: 14px;
    font-weight: 600;
    color: var(--text2);
  }
</style>
