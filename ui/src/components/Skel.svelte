<!-- Neutral loading shape (spec §5.4). Never contains text. Static, as in the design. -->
<script lang="ts">
  interface Props {
    /** Width in px. */
    w: number;
    /** Height in px. */
    h: number;
    /** Border radius: bars are 3px unless the spec says otherwise; "50%" for a circle. */
    r?: number | "50%";
    /** A block box (inside a flex/grid cell) instead of an inline-block bar on the text line. */
    block?: boolean;
  }

  let { w, h, r = 3, block = false }: Props = $props();
</script>

<span
  class={["sk", block && "block"]}
  aria-hidden="true"
  style:width="{w}px"
  style:height="{h}px"
  style:border-radius={r === "50%" ? r : `${r}px`}
></span>

<style>
  .sk {
    display: inline-block;
    flex: none;
    vertical-align: middle;
    background: var(--skel);
  }

  .block {
    display: block;
  }
</style>
