// DEV tool: snapshot the valorant-api names/colors the demo feed needs.
// Mirrors the shape of the Tauri `meta()` command (ruling R21).
// Usage: node ui/scripts/gen-demo-meta.mjs  → ui/src/lib/demo/meta.json
import { writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const API = "https://valorant-api.com/v1";
const get = async (path) => {
  const res = await fetch(`${API}/${path}`);
  if (!res.ok) throw new Error(`${path}: HTTP ${res.status}`);
  return (await res.json()).data;
};

const [agents, maps, tierSets] = await Promise.all([
  get("agents?isPlayableCharacter=true"),
  get("maps"),
  get("competitivetiers"),
]);

const latest = tierSets[tierSets.length - 1];
const meta = {
  agents: Object.fromEntries(
    [...agents].sort((a, b) => a.displayName.localeCompare(b.displayName)).map((a) => [a.uuid, { name: a.displayName }]),
  ),
  maps: Object.fromEntries(
    [...maps].sort((a, b) => a.displayName.localeCompare(b.displayName)).map((m) => [m.uuid, { name: m.displayName }]),
  ),
  tiers: Object.fromEntries(
    latest.tiers.filter((t) => t.tier === 0 || t.tier >= 3).map((t) => [t.tier, { name: t.tierName, color: t.color }]),
  ),
};

const out = fileURLToPath(new URL("../src/lib/demo/meta.json", import.meta.url));
writeFileSync(out, JSON.stringify(meta, null, 2) + "\n");
console.log(`tier set ${latest.uuid} (keep DEV_TIER_SET in src/lib/assets.ts in sync)`);
console.log(`wrote ${out}: ${Object.keys(meta.agents).length} agents, ${Object.keys(meta.maps).length} maps, ${Object.keys(meta.tiers).length} tiers`);
