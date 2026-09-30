/// <reference types="node" />
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { DEFAULT_THEME, TONES } from "./theme";

const SRC = new URL("../", import.meta.url);
const FONTS = new URL("assets/fonts/", SRC);

const FAMILIES = ["barlow", "ibm-plex-sans", "archivo"];
const FILES = FAMILIES.flatMap((f) => [500, 600, 700].map((w) => `${f}-latin-${w}-normal.woff2`).concat(`${f}-latin-500-italic.woff2`));

const walk = (dir: URL): URL[] =>
  readdirSync(dir, { withFileTypes: true }).flatMap((e) => (e.isDirectory() ? walk(new URL(`${e.name}/`, dir)) : [new URL(e.name, dir)]));

describe("bundled fonts (spec §1.7)", () => {
  it("the 12 woff2 files exist and total at most 250 KB", () => {
    expect(FILES).toHaveLength(12);
    let total = 0;
    for (const f of FILES) {
      const u = new URL(f, FONTS);
      expect(existsSync(u), f).toBe(true);
      const size = statSync(u).size;
      expect(size, f).toBeGreaterThan(1000);
      total += size;
    }
    expect(total).toBeLessThanOrEqual(250 * 1024);
  });

  it("no other font files are shipped (the semi-condensed set is gone)", () => {
    const woff = readdirSync(FONTS).filter((f) => f.endsWith(".woff2"));
    expect(woff.sort()).toEqual([...FILES].sort());
  });

  it("every file is a real woff2", () => {
    for (const f of FILES) expect(readFileSync(new URL(f, FONTS)).subarray(0, 4).toString("latin1"), f).toBe("wOF2");
  });

  it("OFL.txt carries all three families' licence texts", () => {
    const ofl = readFileSync(new URL("OFL.txt", FONTS), "utf8");
    for (const name of ["Barlow", "IBM Plex", "Archivo"]) expect(ofl, name).toContain(name);
    expect((ofl.match(/SIL OPEN FONT LICENSE Version 1\.1/g) ?? []).length).toBeGreaterThanOrEqual(3);
  });
});

describe("app.css", () => {
  const css = readFileSync(new URL("app.css", SRC), "utf8");

  it("has exactly 12 @font-face blocks, all font-display: block", () => {
    const blocks = css.match(/@font-face\s*\{[^}]*\}/g) ?? [];
    expect(blocks).toHaveLength(12);
    for (const b of blocks) expect(b).toMatch(/font-display:\s*block/);
    for (const f of FILES) expect(css, f).toContain(f);
    expect(css).not.toContain("barlow-semi-condensed");
  });

  it("declares the three families with weights 500/600/700 and italic 500", () => {
    const blocks = css.match(/@font-face\s*\{[^}]*\}/g) ?? [];
    const seen = new Set(
      blocks.map((b) => {
        const family = /font-family:\s*"([^"]+)"/.exec(b)?.[1];
        const weight = /font-weight:\s*(\d+)/.exec(b)?.[1];
        const style = /font-style:\s*(\w+)/.exec(b)?.[1];
        return `${family}|${weight}|${style}`;
      }),
    );
    for (const family of ["Barlow", "IBM Plex Sans", "Archivo"]) {
      for (const w of ["500", "600", "700"]) expect(seen, `${family} ${w}`).toContain(`${family}|${w}|normal`);
      expect(seen, `${family} italic`).toContain(`${family}|500|italic`);
    }
  });

  it("has no @import and only the design's global rules", () => {
    expect(css).not.toMatch(/@import/);
    expect(css).toMatch(/html,\s*body\s*\{[^}]*margin:\s*0;[^}]*height:\s*100%;[^}]*background:\s*var\(--bg/);
    // no CSS reset (spec §0)
    expect(css).not.toMatch(/box-sizing/);
    expect(css).not.toMatch(/line-height/);
    expect(css).not.toMatch(/(^|\n)\s*(\*|img)\s*[,{]/);
  });

  it("the html,body background fallback is the default tone's --bg (no drift from theme.ts)", () => {
    const fallback = /html,\s*body\s*\{[^}]*background:\s*var\(--bg,\s*(#[0-9a-fA-F]{6})\)/.exec(css)?.[1];
    expect(fallback, "html,body needs a var(--bg, #rrggbb) fallback").toBeDefined();
    expect(fallback!.toLowerCase()).toBe(TONES[DEFAULT_THEME.tone].bg.toLowerCase());
  });

  it("has the keyboard-only focus ring (R106) and nothing that shows on mouse focus", () => {
    expect(css).toMatch(/:focus-visible\s*\{[^}]*outline:\s*1px solid[^}]*outline-offset:\s*2px/);
    expect(css).toMatch(/var\(--accent\)/);
    expect(css).not.toMatch(/(^|\n)\s*:focus\s*\{/);
  });
});

describe("no CDN at runtime", () => {
  // The banned hosts are assembled from parts so this file does not match itself.
  const banned = ["fonts." + "googleapis", "fonts." + "gstatic", "unp" + "kg"];

  it("no font CDN host is named anywhere under ui/src", () => {
    const files = walk(SRC).filter((u) => !/\.(woff2?|png|jpe?g|ico)$/.test(u.href));
    expect(files.length).toBeGreaterThan(10);
    for (const u of files) {
      const text = readFileSync(u, "utf8");
      for (const b of banned) expect(text.includes(b), `${fileURLToPath(u)} mentions ${b}`).toBe(false);
    }
  });
});
