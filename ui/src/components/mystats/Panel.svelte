<!--
  A My Stats section (spec §9.1): the Player Details section frame (§6.3, v8:244) with the scope of its figures as the
  header's tooltip, since scope is never printed (CLAUDE.md). `grow` lets the panel fill its grid cell, so two sections
  side by side end on one line.
-->
<script lang="ts">
  import type { Snippet } from "svelte";
  import { tip } from "../../lib/tooltip.svelte";

  interface Props {
    title: string;
    /** The section's own class (rank, rrchart, session, last5, maps, agents, weekday, daypart, insights). */
    name: string;
    /** The header's tooltip: what the section covers. Empty = none. */
    scope?: string;
    grow?: boolean;
    children: Snippet;
  }

  let { title, name, scope = "", grow = false, children }: Props = $props();
</script>

<section class={["sec", name, grow && "grow"]}>
  <div class="sech"><span use:tip={scope}>{title}</span></div>
  <div class="panel">{@render children()}</div>
</section>

<style>
  .sec {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }

  .sech {
    display: flex;
    font-size: 14px;
    font-weight: 600;
  }

  .panel {
    background: var(--card);
    border-radius: 8px;
    overflow: hidden;
  }

  .grow .panel {
    flex: 1;
    display: flex;
    flex-direction: column;
  }
</style>
