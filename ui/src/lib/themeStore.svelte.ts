// The theme as reactive state (spec §1.8). Not persisted. In DEV only, URL params override it (for Playwright).
import { applyTheme, DEFAULT_THEME, themeFromSearch, type ThemeOptions } from "./theme";

function initial(): ThemeOptions {
  const o: ThemeOptions = { ...DEFAULT_THEME };
  if (import.meta.env.DEV) Object.assign(o, themeFromSearch(location.search));
  return o;
}

export const theme = $state<ThemeOptions>(initial());

// Synchronously, so the variables exist before the first paint; then again on every change.
applyTheme($state.snapshot(theme));
$effect.root(() => {
  $effect(() => applyTheme($state.snapshot(theme)));
});
