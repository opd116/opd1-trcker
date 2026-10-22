<!--
  One player of the live board (design v2 TrackerApp.dc.html 193–236, `rows()` 1289–1350): party gutter, agent,
  name + badges, sub-line, rank with the peak chip and RR bar, Win% with the streak bars, K/D, HS%, chevron. A row
  whose stats have not arrived yet is the design's skeleton row (238–255). `card` absent = a placeholder (the roster
  itself is still loading). Click, Enter or Space opens the profile drawer.
-->
<script lang="ts">
  import { assetUrl } from "../../lib/assets";
  import { retryImg } from "../../lib/imgRetry";
  import type { RowView } from "../../lib/livev2";
  import { tip } from "../../lib/tooltip.svelte";
  import Shimmer from "./Shimmer.svelte";

  interface Props {
    row: RowView | null;
    /** A bottom border (every row but the team's last). */
    border: boolean;
    onopen?: (puuid: string, el: HTMLElement) => void;
  }

  let { row, border, onopen }: Props = $props();

  function key(e: KeyboardEvent) {
    if (e.target !== e.currentTarget || !row) return;
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      onopen?.(row.puuid, e.currentTarget as HTMLElement);
    }
  }
</script>

{#snippet sub(r: RowView, extra: string)}
  <span class="sub" style:color={r.subColor}>
    {#if r.agent === undefined}<Shimmer w={44} h={10} />{:else}{r.agent}{/if}
    <span class="sep">·</span> Lv
    {#if r.level === undefined}<Shimmer w={22} h={10} />{:else}{r.level}{/if}{extra}
  </span>
{/snippet}

{#snippet nameLine(r: RowView, withTag: boolean)}
  {#if r.name.style === "loading"}
    <Shimmer w={120} h={13} />
  {:else}
    <span class="nm" use:tip={r.name.tip}
      ><span class={["n", r.name.style]} dir={r.name.style === "visible" || r.name.style === "remembered" ? "auto" : undefined}
        >{r.name.text}</span
      >{#if withTag && r.name.tag}<span class="t">{r.name.tag}</span>{/if}</span
    >
  {/if}
{/snippet}

{#if !row}
  <div class="row skel" class:bb={border} aria-hidden="true">
    <div></div>
    <div class="player">
      <Shimmer w={38} h={38} r={7} />
      <div class="names sk-names"><Shimmer w={110} h={13} /><Shimmer w={80} h={10} /></div>
    </div>
    <div class="rank"><Shimmer w={30} h={30} r={15} /><div class="rank-col sk-col"><Shimmer w={78} h={12} /><Shimmer w={52} h={6} /></div></div>
    <div><Shimmer w={38} h={16} /></div>
    <div><Shimmer w={36} h={16} /></div>
    <div><Shimmer w={34} h={16} /></div>
    <div></div>
  </div>
{:else if row.statsLoading}
  <div class="row skel" class:bb={border} aria-label="{row.name.text || 'Player'}, loading">
    <div></div>
    <div class="player">
      <div class="agent-wrap">
        <div class="agent" style:border={row.picking ? "1px dashed var(--line-strong)" : "1px solid var(--line-media)"}>
          {#if row.agentId}<img {@attach retryImg} class="loading-img" src={assetUrl("agent", row.agentId)} alt="" />{:else}<span class="q">?</span>{/if}
        </div>
      </div>
      <div class="names sk-names">
        <div class="line1">{@render nameLine(row, false)}</div>
        <div class="line2">{@render sub(row, " · loading stats…")}</div>
      </div>
    </div>
    <div class="rank"><Shimmer w={30} h={30} r={15} /><div class="rank-col sk-col"><Shimmer w={78} h={12} /><Shimmer w={52} h={6} /></div></div>
    <div><Shimmer w={38} h={16} /></div>
    <div><Shimmer w={36} h={16} /></div>
    <div><Shimmer w={34} h={16} /></div>
    <div></div>
  </div>
{:else}
  <div
    class="row"
    class:bb={border}
    class:self={row.isSelf}
    class:sel={row.selected}
    data-row="1"
    role="button"
    tabindex="0"
    aria-label={row.aria}
    onclick={(e) => onopen?.(row.puuid, e.currentTarget)}
    onkeydown={key}
  >
    <div class="bar"></div>
    <div class="gutter" use:tip={row.party?.tip ?? ""}>
      {#if row.party}
        <div
          class={["link", !row.party.confirmed && "likely"]}
          style:--pc={row.party.color}
          style:top={row.party.top ? "0" : "50%"}
          style:bottom={row.party.bottom ? "0" : "50%"}
        ></div>
        <span class={["pdot", !row.party.confirmed && "likely"]} style:--pc={row.party.color}></span>
      {/if}
    </div>
    <div class="player">
      <div class="agent-wrap">
        <div class="agent" style:border={row.agentBorder}>
          {#if row.agentId}<img {@attach retryImg} src={assetUrl("agent", row.agentId)} alt={row.agent ?? ""} />{:else}<span class="q">?</span>{/if}
        </div>
      </div>
      <div class="names">
        <div class="line1">
          {@render nameLine(row, true)}
          {#if row.isSelf}<span class="you">YOU</span>{/if}
          {#if row.smurf}<span class="smurf" use:tip={row.smurf}
              ><svg width="9" height="9" viewBox="0 0 10 10" aria-hidden="true"><path d="M5 .8L9.6 9.2H.4z" fill="var(--warn)"></path></svg>Smurf?</span
            >{/if}
          {#if row.met}<span class="met" use:tip={{ kv: row.met.tip }} role="img" aria-label={row.met.aria}
              ><svg width="11" height="11" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"
                ><path d="M13.2 6.2A5.4 5.4 0 0 0 3.6 4.4"></path><path d="M3.2 1.6v3.2h3.2"></path><path d="M2.8 9.8a5.4 5.4 0 0 0 9.6 1.8"></path><path d="M12.8 14.4v-3.2H9.6"></path></svg
              ></span
            >{/if}
          {#if row.saved}<span class="saved" use:tip={"Saved by you"}
              ><svg width="10" height="12" viewBox="0 0 10 12" aria-hidden="true"><path d="M1 1h8v10L5 8.2 1 11z" fill="var(--accent)"></path></svg></span
            >{/if}
        </div>
        <div class="line2">
          {@render sub(row, "")}
          {#if row.note}<span class="note" use:tip={`Your note\n${row.note}`}><span class="vsep"></span><span class="note-t">{row.note}</span></span>{/if}
        </div>
      </div>
    </div>
    <div class="rank">
      {#if row.rank.state === "loading"}
        <Shimmer w={30} h={30} r={15} /><div class="rank-col sk-col"><Shimmer w={78} h={12} /><Shimmer w={52} h={6} /></div>
      {:else}
        {#if row.rank.state === "missing"}<span class="rank-icon"></span>{:else}<img {@attach retryImg} class="rank-icon" src={assetUrl("tier", row.rank.tier)} alt="" />{/if}
        <div class="rank-col">
          <div class="rank-top">
            <span class="rank-name" style:color={row.rank.color}>{row.rank.name}</span>
            {#if row.peak}<span class="peak" use:tip={{ text: row.peak.tip, img: assetUrl("tier", row.peak.icon), color: row.peak.color }}>▲{row.peak.delta}</span>{/if}
          </div>
          <div class="rr">
            <div class="rr-track"><div class="rr-fill" style:width="{row.rank.rrPct}%" style:background={row.rank.color}></div></div>
            <span class="rr-t">{row.rank.rrText}</span>
          </div>
        </div>
      {/if}
    </div>
    <div class="wr-cell">
      {#if row.wr === undefined}
        <Shimmer w={38} h={16} />
      {:else}
        <div class="wr-top">
          {#if row.wr === null}<span class="big dim">—</span>{:else}<span class="big" use:tip={{ wl: [row.wr.w, row.wr.l], text: `This act: ${row.wr.w} wins, ${row.wr.l} losses` }} style:color={row.wr.color}>{row.wr.text}</span>{/if}
          {#if row.streak}<span class="streak" use:tip={row.streak.tip}
              >{#each { length: row.streak.n } as _, i (i)}<span class="seg" style:background={row.streak.win ? "var(--win)" : "var(--loss)"}></span>{/each}</span
            >{/if}
        </div>
        {#if row.games !== undefined}<span class="games">{row.games}</span>{/if}
      {/if}
    </div>
    {#if row.kd === undefined}<div><Shimmer w={36} h={16} /></div>{:else}<span class="big stat" style:color={row.kd.color}>{row.kd.text}</span>{/if}
    {#if row.hs === undefined}<div><Shimmer w={34} h={16} /></div>{:else}<span class="big stat" style:color={row.hs.color}>{row.hs.text}</span>{/if}
    <svg class="chev" width="7" height="12" viewBox="0 0 7 12" fill="none" stroke="var(--text-disabled)" stroke-width="1.5" stroke-linecap="round" aria-hidden="true"><path d="M1.5 1.5L5.5 6l-4 4.5"></path></svg>
  </div>
{/if}

<style>
  .row {
    position: relative;
    display: grid;
    grid-template-columns: 24px minmax(0, 2.2fr) minmax(0, 1.9fr) 72px 64px 64px 20px;
    column-gap: 12px;
    align-items: center;
    height: 56px;
    padding-right: 10px;
    background: transparent;
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
    box-shadow: inset 0 0 0 1px rgba(183, 171, 255, 0.45);
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

  /* A likely party (not confirmed): a dashed link and a hollow dot (CLAUDE.md Party detection). */
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

  .player {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
  }

  .agent-wrap {
    position: relative;
    flex: none;
    width: 40px;
    height: 40px;
  }

  .agent {
    width: 38px;
    height: 38px;
    border-radius: 7px;
    background: var(--skel);
    overflow: hidden;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .agent img {
    width: 100%;
    height: 100%;
    display: block;
  }

  .agent img.loading-img {
    opacity: 0.75;
  }

  .q {
    font: 600 16px/1 var(--font);
    color: var(--text-dim);
  }

  .names {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
    flex: 1;
  }

  .sk-names {
    gap: 7px;
  }

  .line1 {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }

  .nm {
    flex: 0 1 auto;
    min-width: 0;
    display: flex;
    align-items: baseline;
    gap: 6px;
  }

  .n {
    min-width: 0;
    font: 600 15px/1.15 var(--font);
    color: var(--text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .n.hidden {
    color: var(--text-muted);
    font-style: italic;
    font-weight: 500;
    /* an italic's last glyph overhangs its box: room for it */
    padding-right: 2px;
  }

  .n.missing {
    color: var(--text-muted);
  }

  /* The dotted underline hangs below the clipped box: widen the paint clip (as the old Name.svelte did). */
  .n.remembered {
    text-decoration: underline dotted var(--text-faint);
    text-underline-offset: 3px;
    overflow: clip;
    overflow-clip-margin: 6px;
  }

  .skel .n {
    color: var(--text-2);
  }

  .t {
    flex: none;
    font: 500 12.5px/1 var(--font);
    color: var(--text-tag);
    white-space: nowrap;
  }

  .you {
    flex: none;
    font: 700 10px/1 var(--font-mono);
    letter-spacing: 0.06em;
    color: var(--on-accent);
    background: var(--accent);
    padding: 3px 5px;
    border-radius: 3px;
  }

  .smurf {
    flex: none;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font: 600 11.5px/1 var(--font);
    color: var(--warn-text);
    border: 1px dashed rgba(255, 154, 60, 0.65);
    padding: 2px 6px;
    border-radius: 4px;
    white-space: nowrap;
    cursor: help;
  }

  .met {
    flex: none;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    background: var(--text);
    color: var(--bg);
    cursor: help;
  }

  .saved {
    flex: none;
    display: inline-flex;
  }

  .line2 {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    white-space: nowrap;
  }

  .sub {
    flex: none;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font: 500 12.5px/1 var(--font);
  }

  .sep {
    margin: 0 -1px;
  }

  .note {
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 7px;
    font: 500 12.5px/1 var(--font);
    color: #bdb4f5;
  }

  .vsep {
    width: 1px;
    height: 10px;
    background: var(--line-hover);
    flex: none;
  }

  .note-t {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .rank {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }

  .rank-icon {
    width: 30px;
    height: 30px;
    flex: none;
    display: block;
  }

  .rank-col {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
  }

  .sk-col {
    gap: 7px;
  }

  .rank-top {
    display: flex;
    align-items: center;
    gap: 7px;
    min-width: 0;
  }

  .rank-name {
    min-width: 0;
    font: 700 15.5px/1 var(--font);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .peak {
    flex: none;
    font: 700 11.5px/1 var(--font);
    color: var(--text);
    background: var(--chip-hover);
    border: 1px solid var(--line-tip);
    padding: 2px 5px;
    border-radius: 4px;
    white-space: nowrap;
    cursor: help;
  }

  .rr {
    display: flex;
    align-items: center;
    gap: 7px;
  }

  .rr-track {
    width: 44px;
    height: 4px;
    flex: none;
    border-radius: 2px;
    background: #2a2e35;
    overflow: hidden;
  }

  .rr-fill {
    height: 100%;
  }

  .rr-t {
    font: 500 12px/1 var(--font);
    color: var(--text-muted);
    white-space: nowrap;
  }

  .wr-cell {
    display: flex;
    flex-direction: column;
    gap: 5px;
    cursor: default;
  }

  .wr-top {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .big {
    font: 600 17px/1 var(--font);
    font-variant-numeric: tabular-nums;
  }

  .big.dim {
    color: var(--text-muted);
  }

  .stat {
    cursor: default;
    justify-self: start;
  }

  .streak {
    flex: none;
    display: flex;
    align-items: center;
    gap: 2px;
    height: 14px;
    cursor: help;
  }

  .seg {
    width: 3px;
    height: 11px;
    border-radius: 1px;
  }

  .games {
    font: 500 12px/1 var(--font);
    color: var(--text-tag);
    white-space: nowrap;
  }

  .chev {
    display: block;
  }
</style>
