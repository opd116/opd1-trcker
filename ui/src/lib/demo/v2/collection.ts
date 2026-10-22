// DEV-only (R189): `collection` from a small sample of real valorant-api uuids (the design's `skinCatalog()` names).
import type { Collection, Edition, OwnedSkin } from "../../types";
import { DEMO_NOW } from "../cdv8";

const W = {
  Vandal: "9c82e19d-4575-0200-1a81-3eacf00cf872",
  Phantom: "ee8e8d15-496b-07ac-e5f6-8fae5d4c7b1a",
  Operator: "a03b24d3-4319-996d-0f8c-94bbfba1dfc7",
  Sheriff: "e336c6b8-418d-9340-d77f-7a9e4cfe0702",
  Spectre: "462080d1-4035-2937-7c09-27aa2a5c27a7",
  Classic: "29a0cfab-485b-f5d5-779a-b59f85e204a8",
  Ghost: "1baa85b4-4c70-1284-64bb-6481dfc3bb4e",
  Melee: "2f59173c-4bed-b6c3-2191-dea9b58be9c7",
} as const;

const PRICE: Record<Edition, number> = { select: 875, deluxe: 1275, premium: 1775, exclusive: 2175, ultra: 2475 };

// [skin uuid, first level uuid, name, weapon, edition]
const RAW: [string, string, string, keyof typeof W, Edition][] = [
  ["2bd9c7d5-43a7-3c5f-2a19-52b8c4b3f88e", "578e9077-4f88-260c-e54c-b988425c60e4", "Reaver Vandal", "Vandal", "premium"],
  ["9c9f0c63-42ec-1f8f-8d2f-6aa4cce6b1a1", "b2b1d4a1-4dfe-3a2e-3c2d-95b0d5c2a2e8", "Prime Vandal", "Vandal", "premium"],
  ["c7695ce7-4fc9-1c79-64b3-8c8f9e21571c", "c5ad6c1b-4f7a-8ab2-23d4-34b6d3c0a29d", "Glitchpop Phantom", "Phantom", "exclusive"],
  ["b7c6b6dc-4d8b-2d1e-5a3a-a8ae7c04a2ab", "17bd4ba3-4e9e-1a6b-0a6c-5ab4d0fb3c26", "Elderflame Operator", "Operator", "ultra"],
  ["8a1e3c26-4d5b-1e1b-0d65-33b1e3f2a6c0", "a3f8ad2b-4b8b-6d3e-9e0f-c4f8c3f1b2d1", "Ion Sheriff", "Sheriff", "premium"],
  ["1d7b6c5d-4f2a-7c1e-2b3d-6e0b2a7c1f9e", "e4b1c2d3-4a5b-6c7d-8e9f-0a1b2c3d4e5f", "Oni Phantom", "Phantom", "premium"],
  ["a2c3d4e5-4f60-7182-93a4-b5c6d7e8f901", "b3d4e5f6-4071-8293-a4b5-c6d7e8f90a12", "Sovereign Ghost", "Ghost", "premium"],
  ["c4d5e6f7-4081-92a3-b4c5-d6e7f8091a2b", "d5e6f708-4192-a3b4-c5d6-e7f8091a2b3c", "Spline Spectre", "Spectre", "select"],
  ["e6f70819-42a3-b4c5-d6e7-f8091a2b3c4d", "f708192a-43b4-c5d6-e7f8-091a2b3c4d5e", "Magepunk Classic", "Classic", "deluxe"],
  ["08192a3b-44c5-d6e7-f809-1a2b3c4d5e6f", "192a3b4c-45d6-e7f8-091a-2b3c4d5e6f70", "Prime Karambit", "Melee", "exclusive"],
];

export function demoCollection(): Collection {
  const items: OwnedSkin[] = RAW.map(([skinId, levelId, name, weapon, edition]) => ({
    skinId,
    levelId,
    name,
    weaponId: W[weapon],
    edition,
    vp: weapon === "Melee" ? PRICE[edition] * 2 : PRICE[edition],
  }));
  return { items, weapons: Object.entries(W).map(([name, id]) => ({ id, name })), fetchedMs: DEMO_NOW - 3_600_000 };
}
