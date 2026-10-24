<!--
  A Settings section card (TrackerApp.dc.html 926–927): condensed caps title over a one-line description, then its
  rows (SettingRow), separated by hairlines.
-->
<script lang="ts">
  import type { Snippet } from "svelte";

  let { title, desc, children }: { title: string; desc?: string; children: Snippet } = $props();
</script>

<section class="group" aria-label={title}>
  <div class="head">
    <span class="title">{title}</span>
    {#if desc}<span class="desc">{desc}</span>{/if}
  </div>
  <div class="rows">{@render children()}</div>
</section>

<style>
  .group {
    background: var(--card);
    border: 1px solid var(--line);
    border-radius: 12px;
    padding: 4px 20px;
  }

  .head {
    padding: 16px 0 6px;
    display: flex;
    flex-direction: column;
    gap: 7px;
  }

  .title {
    font: 700 15px/1 var(--font-display);
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: var(--text);
  }

  .desc {
    font: 500 13px/1.4 var(--font);
    color: var(--text-muted);
  }

  .rows > :global(:not(:last-child)) {
    border-bottom: 1px solid var(--line);
  }
</style>
