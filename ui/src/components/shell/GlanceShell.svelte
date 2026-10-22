<!--
  Glance mode (design v2 29–94): the window is a 420px column (`set_glance`); this is its frame: the backdrop, the
  column, and its 32px title bar (wordmark, GLANCE MODE, the status dot, the pin, Exit glance, close). The body (map
  header, the two team tables, the saved-player note) is components/live/GlanceView.svelte.
-->
<script lang="ts">
  import { setGlance } from "../../lib/router.svelte";
  import { tip } from "../../lib/tooltip.svelte";
  import GlanceView from "../live/GlanceView.svelte";
  import { close } from "./appWindow";
  import PinButton from "./PinButton.svelte";
  import StatusPill from "./StatusPill.svelte";
</script>

<div class="glance">
  <div class="col">
    <header class="bar" data-tauri-drag-region>
      <div class="logo"></div>
      <span class="word">OPD1</span>
      <span class="sub">GLANCE MODE</span>
      <div class="status"><StatusPill dotOnly /></div>
      <div class="fill" data-tauri-drag-region></div>
      <PinButton glance />
      <button type="button" class="exit" use:tip={"Back to the full window"} onclick={() => setGlance(false)}
        ><svg width="11" height="11" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" aria-hidden="true"
          ><path d="M9.5 2H14v4.5M14 2L9 7M6.5 14H2V9.5M2 14l5-5"></path></svg
        >Exit glance</button
      >
      <button type="button" class="wb close" aria-label="Close" onclick={close}
        ><svg width="10" height="10" viewBox="0 0 10 10" stroke="currentColor" stroke-width="1.1" aria-hidden="true"><path d="M1 1l8 8M9 1L1 9"></path></svg></button
      >
    </header>
    <GlanceView />
  </div>
</div>

<style>
  .glance {
    width: 100%;
    height: 100%;
    display: flex;
    justify-content: center;
    background: var(--bg-deep);
  }

  .col {
    width: 100%;
    max-width: 420px;
    min-height: 0;
    display: flex;
    flex-direction: column;
    background: var(--bg);
    border-left: 1px solid var(--line-chrome);
    border-right: 1px solid var(--line-chrome);
  }

  .bar {
    height: 32px;
    flex: none;
    display: flex;
    align-items: center;
    gap: 8px;
    padding-left: 12px;
    background: var(--chrome);
    border-bottom: 1px solid var(--line-chrome);
    user-select: none;
  }

  .logo,
  .word,
  .sub {
    pointer-events: none;
  }

  .logo {
    width: 11px;
    height: 11px;
    background: var(--accent);
    clip-path: polygon(0 0, 100% 0, 100% 64%, 64% 100%, 0 100%);
  }

  .word {
    font: 700 13px/1 var(--font-display);
    letter-spacing: 0.14em;
  }

  .sub {
    font: 500 10.5px/1 var(--font-mono);
    letter-spacing: 0.06em;
    color: var(--text-muted);
  }

  .status {
    margin-left: 6px;
    display: flex;
  }

  .fill {
    flex: 1;
    align-self: stretch;
  }

  .exit {
    height: 24px;
    padding: 0 9px;
    margin: 0 4px 0 0;
    border-radius: 6px;
    background: var(--button);
    border: 1px solid var(--line-control);
    color: var(--text);
    display: flex;
    align-items: center;
    gap: 6px;
    font: 600 12px/1 var(--font);
    cursor: pointer;
  }

  .exit:hover {
    background: var(--button-hover);
    border-color: var(--line-hover);
  }

  .wb {
    width: 40px;
    height: 32px;
    padding: 0;
    margin: 0;
    border: 0;
    background: transparent;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-dim);
  }

  .wb.close:hover {
    background: var(--danger);
    color: #ffffff;
  }
</style>
