<!-- The title bar's pin: "Keep on top during matches" (Settings › Window and startup, the same setting). -->
<script lang="ts">
  import { settings, updateSettings } from "../../lib/appdata.svelte";
  import { tip } from "../../lib/tooltip.svelte";

  let { glance = false }: { glance?: boolean } = $props();

  const on = $derived($settings.onTop);
</script>

{#snippet icon()}
  <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"
    ><path d="M10 2l4 4-2.5 1L9 9.5 9.5 13 3 6.5 6.5 7 9 4.5z"></path><path d="M5.5 10.5L2 14"></path></svg
  >
{/snippet}

{#if glance}
  <!-- Glance always stays on top (Rust `set_glance`), whatever the setting: an indicator, as in the design, not a
       toggle that would show "off" while the window is on top. -->
  <span class={["pin", "glance", "on"]} role="img" aria-label="Pinned on top of other windows" use:tip={"Pinned on top of other windows"}
    >{@render icon()}</span
  >
{:else}
  <button
    type="button"
    class={["pin", on && "on"]}
    aria-label="Keep on top during matches"
    aria-pressed={on}
    use:tip={on ? "Keep on top during matches" : "Keep on top during matches · off"}
    onclick={() => updateSettings({ onTop: !on })}>{@render icon()}</button
  >
{/if}

<style>
  /* At the window's top edge: an inset ring, as on the window buttons. */
  .pin:focus-visible {
    outline-offset: -2px;
  }

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

  button.pin:hover {
    background: var(--chrome-hover);
  }

  /* The indicator: nothing to press. */
  .pin.glance {
    width: 34px;
    height: 32px;
    cursor: default;
  }
</style>
