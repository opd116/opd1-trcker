/// <reference types="node" />
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { PALETTE } from "./theme";

const SRC = new URL("../", import.meta.url);
const FONTS = new URL("assets/fonts/", SRC);

/** [family, file slug, weight, style]: exactly what design v2 uses. */
const FACES: [string, string, number, "normal" | "italic"][] = [
  ...[400, 500, 600, 700].map((w) => ["Barlow", "barlow", w, "normal"] as [string, string, number, "normal"]),
  ["Barlow", "barlow", 500, "italic"],
  ...[500, 600, 700].map((w) => ["Saira Condensed", "saira-condensed", w, "normal"] as [string, string, number, "normal"]),
  ...[400, 500, 600].map((w) => ["Geist Mono", "geist-mono", w, "normal"] as [string, string, number, "normal"]),
];
const FILES = FACES.map(([, slug, w, st]) => `${slug}-latin-${w}-${st}.woff2`);

const walk = (dir: URL): URL[] =>
  readdirSync(dir, { withFileTypes: true }).flatMap((e) => (e.isDirectory() ? walk(new URL(`${e.name}/`, dir)) : [new URL(e.name, dir)]));

describe("bundled fonts (design v2)", () => {
  it("the 11 woff2 files exist and total at most 250 KB", () => {
    expect(FILES).toHaveLength(11);
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

  it("no other font files are shipped (IBM Plex and Archivo are gone)", () => {
    const woff = readdirSync(FONTS).filter((f) => /\.(woff2?|ttf|otf)$/.test(f));
    expect(woff.sort()).toEqual([...FILES].sort());
  });

  it("every file is a real woff2", () => {
    for (const f of FILES) expect(readFileSync(new URL(f, FONTS)).subarray(0, 4).toString("latin1"), f).toBe("wOF2");
  });

  it("OFL.txt carries all three families' licence texts", () => {
    const ofl = readFileSync(new URL("OFL.txt", FONTS), "utf8");
    for (const name of ["Barlow", "Saira", "Geist"]) expect(ofl, name).toContain(name);
    expect(ofl).not.toMatch(/IBM Plex|Archivo/);
    expect((ofl.match(/SIL OPEN FONT LICENSE Version 1\.1/g) ?? []).length).toBeGreaterThanOrEqual(3);
  });
});

describe("app.css", () => {
  const css = readFileSync(new URL("app.css", SRC), "utf8");
  const blocks = css.match(/@font-face\s*\{[^}]*\}/g) ?? [];

  it("has exactly one @font-face per bundled file, all font-display: block", () => {
    expect(blocks).toHaveLength(FILES.length);
    for (const b of blocks) expect(b).toMatch(/font-display:\s*block/);
    for (const f of FILES) expect(css, f).toContain(f);
  });

  it("declares each family in exactly the design's weights", () => {
    const seen = blocks.map((b) => {
      const family = /font-family:\s*"([^"]+)"/.exec(b)?.[1];
      const weight = Number(/font-weight:\s*(\d+)/.exec(b)?.[1]);
      const style = /font-style:\s*(\w+)/.exec(b)?.[1];
      return `${family}|${weight}|${style}`;
    });
    expect(seen.sort()).toEqual(FACES.map(([f, , w, st]) => `${f}|${w}|${st}`).sort());
  });

  it("has no @import and the design's global rules", () => {
    expect(css).not.toMatch(/@import/);
    expect(css).toMatch(/html,\s*body\s*\{[^}]*margin:\s*0;[^}]*height:\s*100%;[^}]*background:\s*var\(--bg/);
    for (const k of ["opdPulse", "opdSweep", "opdShimmer"]) expect(css).toContain(`@keyframes ${k}`);
    expect(css).toMatch(/::-webkit-scrollbar\s*\{[^}]*width:\s*10px/);
    expect(css).toMatch(/button\s*\{[^}]*font:\s*inherit/);
    expect(css).toMatch(/::placeholder/);
    // The design's border-box sizing is the one reset (TrackerApp.dc.html line 17).
    expect(css).toMatch(/box-sizing:\s*border-box/);
  });

  it("the html,body background fallback is the palette's --bg (no drift from theme.ts)", () => {
    const fallback = /html,\s*body\s*\{[^}]*background:\s*var\(--bg,\s*(#[0-9a-fA-F]{6})\)/.exec(css)?.[1];
    expect(fallback, "html,body needs a var(--bg, #rrggbb) fallback").toBeDefined();
    expect(fallback!.toLowerCase()).toBe(PALETTE.bg);
  });

  it("the Tauri window's backgroundColor is the palette's --bg, so there is no white flash before first paint", () => {
    const conf = JSON.parse(readFileSync(new URL("../../../src-tauri/tauri.conf.json", import.meta.url), "utf8")) as {
      app: { windows: { backgroundColor?: string }[] };
    };
    expect(conf.app.windows[0]!.backgroundColor?.toLowerCase()).toBe(PALETTE.bg);
  });

  it("has the design's keyboard-only focus ring and nothing that shows on mouse focus", () => {
    expect(css).toMatch(/:focus-visible\s*\{[^}]*outline:\s*2px solid var\(--focus, #b7abff\)[^}]*outline-offset:\s*2px/);
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
