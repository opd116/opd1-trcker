// The theme's CSS variables, written on :root synchronously when this module loads (main.ts imports it before
// mounting), so they exist before the first paint. Design v2 has one fixed theme: nothing here changes at runtime.
import { applyTheme } from "./theme";

applyTheme();
