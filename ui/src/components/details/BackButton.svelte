<!-- Back (spec §6.1, v8:164-168): Details → Live, Match → Details with its Show more kept (router `back`). -->
<script lang="ts">
  import { back } from "../../lib/router.svelte";
</script>

<div class="brow">
  <button type="button" class="back" onclick={back}
    ><svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.6" aria-hidden="true"
      ><path d="M7.5 2.5L4 6l3.5 3.5" /></svg
    >Back</button
  >
</div>

<style>
  .brow {
    display: flex;
    margin-bottom: 14px;
  }

  /* The design's box exactly (it is border-box there too); the rest resets the button's own UA look. */
  .back {
    display: flex;
    align-items: center;
    gap: 7px;
    height: 34px;
    padding: 0 13px 0 10px;
    box-sizing: border-box;
    margin: 0;
    border: 1px solid var(--line2);
    border-radius: 7px;
    background: var(--card);
    font: inherit;
    font-size: 13px;
    font-weight: 500;
    color: inherit;
    cursor: pointer;
  }

  .back:hover {
    background: var(--raised);
  }
</style>
