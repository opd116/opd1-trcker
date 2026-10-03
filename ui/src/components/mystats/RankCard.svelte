<!--
  My Stats rank card (spec §9.2): tier icon, tier name, exact RR and W / L; a thin progress bar toward the next tier
  (its tooltip says how many RR are left; none from Immortal up and unranked, but the row keeps its height so the card
  does not change size); Net RR, Avg win and Avg loss; then the latest rank changes (at most 4), newest first.
  `rank: undefined` = the page's skeleton; `null` = Rust knows nothing yet (every value `—`).
  A known tier with an unknown RR draws `— RR`: same markup and place as a number (R154 on the live card).
-->
<script lang="ts">
  import { assetUrl } from "../../lib/assets";
  import { ago, DASH, rr as rrText } from "../../lib/format";
  import { isRankedTier, tierLabel } from "../../lib/meta";
  import { signedRr, signTone } from "../../lib/mystats";
  import { meta, now } from "../../lib/stores";
  import { ACT_SCOPE, rrToNextTip, whenTip } from "../../lib/tips";
  import { tip } from "../../lib/tooltip.svelte";
  import type { RankSummary } from "../../lib/types";
  import Skel from "../Skel.svelte";
  import Panel from "./Panel.svelte";

  let { rank }: { rank: RankSummary | null | undefined } = $props();

  const EMPTY: RankSummary = {
    tier: null,
    rr: null,
    wins: null,
    losses: null,
    rrToNext: null,
    netRr: null,
    avgWin: null,
    avgLoss: null,
    changes: [],
  };

  const r = $derived(rank ?? EMPTY);
  const tier = $derived(isRankedTier(r.tier) ? r.tier : null);
  const tierName = $derived(tier === null ? null : tierLabel($meta, tier));
  /** How far toward the next tier, 0..1; null when there is no next step to show. */
  const progress = $derived(r.rrToNext === null ? null : Math.max(0, Math.min(1, (100 - r.rrToNext) / 100)));
  const cells = $derived([
    { label: "Net RR", value: r.netRr },
    { label: "Avg win", value: r.avgWin },
    { label: "Avg loss", value: r.avgLoss },
  ]);
  const changes = $derived(r.changes.slice(0, 4));
</script>

<Panel title="Rank" name="rank" scope={ACT_SCOPE} grow>
  {#if rank === undefined}
    <div class="rc" aria-hidden="true">
      <div class="head">
        <Skel w={48} h={48} r="50%" block />
        <div class="skcol"><Skel w={90} h={13} block /><Skel w={48} h={10} block /></div>
      </div>
      <div class="barrow"></div>
      <div class="cells">
        {#each [0, 1, 2] as i (i)}
          <div class="cell"><span class="lab"><Skel w={44} h={10} block /></span><span class="cv"><Skel w={44} h={20} block /></span></div>
        {/each}
      </div>
      <div class="chg">
        <span class="lab"><Skel w={64} h={10} block /></span>
        {#each [0, 1, 2, 3] as i (i)}
          <div class="crow"><Skel w={120} h={10} block /></div>
        {/each}
      </div>
    </div>
  {:else}
    <div class="rc">
      <div class="head">
        {#if tier !== null}
          <img class="icon" src={assetUrl("tier", tier)} alt="" />
          <div class="name">
            <span class="tier">{#if tierName === undefined}<Skel w={90} h={13} />{:else}{tierName ?? DASH}{/if}</span>
            <span class="rr"><span class="v">{rrText(r.rr)}</span> RR</span>
          </div>
        {:else}
          <div class="icon"></div>
          <div class="name"><span class="tier dash">{DASH}</span></div>
        {/if}
        <div class="wl" use:tip={ACT_SCOPE}>
          <span class="lab">W / L</span>
          <span class="val"
            ><span class="w">{r.wins ?? DASH}</span><span class="sep">{" / "}</span><span class="l">{r.losses ?? DASH}</span></span
          >
        </div>
      </div>

      <!-- The row keeps its height with no bar (Immortal and up, unranked, an unknown RR). -->
      <div class="barrow">
        {#if progress !== null}
          <div class="bar" use:tip={rrToNextTip(r.rrToNext)}><div class="fill" style:width="{progress * 100}%"></div></div>
        {/if}
      </div>

      <div class="cells">
        {#each cells as c (c.label)}
          <div class="cell" use:tip={ACT_SCOPE}>
            <span class="lab">{c.label}</span>
            <span class={["cv", signTone(c.value)]}>{signedRr(c.value)}</span>
          </div>
        {/each}
      </div>

      <div class="chg">
        <span class="lab">Rank changes</span>
        {#each changes as c (c.startMs)}
          {@const name = tierLabel($meta, c.tier)}
          <div class="crow">
            <svg class={["arrow", c.up ? "up" : "down"]} width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.6" aria-hidden="true"
              ><path d={c.up ? "M2.5 7.5L6 4l3.5 3.5" : "M2.5 4.5L6 8l3.5-3.5"} /></svg
            >
            <span class="cname">{#if name === undefined}<Skel w={60} h={10} />{:else}{name ?? DASH}{/if}</span>
            <span class="cago" use:tip={whenTip(c.startMs)}>{ago(c.startMs, $now)}</span>
          </div>
        {:else}
          <div class="crow"><span class="cname none">{DASH}</span></div>
        {/each}
      </div>
    </div>
  {/if}
</Panel>

<style>
  .rc {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 16px;
  }

  .head {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
  }

  .icon {
    display: block;
    width: 48px;
    height: 48px;
    flex: none;
  }

  .name,
  .skcol {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .skcol {
    gap: 7px;
  }

  .tier {
    font-size: 18px;
    font-weight: 600;
    white-space: nowrap;
  }

  .tier.dash {
    color: var(--text2);
  }

  .rr {
    font-size: 12px;
    font-weight: 500;
    color: var(--text2);
    white-space: nowrap;
  }

  .v {
    font-size: 14px;
    font-weight: 600;
    color: var(--text);
    font-variant-numeric: tabular-nums;
  }

  .wl {
    flex: none;
    display: flex;
    flex-direction: column;
    align-items: flex-end;
  }

  /* A fixed line, so the skeleton's bars and the loaded text take the same room. */
  .lab {
    display: flex;
    align-items: center;
    height: 14px;
    font-size: 12px;
    font-weight: 500;
    color: var(--text2);
    white-space: nowrap;
  }

  .val {
    font-size: 18px;
    line-height: 24px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  .w {
    color: var(--win);
  }

  .l {
    color: var(--loss);
  }

  .sep {
    color: var(--text3);
    font-weight: 500;
  }

  /* A 16px hover target around the 4px bar. */
  .barrow {
    height: 16px;
    margin: -6px 0;
    display: flex;
    align-items: center;
  }

  .bar {
    flex: 1;
    height: 4px;
    border-radius: 2px;
    background: var(--line2);
    overflow: hidden;
  }

  .fill {
    height: 100%;
    border-radius: 2px;
    background: var(--accent);
  }

  .cells {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 8px;
  }

  .cell {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    padding: 8px 10px 9px;
    border: 1px solid var(--line);
    border-radius: 6px;
  }

  .cv {
    display: flex;
    align-items: center;
    height: 24px;
    font-size: 20px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  .cv.win {
    color: var(--win);
  }

  .cv.loss {
    color: var(--loss);
  }

  .cv.flat {
    color: var(--text);
  }

  .chg {
    display: flex;
    flex-direction: column;
    padding-top: 12px;
    border-top: 1px solid var(--line);
  }

  .crow {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 28px;
    min-width: 0;
  }

  .arrow {
    flex: none;
    display: block;
  }

  .arrow.up {
    color: var(--win);
  }

  .arrow.down {
    color: var(--loss);
  }

  .cname {
    flex: 1;
    min-width: 0;
    font-size: 14px;
    font-weight: 500;
    white-space: nowrap;
  }

  .cname.none {
    color: var(--text2);
  }

  .cago {
    flex: none;
    font-size: 12px;
    font-weight: 500;
    color: var(--text2);
    white-space: nowrap;
  }
</style>
