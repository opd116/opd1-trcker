// The Session panel's chips on My Stats (spec §9.3): one per match, oldest first, the plain 20×20 result boxes. With a way
// to open the match each is a button and a focus stop; without one it is plain (no role, no tabindex), as a plain
// Recent Matches row is. The tip's text and the click itself are in the Playwright suite (the action only runs in a browser).
import { render } from "svelte/server";
import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";
import demoMeta from "../../lib/demo/meta.json";
import { myStatsFull, myStatsLongnames, myStatsUnranked } from "../../lib/demo/mystats";
import type { Meta, SessionSummary } from "../../lib/types";

// The panel pulls in the theme store, which reads `location` and paints CSS variables when it loads (see LastMatches.test.ts).
let SessionPanel: typeof import("./SessionPanel.svelte").default;
let setMeta: (m: Meta | null) => void;
beforeAll(async () => {
  vi.stubGlobal("location", { search: "" });
  vi.stubGlobal("document", { documentElement: { style: { setProperty: vi.fn() } }, getElementById: () => ({ setAttribute: vi.fn() }) });
  SessionPanel = (await import("./SessionPanel.svelte")).default;
  const { meta } = await import("../../lib/stores");
  setMeta = (m) => meta.set(m);
  setMeta(demoMeta as Meta);
});
afterAll(() => {
  setMeta(null);
  vi.unstubAllGlobals();
});

const clean = (h: string) => h.replace(/ svelte-[a-z0-9]+/g, "").replace(/<!--.*?-->/g, "");
const html = (session: SessionSummary | null | undefined, onopen?: (id: string) => void) => clean(render(SessionPanel, { props: { session, onopen } }).body);
const count = (h: string, needle: string) => h.split(needle).length - 1;
/** The letters of the chips, in document order. */
const letters = (h: string) => [...h.matchAll(/class="chip s20"[^>]*>([WLD])</g)].map((m) => m[1]);

describe("Session chips", () => {
  const session = myStatsFull().session!;

  it("one chip per match, oldest first, W / L / D", () => {
    expect(letters(html(session, () => {}))).toEqual(session.matches.map((m) => m.result));
    const long = myStatsLongnames().session!;
    expect(letters(html(long, () => {}))).toEqual(long.matches.map((m) => m.result));
    expect(letters(html(myStatsUnranked().session!))).toHaveLength(3);
  });

  it("a chip that opens a match is a button and a focus stop, with the pointer", () => {
    const h = html(session, () => {});
    expect(count(h, 'role="button"')).toBe(session.matches.length);
    expect(count(h, 'tabindex="0"')).toBe(session.matches.length);
    expect(count(h, 'class="mc open"')).toBe(session.matches.length);
  });

  it("an openable chip is named by its result and tip (its text is only a letter); a plain one has no name of its own", () => {
    const h = html(session, () => {});
    expect(count(h, "aria-label=")).toBe(session.matches.length);
    expect([...h.matchAll(/aria-label="([^"]*)"/g)].map((x) => x[1]!.split(" · ")[0])).toEqual(
      session.matches.map((m) => ({ W: "Win", L: "Loss", D: "Draw" })[m.result]),
    );
    expect(count(html(session), "aria-label=")).toBe(0);
  });

  it("a plain chip (nothing to open it with) has no role, no focus stop and no pointer class", () => {
    const h = html(session);
    expect(count(h, 'role="button"')).toBe(0);
    expect(count(h, "tabindex")).toBe(0);
    expect(h).not.toContain("mc open");
    expect(count(h, 'class="mc"')).toBe(session.matches.length);
    expect(letters(h)).toEqual(session.matches.map((m) => m.result));
  });

  it("no session: the frame with dashes and no chips; loading: the skeleton and no chips", () => {
    for (const s of [null, undefined]) {
      const h = html(s, () => {});
      expect(count(h, 'class="mc'), String(s)).toBe(0);
      expect(count(h, 'role="button"'), String(s)).toBe(0);
    }
    expect(html(null)).toContain("—");
  });
});
