<!-- Neutral loading shape. Never contains text. -->
<script lang="ts">
  interface Props {
    /** CSS width, e.g. "3em" or "36px". */
    w: string;
    /** CSS height. */
    h: string;
    /** CSS border radius. */
    r?: string;
  }

  let { w, h, r = "var(--r-xs)" }: Props = $props();
</script>

<span class="sk" aria-hidden="true" style:width={w} style:height={h} style:border-radius={r}></span>

<style>
  .sk {
    display: inline-block;
    flex: none;
    vertical-align: middle;
    background: var(--skeleton);
    animation: sk-pulse 1.8s ease-in-out infinite;
  }

  @keyframes sk-pulse {
    50% {
      opacity: 0.5;
    }
  }
</style>
