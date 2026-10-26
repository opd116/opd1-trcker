// The double-click guard (v0.2.0 polish). A card click opens Details, which replaces the card grid; the second click
// of a double-click then lands on whatever sits under the pointer on the new page (Back where the top-left card was,
// a recent row under a middle one). For 400 ms after anything opens the profile drawer or a scoreboard (the router
// arms it: a Live or Glance card, a Saved or History row, any other opener), App swallows every click in the capture
// phase.

export const DOUBLE_CLICK_MS = 400;

export interface ClickGuard {
  /** A card just opened Details. */
  arm(): void;
  /** True while a click must be swallowed. */
  swallows(): boolean;
}

export function createClickGuard(now: () => number = () => performance.now()): ClickGuard {
  let armedAt = Number.NEGATIVE_INFINITY;
  return {
    arm() {
      armedAt = now();
    },
    swallows() {
      return now() - armedAt < DOUBLE_CLICK_MS;
    },
  };
}

/** The app's one guard: the router arms it, App reads it. */
export const cardOpenGuard = createClickGuard();
