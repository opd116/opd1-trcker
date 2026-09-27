import { mount } from "svelte";
import "./app.css";
import App from "./App.svelte";
import { startFeeds } from "./lib/stores";

void startFeeds();

const app = mount(App, {
  target: document.getElementById("app")!,
});

export default app;
