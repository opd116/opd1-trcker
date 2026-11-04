<!--
  The custom title bar (window `decorations: false`): 36px, the wordmark and window buttons. Always-on-top and its pin
  were removed by the user's 2026-11-04 decision. The bar is the drag region; Tauri's drag script
  also turns a double-click there into maximize / restore. The wordmark reads TRCKER: the product name is spelled that
  way on purpose (the prototype's "TRACKER" is the one thing not copied).
-->
<script lang="ts">
  import { close, minimize, toggleMaximize, watchMaximized } from "./appWindow";

  let maximized = $state(false);
  $effect(() => watchMaximized((m) => (maximized = m)));
</script>

<header class="bar" data-tauri-drag-region>
  <div class="brand" data-tauri-drag-region>
    <div class="logo"></div>
    <span class="word">OPD1</span>
    <span class="sub">TRCKER</span>
  </div>
  <div class="fill" data-tauri-drag-region></div>
  <button type="button" class="wb" aria-label="Minimize" onclick={minimize}
    ><svg width="10" height="10" viewBox="0 0 10 10" stroke="currentColor" stroke-width="1.1" aria-hidden="true"><path d="M0 5h10"></path></svg></button
  >
  <button type="button" class="wb" aria-label={maximized ? "Restore" : "Maximize"} onclick={toggleMaximize}
    >{#if maximized}<svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1.1" aria-hidden="true"
        ><rect x=".5" y="2.5" width="7" height="7"></rect><path d="M2.5 2.5V.5h7v7h-2"></path></svg
      >{:else}<svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1.1" aria-hidden="true"
        ><rect x=".5" y=".5" width="9" height="9"></rect></svg
      >{/if}</button
  >
  <button type="button" class="wb close" aria-label="Close" onclick={close}
    ><svg width="10" height="10" viewBox="0 0 10 10" stroke="currentColor" stroke-width="1.1" aria-hidden="true"><path d="M0 0l10 10M10 0L0 10"></path></svg></button
  >
</header>

<style>
  .bar {
    height: 36px;
    flex: none;
    display: flex;
    align-items: center;
    background: var(--chrome);
    border-bottom: 1px solid var(--line-chrome);
    user-select: none;
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 18px 0 16px;
    height: 100%;
  }

  /* Children pass the mouse to the drag region. */
  .brand > * {
    pointer-events: none;
  }

  .logo {
    width: 13px;
    height: 13px;
    background: var(--accent);
    clip-path: polygon(0 0, 100% 0, 100% 64%, 64% 100%, 0 100%);
  }

  .word {
    font: 700 14px/1 var(--font-display);
    letter-spacing: 0.09em;
  }

  .sub {
    font: 600 14px/1 var(--font);
    letter-spacing: 0.09em;
    color: var(--text-muted);
  }

  .fill {
    flex: 1;
    align-self: stretch;
  }

  .wb {
    width: 46px;
    height: 36px;
    padding: 0;
    margin: 0;
    border: 0;
    background: transparent;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-dim);
    cursor: default;
  }

  /* The buttons touch the window's top (Close also its right) edge: an inset ring, or the window cuts its sides. */
  .wb:focus-visible {
    outline-offset: -2px;
  }

  .wb:hover {
    background: var(--chrome-hover);
    color: var(--text);
  }

  .wb.close:hover {
    background: var(--danger);
    color: #ffffff;
  }

  .wb svg {
    display: block;
  }
</style>
