<!--
  Loadout (spec §6.8, v8:336-352): Vandal, Phantom, Operator, Sheriff in the equipped chroma, the well tinted by the
  skin's content tier. Each weapon is tri-state (R97): absent = its name is still resolving → the loading row (a --skel
  well and a bar, no text); null = none or unknown → an empty raised well, the weapon name and —; else the skin.
  `loadout: null` = the Details skeleton (§6.9: 4 loading rows).
-->
<script lang="ts">
  import { assetUrl } from "../../lib/assets";
  import { CONTENT_TIERS, weaponState } from "../../lib/details";
  import { rgba } from "../../lib/theme";
  import { loadoutTip } from "../../lib/tips";
  import { tip } from "../../lib/tooltip.svelte";
  import type { Loadout } from "../../lib/types";
  import Skel from "../Skel.svelte";

  let { loadout }: { loadout: Loadout | null } = $props();

  const WEAPONS = [
    { key: "vandal", label: "Vandal" },
    { key: "phantom", label: "Phantom" },
    { key: "operator", label: "Operator" },
    { key: "sheriff", label: "Sheriff" },
  ] as const;

  /** The well's tint from its content tier colour T (v8:600): a rising wash and a 2px floor line. None for a default skin. */
  function wellStyle(tierId: string | null): string {
    const t = tierId ? CONTENT_TIERS[tierId]?.color : undefined;
    if (!t) return "";
    return (
      `background:linear-gradient(0deg, ${rgba(t, 0.26)} 0%, ${rgba(t, 0.07)} 55%, ${rgba(t, 0)} 100%), var(--raised);` +
      `box-shadow:inset 0 -2px 0 ${rgba(t, 0.85)}`
    );
  }
</script>

<section class="sec loadout">
  <div class="sech">Loadout</div>
  <div class="panel">
    {#each WEAPONS as w (w.key)}
      {@const st = loadout === null ? "loading" : weaponState(loadout[w.key])}
      <div class="wrow">
        {#if st === "loading"}
          <Skel w={124} h={48} r={5} block />
          <div class="wtext"><Skel w={90} h={13} block /></div>
        {:else if st === "none"}
          <div class="well"></div>
          <div class="wtext"><span class="wname">{w.label}</span><span class="wskin dash">—</span></div>
        {:else}
          <div class="well" style={wellStyle(st.tierId)} use:tip={loadoutTip(st.tierId)}>
            {#if st.tierId}<img class="wtier" src={assetUrl("contentTier", st.tierId)} alt="" />{/if}
            <img class="wimg" src={assetUrl("chroma", st.chromaId)} alt="" />
          </div>
          <div class="wtext">
            <span class="wname">{w.label}</span>
            <span class="wskin" data-ellipsis>{st.skinName}{#if st.chromaName}<span class="wch">{` · ${st.chromaName}`}</span>{/if}</span>
          </div>
        {/if}
      </div>
    {/each}
  </div>
</section>

<style>
  .sec {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .sech {
    font-size: 14px;
    font-weight: 600;
  }

  .panel {
    background: var(--card);
    border-radius: 8px;
    overflow: hidden;
  }

  .wrow {
    display: flex;
    align-items: center;
    gap: 14px;
    height: 62px;
    padding: 0 14px 0 8px;
  }

  .wrow + .wrow {
    border-top: 1px solid var(--line);
  }

  .well {
    position: relative;
    width: 124px;
    height: 48px;
    flex: none;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 5px;
    overflow: hidden;
    background: var(--raised);
  }

  .wtier {
    position: absolute;
    left: 5px;
    top: 5px;
    display: block;
    width: 13px;
    height: 13px;
    opacity: 0.9;
  }

  .wimg {
    position: relative;
    display: block;
    max-width: 108px;
    max-height: 38px;
    object-fit: contain;
  }

  .wtext {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .wname {
    font-size: 12px;
    font-weight: 500;
    color: var(--text2);
  }

  /* The design's own ellipsis span (spec §8.3 check 5 allows it). */
  .wskin {
    font-size: 14px;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .wskin.dash {
    color: var(--text2);
  }

  .wch {
    font-size: 13px;
    font-weight: 500;
    color: var(--text2);
  }
</style>
