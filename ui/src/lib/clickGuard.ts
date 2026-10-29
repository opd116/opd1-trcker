// Opening a drawer or scoreboard can replace what is under the pointer between two quick clicks.
// Block repeated mouse clicks, including separate clicks at the opening point, for 400 ms.
// Fresh clicks elsewhere and keyboard activation remain available immediately.

export const DOUBLE_CLICK_MS = 400;
const POINTER_SLOP_PX = 4;
type Point = { x: number; y: number };

export interface ClickGuard {
  /** Note the click being dispatched, before its target's handler can open a page. */
  noteClick(detail: number, x: number, y: number): void;
  arm(): void;
  /** Without arguments, reports whether the guard is active. */
  swallows(detail?: number, x?: number, y?: number): boolean;
}

export function createClickGuard(now: () => number = () => performance.now()): ClickGuard {
  let armedAt = Number.NEGATIVE_INFINITY;
  let openingPoint: Point | null = null;
  let dispatchPoint: Point | null = null;
  return {
    noteClick(detail, x, y) {
      const point = detail > 0 ? { x, y } : null;
      dispatchPoint = point;
      // Only a synchronous open from this event inherits its position. A later programmatic or keyboard open must
      // not inherit the last mouse position, even if it happens within the guard's interval.
      queueMicrotask(() => {
        if (dispatchPoint === point) dispatchPoint = null;
      });
    },
    arm() {
      armedAt = now();
      openingPoint = dispatchPoint;
    },
    swallows(detail = 2, x, y) {
      if (now() - armedAt >= DOUBLE_CLICK_MS || detail === 0) return false;
      if (detail > 1) return true;
      return (
        openingPoint !== null &&
        x !== undefined &&
        y !== undefined &&
        Math.abs(x - openingPoint.x) <= POINTER_SLOP_PX &&
        Math.abs(y - openingPoint.y) <= POINTER_SLOP_PX
      );
    },
  };
}

/** The app's one guard: App notes capture-phase clicks, the router arms it. */
export const cardOpenGuard = createClickGuard();
