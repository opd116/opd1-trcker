// Collection (design v2) helpers: the value in VP and SAR, the weapon filter rows, the edition counts and the grid's
// filter + sort. Pure functions, no DOM.
import { describe, expect, it } from "vitest";
import {
  ALL,
  artTint,
  editionColor,
  editionRows,
  fmtInt,
  priceTip,
  sar,
  sarText,
  skinName,
  skinNames,
  totalVp,
  visibleSkins,
  vpText,
  weaponRows,
} from "./skins";
import type { Edition, OwnedSkin } from "./types";

const skin = (name: string, weaponId: string, edition: Edition | null, vp: number | null): OwnedSkin => ({
  skinId: `s-${name}`,
  levelId: `l-${name}`,
  name,
  weaponId,
  edition,
  vp,
});

const ITEMS = [
  skin("Prime Vandal", "vandal", "premium", 1775),
  skin("Reaver Vandal", "vandal", "premium", 1775),
  skin("Elderflame Operator", "op", "ultra", 2475),
  skin("Prime Axe", "melee", "exclusive", 4350),
  skin("Free Classic", "classic", null, null),
];
const WEAPONS = [
  { id: "vandal", name: "Vandal" },
  { id: "op", name: "Operator" },
  { id: "melee", name: "Melee" },
  { id: "classic", name: "Classic" },
];

describe("value", () => {
  it("sums priced skins and converts at the 11,000 VP pack rate", () => {
    expect(totalVp(ITEMS)).toBe(10375);
    expect(fmtInt(107575)).toBe("107,575");
    expect(sar(107575)).toBe(3667); // the prototype's header
    expect(sar(4350)).toBe(148);
    expect(priceTip(1775)).toBe("1,775 VP ≈ 60.50 SAR at the 11,000 VP pack rate");
    expect(vpText(null)).toBe("— VP");
    expect(sarText(null)).toBe("");
  });
});

describe("weapon rows", () => {
  it("All weapons first, then by count, ties by name; bars against the most owned", () => {
    const rows = weaponRows(ITEMS, WEAPONS);
    expect(rows.map((r) => [r.name, r.n, r.w])).toEqual([
      ["All weapons", 5, 100],
      ["Vandal", 2, 100],
      ["Classic", 1, 50],
      ["Melee", 1, 50],
      ["Operator", 1, 50],
    ]);
    expect(rows[0]!.id).toBe(ALL);
  });

  it("a weapon missing from the list reads —", () => {
    expect(weaponRows([skin("X", "zzz", "select", 875)], [])[1]!.name).toBe("—");
  });
});

describe("editions", () => {
  it("counts every edition in store order with the design's colours", () => {
    const rows = editionRows(ITEMS);
    expect(rows.map((r) => [r.name, r.n])).toEqual([
      ["Select", 0],
      ["Deluxe", 0],
      ["Premium", 2],
      ["Exclusive", 1],
      ["Ultra", 1],
    ]);
    expect(editionColor("ultra")).toBe("#fad663");
    expect(editionColor(null)).toBe("#8a8f97");
    expect(artTint("select")).toBe("#5a9fe230");
  });
});

describe("visibleSkins", () => {
  it("sorts by value, then name; unpriced last", () => {
    expect(visibleSkins(ITEMS, ALL, ALL).map((s) => s.name)).toEqual([
      "Prime Axe",
      "Elderflame Operator",
      "Prime Vandal",
      "Reaver Vandal",
      "Free Classic",
    ]);
  });

  it("filters by weapon and edition together", () => {
    expect(visibleSkins(ITEMS, "vandal", ALL)).toHaveLength(2);
    expect(visibleSkins(ITEMS, ALL, "ultra").map((s) => s.name)).toEqual(["Elderflame Operator"]);
    expect(visibleSkins(ITEMS, "vandal", "ultra")).toEqual([]);
  });
});

describe("skinName", () => {
  it("adds the weapon to Rust's short name; melee names stay whole; an already-full name is kept", () => {
    expect(skinName("Reaver", "Vandal")).toBe("Reaver Vandal");
    expect(skinName("Reaver Karambit", "Melee")).toBe("Reaver Karambit");
    expect(skinName("Prime Vandal", "Vandal")).toBe("Prime Vandal");
    expect(skinName("Reaver", undefined)).toBe("Reaver");
    const names = skinNames([skin("Ion", "op", "premium", 1775)], [{ id: "op", name: "Operator" }]);
    expect(names.get("s-Ion")).toBe("Ion Operator");
  });

  it("the grid's name tie-break uses the full name", () => {
    const a = skin("Zeta", "a", "premium", 1775);
    const b = skin("Alpha", "b", "premium", 1775);
    const names = new Map([
      [a.skinId, "Aaa"],
      [b.skinId, "Bbb"],
    ]);
    expect(visibleSkins([b, a], ALL, ALL, names).map((s) => s.name)).toEqual(["Zeta", "Alpha"]);
  });
});
