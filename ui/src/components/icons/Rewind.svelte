<!-- ⟲ encounters (spec §3.5): Lucide `rotate-ccw` geometry (ISC, see THIRD-PARTY-NOTICES.md). 12px card, 13px hero. -->
<script lang="ts">
  let { size = 12 }: { size?: number } = $props();
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="2.4"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"><path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" /><path d="M3 3v5h5" /></svg
>

<style>
  svg {
    flex: none;
    display: block;
  }
</style>
