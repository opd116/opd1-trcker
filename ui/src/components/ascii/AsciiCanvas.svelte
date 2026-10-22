<!--
  The studio's canvas (TrackerApp.dc.html 894–896): a dotted backdrop holding the `<pre>`, which has a faint
  character grid, a violet glow and a crosshair cursor. Drag with the mouse to paint cells (down / move; up or leaving
  the backdrop ends the stroke). Keyboard: the canvas is one tab stop; the arrow keys move a cell cursor and Space or
  Enter paints the cell under it with the current tool.
-->
<script lang="ts">
  import { cellAt, type Canvas } from "../../lib/ascii";

  let { canvas, onpaint }: { canvas: Canvas; onpaint: (x: number, y: number) => void } = $props();

  let painting = false;
  let last: string | null = null;
  /** The keyboard cursor (cell), null until the canvas is used from the keyboard. */
  let cursor = $state<[number, number] | null>(null);
  let focused = $state(false);

  function paint(x: number, y: number) {
    const k = `${x},${y}`;
    if (last === k) return;
    last = k;
    onpaint(x, y);
  }

  function at(e: MouseEvent): [number, number] {
    return cellAt(e.clientX, e.clientY, (e.currentTarget as HTMLElement).getBoundingClientRect(), canvas);
  }

  function down(e: MouseEvent) {
    if (e.button !== 0) return;
    e.preventDefault();
    (e.currentTarget as HTMLElement).focus({ preventScroll: true });
    cursor = null;
    painting = true;
    last = null;
    paint(...at(e));
  }

  function move(e: MouseEvent) {
    if (painting) paint(...at(e));
  }

  function up() {
    painting = false;
    last = null;
  }

  const clamp = (v: number, n: number) => Math.max(0, Math.min(n - 1, v));

  function key(e: KeyboardEvent) {
    const [x, y] = cursor ?? [canvas.cx, canvas.cy];
    const step: Record<string, [number, number]> = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1] };
    const d = step[e.key];
    if (d) {
      e.preventDefault();
      cursor = [clamp(x + d[0], canvas.W), clamp(y + d[1], canvas.H)];
    } else if (e.key === " " || e.key === "Enter") {
      e.preventDefault();
      cursor = [clamp(x, canvas.W), clamp(y, canvas.H)];
      last = null;
      paint(cursor[0], cursor[1]);
    }
  }
</script>

<svelte:window onmouseup={up} />

<div class="backdrop" role="presentation" onmouseup={up} onmouseleave={up}>
  <!-- A drawing surface: the pointer and the keyboard both paint on it (role="application"). -->
  <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
  <pre
    class="canvas"
    role="application"
    aria-label="ASCII art. Click and drag to draw on it. From the keyboard, arrow keys move the cursor and Space draws."
    tabindex="0"
    style:font-size="{canvas.fontPx}px"
    onmousedown={down}
    onmousemove={move}
    onkeydown={key}
    onfocus={() => (focused = true)}
    onblur={() => (focused = false)}>{canvas.out}{#if focused && cursor}<span
        class="cursor"
        aria-hidden="true"
        style:left="{cursor[0]}ch"
        style:top="{cursor[1]}em"></span
      >{/if}</pre>
</div>

<style>
  .backdrop {
    min-width: 0;
    min-height: 0;
    border-radius: 12px;
    border: 1px solid var(--line);
    background: var(--rail) radial-gradient(circle, #191b20 1px, transparent 1.2px) 0 0 / 16px 16px;
    display: flex;
    align-items: center;
    justify-content: center;
    overflow: auto;
    padding: 20px;
    user-select: none;
  }

  .canvas {
    position: relative;
    margin: 0;
    flex: none;
    font-family: "Cascadia Mono", Consolas, "DejaVu Sans Mono", Menlo, var(--font-mono);
    font-weight: 400;
    line-height: 1;
    color: #e9e6ff;
    letter-spacing: 0;
    font-variant-ligatures: none;
    white-space: pre;
    cursor: crosshair;
    background-color: var(--bg);
    background-image:
      linear-gradient(rgba(255, 255, 255, 0.04) 1px, transparent 1px),
      linear-gradient(90deg, rgba(255, 255, 255, 0.04) 1px, transparent 1px);
    background-size: 1ch 1em;
    outline: 1px solid var(--line-soft);
    text-shadow: 0 0 24px rgba(157, 140, 255, 0.35);
  }

  .canvas:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: 2px;
  }

  .cursor {
    position: absolute;
    width: 1ch;
    height: 1em;
    outline: 1px solid var(--accent);
    background: rgba(157, 140, 255, 0.25);
    pointer-events: none;
  }
</style>
