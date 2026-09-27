<!-- Riot ID with a dimmer Tag, or the hidden-name forms (CLAUDE.md "Hidden names"). -->
<script lang="ts">
  import type { Snippet } from "svelte";
  import { agentName } from "../lib/meta";
  import { meta } from "../lib/stores";
  import { tip } from "../lib/tooltip.svelte";
  import type { DisplayName } from "../lib/types";

  interface Props {
    name: DisplayName;
    /** "inline": Name#TAG on one line. "stacked": the tag on its own line. */
    layout: "inline" | "stacked";
    /** Right-aligned extra on the last line (the encounter badge). */
    aside?: Snippet;
  }

  let { name, layout, aside }: Props = $props();

  const shown = $derived(name.kind === "visible" || name.kind === "remembered" ? name : null);
  const remembered = $derived(name.kind === "remembered");
  const hiddenLabel = $derived(
    name.kind === "hiddenAgent" ? (agentName($meta, name.agentId) ?? "Hidden Player") : "Hidden Player",
  );
</script>

{#snippet nameText()}
  {#if shown}
    <span
      class="name"
      class:remembered
      data-text
      {@attach tip(remembered && ["Remembered name", "Seen in a match you played"])}>{shown.name}</span
    >
  {:else}
    <span class="name hidden" data-text {@attach tip("Name hidden by the player")}>{hiddenLabel}</span>
  {/if}
{/snippet}

{#snippet tagText()}
  {#if shown}
    <span class="tag" class:remembered data-text>#{shown.tag}</span>
  {/if}
{/snippet}

{#if layout === "inline"}
  <div class="rid inline">
    {@render nameText()}
    {@render tagText()}
    {#if aside}<span class="aside">{@render aside()}</span>{/if}
  </div>
{:else}
  <div class="rid stacked">
    {@render nameText()}
    <div class="line2">
      {@render tagText()}
      {#if aside}<span class="aside">{@render aside()}</span>{/if}
    </div>
  </div>
{/if}

<style>
  .rid {
    min-width: 0;
  }

  .inline {
    display: flex;
    align-items: baseline;
    gap: 4px;
  }

  .stacked {
    display: grid;
    row-gap: 2px;
  }

  .line2 {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    min-height: var(--tag-lh, 18px);
  }

  .aside {
    display: flex;
    margin-left: auto;
    align-self: center;
  }

  .name,
  .tag {
    display: block;
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    line-height: var(--lh-tight);
  }

  .name {
    color: var(--text-1);
    font-size: var(--name-fs, var(--fs-lg));
    font-weight: var(--fw-strong);
  }

  .tag {
    flex: none;
    color: var(--text-3);
    font-size: var(--tag-fs, var(--fs-md));
  }

  .name.remembered {
    color: var(--accent);
  }

  .tag.remembered {
    color: var(--accent-2);
  }

  .name.hidden {
    color: var(--text-2);
    font-weight: var(--fw-body);
  }
</style>
