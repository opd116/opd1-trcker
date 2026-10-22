// DEV-only (R189): `collection` from real valorant-api uuids (weapon, skin, first level and edition checked against
// /v1/weapons), a sample of the design's `skinCatalog()`. Names are in the backend's format: the weapon word is dropped
// ("Reaver", the Vandal comes from `weaponId`), except melee skins, which keep their whole name ("Reaver Karambit").
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
  Bulldog: "ae3de142-4d85-2547-dd26-4e90bed35cf7",
  Odin: "63e6c2b6-4a8e-869c-3d4c-e38355226584",
  Melee: "2f59173c-4bed-b6c3-2191-dea9b58be9c7",
} as const;

const PRICE: Record<Edition, number> = { select: 875, deluxe: 1275, premium: 1775, exclusive: 2175, ultra: 2475 };

// [skin uuid, first level uuid, name (backend format), weapon, edition]
const RAW: [string, string, string, keyof typeof W, Edition][] = [
  ["18609205-4edb-5966-cff8-0fba0230ba1e", "b3d3ff38-4202-20d8-2f41-c783477e5636", "Elderflame", "Vandal", "ultra"],
  ["74789f33-4632-8052-96d7-258538721a32", "7e44fc1b-44fa-cdda-8491-f8a5bca1cfa3", "Glitchpop", "Vandal", "exclusive"],
  ["30388628-42f0-606c-82c0-73ad43de997f", "ba42fe63-457a-78ce-4499-47950a698129", "Reaver", "Vandal", "premium"],
  ["b9ee2457-481c-6776-3f5b-0ca8e8f90c89", "c9678d8c-4327-f397-b0ec-dca3c3d6fb15", "Prime", "Vandal", "premium"],
  ["980fa063-436e-e51f-c38d-70a5b93a0f1c", "82db01d1-4192-167b-9f53-78ba374c39ac", "Spectrum", "Phantom", "exclusive"],
  ["36791b03-452d-8dad-0091-898cc28d2196", "c00e786e-4e6f-0ef7-0ce3-32ba9918ba41", "Oni", "Phantom", "premium"],
  ["e86bf7e4-4dd3-fbee-533b-fa875344bbaf", "b8bb264c-4578-2410-8dfa-6aacfeb318b0", "Ion", "Phantom", "premium"],
  ["d722313d-43cb-b38d-7841-75880a3ed2cb", "5c273d0e-47fa-bb8c-d914-728de95da30e", "Elderflame", "Operator", "ultra"],
  ["17831113-4ff0-a6c9-0b20-6f9c077d74a2", "4d19c241-4350-6658-f5a6-5c99ca8e5e99", "Origin", "Operator", "premium"],
  ["0eec6f2b-4d64-9c16-7846-b8865030f61c", "72b3bacc-48ac-85f7-ec38-5ab629654486", "Kuronami", "Sheriff", "exclusive"],
  ["a40a6ce2-462c-c864-5d30-7b9408b98d3d", "4e4ebb8d-41d0-c326-595a-1f9b257e91fa", "Reaver", "Sheriff", "premium"],
  ["20807bd8-4259-35e5-e54e-c1b214f58cc8", "d1d528ae-4dcc-e693-68e2-e8a475df83a4", "Prime", "Spectre", "premium"],
  ["e78fb82c-4800-e102-b7a6-33946fa2f199", "303530db-47fe-0e6e-5ea9-0cb5ed61758d", "Task Force 809", "Spectre", "deluxe"],
  ["d653f4a7-4e92-2559-0a97-2c9d46d009b3", "c7695ce7-4fc9-1c79-64b3-8c8f9e21571c", "Prime", "Classic", "premium"],
  ["ea9da25c-422d-d256-a970-cbaeb55542bf", "02cf19c4-4f91-3b66-3368-7ebcabe89e3e", "Resolution", "Classic", "select"],
  ["431799d6-4e79-bdf2-573d-4cbc8112982e", "ca55e031-4ad3-19ab-7c39-42840e4d6f4a", "Kuronami", "Ghost", "exclusive"],
  ["d0fbfdee-4961-cf51-24a4-4a853ee9fd0c", "434f1619-426e-37af-2d7a-ac8ad80706d3", "Flutter", "Ghost", "select"],
  ["9a107ffd-462d-9fc1-241d-759fbe79c02d", "339b6c4d-4584-4c33-fe8f-54ac97baca44", "Frequency", "Bulldog", "select"],
  ["89be9866-4807-6235-2a95-499cd23828df", "578e9077-4f88-260c-e54c-b988425c60e4", "Altitude", "Odin", "deluxe"],
  ["b73d7b16-4652-bc5b-5c4c-068aabb19d0a", "5ac106cd-45ef-a26f-2058-f382f20c64db", "Reaver Karambit", "Melee", "exclusive"],
  ["e100dff1-4cf5-54ec-aa65-6fadbc22973b", "249b0e46-4a11-f045-51bb-649151cd802a", "Prime Axe", "Melee", "exclusive"],
  ["94b40026-4efb-39ea-69d7-fca60be39c56", "f3594bcc-43a9-4a74-8c40-98a4e4a4569a", "Elderflame Dagger", "Melee", "exclusive"],
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
