<!--
  One Saved players row (design v2 479–504): agent, name (+ "In your match" while they are in the live match, with a
  green tint on the row), rank, the teammate / opponent records, the private note and when they were last seen. The
  row opens the profile drawer (click, Enter or Space on the row itself). The note cell edits in place: Edit / Add note
  opens a textarea that saves as you type (`setNote`, debounced), Esc or Done closes it; a note over 70 characters is
  clamped to two lines with a Show full note toggle.
-->
<script lang="ts">
  import { tick } from "svelte";
  import { setNote } from "../../lib/appdata.svelte";
  import { assetUrl } from "../../lib/assets";
  import { retryImg } from "../../lib/imgRetry";
  import { isRankedTier } from "../../lib/meta";
  import { openProfile } from "../../lib/router.svelte";
  import { LONG_NOTE, nameParts, recordCell, seenCell, type SavedView } from "../../lib/savedv2";
  import { meta, now } from "../../lib/stores";
  import { tierColor, tierName, tierShort } from "../../lib/theme";
  import { tip } from "../../lib/tooltip.svelte";
  import Dot from "../shell/Dot.svelte";
  import PlayerName from "../history/PlayerName.svelte";

  interface Props {
    v: SavedView;
    editing: boolean;
    expanded: boolean;
    onedit: (on: boolean) => void;
    ontoggle: () => void;
  }

  let { v, editing, expanded, onedit, ontoggle }: Props = $props();

  const who = $derived(nameParts(v.name, $meta).name);
  const withC = $derived(recordCell(v.s.with, who, "with"));
  const agC = $derived(recordCell(v.s.against, who, "against"));
  const seen = $derived(seenCell(v.s.lastSeenMs, v.inMatch, $now));
  const editLabel = $derived(v.note ? `Edit note on ${who}` : `Add a note on ${who}`);
  const rankText = $derived(v.s.tier === null ? "—" : tierName(v.s.tier, $meta));

  let row: HTMLElement;
  let area = $state<HTMLTextAreaElement | null>(null);
  let editBtn = $state<HTMLButtonElement | null>(null);

  function open() {
    openProfile(v.s.puuid, row);
  }

  function onKey(e: KeyboardEvent) {
    if (e.target !== e.currentTarget) return;
    if (e.key !== "Enter" && e.key !== " ") return;
    e.preventDefault();
    open();
  }

  async function startEdit(e: MouseEvent) {
    e.stopPropagation();
    onedit(true);
    await tick();
    area?.focus();
    const n = area?.value.length ?? 0;
    area?.setSelectionRange(n, n);
  }

  async function done(e?: Event) {
    e?.stopPropagation();
    onedit(false);
    await tick();
    editBtn?.focus();
  }

  function noteKey(e: KeyboardEvent) {
    e.stopPropagation();
    if (e.key === "Escape") {
      e.preventDefault();
      void done();
    }
  }
</script>

{#snippet record(c: ReturnType<typeof recordCell>)}
  <div class="rec" use:tip={c.tip}>
    {#if c.games !== null}
      <span class="games">{c.games}</span><span class="wl"><span class="w">{c.w}</span><span class="l">{c.l}</span></span>
    {:else}
      <span class="none">—</span>
    {/if}
  </div>
{/snippet}

<div
  bind:this={row}
  class={["row", v.inMatch && "live"]}
  data-row="1"
  tabindex="0"
  role="button"
  aria-label={`${who}, ${rankText}. Open profile`}
  onclick={open}
  onkeydown={onKey}
>
  <div class="player">
    <div class="agent">{#if v.s.agentId}<img {@attach retryImg} src={assetUrl("agent", v.s.agentId)} alt="" />{/if}</div>
    <div class="nameline">
      <PlayerName name={v.name} size="saved" />
      {#if v.inMatch}<span class="inmatch" use:tip={"In your current match"}><Dot color="var(--win)" pulse /><span class="imtext">In your match</span></span>{/if}
    </div>
  </div>
  <div class="rankcell">
    {#if isRankedTier(v.s.tier)}<img {@attach retryImg} class="ricon" src={assetUrl("tier", v.s.tier)} alt="" />{/if}
    <span class="rname" style:color={v.s.tier === null ? "var(--text-tag)" : tierColor(v.s.tier)}
      ><span class="rlong">{rankText}</span><span class="rshort">{v.s.tier === null ? "—" : tierShort(v.s.tier, $meta)}</span></span
    >
  </div>
  {@render record(withC)}
  {@render record(agC)}
  <!-- Clicks inside the note never open the profile. -->
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="notecell" onclick={(e) => editing && e.stopPropagation()}>
    {#if editing}
      <textarea
        bind:this={area}
        aria-label={editLabel}
        value={v.note}
        placeholder="Playstyle, comms, anything worth remembering…"
        oninput={(e) => setNote(v.s.puuid, e.currentTarget.value)}
        onkeydown={noteKey}
      ></textarea>
      <div class="edrow">
        <button type="button" class="done" onclick={done}>Done</button><span class="hint">Saves as you type · Esc closes</span>
      </div>
    {:else}
      <div class="noteline">
        {#if v.note}
          <span class={["note", expanded && "open"]}>{v.note}</span>
        {:else}
          <span class="nonote">No note yet</span>
        {/if}
        <button type="button" bind:this={editBtn} class="edit" aria-label={editLabel} use:tip={editLabel} onclick={startEdit}>
          <svg width="11" height="11" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round" aria-hidden="true"
            ><path d="M11 2.5l2.5 2.5L6 12.5 3 13l.5-3z"></path></svg
          >{v.note ? "Edit" : "Add note"}
        </button>
      </div>
      {#if v.note.length > LONG_NOTE}
        <button
          type="button"
          class="more"
          aria-expanded={expanded}
          onclick={(e) => {
            e.stopPropagation();
            ontoggle();
          }}>{expanded ? "Show less" : "Show full note"}</button
        >
      {/if}
    {/if}
  </div>
  <div class="seen" use:tip={seen.tip}><span>{seen.text}</span></div>
  <div class="chev">
    <svg width="7" height="12" viewBox="0 0 7 12" fill="none" stroke="var(--text-ghost)" stroke-width="1.5" stroke-linecap="round" aria-hidden="true"
      ><path d="M1.5 1.5L5.5 6l-4 4.5"></path></svg
    >
  </div>
</div>

<style>
  .row {
    display: grid;
    grid-template-columns: minmax(0, 2fr) minmax(0, 1.05fr) 128px 128px minmax(0, 2.3fr) 96px 12px;
    column-gap: 16px;
    align-items: center;
    padding: 12px 14px 12px 16px;
    border-bottom: 1px solid var(--line-soft);
    cursor: pointer;
  }

  .row.live {
    background: rgba(95, 207, 143, 0.045);
  }

  .row:hover {
    background: var(--row-hover);
  }

  /* The cells size their content to the column (container queries): at the minimum window the live chip keeps its
     dot only (its tooltip says it) so the name keeps room, and the rank uses the short tier name (`Asc 3`). */
  .player,
  .rankcell {
    container-type: inline-size;
  }

  .rshort {
    display: none;
  }

  @container (max-width: 239px) {
    .imtext {
      display: none;
    }

    .inmatch {
      padding: 3px 5px;
    }
  }

  @container (max-width: 121px) {
    .rlong {
      display: none;
    }

    .rshort {
      display: inline;
    }
  }

  .player {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
    min-height: 40px;
  }

  .agent {
    width: 40px;
    height: 40px;
    flex: none;
    border-radius: 8px;
    overflow: hidden;
    background: var(--skel);
    border: 1px solid var(--line-media);
  }

  .agent img {
    width: 100%;
    height: 100%;
    display: block;
  }

  .nameline {
    display: flex;
    align-items: center;
    gap: 5px;
    min-width: 0;
  }

  .inmatch {
    flex: none;
    display: inline-flex;
    align-items: center;
    gap: 5px;
    margin-left: 4px;
    font: 600 11.5px/1 var(--font);
    color: var(--win-text);
    background: rgba(95, 207, 143, 0.1);
    border: 1px solid rgba(95, 207, 143, 0.4);
    padding: 2px 6px;
    border-radius: 4px;
    white-space: nowrap;
  }

  .rankcell {
    display: flex;
    align-items: center;
    gap: 9px;
    min-width: 0;
    min-height: 40px;
  }

  .ricon {
    width: 26px;
    height: 26px;
    display: block;
    flex: none;
  }

  .rname {
    font: 700 15px/1 var(--font);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .rec {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 40px;
    justify-self: start;
  }

  .games {
    font: 600 14.5px/1 var(--font);
    color: var(--text);
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }

  .wl {
    display: flex;
    gap: 4px;
    padding: 3px 6px;
    border-radius: 4px;
    background: #1c1f24;
    font: 600 12px/1 var(--font);
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }

  .w {
    color: var(--win-text);
  }

  .l {
    color: var(--loss-text);
  }

  .none {
    font: 500 15px/1 var(--font);
    color: var(--text-tag);
  }

  .notecell {
    display: flex;
    flex-direction: column;
    gap: 7px;
    min-width: 0;
    padding-top: 1px;
  }

  .noteline {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    min-width: 0;
  }

  .note {
    flex: 1;
    min-width: 0;
    font: 500 13.5px/1.45 var(--font);
    color: #ddd8ff;
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    overflow: hidden;
    text-wrap: pretty;
    overflow-wrap: anywhere;
  }

  .note.open {
    -webkit-line-clamp: unset;
    line-clamp: unset;
  }

  .nonote {
    flex: 1;
    min-width: 0;
    font: 500 13.5px/1.45 var(--font);
    color: var(--text-tag);
  }

  .edit {
    flex: none;
    height: 26px;
    padding: 0 8px;
    border-radius: 6px;
    display: flex;
    align-items: center;
    gap: 5px;
    background: transparent;
    border: 1px solid var(--line-control);
    color: var(--text-3);
    font: 600 12px/1 var(--font);
    cursor: pointer;
  }

  .edit:hover {
    background: var(--button-hover);
    color: var(--text);
    border-color: var(--line-hover);
  }

  .more {
    align-self: flex-start;
    padding: 2px 0;
    background: none;
    border: none;
    font: 600 12.5px/1 var(--font);
    color: var(--accent-text);
    cursor: pointer;
  }

  .more:hover {
    color: var(--accent-pale);
  }

  textarea {
    height: 76px;
    resize: vertical;
    background: var(--input);
    border: 1px solid rgba(157, 140, 255, 0.45);
    border-radius: 7px;
    padding: 8px 10px;
    color: #e2ddff;
    font: 500 13.5px/1.45 var(--font);
    cursor: text;
  }

  .edrow {
    display: flex;
    gap: 10px;
    align-items: center;
  }

  .done {
    height: 26px;
    padding: 0 12px;
    border-radius: 6px;
    background: var(--accent);
    color: var(--on-accent);
    border: none;
    font: 600 12.5px/1 var(--font);
    cursor: pointer;
  }

  .done:hover {
    background: var(--accent-hover);
  }

  .hint {
    font: 500 12px/1 var(--font);
    color: var(--text-label);
  }

  .seen {
    display: flex;
    align-items: center;
    min-height: 40px;
    justify-self: start;
  }

  .seen span {
    font: 600 13.5px/1 var(--font);
    color: var(--text-soft);
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }

  .chev {
    min-height: 40px;
    display: flex;
    align-items: center;
  }
</style>
