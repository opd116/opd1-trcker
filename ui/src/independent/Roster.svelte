<script lang="ts">
  import { asset, getMeta, displayName, kd2, pct0, rrDelta, ago, dateShort, type Scene, type PlayerCard } from '../fresh/data';
  import { agentName, mapName, tierLabel } from '../lib/meta';
  let { scene, sceneName = 'ingame', onplayer }: { scene: Scene; sceneName?: string; onplayer: (id: string) => void } = $props();
  let metadata = $derived(getMeta(sceneName));
  let teams = $derived(scene.live?.stage === 'ingame' ? ['ally','enemy'] : ['ally']);
  const colors = ['#b7a1e0','#dfadc9','#b5c28d','#a6c9dd'];
  const count = (p: PlayerCard) => !p.isSelf && !p.isSelfParty && p.encounters ? p.encounters.with + p.encounters.against : 0;
  const playerName = (p: PlayerCard) => p.name?.kind === 'hiddenAgent' ? agentName(metadata, p.name.agentId) : displayName(p.name).name;
  const nameTip = (p: PlayerCard) => {
    if (p.name?.kind === 'remembered') return `Hidden name · ${p.name.source === 'ownMatch' ? 'known from your match' : p.name.source === 'otherMatch' ? 'known from a past match' : 'seen before'}${p.name.fromMapId ? ' · ' + (mapName(metadata,p.name.fromMapId) ?? '—') : ''}${p.name.fromStartMs ? ' · ' + dateShort(p.name.fromStartMs) : ''}`;
    if (p.name?.kind === 'hiddenAgent' || p.name?.kind === 'hiddenPlayer') return 'Hidden name';
    const n = displayName(p.name); return `${n.name}${n.tag ? '#' + n.tag : ''}`;
  };
  const peakTip = (p: PlayerCard) => { if (!p.peak) return '—'; const name = tierLabel(metadata,p.peak.tier) ?? '—'; const codes:Record<string,string> = {Iron:'IR',Bronze:'BR',Silver:'SV',Gold:'GD',Platinum:'PL',Diamond:'DM',Ascendant:'AS',Immortal:'IM',Radiant:'RD'}; return name.replace(/[A-Za-z]+/, w => codes[w] ?? w) + ' · ' + p.peak.actLabel + ' · ' + Math.floor(Math.max(0, p.peak.sinceActEndMs ?? 0) / 2592000000) + 'mo'; };
</script>

<main class="roster" aria-label="Live players">
  <div class="columns header"><span>Player</span><span>Rank</span><span>K/D</span><span>HS%</span><span>Win%</span><span>Recent</span></div>
  {#each teams as team}
    <section aria-label={team === 'enemy' ? 'Enemy' : 'Team'}>
      <h1>{team === 'enemy' ? 'Enemy' : 'Team'}</h1>
      {#if !scene.live}<div aria-label="Loading players" aria-busy="true">{#each Array.from({length:5}) as _}<div class="columns player placeholder" aria-hidden="true">{#each Array.from({length:6}) as _, i}<span class="skeleton" class:identity-skeleton={i === 0}></span>{/each}</div>{/each}</div>{/if}
      {#each scene.live?.players.filter(p => p.team === team) ?? [] as p}
        {@const n = displayName(p.name)}
        {@const label = playerName(p)}
        {@const rank = tierLabel(metadata,p.rank?.tier)}
        {@const party = !p.isSelf && !p.isSelfParty ? p.party : null}
        {@const encounters = count(p)}
        <button class="columns player" data-puuid={p.puuid} class:self={p.isSelf} onclick={() => onplayer(p.puuid)} aria-label={`Open ${label ?? 'player'}${n.tag ? '#' + n.tag : ''}`} aria-describedby={p.peak !== undefined ? `peak-${p.puuid}` : undefined}>
          <div class="identity-cell">
            <div class="agent">{#if p.agentId}<img src={asset('agent',p.agentId)} alt={agentName(metadata,p.agentId) ?? 'Locked agent'} width="34" height="34"/>{/if}</div>
            <div class="identity" title={nameTip(p)} class:hidden={n.hidden} class:remembered={n.remembered}>
              {#if n.hidden}<svg aria-label="Hidden name" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6"><path d="m3 3 18 18M10 5c5-1 9 4 11 7-1 2-2 3-4 5M6 6c-2 1-4 4-5 6 3 5 8 9 14 6M9 9a4 4 0 0 0 6 6"/></svg>{/if}
              {#if p.name === undefined || label === undefined}<span class="skeleton identity-skeleton" aria-label="Loading identity"></span>{:else}<strong class="name">{label ?? '—'}</strong>{#if n.tag}<span class="tag">#{n.tag}</span>{/if}{/if}
            </div>
            <div class="relationships">
              <span class="party-slot">
                {#if party}
                  <span class="party" class:likely={!party.confirmed} style:--party-color={colors[party.group % colors.length]} title={`Party ${party.group + 1} · ${party.confirmed ? 'confirmed' : 'likely'}`} role="img" aria-label={`Party ${party.group + 1} · ${party.confirmed ? 'confirmed' : 'likely'}`}>
                    <span aria-hidden="true">{["●", "◆", "▲", "■"][party.group % 4]}</span>
                  </span>
                {/if}
              </span>
              <span class="encounter-slot">
                {#if encounters}
                  <span class="encounter" title={`${p.encounters?.with} with · ${p.encounters?.against} against · Competitive`} role="img" aria-label={`${encounters} past competitive matches · ${p.encounters?.with} with · ${p.encounters?.against} against`}>
                    <svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M3 11a9 9 0 1 1 2.6 7M3 5v6h6m3-4v5l3 2"/></svg>
                    <span class="encounter-count" aria-hidden="true">{encounters}</span>
                  </span>
                {/if}
              </span>
            </div>
          </div>
          <div class="rank">
            <span class="rank-emblem">
              {#if p.rank === undefined}
                <span class="skeleton emblem-skeleton" aria-label="Loading rank"></span>
              {:else if p.rank === null}
                <span class="missing-rank">—</span>
              {:else}
                <img class="current-rank" src={asset('tier', p.rank.tier)} alt="" width="32" height="32"/>
              {/if}
              {#if p.peak === undefined}
                <span class="peak-badge peak-loading" aria-label="Loading peak rank"></span>
              {:else}
                <span class="peak-badge" role="img" aria-label={peakTip(p)}>
                  {#if p.peak}<img src={asset('tier', p.peak.tier)} alt="" width="15" height="15"/>{:else}<span>—</span>{/if}
                </span>
                <span class="peak-tooltip" id={`peak-${p.puuid}`} role="tooltip">
                  <span>{peakTip(p)}</span>
                </span>
              {/if}
            </span>
            <div class="rank-copy">
              {#if p.rank === undefined}
                <span class="skeleton rank-skeleton" aria-label="Loading rank name"></span>
              {:else if p.rank === null}
                <strong>—</strong>
              {:else}
                {#if rank === undefined}<span class="skeleton rank-skeleton" aria-label="Loading rank name"></span>{:else}<strong title={rank ?? undefined}>{rank ?? '—'}</strong>{/if}
                <small>{p.rank.rr ?? '—'} RR</small>
              {/if}
            </div>
          </div>
          <div class="metric" title="Last 5 competitive">{#if p.kd === undefined}<span class="skeleton"></span>{:else}{kd2(p.kd)}{/if}</div>
          <div class="metric" title="Last 5 competitive">{#if p.hsPct === undefined}<span class="skeleton"></span>{:else}{pct0(p.hsPct)}{/if}</div>
          <div class="metric" title={`This act · ${p.actWins ?? '—'} wins · ${p.actGames ?? '—'} games`}>{#if p.actWinrate === undefined}<span class="skeleton"></span>{:else}{pct0(p.actWinrate)}{/if}</div>
          <div class="results" title="Recent competitive results · oldest to newest">
            {#if p.last5 === undefined}
              <span class="skeleton results-skeleton" aria-label="Loading recent results"></span>
            {:else}
              {#each Array.from({ length:5 }) as _, i}
                {@const result = p.last5?.[4 - i]}
                {@const resultLabel = result ? `${result.result === 'win' ? 'Win' : result.result === 'loss' ? 'Loss' : 'Draw'} · ${rrDelta(result.rrEarned)} RR · ${ago(result.startMs)}${i === 4 ? ' · Most recent' : ''}` : 'No match'}
                <span class="result" class:win={result?.result === 'win'} class:loss={result?.result === 'loss'} class:draw={result?.result === 'draw'} class:empty={!result} class:newest={i === 4 && !!result} title={result ? resultLabel : undefined} role="img" aria-label={resultLabel}>
                  {#if result}<span aria-hidden="true">{result.result[0].toUpperCase()}</span>{/if}
                </span>
              {/each}
            {/if}
          </div>
        </button>
      {/each}
    </section>
  {/each}
</main>

<style>
  .roster { height:736px; padding:24px; overflow:hidden; color:var(--text); font-size:14px; box-sizing:border-box; }
  .columns { display:grid; grid-template-columns:minmax(0,1fr) 172px 70px 70px 76px 145px; column-gap:12px; align-items:center; }
  .header { height:23px; padding:0 14px; color:var(--muted); font-size:12px; }
  .header>span:nth-child(n+3):not(:last-child) { text-align:center; }
  .roster h1 { font-size:14px; font-weight:600; margin:16px 14px 10px; line-height:20px; }
  .player { position:relative; width:100%; height:54px; padding:0 14px; border:0; border-top:1px solid var(--border, #334258); background:var(--surface); color:inherit; font:inherit; font-variant-numeric:tabular-nums; text-align:left; cursor:pointer; }
  .player:last-child { border-bottom:1px solid var(--border, #334258); }
  .player:hover { background:#282828; }
  .player:focus-visible { outline:2px solid var(--accent); outline-offset:-2px; }
  .player.self { outline:1px solid var(--accent); outline-offset:-1px; background:#242424; }
  .identity-cell { display:flex; align-items:center; gap:10px; min-width:0; }
  .agent { width:34px; height:34px; flex:0 0 34px; }
  .agent img { object-fit:contain; }
  .identity { display:flex; align-items:center; gap:6px; min-width:0; flex:1; }
  .identity svg { color:var(--muted); flex-shrink:0; }
  .name { overflow:hidden; text-overflow:ellipsis; white-space:nowrap; font-size:14px; font-weight:600; min-width:0; }
  .tag { font-size:12px; color:var(--muted); flex-shrink:0; }
  .hidden:not(.remembered) .name { font-style:italic; font-weight:500; }
  .remembered .name { text-decoration:underline dotted; text-underline-offset:4px; }
  .relationships { display:flex; gap:6px; flex-shrink:0; align-items:center; }
  .party-slot { width:36px; flex:0 0 36px; }
  .party { display:flex; align-items:center; justify-content:center; gap:4px; height:25px; border:1px solid var(--party-color); border-radius:4px; background:color-mix(in srgb, var(--party-color) 22%, #192231); color:var(--party-color); font-size:12px; font-weight:600; }
  .party.likely { background:transparent; border-style:dashed; }
  .encounter-slot { display:flex; justify-content:flex-end; min-width:46px; flex:0 0 auto; }
  .encounter { display:flex; align-items:center; gap:3px; color:#cccccc; white-space:nowrap; }
  .encounter svg { flex:none; }
  .encounter-count { display:flex; align-items:center; justify-content:center; min-width:20px; height:21px; padding:0 4px; border:1px solid #505050; border-radius:4px; background:#272727; font-size:12px; font-weight:600; }
  .rank { display:flex; align-items:center; gap:10px; min-width:0; }
  .rank-emblem { display:flex; align-items:center; justify-content:center; position:relative; width:40px; height:40px; flex:0 0 40px; }
  .current-rank { object-fit:contain; }
  .missing-rank { color:var(--muted); font-size:16px; }
  .peak-badge { position:absolute; display:flex; align-items:center; justify-content:center; right:-1px; bottom:-1px; width:23px; height:23px; border:1px solid #75879f; border-radius:50%; background:#111b2a; box-shadow:0 0 0 2px var(--surface); color:var(--muted); font-size:12px; }
  .peak-badge img { object-fit:contain; }
  .peak-badge.peak-loading { border-color:#3b4d65; background:#383838; }
  .peak-tooltip { display:flex; flex-direction:column; gap:3px; position:absolute; z-index:4; left:-8px; top:calc(100% + 7px); min-width:230px; width:max-content; max-width:300px; padding:11px 13px; border:1px solid #506580; border-radius:6px; background:#0c1421; box-shadow:0 7px 22px #050a1580; color:var(--text); font-size:12px; line-height:1.4; white-space:normal; visibility:hidden; opacity:0; pointer-events:none; }
  .peak-tooltip::before { content:''; position:absolute; left:0; right:0; height:9px; bottom:100%; }
  .player:nth-last-child(-n+2) .peak-tooltip { top:auto; bottom:calc(100% + 7px); }
  .player:nth-last-child(-n+2) .peak-tooltip::before { bottom:auto; top:100%; }
  .peak-badge:hover + .peak-tooltip, .peak-tooltip:hover, .player:focus-visible .peak-tooltip { visibility:visible; opacity:1; pointer-events:auto; }
  .rank-copy { min-width:0; }
  .rank strong { display:block; font-weight:500; font-size:14px; white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }
  .rank small { display:block; color:var(--muted); font-size:12px; margin-top:2px; }
  .metric { text-align:center; font-size:16px; }
  .results { display:flex; gap:5px; align-items:center; min-height:26px; }
  .result { position:relative; display:flex; flex:none; align-items:center; justify-content:center; width:25px; height:25px; box-sizing:border-box; font-size:12px; font-weight:600; border:1px solid transparent; color:var(--muted); }
  .result.win { background:#24554f; border-color:#518b7f; color:#c0f0e4; border-radius:50%; }
  .result.loss { background:#57383d; border-color:#9c6268; color:#ffd0c8; border-radius:3px; }
  .result.draw { background:#51472d; border-color:#ad9355; color:#f5d58c; border-radius:7px 1px; }
  .result.empty { border:1px dashed #51617a; border-radius:3px; }
  .result.newest::after { content:''; position:absolute; bottom:-6px; left:6px; right:6px; height:2px; border-radius:1px; background:currentColor; }
  .skeleton { display:inline-block; width:40px; height:12px; border-radius:3px; background:#383838; }
  .identity-skeleton { width:100px; max-width:100%; }
  .emblem-skeleton { width:30px; height:30px; }
  .rank-skeleton { width:85px; }
  .results-skeleton { width:145px; height:25px; }
  .placeholder { cursor:default; }
  .placeholder:hover { background:var(--surface); }

.party{border:0;border-radius:0;background:transparent;font-size:18px;height:24px}.party.likely{opacity:.7;border-bottom:1px dashed currentColor}.party-slot{width:22px;flex-basis:22px}.peak-badge{width:17px;height:17px;border:0;border-radius:0;background:transparent;box-shadow:none;right:0;bottom:0}.peak-tooltip{min-width:0;max-width:none;padding:7px 10px;background:#202020;border-color:#555;box-shadow:none;white-space:nowrap}.result{width:20px;height:21px;border-radius:0!important;background:transparent!important;border:0;border-bottom:2px solid currentColor;font-weight:500}.result.win{color:#bad3c1}.result.loss{color:#d6b0b0}.result.draw{color:#d3c7a7}.result.empty{border-bottom:1px dashed #666}.result.newest::after{width:3px;height:3px;left:9px;right:auto;bottom:-6px;border-radius:50%}.results{gap:7px}
</style>
