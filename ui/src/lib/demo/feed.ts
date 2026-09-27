// DEV-only demo feed (rulings R12, R15). Loaded with a dynamic import behind
// `import.meta.env.DEV && !isTauri()`, so `vite build` never includes it.
//
// Scene selection:
//   URL  ?scene=closed|lobby1..lobby5|pregame|ingame|ingame-longnames   (Playwright)
//   keys 1–5 lobby with n players · 6 pregame · 7 ingame · 8 ingame-longnames · 0 closed
// No on-screen control, so layout is never affected.
import type { Writable } from "svelte/store";
import type { LiveState, Meta } from "../types";
import { buildScene, isScene, type SceneName } from "./fixtures";
import demoMeta from "./meta.json";

const KEYS: Record<string, SceneName> = {
  "0": "closed",
  "1": "lobby1",
  "2": "lobby2",
  "3": "lobby3",
  "4": "lobby4",
  "5": "lobby5",
  "6": "pregame",
  "7": "ingame",
  "8": "ingame-longnames",
};

let running = false;

export function startDemo(live: Writable<LiveState>, meta: Writable<Meta>): void {
  if (running) return;
  running = true;

  // Invisible marker: Task 10's CI fails the build if this string ever reaches ui/dist.
  document.documentElement.dataset.feed = "demo-feed";

  meta.set(demoMeta as Meta);

  const fromUrl = new URLSearchParams(location.search).get("scene");
  const show = (scene: SceneName) => {
    live.set(buildScene(scene));
    const url = new URL(location.href);
    url.searchParams.set("scene", scene);
    history.replaceState(null, "", url);
  };

  if (isScene(fromUrl)) show(fromUrl);
  else if (fromUrl === null) show("lobby1");
  // Other values (e.g. ?scene=cards) are DEV pages that bring their own data.

  window.addEventListener("keydown", (e) => {
    if (e.ctrlKey || e.metaKey || e.altKey || e.repeat) return;
    const target = e.target as HTMLElement | null;
    if (target?.closest("input, textarea, [contenteditable]")) return;
    const scene = KEYS[e.key];
    if (scene) show(scene);
  });
}
