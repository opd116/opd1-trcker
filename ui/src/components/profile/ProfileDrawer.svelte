<!--
  TODO(profile agent): placeholder. The profile drawer (design v2 949–1131) replaces this file.
  App mounts it inside <main> (position: relative) while `router.profile` is set, keyed by nothing: `puuid` changes in
  place when another player is opened. The router already fetches the data: read `$openDetails` from lib/stores
  (only when `$openDetails?.card.puuid === puuid`). Close with `closeProfile()` (backdrop click, Escape); open a match
  with `openMatch(id, { from: "profile" | "encounter", focusPuuid })` (encFrom defaults to this drawer).
-->
<script lang="ts">
  import { closeProfile } from "../../lib/router.svelte";

  let { puuid }: { puuid: string } = $props();
</script>

<div class="drawer-stub" data-puuid={puuid}>
  <button type="button" class="scrim" aria-label="Close profile" onclick={closeProfile}></button>
</div>

<style>
  .drawer-stub {
    position: absolute;
    inset: 0;
    z-index: 20;
  }

  .scrim {
    position: absolute;
    inset: 0;
    border: 0;
    padding: 0;
    background: var(--scrim);
  }
</style>
