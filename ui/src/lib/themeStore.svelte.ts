// The theme as reactive state (spec §1.8). Not persisted. In DEV only, URL params override it (for Playwright).
import { applyTheme, DEFAULT_THEME, themeFromSearch, type ThemeOptions } from "./theme";

function initial(): ThemeOptions {
  const o: ThemeOptions = { ...DEFAULT_THEME };
  if (import.meta.env.DEV) Object.assign(o, themeFromSearch(location.search));
  return o;
}

export const theme = $state<ThemeOptions>(initial());

// Synchronously, so the variables exist before the first paint; then again on every change. The effect's own
// first run (same values, about 60 setProperty calls and a style recalc) is skipped: it only has to subscribe.
applyTheme($state.snapshot(theme));
let firstRun = true;
$effect.root(() => {
  $effect(() => {
    const snapshot = $state.snapshot(theme); // reads every field, so all of them are tracked
    if (firstRun) {
      firstRun = false;
      return;
    }
    applyTheme(snapshot);
  });
});
