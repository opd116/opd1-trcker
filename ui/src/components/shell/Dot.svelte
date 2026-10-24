<!-- The design's status dot (`dot(c, pulse)`): 8px, with an expanding ring when it pulses. -->
<script lang="ts">
  let { color, pulse = false }: { color: string; pulse?: boolean } = $props();
</script>

<span class="dot" aria-hidden="true">
  {#if pulse}<span class="ring" style:background={color}></span>{/if}
  <span class="core" style:background={color}></span>
</span>

<style>
  .dot {
    position: relative;
    width: 8px;
    height: 8px;
    display: inline-block;
    flex: none;
  }

  .ring,
  .core {
    position: absolute;
    inset: 0;
    border-radius: 50%;
  }

  .ring {
    animation: opdPulse 1.8s ease-out infinite;
  }
</style>
