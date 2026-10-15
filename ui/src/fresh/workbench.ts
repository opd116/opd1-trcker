if (import.meta.env.DEV) {
  const root = document.getElementById('app')!;
  document.body.style.cssText = 'margin:0;background:#0e1113;color:#c9d0d3;font:14px system-ui;color-scheme:dark';
  root.innerHTML = `<div style="max-width:1000px;margin:auto;padding:16px 12px;display:flex;gap:16px;align-items:center;flex-wrap:wrap"><div><label for="scene">Preview</label> <select id="scene"></select></div><div><label for="layout">Layout</label> <select id="layout"><option value="roster">Aligned roster</option><option value="teams">Team summaries</option></select></div><a id="actual" style="color:#b5c6b1" target="_blank">1000 × 800</a><span style="margin-left:auto;color:#8d989e">Mock data · Design study</span></div><div id="frame" style="margin:auto;overflow:hidden"><iframe title="OPD1 TRCKER interactive prototype" style="border:0;width:1000px;height:800px;transform-origin:top left"></iframe></div>`;
  const choices = [['ingame','In game'],['pregame','Agent select'],['lobby2','Party lobby'],['closed','Game closed'],['ingame-longnames','Long names'],['ingame-partial','Partial data'],['ingame-loading','Loading'],['ingame-remembered','Hidden identities'],['ingame-parties4','Parties'],['details','Player details'],['match','Scoreboard'],['mystats','My Stats']];
  const select = document.getElementById('scene') as HTMLSelectElement;
  const layout = document.getElementById('layout') as HTMLSelectElement;
  for (const [value, label] of choices) select.add(new Option(label, value));
  const iframe = root.querySelector('iframe')!;
  const frame = document.getElementById('frame')!;
  const link = document.getElementById('actual') as HTMLAnchorElement;
  select.value = new URLSearchParams(location.search).get('scene') ?? 'ingame';
  layout.value = new URLSearchParams(location.search).get('layout') ?? 'roster';
  const update = () => { const query = `?scene=${encodeURIComponent(select.value)}&layout=${encodeURIComponent(layout.value)}`; iframe.src = `/design-screen.html${query}`; link.href = iframe.src; history.replaceState(null,'',query); };
  layout.addEventListener('change',update);
  select.addEventListener('change', update); update();
  const fit = () => {const scale = Math.min(1, (document.documentElement.clientWidth - 24) / 1000); iframe.style.transform = `scale(${scale})`; frame.style.width = `${1000*scale}px`; frame.style.height = `${800*scale}px`;};
  new ResizeObserver(fit).observe(document.documentElement); fit();
}
