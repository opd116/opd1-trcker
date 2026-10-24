<!--
  The profile drawer (design v2, TrackerApp.dc.html 949–1128; logic `profile(id)` 1438–1574). App mounts it inside
  <main> while `router.profile` is set; `puuid` changes in place when another player is opened. The router fetches the
  data into `openDetails` (only used when its card is this player's); this drawer adds `profile_extras` (maps and the
  "in your match" chips). While the details load, the header comes from the live card and every tri-state field shows
  its skeleton; a details answer that settled empty (router `settledDetails`) draws `—`.
  Close: the scrim, ×, Escape (the note popover first). Focus moves into the panel on open and the router puts it back
  on the element that opened the drawer.
-->
<script lang="ts">
  import { tick } from "svelte";
  import { profileExtras } from "../../lib/api";
  import { dismissSmurf, isSaved, marks, noteOf, setNote, settings, toggleSaved } from "../../lib/appdata.svelte";
  import { assetUrl } from "../../lib/assets";
  import { retryBg, retryImg } from "../../lib/imgRetry";
  import { agentName } from "../../lib/meta";
  import { nameView, partyChip, statCells } from "../../lib/profile";
  import { closeProfile, go } from "../../lib/router.svelte";
  import { smurfSignal } from "../../lib/smurf";
  import { live, meta, mystats, openDetails } from "../../lib/stores";
  import { tip } from "../../lib/tooltip.svelte";
  import type { PlayerCard, ProfileExtras } from "../../lib/types";
  import { reportError } from "../../lib/uilog";
  import EyeOff from "../icons/EyeOff.svelte";
  import Skel from "../Skel.svelte";
  import Encounters from "./Encounters.svelte";
  import Loadout from "./Loadout.svelte";
  import Maps from "./Maps.svelte";
  import Mates from "./Mates.svelte";
  import RankCard from "./RankCard.svelte";
  import RecentTable from "./RecentTable.svelte";
  import SmurfPanel from "./SmurfPanel.svelte";

  let { puuid }: { puuid: string } = $props();

  /** This player's details, never another player's (a stale answer is not shown). */
  const details = $derived($openDetails?.card.puuid === puuid ? $openDetails : null);
  const liveCard = $derived($live?.players.find((c) => c.puuid === puuid));
  const inMatch = $derived(!!liveCard && ($live?.stage === "ingame" || $live?.stage === "pregame"));
  /** The header's card: the details' (newest), else the live card's while they load. */
  const card: PlayerCard | null = $derived(details?.card ?? liveCard ?? null);
  const isSelf = $derived(!!card?.isSelf);
  const nv = $derived(card ? nameView(card.name, $meta) : undefined);
  const agent = $derived(card ? agentName($meta, card.agentId) : null);
  const label = $derived(`${!nv || nv.text === "—" ? "Player" : nv.style === "hidden" ? "Hidden player" : nv.text} profile`);
  const sub = $derived.by(() => {
    if (!card) return undefined;
    const lv = card.level === undefined ? undefined : card.level === null ? "—" : String(card.level);
    return { agent: agent || null, level: lv, remembered: nv?.style === "remembered" };
  });
  const party = $derived(liveCard && inMatch ? partyChip(liveCard, $live, $meta) : null);
  const smurf = $derived(
    liveCard && inMatch ? smurfSignal(liveCard, $live?.players ?? [], $settings.smurf, $marks.notSmurf) : null,
  );
  const saved = $derived(isSaved($marks, puuid));
  const note = $derived(noteOf($marks, puuid));
  const stats = $derived(
    card
      ? statCells(card, details, {
          highlight: $settings.highlight,
          netRr: $mystats === undefined ? undefined : ($mystats?.rank.netRr ?? null),
          netGames:
            $mystats && $mystats.rank.wins !== null && $mystats.rank.losses !== null
              ? $mystats.rank.wins + $mystats.rank.losses
              : null,
        })
      : null,
  );
  const artUrl = $derived(card?.cardId ? assetUrl("cardWide", card.cardId) : "");

  // ── profile_extras (maps, in-your-match chips) ─────────────────────────────────
  /** Asked again when the player, their newest stored match, or the live stage changes. */
  const extrasKey = $derived(`${puuid}|${details?.recent[0]?.matchId ?? ""}|${$live?.stage ?? ""}`);
  let extrasState = $state<{ puuid: string; data: ProfileExtras | null } | null>(null);
  $effect(() => {
    const key = extrasKey;
    const p = key.slice(0, key.indexOf("|"));
    let gone = false;
    profileExtras(p)
      .then((x) => {
        if (!gone) extrasState = { puuid: p, data: x };
      })
      .catch((err: unknown) => {
        reportError("profile_extras", err);
        if (!gone) extrasState = { puuid: p, data: null };
      });
    return () => {
      gone = true;
    };
  });
  /** undefined while the first answer for this player is pending, null when it failed. */
  const extras = $derived(extrasState?.puuid === puuid ? extrasState.data : undefined);

  // ── layout (prototype `loadOrder` / `loadCol` / `mapsPad`) ──────────────────────
  /** Encounters: the details' list; while loading, the live card's count says whether to hold a place for it. */
  const encRows = $derived(details ? details.encounters : null);
  const encExpected = $derived(
    !isSelf && (details ? details.encounters.length > 0 : (liveCard?.encounters?.with ?? 0) + (liveCard?.encounters?.against ?? 0) > 0),
  );
  /** No encounters (another player): the loadout spans both columns in four tiles, the maps sit level with the mates. */
  const wideLoadout = $derived(!isSelf && !encExpected);

  // ── save + note popover (prototype 975–985, `onSave` / `unsave`) ──────────────
  let noteOpen = $state(false);
  /** Saved by this very click: the popover's × reads Cancel and also clears the note. */
  let fresh = $state(false);
  let saveBtn = $state<HTMLButtonElement | null>(null);

  function onSave() {
    if (saved) {
      fresh = false;
      noteOpen = !noteOpen;
    } else {
      fresh = true;
      void toggleSaved(puuid, true);
      noteOpen = true;
    }
  }

  function closeNote(refocus = true) {
    noteOpen = false;
    if (refocus) void tick().then(() => saveBtn?.focus({ preventScroll: true }));
  }

  function unsave() {
    const wasFresh = fresh;
    fresh = false;
    void toggleSaved(puuid, false);
    if (wasFresh) setNote(puuid, "");
    closeNote();
  }

  function onNoteInput(v: string) {
    setNote(puuid, v);
    if (v.trim() !== "" && !saved) void toggleSaved(puuid, true);
  }

  /** The popover's field takes the focus when it opens (the prototype's autoFocus). */
  const focusNow = (el: HTMLElement) => {
    queueMicrotask(() => el.focus({ preventScroll: true }));
  };

  function noteKey(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.stopPropagation();
      closeNote();
    }
  }

  // ── focus, Escape, scroll shadow ───────────────────────────────────────────────
  let panel = $state<HTMLDivElement | null>(null);
  let body = $state<HTMLDivElement | null>(null);
  let scrolled = $state(false);

  // A new player (or the first open): top of the body, popover closed, focus in the panel.
  $effect(() => {
    void puuid;
    noteOpen = false;
    fresh = false;
    scrolled = false;
    if (body) body.scrollTop = 0;
    void tick().then(() => panel?.focus({ preventScroll: true }));
  });

  const FOCUSABLE = 'a[href],button:not([disabled]),textarea,input,select,[tabindex]:not([tabindex="-1"])';

  function panelKey(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.stopPropagation();
      if (noteOpen) closeNote();
      else closeProfile();
      return;
    }
    // aria-modal: Tab stays inside the panel.
    if (e.key === "Tab" && panel) {
      const els = [...panel.querySelectorAll<HTMLElement>(FOCUSABLE)].filter((el) => el.offsetParent !== null);
      if (els.length === 0) return;
      const first = els[0]!;
      const last = els[els.length - 1]!;
      const a = document.activeElement;
      if (e.shiftKey && (a === first || a === panel)) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && a === last) {
        e.preventDefault();
        first.focus();
      }
    }
  }

  /**
   * Escape with the focus outside the panel (a title bar button was clicked, or focus fell to the page): the panel's
   * own handler never hears it, so the window's closes the drawer too. Inside the panel that handler stops it first.
   */
  function windowKey(e: KeyboardEvent) {
    if (e.key !== "Escape" || e.defaultPrevented) return;
    e.preventDefault();
    if (noteOpen) closeNote();
    else closeProfile();
  }

  function onScroll(e: Event) {
    const s = (e.currentTarget as HTMLElement).scrollTop > 4;
    if (s !== scrolled) scrolled = s;
  }
</script>

<svelte:window onkeydown={windowKey} />

<div class="overlay">
  <button type="button" class="scrim" tabindex="-1" aria-label="Close profile" onclick={closeProfile}></button>
  <div
    bind:this={panel}
    class="panel"
    role="dialog"
    aria-modal="true"
    aria-label={label}
    aria-busy={details === null}
    tabindex="-1"
    onkeydown={panelKey}
  >
    <header class={["head", scrolled && "shadow"]}>
      {#if artUrl}<div class="art" {@attach retryBg(artUrl)}></div>{/if}
      <div class="fade"></div>
      <div class="hrow">
        <div class="agent">
          {#if card?.agentId}<img {@attach retryImg} src={assetUrl("agent", card.agentId)} alt={agent || ""} />{:else if !card}<Skel
              w={60}
              h={60}
              r={10}
              block
            />{/if}
        </div>
        <div class="who">
          <div class="nline" use:tip={!nv ? "" : nv.style === "visible" ? { text: nv.tip, overflowOnly: true } : nv.tip}>
            {#if nv === undefined}
              <Skel w={180} h={24} block />
            {:else}
              {#if nv.style === "remembered"}<span class="eye"><EyeOff size={16} /></span>{/if}
              <span class={["name", nv.style]} dir="auto" data-ellipsis>{nv.text}</span>
              {#if nv.tag}<span class={["tag", nv.style === "remembered" && "rem"]}>{nv.tag}</span>{/if}
              {#if isSelf}<span class="you">YOU</span>{/if}
            {/if}
          </div>
          <div class="sline">
            {#if sub === undefined}
              <Skel w={120} h={13} block />
            {:else}
              <span class="sub"
                >{sub.agent ? `${sub.agent} · Level ` : "Level "}{#if sub.level === undefined}<Skel w={28} h={11} />{:else}{sub.level}{/if}{sub.remembered
                  ? " · name remembered from an earlier match"
                  : ""}</span
              >
            {/if}
            {#if party}
              <span class="party" style:--pc={party.color} use:tip={party.tip}>
                <span class="pdot"></span>Party<span class="pm">
                  {#each party.mates as m (m.puuid)}
                    {#if m.agentId}<img {@attach retryImg} src={assetUrl("agent", m.agentId)} alt={m.alt} />{:else}<span class="pmx"></span>{/if}
                  {/each}
                </span>
              </span>
            {/if}
            {#if smurf}
              <span class="smurf"
                ><svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><path d="M5 .8L9.6 9.2H.4z" fill="currentColor" /></svg
                >Possible smurf · automated</span
              >
            {/if}
          </div>
        </div>
        {#if card && !isSelf}
          <button
            bind:this={saveBtn}
            type="button"
            class={["btn", "save", saved && "on"]}
            aria-pressed={saved}
            aria-expanded={noteOpen}
            onclick={onSave}
          >
            <svg width="11" height="13" viewBox="0 0 10 12" fill={saved ? "currentColor" : "none"} stroke="currentColor" stroke-width="1.2" stroke-linejoin="round" aria-hidden="true"
              ><path d="M1 1h8v10L5 8.2 1 11z" /></svg
            >{saved ? "Saved" : "Save player"}
          </button>
        {/if}
        {#if isSelf}
          <button type="button" class="btn" onclick={() => go("comp")}>
            <svg width="14" height="14" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"
              ><path d="M3 16l4.5-5 3.5 3 6-8M13 6h4v4" /></svg
            >Competitive stats
          </button>
        {/if}
        <button type="button" class="close" aria-label="Close profile" use:tip={"Close (Esc)"} onclick={closeProfile}>
          <svg width="12" height="12" viewBox="0 0 10 10" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" aria-hidden="true"
            ><path d="M1 1l8 8M9 1L1 9" /></svg
          >
        </button>
      </div>
    </header>

    {#if noteOpen && !isSelf}
      <button type="button" class="nscrim" tabindex="-1" aria-label="Close note" onclick={() => closeNote()}></button>
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="pop" onkeydown={noteKey}>
        <span class="mlabel">NOTE ON {(nv?.ref ?? "this player").toUpperCase()}</span>
        <textarea
          {@attach focusNow}
          aria-label={`Note on ${nv?.ref ?? "this player"}`}
          placeholder="Playstyle, comms, anything worth remembering…"
          value={note}
          oninput={(e) => onNoteInput(e.currentTarget.value)}
        ></textarea>
        <div class="pact">
          <button type="button" class="px" aria-label={fresh ? "Cancel" : "Remove from saved"} use:tip={fresh ? "Cancel" : "Remove from saved"} onclick={unsave}>
            <svg width="12" height="12" viewBox="0 0 12 12" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" aria-hidden="true"
              ><path d="M2 2l8 8M10 2l-8 8" /></svg
            >
          </button>
          <button type="button" class="pok" aria-label="Save" use:tip={"Save"} onclick={() => closeNote()}>
            <svg width="14" height="12" viewBox="0 0 14 12" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"
              ><path d="M1.5 6.5l3.8 3.8L12.5 1.8" /></svg
            >
          </button>
        </div>
      </div>
    {/if}

    <div bind:this={body} class="body" onscroll={onScroll}>
      <RankCard {card} {stats} />

      {#if smurf}
        <SmurfPanel reasons={smurf.reasons} level={$settings.smurf} ondismiss={() => {
            void dismissSmurf(puuid);
            panel?.focus({ preventScroll: true });
          }}
        />
      {/if}

      <div class="grid">
        {#if isSelf}
          <div class="notes">
            <div class="nhead"><span class="mlabel">PERSONAL NOTES</span><span class="hint">Reminders for yourself</span></div>
            <textarea
              aria-label="Personal notes"
              placeholder="Goals for this session, warm-up routine, things to work on…"
              value={note}
              oninput={(e) => setNote(puuid, e.currentTarget.value)}
            ></textarea>
            <span class="foot">Not shown in live matches or to other players.</span>
          </div>
        {:else if card}
          <Encounters rows={encRows} {puuid} them={nv} expected={encExpected} />
        {/if}
        <Mates mates={details ? details.topTeammates : null} live={$live} {party} {isSelf} />
        <Loadout loadout={details ? details.loadout : null} wide={wideLoadout} />
        <Maps rows={extras === undefined ? undefined : (extras?.maps ?? [])} pad={!wideLoadout} />
      </div>

      <RecentTable rows={details ? details.recent : null} {extras} {isSelf} {puuid} />
    </div>
  </div>
</div>

<style>
  .overlay {
    position: absolute;
    inset: 0;
    z-index: 20;
    display: flex;
    justify-content: flex-end;
  }

  .scrim {
    position: absolute;
    inset: 0;
    border: 0;
    padding: 0;
    background: var(--scrim);
    cursor: default;
  }

  .panel {
    position: relative;
    width: 820px;
    max-width: calc(100% - 48px);
    height: 100%;
    background: var(--drawer);
    border-left: 1px solid var(--line-control);
    box-shadow: -30px 0 60px rgba(0, 0, 0, 0.5);
    display: flex;
    flex-direction: column;
    overflow: hidden;
    outline: none;
  }

  /* ── header ── */
  .head {
    position: relative;
    z-index: 2;
    flex: none;
    height: 100px;
    overflow: hidden;
    border-bottom: 1px solid var(--line-control);
    transition: box-shadow 0.15s ease;
  }

  .head.shadow {
    box-shadow: 0 12px 22px -10px rgba(0, 0, 0, 0.8);
  }

  .art {
    position: absolute;
    inset: 0;
    background-size: cover;
    background-position: center;
    opacity: 0.26;
  }

  .fade {
    position: absolute;
    inset: 0;
    background: linear-gradient(90deg, var(--drawer) 12%, rgba(17, 19, 23, 0.8) 60%, rgba(17, 19, 23, 0.5));
  }

  .hrow {
    position: relative;
    height: 100%;
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 0 16px 0 24px;
  }

  .agent {
    width: 60px;
    height: 60px;
    border-radius: 10px;
    overflow: hidden;
    border: 1px solid var(--line-control);
    background: var(--skel);
    flex: none;
  }

  .agent img {
    width: 100%;
    height: 100%;
    display: block;
  }

  .who {
    display: flex;
    flex-direction: column;
    gap: 9px;
    min-width: 0;
    flex: 1;
  }

  .nline {
    display: flex;
    align-items: baseline;
    gap: 8px;
    min-width: 0;
  }

  .name {
    font: 700 28px/1 var(--font-display);
    letter-spacing: 0.02em;
    color: var(--text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }

  .name.hidden {
    color: var(--text-muted);
    font-style: italic;
  }

  /* The dotted underline hangs below the clipped box: widen the paint clip (as PlayerRow does). */
  .name.remembered {
    text-decoration: underline dotted var(--text-muted);
    text-underline-offset: 4px;
    text-decoration-thickness: 1.5px;
    overflow: clip;
    overflow-clip-margin: 6px;
  }

  .eye {
    align-self: center;
    display: flex;
    color: var(--text-muted);
  }

  .tag {
    flex: none;
    font: 500 15px/1 var(--font);
    color: var(--text-muted);
  }

  .tag.rem {
    text-decoration: underline dotted var(--text-muted);
    text-underline-offset: 3px;
  }

  .you {
    flex: none;
    align-self: center;
    font: 700 10px/1 var(--font-mono);
    letter-spacing: 0.06em;
    color: var(--on-accent);
    background: var(--accent);
    padding: 3px 5px;
    border-radius: 3px;
  }

  .sline {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    min-width: 0;
  }

  .sub {
    font: 500 13px/1 var(--font);
    color: var(--text-2);
  }

  .party {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    font: 600 12.5px/1 var(--font);
    color: var(--text);
    background: var(--skel);
    border: 1px solid color-mix(in srgb, var(--pc) 50%, transparent);
    padding: 2px 3px 2px 8px;
    border-radius: 5px;
    cursor: default;
  }

  .pdot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--pc);
  }

  .pm {
    display: flex;
    gap: 3px;
  }

  .pm img,
  .pmx {
    width: 20px;
    height: 20px;
    border-radius: 4px;
    display: block;
    background: var(--drawer);
  }

  .smurf {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font: 600 12.5px/1 var(--font);
    color: var(--warn-text);
    border: 1px dashed color-mix(in srgb, var(--warn) 65%, transparent);
    padding: 3px 7px;
    border-radius: 4px;
  }

  .smurf svg {
    color: var(--warn);
  }

  .btn {
    height: 34px;
    padding: 0 14px;
    border-radius: 8px;
    background: var(--button);
    color: var(--text);
    border: 1px solid var(--line-control);
    font: 600 13px/1 var(--font);
    cursor: pointer;
    display: flex;
    align-items: center;
    gap: 8px;
    flex: none;
  }

  .btn:hover {
    background: var(--button-hover);
    border-color: var(--line-hover);
  }

  .save.on {
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    color: var(--accent-pale);
    border-color: color-mix(in srgb, var(--accent) 50%, transparent);
  }

  .close {
    width: 34px;
    height: 34px;
    border-radius: 8px;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-muted);
    background: transparent;
    border: none;
    cursor: pointer;
    flex: none;
  }

  .close:hover {
    background: var(--skel);
    color: var(--text);
  }

  /* ── note popover ── */
  .nscrim {
    position: absolute;
    inset: 0;
    z-index: 5;
    border: 0;
    padding: 0;
    background: transparent;
    cursor: default;
  }

  .pop {
    position: absolute;
    z-index: 6;
    top: 74px;
    right: 64px;
    width: 320px;
    background: var(--button);
    border: 1px solid var(--line-tip);
    border-radius: 10px;
    box-shadow: 0 16px 40px rgba(0, 0, 0, 0.55);
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .pop .mlabel {
    color: var(--text-muted);
  }

  .pop textarea {
    height: 88px;
    resize: vertical;
    background: var(--input);
    border: 1px solid color-mix(in srgb, var(--accent) 38%, transparent);
    border-radius: 7px;
    padding: 9px 11px;
    color: var(--accent-pale);
    font: 500 14px/1.45 var(--font);
  }

  .pact {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }

  .px,
  .pok {
    height: 30px;
    border-radius: 7px;
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
  }

  .px {
    width: 34px;
    background: var(--button-hover);
    border: 1px solid var(--line-tip);
    color: var(--error-text);
  }

  .px:hover {
    background: color-mix(in srgb, var(--loss) 16%, transparent);
    border-color: color-mix(in srgb, var(--loss) 50%, transparent);
  }

  .pok {
    width: 40px;
    background: var(--accent);
    border: none;
    color: var(--on-accent);
  }

  .pok:hover {
    background: var(--accent-hover);
  }

  /* ── body ── */
  .body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    overflow-x: hidden;
    padding: 20px 24px 32px;
    display: flex;
    flex-direction: column;
    gap: 22px;
  }

  .grid {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    gap: 22px 24px;
    align-items: start;
  }

  .mlabel {
    font: 500 10.5px/1 var(--font-mono);
    letter-spacing: 0.06em;
    color: var(--text-label);
  }

  .notes {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .nhead {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 10px;
  }

  .hint {
    font: 500 12.5px/1 var(--font);
    color: var(--text-label);
  }

  .notes textarea {
    height: 84px;
    resize: vertical;
    background: var(--input);
    border: 1px solid var(--line-control);
    border-radius: 8px;
    padding: 10px 12px;
    color: var(--text);
    font: 500 14px/1.45 var(--font);
  }

  .foot {
    font: 500 12px/1.4 var(--font);
    color: var(--text-label);
  }
</style>
