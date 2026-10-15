<script lang="ts">
  import { asset, rankName, kd2, pct0, rr, ago, rrDelta, type Scene, type PlayerCard } from './data';
  import Identity from './Identity.svelte';
  let { scene, onplayer }: { scene: Scene; onplayer: (id: string) => void } = $props();
  let teams = $derived([scene.live?.players.filter(p => p.team === 'ally') ?? [], scene.live?.players.filter(p => p.team === 'enemy') ?? []]);
  const partyColors = ['#b4bdcc','#c6afc5','#bdbe96','#a2c3bd'];
  const encounter = (p: PlayerCard) => p.encounters ? p.encounters.with + p.encounters.against : 0;
</script>
<main class="roster" aria-label="Live players">
  {#each teams as players, team}
    <section aria-label={team === 0 ? 'Your team' : 'Opponents'}>
      <h1>{team === 0 ? 'Your team' : 'Opponents'}</h1>
      <div class="players">
        {#each Array.from({length:5}) as _, i}
          {@const p = players[i]}
          {#if p}
            <button class="player" data-puuid={p.puuid} class:self={p.isSelf} onclick={() => onplayer(p.puuid)} aria-label={`Open player ${i + 1} on ${team === 0 ? 'your team' : 'opponents'}`}>
              <div class="recognition">
                {#if p.agentId}<img class="card-art" src={asset('agent',p.agentId)} alt="Locked agent" width="40" height="40"/>{:else}<img class="card-art" src={asset('cardSmall', p.cardId)} alt="" width="40" height="40"/>{/if}
              </div>
              <div class="player-body">
                <div class="topline">
                  <div class="name-block"><Identity value={p.name}/><span class="encounter" title={encounter(p) ? `${p.encounters?.with} with · ${p.encounters?.against} against · Competitive` : undefined}>{#if encounter(p)}⟲ {encounter(p)}{/if}</span></div>
                  <div class="rank" class:loading={p.rank === undefined}>
                    {#if p.rank}<img src={asset('tier',p.rank.tier)} alt="" width="26" height="26"/><div><strong>{rankName(p.rank.tier)}</strong><span>{rr(p.rank.rr)} RR</span></div>{:else if p.rank === null}<strong>—</strong>{:else}<span class="skeleton"></span>{/if}
                  </div>
                </div>
                <div class="form">
                  <div title="Last 5 competitive" class:loading={p.kd === undefined}><span>K/D</span><strong>{kd2(p.kd)}</strong></div>
                  <div title="Last 5 competitive" class:loading={p.hsPct === undefined}><span>HS%</span><strong>{pct0(p.hsPct)}</strong></div>
                  <div title={`This act · ${p.actWins ?? '—'}W · ${(p.actGames ?? 0) - (p.actWins ?? 0)}L · ${p.actGames ?? '—'} games`} class:loading={p.actWinrate === undefined}><span>Win%</span><strong>{pct0(p.actWinrate)}</strong></div>
                  <div class="recent" title="Recent competitive results · oldest to newest" class:loading={p.last5 === undefined}><span>Recent</span><div class="results">{#each Array.from({length:5}) as _, j}{@const result = p.last5?.[4-j]}<span class="result" class:empty-result={!result} class:newest={j === 4 && !!result} class:win={result?.result === 'win'} class:loss={result?.result === 'loss'} title={result ? `${result.result} · ${rrDelta(result.rrEarned)} RR · ${ago(result.startMs)}` : undefined}>{result ? result.result === 'win' ? 'W' : result.result === 'loss' ? 'L' : 'D' : ''}</span>{/each}</div></div>
                </div>
              </div>
              {#if p.party && !p.isSelfParty}<span class="party" class:likely={!p.party.confirmed} style:--party-color={partyColors[(p.party.group-1+4)%4]} title={`Party ${p.party.group} · ${p.party.confirmed ? 'confirmed' : 'likely'}`}></span>{/if}
            </button>
          {:else}<div class="empty" class:pending={!scene.live || scene.live.players.length === 0} aria-hidden="true">{#if !scene.live || scene.live.players.length === 0}<div class="skeleton"></div>{/if}</div>{/if}
        {/each}
      </div>
    </section>
  {/each}
</main>
<style>
  .roster{padding:30px 24px 24px;display:grid;grid-template-columns:1fr 1fr;gap:24px}h1{font-size:14px;font-weight:500;margin:0 0 16px;color:var(--muted)}.players{display:grid;gap:12px}.player,.empty{height:112px;border:1px solid transparent;border-radius:5px;background:var(--surface);position:relative}.player{padding:16px 14px;display:flex;gap:12px;width:100%;text-align:left;cursor:pointer;color:var(--text);font:inherit}.player:hover{background:#23282c;border-color:#41494e}.player.self{border-color:#71806f}.recognition{width:40px;flex-shrink:0;align-self:flex-start;position:relative;margin-top:2px}.card-art{object-fit:cover;border-radius:3px}.player-body{flex:1;min-width:0}.topline{display:flex;align-items:center;justify-content:space-between;gap:12px;height:36px}.name-block{min-width:0;flex:1;font-size:15px;display:flex;align-items:center;gap:6px}.encounter{font-size:12px;color:var(--muted);white-space:nowrap}.rank{display:flex;gap:6px;align-items:center;flex-shrink:0}.rank div{display:flex;flex-direction:column;gap:2px}.rank strong{font-size:12px;font-weight:500}.rank span{font-size:12px;color:var(--muted)}.form{display:grid;grid-template-columns:1fr 1fr 1fr 1.7fr;gap:10px;margin-top:12px}.form>div>span{font-size:12px;color:var(--muted);display:block;margin-bottom:3px}.form strong{font-size:17px;font-weight:500;line-height:20px}.results{display:flex;justify-content:space-between;gap:4px;line-height:20px}.result{font-size:13px;color:var(--muted);width:14px;text-align:center}.result.win{color:#a6c7b4}.result.loss{color:#d19e9e}.result.newest{text-decoration:underline;text-underline-offset:5px;text-decoration-thickness:2px}.party{position:absolute;right:0;top:16px;bottom:16px;border-right:3px solid var(--party-color);border-radius:3px}.party.likely{border-right-style:dashed}.empty{background:transparent;border-color:#292f33;border-style:dashed;opacity:.5}.pending{background:var(--surface);border:0}.skeleton{height:12px;width:72px;background:#30373b;border-radius:3px}.pending .skeleton{margin:22px}.loading strong,.loading .results{visibility:hidden}.loading{background:linear-gradient(#30373b,#30373b) left bottom/42px 14px no-repeat}

  .roster{grid-template-columns:minmax(0,1fr) minmax(0,1fr)}.roster section,.player{min-width:0}.topline{height:32px}.form{margin-top:8px}.rank strong{font-size:14px;font-weight:600}.encounter{width:42px;flex-shrink:0;text-align:right}.name-block :global(.identity){flex:1}.recent>span{visibility:hidden}.results{gap:3px}.result{font-size:14px;width:18px;border-radius:2px;background:#282e32}.result.win{background:#25372e}.result.loss{background:#3a2b2d}.result.newest{font-weight:600;text-decoration-thickness:2px}.empty-result{background:transparent;border-bottom:1px solid #3b454a}
</style>


