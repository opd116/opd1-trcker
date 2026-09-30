<!-- Agent head icon (spec §3.3): only once the agent is locked (R99). No agent → the same box, empty.
     Never a placeholder agent. A missing image keeps the box. -->
<script lang="ts">
  import { assetUrl } from "../lib/assets";

  let { agentId }: { agentId: string | null } = $props();
</script>

{#if agentId}
  <img class="agent" src={assetUrl("agent", agentId)} alt="" />
{:else}
  <div class="agent"></div>
{/if}

<style>
  .agent {
    display: block;
    width: 40px;
    height: 40px;
    flex: none;
    border-radius: 6px;
    background: var(--raised);
  }
</style>
