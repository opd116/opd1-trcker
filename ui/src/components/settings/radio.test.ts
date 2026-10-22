import { describe, expect, it } from "vitest";
import { radioTabIndex, radioTarget } from "./radio";

describe("radio groups", () => {
  it("arrows wrap, Home / End jump, other keys do nothing", () => {
    expect(radioTarget("ArrowRight", 0, 3)).toBe(1);
    expect(radioTarget("ArrowDown", 2, 3)).toBe(0);
    expect(radioTarget("ArrowLeft", 0, 3)).toBe(2);
    expect(radioTarget("ArrowUp", 1, 3)).toBe(0);
    expect(radioTarget("Home", 2, 3)).toBe(0);
    expect(radioTarget("End", 0, 3)).toBe(2);
    expect(radioTarget("a", 0, 3)).toBeNull();
    expect(radioTarget("ArrowRight", 0, 0)).toBeNull();
  });
  it("one tab stop: the checked radio, else the first", () => {
    expect([0, 1, 2].map((i) => radioTabIndex(i, 1))).toEqual([-1, 0, -1]);
    expect([0, 1, 2].map((i) => radioTabIndex(i, -1))).toEqual([0, -1, -1]);
  });
});
