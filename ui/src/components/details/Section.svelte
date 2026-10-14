<!--
  A Player Details section (spec §6.3, v8:244, 277, 317, 336): the 14px/600 header, 8px, then the card-coloured panel
  with the section's rows. `name` is the section's own class (recent, encs, mates, loadout); `after` is what follows
  the panel inside the section (Recent's Show more).
-->
<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    title: string;
    name: string;
    children: Snippet;
    after?: Snippet;
  }

  let { title, name, children, after }: Props = $props();
</script>

<section class={["sec", name]}>
  <div class="sech">{title}</div>
  <div class="panel">{@render children()}</div>
  {@render after?.()}
</section>

<style>
  .sec {
    display: flex;
    flex-direction: column;
    gap: 0;
    background: var(--card);
    border: 1px solid var(--line);
    border-radius: 9px;
    overflow: hidden;
  }

  .sech {
    padding: 13px 14px;
    font-size: 14px;
    font-weight: 600;
    border-bottom: 1px solid var(--line);
  }

  .panel {
    background: var(--card);
    overflow: hidden;
  }
</style>
