// Every v2 screen fits (CLAUDE.md "Design rules": verify every screen at 1280×800, 1180×760 and, for Live, the 420px
// glance column). For each screen and its important demo states (demo/feed.ts parameters) at each size: #app fills the
// viewport, nothing scrolls sideways, no text is clipped (only the `data-ellipsis` elements may truncate, with an
// ellipsis), no two texts overlap, every font size is one the design uses (9–10px badges, 10.5px Geist Mono
// micro-labels, nothing else under 12px except the design's own 11 / 11.5px: checks.ts `checkType`), the fonts are the
// three bundled families, and a hidden name never reaches the DOM. The Live board does not scroll at the design's
// 1280×800 baseline; at the 1180×760 minimum, with the lost banner, and in the glance column it may, as the design's
// own containers do (TrackerApp.dc.html 46, 180: `overflow:auto`). The page clock is frozen (harness NOW). A few dense
// states run again at DPR 1.5 (Windows at 150%), where text metrics round differently.
import { expect, test } from "@playwright/test";
import { PUUID } from "../src/lib/demo/cdv8";
import { RECAP_MATCH_ID } from "../src/lib/demo/v2/status";
import { fitChecks } from "./checks";
import { openScene, pointerAway, SIZES, watchFontCdn } from "./harness";

type Size = keyof typeof SIZES;
type Case = {
  /** The test title. */
  name: string;
  scene: string;
  params?: Record<string, string>;
  /** The Live screen: at 1280×800 (no lost banner) nothing may scroll there. */
  live?: boolean;
  sizes: readonly Size[];
};

const BOTH = ["default", "min"] as const;
const LIVE_ALL = ["default", "min", "glance"] as const;
const MARA = PUUID.mara!;
const FAHAD = PUUID.fahad!;

const CASES: Case[] = [
  // ── Live: the board ──
  { name: "live in game", scene: "ingame", live: true, sizes: LIVE_ALL },
  { name: "live in game, longest names", scene: "ingame-longnames", live: true, sizes: LIVE_ALL },
  { name: "live in game, loading", scene: "ingame-loading", live: true, sizes: BOTH },
  { name: "live in game, partial cards", scene: "ingame-partial", live: true, sizes: LIVE_ALL },
  { name: "live in game, four parties", scene: "ingame-parties4", live: true, sizes: LIVE_ALL },
  { name: "live in game, remembered names", scene: "ingame-remembered", live: true, sizes: LIVE_ALL },
  { name: "live in game, unranked", scene: "ingame-unranked", live: true, sizes: BOTH },
  { name: "live in game, RR unknown", scene: "ingame-rr-unknown", live: true, sizes: BOTH },
  { name: "live in game, names pending", scene: "ingame-nometa", live: true, sizes: BOTH },
  { name: "live in game, connection lost", scene: "ingame", params: { variant: "lost" }, live: true, sizes: BOTH },
  { name: "live agent select", scene: "pregame", live: true, sizes: LIVE_ALL },
  { name: "live agent select, no map yet", scene: "pregame-nomap", live: true, sizes: BOTH },
  { name: "live agent select, loading", scene: "pregame-loading", live: true, sizes: BOTH },
  // ── Live: waiting, error, recap ──
  { name: "live waiting, closed", scene: "closed", live: true, sizes: LIVE_ALL },
  { name: "live waiting, never connected", scene: "closed-empty", live: true, sizes: BOTH },
  { name: "live waiting, booting", scene: "boot", live: true, sizes: BOTH },
  { name: "live lobby, solo", scene: "lobby1", live: true, sizes: BOTH },
  { name: "live lobby, party of 5 in queue", scene: "lobby5", params: { queue: "1" }, live: true, sizes: LIVE_ALL },
  { name: "live lobby, party of 3 in queue", scene: "lobby3", params: { queue: "1" }, live: true, sizes: BOTH },
  { name: "live error", scene: "closed", params: { variant: "error" }, live: true, sizes: LIVE_ALL },
  { name: "live recap", scene: "closed", params: { recap: "1" }, live: true, sizes: LIVE_ALL },
  // ── the profile drawer ──
  { name: "profile, another player", scene: "ingame", params: { profile: MARA }, sizes: BOTH },
  { name: "profile, the user", scene: "ingame", params: { profile: FAHAD }, sizes: BOTH },
  { name: "profile, remembered name", scene: "ingame", params: { profile: PUUID.rook! }, sizes: BOTH },
  { name: "profile, all recent matches", scene: "details-more", sizes: BOTH },
  { name: "profile, hidden player", scene: "details-hidden", sizes: BOTH },
  { name: "profile, empty", scene: "details-empty", sizes: BOTH },
  { name: "profile, loading", scene: "details-loading", sizes: BOTH },
  { name: "profile, longest names", scene: "details-longnames", sizes: BOTH },
  { name: "profile, partial", scene: "details-partial", sizes: BOTH },
  { name: "profile, no stats", scene: "details-nostats", sizes: BOTH },
  { name: "profile, agent select (hidden Nova)", scene: "pregame", params: { profile: PUUID.kilo! }, sizes: ["default"] },
  // ── Saved ──
  { name: "saved, in a match", scene: "ingame", params: { screen: "saved" }, sizes: BOTH },
  { name: "saved, closed", scene: "closed", params: { screen: "saved" }, sizes: BOTH },
  { name: "saved, empty", scene: "closed", params: { screen: "saved", empty: "1" }, sizes: BOTH },
  // ── History and the scoreboard ──
  { name: "history", scene: "closed", params: { screen: "history" }, sizes: BOTH },
  { name: "history, empty", scene: "closed", params: { screen: "history", empty: "1" }, sizes: BOTH },
  { name: "scoreboard, from history", scene: "closed", params: { screen: "history", match: RECAP_MATCH_ID, puuid: FAHAD }, sizes: BOTH },
  { name: "scoreboard, from a profile", scene: "match", sizes: BOTH },
  { name: "scoreboard, from an encounter", scene: "match-enc", sizes: BOTH },
  { name: "scoreboard, hidden names", scene: "match-hidden", sizes: BOTH },
  { name: "scoreboard, deathmatch", scene: "match-dm", sizes: BOTH },
  { name: "scoreboard, loading", scene: "match-loading", sizes: BOTH },
  { name: "scoreboard, longest names", scene: "match-longnames", sizes: BOTH },
  { name: "scoreboard, no stats", scene: "match-nostats", sizes: BOTH },
  // ── Competitive ──
  { name: "comp", scene: "mystats", sizes: BOTH },
  { name: "comp, new account", scene: "mystats-new", sizes: BOTH },
  { name: "comp, loading", scene: "mystats-loading", sizes: BOTH },
  { name: "comp, unranked", scene: "mystats-unranked", sizes: BOTH },
  { name: "comp, immortal", scene: "mystats-immortal", sizes: BOTH },
  { name: "comp, RR unknown", scene: "mystats-rrnull", sizes: BOTH },
  { name: "comp, longest names", scene: "mystats-longnames", sizes: BOTH },
  // ── Skins, ASCII, Settings, About ──
  { name: "skins", scene: "closed", params: { screen: "skins" }, sizes: BOTH },
  { name: "skins, empty", scene: "closed", params: { screen: "skins", empty: "1" }, sizes: BOTH },
  { name: "ascii", scene: "closed", params: { screen: "ascii" }, sizes: BOTH },
  { name: "settings", scene: "closed", params: { screen: "settings" }, sizes: BOTH },
  { name: "about", scene: "closed", params: { screen: "about" }, sizes: BOTH },
];

async function run(page: import("@playwright/test").Page, c: Case, size: Size): Promise<void> {
  await page.setViewportSize(SIZES[size]);
  const cdn = watchFontCdn(page);
  await openScene(page, c.scene, { ...c.params, ...(size === "glance" ? { glance: "1" } : {}) });
  await pointerAway(page);
  await expect(page.locator("#tooltip")).toHaveCount(0);
  const noScroll = !!c.live && size === "default" && c.params?.variant !== "lost";
  expect(await fitChecks(page, c.scene, { noScroll })).toEqual([]);
  expect(cdn, "requests to a font CDN").toEqual([]);
}

for (const c of CASES) {
  for (const size of c.sizes) {
    const { width, height } = SIZES[size];
    const title = `${c.name} at ${width}×${height}`;
    test(`fit: ${title}`, async ({ page }) => {
      await run(page, c, size);
    });
  }
}

test.describe("at DPR 1.5", () => {
  test.use({ deviceScaleFactor: 1.5 });
  const DENSE = ["live in game, longest names", "live agent select", "profile, longest names", "scoreboard, longest names", "comp, longest names", "history"];
  for (const c of CASES.filter((c) => DENSE.includes(c.name))) {
    test(`fit: ${c.name} at 1180×760, DPR 1.5`, async ({ page }) => {
      await run(page, c, "min");
    });
  }
});
