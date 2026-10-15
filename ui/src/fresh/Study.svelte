<script lang="ts">
  import { getScene, getStats, getDetails, getMatch, mapName, asset } from './data';
  import Live from './Live.svelte';
  import Roster from '../independent/Roster.svelte';
  import PlayerDetails from './PlayerDetails.svelte';
  import Scoreboard from './Scoreboard.svelte';
  import Stats from './Stats.svelte';
  import './study.css';
  const sceneName = new URLSearchParams(location.search).get('scene') ?? 'ingame';
  const scene = getScene(sceneName);
  const layout = new URLSearchParams(location.search).get('layout') ?? 'roster';
  let route = $state(scene.route);
  let previous = $state(scene.route);
  let expanded = $state(scene.route.name === 'details' && scene.route.more);
  const positions = new Map<string,number>();
  const rememberScroll = (node:HTMLElement) => {
    const key = route.name === 'details' ? `details:${route.puuid}` : route.name;
    queueMicrotask(() => {node.scrollTop = positions.get(key) ?? 0;});
    return {destroy:() => positions.set(key,node.scrollTop)};
  };
  let active = $derived(route.name === 'mystats' || (route.name === 'match' && previous.name === 'mystats') ? 'mystats' : 'live');
  const openPlayer = (puuid:string) => {expanded = false; route = {name:'details',puuid,more:false};};
  const openMatch = (matchId:string) => {
    previous = route;
    const puuid = route.name === 'details' ? route.puuid : getStats(sceneName)?.puuid ?? '';
    route = {name:'match',matchId,focusPuuid:puuid,from:'recent',back:previous.name === 'details' ? {puuid,more:false} : 'mystats'};
  };
</script>
<div class="fresh-app">
  <header class="app-header">
    <div class="brand">OPD1 <span>TRCKER</span></div>
    <nav aria-label="Main navigation"><button class:active={active === 'live'} onclick={() => route = {name:'live'}}>Live</button><button class:active={active === 'mystats'} onclick={() => route = {name:'mystats'}}>My Stats</button></nav>
    {#if route.name === 'live' && scene.live?.mapId}<div class="map"><img src={asset('mapList',scene.live.mapId)} alt=""/><span>{mapName(scene.live.mapId)}</span></div>{/if}
    <button class="close" aria-label="Close preview" title="Close preview" onclick={() => location.replace('about:blank')}>×</button>
  </header>
  {#key route.name + (route.name === 'details' ? route.puuid : route.name === 'match' ? route.matchId : '')}
  <div class="page" class:scroll={route.name !== 'live'} use:rememberScroll>
    {#if route.name === 'live'}{#if layout === 'teams'}<Live {scene} onplayer={openPlayer}/>{:else}<Roster {scene} {sceneName} onplayer={openPlayer}/>{/if}
    {:else if route.name === 'details'}<PlayerDetails data={getDetails(sceneName,route.puuid)} bind:more={expanded} onback={() => route = {name:'live'}} onmatch={openMatch}/>
    {:else if route.name === 'match'}<Scoreboard data={getMatch(sceneName,route.matchId,route.focusPuuid)} onback={() => route = previous}/>
    {:else}<Stats data={getStats(sceneName)} onmatch={openMatch}/>{/if}
  </div>
  {/key}
</div>
