// A side-effect module, imported FIRST by main.ts: ES modules run in import order, so the listeners are on the window
// before the stylesheet, the theme store, the app and the stores load. An error thrown while any of them evaluates is
// then logged too (Ctrl+Shift+L / Ctrl+Shift+D), not only the ones after mount.
import { installErrorReporting } from "./uilog";

installErrorReporting(window);
