<!-- Show more (spec §6.5, v8:272-274): reveals up to 8 recent rows and goes away. No "show less", no count picker. -->
<script lang="ts">
  // LEGACY v8: "Show more" lived on the old router; the v2 profile drawer owns its own list state.
  let { onmore = () => {} }: { onmore?: () => void } = $props();
</script>

<button type="button" class="more" onclick={onmore}>Show more</button>

<style>
  /* The design's box: 34px tall plus a 1px border (content-box; a button defaults to border-box), the section's full
     width (a flex item of the section column, stretched). */
  .more {
    display: flex;
    align-items: center;
    justify-content: center;
    box-sizing: content-box;
    height: 34px;
    margin: 0;
    padding: 0;
    border: 1px solid var(--line2);
    border-radius: 6px;
    background: transparent;
    font: inherit;
    font-size: 13px;
    font-weight: 500;
    color: var(--text-soft);
    cursor: pointer;
  }

  .more:hover {
    background: var(--card);
  }
</style>
