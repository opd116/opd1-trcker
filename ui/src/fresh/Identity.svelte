<script lang="ts">
  import { displayName, mapName, dateShort, type DisplayName } from './data';
  let { value }: { value: DisplayName | null | undefined } = $props();
  let identity = $derived(displayName(value));
  let explanation = $derived(value?.kind === 'remembered'
    ? `Hidden name · ${value.source === 'ownMatch' ? 'known from your match' : value.source === 'otherMatch' ? 'known from a past match' : 'seen before'}${value.fromMapId ? ' · ' + mapName(value.fromMapId) : ''}${value.fromStartMs ? ' · ' + dateShort(value.fromStartMs) : ''}`
    : identity.hidden ? 'Hidden name' : `${identity.name}${identity.tag ? '#' + identity.tag : ''}`);
</script>
<span class="identity" class:hidden={identity.hidden} class:remembered={identity.remembered} title={explanation}>
  {#if identity.hidden}<svg aria-label="Hidden name" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6"><path d="m3 3 18 18M10 5c5-1 9 4 11 7-1 2-2 3-4 5M6 6c-2 1-4 4-5 6 3 5 8 9 14 6M9 9a4 4 0 0 0 6 6"/></svg>{/if}
  {#if value === undefined}<span class="name-skeleton" aria-label="Loading identity"></span>{:else}<span class="name">{identity.name}</span>{#if identity.tag}<span class="tag">#{identity.tag}</span>{/if}{/if}
</span>
<style>
  .identity{display:flex;align-items:center;gap:6px;min-width:0;font-weight:600}.name{overflow:hidden;text-overflow:ellipsis;white-space:nowrap;min-width:0}.name-skeleton{display:block;width:90px;height:12px;background:#30373b;border-radius:3px}.tag{color:var(--muted);font-size:12px;font-weight:500;flex-shrink:0}.hidden:not(.remembered) .name{font-style:italic}.remembered .name{text-decoration:underline dotted;text-underline-offset:4px}.identity svg{flex-shrink:0;color:var(--muted)}
</style>
