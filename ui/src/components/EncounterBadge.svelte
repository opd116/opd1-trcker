<!-- ⟲ + count of past matches shared with the user (spec §3.5; hero §6.2). Visibility: `showEncounters` (live.ts). -->
<script lang="ts">
  import { encounterTip } from "../lib/tips";
  import { tip } from "../lib/tooltip.svelte";
  import type { EncounterCount } from "../lib/types";
  import Rewind from "./icons/Rewind.svelte";

  let { enc, size }: { enc: EncounterCount; size: "card" | "hero" } = $props();
</script>

<span class={["enc", `size-${size}`]} use:tip={encounterTip(enc)}><Rewind size={size === "hero" ? 13 : 12} />{enc.with + enc.against}</span>

<style>
  .enc {
    display: flex;
    align-items: center;
    gap: 3px;
    font-size: 13px;
    font-weight: 600;
    color: var(--text2);
    font-variant-numeric: tabular-nums;
  }

  /* [C] 3+ digit counts only: the card's slot is min-width 36px and right-aligned, so this margin is absorbed
     for 1–2 digits (the design's geometry) and keeps a 5px gap (the name–tag gap) once the count widens the
     slot, so the tag never touches the icon. */
  .size-card {
    margin-left: 5px;
  }

  .size-hero {
    font-size: 14px;
    color: inherit;
  }
</style>
