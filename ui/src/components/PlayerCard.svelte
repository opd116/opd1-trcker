<!--
  The player card. Identical content in Lobby, Agent Select and In Game; only the layout adapts:
    sm  horizontal strip (In Game, 5 stacked per team column, ~476×124)
    md  vertical card    (Agent Select, Lobby 4–5, ≥ ~300 wide)
    lg  vertical card    (Lobby 1–3, roomier)
  Field states (ruling R23): undefined → skeleton, null → "—".
-->
<script lang="ts">
  import { fade } from "svelte/transition";
  import { assetUrl } from "../lib/assets";
  import { kd, pct, peakLabel, level, span } from "../lib/format";
  import { agentName, FIRST_UNCAPPED_TIER, isRankedTier, tierColor, tierLabel } from "../lib/meta";
  import { meta } from "../lib/stores";
  import { tip, type TipContent } from "../lib/tooltip.svelte";
  import type { PlayerCard } from "../lib/types";
  import EncounterBadge from "./EncounterBadge.svelte";
  import Last5 from "./Last5.svelte";
  import PlayerName from "./PlayerName.svelte";
  import RankBadge from "./RankBadge.svelte";
  import Skeleton from "./Skeleton.svelte";

  type Size = "sm" | "md" | "lg";

  interface Props {
    card: PlayerCard;
    size?: Size;
    /** Tint the border with the team colour (In Game). */
    showTeam?: boolean;
    onopen?: (card: PlayerCard) => void;
  }

  let { card, size = "md", showTeam = false, onopen }: Props = $props();

  /** Icon edges in px per size. */
  const ICONS = {
    sm: { cur: 36, sub: 22 },
    md: { cur: 40, sub: 32 },
    lg: { cur: 56, sub: 44 },
  } as const;

  const icons = $derived(ICONS[size]);
  const strip = $derived(size === "sm");

  const agent = $derived(agentName($meta, card.agentId));

  // Current rank.
  const rank = $derived(card.rank);
  const curTier = $derived(rank === undefined ? undefined : rank === null ? null : rank.tier);
  const ranked = $derived(!!rank && isRankedTier(rank.tier));
  const rrFill = $derived(
    rank && ranked && rank.tier < FIRST_UNCAPPED_TIER ? Math.max(0, Math.min(100, rank.rr)) : null,
  );
  const curTip = $derived.by((): TipContent => {
    if (rank === undefined) return null;
    if (!rank || !ranked) return "Unranked";
    const name = tierLabel($meta, rank.tier) ?? "Current rank";
    if (rank.tier >= FIRST_UNCAPPED_TIER) return [name, `${rank.rr} RR`];
    return [name, `${rank.rr} RR · ${Math.max(0, 100 - rank.rr)} RR to next tier`];
  });

  // Previous act.
  const prevName = $derived(card.prevActTier == null ? null : tierLabel($meta, card.prevActTier));
  const prevTip = $derived.by((): TipContent => {
    if (card.prevActTier === undefined) return null;
    return ["Previous act", prevName ?? "Unranked"];
  });

  // Peak.
  const peak = $derived(card.peak);
  const peakTip = $derived.by((): TipContent => {
    if (peak === undefined) return null;
    if (peak === null) return ["Peak rank", "No ranked history"];
    return [
      `Peak · ${tierLabel($meta, peak.tier) ?? "—"}`,
      peak.actName,
      peak.sinceActEndMs === null ? "This act" : `Act ended ${span(peak.sinceActEndMs)} ago`,
    ];
  });

  const stripe = $derived(!card.isSelfParty && card.party ? card.party : null);
  const showEncounters = $derived(
    !card.isSelfParty && !!card.encounters && card.encounters.with + card.encounters.against > 0,
  );

  /** Image ids that failed to load (a changed id gets a fresh attempt). */
  let artFailed = $state<string | null>(null);
  let agentFailed = $state<string | null>(null);

  function open() {
    onopen?.(card);
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      open();
    }
  }
</script>

{#snippet encounterBadge()}
  {#if showEncounters && card.encounters}
    <EncounterBadge encounters={card.encounters} />
  {/if}
{/snippet}

{#snippet stat(label: string, value: string | undefined, hint: string)}
  <div class="stat" data-stat {@attach tip(hint)}>
    <span class="lbl" data-text>{label}</span>
    <span class="sval num" data-text>
      {#if value === undefined}<Skeleton w="2.6em" h="0.72em" />{:else}{value}{/if}
    </span>
  </div>
{/snippet}

<div
  class="card {size}"
  class:team={showTeam}
  class:enemy={card.team === "enemy"}
  role="button"
  tabindex="0"
  data-card
  data-team={card.team}
  onclick={open}
  {onkeydown}
>
  {#if stripe}
    <span
      class="stripe"
      class:likely={!stripe.confirmed}
      style:--party="var(--party-{stripe.group % 5})"
      {@attach tip(stripe.confirmed ? "Party" : "Likely party")}
    ></span>
  {/if}

  <div class="art">
    {#if card.cardId && artFailed !== card.cardId}
      <img
        class="art-img"
        src={assetUrl(strip ? "cardSmall" : "cardWide", card.cardId)}
        alt=""
        decoding="async"
        onerror={() => (artFailed = card.cardId)}
      />
    {/if}
    <span class="level num" data-text {@attach tip("Account level")}>{level(card.level)}</span>
    {#if card.agentId && agentFailed !== card.agentId}
      <span class="agent" in:fade={{ duration: 180 }} {@attach tip(agent)}>
        <img src={assetUrl("agent", card.agentId)} alt={agent ?? ""} decoding="async" onerror={() => (agentFailed = card.agentId)} />
      </span>
    {/if}
  </div>

  <div class="body">
    <div class="id">
      <PlayerName name={card.name} layout={strip ? "inline" : "stacked"} aside={encounterBadge} />
    </div>

    <div class="ranks">
      <div class="slot cur" data-stat {@attach tip(curTip)}>
        <span class="lbl" data-text>Rank</span>
        <span class="ico"><RankBadge tier={curTier} px={icons.cur} /></span>
        <span class="val">
          {#if rank === undefined}
            <Skeleton w="3.4em" h="0.8em" />
          {:else if rank && ranked}
            <span class="rr-line" data-text><span class="rrn num">{rank.rr}</span><span class="unit">RR</span></span>
            {#if rrFill !== null}
              <span class="bar"><i style:width="{rrFill}%" style:background={tierColor($meta, rank.tier)}></i></span>
            {/if}
          {/if}
        </span>
      </div>

      <div class="slot prev" data-stat {@attach tip(prevTip)}>
        <span class="lbl" data-text>Prev act</span>
        <span class="ico"><RankBadge tier={card.prevActTier} px={icons.sub} /></span>
        <span class="val">
          {#if card.prevActTier === undefined}
            <Skeleton w="4em" h="0.72em" />
          {:else if prevName}
            <span class="txt" data-text>{prevName}</span>
          {/if}
        </span>
      </div>

      <div class="slot peak" data-stat {@attach tip(peakTip)}>
        <span class="lbl" data-text>Peak</span>
        <span class="ico"><RankBadge tier={peak === undefined ? undefined : (peak?.tier ?? null)} px={icons.sub} /></span>
        <span class="val">
          {#if peak === undefined}
            <Skeleton w="4.4em" h="0.72em" />
          {:else if peak}
            <span class="txt num" data-text>{peakLabel(peak)}</span>
          {/if}
        </span>
      </div>
    </div>

    <div class="perf">
      <div class="stats">
        {@render stat("K/D", card.kd === undefined ? undefined : kd(card.kd), "K/D · last 5 competitive")}
        {@render stat("HS%", card.hsPct === undefined ? undefined : pct(card.hsPct), "Headshot % · last 5 competitive")}
        {@render stat(
          "WR",
          card.actWinrate === undefined ? undefined : pct(card.actWinrate),
          "Win rate · current act",
        )}
      </div>
      <div class="l5"><Last5 items={card.last5} fit={strip ? "fixed" : "fill"} /></div>
    </div>
  </div>
</div>

<style>
  /* ---------- shell ---------- */
  .card {
    position: relative;
    display: grid;
    height: 100%;
    min-width: 0;
    overflow: hidden;
    background: var(--surface-1);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    color: var(--text-1);
    cursor: pointer;
    text-align: left;
    user-select: none;
  }

  .card:hover {
    background: var(--surface-1-hover);
    border-color: var(--line-strong);
  }

  .card.team {
    border-color: color-mix(in srgb, var(--ally) 42%, var(--line));
  }

  .card.team.enemy {
    border-color: color-mix(in srgb, var(--enemy) 42%, var(--line));
  }

  .card.team:hover {
    border-color: color-mix(in srgb, var(--ally) 70%, var(--line));
  }

  .card.team.enemy:hover {
    border-color: color-mix(in srgb, var(--enemy) 70%, var(--line));
  }

  /* Party stripe: solid = confirmed, dashed = likely. Dash gaps are slate, not
     transparent, so the pattern reads the same over any card art. */
  .stripe {
    position: absolute;
    z-index: 3;
    inset: 0 auto 0 0;
    width: 4px;
    background: var(--party);
    box-shadow: 1px 0 0 rgb(8 11 15 / 0.55);
  }

  .stripe.likely {
    background: repeating-linear-gradient(to bottom, var(--party) 0 7px, var(--bg) 7px 11px);
  }

  /* ---------- art: player card + level plate + agent head ---------- */
  .art {
    position: relative;
    overflow: hidden;
    background: var(--surface-2);
  }

  .art-img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .art::after {
    content: "";
    position: absolute;
    inset: 0;
    pointer-events: none;
  }

  .level {
    position: absolute;
    z-index: 1;
    top: 6px;
    left: 6px;
    min-width: 28px;
    padding: 1px 5px;
    border: 1px solid rgb(255 255 255 / 0.1);
    border-radius: var(--r-xs);
    background: var(--scrim);
    color: var(--text-1);
    font-size: var(--fs-xs);
    font-weight: var(--fw-strong);
    line-height: 16px;
    text-align: center;
  }

  .agent {
    position: absolute;
    z-index: 1;
    right: 6px;
    bottom: 6px;
    width: var(--agent);
    height: var(--agent);
    overflow: hidden;
    border: 1px solid var(--line-strong);
    border-radius: var(--r-sm);
    background: linear-gradient(160deg, var(--surface-3), var(--surface-2));
    box-shadow: 0 2px 10px rgb(5 8 12 / 0.5);
  }

  .agent img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  /* ---------- body ---------- */
  .body {
    display: grid;
    min-width: 0;
  }

  .id {
    min-width: 0;
  }

  .lbl {
    display: block;
    color: var(--text-3);
    font-size: var(--fs-xs);
    line-height: 15px;
    max-width: 100%;
    overflow: hidden;
    white-space: nowrap;
    word-spacing: 0.12em;
  }

  .slot {
    display: grid;
    min-width: 0;
  }

  .ico {
    display: flex;
    align-items: center;
  }

  .val {
    display: grid;
    align-content: center;
    justify-items: start;
    min-width: 0;
  }

  .rr-line {
    display: flex;
    align-items: baseline;
    gap: 3px;
    max-width: 100%;
    overflow: hidden;
    white-space: nowrap;
  }

  .rrn {
    font-size: var(--rr-fs);
    font-weight: var(--fw-strong);
    line-height: 1.05;
  }

  .unit {
    color: var(--text-3);
    font-size: var(--fs-xs);
  }

  .bar {
    display: block;
    width: 100%;
    height: 2px;
    margin-top: 3px;
    overflow: hidden;
    border-radius: 1px;
    background: var(--line-strong);
  }

  .bar i {
    display: block;
    height: 100%;
    background: var(--text-2);
  }

  .txt {
    display: block;
    max-width: 100%;
    overflow: hidden;
    color: var(--text-2);
    font-size: var(--sub-fs);
    line-height: 1.2;
    white-space: nowrap;
  }

  .stat {
    display: grid;
    justify-items: start;
    min-width: 0;
  }

  .sval {
    display: flex;
    align-items: center;
    max-width: 100%;
    overflow: hidden;
    min-height: 1.15em;
    font-size: var(--val-fs);
    font-weight: var(--fw-strong);
    line-height: 1.15;
    white-space: nowrap;
  }

  /* ---------- sm: horizontal strip ---------- */
  .sm {
    --agent: 40px;
    --name-fs: var(--fs-lg);
    --tag-fs: var(--fs-md);
    --rr-fs: var(--fs-2xl);
    --val-fs: var(--fs-lg);
    --sub-fs: var(--fs-sm);
    --tile-w: 36px;
    --tile-h: 34px;
    --tile-fs: var(--fs-md);

    grid-template-columns: 76px minmax(0, 1fr);
  }

  .sm .art::after {
    background: linear-gradient(to right, transparent 55%, rgb(20 26 34 / 0.45));
  }

  .sm .body {
    grid-template-rows: auto auto auto;
    align-content: space-between;
    row-gap: 4px;
    padding: 8px 12px;
  }

  .sm .lbl {
    line-height: 14px;
  }

  .sm .ranks {
    display: flex;
    align-items: stretch;
    gap: 18px;
  }

  .sm .slot {
    grid-template-columns: auto auto;
    justify-content: start;
    grid-template-areas:
      "lbl lbl"
      "ico val";
    column-gap: 5px;
    align-content: end;
  }

  .sm .slot .lbl {
    grid-area: lbl;
  }

  .sm .slot .ico {
    grid-area: ico;
  }

  .sm .slot .val {
    grid-area: val;
  }

  .sm .cur {
    grid-template-areas: "ico val";
    column-gap: 7px;
    align-content: center;
  }

  .sm .cur .lbl {
    display: none;
  }

  .sm .perf {
    display: flex;
    align-items: end;
    justify-content: space-between;
    gap: 12px;
  }

  .sm .stats {
    display: flex;
    gap: 14px;
  }

  /* ---------- md / lg: vertical card ---------- */
  .md,
  .lg {
    grid-template-rows: var(--banner-h) minmax(0, 1fr);
  }

  .md {
    --banner-h: 58px;
    --agent: 44px;
    --name-fs: var(--fs-lg);
    --tag-fs: var(--fs-md);
    --rr-fs: var(--fs-xl);
    --val-fs: var(--fs-xl);
    --sub-fs: var(--fs-md);
    --tile-h: 36px;
    --tile-fs: var(--fs-lg);
    --pad-x: 14px;
    --ico-row: 40px;
  }

  .lg {
    --banner-h: 88px;
    --agent: 64px;
    --name-fs: var(--fs-xl);
    --tag-fs: var(--fs-lg);
    --tag-lh: 20px;
    --rr-fs: var(--fs-2xl);
    --val-fs: var(--fs-2xl);
    --sub-fs: var(--fs-lg);
    --tile-h: 46px;
    --tile-fs: var(--fs-lg);
    --tile-gap: 6px;
    --pad-x: 18px;
    --ico-row: 56px;
  }

  .md .level,
  .lg .level {
    top: 8px;
    left: 10px;
  }

  .md .agent,
  .lg .agent {
    right: 10px;
    bottom: 8px;
  }

  .md .art::after,
  .lg .art::after {
    background: linear-gradient(to bottom, transparent 45%, rgb(27 34 44 / 0.55));
  }

  /* One 3-column grid for the whole body: rank slots and stats share columns
     (subgrid), each column at least as wide as its content, equal otherwise. */
  .md .body,
  .lg .body {
    grid-template-columns: repeat(3, minmax(max-content, 1fr));
    grid-template-rows: auto auto auto;
    align-content: space-between;
    column-gap: 10px;
    row-gap: 12px;
    padding: 10px var(--pad-x) 12px;
  }

  .md .id,
  .lg .id,
  .md .l5,
  .lg .l5 {
    grid-column: 1 / -1;
    min-width: 0;
  }

  .lg .lbl {
    font-size: var(--fs-sm);
    line-height: 16px;
  }

  .lg .body {
    row-gap: 20px;
    padding-top: 14px;
    padding-bottom: 18px;
  }

  .md .ranks,
  .lg .ranks,
  .md .perf,
  .lg .perf,
  .md .stats,
  .lg .stats {
    display: grid;
    grid-column: 1 / -1;
    grid-template-columns: subgrid;
  }

  .md .slot,
  .lg .slot {
    grid-template-rows: auto var(--ico-row) auto;
    row-gap: 2px;
  }

  .md .perf {
    row-gap: 8px;
  }

  .lg .perf {
    row-gap: 16px;
  }

  .md .val,
  .lg .val {
    min-height: 20px;
  }
</style>
