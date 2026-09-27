import { isTauri } from "@tauri-apps/api/core";

/** Image kinds served by the backend's `img` scheme (rulings R11/R21). */
export type AssetKind = "agent" | "cardSmall" | "cardWide" | "tier" | "mapList" | "mapSplash" | "chroma";

let base: string | undefined;

/** Tauri 2 custom schemes: `http://img.localhost/` on Windows (WebView2), `img://localhost/` elsewhere. */
function imgBase(): string {
  base ??= navigator.userAgent.includes("Windows") ? "http://img.localhost/" : "img://localhost/";
  return base;
}

/** Image URL for a valorant-api asset. The backend downloads and caches on first request. */
export function assetUrl(kind: AssetKind, id: string | number): string {
  if (import.meta.env.DEV && !isTauri()) return devCdnUrl(kind, id);
  return `${imgBase()}${kind}/${encodeURIComponent(String(id))}`;
}

/* DEV browser only (dead code in `vite build`): valorant-api's CDN directly. */
const DEV_CDN = "https://media.valorant-api.com";
/** Latest /v1/competitivetiers set (see ui/scripts/gen-demo-meta.mjs). */
const DEV_TIER_SET = "03621f52-342b-cf4e-4f86-9350a49c6d04";

function devCdnUrl(kind: AssetKind, id: string | number): string {
  switch (kind) {
    case "agent":
      return `${DEV_CDN}/agents/${id}/displayicon.png`;
    case "cardSmall":
      return `${DEV_CDN}/playercards/${id}/smallart.png`;
    case "cardWide":
      return `${DEV_CDN}/playercards/${id}/wideart.png`;
    case "tier":
      return `${DEV_CDN}/competitivetiers/${DEV_TIER_SET}/${id}/smallicon.png`;
    case "mapList":
      return `${DEV_CDN}/maps/${id}/listviewicon.png`;
    case "mapSplash":
      return `${DEV_CDN}/maps/${id}/splash.png`;
    case "chroma":
      return `${DEV_CDN}/weaponskinchromas/${id}/fullrender.png`;
  }
}
