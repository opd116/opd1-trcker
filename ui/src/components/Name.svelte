<!--
  A player's display name (spec §3.4): visible, remembered, hidden agent, Hidden Player, missing (—) or loading.
  Sizes: card 16px (§3.4), hero 22px (§6.2), row 14px (§6.10). Only the name part of a Riot ID truncates; the tag
  never does. A hidden player's name never reaches the DOM: the backend never sends it (R18, R96).
-->
<script lang="ts">
  import { agentName } from "../lib/meta";
  import { meta } from "../lib/stores";
  import { nameTip } from "../lib/tips";
  import { tip, type TipArg } from "../lib/tooltip.svelte";
  import type { DisplayName } from "../lib/types";
  import EyeOff from "./icons/EyeOff.svelte";
  import Skel from "./Skel.svelte";

  interface Props {
    name: DisplayName | null | undefined;
    size: "card" | "hero" | "row";
    /** `card`: the §4.2 name tooltips (visible: only when truncated). `none`: no tooltip (Top 3, scoreboard). */
    tipMode?: "card" | "none";
  }

  let { name, size, tipMode = "card" }: Props = $props();

  /** The hidden-style text: `Hidden Player`, or the agent name (undefined while meta is pending, spec §5.8). */
  const hidden = $derived(
    name?.kind === "hiddenPlayer" ? "Hidden Player" : name?.kind === "hiddenAgent" ? agentName($meta, name.agentId) : null,
  );

  const tipArg: TipArg = $derived.by(() => {
    if (tipMode === "none" || !name) return "";
    const text = nameTip(name, $meta);
    return name.kind === "visible" ? { text, overflowOnly: true } : text;
  });

  const eye = $derived(size === "hero" ? 16 : 14);
</script>

<div class={["nm", `size-${size}`]} use:tip={tipArg}>
  {#if name === undefined}
    <span class="line"><Skel w={120} h={13} /></span>
  {:else if name === null}
    <span class="dash">—</span>
  {:else if name.kind === "visible"}
    <span class="n" data-ellipsis>{name.name}</span><span class="t">#{name.tag}</span>
  {:else if name.kind === "remembered"}
    <EyeOff size={eye} /><span class="n rem" data-ellipsis>{name.name}</span><span class="t">#{name.tag}</span>
  {:else}
    <EyeOff size={eye} />
    {#if hidden === undefined}
      <span class="line"><Skel w={120} h={13} /></span>
    {:else}
      <span class="hid">{hidden ?? "—"}</span>
    {/if}
  {/if}
</div>

<style>
  .nm {
    min-width: 0;
    display: flex;
    align-items: baseline;
    gap: 5px;
  }

  .size-card {
    flex: 1;
  }

  .size-hero {
    gap: 6px;
  }

  .n {
    min-width: 0;
    font-size: 16px;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /*
    The dotted underline hangs 2–4px below the name's box (font and size dependent), and the name clips for its
    ellipsis, so the clip edge moves out 6px: `overflow:clip` + `overflow-clip-margin` only widens what is painted.
    It adds no layout or scrollable overflow (spec §8.3 check 2 stays clean) and the ellipsis still applies. The
    design's own mock clips the underline away by accident; CLAUDE.md "Card indicators" makes it part of the
    remembered style. Covered by fit-live's "remembered name" pixel probe.
  */
  .rem {
    text-decoration: underline dotted var(--text2);
    text-underline-offset: 4px;
    overflow: clip;
    overflow-clip-margin: 6px;
  }

  .t {
    flex: none;
    font-size: 13px;
    font-weight: 500;
    color: var(--text2);
    white-space: nowrap;
  }

  .hid {
    min-width: 0;
    font-size: 16px;
    font-weight: 500;
    font-style: italic;
    color: var(--text-soft);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* Missing (null): the name slot's own size, in --text2. */
  .dash {
    font-size: 16px;
    font-weight: 600;
    color: var(--text2);
  }

  /* Loading: an inline bar on a name-sized line, so the line box stays. */
  .line {
    font-size: 16px;
  }

  .size-hero .n,
  .size-hero .hid,
  .size-hero .dash,
  .size-hero .line {
    font-size: 22px;
  }

  .size-hero .rem {
    text-underline-offset: 5px;
  }

  .size-hero .t {
    font-size: 15px;
  }

  .size-row .n,
  .size-row .hid,
  .size-row .dash,
  .size-row .line {
    font-size: 14px;
  }
</style>
