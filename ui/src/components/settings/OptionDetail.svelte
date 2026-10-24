<!--
  The table under a segmented setting that explains each option (TrackerApp.dc.html 935–939): a dot + the option's
  name in a 92px column, its explanation beside it; the chosen option's row is tinted and its dot violet. Plain
  text, so screen readers read it too: it is the only place the options are explained.
-->
<script lang="ts">
  let { rows, selected }: { rows: readonly { name: string; desc: string }[]; selected: string } = $props();
</script>

<div class="table">
  {#each rows as r (r.name)}
    <div class="r" class:on={r.name === selected}>
      <span class="name"><span class="dot" aria-hidden="true"></span>{r.name}</span>
      <span class="desc">{r.desc}</span>
    </div>
  {/each}
</div>

<style>
  .table {
    display: flex;
    flex-direction: column;
    border: 1px solid var(--line-card);
    border-radius: 8px;
    overflow: hidden;
  }

  .r {
    display: grid;
    grid-template-columns: 92px minmax(0, 1fr);
    column-gap: 12px;
    padding: 10px 12px;
    background: transparent;
  }

  .r:not(:last-child) {
    border-bottom: 1px solid var(--line);
  }

  .r.on {
    background: rgba(157, 140, 255, 0.09);
  }

  .name {
    display: flex;
    align-items: center;
    gap: 7px;
    font: 600 13px/1.45 var(--font);
    color: var(--text-3);
  }

  .on .name {
    color: var(--text);
  }

  .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--line-strong);
    flex: none;
  }

  .on .dot {
    background: var(--accent);
  }

  .desc {
    font: 500 13px/1.45 var(--font);
    color: var(--text-muted);
    text-wrap: pretty;
  }

  .on .desc {
    color: var(--text-soft);
  }
</style>
