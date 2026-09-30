// DEV-only demo feed (rulings R12, R15). Loaded with a dynamic import behind
// `import.meta.env.DEV && !isTauri()`, so `vite build` never includes it.
//
// Scene selection:
//   URL  ?scene=<any SceneName>            (Playwright; spec §8.2)
//        &route=details&puuid=…[&more=1]   open Player Details
//        &route=match&match=…&puuid=…&from=recent|encounter
//   keys 1–5 lobby with n players · 6 pregame · 7 ingame · 8 ingame-longnames · 0 closed
// Theme options (?tone= …) are read by themeStore. No on-screen control, so layout is never affected.
import type { Writable } from "svelte/store";
import { go, type Route } from "../router.svelte";
import type { LiveState, Meta } from "../types";
import demoMeta from "./meta.json";
import { SCENES, type SceneName } from "./scenes";

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

const DEFAULT_SCENE: SceneName = "lobby1";

const isScene = (v: string | null): v is SceneName => v !== null && Object.hasOwn(SCENES, v);

/** `&route=…` overrides the scene's own route; anything incomplete keeps `fallback`. */
export function routeFromSearch(q: URLSearchParams, fallback: Route): Route {
  const puuid = q.get("puuid");
  switch (q.get("route")) {
    case "live":
      return { name: "live" };
    case "details":
      return puuid ? { name: "details", puuid, more: q.get("more") === "1" } : fallback;
    case "match": {
      const matchId = q.get("match");
      if (!matchId || !puuid) return fallback;
      return {
        name: "match",
        matchId,
        focusPuuid: puuid,
        from: q.get("from") === "encounter" ? "encounter" : "recent",
        back: { puuid, more: false },
      };
    }
    default:
      return fallback;
  }
}

let running = false;

export function startDemo(apply: (s: LiveState | null) => void, meta: Writable<Meta | null>): void {
  if (running) return;
  running = true;

  // Invisible marker: the dist guard (Task 16a Step 6, Task 19 CI) fails the build if this string ever reaches ui/dist.
  document.documentElement.dataset.feed = "demo-feed";

  const show = (name: SceneName, route?: Route) => {
    const scene = SCENES[name];
    meta.set(scene.meta === "pending" ? null : (scene.meta ?? (demoMeta as Meta)));
    apply(scene.live);
    go(route ?? scene.route);
  };
  // A key press changes the scene: keep the address bar honest and drop the route overrides of the old one.
  const syncUrl = (name: SceneName) => {
    const url = new URL(location.href);
    url.searchParams.set("scene", name);
    for (const k of ["route", "puuid", "match", "from", "more"]) url.searchParams.delete(k);
    history.replaceState(null, "", url);
  };

  const q = new URLSearchParams(location.search);
  const fromUrl = q.get("scene");
  const first = isScene(fromUrl) ? fromUrl : DEFAULT_SCENE;
  show(first, routeFromSearch(q, SCENES[first].route));
  if (first !== fromUrl) syncUrl(first);

  window.addEventListener("keydown", (e) => {
    if (e.ctrlKey || e.metaKey || e.altKey || e.repeat) return;
    const target = e.target as HTMLElement | null;
    if (target?.closest("input, textarea, [contenteditable]")) return;
    const name = KEYS[e.key];
    if (!name) return;
    show(name);
    syncUrl(name);
  });
}
