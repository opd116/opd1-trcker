// Skins / Collection screen (design v2, TrackerApp.dc.html 793–861; logic `skinCatalog` / `extraVals3`). Pure: the
// screen renders what these return from the real `Collection` (`invoke("collection")`). Prices are the standard store
// price per edition that Rust attaches (`OwnedSkin.vp`); the SAR figure is the design's 11,000 VP pack rate.
import { DASH } from "./format";
import { EDITION_COLORS } from "./theme";
import type { Edition, OwnedSkin } from "./types";

/** SAR per VP at the 11,000 VP pack (374.96 SAR), as in the prototype's `RATE`. */
export const VP_TO_SAR = 374.96 / 11000;

/** The editions, cheapest first (the order of the tier bar and its chips). */
export const EDITIONS: readonly Edition[] = ["select", "deluxe", "premium", "exclusive", "ultra"];

/** "Premium". */
export const editionLabel = (e: Edition): string => e.charAt(0).toUpperCase() + e.slice(1);

/** The design's edition colour; a skin with no edition gets the dim grey. */
export function editionColor(e: Edition | null): string {
  return e ? EDITION_COLORS[editionLabel(e) as keyof typeof EDITION_COLORS] : "#8a8f97";
}

/** "107,575". */
export const fmtInt = (n: number): string => Math.round(n).toLocaleString("en-US");

/** Whole SAR for an amount of VP. */
export const sar = (vp: number): number => Math.round(vp * VP_TO_SAR);

/** The sum of every priced skin's VP. */
export function totalVp(items: readonly OwnedSkin[]): number {
  return items.reduce((a, s) => a + (typeof s.vp === "number" ? s.vp : 0), 0);
}

/** The header value's tooltip (the prototype's `skValueTip`). */
export const VALUE_TIP =
  "Sum of the standard store price for each skin’s edition: Select 875, Deluxe 1,275, Premium 1,775, Exclusive 2,175, Ultra 2,475 VP (melee about double).\nNot what you spent: bundles, Night Market and Battle Pass skins cost less or nothing.\nNot a resale value: skins can’t be sold.";

/** A card's price tooltip: "1,775 VP ≈ 60.51 SAR at the 11,000 VP pack rate"; none without a price. */
export function priceTip(vp: number | null): string {
  if (typeof vp !== "number") return "Not sold for VP";
  return `${fmtInt(vp)} VP ≈ ${(vp * VP_TO_SAR).toFixed(2)} SAR at the 11,000 VP pack rate`;
}

export const vpText = (vp: number | null): string => (typeof vp === "number" ? `${fmtInt(vp)} VP` : `${DASH} VP`);
export const sarText = (vp: number | null): string => (typeof vp === "number" ? `≈ ${sar(vp)} SAR` : "");

// ── names ─────────────────────────────────────────────────────────────────────────────────────────────────────────
/**
 * A skin's full name as the design writes it ("Reaver Vandal"). Rust's `OwnedSkin.name` drops the weapon word
 * ("Reaver"; the weapon is `weaponId`), except for melee skins, whose names stay whole ("Reaver Karambit"); a name
 * that already ends with its weapon is left as it is.
 */
export function skinName(name: string, weapon: string | null | undefined): string {
  if (!weapon || weapon === "Melee" || name.toLowerCase().endsWith(weapon.toLowerCase())) return name;
  return `${name} ${weapon}`;
}

/** Every skin's full name, by skin id, from the collection's weapon list. */
export function skinNames(items: readonly OwnedSkin[], weapons: readonly { id: string; name: string }[]): Map<string, string> {
  const w = new Map(weapons.map((x) => [x.id, x.name]));
  return new Map(items.map((s) => [s.skinId, skinName(s.name, w.get(s.weaponId))]));
}

// ── the weapon filter ─────────────────────────────────────────────────────────────────────────────────────────────
export const ALL = "all";

export interface WeaponRow {
  /** A weapon uuid, or `ALL`. */
  id: string;
  name: string;
  n: number;
  /** Bar width, 0–100: against the total for All, against the most owned weapon otherwise. */
  w: number;
}

/** "All weapons" first, then every weapon the user owns a skin for, most skins first (ties by name). */
export function weaponRows(items: readonly OwnedSkin[], weapons: readonly { id: string; name: string }[]): WeaponRow[] {
  const names = new Map(weapons.map((w) => [w.id, w.name]));
  const counts = new Map<string, number>();
  for (const s of items) counts.set(s.weaponId, (counts.get(s.weaponId) ?? 0) + 1);
  const max = Math.max(1, ...counts.values());
  const rows = [...counts.entries()]
    .map(([id, n]) => ({ id, name: names.get(id) ?? DASH, n, w: Math.round((n / max) * 100) }))
    .sort((a, b) => b.n - a.n || a.name.localeCompare(b.name));
  return [{ id: ALL, name: "All weapons", n: items.length, w: items.length ? 100 : 0 }, ...rows];
}

// ── the edition card ──────────────────────────────────────────────────────────────────────────────────────────────
export interface EditionRow {
  edition: Edition;
  name: string;
  n: number;
  color: string;
}

export function editionRows(items: readonly OwnedSkin[]): EditionRow[] {
  return EDITIONS.map((e) => ({ edition: e, name: editionLabel(e), n: items.filter((s) => s.edition === e).length, color: editionColor(e) }));
}

// ── the grid ──────────────────────────────────────────────────────────────────────────────────────────────────────
/** The skins a filter pair keeps, by value (highest VP first, unpriced last), then full name (`names`, else `name`). */
export function visibleSkins(
  items: readonly OwnedSkin[],
  weapon: string,
  edition: Edition | typeof ALL,
  names: ReadonlyMap<string, string> = new Map(),
): OwnedSkin[] {
  const n = (s: OwnedSkin) => names.get(s.skinId) ?? s.name;
  return items
    .filter((s) => (weapon === ALL || s.weaponId === weapon) && (edition === ALL || s.edition === edition))
    .sort((a, b) => (b.vp ?? -1) - (a.vp ?? -1) || n(a).localeCompare(n(b)));
}

/** The radial tint behind a card's art: the edition colour at alpha 0x30 (the prototype's `TIERC[t] + '30'`). */
export const artTint = (e: Edition | null): string => `${editionColor(e)}30`;
