// A separate, DEV-only study. Production keeps the existing entry and data feeds.
if (import.meta.env.DEV) {
  const [{ mount }, { default: Study }] = await Promise.all([import("svelte"), import("./Study.svelte")]);
  mount(Study, { target: document.getElementById("app")! });
}
