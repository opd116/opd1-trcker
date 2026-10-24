<!--
  The design's segmented control, a radiogroup in two looks:
    "pill" — Settings (TrackerApp.dc.html 931): content-width options, the checked one raised (#2a2d35 + a #4a4e56 edge).
    "fill" — the ASCII studio's Effect / Tool pickers (870, 878): options share the width, the checked one #22252b.
  One tab stop (the checked option); arrow keys / Home / End move the choice (radio.ts). `item` draws an option's
  content (an icon) instead of its label; the label is then its accessible name.
-->
<script lang="ts" generics="T extends string">
  import type { Snippet } from "svelte";
  import { tip } from "../../lib/tooltip.svelte";
  import { radioKeydown, radioTabIndex } from "./radio";

  type Option = { value: T; label: string; tip?: string };

  let {
    options,
    value,
    onchange,
    variant = "pill",
    labelledby,
    label,
    item,
    class: cls = "",
  }: {
    options: readonly Option[];
    value: T;
    onchange: (v: T) => void;
    variant?: "pill" | "fill";
    labelledby?: string;
    label?: string;
    item?: Snippet<[Option]>;
    class?: string;
  } = $props();

  const sel = $derived(options.findIndex((o) => o.value === value));
</script>

<div
  role="radiogroup"
  class="seg {variant} {cls}"
  aria-labelledby={labelledby}
  aria-label={labelledby ? undefined : label}
  tabindex="-1"
  onkeydown={radioKeydown}
>
  {#each options as o, i (o.value)}
    <button
      type="button"
      role="radio"
      class="opt"
      aria-checked={o.value === value}
      aria-label={item ? o.label : undefined}
      tabindex={radioTabIndex(i, sel)}
      use:tip={o.tip ?? (item ? o.label : "")}
      onclick={() => onchange(o.value)}
    >
      {#if item}{@render item(o)}{:else}{o.label}{/if}
    </button>
  {/each}
</div>

<style>
  .seg {
    display: flex;
    padding: 3px;
    border-radius: 8px;
    background: var(--input);
  }

  .seg:focus {
    outline: none;
  }

  .opt {
    height: 28px;
    border-radius: 6px;
    display: flex;
    align-items: center;
    justify-content: center;
    font: 600 13px/1 var(--font);
    cursor: pointer;
    white-space: nowrap;
  }

  .opt:hover {
    color: var(--text);
  }

  /* Settings */
  .pill {
    gap: 2px;
    border: 1px solid var(--line-control);
  }

  .pill .opt {
    padding: 0 12px;
    color: var(--text-3);
    background: transparent;
    border: 1px solid transparent;
  }

  .pill .opt[aria-checked="true"] {
    color: var(--text);
    background: #2a2d35;
    border-color: var(--line-strong);
  }

  /* ASCII studio */
  .fill {
    border: 1px solid var(--track);
  }

  .fill .opt {
    flex: 1;
    padding: 0 10px;
    color: var(--text-muted);
    background: transparent;
    border: none;
  }

  .fill .opt[aria-checked="true"] {
    color: var(--text);
    background: var(--button-hover);
  }
</style>
