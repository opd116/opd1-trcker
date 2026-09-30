<!--
  Player Details hero (spec §6.2, v8:170-240): card art, agent, name + level + ⟲, current / Peak / Prev rank, then the
  stats strip (K/D, HS%, Win%, Games, W / L) and the last 5 at hero size. K/D and HS% are the details' own act figures
  (R116); Win%, Games and W / L the card's act-wide ones. A card field still absent is its bar (§6.2, R23);
  `details: null` is the Details skeleton's hero (§6.9).
-->
<script lang="ts">
  import { assetUrl } from "../../lib/assets";
  import { kd2, level, pct0, peakLabel, rr } from "../../lib/format";
  import { showEncounters } from "../../lib/live";
  import { isRankedTier, tierLabel } from "../../lib/meta";
  import { meta } from "../../lib/stores";
  import { actScopeTip, heroStatTip, peakTip, winTip } from "../../lib/tips";
  import { tip } from "../../lib/tooltip.svelte";
  import type { PlayerDetails } from "../../lib/types";
  import EncounterBadge from "../EncounterBadge.svelte";
  import Last5 from "../Last5.svelte";
  import Name from "../Name.svelte";
  import Skel from "../Skel.svelte";

  let { details }: { details: PlayerDetails | null } = $props();

  const card = $derived(details?.card);
  const ranked = $derived(card?.rank && isRankedTier(card.rank.tier) ? card.rank : null);
  const rankName = $derived(ranked ? tierLabel($meta, ranked.tier) : null);
  const peak = $derived(card?.peak);
  const peakName = $derived(peak ? tierLabel($meta, peak.tier) : null);
  const prev = $derived(card?.prevActTier);
  const prevName = $derived(isRankedTier(prev) ? tierLabel($meta, prev) : null);

  /** W / L from the act's wins and games: losses = games − wins. Absent while either is loading. */
  const wl = $derived.by(() => {
    if (!card || card.actWins === undefined || card.actGames === undefined) return undefined;
    const w = card.actWins;
    const g = card.actGames;
    return { w, l: w !== null && g !== null ? g - w : null };
  });

  type Stat = { label: string; loading: boolean; text: string; tip: string };
  const stats: Stat[] = $derived.by(() => {
    if (!details || !card) return [];
    const n = details.heroMatches;
    return [
      { label: "K/D", loading: false, text: kd2(details.heroKd), tip: heroStatTip(details.heroKd, n) },
      { label: "HS%", loading: false, text: pct0(details.heroHs), tip: heroStatTip(details.heroHs, n) },
      { label: "Win%", loading: card.actWinrate === undefined, text: pct0(card.actWinrate), tip: winTip(card) },
      { label: "Games", loading: card.actGames === undefined, text: level(card.actGames), tip: actScopeTip(card.actGames) },
      { label: "W / L", loading: wl === undefined, text: "", tip: wl === undefined ? "" : actScopeTip(card.actGames) },
    ];
  });
</script>

{#if details && card}
  <div class="hero">
    <div class="hart">
      <div class="hart-img" style:background-image={`url("${assetUrl("cardLarge", card.cardId)}")`}></div>
      <div class="hart-fade"></div>
    </div>
    {#if card.agentId}
      <img class="hagent" src={assetUrl("agent", card.agentId)} alt="" />
    {:else}
      <div class="hagent"></div>
    {/if}
    <div class="htop">
      <div class="hname">
        <Name name={card.name} size="hero" />
        <div class="hmeta">
          <span class="hlv">{#if card.level === undefined}<Skel w={48} h={10} />{:else}Lv <span class="v">{level(card.level)}</span>{/if}</span>
          {#if card.encounters && showEncounters(card)}
            <EncounterBadge enc={card.encounters} size="hero" />
          {/if}
        </div>
      </div>

      <div class="hrank">
        {#if card.rank === undefined}
          <Skel w={48} h={48} r="50%" block />
          <Skel w={90} h={13} block />
        {:else if !ranked}
          <div class="ri"></div>
          <span class="rdash">—</span>
        {:else}
          <img class="ri" src={assetUrl("tier", ranked.tier)} alt="" />
          <div class="rcol">
            <span class="rname">{#if rankName === undefined}<Skel w={90} h={13} />{:else}{rankName ?? "—"}{/if}</span>
            <span class="rr"><span class="v">{rr(ranked.rr)}</span> RR</span>
          </div>
        {/if}
      </div>

      <!-- The tooltip covers the whole Peak block [C]; the design puts it on the icon, with the dates only. -->
      <div class="hpeak" use:tip={peak ? peakTip(peak) : ""}>
        {#if peak === undefined}
          <Skel w={28} h={28} r="50%" block />
          <Skel w={90} h={13} block />
        {:else if peak === null}
          <div class="prow"><span class="lbl">Peak</span><span class="pdash">—</span></div>
        {:else}
          <img class="pi" src={assetUrl("tier", peak.tier)} alt="" />
          <div class="pcol">
            <div class="prow">
              <span class="lbl">Peak</span><span class="pname">{#if peakName === undefined}<Skel w={90} h={13} />{:else}{peakName ?? "—"}{/if}</span>
            </div>
            <span class="when">{peakLabel(peak)}</span>
          </div>
        {/if}
      </div>

      <div class="hprev">
        {#if prev === undefined}
          <!-- [C] The Prev block's own shapes: its 24px icon as a circle, then a name bar. -->
          <Skel w={24} h={24} r="50%" block />
          <Skel w={90} h={13} block />
        {:else}
          {#if isRankedTier(prev)}<img class="vi" src={assetUrl("tier", prev)} alt="" />{/if}
          <div class="prow">
            <span class="lbl">Prev</span>
            {#if isRankedTier(prev)}
              <span class="vname">{#if prevName === undefined}<Skel w={90} h={13} />{:else}{prevName ?? "—"}{/if}</span>
            {:else}
              <span class="vname">—</span>
            {/if}
          </div>
        {/if}
      </div>
    </div>

    <div class="hstats">
      {#each stats as s (s.label)}
        <div class="hstat" use:tip={s.loading ? "" : s.tip}>
          <div class="hcol">
            <span class="k">{s.label}</span>
            {#if s.loading}
              <Skel w={44} h={27} block />
            {:else if s.label === "W / L" && wl}
              <span class="v"><span class="w">{wl.w ?? "—"}</span><span class="sep">{" / "}</span><span class="l">{wl.l ?? "—"}</span></span>
            {:else}
              <span class="v">{s.text}</span>
            {/if}
          </div>
        </div>
      {/each}
      <div class="hl5"><Last5 entries={card.last5} size="hero" /></div>
    </div>
  </div>
{:else}
  <!-- The Details skeleton's hero (spec §6.9): the same boxes, grey shapes, no text. -->
  <div class="hero hsk" aria-hidden="true">
    <div class="hagent sk"></div>
    <div class="htop">
      <div class="hname"><Skel w={160} h={18} block /><Skel w={48} h={10} block /></div>
      <div class="hrank"><Skel w={48} h={48} r="50%" block /><Skel w={90} h={13} block /></div>
      <div class="hpeak"><Skel w={28} h={28} r="50%" block /><Skel w={90} h={13} block /></div>
      <div class="hprev"><Skel w={24} h={24} r="50%" block /><Skel w={90} h={13} block /></div>
    </div>
    <div class="hstats">
      {#each [0, 1, 2, 3, 4] as i (i)}
        <div class="hstat"><Skel w={44} h={27} block /></div>
      {/each}
      <div class="hl5"><Skel w={154} h={26} r={4} block /></div>
    </div>
  </div>
{/if}

<style>
  .hero {
    position: relative;
    background: var(--card);
    border-radius: 8px;
    overflow: hidden;
    margin-bottom: 20px;
  }

  .hart {
    position: absolute;
    left: 0;
    top: 0;
    bottom: 0;
    width: 84px;
    overflow: hidden;
  }

  .hart-img,
  .hart-fade {
    position: absolute;
    inset: 0;
  }

  .hart-img {
    background-size: 130% auto;
    background-position: 50% 22%;
    background-repeat: no-repeat;
    opacity: 0.6;
    filter: saturate(0.65);
  }

  .hart-fade {
    background: var(--strip-fade);
  }

  .hagent {
    position: absolute;
    left: 12px;
    top: 18px;
    display: block;
    width: 56px;
    height: 56px;
    border-radius: 7px;
    background: var(--raised);
    box-shadow: 0 0 0 2px var(--card);
  }

  .hagent.sk {
    background: var(--skel);
  }

  .htop {
    position: relative;
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto auto auto;
    align-items: center;
    column-gap: 30px;
    min-height: 56px;
    padding: 18px 22px 0 96px;
  }

  .hname {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .hmeta {
    display: flex;
    align-items: center;
    font-size: 12px;
    font-weight: 500;
    color: var(--text2);
    white-space: nowrap;
  }

  .hlv {
    width: 64px;
    flex: none;
  }

  .hlv .v {
    font-size: 14px;
    color: var(--text);
    font-variant-numeric: tabular-nums;
  }

  .hrank {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .ri {
    display: block;
    width: 48px;
    height: 48px;
    flex: none;
  }

  .rcol,
  .pcol {
    display: flex;
    flex-direction: column;
    gap: 2px;
    white-space: nowrap;
  }

  .rname {
    font-size: 18px;
    font-weight: 600;
  }

  .rr {
    font-size: 12px;
    font-weight: 500;
    color: var(--text2);
  }

  .rr .v {
    font-size: 14px;
    font-weight: 600;
    color: var(--text);
    font-variant-numeric: tabular-nums;
  }

  .rdash {
    font-size: 18px;
    font-weight: 600;
    color: var(--text2);
  }

  .hpeak,
  .hprev {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .pi {
    display: block;
    width: 28px;
    height: 28px;
    flex: none;
  }

  .prow {
    display: flex;
    align-items: baseline;
    gap: 6px;
    white-space: nowrap;
  }

  .lbl {
    font-size: 12px;
    font-weight: 500;
    color: var(--text2);
  }

  .pname {
    font-size: 15px;
    font-weight: 600;
  }

  .pdash {
    font-size: 15px;
    font-weight: 600;
    color: var(--text2);
  }

  .when {
    font-size: 12px;
    font-weight: 500;
    color: var(--text2);
    font-variant-numeric: tabular-nums;
  }

  .vi {
    display: block;
    width: 24px;
    height: 24px;
    flex: none;
    filter: grayscale(0.7);
    opacity: 0.75;
  }

  /* The design leaves the `—` at the inherited weight 400; 400 is not bundled, so it is pinned to 500 (spec §1.7). */
  .vname {
    font-size: 15px;
    font-weight: 500;
    color: var(--text2);
  }

  .hstats {
    position: relative;
    display: grid;
    grid-template-columns: repeat(5, minmax(0, 1fr)) 196px;
    margin: 18px 0 0 96px;
    border-top: 1px solid var(--line);
  }

  .hstat {
    padding: 12px 16px 14px 18px;
    border-left: 1px solid var(--line);
  }

  .hstat:first-child {
    padding-left: 0;
    border-left: none;
  }

  .hcol {
    display: flex;
    flex-direction: column;
  }

  .k {
    font-size: 12px;
    line-height: 14px;
    font-weight: 500;
    color: var(--text2);
  }

  .hcol .v {
    font-size: 22px;
    line-height: 27px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  .w {
    color: var(--win);
  }

  .sep {
    color: var(--text3);
    font-weight: 500;
  }

  .l {
    color: var(--loss);
  }

  .hl5 {
    display: flex;
    align-items: center;
    padding: 12px 20px 14px 20px;
    border-left: 1px solid var(--line);
  }
</style>
