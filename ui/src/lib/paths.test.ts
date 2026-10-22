/// <reference types="node" />
// Test code builds file paths the Windows way too (the CI runner is windows-latest). A file URL's `pathname` is
// `/D:/a/…` on Windows, which fs resolves to `D:\D:\a\…` (the Task 18 CI failure, scenes.test.ts). Linux never shows
// it, so this scan is what catches it here: file URLs go to fs as URL objects or through `fileURLToPath`.
import { readdirSync, readFileSync } from "node:fs";
import { basename } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const UI = new URL("../../", import.meta.url);
const SKIP_DIRS = new Set(["node_modules", "dist", ".cache", "ref", "shots", "test-results"]);

const walk = (dir: URL): URL[] =>
  readdirSync(dir, { withFileTypes: true }).flatMap((e) =>
    e.isDirectory() ? (SKIP_DIRS.has(e.name) ? [] : walk(new URL(`${e.name}/`, dir))) : [new URL(e.name, dir)],
  );

/** Every file of test code and test/build config: unit tests, the Playwright harness and specs, configs, scripts. */
function testCode(): URL[] {
  const src = walk(new URL("src/", UI)).filter((u) => /\.test(-d)?\.ts$/.test(u.href));
  const tests = walk(new URL("tests/", UI)).filter((u) => u.href.endsWith(".ts"));
  const scripts = walk(new URL("scripts/", UI)).filter((u) => /\.m?[jt]s$/.test(u.href));
  const configs = ["playwright.config.ts", "vite.config.ts"].map((f) => new URL(f, UI));
  return [...src, ...tests, ...scripts, ...configs];
}

// Assembled from parts, so this file does not match itself.
const PATHNAME = new RegExp("\\." + "pathname\\b");
const STRING_OPS = new RegExp("\\b(dirname|join|resolve|normalize|basename|relative)\\(\\s*import\\.meta\\." + "url");
const FS_HREF = new RegExp("\\b(readFileSync|readdirSync|existsSync|statSync|writeFileSync|mkdirSync)\\([^)]*\\." + "href\\b");

/** `file:line  text` for every line that turns a file URL into a path by hand. */
function offenders(): string[] {
  const out: string[] = [];
  for (const u of testCode()) {
    const lines = readFileSync(u, "utf8").split("\n");
    lines.forEach((line, i) => {
      // An HTTP request's URL path (a Playwright route's `request().url()`) is not a file path.
      const httpPath = PATHNAME.test(line) && line.includes("request().url()");
      if ((PATHNAME.test(line) && !httpPath) || STRING_OPS.test(line) || FS_HREF.test(line)) {
        out.push(`${basename(fileURLToPath(u))}:${i + 1}  ${line.trim()}`);
      }
    });
  }
  return out;
}

describe("windows-safe paths in test code", () => {
  it("scans the unit tests, the Playwright harness and specs, and the configs", () => {
    const names = testCode().map((u) => basename(fileURLToPath(u)));
    for (const f of ["scenes.test.ts", "fonts.test.ts", "contract.test.ts", "harness.ts", "fit-v2.spec.ts", "vite.config.ts"]) {
      expect(names, f).toContain(f);
    }
  });

  it("no file URL becomes a path through its pathname, a string op on import.meta.url, or an href handed to fs", () => {
    expect(offenders()).toEqual([]);
  });
});
