import { describe, expect, it } from "vitest";
import {
  artSize,
  asciiGrid,
  asciiText,
  canvasFontPx,
  cellAt,
  composeCanvas,
  filterText,
  loadRaw,
  paintAt,
  PRESETS,
  tileFontPx,
  trimArt,
} from "./ascii";

describe("asciiGrid / asciiText: the 5×5 font", () => {
  it("Flat: one character per pixel, glyphs one column apart", () => {
    expect(asciiText("HI", "#", "flat")).toBe(["#   # ###", "#   #  #", "#####  #", "#   #  #", "#   # ###"].join("\n"));
    const g = asciiGrid("HI", "#", "flat");
    expect(g).toHaveLength(5);
    expect(g[0]).toHaveLength(5 + 1 + 3);
  });

  it("Wide: every column doubled", () => {
    expect(asciiText("I", "█", "wide")).toBe(["██████", "  ██", "  ██", "  ██", "██████"].join("\n"));
  });

  it("Shadow: wide plus a ░ copy one down and right, under the ink", () => {
    const g = asciiGrid(".", "█", "shadow");
    expect(g).toHaveLength(6);
    expect(g[0]).toHaveLength(3);
    expect(g.map((r) => r.join(""))).toEqual(["   ", "   ", "   ", "   ", "██ ", " ░░"]);
    // Ink wins over shadow where they overlap.
    const i = asciiGrid("I", "█", "shadow").map((r) => r.join(""));
    expect(i[0]).toBe("██████ ");
    expect(i[1]).toBe(" ░██░░░");
    expect(i[5]).toBe(" ░░░░░░");
  });

  it("lower case reads as upper case; an unknown character draws ?; empty text is empty", () => {
    expect(asciiText("ace", "#", "flat")).toBe(asciiText("ACE", "#", "flat"));
    expect(asciiText("~", "#", "flat")).toBe(asciiText("?", "#", "flat"));
    expect(asciiGrid("", "#", "flat")).toEqual([]);
    expect(asciiText("", "#", "flat")).toBe("");
  });
});

describe("filterText: the input", () => {
  it("keeps a-z 0-9 space ! ? . - only, at most 10", () => {
    expect(filterText("gg_wp!")).toBe("ggwp!");
    expect(filterText("1v5 #clutch")).toBe("1v5 clutch");
    expect(filterText("a-b.c?d")).toBe("a-b.c?d");
    expect(filterText("ABCDEFGHIJKLMNOP")).toBe("ABCDEFGHIJ");
    expect(filterText("é😀")).toBe("");
  });
});

describe("trimArt", () => {
  it("drops blank rows above and below, the common indent and trailing spaces", () => {
    const g = ["      ", "  ab  ", "   c  ", "      "].map((r) => [...r]);
    expect(trimArt(g)).toBe("ab\n c");
  });
  it("a blank grid is empty", () => {
    expect(trimArt([[" ", " "], [" ", " "]])).toBe("");
  });
});

describe("composeCanvas", () => {
  it("is max(64, art + 8) wide and 15 tall, the art centred", () => {
    const c = composeCanvas("CLUTCH", "█", "shadow", {});
    const aw = asciiGrid("CLUTCH", "█", "shadow")[0]!.length;
    expect(c.W).toBe(Math.max(64, aw + 8));
    expect(c.H).toBe(15);
    expect(c.out.split("\n")).toHaveLength(15);
    expect(c.out.split("\n").every((l) => l.length === c.W)).toBe(true);
    expect(c.art).toBe(asciiText("CLUTCH", "█", "shadow"));

    const small = composeCanvas("A", "#", "flat", {});
    expect(small.W).toBe(64);
    expect(small.grid[7]!.join("").indexOf("#")).toBe(Math.floor((64 - 5) / 2));

    const long = composeCanvas("ABCDEFGHIJ", "#", "shadow", {});
    expect(long.W).toBe(asciiGrid("ABCDEFGHIJ", "#", "shadow")[0]!.length + 8);
  });

  it("paint is keyed from the centre and drawn on top; off-canvas strokes are ignored", () => {
    const c = composeCanvas("", "#", "flat", { "0,0": "*", "-32,-7": "+", "99,0": "@" });
    expect(c.W).toBe(64);
    expect(c.cx).toBe(32);
    expect(c.cy).toBe(7);
    expect(c.grid[7]![32]).toBe("*");
    expect(c.grid[0]![0]).toBe("+");
    expect(c.out).not.toContain("@");
    expect(c.art).toBe(["+", ...Array(6).fill(""), " ".repeat(32) + "*"].join("\n"));
  });

  it("strokes stay put relative to the centre when the text grows", () => {
    const paint = { "3,-6": "·" };
    for (const text of ["A", "ABCDEFGHIJ"]) {
      const c = composeCanvas(text, "█", "shadow", paint);
      expect(c.grid[c.cy - 6]![c.cx + 3]).toBe("·");
    }
  });

  it("an erase stroke (' ') clears an ink cell", () => {
    const base = composeCanvas("I", "#", "flat", {});
    const y = base.grid.findIndex((r) => r.includes("#"));
    const x = base.grid[y]!.indexOf("#");
    const erased = composeCanvas("I", "#", "flat", paintAt({}, x, y, base, " "));
    expect(erased.grid[y]![x]).toBe(" ");
  });
});

describe("painting helpers", () => {
  const c = { W: 64, H: 15, cx: 32, cy: 7 };
  it("cellAt maps a pointer to a cell", () => {
    const rect = { left: 100, top: 50, width: 640, height: 300 };
    expect(cellAt(100, 50, rect, c)).toEqual([0, 0]);
    expect(cellAt(739, 349, rect, c)).toEqual([63, 14]);
    expect(cellAt(425, 200, rect, c)).toEqual([32, 7]);
  });
  it("paintAt returns the same map for an off-canvas or unchanged cell", () => {
    const m = { "0,0": "█" };
    expect(paintAt(m, -1, 0, c, "#")).toBe(m);
    expect(paintAt(m, 64, 0, c, "#")).toBe(m);
    expect(paintAt(m, 32, 7, c, "█")).toBe(m);
    expect(paintAt(m, 33, 7, c, "#")).toEqual({ "0,0": "█", "1,0": "#" });
  });
  it("loadRaw centres a drawing on the 64 × 15 canvas and round-trips through composeCanvas", () => {
    const raw = PRESETS.find((p) => p.title === "Crosshair")!.raw;
    const out = composeCanvas("", "█", "shadow", loadRaw(raw));
    expect(out.art).toBe(trimArt(raw.split("\n").map((l) => [...l])));
    expect(Object.values(loadRaw("a b")).sort()).toEqual(["a", "b"]);
  });
});

describe("sizes", () => {
  it("canvas font: 770px / (W × .6), clamped 7–20", () => {
    expect(canvasFontPx(64)).toBe(20);
    expect(canvasFontPx(100)).toBe(12);
    expect(canvasFontPx(500)).toBe(7);
  });
  it("tile font: fits 190 × 96, clamped 4–13", () => {
    expect(tileFontPx("ab\ncd")).toBe(13);
    expect(tileFontPx("x".repeat(60))).toBe(5);
    expect(tileFontPx("x".repeat(200))).toBe(4);
    expect(tileFontPx(Array(16).fill("x").join("\n"))).toBe(6);
  });
  it("artSize", () => {
    expect(artSize("ab\nabcd\n")).toEqual({ cols: 4, rows: 3 });
    expect(artSize("")).toEqual({ cols: 0, rows: 0 });
  });
});

describe("PRESETS", () => {
  it("are the prototype's six, in order", () => {
    expect(PRESETS.map((p) => p.title)).toEqual(["ACE", "GG", "1V5", "DEFUSED", "OPD1", "Crosshair"]);
    const ace = PRESETS[0]!;
    expect(ace.text && { char: ace.char, effect: ace.effect }).toEqual({ char: "█", effect: "shadow" });
    expect(ace.raw).toBe(asciiText("ACE", "█", "shadow"));
    expect(PRESETS[3]!.raw).toBe(asciiText("DEFUSED", "#", "flat"));
  });
});
