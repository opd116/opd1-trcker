<!-- The automated smurf signal (prototype 1011–1020): the reasons from `smurfSignal`, the sensitivity, and the dismiss. -->
<script lang="ts">
  import type { SmurfLevel } from "../../lib/types";

  let { reasons, level, ondismiss }: { reasons: string[]; level: SmurfLevel; ondismiss: () => void } = $props();

  const LEVEL: Record<SmurfLevel, string> = { off: "Off", cautious: "Cautious", eager: "Eager" };
</script>

<section class="sp" aria-label="Automated smurf signal">
  <div class="top">
    <span class="lbl">AUTOMATED SMURF SIGNAL · NOT CONFIRMED</span>
    <span class="gap"></span>
    <span class="basis">Sensitivity: {LEVEL[level]} (Settings)</span>
  </div>
  <ul class="rs">
    {#each reasons as r (r)}
      <li><span class="dia"></span><span class="rt">{r}</span></li>
    {/each}
  </ul>
  <div class="bot">
    <span class="fine">Patterns, not proof. A returning player on a new account can look the same.</span>
    <button type="button" onclick={ondismiss}>Not a smurf · hide flag</button>
  </div>
</section>

<style>
  .sp {
    flex: none;
    border: 1px dashed color-mix(in srgb, var(--warn) 50%, transparent);
    background: color-mix(in srgb, var(--warn) 5%, transparent);
    border-radius: 10px;
    padding: 14px 16px;
    display: flex;
    flex-direction: column;
    gap: 11px;
    /* The panel's warm text tones (#eadccf, #cdbba8), mixed from the palette. */
    --warm-1: color-mix(in srgb, var(--text) 82%, var(--warn-text));
    --warm-2: color-mix(in srgb, var(--text-3) 78%, var(--warn-text));
  }

  .top {
    display: flex;
    align-items: baseline;
    gap: 10px;
  }

  .gap {
    flex: 1;
  }

  .lbl {
    font: 600 11px/1 var(--font-mono);
    letter-spacing: 0.06em;
    color: var(--warn-text);
  }

  .basis {
    font: 500 12.5px/1 var(--font);
    color: var(--warm-2);
  }

  .rs {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    gap: 8px 22px;
  }

  li {
    display: flex;
    gap: 10px;
    align-items: baseline;
  }

  .dia {
    width: 5px;
    height: 5px;
    flex: none;
    background: var(--warn);
    transform: rotate(45deg) translateY(-2px);
  }

  .rt {
    font: 500 13.5px/1.4 var(--font);
    color: var(--warm-1);
  }

  .bot {
    display: flex;
    align-items: center;
    gap: 12px;
    padding-top: 2px;
  }

  .fine {
    flex: 1;
    font: 500 12.5px/1.45 var(--font);
    color: var(--warm-2);
    text-wrap: pretty;
  }

  button {
    height: 28px;
    padding: 0 10px;
    border-radius: 6px;
    background: transparent;
    color: var(--lost-text);
    border: 1px solid color-mix(in srgb, var(--warn) 45%, transparent);
    font: 600 12px/1 var(--font);
    cursor: pointer;
    flex: none;
  }

  button:hover {
    background: color-mix(in srgb, var(--warn) 12%, transparent);
  }
</style>
