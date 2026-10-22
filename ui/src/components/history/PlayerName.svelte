<!--
  A player's name in the Saved table and the scoreboard (design v2 482, 637–638), with the hidden-name rules
  (CLAUDE.md Card indicators): visible `Name #TAG` (the full Riot ID on hover only when the name is cut); a remembered
  name with the eye, a dotted underline and its source on hover; a hidden player as the italic agent name or
  `Hidden Player` with the eye (tooltip `Hidden name`). Only the name part truncates; the tag never does.
  `size`: `saved` 15 / 12.5px, `board` 14.5 / 12px.
-->
<script lang="ts">
  import { agentName } from "../../lib/meta";
  import { meta } from "../../lib/stores";
  import { nameTip } from "../../lib/tips";
  import { tip, type TipArg } from "../../lib/tooltip.svelte";
  import type { DisplayName } from "../../lib/types";
  import EyeOff from "../icons/EyeOff.svelte";

  let { name, size }: { name: DisplayName; size: "saved" | "board" } = $props();

  const hidden = $derived(
    name.kind === "hiddenPlayer" ? "Hidden Player" : name.kind === "hiddenAgent" ? (agentName($meta, name.agentId) ?? "Hidden Player") : null,
  );

  const tipArg: TipArg = $derived.by(() => {
    const text = nameTip(name, $meta);
    if (name.kind === "visible") return { text, overflowOnly: true };
    if (name.kind === "remembered") return { text, ifCut: `${name.name}#${name.tag}` };
    return text;
  });
</script>

<span class={["pn", `s-${size}`]} use:tip={tipArg}>
  {#if name.kind === "visible"}
    <span class="n" dir="auto" data-ellipsis>{name.name}</span><span class="t">#{name.tag}</span>
  {:else if name.kind === "remembered"}
    <i class="eye"><EyeOff size={13} /></i><span class="n rem" dir="auto" data-ellipsis>{name.name}</span><span class="t">#{name.tag}</span>
  {:else}
    <i class="eye"><EyeOff size={13} /></i><span class="n hid" data-ellipsis>{hidden}</span>
  {/if}
</span>

<style>
  .pn {
    display: flex;
    align-items: center;
    gap: 5px;
    min-width: 0;
  }

  .n {
    min-width: 0;
    font-weight: 600;
    line-height: 1.1;
    color: var(--text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .s-saved .n {
    font-size: 15px;
  }

  .s-board .n {
    font-size: 14.5px;
  }

  .t {
    flex: none;
    font-weight: 500;
    line-height: 1;
    white-space: nowrap;
  }

  .s-saved .t {
    font-size: 12.5px;
    color: var(--text-tag);
  }

  .s-board .t {
    font-size: 12px;
    color: var(--text-label);
  }

  .eye {
    font-style: normal;
    flex: none;
    display: flex;
    color: var(--text-muted);
  }

  /* The dotted underline hangs below the clip box: widen what is painted, not the layout (as Name.svelte does). */
  .rem {
    text-decoration: underline dotted var(--text-muted);
    text-underline-offset: 4px;
    overflow: clip;
    overflow-clip-margin: 6px;
  }

  .hid {
    font-weight: 500;
    font-style: italic;
    color: var(--text-2);
    padding-right: 2px;
  }
</style>
