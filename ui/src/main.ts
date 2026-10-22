// Very first: the error listeners are on the window before anything else evaluates (see uilog-install.ts).
import "./lib/uilog-install";
import "./app.css";
// First, so the theme variables exist before anything paints.
import "./lib/themeStore.svelte";
import { mount } from "svelte";
import App from "./App.svelte";
import { installReleaseGuards } from "./lib/boot";
import { startAppData } from "./lib/appdata.svelte";
import { startFeeds } from "./lib/stores";
import { attachTooltips } from "./lib/tooltip.svelte";

// Release only: no context menu, no reload / print / find / view-source / caret-browsing keys (Ctrl+Shift+L and
// Ctrl+Shift+D are the app's own and stay). Development keeps the browser's behaviour.
if (!import.meta.env.DEV) installReleaseGuards(window);

const root = document.getElementById("app")!;
const app = mount(App, { target: root });
attachTooltips(root);
void startFeeds().then(() => startAppData());

export default app;
