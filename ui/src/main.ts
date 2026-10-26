// Very first: the error listeners are on the window before anything else evaluates (see uilog-install.ts).
import "./lib/uilog-install";
import "./app.css";
// First, so the theme variables exist before anything paints.
import "./lib/themeStore.svelte";
import { isTauri } from "@tauri-apps/api/core";
import { mount } from "svelte";
import App from "./App.svelte";
import { installReleaseGuards } from "./lib/boot";
import { startAppData } from "./lib/appdata.svelte";
import { startFeeds } from "./lib/stores";
import { attachTooltips } from "./lib/tooltip.svelte";

// Release only: no context menu, no reload / print / find / view-source / caret-browsing keys (Ctrl+Shift+L and
// Ctrl+Shift+D are the app's own and stay). Development keeps the browser's behaviour.
if (!import.meta.env.DEV) installReleaseGuards(window);

// DEV in a plain browser: line the clock up with the demo fixtures before any view reads it (compiled out of builds).
if (import.meta.env.DEV && !isTauri()) (await import("./lib/demo/feed")).alignDemoClock();

const root = document.getElementById("app")!;
const app = mount(App, { target: root });
attachTooltips(root);
// The status pill, settings and marks do not wait for the feeds (whose My Stats can wait on a valorant-api download).
// The DEV demo keeps its order: its status follows the scene the feeds start.
if (import.meta.env.DEV && !isTauri()) void startFeeds().then(() => startAppData());
else {
  void startFeeds();
  void startAppData();
}

export default app;
