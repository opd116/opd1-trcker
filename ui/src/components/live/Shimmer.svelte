<!-- The v2 skeleton block (design `skel(w, h, r)`, TrackerApp.dc.html 1255): a --skel bar with a moving sheen. No text. -->
<script lang="ts">
  let { w, h, r = 3 }: { w: number; h: number; r?: number } = $props();
</script>

<span class="sk" aria-hidden="true" style:width="{w}px" style:height="{h}px" style:border-radius="{r}px"><span class="sheen"></span></span>

<style>
  .sk {
    position: relative;
    display: block;
    flex: none;
    overflow: hidden;
    background: var(--skel);
  }

  .sheen {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 40%;
    background: linear-gradient(90deg, transparent, rgba(255, 255, 255, 0.05), transparent);
    animation: opdShimmer 1.4s ease-in-out infinite;
  }
</style>
