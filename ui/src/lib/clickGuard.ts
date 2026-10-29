// A page opener can replace what is under the pointer before a double-click's second click arrives.
// Only repeated mouse clicks (MouseEvent.detail > 1) are blocked during that transition.
// A fresh single click, keyboard activation (detail 0), or programmatic click must still work.

export const DOUBLE_CLICK_MS = 400;

export interface ClickGuard {
  /** A card just opened Details. */
  arm(): void;
  /** Without a detail, reports whether the guard is active. With one, filters only repeated mouse clicks. */
  swallows(detail?: number): boolean;
}

export function createClickGuard(now: () => number = () => performance.now()): ClickGuard {
  let armedAt = Number.NEGATIVE_INFINITY;
  return {
    arm() {
      armedAt = now();
    },
    swallows(detail = 2) {
      return detail > 1 && now() - armedAt < DOUBLE_CLICK_MS;
    },
  };
}

/** The app's one guard: the router arms it, App reads it. */
export const cardOpenGuard = createClickGuard();
