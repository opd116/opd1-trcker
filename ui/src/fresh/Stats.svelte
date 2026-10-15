<script lang="ts">
  import type { MyStats, RrPoint } from "../lib/types";
  import { agentName, asset, dateShort, kd2, mapName, pct0, rankName, rr, rrDelta, score } from "./data";

  let { data, onmatch }: { data: MyStats | null | undefined; onmatch: (id: string) => void } = $props();
  let showSession = $state(false);
  const days = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
  const periods = ["Morning", "Afternoon", "Evening", "Night"];
  const hours = ["05–12", "12–17", "17–22", "22–05"];
  const number = (value: number | null | undefined) => value == null ? "—" : String(Math.round(value));
  const tone = (value: number | null | undefined) => value == null || value === 0 ? "" : value > 0 ? "positive" : "negative";

  function chartFor(series: readonly RrPoint[]) {
    if (!series.length) return null;
    const first = series[0]!;
    const last = series[series.length - 1]!;
    const changes = series.map(point => point.absoluteRr - first.absoluteRr);
    const low = Math.min(0, ...changes);
    const high = Math.max(0, ...changes);
    const padding = Math.max(12, (high - low) * .12);
    const min = Math.floor((low - padding) / 10) * 10;
    const max = Math.ceil((high + padding) / 10) * 10;
    const y = (change: number) => 152 - (change - min) / (max - min) * 132;
    const points = series.map((point, index) => ({
      x: last.startMs === first.startMs ? 320 : 60 + (point.startMs - first.startMs) / (last.startMs - first.startMs) * 552,
      y: y(changes[index]!), change: changes[index]!, startMs: point.startMs,
    }));
    return {
      points, line: points.map(point => `${point.x},${point.y}`).join(" "),
      ticks: Array.from({ length: 4 }, (_, index) => {
        const value = min + (max - min) * index / 3;
        return { y: y(value), label: rrDelta(value) };
      }),
      first: first.startMs, last: last.startMs,
    };
  }
  let chart = $derived(chartFor(data?.rrSeries ?? []));
</script>

<div class="stats">
  {#if data === undefined}
    <div class="skeleton" aria-label="Loading statistics" aria-busy="true">
      <div class="skeleton-heading"></div>
      <div class="skeleton-overview"><div></div><div></div></div>
      <div class="skeleton-session"></div>
      {#each Array(5) as _}<div class="skeleton-row"></div>{/each}
    </div>
  {:else}
    <header class="page-heading"><h1>Your progress</h1><span>Competitive</span></header>

    <div class="overview">
      <section class="rank-summary" aria-label="Current rank">
        <div class="rank-heading">
          {#if data?.rank.tier && data.rank.tier >= 3}<img class="rank-icon" src={asset("tier", data.rank.tier)} alt="" />{/if}
          <div><h2>{rankName(data?.rank.tier)}</h2><div class="rating">{rr(data?.rank.rr)} <span>RR</span></div></div>
        </div>
        <div class="rank-facts">
          <div><span>Record</span><strong>{number(data?.rank.wins)} <small>W</small> · {number(data?.rank.losses)} <small>L</small></strong></div>
          <div><span>Net RR</span><strong class={tone(data?.rank.netRr)}>{rrDelta(data?.rank.netRr)}</strong></div>
          <div><span>Next rank</span><strong>{rr(data?.rank.rrToNext)} <small>RR</small></strong></div>
          <div><span>Avg. win / loss</span><strong><i class="positive">{rrDelta(data?.rank.avgWin)}</i> / <i class="negative">{rrDelta(data?.rank.avgLoss)}</i></strong></div>
        </div>
      </section>

      <section class="progression" aria-label="RR progression">
        <div class="section-heading"><h2 title="RR change relative to the first recorded chart point">RR progression</h2><span title="Change from the first recorded chart point">{chart ? `RR change since ${dateShort(chart.first)}` : 'Δ RR'}</span></div>
        {#if chart}
          <svg viewBox="0 0 640 186" role="img" aria-labelledby="rr-chart-title rr-chart-description">
            <title id="rr-chart-title">RR progression</title>
            <desc id="rr-chart-description">Change from the first recorded match, from {dateShort(chart.first)} to {dateShort(chart.last)}. {chart.points.length} recorded points.</desc>
            {#each chart.ticks as tick}
              <line class="gridline" x1="60" x2="612" y1={tick.y} y2={tick.y} />
              <text x="47" y={tick.y + 4} text-anchor="end">{tick.label}</text>
            {/each}
            {#if chart.points.length > 1}<polyline class="trend" points={chart.line} />{/if}
            {#each chart.points as point}
              <circle class="point" cx={point.x} cy={point.y} r={chart.points.length > 50 ? 1.5 : 2.5}>
                <title>{dateShort(point.startMs)} · {rrDelta(point.change)} RR from first point</title>
              </circle>
            {/each}
            <text x="60" y="179">{dateShort(chart.first)}</text>
            {#if chart.first !== chart.last}<text x="612" y="179" text-anchor="end">{dateShort(chart.last)}</text>{/if}
          </svg>
        {:else}
          <div class="empty-chart" aria-label="No RR progression data"><span>—</span></div>
        {/if}
      </section>
    </div>

    <section class="session" aria-label="Session">
      <div class="session-summary"><h2>Session</h2><strong class={tone(data?.session?.netRr)}>{rrDelta(data?.session?.netRr)} <small>RR</small></strong><span>{data?.session ? `${data.session.wins} W · ${data.session.losses} L` : "—"}</span></div>
      <div class="session-matches">
        {#each showSession ? data?.session?.matches ?? [] : (data?.session?.matches ?? []).slice(-5) as match}
          <button class:positive={match.result === "W"} class:negative={match.result === "L"} onclick={() => onmatch(match.matchId)} title={`${mapName(match.mapId)} · ${dateShort(match.startMs)} · ${rrDelta(match.rrEarned)} RR`} aria-label={`Open ${mapName(match.mapId)} match, ${match.result}, ${rrDelta(match.rrEarned)} RR`}>{match.result}</button>
        {/each}
        {#if (data?.session?.matches.length ?? 0) > 5}<button class="session-toggle" title={showSession ? 'Show five recent session matches' : 'Show all session matches'} aria-expanded={showSession} onclick={() => showSession = !showSession}>{showSession ? 'Less' : `All ${data!.session!.matches.length} games`}</button>{/if}
      </div>
    </section>

    <section class="recent" aria-label="Recent matches">
      <div class="section-heading"><h2>Recent matches</h2><span>{data?.last5.length ? `${data.last5.length} games` : "—"}</span></div>
      <div class="match-columns" aria-hidden="true"><span>Map / agent</span><span>Result</span><span>K / D / A</span><span>K/D</span><span>ACS</span><span>HS%</span><span>RR</span></div>
      {#each data?.last5 ?? [] as match}
        <button class="match-row" onclick={() => onmatch(match.matchId)} aria-label={`Open ${mapName(match.mapId)}, ${match.result}, ${score(match.roundsWon, match.roundsLost)}`}>
          <span class="match-identity">
            {#if match.agentId}<img src={asset("agent", match.agentId)} alt="" />{/if}
            <span><strong>{mapName(match.mapId)}</strong><small>{agentName(match.agentId)} · {dateShort(match.startMs)}</small></span>
          </span>
          <span class="result"><strong class:positive={match.result === "win"} class:negative={match.result === "loss"}>{match.result === "win" ? "Win" : match.result === "loss" ? "Loss" : "Draw"}</strong><small>{score(match.roundsWon, match.roundsLost)}</small></span>
          <span>{number(match.kills)} / {number(match.deaths)} / {number(match.assists)}</span>
          <span>{kd2(match.kd)}</span><span>{number(match.acs)}</span><span>{pct0(match.hsPct)}</span>
          <strong class={tone(match.rrEarned)}>{rrDelta(match.rrEarned)}</strong>
        </button>
      {:else}<div class="empty-row">—</div>{/each}
    </section>

    {#if data?.rank.changes.length}
      <section class="rank-changes"><h2>Rank changes</h2><div>{#each data.rank.changes as change}<span><i class:positive={change.up} class:negative={!change.up}>{change.up ? "↑" : "↓"}</i> {rankName(change.tier)} <small>{dateShort(change.startMs)}</small></span>{/each}</div></section>
    {/if}

    <div class="breakdowns">
      {#each ["Maps", "Agents"] as label}
        <section class="breakdown"><h2>{label}</h2>
          <table><thead><tr><th>{label === "Maps" ? "Map" : "Agent"}</th><th>Games</th><th>W–L</th><th>Win%</th><th>RR</th><th>K/D</th><th>ACS</th></tr></thead><tbody>
            {#each (label === "Maps" ? data?.maps : data?.agents) ?? [] as row}
              <tr><th scope="row">{label === "Maps" ? mapName(row.id) : agentName(row.id)}</th><td>{row.games}</td><td>{row.wins}–{row.losses}</td><td>{pct0(row.winrate)}</td><td class={tone(row.netRr)}>{rrDelta(row.netRr)}</td><td>{kd2(row.kd)}</td><td>{number(row.acs)}</td></tr>
            {:else}<tr><td colspan="7">—</td></tr>{/each}
          </tbody></table>
        </section>
      {/each}
    </div>

    <div class="timing">
      <section><h2>By weekday</h2><div class="weekday-grid">
        {#each days as day, index}{@const cell = data?.byWeekday[index]}
          <div class="time-cell"><span>{day}</span><strong>{pct0(cell?.winrate)}</strong><div class="bar-track"><div style:width={`${Math.max(0, Math.min(100, (cell?.winrate ?? 0) * 100))}%`}></div></div><small>{cell ? `${cell.games} games · ${cell.wins} W` : "—"}</small></div>
        {/each}
      </div></section>
      <section><h2>By time of day</h2><div class="period-grid">
        {#each periods as period, index}{@const cell = data?.byDaypart[index]}
          <div class="time-cell"><span title={hours[index]}>{period}</span><strong>{pct0(cell?.winrate)}</strong><div class="bar-track"><div style:width={`${Math.max(0, Math.min(100, (cell?.winrate ?? 0) * 100))}%`}></div></div><small>{cell ? `${cell.games} games · ${cell.wins} W` : "—"}</small></div>
        {/each}
      </div></section>
    </div>

    <section class="insights"><h2>Patterns worth noticing</h2><div class="insight-list">
      {#each data?.insights ?? [] as insight}<p class:strength={insight.positive}><span>{insight.text}</span><small>{insight.matches} games</small></p>{:else}<p>—</p>{/each}
    </div></section>
  {/if}
</div>

<style>
  .stats { height:auto; min-height:100%; overflow-y:visible; padding:25px 28px 40px; box-sizing:border-box; background:#141719; color:#e4e7e8; font-size:14px; scrollbar-color:#414a4d #141719; }
  h1,h2,p { margin:0; } h1 { font-size:24px; font-weight:600; letter-spacing:-.6px; } h2 { font-size:15px; font-weight:600; }
  strong { font-weight:600; } small { font-size:12px; color:#a6afb3; font-weight:400; } i { font-style:normal; }
  button { font:inherit; color:inherit; cursor:pointer; } button:focus-visible { outline:2px solid #b5c6b1; outline-offset:3px; }
  .page-heading,.section-heading { display:flex; align-items:center; justify-content:space-between; gap:16px; }
  .page-heading { margin-bottom:22px; } .page-heading>span,.section-heading>span { color:#a6afb3; font-size:12px; }
  .overview { display:grid; grid-template-columns:248px minmax(0,1fr); gap:30px; padding-bottom:22px; }
  .rank-summary { border-right:1px solid #32393c; padding-right:25px; } .rank-heading { display:flex; gap:12px; align-items:center; min-height:64px; }
  .rank-icon { width:52px; height:52px; object-fit:contain; } .rank-heading h2 { font-size:18px; } .rating { font-size:28px; line-height:1.25; font-weight:600; font-variant-numeric:tabular-nums; } .rating>span { color:#a6afb3; font-size:14px; font-weight:400; }
  .rank-facts { display:grid; gap:9px; margin-top:17px; } .rank-facts>div { display:flex; justify-content:space-between; gap:12px; align-items:baseline; font-size:12px; } .rank-facts>div>span { color:#a6afb3; } .rank-facts strong { white-space:nowrap; font-variant-numeric:tabular-nums; }
  .progression { min-width:0; } .progression svg { display:block; width:100%; height:186px; margin-top:5px; overflow:visible; } svg text { fill:#a6afb3; font-size:12px; font-family:inherit; } .gridline { stroke:#30373a; stroke-width:1; } .trend { fill:none; stroke:#b5c6b1; stroke-width:2; stroke-linejoin:round; } .point { fill:#b5c6b1; } .empty-chart { height:186px; display:grid; place-items:center; color:#a6afb3; }
  .session { background:#1c2023; border-radius:6px; display:flex; align-items:center; gap:25px; padding:15px 18px; margin-bottom:26px; min-height:30px; }
  .session-summary { display:flex; align-items:baseline; gap:18px; flex-shrink:0; } .session-summary>strong { font-size:18px; font-variant-numeric:tabular-nums; } .session-summary>span { color:#a6afb3; font-size:12px; } .session-matches { display:flex; flex-wrap:wrap; gap:6px; margin-left:auto; }
  .session-matches>button { border:1px solid #45504d; background:transparent; border-radius:3px; width:28px; height:28px; padding:0; font-size:12px; }
  .positive { color:#b5c6b1; } .negative { color:#d3a8a3; }
  .match-columns,.match-row { display:grid; grid-template-columns:minmax(220px,1fr) 82px 118px 62px 58px 65px 58px; align-items:center; gap:12px; }
  .match-columns { color:#a6afb3; font-size:12px; padding:13px 12px 9px; border-bottom:1px solid #32393c; }
  .match-columns>span:not(:first-child),.match-row>span:not(:first-child),.match-row>strong { text-align:right; }
  .match-row { width:100%; border:0; border-bottom:1px solid #30373a; padding:11px 12px; min-height:65px; background:transparent; text-align:left; font-variant-numeric:tabular-nums; font-size:13px; }
  .match-row:hover { background:#1c2023; } .match-identity { display:flex; align-items:center; gap:12px; min-width:0; } .match-identity img { width:34px; height:34px; object-fit:contain; background:#202629; border-radius:4px; } .match-identity>span { display:grid; gap:3px; min-width:0; } .match-identity strong { overflow-wrap:anywhere; font-size:14px; } .result { display:grid; gap:3px; } .empty-row { padding:24px 12px; color:#a6afb3; border-bottom:1px solid #32393c; }
  .rank-changes { display:flex; align-items:baseline; gap:24px; margin-top:23px; } .rank-changes>div { display:flex; flex-wrap:wrap; gap:12px 22px; } .rank-changes span { font-size:12px; } .rank-changes small { margin-left:5px; }
  .breakdowns { display:grid; grid-template-columns:minmax(0,1fr) minmax(0,1fr); gap:30px; margin-top:32px; } .breakdown { min-width:0; } table { width:100%; border-collapse:collapse; font-size:12px; margin-top:11px; font-variant-numeric:tabular-nums; } th,td { padding:11px 4px; text-align:right; border-bottom:1px solid #30373a; } thead th { color:#a6afb3; font-weight:400; } th:first-child { text-align:left; padding-left:0; overflow-wrap:anywhere; } tbody th { font-weight:500; } td:last-child,th:last-child { padding-right:0; }
  .timing { display:grid; gap:28px; margin-top:32px; } .weekday-grid,.period-grid { display:grid; gap:18px; margin-top:16px; } .weekday-grid { grid-template-columns:repeat(7,minmax(0,1fr)); } .period-grid { grid-template-columns:repeat(4,minmax(0,1fr)); } .time-cell { display:grid; gap:8px; } .time-cell>span { color:#a6afb3; font-size:12px; } .time-cell>strong { font-size:18px; font-variant-numeric:tabular-nums; } .time-cell>small { font-size:12px; white-space:nowrap; } .bar-track { height:4px; background:#30373a; } .bar-track>div { height:100%; background:#8d9f89; }
  .insights { margin-top:32px; } .insight-list { display:grid; grid-template-columns:minmax(0,1fr) minmax(0,1fr); gap:0 30px; margin-top:10px; } .insight-list>p { padding:14px 0 14px 14px; border-bottom:1px solid #30373a; border-left:2px solid #66544f; display:grid; align-content:start; gap:8px; font-size:13px; line-height:1.55; } .insight-list>p.strength { border-left-color:#8d9f89; }
  .skeleton { display:grid; gap:12px; } .skeleton-heading { width:180px; height:28px; background:#252c2f; margin-bottom:12px; border-radius:3px; } .skeleton-overview { display:grid; grid-template-columns:248px 1fr; gap:30px; } .skeleton-overview>div { height:224px; background:#1c2023; border-radius:5px; } .skeleton-session { height:60px; background:#1c2023; border-radius:5px; margin-bottom:15px; } .skeleton-row { height:56px; background:#1c2023; border-radius:3px; }
.session-matches .session-toggle{width:auto;padding:0 10px;white-space:nowrap}</style>


