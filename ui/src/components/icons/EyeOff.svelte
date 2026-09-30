<!-- Hidden-name eye (spec §3.4, v8:62). 14px on cards and rows, 16px in the hero. -->
<script lang="ts">
  let { size = 14 }: { size?: number } = $props();
</script>

<svg width={size} height={size} viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true"
  ><path d="M1.5 8s2.4-4.5 6.5-4.5S14.5 8 14.5 8 12.1 12.5 8 12.5 1.5 8 1.5 8z" /><circle cx="8" cy="8" r="2" /><path
    d="M2.5 13.5l11-11"
  /></svg
>

<style>
  svg {
    flex: none;
    align-self: center;
    color: var(--text2);
  }
</style>
