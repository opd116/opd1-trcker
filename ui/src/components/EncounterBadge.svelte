<!-- ⟲ N: past matches shared with the user. Hover: "N with · M against". -->
<script lang="ts">
  import { encounterTip } from "../lib/cardTips";
  import { tip } from "../lib/tooltip.svelte";
  import type { Encounters } from "../lib/types";

  let { encounters }: { encounters: Encounters } = $props();

  const total = $derived(encounters.with + encounters.against);
</script>

<span
  class="enc"
  data-stat
  {@attach tip(encounterTip(encounters))}
>
  <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true">
    <path
      d="M3.2 6.1A5 5 0 1 1 3 9.4"
      fill="none"
      stroke="currentColor"
      stroke-width="1.6"
      stroke-linecap="round"
    />
    <path d="M1.6 3.1 3.1 6.4 6.4 4.9" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" />
  </svg>
  <span class="n num" data-text>{total}</span>
</span>

<style>
  .enc {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    padding: 0 6px 0 5px;
    border: 1px solid var(--line-strong);
    border-radius: 999px;
    color: var(--text-2);
    line-height: 1;
  }

  svg {
    flex: none;
  }

  .n {
    font-size: var(--fs-sm);
    font-weight: var(--fw-strong);
    line-height: 16px;
  }
</style>
