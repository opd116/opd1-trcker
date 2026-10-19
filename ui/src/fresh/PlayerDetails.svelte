<script lang="ts">
  import { asset, displayName, rankName, mapName, agentName, ago, dateShort, kd2, pct0, level, rr, peakLabel, score, recentShown, meta, type PlayerDetails, type DisplayName } from "./data";
  import { nameTip, peakTip, heroStatTip, encounterTip, last5Tip } from "../lib/tips";
  import { DEMO_TIME } from "./data";

  let { data, onback, onmatch, more = $bindable(false) }: { data: PlayerDetails | null; onback: () => void; onmatch: (id: string) => void; more?: boolean } = $props();
  const c = $derived(data?.card);
  const recent = $derived(data ? recentShown(data.recent, more) : null);
  const wins = $derived(c?.actWins);
  const games = $derived(c?.actGames);
  const losses = $derived(typeof wins === "number" && typeof games === "number" ? games - wins : null);
  const weapons = ["vandal", "phantom", "operator", "sheriff"] as const;
  const number = (v: number | null | undefined) => v == null ? "—" : String(v);
  const result = { win: "W", loss: "L", draw: "D" };
</script>

{#snippet identity(value: DisplayName | null | undefined, wrap = false)}
  {@const n = displayName(value)}
  <span class:wrap class="identity" title={value ? nameTip(value, meta) : undefined}>
    {#if n.hidden}<svg class="eye" width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" aria-label="Hidden name"><path d="m2 2 12 12M6.5 3.7C11.3 2.8 14 8 14 8s-.8 1.5-2.2 2.8M9.4 12.2C4.8 13.1 2 8 2 8s.7-1.4 2-2.6"/><path d="M6.6 6.6a2 2 0 0 0 2.8 2.8"/></svg>{/if}
    {#if value === undefined}<span class="skeleton small" aria-label="Loading identity"></span>{:else}<span class="name" class:remembered={n.remembered} class:hidden={n.hidden && !n.remembered}>{n.name}</span>{/if}
    {#if n.tag}<span class="tag">#{n.tag}</span>{/if}
  </span>
{/snippet}

{#snippet rank(tier: number | null | undefined)}
  <span class="rank-inline">{#if tier != null && tier >= 3}<img src={asset("tier", tier)} alt="" />{/if}<span>{rankName(tier)}</span></span>
{/snippet}

<div class="fresh-details" aria-busy={data === null}>
  <button class="back" onclick={onback}><span aria-hidden="true">‹</span> Back</button>
  {#if data && c}
    <section class="hero" aria-label="Player overview">
      <div class="profile">
        {#if c.agentId}<img class="portrait" src={asset("agent", c.agentId)} alt={agentName(c.agentId)} />{/if}
        <div class="profile-copy">
          <h1>{@render identity(c.name)}</h1>
          <div class="subline">
            {#if c.level === undefined}<span class="skeleton small"></span>{:else}<span>Lv {level(c.level)}</span>{/if}
            {#if !c.isSelf && !c.isSelfParty && c.encounters && c.encounters.with + c.encounters.against > 0}
              <span title={encounterTip(c.encounters)}>⟲ {c.encounters.with + c.encounters.against}</span>
            {/if}
          </div>
        </div>
      </div>
      <div class="current">
        {#if c.rank === undefined}<span class="skeleton rank-skeleton"></span>{:else}
          {#if c.rank && c.rank.tier >= 3}<img src={asset("tier", c.rank.tier)} alt="" />{/if}
          <div><span class="label">Current rank</span><strong>{rankName(c.rank?.tier)}</strong><span class="rr">{rr(c.rank?.rr)} <span>RR</span></span></div>
        {/if}
      </div>
      <div class="rank-history">
        <div><span class="label">Previous act</span>{#if c.prevActTier === undefined}<span class="skeleton small"></span>{:else}{@render rank(c.prevActTier)}{/if}</div>
        <div title={c.peak ? peakTip(c.peak) : undefined}><span class="label">Peak</span>{#if c.peak === undefined}<span class="skeleton small"></span>{:else}{@render rank(c.peak?.tier)}{#if c.peak}<span class="peak-date">{peakLabel(c.peak)}</span>{/if}{/if}</div>
      </div>
    </section>

    <section class="performance" aria-label="Performance">
      <div class="combat">
        <div title={heroStatTip(data.heroKd, data.heroMatches)}><strong>{kd2(data.heroKd)}</strong><span class="label">K/D</span></div>
        <div title={heroStatTip(data.heroHs, data.heroMatches)}><strong>{pct0(data.heroHs)}</strong><span class="label">Headshots</span></div>
      </div>
      <div class="record" title={`This act · ${number(wins)} W · ${number(losses)} L · ${number(games)} games`}>
        <div><strong>{pct0(c.actWinrate)}</strong><span class="label">Win rate</span></div>
        <p>{number(games)} games <span>·</span> {number(wins)} W <span>/</span> {number(losses)} L</p>
      </div>
      <div class="form" aria-label="Last five competitive results">
        {#if c.last5 === undefined}<span class="skeleton form-skeleton"></span>{:else}
          {#each Array(Math.max(0, 5 - (c.last5?.length ?? 0))) as _}<span class="result empty"></span>{/each}
          {#each [...(c.last5 ?? [])].slice(0, 5).reverse() as m, i}
            <span class="result" class:win={m.result === "win"} class:loss={m.result === "loss"} class:latest={i === Math.min(4, (c.last5?.length ?? 0) - 1)} title={last5Tip(m, DEMO_TIME)}>{result[m.result]}</span>
          {/each}
        {/if}
      </div>
    </section>

    <div class="columns">
      <div class="primary">
        <section class="panel recent">
          <h2>Recent competitive matches</h2>
          <div class="match-head match-grid"><span>Map</span><span></span><span>Result</span><span>K / D / A</span><span>K/D</span><span>HS%</span><span></span></div>
          {#each recent?.rows ?? [] as m (m.matchId)}
            <button class="match-row match-grid" onclick={() => onmatch(m.matchId)} aria-label={`${mapName(m.mapId)} · ${m.result} · ${score(m.roundsWon, m.roundsLost)}`}>
              <span class="map-cell">{#if m.mapId}<img src={asset("mapList", m.mapId)} alt="" />{/if}<span><strong>{mapName(m.mapId)}</strong><small>{ago(m.startMs)}</small></span></span>
              <span>{#if m.agentId}<img class="agent" src={asset("agent", m.agentId)} alt={agentName(m.agentId)} />{/if}</span>
              <span class="match-result"><span class="outcome" class:win={m.result === "win"} class:loss={m.result === "loss"}>{result[m.result]}</span><strong>{score(m.roundsWon, m.roundsLost)}</strong></span>
              <span class="kda">{number(m.kills)} <em>/</em> {number(m.deaths)} <em>/</em> {number(m.assists)}</span>
              <strong>{kd2(m.kd)}</strong><span>{pct0(m.hsPct)}</span>
              <span>{#if m.tier != null && m.tier >= 3}<img class="tier" src={asset("tier", m.tier)} alt={rankName(m.tier)} title={rankName(m.tier)} />{/if}</span>
            </button>
          {:else}<div class="empty-row">—</div>{/each}
          {#if recent?.showMore}<button class="show-more" onclick={() => more = true}>Show more <span aria-hidden="true">⌄</span></button>{/if}
        </section>

        {#if !c.isSelf && data.encounters.length}
          <section class="panel encounters">
            <h2>Encounters</h2>
            <div class="enc-head enc-grid"><span>Map</span><span>Side</span><span>You</span><span>{displayName(c.name).name}</span><span>Your result</span></div>
            {#each data.encounters as e (e.matchId)}
              <button class="enc-row enc-grid" onclick={() => onmatch(e.matchId)}>
                <span class="map-cell">{#if e.mapId}<img src={asset("mapList", e.mapId)} alt="" />{/if}<span><strong>{mapName(e.mapId)}</strong><small>{dateShort(e.startMs)}</small></span></span>
                <span class="side">{e.with ? "With" : "Against"}</span>
                <span class="enc-player">{#if e.selfAgentId}<img class="agent" src={asset("agent", e.selfAgentId)} alt={agentName(e.selfAgentId)} />{/if}{@render rank(e.selfTier)}</span>
                <span class="enc-player">{#if e.themAgentId}<img class="agent" src={asset("agent", e.themAgentId)} alt={agentName(e.themAgentId)} />{/if}{@render rank(e.themTier)}</span>
                <span class="match-result"><span class="outcome" class:win={e.result === "win"} class:loss={e.result === "loss"}>{result[e.result]}</span><strong>{score(e.roundsWon, e.roundsLost)}</strong></span>
              </button>
            {/each}
          </section>
        {/if}
      </div>
      <aside>
        <section class="panel teammates"><h2>Top 3 teammates</h2>
          {#each data.topTeammates.slice(0, 3) as m (m.puuid)}
            <div class="teammate">{@render identity(m.name, true)}<span class="shared" title="Shared competitive matches · this act"><strong>{m.count}</strong><small>games</small></span></div>
          {:else}<div class="empty-row">—</div>{/each}
        </section>
        <section class="panel loadout"><h2>Loadout</h2>
          {#each weapons as weapon}
            {@const skin = data.loadout[weapon]}
            <div class="weapon-row">
              {#if skin === undefined}<span class="skeleton weapon-skeleton"></span>{:else}
                <div class="weapon-art">{#if skin}<img src={asset("chroma", skin.chromaId)} alt="" />{/if}</div>
                <div class="weapon-copy"><span class="weapon-name">{weapon}</span><span class="skin-name" title={skin ? `${skin.skinName}${skin.chromaName ? ` · ${skin.chromaName}` : ""}` : undefined}>{skin?.skinName ?? "—"}{#if skin?.chromaName}<small> · {skin.chromaName}</small>{/if}</span></div>
              {/if}
            </div>
          {/each}
        </section>
      </aside>
    </div>
  {:else}
    <div class="loading" aria-hidden="true"><div class="skeleton loading-hero"></div><div class="skeleton loading-stats"></div><div class="loading-columns"><div class="skeleton loading-table"></div><div class="skeleton loading-table"></div></div></div>
  {/if}
</div>

<style>
  .fresh-details{flex:1;min-height:0;overflow-y:auto;padding:20px 24px 28px;box-sizing:border-box;color:#eeeeee;font-size:13px;font-variant-numeric:tabular-nums}
  .back{display:flex;align-items:center;gap:8px;height:30px;margin-bottom:18px;padding:0 8px;color:#b6b6b6;font-size:13px;border:none;background:transparent;cursor:pointer}.back>span{font-size:22px}.back:hover{color:#eeeeee}
  .hero{display:grid;grid-template-columns:minmax(0,1fr) 228px 252px;align-items:center;gap:24px;min-height:112px;padding:16px 20px;background:#1d1d1d;border:1px solid #2c3236;border-radius:8px;box-sizing:border-box}
  .profile{display:flex;align-items:center;gap:16px;min-width:0}.portrait{width:58px;height:58px;object-fit:cover;border-radius:7px}.profile-copy{min-width:0;flex:1}h1{font-size:22px;line-height:1.35;font-weight:600;margin:0 0 8px}.identity{display:flex;align-items:baseline;gap:6px;min-width:0}.name{min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.tag{flex:none;color:#b6b6b6;font-size:12px;font-weight:400}.eye{flex:none;align-self:center;color:#b6b6b6}.hidden{font-style:italic}.remembered{text-decoration:underline dotted;text-underline-offset:4px}.wrap{display:block;overflow-wrap:anywhere}.wrap .name{white-space:normal;overflow:visible}.wrap .tag{display:inline-block;white-space:nowrap}.wrap .eye{vertical-align:-2px;margin-right:4px}.subline{display:flex;gap:20px;color:#b6b6b6;font-size:12px}
  .current{display:flex;gap:12px;align-items:center;border-left:1px solid #343a3e;padding-left:24px}.current>img{width:50px;height:50px}.current>div{display:flex;flex-direction:column;gap:4px}.label{display:block;color:#b6b6b6;font-size:12px;font-weight:400}.current strong{font-size:20px;font-weight:600;white-space:nowrap}.rr{font-size:15px;color:#eeeeee}.rr>span{font-size:12px;color:#b6b6b6}.rank-history{display:flex;justify-content:space-between;gap:20px}.rank-history>div{display:flex;flex-direction:column;gap:7px;min-width:0}.rank-inline{display:flex;align-items:center;gap:5px;white-space:nowrap;font-size:12px}.rank-inline>img{width:20px;height:20px;flex:none}.peak-date{color:#b6b6b6;font-size:12px}
  .performance{display:flex;align-items:center;gap:30px;padding:23px 20px 25px;margin-bottom:8px}.performance strong{font-size:25px;line-height:1.2;font-weight:500}.combat{display:flex;gap:38px}.combat .label{margin-top:7px}.record{display:flex;align-items:center;gap:18px;border-left:1px solid #343a3e;padding-left:30px}.record .label{margin-top:7px}.record p{margin:0;color:#b6b6b6;white-space:nowrap;font-size:12px}.record p>span{margin:0 5px;color:#657178}.form{margin-left:auto;display:flex;gap:6px}.result{display:flex;align-items:center;justify-content:center;width:26px;height:28px;border-radius:4px;background:#2a3034;font-size:12px;font-weight:600}.win{color:#a8c3ac}.loss{color:#d3a09d}.result.win{background:#29392f}.result.loss{background:#3b2e2e}.result.latest{outline:1px solid currentColor;outline-offset:3px}.result.empty{background:transparent;border:1px dashed #343a3e;box-sizing:border-box}
  .columns{display:grid;grid-template-columns:minmax(0,1fr) 282px;gap:20px;align-items:start}.primary,aside{min-width:0;display:flex;flex-direction:column;gap:20px}.panel{background:#1d1d1d;border:1px solid #2c3236;border-radius:8px;overflow:hidden}h2{font-size:14px;line-height:20px;font-weight:600;margin:0;padding:15px 16px;border-bottom:1px solid #2c3236}.match-grid{display:grid;grid-template-columns:minmax(120px,1fr) 26px 80px 90px 42px 40px 22px;gap:8px;align-items:center;padding:0 12px}.match-head,.enc-head{height:34px;color:#b6b6b6;font-size:12px}.match-head>*:nth-child(n+4),.match-row>*:nth-child(n+4){text-align:right}.match-head>*:nth-child(4),.match-row>*:nth-child(4){text-align:center}.match-row,.enc-row{width:100%;border:0;border-top:1px solid #2c3236;background:transparent;color:inherit;font:inherit;min-height:64px;text-align:left;cursor:pointer}.match-row:hover,.enc-row:hover{background:#252b2f}.map-cell{display:flex;align-items:center;gap:9px;min-width:0}.map-cell>img{width:48px;height:34px;object-fit:cover;border-radius:4px;flex:none}.map-cell>span{display:flex;flex-direction:column;gap:5px;min-width:0}.map-cell strong{font-size:13px;font-weight:500;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}small{font-size:12px;color:#b6b6b6}.agent{display:block;width:26px;height:26px;border-radius:4px;object-fit:cover}.tier{width:22px;height:22px;display:block}.match-result{display:flex;align-items:center;gap:7px;white-space:nowrap}.match-result strong{font-weight:500;font-size:13px}.outcome{font-size:12px;font-weight:600}.kda{white-space:nowrap;font-size:12px}.kda em{color:#657178;font-style:normal;margin:0 2px}.match-row>strong{font-weight:500;font-size:13px}.show-more{display:flex;align-items:center;justify-content:center;gap:7px;width:100%;height:38px;border:0;border-top:1px solid #2c3236;background:transparent;color:#dedede;cursor:pointer;font-size:12px}.show-more:hover{background:#252b2f}.empty-row{padding:22px 16px;color:#b6b6b6}
  .teammate{display:flex;align-items:center;gap:14px;padding:17px 16px;min-height:24px;border-top:1px solid #2c3236}.teammate:first-of-type{border-top:none}.teammate>.identity{flex:1;font-size:13px;line-height:20px}.shared{flex:none;display:flex;align-items:baseline;gap:5px}.shared strong{font-size:15px;font-weight:500}.weapon-row{display:flex;align-items:center;gap:12px;padding:11px 14px;min-height:44px;border-top:1px solid #2c3236}.weapon-row:first-of-type{border-top:none}.weapon-art{width:100px;height:44px;flex:none;display:flex;align-items:center;justify-content:center}.weapon-art img{max-width:100%;max-height:38px}.weapon-copy{display:flex;flex-direction:column;gap:6px;min-width:0}.weapon-name{text-transform:capitalize;font-size:12px;color:#b6b6b6}.skin-name{font-size:12px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.skin-name small{color:#b6b6b6}
  .enc-grid{display:grid;grid-template-columns:minmax(120px,1fr) 54px 126px 126px 77px;gap:8px;align-items:center;padding:0 12px}.enc-head>*{white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.enc-row{min-height:66px}.side{font-size:12px;color:#b6b6b6}.enc-player{display:flex;align-items:center;gap:5px;min-width:0}.enc-player .rank-inline{flex-direction:column;align-items:flex-start;gap:3px;font-size:12px}.enc-player :global(.rank-inline img){display:none}
  .skeleton{display:block;background:#2b3135;border-radius:4px}.small{width:54px;height:12px}.rank-skeleton{width:160px;height:54px}.form-skeleton{width:154px;height:28px}.weapon-skeleton{width:100%;height:44px}.loading-hero{height:112px}.loading-stats{height:75px;margin:20px 0}.loading-columns{display:grid;grid-template-columns:1fr 282px;gap:20px}.loading-table{height:270px}
</style>
