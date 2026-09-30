<!--
  A player's display name (spec §3.4): visible, remembered, hidden agent, Hidden Player, missing (—) or loading.
  Sizes: card 16px (§3.4), hero 22px (§6.2), row 14px (§6.7, §6.10). Only the name part of a Riot ID truncates; the
  tag never does. `wrap` (Top 3, §6.7): full names wrap and never truncate. A hidden player's name never reaches the
  DOM: the backend never sends it (R18, R96).
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
    /** Top 3 (spec §6.7): the block takes the row's free width and a long name wraps (anywhere) instead of truncating. */
    wrap?: boolean;
  }

  let { name, size, tipMode = "card", wrap = false }: Props = $props();

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
  // Loading: the skeleton's own name bar. The hero's is the Details skeleton's 160×18 (spec §6.9), so a hero whose
  // name is still absent looks like the skeleton hero; card and row keep the §3.4 120×13.
  const bar = $derived(size === "hero" ? { w: 160, h: 18 } : { w: 120, h: 13 });
  // Only a truncating name is an allowed ellipsis span (spec §8.3 check 5); a wrapping one never clips.
  const ellipsis = $derived(wrap ? undefined : true);
</script>

<div class={["nm", `size-${size}`, wrap && "wrap"]} use:tip={tipArg}>
  {#if name === undefined}
    <span class="line"><Skel w={bar.w} h={bar.h} /></span>
  {:else if name === null}
    <span class="dash">—</span>
  {:else if name.kind === "visible"}
    <span class="n" data-ellipsis={ellipsis}>{name.name}</span><span class="t">#{name.tag}</span>
  {:else if name.kind === "remembered"}
    <!-- `wrap`: the eye rides inside the name, so it stays on the name's first line when the name wraps. -->
    {#if !wrap}<EyeOff size={eye} />{/if}<span class={["n rem", wrap && "eyed"]} data-ellipsis={ellipsis}
      >{#if wrap}<span class="eye"><EyeOff size={eye} /></span>{/if}{name.name}</span
    ><span class="t">#{name.tag}</span>
  {:else}
    {#if !wrap || hidden === undefined}<EyeOff size={eye} />{/if}
    {#if hidden === undefined}
      <span class="line"><Skel w={120} h={13} /></span>
    {:else}
      <span class={["hid", wrap && "eyed"]}>{#if wrap}<span class="eye"><EyeOff size={eye} /></span>{/if}{hidden ?? "—"}</span>
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

  /* The design's row-size hidden name (scoreboard, v8:407) does not clip: an italic's last glyph overhangs its box,
     and a clip would shave it. An agent name or `Hidden Player` never needs the ellipsis at 14px. */
  .size-row .hid {
    overflow: visible;
  }

  /* Top 3 (spec §6.7, v8:321-323): the block fills the row beside the live icons and the count, and wraps. [C] A
     16-character name has no break point, so it may break anywhere rather than overflow under the icons. */
  .wrap {
    flex: 1;
    flex-wrap: wrap;
  }

  .wrap .n {
    white-space: normal;
    overflow: visible;
    text-overflow: clip;
    overflow-wrap: anywhere;
  }

  /* A wrapping name has no clip box for the underline to escape. [C] Its lines are 1.3 apart, not `normal`: the dotted
     underline hangs 4px under each line, and at `normal` it touches the cap tops of the line below (Archivo at 150%). */
  .wrap .rem {
    overflow: visible;
    line-height: 1.3;
  }

  /* Top 3 hidden variants (Task 18 review, Minor 4): the eye opens the name's first line and never sits alone above
     it. The first line is indented by the eye and the row's 5px gap, and the eye is laid in that indent, centred on
     the line as the flex row centres it. Out of the text flow it is no break opportunity (an inline icon is one, so
     a long name would break right after it) and takes no underline. */
  .wrap .hid {
    white-space: normal;
    overflow: visible;
    overflow-wrap: anywhere;
  }

  .eyed {
    position: relative;
    text-indent: 19px;
  }

  .eye {
    position: absolute;
    left: 0;
    top: 0;
    display: flex;
    align-items: center;
    height: 1lh;
  }
</style>
