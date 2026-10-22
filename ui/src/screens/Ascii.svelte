<!--
  ASCII studio (design v2, TrackerApp.dc.html 862–917; logic 1849–2010, model in lib/ascii.ts): a 440px editor
  (controls 300px | canvas) over the gallery. The user's saved drawings live in the DB (`ascii_list` / `ascii_save` /
  `ascii_delete`) and come first in the gallery, then the built-in presets. Copy goes through the clipboard; every
  outcome is said in the polite status line and on the button for 2.4 s.
-->
<script lang="ts">
  import AsciiCanvas from "../components/ascii/AsciiCanvas.svelte";
  import CharGrid from "../components/ascii/CharGrid.svelte";
  import GalleryTile from "../components/ascii/GalleryTile.svelte";
  import Segmented from "../components/settings/Segmented.svelte";
  import * as api from "../lib/api";
  import {
    artSize,
    BRUSHES,
    composeCanvas,
    EFFECTS,
    type Effect,
    FILL_CHARS,
    filterText,
    INITIAL,
    loadRaw,
    MAX_TEXT,
    type PaintMap,
    paintAt,
    PRESETS,
    type Tool,
  } from "../lib/ascii";
  import { scrollHost } from "../lib/router.svelte";
  import type { AsciiArt } from "../lib/types";
  import { reportError } from "../lib/uilog";

  const FLASH_MS = 2400;
  const TOOLS: readonly { value: Tool; label: string }[] = [
    { value: "draw", label: "Draw" },
    { value: "erase", label: "Erase" },
  ];

  let text = $state(INITIAL.text);
  let char = $state(INITIAL.char);
  let fx = $state<Effect>(INITIAL.effect);
  let tool = $state<Tool>(INITIAL.tool);
  let brush = $state(INITIAL.brush);
  let paint = $state<PaintMap>({});
  let saved = $state<AsciiArt[]>([]);

  /**
   * The last action while its 2.4 s feedback shows: `key` is "gen" (Copy), "saved", "saveFail" or a tile's key; `msg`
   * is the status line, fixed when it happened (the art may change while it shows).
   */
  let flash = $state<{ key: string; ok: boolean; msg: string } | null>(null);
  let flashTimer: ReturnType<typeof setTimeout> | undefined;

  const canvas = $derived(composeCanvas(text, char, fx, paint));
  const noPaint = $derived(Object.keys(paint).length === 0);

  const CLIPBOARD_FAIL = "Couldn’t reach the clipboard.";

  function show(key: string, ok: boolean, msg: string) {
    flash = { key, ok, msg };
    clearTimeout(flashTimer);
    flashTimer = setTimeout(() => (flash = null), FLASH_MS);
  }

  $effect(() => () => clearTimeout(flashTimer));

  $effect(() => {
    api
      .asciiList()
      .then((list) => (saved = list))
      .catch((err) => reportError("ascii_list", err));
  });

  /** The design's copy: the async clipboard first, then a hidden textarea + execCommand. */
  function fallbackCopy(value: string): boolean {
    try {
      const ta = document.createElement("textarea");
      ta.value = value;
      ta.style.cssText = "position:fixed;left:-9999px;opacity:0";
      document.body.appendChild(ta);
      ta.select();
      const ok = document.execCommand("copy");
      ta.remove();
      return ok;
    } catch {
      return false;
    }
  }

  /** Copy `value`; `okMsg` is the status line when it worked. */
  async function copy(value: string, key: string, okMsg: string) {
    let ok: boolean;
    try {
      if (!navigator.clipboard?.writeText) throw new Error("no async clipboard");
      await navigator.clipboard.writeText(value);
      ok = true;
    } catch {
      ok = fallbackCopy(value);
    }
    show(key, ok, ok ? okMsg : CLIPBOARD_FAIL);
  }

  async function save() {
    const art = canvas.art;
    if (!art) return;
    const title = text.trim().toUpperCase() || "Drawing";
    try {
      const item = await api.asciiSave(title, art);
      // The design keeps one copy of a drawing: saving it again moves it to the front.
      const dupes = saved.filter((x) => x.raw === art);
      saved = [item, ...saved.filter((x) => x.raw !== art)];
      for (const d of dupes) api.asciiDelete(d.id).catch((err) => reportError("ascii_delete", err));
      show("saved", true, "Saved to the gallery below.");
    } catch (err) {
      reportError("ascii_save", err);
      show("saveFail", false, "Couldn’t save the drawing.");
    }
  }

  async function remove(id: number) {
    const before = saved;
    saved = saved.filter((x) => x.id !== id);
    try {
      await api.asciiDelete(id);
    } catch (err) {
      reportError("ascii_delete", err);
      saved = before;
    }
  }

  function copyArt() {
    const art = canvas.art;
    if (!art) return;
    const { cols, rows } = artSize(art);
    copy(art, "gen", `Copied ${cols} × ${rows} characters.`);
  }

  function onPaint(x: number, y: number) {
    paint = paintAt(paint, x, y, canvas, tool === "erase" ? " " : brush);
  }

  function loadPaint(raw: string) {
    text = "";
    paint = loadRaw(raw);
  }

  type Tile = { key: string; title: string; raw: string; mine: boolean; open: () => void; id?: number };
  const gallery = $derived<Tile[]>([
    ...saved.map((a) => ({ key: `u${a.id}`, id: a.id, title: a.title, raw: a.raw, mine: true, open: () => loadPaint(a.raw) })),
    ...PRESETS.map((p, i) => ({
      key: `p${i}`,
      title: p.title,
      raw: p.raw,
      mine: false,
      open:
        p.text !== undefined
          ? () => {
              text = p.text;
              char = p.char;
              fx = p.effect;
              paint = {};
            }
          : () => loadPaint(p.raw),
    })),
  ]);

  const copyLabel = $derived(flash?.key === "gen" ? (flash.ok ? "Copied ✓" : "Copy failed") : "Copy to clipboard");
</script>

<div class="page" use:scrollHost>
  <div class="editor">
    <div class="controls">
      <div class="field">
        <input
          id="opd-ascii-text"
          aria-label="Text"
          value={text}
          maxlength={MAX_TEXT}
          placeholder="Type…"
          spellcheck="false"
          autocomplete="off"
          oninput={(e) => {
            text = filterText(e.currentTarget.value);
            e.currentTarget.value = text;
          }}
        />
        <span class="count" class:full={text.length >= MAX_TEXT} aria-hidden="true">{text.length} / {MAX_TEXT}</span>
      </div>
      <CharGrid chars={FILL_CHARS} value={char} onchange={(c) => (char = c)} label="Fill character" columns={6} />
      <Segmented options={EFFECTS} value={fx} onchange={(v) => (fx = v)} variant="fill" label="Effect" class="fx" />
      <div class="rule"></div>
      <div class="toolrow">
        <Segmented options={TOOLS} value={tool} onchange={(v) => (tool = v)} variant="fill" label="Tool" class="tools">
          {#snippet item(o)}
            {#if o.value === "draw"}
              <svg width="15" height="15" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round" aria-hidden="true"
                ><path d="M11 2.5l2.5 2.5L6 12.5 2.8 13.2 3.5 10z"></path></svg
              >
            {:else}
              <svg width="16" height="15" viewBox="0 0 17 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round" stroke-linecap="round" aria-hidden="true"
                ><path d="M6.5 13.5L2 9l7-7 5.5 5.5-6 6z"></path><path d="M5 6l5.5 5.5M6.5 13.5H15"></path></svg
              >
            {/if}
          {/snippet}
        </Segmented>
        <button type="button" class="clear" disabled={noPaint} aria-label="Clear drawing" data-tip="Clear drawing" onclick={() => (paint = {})}
          ><svg width="13" height="14" viewBox="0 0 13 14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"
            ><path d="M1 3.5h11M4.5 3.5V1.8h4v1.7M2.5 3.5l.7 8.8h6.6l.7-8.8"></path></svg
          >Clear</button
        >
      </div>
      <CharGrid
        chars={BRUSHES}
        value={brush}
        onchange={(c) => {
          brush = c;
          tool = "draw";
        }}
        label="Brush"
        columns={5}
        size="brush"
        lit={tool === "draw"}
      />
      <div class="spacer"></div>
      <div class="actions">
        <button type="button" class="primary" onclick={copyArt}>{copyLabel}</button>
        <button type="button" class="secondary" onclick={save}>{flash?.key === "saved" ? "Saved ✓" : "Save"}</button>
      </div>
      <span class="status" class:bad={flash !== null && !flash.ok} role="status" aria-live="polite">{flash?.msg ?? ""}</span>
    </div>
    <AsciiCanvas {canvas} onpaint={onPaint} />
  </div>

  <div class="gallery">
    {#each gallery as g (g.key)}
      <GalleryTile
        title={g.title}
        raw={g.raw}
        mine={g.mine}
        copyState={flash?.key === g.key ? (flash.ok ? "ok" : "fail") : null}
        onopen={g.open}
        oncopy={() => copy(g.raw, g.key, "Copied from the gallery.")}
        ondelete={g.id !== undefined ? () => remove(g.id!) : undefined}
      />
    {/each}
  </div>
</div>

<style>
  .page {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 18px 24px 24px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .editor {
    flex: none;
    height: 440px;
    display: grid;
    grid-template-columns: 300px minmax(0, 1fr);
    gap: 12px;
  }

  .controls {
    background: var(--card);
    border: 1px solid var(--line);
    border-radius: 12px;
    padding: 14px;
    display: flex;
    flex-direction: column;
    gap: 14px;
    overflow: auto;
  }

  .field {
    position: relative;
    flex: none;
  }

  input {
    width: 100%;
    height: 40px;
    border-radius: 8px;
    background: var(--input);
    border: 1px solid var(--line-control);
    padding: 0 52px 0 12px;
    color: var(--text);
    font: 700 18px/1 var(--font-display);
    letter-spacing: 0.12em;
    text-transform: uppercase;
  }

  .count {
    position: absolute;
    right: 11px;
    top: 50%;
    transform: translateY(-50%);
    font: 500 12px/1 var(--font);
    color: var(--text-muted);
    pointer-events: none;
  }

  .count.full {
    color: var(--lost-text);
  }

  .controls :global(.fx),
  .controls :global(.tools) {
    flex: none;
  }

  .controls :global(.tools) {
    flex: 1;
  }

  .rule {
    flex: none;
    height: 1px;
    background: var(--line);
  }

  .toolrow {
    flex: none;
    display: flex;
    gap: 8px;
  }

  .clear {
    flex: none;
    height: 36px;
    padding: 0 12px;
    border-radius: 8px;
    display: flex;
    align-items: center;
    gap: 7px;
    background: rgba(239, 106, 91, 0.12);
    border: 1px solid rgba(239, 106, 91, 0.5);
    color: var(--error-text);
    font: 600 13px/1 var(--font);
    cursor: pointer;
  }

  .clear:hover:not(:disabled) {
    background: rgba(239, 106, 91, 0.2);
    color: #ffd0ca;
  }

  .clear:disabled {
    opacity: 0.45;
    cursor: default;
  }

  .spacer {
    flex: 1;
  }

  .actions {
    flex: none;
    display: flex;
    gap: 8px;
  }

  .primary {
    flex: 1;
    height: 38px;
    border-radius: 8px;
    background: var(--accent);
    color: var(--on-accent);
    border: none;
    font: 600 13.5px/1 var(--font);
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 7px;
  }

  .primary:hover {
    background: var(--accent-hover);
  }

  .secondary {
    height: 38px;
    padding: 0 14px;
    border-radius: 8px;
    background: var(--button);
    color: var(--text);
    border: 1px solid var(--line-control);
    font: 600 13.5px/1 var(--font);
    cursor: pointer;
  }

  .secondary:hover {
    background: var(--button-hover);
    border-color: var(--line-hover);
  }

  .status {
    flex: none;
    min-height: 16px;
    margin-top: -6px;
    font: 500 12.5px/1.3 var(--font);
    color: var(--win-text);
  }

  .status.bad {
    color: var(--error-text);
  }

  .gallery {
    flex: none;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(210px, 1fr));
    grid-auto-rows: 160px;
    gap: 10px;
  }
</style>
