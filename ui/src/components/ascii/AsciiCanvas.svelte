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
  let backdrop = $state<HTMLDivElement | null>(null);
  let surface = $state<HTMLPreElement | null>(null);
  let fittedPx = $state<number | null>(null);
  const MIN_PX = 8;
  const fontPx = $derived(Math.min(canvas.fontPx, fittedPx ?? canvas.fontPx));

  // Fit the art to the frame by what the browser really lays out, not by a prediction. Measuring glyph advances at one
  // size and scaling them linearly fails where a browser rounds advances to whole pixels per font size (Chromium on
  // Linux without subpixel positioning): the real <pre> came out wider than predicted and was clipped (R213). A hidden
  // twin of the <pre> is laid out at candidate sizes (a bigger font is never narrower) and a bisection keeps the largest
  // size whose real width fits. Preserve the 8px art floor: unusually wide saved drawings still scroll, not clip.
  $effect(() => {
    const frame = backdrop;
    const pre = surface;
    const preferred = canvas.fontPx;
    const art = canvas.out;
    if (!frame || !pre) return;
    let pending = 0;
    let active = true;
    const fit = () => {
      pending = 0;
      if (!active) return;
      const style = getComputedStyle(frame);
      const available = frame.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight) - 2;
      if (available <= 0) return;
      const twin = document.createElement("pre");
      twin.className = pre.className;
      twin.setAttribute("aria-hidden", "true");
      twin.textContent = art;
      twin.style.cssText = "position:fixed;left:-100000px;top:0;visibility:hidden;pointer-events:none;";
      document.body.append(twin);
      let next: number;
      try {
        const widthAt = (px: number) => {
          twin.style.fontSize = `${px}px`;
          return twin.getBoundingClientRect().width;
        };
        if (widthAt(preferred) <= available) next = preferred;
        else if (widthAt(MIN_PX) > available) next = MIN_PX;
        else {
          let lo = MIN_PX;
          let hi = preferred;
          for (let i = 0; i < 14 && hi - lo > 0.02; i++) {
            const mid = (lo + hi) / 2;
            if (widthAt(mid) <= available) lo = mid;
            else hi = mid;
          }
          next = lo;
        }
      } finally {
        twin.remove();
      }
      if (next !== fittedPx) fittedPx = next;
    };
    const schedule = () => {
      if (active && !pending) pending = requestAnimationFrame(fit);
    };
    const observer = new ResizeObserver(schedule);
    observer.observe(frame);
    observer.observe(pre);
    void document.fonts.ready.then(schedule);
    document.fonts.addEventListener("loadingdone", schedule);
    schedule();
    return () => {
      active = false;
      cancelAnimationFrame(pending);
      observer.disconnect();
      document.fonts.removeEventListener("loadingdone", schedule);
    };
  });

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

<div class="backdrop" bind:this={backdrop} role="presentation" onmouseup={up} onmouseleave={up}>
  <!-- A drawing surface: the pointer and the keyboard both paint on it (role="application"). -->
  <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
  <pre
    bind:this={surface}
    class="canvas"
    role="application"
    aria-label="ASCII art. Click and drag to draw on it. From the keyboard, arrow keys move the cursor and Space draws."
    tabindex="0"
    style:font-size="{fontPx}px"
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
    background: var(--rail);
    display: flex;
    align-items: safe center;
    justify-content: safe center;
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
    color: var(--text);
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
    background: var(--accent-soft);
    pointer-events: none;
  }
</style>
