<!-- Full skeleton card (spec §5.4, v8:143-160): used when there is no card object yet. No text, not clickable. -->
<script lang="ts">
  import Skel from "./Skel.svelte";
</script>

<div class="skcard" aria-hidden="true">
  <div class="row">
    <Skel w={40} h={40} r={6} block />
    <div class="stack"><Skel w={120} h={13} block /><Skel w={48} h={10} block /></div>
  </div>
  <div class="row">
    <Skel w={40} h={40} r="50%" block />
    <div class="stack"><Skel w={84} h={13} block /><Skel w={40} h={10} block /></div>
  </div>
  <div class="line"><Skel w={176} h={10} block /></div>
  <div class="line"><Skel w={124} h={10} block /></div>
  <div class="trio"><Skel w={44} h={30} block /><Skel w={44} h={30} block /><Skel w={44} h={30} block /></div>
  <div class="l5"><Skel w={130} h={22} r={4} block /></div>
</div>

<style>
  .skcard {
    min-height: 0;
    background: linear-gradient(90deg, var(--raised) 0 44px, var(--card) 44px);
    border-radius: 6px;
    overflow: hidden;
    display: grid;
    grid-template-columns: minmax(0, 1fr) 158px;
    grid-template-rows: 40px 18px 1fr;
    column-gap: 16px;
    row-gap: 10px;
    padding: 12px 14px 12px 56px;
    box-sizing: border-box;
    cursor: default;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .stack {
    display: flex;
    flex-direction: column;
    gap: 7px;
  }

  .line {
    display: flex;
    align-items: center;
  }

  .trio {
    align-self: end;
    display: flex;
    gap: 28px;
  }

  .l5 {
    align-self: end;
  }
</style>
