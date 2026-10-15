<script lang="ts">
  import { asset, getMeta, displayName, kd2, pct0, rrDelta, ago, dateShort, type Scene, type PlayerCard } from '../fresh/data';
  import { agentName, mapName, tierLabel } from '../lib/meta';
  let { scene, sceneName = 'ingame', onplayer }: { scene: Scene; sceneName?: string; onplayer: (id: string) => void } = $props();
  let metadata = $derived(getMeta(sceneName));
  let teams = $derived(scene.live?.stage === 'ingame' ? ['ally','enemy'] : ['ally']);
  const colors = ['#91a6c5','#bb9abc','#9baa85','#84b4b0'];
  const count = (p: PlayerCard) => !p.isSelf && !p.isSelfParty && p.encounters ? p.encounters.with + p.encounters.against : 0;
  const playerName = (p: PlayerCard) => p.name?.kind === 'hiddenAgent' ? agentName(metadata, p.name.agentId) : displayName(p.name).name;
  const nameTip = (p: PlayerCard) => {
    if (p.name?.kind === 'remembered') return `Hidden name · ${p.name.source === 'ownMatch' ? 'known from your match' : p.name.source === 'otherMatch' ? 'known from a past match' : 'seen before'}${p.name.fromMapId ? ' · ' + (mapName(metadata,p.name.fromMapId) ?? '—') : ''}${p.name.fromStartMs ? ' · ' + dateShort(p.name.fromStartMs) : ''}`;
    if (p.name?.kind === 'hiddenAgent' || p.name?.kind === 'hiddenPlayer') return 'Hidden name';
    const n = displayName(p.name); return `${n.name}${n.tag ? '#' + n.tag : ''}`;
  };
</script>

<main class="roster" aria-label="Live players">
  <div class="columns header"><span>Player</span><span>Rank</span><span>K/D</span><span>HS%</span><span>Win%</span><span>Recent</span></div>
  {#each teams as team}
    <section aria-label={team === 'enemy' ? 'Opponents' : scene.live?.stage === 'ingame' || scene.live?.stage === 'pregame' ? 'Your team' : 'Your party'}>
      <h1>{team === 'enemy' ? 'Opponents' : scene.live?.stage === 'ingame' || scene.live?.stage === 'pregame' ? 'Your team' : 'Your party'}</h1>
      {#if !scene.live}<div aria-label="Loading players" aria-busy="true">{#each Array.from({length:5}) as _}<div class="columns player placeholder" aria-hidden="true">{#each Array.from({length:6}) as _, i}<span class="skeleton" class:identity-skeleton={i === 0}></span>{/each}</div>{/each}</div>{/if}
      {#each scene.live?.players.filter(p => p.team === team) ?? [] as p}
        {@const n = displayName(p.name)}
        {@const label = playerName(p)}
        {@const rank = tierLabel(metadata,p.rank?.tier)}
        <button class="columns player" data-puuid={p.puuid} class:self={p.isSelf} onclick={() => onplayer(p.puuid)} aria-label={`Open ${label ?? 'player'}`}>
          <div class="identity-cell">
            <div class="agent">{#if p.agentId}<img src={asset('agent',p.agentId)} alt={agentName(metadata,p.agentId) ?? 'Locked agent'} width="34" height="34"/>{/if}</div>
            <div class="identity" title={nameTip(p)} class:hidden={n.hidden} class:remembered={n.remembered}>
              {#if n.hidden}<svg aria-label="Hidden name" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6"><path d="m3 3 18 18M10 5c5-1 9 4 11 7-1 2-2 3-4 5M6 6c-2 1-4 4-5 6 3 5 8 9 14 6M9 9a4 4 0 0 0 6 6"/></svg>{/if}
              {#if p.name === undefined || label === undefined}<span class="skeleton identity-skeleton" aria-label="Loading identity"></span>{:else}<strong class="name">{label ?? '—'}</strong>{#if n.tag}<span class="tag">#{n.tag}</span>{/if}{/if}
            </div>
            <span class="encounter" title={count(p) ? `${p.encounters?.with} with · ${p.encounters?.against} against · Competitive` : undefined}>{#if count(p)}⟲ {count(p)}{/if}</span>
          </div>
          <div class="rank">
            {#if p.rank === undefined}<span class="skeleton rank-skeleton" aria-label="Loading rank"></span>{:else if p.rank === null}<strong>—</strong>{:else}<img src={asset('tier',p.rank.tier)} alt="" width="30" height="30"/><div>{#if rank === undefined}<span class="skeleton rank-skeleton" aria-label="Loading rank name"></span>{:else}<strong title={rank ?? undefined}>{rank ?? '—'}</strong>{/if}<small>{p.rank.rr ?? '—'} RR</small></div>{/if}
          </div>
          <div class="metric" title="Last 5 competitive">{#if p.kd === undefined}<span class="skeleton"></span>{:else}{kd2(p.kd)}{/if}</div>
          <div class="metric" title="Last 5 competitive">{#if p.hsPct === undefined}<span class="skeleton"></span>{:else}{pct0(p.hsPct)}{/if}</div>
          <div class="metric" title={`This act · ${p.actWins ?? '—'} wins · ${p.actGames ?? '—'} games`}>{#if p.actWinrate === undefined}<span class="skeleton"></span>{:else}{pct0(p.actWinrate)}{/if}</div>
          <div class="results" title="Recent competitive results · oldest to newest">
            {#if p.last5 === undefined}<span class="skeleton results-skeleton" aria-label="Loading recent results"></span>{:else}{#each Array.from({length:5}) as _, i}{@const result = p.last5?.[4-i]}<span class="result" class:win={result?.result === 'win'} class:loss={result?.result === 'loss'} class:draw={result?.result === 'draw'} class:empty={!result} class:newest={i === 4 && !!result} title={result ? `${result.result} · ${rrDelta(result.rrEarned)} RR · ${ago(result.startMs)}` : undefined}>{result ? result.result[0].toUpperCase() : ''}</span>{/each}{/if}
          </div>
          {#if p.party && !p.isSelf && !p.isSelfParty}<span class="party" class:likely={!p.party.confirmed} style:--party-color={colors[(p.party.group-1+4)%4]} title={`Party ${p.party.group} · ${p.party.confirmed ? 'confirmed' : 'likely'}`}></span>{/if}
        </button>
      {/each}
    </section>
  {/each}
</main>

<style>
  .roster{height:736px;padding:24px;overflow:hidden;color:var(--text);font-size:14px;box-sizing:border-box}.columns{display:grid;grid-template-columns:minmax(0,1fr) 172px 70px 70px 76px 145px;column-gap:12px;align-items:center}.header{height:23px;padding:0 14px;color:var(--muted);font-size:12px}.header>span:nth-child(n+3):not(:last-child){text-align:center}.roster h1{font-size:14px;font-weight:600;margin:16px 14px 10px;line-height:20px}.player{position:relative;width:100%;height:54px;padding:0 14px;border:0;border-top:1px solid #30373b;background:var(--surface);color:inherit;font:inherit;font-variant-numeric:tabular-nums;text-align:left;cursor:pointer}.player:last-child{border-bottom:1px solid #30373b}.player:hover{background:#272d31}.player:focus-visible{outline:2px solid var(--accent);outline-offset:-2px}.player.self{outline:1px solid var(--accent);outline-offset:-1px;background:#242b29}.identity-cell{display:flex;align-items:center;gap:10px;min-width:0}.agent{width:34px;height:34px;flex:0 0 34px}.agent img{object-fit:contain}.identity{display:flex;align-items:center;gap:6px;min-width:0;flex:1}.identity svg{color:var(--muted);flex-shrink:0}.name{overflow:hidden;text-overflow:ellipsis;white-space:nowrap;font-size:14px;font-weight:600;min-width:0}.tag{font-size:12px;color:var(--muted);flex-shrink:0}.hidden:not(.remembered) .name{font-style:italic;font-weight:500}.remembered .name{text-decoration:underline dotted;text-underline-offset:4px}.encounter{width:47px;flex:0 0 47px;text-align:right;color:var(--muted);font-size:12px}.rank{display:flex;align-items:center;gap:9px;min-width:0}.rank>img{object-fit:contain;flex-shrink:0}.rank>div{min-width:0}.rank strong{display:block;font-weight:500;font-size:14px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.rank small{display:block;color:var(--muted);font-size:12px;margin-top:2px}.metric{text-align:center;font-size:16px}.results{display:flex;gap:6px;align-items:center;min-height:24px}.result{display:flex;align-items:center;justify-content:center;width:23px;height:24px;box-sizing:border-box;font-size:12px;border:1px solid transparent;color:var(--muted)}.result.win{background:#293c34;color:#b1d2bd}.result.loss{background:#423034;color:#dfb3b5}.result.draw{background:#363b40;color:#c3c8cb}.result.empty{border:1px dashed #485055}.result.newest{border-bottom:2px solid currentColor}.party{position:absolute;right:0;top:7px;bottom:7px;width:3px;background:var(--party-color)}.party.likely{background:repeating-linear-gradient(to bottom,var(--party-color) 0 5px,transparent 5px 9px)}.skeleton{display:inline-block;width:40px;height:12px;border-radius:3px;background:#343c40}.identity-skeleton{width:100px;max-width:100%}.rank-skeleton{width:85px}.results-skeleton{width:139px;height:23px}
.placeholder{cursor:default}.placeholder:hover{background:var(--surface)}</style>
