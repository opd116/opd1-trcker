// The agent box is one rule in every stage (task 17 review F8): the component has no stage input, so Lobby and Closed
// (no agent can exist) render exactly what Agent Select renders before a lock. It is never the skeleton shape.
import { render } from "svelte/server";
import { describe, expect, it } from "vitest";
import AgentBox from "./AgentBox.svelte";

/** The markup with Svelte's scope hashes and hydration comments stripped, so two renders compare. */
const shape = (s: string) => s.replace(/ svelte-[a-z0-9]+/g, "").replace(/<!--.*?-->/g, "");
const html = (agentId: string | null) => shape(render(AgentBox, { props: { agentId } }).body);

describe("AgentBox", () => {
  it("no agent: an empty div, no image, and not the skeleton's .sk shape (the same in every stage)", () => {
    const h = html(null);
    expect(h).toBe('<div class="agent"></div>');
    expect(h).not.toContain("<img");
    expect(h).not.toContain("sk");
  });

  it("a locked agent: the head icon in the same box, with no alt text", () => {
    const h = html("add6443a-41bd-e414-f6ad-e58d267f4e95");
    expect(h).toMatch(/^<img class="agent" src="[^"]*add6443a-41bd-e414-f6ad-e58d267f4e95[^"]*" alt=""\/?>$/);
    expect(h).not.toContain("sk");
  });
});
