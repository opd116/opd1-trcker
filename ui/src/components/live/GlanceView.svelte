<script lang="ts">
  import { marks, settings } from "../../lib/appdata.svelte";
  import { assetUrl } from "../../lib/assets";
  import { cardOpenGuard } from "../../lib/clickGuard";
  import { retryImg } from "../../lib/imgRetry";
  import { avgRankShort, glanceTeams, rowView, savedCallout, teamTier, type RowCtx, type RowView } from "../../lib/livev2";
  import { mapName } from "../../lib/meta";
  import { openProfile, router, setGlance } from "../../lib/router.svelte";
  import { live, meta } from "../../lib/stores";
  import { tip } from "../../lib/tooltip.svelte";
  import Radar from "./Radar.svelte";
  import Shimmer from "./Shimmer.svelte";

  const l = $derived($live);
  const onBoard = $derived(!!l && (l.stage === "pregame" || l.stage === "ingame"));
  const ctx: RowCtx = $derived({
    meta: $meta, lobby: l?.players ?? [], settings: $settings, marks: $marks,
    selected: router.lastProfile, stage: l?.stage ?? "closed",
  });
  // Keep the established enemy-first ordering; data and privacy come from the same model as Live.
  const teams = $derived(l && onBoard
    ? glanceTeams(l).map((t) => ({ ...t, tier: teamTier(t.cards), rows: t.cards.map((c, i) => rowView(c, i, t.cards, ctx)) }))
    : []);
  const callout = $derived(savedCallout(l, $marks, $meta));
  const map = $derived(l?.mapId ? mapName($meta, l.mapId) : null);
  let pointed = $state<string | null>(null);
  let focused = $state<string | null>(null);
  const active = $derived(teams.flatMap((t) => t.rows).find((r) => r.puuid === (pointed ?? focused)) ?? null);

  function open(puuid: string) {
    cardOpenGuard.arm();
    setGlance(false);
    openProfile(puuid, null);
  }
  function key(e: KeyboardEvent, puuid: string) {
    if (e.target !== e.currentTarget) return;
    if (e.key === "Enter" || e.key === " ") { e.preventDefault(); open(puuid); }
  }
</script>

{#snippet agent(r: RowView)}
  <span class="agent" class:picking={r.picking}>
    {#if r.agentId}<img {@attach retryImg} src={assetUrl("agent", r.agentId)} alt={r.agent ?? ""} />{/if}
  </span>
{/snippet}

{#snippet identity(r: RowView)}
  <span class="identity" use:tip={r.name.tip}>
    {#if r.name.style === "loading"}<Shimmer w={100} h={14} />{:else}
      {#if r.name.style === "hidden" || r.name.style === "remembered"}
        <svg class="eye" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" aria-hidden="true"><path d="m2 2 12 12M6 3.6c4-1 7 4.4 7 4.4s-.8 1.4-2.2 2.6M9.8 12.3C5.5 14 2 8 2 8s.8-1.4 2.2-2.6" /></svg>
      {/if}
      <span class={["name", r.name.style]} dir={r.name.style === "visible" || r.name.style === "remembered" ? "auto" : undefined} data-ellipsis>{r.name.text}</span>
      {#if r.name.tag}<span class="tag">{r.name.tag}</span>{/if}
    {/if}
  </span>
{/snippet}

<div class="glance-body" class:inspecting={active !== null}>
  {#if l && onBoard}
    <div class="strip">
      {#if l.mapId}<img {@attach retryImg} src={assetUrl("mapList", l.mapId)} alt="" />{/if}
      {#if map === undefined}<Shimmer w={80} h={16} />{:else}<span class="map">{map ?? "—"}</span>{/if}
    </div>
    <div class="scroll" role="presentation" onmouseleave={() => pointed = null}>
      <div class="grid" class:single={teams.length === 1}>
        {#each teams as t (t.team)}
          <section class="team" style:--team-color={t.color} aria-label={t.team === "ally" ? "Your team" : "Enemy team"}>
            <div class="head">
              <span class="label">{t.label}</span>
              <span class="average">{#if t.tier !== null}<img {@attach retryImg} src={assetUrl("tier", t.tier)} alt="" />{/if}{avgRankShort(t.tier, $meta)} avg</span>
            </div>
            {#each t.rows as r (r.puuid)}
              <div class="row" class:self={r.isSelf} class:sel={r.selected || active?.puuid === r.puuid}
                data-row="1" role="button" tabindex="0" aria-label={`${r.aria}${r.saved ? `. Saved by you${r.note ? `. Note: ${r.note}` : ""}` : ""}`}
                aria-describedby={active?.puuid === r.puuid ? "glance-player-detail" : undefined}
                onmouseenter={() => pointed = r.puuid}
                onfocus={() => focused = r.puuid} onblur={() => focused = null}
                onclick={() => open(r.puuid)} onkeydown={(e) => key(e, r.puuid)}>
                {@render agent(r)}
                <span class="rank" use:tip={`${r.rank.name} · ${r.rank.rrText}`}>
                  {#if r.rank.state === "loading"}<Shimmer w={23} h={23} />{:else if r.rank.state !== "missing"}<img {@attach retryImg} src={assetUrl("tier", r.rank.tier)} alt={r.rank.name} />
                  {:else}<span class="missing">—</span>{/if}
                </span>
                <span class="num" use:tip={"Last 5 competitive"}>
                  {#if r.kd === undefined}<Shimmer w={30} h={20} />{:else}<b>{r.kd.text}</b>{/if}<span>K/D</span>
                </span>
                <span class="markers">
                  {#if r.saved}<svg class="saved" width="11" height="13" viewBox="0 0 10 12" use:tip={r.note ? `Saved by you\n${r.note}` : "Saved by you"} aria-label="Saved by you"><path d="M1 1h8v10L5 8.2 1 11z" fill="currentColor" /></svg>{/if}
                  {#if r.party}<span class="party" class:likely={!r.party.confirmed} style:--pc={r.party.color} use:tip={r.party.tip}></span>{/if}
                  {#if r.met}<svg class="met" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" use:tip={r.met.tip} aria-label={r.met.aria}><path d="M2 5a6 6 0 1 1 0 6M2 1v4h4" /></svg>{/if}
                </span>
              </div>
            {/each}
            {#each { length: t.placeholders } as _, i (i)}
              <div class="row skeleton" aria-hidden="true"><Shimmer w={29} h={29} r={15} /><Shimmer w={23} h={23} /><Shimmer w={30} h={20} /></div>
            {/each}
          </section>
        {/each}
      </div>
      <div class="detail-space">
        {#if active}
          <div class="hover-card" id="glance-player-detail">
            <div class="detail-top">{@render agent(active)}<div class="detail-name">{@render identity(active)}<span class="sub">{active.agent ?? "—"} · Lv {active.level ?? "—"}</span></div>
              <div class="detail-rank">
                {#if active.rank.state === "loading"}<Shimmer w={90} h={16} />{:else}<b>{active.rank.name}</b><span>{active.rank.rrText}</span>{/if}
              </div>
            </div>
            <div class="detail-stats">
              <span use:tip={"Last 5 competitive"}>{#if active.kd === undefined}<Shimmer w={34} h={18} />{:else}<b>{active.kd.text}</b>{/if} K/D</span>
              <span use:tip={active.wr ? `This act · ${active.wr.w} W · ${active.wr.l} L` : "This act"}>{#if active.wr === undefined}<Shimmer w={34} h={18} />{:else}<b>{active.wr?.text ?? "—"}</b>{/if} WIN</span>
              <span use:tip={"Last 5 competitive"}>{#if active.hs === undefined}<Shimmer w={34} h={18} />{:else}<b>{active.hs.text}</b>{/if} HS</span>
            </div>
            {#if active.peak}<p class="peak" data-ellipsis use:tip={active.peak.tip}>Peak {active.peak.tip.split("\n").slice(0, 2).join(" · ")}</p>{/if}
            {#if active.note}<p class="note" data-ellipsis use:tip={active.note}>{active.note}</p>{/if}
          </div>
        {/if}
        {#if callout && !active}
          <div class="callout" use:tip={`${callout.text}${callout.note ? `\nYour note: ${callout.note}` : ""}`}><svg width="12" height="14" viewBox="0 0 10 12" aria-hidden="true"><path d="M1 1h8v10L5 8.2 1 11z" fill="currentColor" /></svg><div><span class="c-head" data-ellipsis>{callout.text}</span>{#if callout.note}<span class="c-note" data-ellipsis>{callout.note}</span>{/if}</div></div>
        {/if}
      </div>
    </div>
  {:else}
    <div class="idle"><Radar size={120} /><h1>Waiting for match</h1><p>Your lobby appears when agent select begins.</p></div>
  {/if}
</div>

<style>
  /* Approved prototype Glance in the square 420×420 window (R205, compact-design): map strip, two five-row
     team columns of cells capped at 49px, and the hover detail card replacing the saved-player callout.
     Map/header/grid compress while the pointer is on a player: all ten cells stay visible beneath the card. */
  .glance-body{flex:1;min-height:0;display:flex;flex-direction:column}
  .strip{height:30px;flex:none;display:flex;align-items:center;gap:10px;padding:0 12px}
  .strip img{width:52px;height:20px;object-fit:cover;border-radius:4px}
  .map{font:600 12px/1 var(--font)}
  .scroll{flex:1;min-height:0;overflow:auto;display:flex;flex-direction:column;padding:0 12px 4px}
  /* The grid spends the body's remaining height (prototype `.glance-grid{flex:1}`): its row stretches,
     each team column stretches with it, and the cells flex up to the prototype's 49px cap. */
  .grid{flex:1;min-height:0;display:grid;grid-template-columns:minmax(0,1fr) minmax(0,1fr);grid-template-rows:minmax(0,1fr);gap:10px}
  .grid.single{grid-template-columns:minmax(0,1fr)}
  .team{min-width:0;display:flex;flex-direction:column}
  .head{height:42px;flex:none;display:flex;flex-direction:column;gap:3px;padding-top:6px;margin-bottom:4px;border-top:1px solid var(--team-color)}
  .label{font:700 11px/1 var(--font);letter-spacing:.06em;color:var(--team-color)}
  .average{display:flex;align-items:center;gap:4px;font:500 11px/1 var(--font);color:var(--text-muted)}
  .average img{width:14px;height:14px}
  /* Cells flex to spend the body's remaining height, never past the prototype's 49px. */
  .row{flex:1;min-height:0;max-height:49px;display:grid;grid-template-columns:29px 23px minmax(30px,1fr) 26px;align-items:center;gap:7px;padding:4px 7px;margin-bottom:4px;border:1px solid var(--line);border-radius:6px;background:var(--card);cursor:pointer}
  .row:hover,.row.sel{background:var(--row-hover);border-color:var(--line-hover)}
  .row.self{border-color:var(--accent-line);background:var(--accent-faint)}
  .row:focus-visible{outline:2px solid var(--focus);outline-offset:2px}
  .agent{width:29px;height:29px;flex:none;display:flex;align-items:center;justify-content:center;border:1px solid var(--team-color,var(--line-media));border-radius:50%;overflow:hidden}
  .agent.picking{border-style:dashed;border-color:var(--line-strong)}
  .agent img{width:100%;height:100%;object-fit:contain}
  .rank{display:flex;justify-content:center;min-width:0}
  .rank img{width:23px;height:23px;object-fit:contain}
  .missing{color:var(--text-muted)}
  .num{display:flex;flex-direction:column;align-items:flex-start;gap:3px;font-variant-numeric:tabular-nums}
  .num b{font:700 20px/21px var(--font)}
  .num>span{font:500 11px/12px var(--font-mono);color:var(--text-muted)}
  .markers{display:grid;grid-template-columns:12px 12px;align-items:center;justify-items:center;gap:2px;min-width:0}
  .saved{color:var(--accent)}
  .party{width:9px;height:9px;border:1px solid var(--pc);border-radius:50%;background:var(--pc)}
  .party.likely{background:transparent;border-style:dashed}
  .met{width:12px;height:12px;color:var(--text-muted)}
  .skeleton{cursor:default}
  .detail-space{margin-top:2px}
  .detail-space:empty{display:none}
  .hover-card{border:1px solid var(--line-control);border-radius:8px;background:var(--card);padding:8px}
  .detail-top{display:flex;align-items:center;gap:8px;min-width:0}
  .detail-top .agent{width:26px;height:26px}
  .detail-name{display:flex;flex-direction:column;gap:2px;min-width:0;flex:1}
  .identity{display:flex;align-items:baseline;gap:5px;min-width:0;white-space:nowrap}
  .name{font:600 14px/1.15 var(--font);overflow:hidden;text-overflow:ellipsis;min-width:0}
  .name.hidden{font-style:italic;color:var(--text-muted)}
  .name.remembered{text-decoration:underline dotted;text-underline-offset:3px}
  .eye{width:12px;height:12px;flex:none;color:var(--text-muted)}
  .tag,.sub{font:500 11px/1.2 var(--font);color:var(--text-muted)}
  .tag{flex:none}
  .detail-rank{display:flex;flex-direction:column;align-items:flex-end;gap:3px;flex:none;font:500 12px/1.2 var(--font);color:var(--text-muted)}
  .detail-rank b{font-weight:600;color:var(--text)}
  .detail-stats{display:flex;gap:16px;margin-top:4px;font:500 11px/1 var(--font);color:var(--text-muted)}
  .detail-stats>span{display:flex;align-items:baseline;gap:3px}
  .detail-stats b{font:700 18px/1 var(--font);color:var(--text);margin-right:3px}
  .peak,.note,.c-note{margin:4px 0 0;font:500 12px/15px var(--font);color:var(--text-muted);white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
  .note,.c-note{color:var(--accent-pale)}
  .callout{display:flex;align-items:center;gap:7px;padding:5px 8px;border:1px solid var(--accent-line);border-radius:6px;background:var(--accent-faint)}
  .callout>svg{flex:none;color:var(--accent)}
  /* The note gets its own full-width line (no crowding the head): each line truncates within the strip. */
  .callout>div{min-width:0;display:flex;flex-direction:column;gap:1px;flex:1}
  .c-head{font:600 12px/15px var(--font);min-width:0;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
  .c-note{margin:0;min-width:0;font:500 12px/15px var(--font);color:var(--accent-pale);white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
  /* Hovered detail: the callout gives way and the map strip, team header reserved height and cells compact. */
  .inspecting .strip{height:27px}
  .inspecting .strip img{height:16px}
  .inspecting .row{padding-top:0;padding-bottom:0;min-height:33px}
  .inspecting .row .agent{width:25px;height:25px}
  .inspecting .num{gap:1px}
  .inspecting .num b{font-size:18px;line-height:18px}
  .idle{flex:1;display:flex;flex-direction:column;align-items:center;justify-content:center;gap:18px;padding:24px;text-align:center}
  .idle h1{font:700 26px/1 var(--font);margin:0}
  .idle p{font:500 14px/1.4 var(--font);color:var(--text-muted);margin:0}
</style>
