<!--
  The design's switch (TrackerApp.dc.html 930, `tog` in extraVals4): "On" / "Off" then a 42 × 24 track with a 16px
  knob. Disabled = half opacity, not-allowed cursor, a muted state word, and `tip` saying why.
-->
<script lang="ts">
  import { tip as tipAction } from "../../lib/tooltip.svelte";

  let {
    on,
    onchange,
    labelledby,
    label,
    disabled = false,
    tip = "",
  }: {
    on: boolean;
    onchange: (on: boolean) => void;
    /** The id of the visible label (preferred), or `label` for an aria-label. */
    labelledby?: string;
    label?: string;
    disabled?: boolean;
    tip?: string;
  } = $props();
</script>

<span class="state" class:on class:disabled aria-hidden="true">{on ? "On" : "Off"}</span>
<button
  type="button"
  role="switch"
  class="track"
  class:on
  aria-checked={on}
  aria-labelledby={labelledby}
  aria-label={labelledby ? undefined : label}
  aria-disabled={disabled || undefined}
  use:tipAction={tip}
  onclick={() => {
    if (!disabled) onchange(!on);
  }}
>
  <span class="knob"></span>
</button>

<style>
  .state {
    font: 600 12.5px/1 var(--font);
    color: var(--text-muted);
    min-width: 26px;
    text-align: right;
  }

  .state.on {
    color: var(--accent-pale);
  }

  .state.disabled {
    color: var(--text-tag);
  }

  .track {
    position: relative;
    width: 42px;
    height: 24px;
    flex: none;
    border-radius: 12px;
    background: var(--skel);
    border: 1px solid var(--line-strong);
    cursor: pointer;
    padding: 0;
  }

  .track.on {
    background: var(--accent);
    border-color: var(--accent);
  }

  .track[aria-disabled="true"] {
    cursor: not-allowed;
    opacity: 0.5;
  }

  .knob {
    position: absolute;
    top: 3px;
    left: 3px;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--text-muted);
    transition: left 0.15s ease;
  }

  .on .knob {
    left: 21px;
    background: var(--bg);
  }

  @media (prefers-reduced-motion: reduce) {
    .knob {
      transition: none;
    }
  }
</style>
