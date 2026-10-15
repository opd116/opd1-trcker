<script lang="ts">
  import { getScene, getDetails, displayName, rankName, mapName, agentName, asset, kd2, pct0, dateShort, rrDelta, type PlayerCard } from '../fresh/data';
  const sceneName = new URLSearchParams(location.search).get('scene') ?? 'ingame';
  const live = getScene(sceneName).live;
  const teams = live?.stage === 'ingame' ? ['ally', 'enemy'] : ['ally'];
  let selected = $state<PlayerCard | null>(null);
  let more = $state(false);
  const details = $derived(selected ? getDetails(sceneName, selected.puuid) : null);
  const results = (p: PlayerCard) => Array.from({length: 5}, (_, i) => [...(p.last5 ?? [])].reverse()[i - (5 - (p.last5?.length ?? 0))]);
  const title = (p: PlayerCard) => { const n = displayName(p.name); return `${n.name}${n.tag ? '#' + n.tag : ''}`; };
  const partyColors = ['#8d9fc5','#b797ba','#7fa7a6','#baa184'];
  const rememberedHint = (p: PlayerCard) => p.name?.kind === 'remembered' ? `Hidden name · ${p.name.source === 'ownMatch' ? 'known from your match' : p.name.source === 'otherMatch' ? 'known from a past match' : 'seen before'}${p.name.fromMapId ? ' · ' + mapName(p.name.fromMapId) : ''}${p.name.fromStartMs ? ' · ' + dateShort(p.name.fromStartMs) : ''}` : 'Hidden name';
</script>

<svelte:window onkeydown={(e) => {if(e.key === 'Escape') selected = null;}} />
<div class="window">
  <header class="chrome"><strong>OPD1 <span>TRCKER</span></strong><div class="chrome-end"><span class="minimize">—</span><span>×</span></div></header>
  <div class="masthead"><nav><button class:active={!selected} onclick={() => selected = null}>Live</button></nav><div class="context">{#if live?.mapId}<span class="map-name">{mapName(live.mapId)}</span><img src={asset('mapList',live.mapId)} alt=""/>{:else}<span class="map-name">Your party</span>{/if}</div></div>
  <main>
  {#if selected && details}
    <div class="detail-top"><button class="back" onclick={() => {selected = null; more = false;}}>← Back to roster</button></div>
    <section class="profile">
      <img class="card-art" src={asset('cardSmall', selected.cardId)} alt=""/>
      <div class="profile-name"><h1 title={displayName(selected.name).hidden ? rememberedHint(selected) : title(selected)}>{#if displayName(selected.name).remembered}<svg class="eye" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true"><path d="m3 3 18 18M10.6 10.6a2 2 0 0 0 2.8 2.8M4.7 7.7C3.5 8.8 2.6 10.3 2 12c2 5.3 6.7 8.2 11.6 6.6M9 5.4C14.2 3.8 19.7 6.6 22 12c-.6 1.5-1.4 2.7-2.4 3.7"/></svg>{/if}<span class="profile-identity" class:hidden={displayName(selected.name).hidden} class:remembered={displayName(selected.name).remembered}>{displayName(selected.name).name}</span><span class="profile-tag">{displayName(selected.name).tag ? '#' + displayName(selected.name).tag : ''}</span></h1><p>Level {selected.level ?? '—'}{selected.agentId ? ' · ' + agentName(selected.agentId) : ''}</p></div>
      <div class="profile-rank">{#if selected.rank}<img src={asset('tier',selected.rank.tier)} alt=""/>{/if}<div><strong>{rankName(selected.rank?.tier)}</strong><span>{selected.rank?.rr ?? '—'} RR</span></div></div>
    </section>
    <div class="hero-stats"><div title="This act"><strong>{pct0(selected.actWinrate)}</strong><span>Win rate</span></div><div title="This act"><strong>{selected.actGames ?? '—'}</strong><span>Games</span></div><div title="This act"><strong>{selected.actWins ?? '—'} <span class="quiet">/</span> {selected.actGames != null && selected.actWins != null ? selected.actGames - selected.actWins : '—'}</strong><span>Wins / losses</span></div><div title={`Last ${details.heroMatches} competitive · this act`}><strong>{kd2(details.heroKd)}</strong><span>K/D</span></div><div title={`Last ${details.heroMatches} competitive · this act`}><strong>{pct0(details.heroHs)}</strong><span>Headshots</span></div></div>
    <div class="detail-columns"><section><h2>Recent matches</h2><div class="recent-head"><span>Map</span><span>Score</span><span>K / D / A</span><span>RR</span></div>{#each details.recent.slice(0,more ? 8 : 3) as match}<div class="match-row"><div class="match-map"><img src={asset('mapList',match.mapId)} alt=""/><div><strong>{mapName(match.mapId)}</strong><small>{dateShort(match.startMs)}</small></div></div><span class:win={match.result === 'win'} class:loss={match.result === 'loss'}>{match.roundsWon ?? '—'} : {match.roundsLost ?? '—'}</span><span>{match.kills ?? '—'} / {match.deaths ?? '—'} / {match.assists ?? '—'}</span><span>{rrDelta(match.rrEarned)}</span></div>{/each}<button class="show-more" onclick={() => more = !more}>{more ? 'Show less' : 'Show more'}</button>
      <h2 class="teammates-heading">Frequent teammates</h2>{#each details.topTeammates as mate}<div class="teammate"><span>{displayName(mate.name).name}<small>{displayName(mate.name).tag ? '#' + displayName(mate.name).tag : ''}</small></span><span>{mate.count} matches</span></div>{/each}
    </section><aside><h2>Rank history</h2><div class="rank-history"><span>Previous act</span><strong>{rankName(selected.prevActTier)}</strong></div><div class="rank-history"><span>Peak</span><strong>{rankName(selected.peak?.tier)}</strong><small>{selected.peak?.actLabel ?? '—'}</small></div><h2 class="encounters-heading">Encounters</h2>{#if selected.encounters}<p class="encounter-summary">{selected.encounters.with} with you <span>·</span> {selected.encounters.against} against</p>{:else}<p class="encounter-summary">—</p>{/if}<h2 class="loadout-heading">Loadout</h2>{#each Object.entries(details.loadout) as [weapon,skin]}<div class="weapon"><span>{weapon}</span><strong>{skin?.skinName ?? '—'}</strong></div>{/each}</aside></div>
  {:else}
    <div class="table-head row-grid"><span>Player</span><span>Rank</span><span title="Last 5 competitive">K/D</span><span title="This act">Win rate</span><span title="Last 5 competitive · oldest to newest">Recent</span></div>
    {#each teams as team}
      <section class="roster"><h2>{live?.stage === 'menus' || live?.stage === 'closed' ? 'Your party' : team === 'ally' ? 'Your team' : 'Opponents'}</h2>
      {#each live?.players.filter(p => p.team === team) ?? [] as p}
        {@const name = displayName(p.name)}
        <button class="player row-grid" class:self={p.isSelf} aria-label={`Open ${title(p)}`} onclick={() => selected = p}>
          <div class="identity">{#if p.agentId}<img class="agent" src={asset('agent',p.agentId)} alt={agentName(p.agentId)}/>{:else}<span class="empty-agent"></span>{/if}<div class="name-group"><div class="name-line" title={name.hidden ? rememberedHint(p) : title(p)}><strong class:hidden={name.hidden} class:remembered={name.remembered} class:skeleton={p.name === undefined}>{#if name.remembered}<svg class="eye" width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true"><path d="m3 3 18 18M10.6 10.6a2 2 0 0 0 2.8 2.8M4.7 7.7C3.5 8.8 2.6 10.3 2 12c2 5.3 6.7 8.2 11.6 6.6M9 5.4C14.2 3.8 19.7 6.6 22 12c-.6 1.5-1.4 2.7-2.4 3.7"/></svg>{/if}{name.name}</strong>{#if name.tag}<span class="tag">#{name.tag}</span>{/if}</div></div><span class="encounter" title={`${p.encounters?.with ?? 0} with · ${p.encounters?.against ?? 0} against`}>{!p.isSelf && !p.isSelfParty && p.encounters && p.encounters.with + p.encounters.against > 0 ? '⟲ ' + (p.encounters.with + p.encounters.against) : ''}</span></div>
          <div class="rank">{#if p.rank}<img src={asset('tier',p.rank.tier)} alt=""/>{/if}<div><strong class:skeleton={p.rank === undefined}>{rankName(p.rank?.tier)}</strong><small class:skeleton={p.rank === undefined}>{p.rank?.rr ?? '—'} RR</small></div></div>
          <span class="metric" class:skeleton={p.kd === undefined} title="Last 5 competitive">{kd2(p.kd)}</span><span class="metric" class:skeleton={p.actWinrate === undefined} title={`This act · ${p.actWins ?? '—'} wins · ${p.actGames ?? '—'} games`}>{pct0(p.actWinrate)}</span>
          <div class="results">{#each results(p) as result, i}<span class="result" class:win={result?.result === 'win'} class:loss={result?.result === 'loss'} class:draw={result?.result === 'draw'} class:newest={i === 4} class:empty={!result} title={result ? `${result.result} · ${rrDelta(result.rrEarned)}` : ''}>{result ? result.result[0].toUpperCase() : ''}</span>{/each}</div>
          {#if p.party && !p.isSelf && !p.isSelfParty}<span class="party" class:likely={!p.party.confirmed} style:background={partyColors[p.party.group % 4]} title={`Party · ${p.party.confirmed ? 'confirmed' : 'likely'}`}></span>{/if}
        </button>
      {/each}</section>
    {/each}
  {/if}
  </main>
</div>
