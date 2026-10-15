<script lang="ts">
  import { asset, displayName, mapName, agentName, rankName, dateShort, durationMin, queueLabel, halvesText, pct0, score, pmText, meta, type MatchDetailsView, type DisplayName } from "./data";
  import { nameTip, ACS_TIP, ADR_TIP, PM_TIP, HALVES_TIP } from "../lib/tips";
  let { data, onback }: { data: MatchDetailsView | null; onback: () => void } = $props();
  const teams = $derived(data && data.teams.length > 1 ? data.teams : null);
  const focus = $derived(data?.teams.flatMap(t => t.players).find(p => p.isFocus));
  const halves = $derived(data ? halvesText(data) : null);
  const number = (v: number | null) => v === null ? "—" : String(Math.round(v));
</script>

{#snippet identity(value: DisplayName)}
  {@const n = displayName(value)}
  <span class="identity" title={nameTip(value, meta)}>
    {#if n.hidden}<svg class="eye" width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" aria-label="Hidden name"><path d="m2 2 12 12M6.5 3.7C11.3 2.8 14 8 14 8s-.8 1.5-2.2 2.8M9.4 12.2C4.8 13.1 2 8 2 8s.7-1.4 2-2.6"/><path d="M6.6 6.6a2 2 0 0 0 2.8 2.8"/></svg>{/if}
    <span class="name" class:remembered={n.remembered} class:hidden={n.hidden && !n.remembered}>{n.name}</span>{#if n.tag}<span class="tag">#{n.tag}</span>{/if}
  </span>
{/snippet}

<div class="fresh-scoreboard" aria-busy={data === null}>
  <button class="back" onclick={onback}><span aria-hidden="true">‹</span> Back</button>
  {#if data}
    <section class="match-overview" aria-label="Match overview">
      {#if data.mapId}<img class="map-art" src={asset("mapList", data.mapId)} alt="" />{/if}
      <div class="match-copy"><h1>{mapName(data.mapId)}</h1><p>{data.teams.length ? `${queueLabel(data.queueId)} · ${dateShort(data.startMs)}` : "—"}{#if data.durationMs !== null} · {durationMin(data.durationMs)}{/if}</p></div>
      <div class="score"><div class="rounds">{#if teams}<span class:win={teams[0]?.result === "win"} class:loss={teams[0]?.result === "loss"}>{number(teams[0]!.roundsWon)}</span><span class="dash">–</span><span>{number(teams[1]!.roundsWon)}</span>{:else}—{/if}</div>{#if halves}<span class="halves" title={HALVES_TIP}>{halves}</span>{/if}</div>
    </section>
    <section class="board" aria-label="Scoreboard">
      <div class="column-head row-grid"><span>Player</span><span title={ACS_TIP}>ACS</span><span>K / D / A</span><span title={PM_TIP}>+/−</span><span title={ADR_TIP}>ADR</span><span>HS%</span></div>
      {#each data.teams as team, i (`${i}:${team.teamId}`)}
        <div class="team-head"><h2>{!teams ? "Players" : i > 0 ? "Opponents" : data.perspective === "self" ? "Your team" : `${displayName(focus?.name).name}'s team`}</h2>{#if teams}<span class:win={team.result === "win"} class:loss={team.result === "loss"}>{team.result === "win" ? "Won" : team.result === "loss" ? "Lost" : "Draw"} <span class="team-rounds">{number(team.roundsWon)}</span></span>{/if}</div>
        {#each team.players as player (player.puuid)}
          {@const pm = pmText(player.kills, player.deaths)}
          <div class="player-row row-grid" class:self={player.isSelf} class:focus={player.isFocus && !player.isSelf}>
            <div class="player">{#if player.agentId}<img class="agent" src={asset("agent", player.agentId)} alt={agentName(player.agentId)} />{:else}<span class="agent"></span>{/if}{#if player.tier != null && player.tier >= 3}<img class="rank" src={asset("tier", player.tier)} alt={rankName(player.tier)} title={rankName(player.tier)} />{:else}<span class="rank"></span>{/if}{@render identity(player.name)}</div>
            <strong>{number(player.acs)}</strong><span class="kda">{number(player.kills)} <em>/</em> {number(player.deaths)} <em>/</em> {number(player.assists)}</span><span class:win={pm.tone === "win"} class:loss={pm.tone === "loss"}>{pm.text}</span><span>{number(player.adr)}</span><span>{pct0(player.hsPct)}</span>
          </div>
        {/each}
      {/each}
    </section>
  {:else}
    <div class="skeleton loading-header" aria-hidden="true"></div><div class="skeleton loading-board" aria-hidden="true"></div>
  {/if}
</div>

<style>
  .fresh-scoreboard{flex:1;min-height:0;overflow-y:auto;padding:20px 24px 28px;box-sizing:border-box;color:#e4e7e8;font-size:13px;font-variant-numeric:tabular-nums}.back{display:flex;align-items:center;gap:8px;height:30px;margin-bottom:18px;padding:0 8px;color:#a6afb3;font-size:13px;border:none;background:transparent;cursor:pointer}.back>span{font-size:22px}.back:hover{color:#e4e7e8}
  .match-overview{display:flex;align-items:center;gap:18px;min-height:80px;margin-bottom:12px}.map-art{width:132px;height:72px;object-fit:cover;border-radius:6px}.match-copy{flex:1;min-width:0}h1{font-size:25px;line-height:1.3;font-weight:500;margin:0 0 10px}.match-copy p{margin:0;color:#a6afb3;font-size:12px}.score{display:flex;flex-direction:column;align-items:flex-end;gap:7px;padding-right:4px}.rounds{display:flex;align-items:center;gap:16px;font-size:32px;line-height:36px;font-weight:500}.dash{color:#657178;font-size:22px}.halves{font-size:12px;color:#a6afb3}.win{color:#a8c3ac}.loss{color:#d3a09d}
  .board{border:1px solid #2c3236;border-radius:8px;overflow:hidden;background:#1c2023}.row-grid{display:grid;grid-template-columns:minmax(0,1fr) 60px 114px 55px 60px 55px;align-items:center;gap:20px;padding:0 20px}.column-head{height:38px;color:#a6afb3;font-size:12px}.row-grid>*:nth-child(n+2){text-align:right}.row-grid>*:nth-child(3){text-align:center}.team-head{display:flex;align-items:center;justify-content:space-between;gap:16px;height:35px;padding:0 20px;border-top:1px solid #2c3236;border-bottom:1px solid #2c3236;background:#20262a}.team-head h2{font-size:12px;font-weight:500;line-height:18px;margin:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;color:#bfc7cb}.team-head>span{flex:none;font-size:12px}.team-rounds{margin-left:10px;color:#e4e7e8;font-size:14px}.player-row{position:relative;min-height:43px;box-sizing:border-box}.player-row+.player-row{border-top:1px solid #2c3236}.player-row.self{outline:1px solid #82967e;outline-offset:-1px}.player-row.focus{border-left:3px solid #b5c6b1;padding-left:17px}.player{min-width:0;display:flex;align-items:center;gap:10px}.agent{width:27px;height:27px;flex:none;border-radius:4px}.rank{width:22px;height:22px;flex:none}.identity{display:flex;align-items:baseline;gap:6px;min-width:0}.name{min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;font-size:13px}.tag{flex:none;font-size:12px;color:#a6afb3}.eye{flex:none;align-self:center;color:#a6afb3}.hidden{font-style:italic}.remembered{text-decoration:underline dotted;text-underline-offset:4px}.player-row>strong{font-weight:500}.kda{white-space:nowrap;font-size:13px}.kda em{font-style:normal;margin:0 3px;color:#657178}.skeleton{background:#2b3135;border-radius:8px}.loading-header{height:100px;margin-bottom:22px}.loading-board{height:540px}
</style>

