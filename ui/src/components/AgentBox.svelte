<!-- Agent head icon (spec §3.3): only once the agent is locked (R99). No agent: the same box, empty. Never a
     placeholder agent. A missing image keeps the box.
     One rule for every stage, on purpose: this component takes no stage prop. Agent Select before the lock, Lobby
     and Closed (where no agent can ever exist) all draw the same empty 40x40 `--raised` box, because the card is
     identical everywhere (CLAUDE.md) and R99 already names this state "the empty agent slot". The box is not a
     loading shape: it is `div.agent`, never the skeleton's `.sk`, and it sits beside a real name, rank and stats.
     A transparent slot in Lobby/Closed would make one card state (no agent) render two ways depending on stage,
     and leave a 40px hole beside the name. If the user ever wants it: a stage prop plus one `transparent` rule
     here, and flip the fit-live agent-box test (task 17 review F8; ruled R127). -->
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
