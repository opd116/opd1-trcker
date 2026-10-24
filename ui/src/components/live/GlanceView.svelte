<!--
  Glance mode's body (design v2 TrackerApp.dc.html 44–92), under GlanceShell's 32px title bar in the 420px column:
  the map strip, the enemy team then yours in 52px rows (party gutter, agent, rank + RR, Smurf?, bookmark,
  name, K/D, HS%), each team's average rank, and the callout when a saved player is in the match. A row opens the
  profile in the full window. Outside a match the body shows the waiting radar.
-->
<script lang="ts">
  import { marks, settings } from "../../lib/appdata.svelte";
  import { assetUrl } from "../../lib/assets";
  import { retryImg } from "../../lib/imgRetry";
  import { avgRankShort, glanceTeams, rowView, savedCallout, teamTier, type RowCtx, type RowView } from "../../lib/livev2";
  import { mapName } from "../../lib/meta";
  import { openProfile, router, setGlance } from "../../lib/router.svelte";
  import { live, meta } from "../../lib/stores";
  import { tierColor, tierShort } from "../../lib/theme";
  import { tip } from "../../lib/tooltip.svelte";
  import Radar from "./Radar.svelte";
  import Shimmer from "./Shimmer.svelte";

  const l = $derived($live);
  const onBoard = $derived(!!l && (l.stage === "pregame" || l.stage === "ingame"));
  const ctx: RowCtx = $derived({
    meta: $meta,
    lobby: l?.players ?? [],
    settings: $settings,
    marks: $marks,
    selected: router.lastProfile,
    stage: l?.stage ?? "closed",
  });
  const teams = $derived(
    l && onBoard
      ? glanceTeams(l).map((t) => ({ ...t, tier: teamTier(t.cards), rows: t.cards.map((c, i) => rowView(c, i, t.cards, ctx)) }))
      : [],
  );
  const callout = $derived(savedCallout(l, $marks, $meta));
  const map = $derived(l?.mapId ? mapName($meta, l.mapId) : null);

  function open(puuid: string) {
    setGlance(false);
    openProfile(puuid, null);
  }

  function key(e: KeyboardEvent, puuid: string) {
    if (e.target !== e.currentTarget) return;
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      open(puuid);
    }
  }
</script>

{#snippet skelRow(border: boolean, r: RowView | null)}
  <div class="row" class:bb={border} aria-hidden="true">
    <div></div>
    <div class="agent">{#if r?.agentId}<img {@attach retryImg} class="dim" src={assetUrl("agent", r.agentId)} alt="" />{/if}</div>
    <div class="names">
      <Shimmer w={70} h={13} />
      {#if r && r.name.style !== "loading"}<span class={["n", r.name.style]} data-ellipsis>{r.name.text}</span>{:else}<Shimmer w={96} h={11} />{/if}
    </div>
    <div class="num"><Shimmer w={32} h={14} /></div>
    <div class="num"><Shimmer w={30} h={14} /></div>
  </div>
{/snippet}

<div class="glance-body">
  {#if l && onBoard}
    <div class="strip">
      {#if l.mapId}<img {@attach retryImg} src={assetUrl("mapList", l.mapId)} alt="" />{:else}<span class="noimg"></span>{/if}
      {#if map === undefined}<Shimmer w={80} h={16} />{:else}<span class="map">{map ?? "—"}</span>{/if}
    </div>
    <div class="scroll">
      {#each teams as t (t.team)}
        {@const total = t.rows.length + t.placeholders}
        <section class="team" aria-label={t.team === "ally" ? "Your team" : "Enemy team"}>
          <div class="head">
            <span class="edge" style:background={t.color}></span>
            <span class="label" style:color={t.color}>{t.label}</span>
            <div class="fill"></div>
            <span class="avg">Avg</span>
            {#if t.tier !== null}<img {@attach retryImg} class="avg-icon" src={assetUrl("tier", t.tier)} alt="" />{/if}
            <span class="avg-rank" style:color={t.tier === null ? "var(--text-muted)" : tierColor(t.tier)}>{avgRankShort(t.tier, $meta)}</span>
          </div>
          {#each t.rows as r, i (r.puuid)}
            {#if r.statsLoading}
              {@render skelRow(i < total - 1, r)}
            {:else}
              <div
                class="row"
                class:bb={i < total - 1}
                class:self={r.isSelf}
                class:sel={r.selected}
                data-row="1"
                role="button"
                tabindex="0"
                aria-label={r.aria}
                onclick={() => open(r.puuid)}
                onkeydown={(e) => key(e, r.puuid)}
              >
                <div class="bar"></div>
                <div class="gutter" use:tip={r.party?.tip ?? ""}>
                  {#if r.party}
                    <div
                      class={["link", !r.party.confirmed && "likely"]}
                      style:--pc={r.party.color}
                      style:top={r.party.top ? "0" : "50%"}
                      style:bottom={r.party.bottom ? "0" : "50%"}
                    ></div>
                    <span class={["pdot", !r.party.confirmed && "likely"]} style:--pc={r.party.color}></span>
                  {/if}
                </div>
                <div class="agent" class:picking={r.picking}>
                  {#if r.agentId}<img {@attach retryImg} src={assetUrl("agent", r.agentId)} alt={r.agent ?? ""} />{:else}<span class="q">?</span>{/if}
                </div>
                <div class="names">
                  <div class="l1">
                    {#if r.rank.state === "loading"}
                      <Shimmer w={70} h={13} />
                    {:else}
                      {#if r.rank.state !== "missing"}<img {@attach retryImg} class="ri" src={assetUrl("tier", r.rank.tier)} alt="" />{/if}
                      <span class="rs" style:color={r.rank.color}>{r.rank.state === "missing" ? "—" : tierShort(r.rank.tier, $meta)}</span>
                      <span class="rr">{r.rank.rrText}</span>
                    {/if}
                    {#if r.smurf}<span class="smurf" use:tip={r.smurf}
                        ><svg width="8" height="8" viewBox="0 0 10 10" aria-hidden="true"><path d="M5 .8L9.6 9.2H.4z" fill="var(--warn)"></path></svg>Smurf?</span
                      >{/if}
                    {#if r.saved}<span class="saved" use:tip={r.note ? `Saved by you\n${r.note}` : "Saved by you"}
                        ><svg width="9" height="11" viewBox="0 0 10 12" aria-hidden="true"><path d="M1 1h8v10L5 8.2 1 11z" fill="var(--accent)"></path></svg></span
                      >{/if}
                  </div>
                  <div class="l2">
                    {#if r.name.style === "loading"}
                      <Shimmer w={96} h={11} />
                    {:else}
                      <span class="nm" use:tip={r.name.tip}
                        ><span class={["n", r.name.style]} dir={r.name.style === "visible" || r.name.style === "remembered" ? "auto" : undefined} data-ellipsis>{r.name.text}</span
                        >{#if r.name.tag}<span class="t">{r.name.tag}</span>{/if}</span
                      >
                    {/if}
                  </div>
                </div>
                <div class="num">
                  {#if r.kd === undefined}<Shimmer w={32} h={14} />{:else}<span class="v" style:color={r.kd.color}>{r.kd.text}</span>{/if}
                  <span class="k">K/D</span>
                </div>
                <div class="num">
                  {#if r.hs === undefined}<Shimmer w={30} h={14} />{:else}<span class="v" style:color={r.hs.color}>{r.hs.text}</span>{/if}
                  <span class="k">HS%</span>
                </div>
              </div>
            {/if}
          {/each}
          {#each { length: t.placeholders } as _, i (i)}
            {@render skelRow(t.rows.length + i < total - 1, null)}
          {/each}
        </section>
      {/each}
      {#if callout}
        <div class="callout">
          <svg width="12" height="14" viewBox="0 0 10 12" aria-hidden="true"><path d="M1 1h8v10L5 8.2 1 11z" fill="var(--accent)"></path></svg>
          <div class="c-col">
            <span class="c-head">{callout.text}</span>
            {#if callout.note}<span class="c-note">Your note: {callout.note}</span>{/if}
          </div>
        </div>
      {/if}
    </div>
  {:else}
    <div class="idle"><Radar /></div>
  {/if}
</div>

<style>
  .glance-body {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  .strip {
    flex: none;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 14px;
    border-bottom: 1px solid var(--line-chrome);
  }

  .strip img,
  .noimg {
    width: 72px;
    height: 28px;
    object-fit: cover;
    border-radius: 4px;
    display: block;
    background: var(--row-hover);
    flex: none;
  }

  .map {
    font: 700 18px/1 var(--font-display);
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  .scroll {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 10px 10px 12px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .team {
    flex: none;
    background: var(--card);
    border: 1px solid var(--line);
    border-radius: 9px;
    overflow: hidden;
  }

  .head {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 32px;
    padding: 0 12px;
    background: var(--card-head);
    border-bottom: 1px solid var(--line);
  }

  .edge {
    width: 3px;
    height: 14px;
    border-radius: 1px;
  }

  .label {
    font: 700 13px/1 var(--font-display);
    letter-spacing: 0.12em;
  }

  .fill {
    flex: 1;
  }

  .avg {
    font: 500 12px/1 var(--font);
    color: var(--text-label);
  }

  .avg-icon {
    width: 16px;
    height: 16px;
    display: block;
  }

  .avg-rank {
    font: 700 13.5px/1 var(--font);
  }

  .row {
    position: relative;
    display: grid;
    grid-template-columns: 20px 34px minmax(0, 1fr) 44px 40px;
    column-gap: 10px;
    align-items: center;
    height: 52px;
    padding-right: 12px;
  }

  .row.bb {
    border-bottom: 1px solid var(--line-soft);
  }

  .row[role="button"] {
    cursor: pointer;
  }

  .row[role="button"]:hover {
    background: var(--row-hover);
  }

  .row.self {
    background: var(--accent-faint);
  }

  .row.sel {
    background: var(--accent-soft);
  }

  .row:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: -2px;
  }

  .bar {
    position: absolute;
    left: 0;
    top: 0;
    bottom: 0;
    width: 3px;
  }

  .self .bar {
    background: var(--accent);
  }

  .sel .bar {
    background: var(--accent-text);
  }

  .gutter {
    position: relative;
    height: 100%;
  }

  .link {
    position: absolute;
    left: 9px;
    width: 2px;
    background: var(--pc);
    opacity: 0.5;
    border-radius: 1px;
  }

  .link.likely {
    background: repeating-linear-gradient(180deg, var(--pc) 0 3px, transparent 3px 6px);
  }

  .pdot {
    position: absolute;
    left: 5px;
    top: 50%;
    margin-top: -5px;
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--pc);
    box-shadow: 0 0 0 3px var(--card);
  }

  .pdot.likely {
    background: var(--card);
    border: 2px solid var(--pc);
  }

  .agent {
    width: 34px;
    height: 34px;
    border-radius: 6px;
    background: var(--skel);
    overflow: hidden;
    border: 1px solid var(--line-media);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .agent.picking {
    border-style: dashed;
    border-color: var(--line-strong);
  }

  .agent img {
    width: 100%;
    height: 100%;
    display: block;
  }

  .agent img.dim {
    opacity: 0.75;
  }

  .q {
    font: 600 14px/1 var(--font);
    color: var(--text-dim);
  }

  .names {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
  }

  .l1 {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }

  .ri {
    width: 18px;
    height: 18px;
    display: block;
    flex: none;
  }

  .rs {
    font: 700 14.5px/1 var(--font);
    white-space: nowrap;
  }

  .rr {
    font: 500 12px/1 var(--font);
    color: var(--text-muted);
    white-space: nowrap;
  }

  .smurf {
    flex: none;
    display: inline-flex;
    align-items: center;
    gap: 3px;
    font: 600 11px/1 var(--font);
    color: var(--warn-text);
    border: 1px dashed rgba(255, 154, 60, 0.65);
    padding: 2px 5px;
    border-radius: 4px;
    white-space: nowrap;
  }

  .saved {
    flex: none;
    display: inline-flex;
  }

  .l2 {
    display: flex;
    min-width: 0;
  }

  .nm {
    display: flex;
    align-items: baseline;
    gap: 5px;
    min-width: 0;
    white-space: nowrap;
  }

  .n {
    min-width: 0;
    font: 600 13px/1 var(--font);
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .n.hidden {
    font-style: italic;
    font-weight: 500;
    color: var(--text-muted);
    padding-right: 2px;
  }

  .n.remembered {
    text-decoration: underline dotted var(--text-faint);
    text-underline-offset: 3px;
    overflow: clip;
    overflow-clip-margin: 6px;
  }

  .t {
    flex: none;
    font: 500 12px/1 var(--font);
    color: var(--text-tag);
  }

  .num {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 5px;
  }

  .v {
    font: 600 16px/1 var(--font);
    font-variant-numeric: tabular-nums;
  }

  .k {
    font: 500 10.5px/1 var(--font-mono);
    letter-spacing: 0.04em;
    color: var(--text-label);
  }

  .callout {
    flex: none;
    display: flex;
    gap: 10px;
    padding: 11px 12px;
    border-radius: 9px;
    background: rgba(157, 140, 255, 0.07);
    border: 1px solid rgba(157, 140, 255, 0.26);
  }

  .callout svg {
    flex: none;
    margin-top: 1px;
  }

  .c-col {
    display: flex;
    flex-direction: column;
    gap: 5px;
    min-width: 0;
  }

  .c-head {
    font: 600 12.5px/1.2 var(--font);
    color: var(--text);
  }

  .c-note {
    font: 500 12.5px/1.4 var(--font);
    color: var(--accent-pale);
    overflow-wrap: anywhere;
  }

  .idle {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
  }
</style>
