// Keyboard behaviour for the design's radio groups (Settings' segmented controls, the ASCII studio's character grids,
// effect and tool pickers): one tab stop per group (the checked radio), arrow keys move the choice and the focus,
// wrapping; Home / End jump to the ends. Space / Enter are the buttons' own click.

/** The index an arrow / Home / End key moves to from `i` among `n` radios, or null for any other key. */
export function radioTarget(key: string, i: number, n: number): number | null {
  if (n <= 0) return null;
  switch (key) {
    case "ArrowRight":
    case "ArrowDown":
      return (i + 1) % n;
    case "ArrowLeft":
    case "ArrowUp":
      return (i - 1 + n) % n;
    case "Home":
      return 0;
    case "End":
      return n - 1;
    default:
      return null;
  }
}

/** The tabindex of radio `i` when `selected` is checked (-1 = none checked: the first one takes the stop). */
export const radioTabIndex = (i: number, selected: number): 0 | -1 => (i === (selected < 0 ? 0 : selected) ? 0 : -1);

/**
 * `onkeydown` for a `role="radiogroup"` element: moves to the next `[role=radio]` button inside it (skipping disabled
 * ones is not needed: the design's groups have none) and clicks it, so the group's own handler picks it.
 */
export function radioKeydown(e: KeyboardEvent): void {
  const group = e.currentTarget as HTMLElement;
  const radios = [...group.querySelectorAll<HTMLElement>('[role="radio"]')];
  const at = radios.indexOf(document.activeElement as HTMLElement);
  if (at < 0) return;
  const to = radioTarget(e.key, at, radios.length);
  if (to === null) return;
  e.preventDefault();
  radios[to]!.focus();
  radios[to]!.click();
}
