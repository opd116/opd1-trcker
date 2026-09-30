<!--
  Detected party (spec §3.9): a right-edge stripe (dashed = likely, solid = confirmed) and a tab with the party
  glyph. One colour per group across the screen: --party-{group % 4}. Visibility: `showParty` (live.ts).
-->
<script lang="ts">
  import { partyTip } from "../lib/tips";
  import { tip } from "../lib/tooltip.svelte";
  import type { PartyMark } from "../lib/types";
  import PartyGlyph from "./icons/PartyGlyph.svelte";

  let { party }: { party: PartyMark } = $props();

  const col = $derived(`var(--party-${party.group % 4})`);
  const kind = $derived(party.confirmed ? "confirmed" : "likely");
  const text = $derived(partyTip(party));
</script>

<div class="stripe" use:tip={text}><div class={["sline", kind]} style:--pc={col}></div></div>
<div class={["ptab", kind]} style:--pc={col} use:tip={text}><PartyGlyph /></div>

<style>
  .stripe {
    position: absolute;
    right: 0;
    top: 0;
    bottom: 0;
    width: 10px;
    z-index: 2;
  }

  .sline {
    position: absolute;
    right: 0;
    top: 0;
    bottom: 0;
    width: 3px;
  }

  .sline.confirmed {
    background: var(--pc);
  }

  .sline.likely {
    background: repeating-linear-gradient(180deg, var(--pc) 0 6px, transparent 6px 10px);
  }

  .ptab {
    position: absolute;
    right: 0;
    top: 22px;
    z-index: 3;
    width: 21px;
    height: 20px;
    box-sizing: border-box;
    padding-right: 1px;
    border-radius: 5px 0 0 5px;
    display: flex;
    align-items: center;
    justify-content: center;
    box-shadow: inset 0 0 0 1.5px var(--pc);
  }

  .ptab.confirmed {
    background: linear-gradient(var(--pc), var(--pc)), var(--card);
    color: var(--on-solid);
  }

  .ptab.likely {
    background: linear-gradient(transparent, transparent), var(--card);
    color: var(--pc);
  }
</style>
