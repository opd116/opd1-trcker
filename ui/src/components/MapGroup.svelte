<!-- The map in the top bar (spec §2.3, v8:26-31): name + list image, or the skeleton while the id is unknown. -->
<script lang="ts">
  import { assetUrl } from "../lib/assets";
  import { retryImg } from "../lib/imgRetry";
  import { mapName } from "../lib/meta";
  import { meta } from "../lib/stores";
  import Skel from "./Skel.svelte";

  let { mapId }: { mapId: string | null } = $props();

  const name = $derived(mapName($meta, mapId));
</script>

<div class="map">
  {#if mapId === null}
    <Skel w={52} h={12} block />
    <Skel w={116} h={32} r={4} block />
  {:else}
    {#if name === undefined}<Skel w={52} h={12} block />{:else}<span class="mname">{name ?? "—"}</span>{/if}
    <img {@attach retryImg} src={assetUrl("mapList", mapId)} alt="" />
  {/if}
</div>

<style>
  .map {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .mname {
    font-size: 14px;
    font-weight: 600;
    white-space: nowrap;
  }

  img {
    display: block;
    width: 116px;
    height: 32px;
    object-fit: cover;
    border-radius: 4px;
  }
</style>
