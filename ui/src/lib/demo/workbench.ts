// Separate Vite development entry. Never imported by the app or included in dist.
const controls = document.querySelector<HTMLFormElement>("#controls")!;
const frame = document.querySelector<HTMLIFrameElement>("iframe")!;
const stage = document.querySelector<HTMLDivElement>(".stage")!;
const link = document.querySelector<HTMLAnchorElement>("#open")!;
const initial = new URLSearchParams(location.search);
for (const control of controls.elements) {
  if (!(control instanceof HTMLInputElement || control instanceof HTMLSelectElement)) continue;
  const value = initial.get(control.name);
  if (value !== null && (control instanceof HTMLInputElement || [...control.options].some((o) => o.value === value))) control.value = value;
}
function update() {
  const query = new URLSearchParams(new FormData(controls) as unknown as Record<string, string>);
  frame.src = "/design-screen.html?" + query;
  link.href = frame.src;
  history.replaceState(null, "", "/design.html?" + query);
}
controls.addEventListener("change", update);
controls.addEventListener("submit", (e) => e.preventDefault());
const resize = new ResizeObserver(() => {
  const scale = Math.min(1, stage.clientWidth / 1000);
  frame.style.transform = `scale(${scale})`;
  stage.style.height = `${800 * scale}px`;
});
resize.observe(stage);
update();
