<!-- The title bar's pin: "Keep on top during matches" (Settings › Window and startup, the same setting). -->
<script lang="ts">
  import { settings, updateSettings } from "../../lib/appdata.svelte";
  import { tip } from "../../lib/tooltip.svelte";

  let { glance = false }: { glance?: boolean } = $props();

  const on = $derived($settings.onTop);
  const label = $derived(glance ? "Pinned on top of other windows" : "Keep on top during matches");
</script>

<button
  type="button"
  class={["pin", glance && "glance", on && "on"]}
  aria-label="Keep on top during matches"
  aria-pressed={on}
  use:tip={on ? label : `${label} · off`}
  onclick={() => updateSettings({ onTop: !on })}
  ><svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"
    ><path d="M10 2l4 4-2.5 1L9 9.5 9.5 13 3 6.5 6.5 7 9 4.5z"></path><path d="M5.5 10.5L2 14"></path></svg
  ></button
>

<style>
  .pin {
    width: 40px;
    height: 36px;
    padding: 0;
    margin: 0;
    border: 0;
    background: transparent;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-dim);
    cursor: pointer;
  }

  .pin.on {
    color: var(--accent);
  }

  .pin:hover {
    background: var(--chrome-hover);
  }

  .pin.glance {
    width: 34px;
    height: 32px;
  }
</style>
