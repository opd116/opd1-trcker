<!--
  One Settings row (TrackerApp.dc.html 928–939): label + description on the left, the control in a 250px column on
  the right, and an optional full-width detail under both (OptionDetail). A disabled row's text is at half opacity.
  `id` is the label's id: the control names itself with `aria-labelledby={id}`.
-->
<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    id,
    label,
    desc,
    disabled = false,
    control,
    detail,
  }: { id: string; label: string; desc: string; disabled?: boolean; control: Snippet; detail?: Snippet } = $props();
</script>

<div class="row">
  <div class="text" class:disabled>
    <span class="label" {id}>{label}</span>
    <span class="desc">{desc}</span>
  </div>
  <div class="control">{@render control()}</div>
  {#if detail}<div class="detail">{@render detail()}</div>{/if}
</div>

<style>
  .row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 250px;
    column-gap: 24px;
    align-items: center;
    padding: 14px 0;
  }

  .text {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
  }

  .text.disabled {
    opacity: 0.5;
  }

  .label {
    font: 600 14.5px/1.2 var(--font);
    color: var(--text);
  }

  .desc {
    font: 500 13px/1.45 var(--font);
    color: var(--text-3);
    text-wrap: pretty;
  }

  .control {
    justify-self: end;
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .detail {
    grid-column: 1 / -1;
    margin-top: 12px;
  }
</style>
