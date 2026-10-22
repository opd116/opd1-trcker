// DEV-only demo feed (rulings R12, R15). Loaded with a dynamic import behind
// `import.meta.env.DEV && !isTauri()`, so `vite build` never includes it.
//
// Scene selection:
//   URL  ?scene=<any SceneName>                      (Playwright; spec §8.2)
//        &screen=live|saved|history|comp|skins|ascii|settings|about
//        &profile=<puuid>                            open the profile drawer
//        &match=<id>&puuid=<focus>[&from=<MatchFrom>] open the History scoreboard (from defaults to history)
//        &glance=1                                   glance mode
//   v8 names, still accepted: &route=mystats (→ comp), &route=details&puuid=… (→ drawer),
//        &route=match&match=…&puuid=…&from=recent|encounter (→ scoreboard, Back reopens the drawer)
//   keys 1–5 lobby with n players · 6 pregame · 7 ingame · 8 ingame-longnames · 0 closed
// No on-screen control, so layout is never affected.
import type { Writable } from "svelte/store";
import { go, openMatch, openProfile, router, SCREENS, setGlance, type EncFrom, type MatchFrom, type Screen } from "../router.svelte";
import type { LiveState, Meta, MyStats } from "../types";
import demoMeta from "./meta.json";
import { SCENES, sceneMyStats, type DemoRoute, type SceneName } from "./scenes";

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

/** Where the demo opens the app: a screen, maybe the drawer or a scoreboard over it, maybe glance mode. */
export type DemoNav = {
  screen: Screen;
  profile: string | null;
  match: { matchId: string; focusPuuid: string; from: MatchFrom; encFrom?: EncFrom } | null;
  glance: boolean | null;
};

const MATCH_FROM: readonly MatchFrom[] = ["history", "recap", "lastMatch", "encounter", "profile", "comp"];

/** A scene's v8 route on the v2 router: mystats → Competitive, details → the drawer, match → the scoreboard. */
export function navFromRoute(r: DemoRoute): DemoNav {
  const base: DemoNav = { screen: "live", profile: null, match: null, glance: null };
  switch (r.name) {
    case "live":
      return base;
    case "mystats":
      return { ...base, screen: "comp" };
    case "details":
      return { ...base, profile: r.puuid };
    case "match":
      return r.back === "mystats"
        ? { ...base, screen: "comp", match: { matchId: r.matchId, focusPuuid: r.focusPuuid, from: "comp" } }
        : {
            ...base,
            match: {
              matchId: r.matchId,
              focusPuuid: r.focusPuuid,
              from: r.from === "encounter" ? "encounter" : "profile",
              encFrom: { puuid: r.back.puuid, screen: "live" },
            },
          };
  }
}

/** The URL's overrides on top of `fallback` (the scene's own). Anything incomplete keeps the fallback's part. */
export function navFromSearch(q: URLSearchParams, fallback: DemoNav): DemoNav {
  const puuid = q.get("puuid");
  let nav: DemoNav = { ...fallback };
  // v8 names first, so the v2 parameters can refine them.
  switch (q.get("route")) {
    case "live":
      nav = navFromRoute({ name: "live" });
      break;
    case "mystats":
      nav = navFromRoute({ name: "mystats" });
      break;
    case "details":
      if (puuid) nav = navFromRoute({ name: "details", puuid, more: false });
      break;
    case "match": {
      const matchId = q.get("match");
      if (matchId && puuid) {
        const from = q.get("from") === "encounter" ? "encounter" : "recent";
        nav = navFromRoute({ name: "match", matchId, focusPuuid: puuid, from, back: { puuid, more: false } });
      }
      break;
    }
  }
  if (!q.has("route")) {
    const screen = q.get("screen");
    if (screen && (SCREENS as readonly string[]).includes(screen)) nav = { ...nav, screen: screen as Screen, profile: null, match: null };
    const profile = q.get("profile");
    if (profile) nav = { ...nav, profile };
    const matchId = q.get("match");
    if (matchId && puuid) {
      const f = q.get("from");
      const from = MATCH_FROM.find((m) => m === f) ?? "history";
      nav = { ...nav, match: { matchId, focusPuuid: puuid, from } };
    }
  }
  if (q.has("glance")) nav = { ...nav, glance: q.get("glance") === "1" };
  return nav;
}

/** Put the router where `nav` says. */
export function applyNav(nav: DemoNav): void {
  go(nav.screen);
  if (nav.match) openMatch(nav.match.matchId, nav.match);
  if (nav.profile) openProfile(nav.profile, null);
  if (nav.glance !== null && nav.glance !== router.glance) setGlance(nav.glance);
}

/** What the scene's `meta` field means: pending (null), the demo meta, a copy edited by the scene, or its own. */
export function sceneMeta(m: (typeof SCENES)[SceneName]["meta"]): Meta | null {
  if (m === "pending") return null;
  if (typeof m === "function") return m(demoMeta as Meta);
  return m ?? (demoMeta as Meta);
}

let running = false;

export function startDemo(
  apply: (s: LiveState | null) => void,
  meta: Writable<Meta | null>,
  mystats: Writable<MyStats | null | undefined>,
): void {
  if (running) return;
  running = true;

  // Invisible marker: the dist guard (Task 16a Step 6, Task 19 CI) fails the build if this string ever reaches ui/dist.
  document.documentElement.dataset.feed = "demo-feed";

  const show = (name: SceneName, nav?: DemoNav) => {
    const scene = SCENES[name];
    meta.set(sceneMeta(scene.meta));
    mystats.set(sceneMyStats(scene));
    apply(scene.live);
    // An `autoRoute` scene (VALORANT closed at startup) leaves the route to the app's own start-page rule, unless the
    // URL names one.
    if (nav || !scene.autoRoute) applyNav(nav ?? navFromRoute(scene.route));
  };
  // A key press changes the scene: keep the address bar honest and drop the route overrides of the old one.
  const syncUrl = (name: SceneName) => {
    const url = new URL(location.href);
    url.searchParams.set("scene", name);
    for (const k of ["route", "puuid", "match", "from", "more", "screen", "profile", "glance"]) url.searchParams.delete(k);
    history.replaceState(null, "", url);
  };

  const q = new URLSearchParams(location.search);
  const fromUrl = q.get("scene");
  const first = isScene(fromUrl) ? fromUrl : DEFAULT_SCENE;
  const overrides = ["route", "screen", "profile", "match", "glance"].some((k) => q.has(k));
  show(first, overrides ? navFromSearch(q, navFromRoute(SCENES[first].route)) : undefined);
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
