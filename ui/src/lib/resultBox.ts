// Result boxes (spec §1.5): W / L / D chips and E (an empty last-5 slot), read from the theme's CSS variables.
// `newest` picks the Arrow-style newest-tile variant; an empty slot has none. Used by last-5 tiles (here) and
// by the result chips of Details / Match (Task 18).
export type ResultKey = "W" | "L" | "D" | "E";

export function boxStyle(key: ResultKey, newest = false): string {
  const v = `--${newest && key !== "E" ? "rn" : "r"}-${key.toLowerCase()}`;
  return `background:var(${v}-bg);color:var(${v}-fg);border:var(${v}-bd)`;
}
