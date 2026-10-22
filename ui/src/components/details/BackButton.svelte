<!-- LEGACY v8 Back (spec §6.1): a scoreboard goes back (router `backFromMatch`), the old Details page closes. -->
<script lang="ts">
  import { backFromMatch, closeProfile, router } from "../../lib/router.svelte";

  const back = () => (router.matchId !== null ? backFromMatch() : closeProfile());
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
    margin-bottom: 12px;
  }

  /* The design's box exactly (it is border-box there too); the rest resets the button's own UA look. */
  .back {
    display: flex;
    align-items: center;
    gap: 7px;
    height: 32px;
    padding: 0 13px 0 10px;
    box-sizing: border-box;
    margin: 0;
    border: 1px solid var(--line2);
    border-radius: 5px;
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
