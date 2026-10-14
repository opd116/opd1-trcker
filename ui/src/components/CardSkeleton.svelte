<!-- Full skeleton card (spec §5.4, v8:143-160): used when there is no card object yet. No text, not clickable. -->
<script lang="ts">
  import Skel from "./Skel.svelte";
</script>

<div class="skcard" aria-hidden="true">
  <div class="identity">
    <Skel w={28} h={28} r={6} block />
    <Skel w={140} h={13} block />
  </div>
  <div class="ranking">
  <div class="row">
    <Skel w={40} h={40} r="50%" block />
    <div class="stack"><Skel w={84} h={13} block /><Skel w={40} h={10} block /></div>
  </div>
  <div class="stack"><Skel w={160} h={10} block /><Skel w={176} h={10} block /></div>
  </div>
  <div class="performance">
    <div class="trio"><Skel w={54} h={18} block /><Skel w={54} h={18} block /><Skel w={54} h={18} block /></div>
    <div class="l5"><Skel w={130} h={22} r={4} block /></div>
  </div>
</div>

<style>
  .skcard {
    min-height: 0;
    background: linear-gradient(90deg, var(--raised) 0 38px, var(--card) 38px);
    border-radius: 6px;
    overflow: hidden;
    display: grid;
    grid-template-rows: 28px minmax(0, 1fr) 32px;
    row-gap: 6px;
    padding: 8px 14px 8px 48px;
    box-sizing: border-box;
    cursor: default;
  }

  .row, .identity, .ranking, .performance {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .stack {
    display: flex;
    flex-direction: column;
    gap: 7px;
  }

  .ranking { gap: 36px; }
  .performance { border-top: 1px solid var(--line); padding-top: 6px; justify-content: space-between; }

  .trio {
    display: flex;
    gap: 20px;
  }

</style>
