<!--
  Competitive · RANK NOW (design v2 TrackerApp.dc.html 668–690): the tier icon and name in the tier colour, "21 / 100 RR ·
  79 to Ascendant 2", the bar toward the next tier; the act's record, net RR and average win / loss (each with its
  tooltip); then the act's first rank ("Act start", from the first RR point) and the latest tier change.
  `stats: undefined` = skeleton; `null` = the frame with `—`.
-->
<script lang="ts">
  import { assetUrl } from "../../lib/assets";
  import { actGames, chartTierName, exactWhen, plural, rankFill, rankSubline, type KnownMatch } from "../../lib/compv2";
  import { ago, DASH, dateShort, rrDelta } from "../../lib/format";
  import { retryImg } from "../../lib/imgRetry";
  import { mapName } from "../../lib/meta";
  import { rrInTier, tierOfAbsolute } from "../../lib/mystats";
  import { meta, now } from "../../lib/stores";
  import { tierColor, tierName } from "../../lib/theme";
  import { tip } from "../../lib/tooltip.svelte";
  import type { MyStats } from "../../lib/types";
  import Skel from "../Skel.svelte";

  let { stats, known }: { stats: MyStats | null | undefined; known: Map<number, KnownMatch> } = $props();

  const r = $derived(stats?.rank ?? null);
  const tier = $derived(typeof r?.tier === "number" ? r.tier : null);
  const color = $derived(tier === null ? "var(--text-muted)" : tierColor(tier));
  const name = $derived(tier === null ? DASH : tierName(tier, $meta));
  const sub = $derived(rankSubline(tier, r?.rr ?? null, r?.rrToNext ?? null, $meta));
  const fill = $derived(rankFill(tier, r?.rr ?? null));
  const games = $derived(stats ? actGames(stats) : null);

  const signed = (n: number | null | undefined) => (typeof n === "number" ? rrDelta(n) : DASH);
  const tone = (n: number | null | undefined) =>
    typeof n !== "number" || Math.round(n) === 0 ? "var(--text)" : n > 0 ? "var(--win)" : "var(--loss)";

  const cells = $derived.by(() => {
    const w = r?.wins ?? null;
    const l = r?.losses ?? null;
    const g = (w ?? 0) + (l ?? 0);
    const series = stats?.rrSeries.length ?? 0;
    return [
      {
        l: "Record",
        v: w === null || l === null ? DASH : `${w}–${l}`,
        c: "var(--text)",
        tip: w === null || l === null ? "" : `${w} wins, ${l} losses` + (g ? ` · ${Math.round((w / g) * 100)}% win rate` : ""),
      },
      {
        l: "Net RR",
        v: signed(r?.netRr),
        c: tone(r?.netRr),
        tip:
          games === null
            ? ""
            : `All ${plural(games, "match", "matches")} this act.` + (series > 40 ? " The chart’s RR covers only its last 40." : ""),
      },
      { l: "Avg win", v: signed(r?.avgWin), c: tone(r?.avgWin), tip: "Average RR gained per win" },
      { l: "Avg loss", v: signed(r?.avgLoss), c: tone(r?.avgLoss), tip: "Average RR lost per loss" },
    ];
  });

  /** "Act start": the rank after the act's first match (the first RR point). */
  const start = $derived.by(() => {
    const p = stats?.rrSeries[0];
    if (!p) return null;
    const t = tierOfAbsolute(p.absoluteRr);
    return { tier: t, name: `${chartTierName(p.absoluteRr, $meta)} · ${rrInTier(p.absoluteRr)} RR`, when: dateShort(p.startMs, $now), exact: exactWhen(p.startMs) };
  });

  /** "Last change": the newest tier change, its map and RR when the snapshot knows that match. */
  const last = $derived.by(() => {
    const c = r?.changes[0];
    if (!c) return null;
    const m = known.get(c.startMs) ?? null;
    const map = m ? mapName($meta, m.mapId) : null;
    const mapText = typeof map === "string" && map ? map : null;
    const rr = m?.rrEarned ?? null;
    const how = `${c.up ? "promoted" : "demoted"}` + (mapText ? ` after ${mapText}` : "") + (typeof rr === "number" ? `, ${rrDelta(rr)} RR` : "");
    return {
      tier: c.tier,
      up: c.up,
      name: tierName(c.tier, $meta),
      ago: ago(c.startMs, $now) + (mapText ? ` · ${mapText}` : ""),
      tip: `${exactWhen(c.startMs)} · ${how}`,
    };
  });
</script>

<section class="card" aria-label="Rank now" aria-busy={stats === undefined}>
  <span class="micro">RANK NOW</span>
  {#if stats === undefined}
    <div class="head">
      <Skel w={56} h={56} r={8} block />
      <div class="names"><Skel w={150} h={22} block /><Skel w={170} h={12} block /></div>
    </div>
    <div class="track"></div>
    <div class="sect">
      <Skel w={150} h={10} block />
      <div class="grid">
        {#each [0, 1, 2, 3] as i (i)}
          <div class="cell"><Skel w={50} h={11} block /><Skel w={56} h={18} block /></div>
        {/each}
      </div>
    </div>
    <div class="sect changes">
      <Skel w={260} h={14} block />
      <Skel w={260} h={14} block />
    </div>
  {:else}
    <div class="head">
      {#if tier !== null}
        <img {@attach retryImg} class="icon" src={assetUrl("tier", tier)} alt="" />
      {:else}
        <span class="icon well"></span>
      {/if}
      <div class="names">
        <span class="tier" style:color={color} data-ellipsis>{name}</span>
        <span class="sub">{sub}</span>
      </div>
    </div>
    <div class="track" use:tip={typeof r?.rrToNext === "number" ? `${r.rrToNext} RR to next tier` : ""}>
      <div class="fill" style:width="{fill}%" style:background={color}></div>
    </div>
    <div class="sect">
      <span class="micro">CURRENT ACT · {games === null ? DASH : plural(games, "MATCH", "MATCHES")}</span>
      <div class="grid">
        {#each cells as c (c.l)}
          <div class="cell" use:tip={c.tip}>
            <span class="lab">{c.l}</span>
            <span class="val" style:color={c.c}>{c.v}</span>
          </div>
        {/each}
      </div>
    </div>
    <div class="sect changes">
      <div class="chg" use:tip={start ? `Rank after your first match this act · ${start.exact}` : ""}>
        <span class="clab">Act start</span>
        <span class="arrow"></span>
        {#if start}
          <img {@attach retryImg} class="cicon" src={assetUrl("tier", start.tier)} alt="" />
          <span class="cname">{start.name}</span>
          <span class="flex"></span>
          <span class="ago" data-ellipsis>{start.when}</span>
        {:else}
          <span class="cname dim">{DASH}</span>
        {/if}
      </div>
      <div class="chg" use:tip={last?.tip ?? ""}>
        <span class="clab">Last change</span>
        {#if last}
          <span class="arrow" style:color={last.up ? "var(--win)" : "var(--loss)"}>{last.up ? "▲" : "▼"}</span>
          <img {@attach retryImg} class="cicon" src={assetUrl("tier", last.tier)} alt="" />
          <span class="cname">{last.name}</span>
          <span class="flex"></span>
          <span class="ago" data-ellipsis>{last.ago}</span>
        {:else}
          <span class="arrow"></span>
          <span class="cname dim">{DASH}</span>
        {/if}
      </div>
    </div>
  {/if}
</section>

<style>
  .card {
    background: var(--card);
    border: 1px solid var(--line);
    border-radius: 12px;
    padding: 16px 18px;
    display: flex;
    flex-direction: column;
    gap: 14px;
    min-width: 0;
  }

  .micro {
    font: 500 10.5px/1 var(--font-mono);
    letter-spacing: 0.06em;
    color: var(--text-muted);
  }

  .head {
    display: flex;
    align-items: center;
    gap: 14px;
  }

  .icon {
    width: 56px;
    height: 56px;
    display: block;
    flex: none;
  }

  .well {
    border-radius: 8px;
    background: var(--skel);
  }

  .names {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }

  .tier {
    font: 700 25px/1 var(--font);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .sub {
    font: 500 13px/1 var(--font);
    color: var(--text-2);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  .track {
    height: 5px;
    border-radius: 3px;
    background: var(--track);
    overflow: hidden;
  }

  .fill {
    height: 100%;
  }

  .sect {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding-top: 12px;
    border-top: 1px solid var(--line);
  }

  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px 14px;
  }

  .cell {
    display: flex;
    flex-direction: column;
    gap: 6px;
    cursor: default;
  }

  .lab {
    font: 500 12.5px/1 var(--font);
    color: var(--text-muted);
  }

  .val {
    font: 700 20px/1 var(--font);
    font-variant-numeric: tabular-nums;
  }

  .changes {
    gap: 9px;
  }

  .chg {
    display: flex;
    align-items: center;
    gap: 8px;
    cursor: default;
    min-width: 0;
  }

  .clab {
    font: 500 12.5px/1 var(--font);
    color: var(--text-muted);
    width: 76px;
    flex: none;
  }

  .arrow {
    font: 700 11px/1 var(--font);
    width: 10px;
    flex: none;
  }

  .cicon {
    width: 18px;
    height: 18px;
    display: block;
    flex: none;
  }

  /* The rank keeps its full name; the time (and map) beside it gives way, and its tooltip has the full detail. */
  .cname {
    font: 600 13.5px/1 var(--font);
    white-space: nowrap;
    flex: none;
  }

  .dim {
    color: var(--text-muted);
  }

  .flex {
    flex: 1 0 0;
  }

  .ago {
    font: 500 12.5px/1 var(--font);
    color: var(--text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }
</style>
