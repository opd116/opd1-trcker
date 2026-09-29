import "./app.css";
// First, so the theme variables exist before anything paints.
import "./lib/themeStore.svelte";
import { mount } from "svelte";
import App from "./App.svelte";
import { startFeeds } from "./lib/stores";
import { attachTooltips } from "./lib/tooltip.svelte";

const root = document.getElementById("app")!;
const app = mount(App, { target: root });
attachTooltips(root);
void startFeeds();

export default app;
