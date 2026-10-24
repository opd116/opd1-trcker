<!--
  Who the player queues with most (prototype 1050–1062; `topTeammates`, current act): the agent when that teammate is
  on the live screen with a visible name (else a silhouette, so a row never reveals who is behind a hidden card), the
  name as the data gives it (hidden-name rules applied upstream, R93), and the games together. On the user's own
  profile the design adds the record under the name ("10W – 4L together", prototype 1502), left out while no shared
  match has a winner.
-->
<script lang="ts">
  import { assetUrl } from "../../lib/assets";
  import { liveTeammate } from "../../lib/details";
  import { retryImg } from "../../lib/imgRetry";
  import { nameView, type PartyChip } from "../../lib/profile";
  import { meta } from "../../lib/stores";
  import { liveTip } from "../../lib/tips";
  import { tip } from "../../lib/tooltip.svelte";
  import type { LiveState, TeammateSummary } from "../../lib/types";
  import EyeOff from "../icons/EyeOff.svelte";
  import Skel from "../Skel.svelte";

  interface Props {
    /** null while the details load. */
    mates: TeammateSummary[] | null;
    live: LiveState | null;
    /** The profile player's live party: its members' icons take the party colour. */
    party: PartyChip | null;
    /** The user's own profile: the rows carry the record together. */
    isSelf?: boolean;
  }

  let { mates, live, party, isSelf = false }: Props = $props();

  const rows = $derived(
    (mates ?? []).slice(0, 3).map((m) => ({
      puuid: m.puuid,
      nv: nameView(m.name, $meta),
      count: m.count,
      record: isSelf && m.wins + m.losses > 0 ? `${m.wins}W – ${m.losses}L together` : null,
      lv: liveTeammate(m.puuid, live),
      inParty: !!party?.mates.some((x) => x.puuid === m.puuid),
    })),
  );
</script>

<div class="card">
  {#if mates === null}
    {#each [0, 1, 2] as i (i)}
      <div class="row" aria-hidden="true"><Skel w={30} h={30} r={6} block /><div class="who"><Skel w={110} h={13} block /></div><Skel w={18} h={16} block /></div>
    {/each}
  {:else if rows.length === 0}
    <div class="row"><span class="dash">—</span></div>
  {:else}
    {#each rows as m (m.puuid)}
      <div class="row">
        {#if m.lv && m.lv.agentId}
          <img
            {@attach retryImg}
            class="ic"
            style:border-color={m.inParty ? party?.color : undefined}
            src={assetUrl("agent", m.lv.agentId)}
            alt=""
            use:tip={liveTip($meta, m.lv.agentId)}
          />
        {:else}
          <span class="ic ghost"
            ><svg width="22" height="24" viewBox="0 0 22 24" fill="currentColor" aria-hidden="true"
              ><circle cx="11" cy="8" r="5" /><path d="M1 24c.6-6 4.6-9.5 10-9.5S20.4 18 21 24z" /></svg
            ></span
          >
        {/if}
        <div class="who">
          <div class="nl" use:tip={!m.nv ? "" : m.nv.style === "visible" ? { text: m.nv.tip, overflowOnly: true } : m.nv.tip}>
            {#if m.nv?.style === "remembered"}<EyeOff size={13} />{/if}
            <span class={["nm", m.nv?.style]} dir="auto" data-ellipsis>{m.nv?.text ?? "—"}</span>
            {#if m.nv?.tag}<span class="tg">{m.nv.tag}</span>{/if}
          </div>
          {#if m.record}<span class="sub">{m.record}</span>{/if}
        </div>
        <div class="g"><span class="gn">{m.count}</span><span class="gl">{m.count === 1 ? "game" : "games"}</span></div>
      </div>
    {/each}
  {/if}
</div>

<style>
  .card {
    border: 1px solid var(--line);
    border-radius: 10px;
    overflow: hidden;
    background: var(--card);
  }

  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 52px;
    padding: 0 12px;
    border-bottom: 1px solid var(--line-soft);
  }

  .row:last-child {
    border-bottom: none;
  }

  .ic {
    width: 30px;
    height: 30px;
    box-sizing: border-box;
    border-radius: 6px;
    display: block;
    flex: none;
    background: var(--skel);
    border: 1px solid var(--line-control);
  }

  .ghost {
    background: linear-gradient(160deg, #262a31, var(--button));
    display: flex;
    align-items: flex-end;
    justify-content: center;
    overflow: hidden;
    color: var(--text-ghost);
  }

  .who {
    display: flex;
    flex-direction: column;
    gap: 5px;
    min-width: 0;
    flex: 1;
  }

  .nl {
    display: flex;
    align-items: baseline;
    gap: 5px;
    min-width: 0;
  }

  .nl :global(svg) {
    align-self: center;
    color: var(--text-muted);
  }

  .nm {
    font: 600 14px/1 var(--font);
    color: var(--text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }

  .nm.hidden {
    font-style: italic;
    color: var(--text-muted);
  }

  /* The dotted underline hangs below the clipped box: widen the paint clip (as PlayerRow does). */
  .nm.remembered {
    text-decoration: underline dotted var(--text-muted);
    text-underline-offset: 3px;
    overflow: clip;
    overflow-clip-margin: 6px;
  }

  .tg {
    flex: none;
    font: 500 12px/1 var(--font);
    color: var(--text-tag);
  }

  .sub {
    font: 500 12px/1 var(--font);
    color: var(--text-label);
    white-space: nowrap;
  }

  .g {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 4px;
    flex: none;
  }

  .gn {
    font: 700 16px/1 var(--font);
    color: var(--text);
    font-variant-numeric: tabular-nums;
  }

  .gl {
    font: 500 11.5px/1 var(--font);
    color: var(--text-label);
  }

  .dash {
    color: var(--text-muted);
    font: 500 14px/1 var(--font);
  }
</style>
