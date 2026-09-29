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

function inTauri(userAgent: string) {
  vi.stubGlobal("isTauri", true);
  vi.stubGlobal("navigator", { userAgent });
}

describe("assetUrl in Tauri", () => {
  it("serves every kind from the img scheme", () => {
    inTauri(MAC_UA);
    for (const kind of KINDS) {
      expect(assetUrl(kind, "abc-123")).toBe(`img://localhost/${kind}/abc-123`);
    }
  });

  it("uses the Windows (WebView2) base when the UA says Windows", () => {
    inTauri(WIN_UA);
    expect(assetUrl("agent", "a1")).toBe("http://img.localhost/agent/a1");
    expect(assetUrl("cardLarge", "c1")).toBe("http://img.localhost/cardLarge/c1");
    expect(assetUrl("contentTier", "t1")).toBe("http://img.localhost/contentTier/t1");
  });

  it("puts numeric tier ids in the path and encodes odd ids", () => {
    inTauri(MAC_UA);
    expect(assetUrl("tier", 21)).toBe("img://localhost/tier/21");
    expect(assetUrl("agent", "a b/c")).toBe("img://localhost/agent/a%20b%2Fc");
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
