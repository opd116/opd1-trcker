<!--
  One gallery tile (TrackerApp.dc.html 899–904): the art on the rail colour (click = open it in the editor), then a
  36px footer with the title, a violet dot for the user's own drawings ("Saved by you"), and Copy. The user's own
  drawings also get a Delete button (built from the footer's Copy button and the studio's red Clear button): the
  first click arms it ("Delete?", red), a second click within 2.4 s deletes.
-->
<script lang="ts">
  import { tileFontPx } from "../../lib/ascii";
  import { tip } from "../../lib/tooltip.svelte";

  let {
    title,
    raw,
    mine = false,
    copyState = null,
    onopen,
    oncopy,
    ondelete,
  }: {
    title: string;
    raw: string;
    mine?: boolean;
    /** null, or the last copy's outcome while its 2.4 s label shows. */
    copyState?: "ok" | "fail" | null;
    onopen: () => void;
    oncopy: () => void;
    ondelete?: () => void;
  } = $props();

  let armed = $state(false);
  let timer: ReturnType<typeof setTimeout> | undefined;

  function del() {
    if (!armed) {
      armed = true;
      clearTimeout(timer);
      timer = setTimeout(() => (armed = false), 2400);
      return;
    }
    clearTimeout(timer);
    armed = false;
    ondelete?.();
  }

  $effect(() => () => clearTimeout(timer));
</script>

<div class="tile">
  <button type="button" class="art" aria-label="Open {title} in the editor" use:tip={"Open in the editor"} onclick={onopen}
    ><pre style:font-size="{tileFontPx(raw)}px">{raw}</pre></button
  >
  <div class="foot">
    <span class="title" data-ellipsis>{title}</span>
    {#if mine}<span class="mine" use:tip={"Saved by you"}></span>{/if}
    {#if mine && ondelete}
      <button
        type="button"
        class="del"
        class:armed
        aria-label={armed ? `Confirm delete ${title}` : `Delete ${title}`}
        use:tip={armed ? "Click again to delete" : "Delete"}
        onclick={del}
        onblur={() => (armed = false)}
        >{#if armed}Delete?{:else}<svg width="12" height="13" viewBox="0 0 13 14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"
            ><path d="M1 3.5h11M4.5 3.5V1.8h4v1.7M2.5 3.5l.7 8.8h6.6l.7-8.8"></path></svg
          >{/if}</button
      >
    {/if}
    <button type="button" class="copy" class:ok={copyState === "ok"} aria-label="Copy {title}" onclick={oncopy}
      >{copyState === "ok" ? "Copied ✓" : copyState === "fail" ? "Failed" : "Copy"}</button
    >
  </div>
</div>

<style>
  .tile {
    min-width: 0;
    border-radius: 10px;
    background: var(--card);
    border: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .art {
    flex: 1;
    min-height: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--rail);
    border: none;
    border-bottom: 1px solid var(--line);
    cursor: pointer;
    overflow: hidden;
    padding: 0;
  }

  .art:hover {
    background: var(--drawer);
  }

  .art:focus-visible {
    outline-offset: -2px;
  }

  pre {
    margin: 0;
    font-family: "Cascadia Mono", Consolas, "DejaVu Sans Mono", Menlo, var(--font-mono);
    line-height: 1;
    color: var(--text-mid);
    white-space: pre;
  }

  .foot {
    flex: none;
    display: flex;
    align-items: center;
    gap: 8px;
    height: 36px;
    padding: 0 6px 0 12px;
  }

  .title {
    flex: 1;
    min-width: 0;
    font: 600 13px/1 var(--font);
    color: var(--text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .mine {
    flex: none;
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--accent);
  }

  .copy,
  .del {
    flex: none;
    height: 26px;
    padding: 0 9px;
    border-radius: 6px;
    background: transparent;
    border: 1px solid var(--line-control);
    font: 600 12px/1 var(--font);
    color: var(--accent-pale);
    cursor: pointer;
  }

  .copy:hover,
  .del:hover {
    background: var(--chip-hover);
  }

  .copy.ok {
    color: var(--win-text);
  }

  .del {
    width: 26px;
    padding: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-muted);
  }

  .del:hover {
    color: var(--error-text);
  }

  .del.armed {
    width: auto;
    padding: 0 9px;
    background: rgba(239, 106, 91, 0.12);
    border-color: rgba(239, 106, 91, 0.5);
    color: var(--error-text);
  }

  .del.armed:hover {
    background: rgba(239, 106, 91, 0.2);
    color: #ffd0ca;
  }
</style>
