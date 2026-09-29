import { afterEach, describe, expect, it, vi } from "vitest";
import { assetUrl, type AssetKind } from "./assets";

const KINDS: AssetKind[] = [
  "agent",
  "tier",
  "mapList",
  "chroma",
  "cardLarge",
  "contentTier",
  "cardSmall",
  "cardWide",
  "mapSplash",
];
const MAC_UA = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15";
const WIN_UA = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Edg/140.0";

afterEach(() => vi.unstubAllGlobals());

/** `imgBase()` is memoised per module, so each user-agent case loads a fresh copy of `assets.ts`. */
async function inTauri(userAgent: string) {
  vi.resetModules();
  vi.stubGlobal("isTauri", true);
  vi.stubGlobal("navigator", { userAgent });
  return (await import("./assets")).assetUrl;
}

describe("assetUrl in Tauri", () => {
  it("serves every kind from the img scheme", async () => {
    const url = await inTauri(MAC_UA);
    for (const kind of KINDS) {
      expect(url(kind, "abc-123")).toBe(`img://localhost/${kind}/abc-123`);
    }
  });

  it("uses the Windows (WebView2) base when the UA says Windows", async () => {
    const url = await inTauri(WIN_UA);
    expect(url("agent", "a1")).toBe("http://img.localhost/agent/a1");
    expect(url("cardLarge", "c1")).toBe("http://img.localhost/cardLarge/c1");
    expect(url("contentTier", "t1")).toBe("http://img.localhost/contentTier/t1");
  });

  it("puts numeric tier ids in the path and encodes odd ids", async () => {
    const url = await inTauri(MAC_UA);
    expect(url("tier", 21)).toBe("img://localhost/tier/21");
    expect(url("agent", "a b/c")).toBe("img://localhost/agent/a%20b%2Fc");
  });

  it("reads the user agent once (the base is memoised)", async () => {
    vi.resetModules();
    vi.stubGlobal("isTauri", true);
    let reads = 0;
    vi.stubGlobal("navigator", {
      get userAgent() {
        reads++;
        return WIN_UA;
      },
    });
    const { assetUrl } = await import("./assets");
    expect(reads).toBe(0); // lazy: nothing is read at import time
    for (let i = 0; i < 5; i++) assetUrl("agent", `a${i}`);
    expect(reads).toBe(1);
  });
});

describe("assetUrl in the DEV browser", () => {
  const CDN = "https://media.valorant-api.com";
  const SET = "03621f52-342b-cf4e-4f86-9350a49c6d04";

  it("uses the design's CDN patterns", () => {
    // vitest runs with import.meta.env.DEV = true and no `isTauri` global
    expect(assetUrl("agent", "A")).toBe(`${CDN}/agents/A/displayicon.png`);
    expect(assetUrl("tier", 21)).toBe(`${CDN}/competitivetiers/${SET}/21/smallicon.png`);
    expect(assetUrl("mapList", "M")).toBe(`${CDN}/maps/M/listviewicon.png`);
    expect(assetUrl("chroma", "C")).toBe(`${CDN}/weaponskinchromas/C/fullrender.png`);
    expect(assetUrl("cardLarge", "P")).toBe(`${CDN}/playercards/P/largeart.png`);
    expect(assetUrl("contentTier", "T")).toBe(`${CDN}/contenttiers/T/displayicon.png`);
  });

  it("keeps the older kinds", () => {
    expect(assetUrl("cardSmall", "P")).toBe(`${CDN}/playercards/P/smallart.png`);
    expect(assetUrl("cardWide", "P")).toBe(`${CDN}/playercards/P/wideart.png`);
    expect(assetUrl("mapSplash", "M")).toBe(`${CDN}/maps/M/splash.png`);
  });
});
