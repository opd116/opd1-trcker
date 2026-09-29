<!-- Neutral loading shape (spec §5.4). Never contains text. Static, as in the design. -->
<script lang="ts">
  interface Props {
    /** CSS width, e.g. "120px". */
    w: string;
    /** CSS height. */
    h: string;
    /** CSS border radius; bars are 3px unless the spec says otherwise. */
    r?: string;
  }

  let { w, h, r = "3px" }: Props = $props();
</script>

<span class="sk" aria-hidden="true" style:width={w} style:height={h} style:border-radius={r}></span>

<style>
  .sk {
    display: inline-block;
    flex: none;
    vertical-align: middle;
    background: var(--skel);
  }
</style>
