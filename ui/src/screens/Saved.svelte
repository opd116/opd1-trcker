<!--
  Saved players (design v2 450–522, logic 1680–1740): the title with the count, a search over name, tag and note, the
  All / Teammates / Opponents pills with counts, and a sortable table (default: last seen, most recent first; the
  header's columns sort, NOTE does not). Rows are `saved_players`, asked again whenever the saved set changes and after a
  match ends; a note shows the live `marks` copy, so typing in a row never waits on a round trip. Empty: the design's "No saved players
  yet" with Go to live match.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { fromAction } from "svelte/attachments";
  import SavedRow from "../components/saved/SavedRow.svelte";
  import Skel from "../components/Skel.svelte";
  import { savedPlayers } from "../lib/api";
  import { marks, savedSet } from "../lib/appdata.svelte";
  import { followMatchEnds } from "../lib/matchEnd";
  import { go, scrollHost } from "../lib/router.svelte";
  import {
    arrange,
    counts,
    DEFAULT_SORT,
    liveCards,
    nextSort,
    noResultsText,
    rowName,
    sortState,
    summary,
    type SavedFilter,
    type SavedSort,
    type SavedView,
    type SortKey,
  } from "../lib/savedv2";
  import { live, meta, now } from "../lib/stores";
  import { tip } from "../lib/tooltip.svelte";
  import type { SavedPlayer } from "../lib/types";
  import { reportError } from "../lib/uilog";

  let list = $state<SavedPlayer[] | null>(null);
  let filter = $state<SavedFilter>("all");
  let query = $state("");
  let sort = $state<SavedSort>(DEFAULT_SORT);
  let editing = $state<string | null>(null);
  let expanded = $state<Record<string, boolean>>({});

  let gen = 0;
  function load() {
    const g = ++gen;
    savedPlayers()
      .then((rows) => {
        if (g === gen) list = rows;
      })
      .catch((err: unknown) => {
        reportError("saved_players", err);
        if (g === gen && list === null) list = [];
      });
  }

  // Ask for the rows on open and whenever someone is saved or unsaved (a note change needs no round trip).
  const savedKey = $derived($marks.saved.join(","));
  $effect(() => {
    void savedKey;
    load();
  });

  // A match that just ended changes LAST SEEN and the records once it is stored (lib/matchEnd.ts): ask again then.
  onMount(() =>
    followMatchEnds(live, () => async () => {
      load();
      return false;
    }),
  );

  const cards = $derived(liveCards($live));

  /** Every saved row with the name and note it shows; an unsaved row leaves at once. */
  const views = $derived.by((): SavedView[] => {
    if (!list) return [];
    const m = $marks;
    return list
      .filter((s) => $savedSet.has(s.puuid) || !m.saved.length)
      .map((s) => ({
        s,
        name: rowName(s, cards.get(s.puuid)),
        note: m.saved.includes(s.puuid) ? (m.notes[s.puuid] ?? "") : s.note,
        inMatch: cards.has(s.puuid),
      }));
  });

  const n = $derived(counts(views.map((v) => v.s)));
  const shown = $derived(arrange(views, filter, query, sort, $meta, $now, editing));

  const FILTERS: { k: SavedFilter; label: string }[] = [
    { k: "all", label: "All" },
    { k: "mates", label: "Teammates" },
    { k: "opp", label: "Opponents" },
  ];

  const COLS: { k: SortKey | null; label: string; tip: string }[] = [
    { k: "name", label: "PLAYER", tip: "Sort by name" },
    { k: "rank", label: "RANK", tip: "Sort by current rank" },
    { k: "with", label: "AS TEAMMATE", tip: "Stored matches where they were on your team, and your record together. Click to sort." },
    { k: "against", label: "AS OPPONENT", tip: "Stored matches where they were on the other team, and your record against them. Click to sort." },
    { k: null, label: "NOTE", tip: "Your private note" },
    { k: "seen", label: "LAST SEEN", tip: "Sort by when you last met" },
  ];

  const empty = $derived(list !== null && views.length === 0);

  /**
   * The card is the scroll host only once the rows are in, as History's list: Back from a scoreboard restores the old
   * position when the host mounts, and the skeleton is too short to hold it.
   */
  const scrollHostAttach = fromAction(scrollHost);
  const loaded = $derived(list !== null);
</script>

<div class="page">
  <div class="head">
    <div class="titles">
      <h1 class="title">Saved players</h1>
      <span class="sum">{list === null ? "" : summary(views.length)}</span>
    </div>
    <div class="grow"></div>
    <label class="search">
      <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" aria-hidden="true"
        ><circle cx="7" cy="7" r="5"></circle><path d="M11 11l3.5 3.5"></path></svg
      >
      <input bind:value={query} placeholder="Search name, tag or note" aria-label="Search saved players" />
    </label>
  </div>

  {#if empty}
    <div class="empty">
      <div class="ebox">
        <div class="eicon">
          <svg width="20" height="24" viewBox="0 0 10 12" fill="none" stroke="currentColor" stroke-width=".9" stroke-linejoin="round" aria-hidden="true"
            ><path d="M1 1h8v10L5 8.2 1 11z"></path></svg
          >
        </div>
        <span class="etitle">No saved players yet</span>
        <span class="etext">Bookmark someone from a live match, a profile or a scoreboard. Next time they’re in your lobby, they’re flagged with your note.</span>
        <button type="button" class="golive" onclick={() => go("live")}>Go to live match</button>
      </div>
    </div>
  {:else}
    <div class="pills">
      {#each FILTERS as f (f.k)}
        <button type="button" class={["pill", filter === f.k && "on"]} aria-pressed={filter === f.k} onclick={() => (filter = f.k)}
          >{f.label}<span class="pcnt">{list === null ? "" : n[f.k]}</span></button
        >
      {/each}
    </div>
    <div class="card" {@attach loaded && scrollHostAttach} aria-busy={list === null}>
      <div class="cols" role="row">
        {#each COLS as c (c.label)}
          {@const st = c.k ? sortState(sort, c.k) : { aria: "none" as const, arrow: "" }}
          <div class="ch" role="columnheader" aria-sort={c.k ? st.aria : undefined}>
            <button
              type="button"
              class={["col", c.k && sort.key === c.k && "on"]}
              disabled={!c.k}
              use:tip={c.tip}
              onclick={() => c.k && (sort = nextSort(sort, c.k))}>{c.label}<span class="arrow">{st.arrow}</span></button
            >
          </div>
        {/each}
        <span></span>
      </div>
      {#if list === null}
        {#each [0, 1, 2, 3, 4] as i (i)}
          <div class="skrow" aria-hidden="true">
            <div class="skp"><Skel w={40} h={40} r={8} block /><Skel w={120} h={14} block /></div>
            <Skel w={110} h={14} block />
            <Skel w={90} h={14} block />
            <Skel w={90} h={14} block />
            <Skel w={180} h={14} block />
            <Skel w={60} h={14} block />
          </div>
        {/each}
      {:else}
        {#each shown as v (v.s.puuid)}
          <SavedRow
            {v}
            editing={editing === v.s.puuid}
            expanded={!!expanded[v.s.puuid]}
            onedit={(on) => (editing = on ? v.s.puuid : null)}
            ontoggle={() => (expanded = { ...expanded, [v.s.puuid]: !expanded[v.s.puuid] })}
          />
        {/each}
        {#if shown.length === 0}<div class="nores">{noResultsText(query)}</div>{/if}
      {/if}
    </div>
  {/if}
</div>

<style>
  .page {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    padding: 18px 24px 18px;
    gap: 14px;
  }

  .head {
    display: flex;
    align-items: flex-end;
    gap: 16px;
    flex: none;
  }

  .titles {
    display: flex;
    flex-direction: column;
    gap: 9px;
  }

  .title {
    margin: 0;
    font: 700 32px/1 var(--font-display);
    letter-spacing: 0.03em;
    text-transform: uppercase;
  }

  .sum {
    font: 500 13px/1 var(--font);
    color: var(--text-muted);
    min-height: 13px;
  }

  .grow {
    flex: 1;
  }

  .search {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 34px;
    width: 270px;
    padding: 0 4px 0 12px;
    border-radius: 8px;
    background: var(--surface);
    border: 1px solid var(--line-control);
    color: var(--text-label);
  }

  .search svg {
    flex: none;
  }

  .search input {
    flex: 1;
    min-width: 0;
    height: 28px;
    background: transparent;
    border: 1px solid transparent;
    border-radius: 5px;
    padding: 0 6px;
    color: var(--text);
    font: 500 13.5px/1 var(--font);
  }

  .pills {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: none;
  }

  .pill {
    height: 30px;
    padding: 0 12px;
    border-radius: 15px;
    display: flex;
    align-items: center;
    gap: 7px;
    cursor: pointer;
    background: var(--surface);
    border: 1px solid var(--line-control);
    color: var(--text-2);
    font: 600 13px/1 var(--font);
  }

  .pill:hover {
    color: var(--text);
  }

  .pill.on {
    background: rgba(157, 140, 255, 0.14);
    border-color: rgba(157, 140, 255, 0.5);
    color: #e2ddff;
  }

  .pcnt {
    font: 600 12px/1 var(--font);
    color: var(--text-label);
  }

  .pill.on .pcnt {
    color: #c9c2ff;
  }

  .card {
    flex: 1;
    min-height: 0;
    overflow: auto;
    background: var(--card);
    border: 1px solid var(--line);
    border-radius: 10px;
  }

  .cols,
  .skrow {
    display: grid;
    grid-template-columns: minmax(0, 2fr) minmax(0, 1.05fr) 128px 128px minmax(0, 2.3fr) 96px 12px;
    column-gap: 16px;
    align-items: center;
  }

  .cols {
    position: sticky;
    top: 0;
    z-index: 1;
    height: 40px;
    padding: 0 14px 0 16px;
    background: var(--card-head);
    border-bottom: 1px solid var(--line-media);
  }

  .ch {
    justify-self: start;
    display: flex;
  }

  .col {
    height: 28px;
    padding: 0 6px;
    margin-left: -6px;
    border-radius: 5px;
    display: flex;
    align-items: center;
    gap: 6px;
    background: transparent;
    border: none;
    cursor: pointer;
    font: 500 10.5px/1 var(--font-mono);
    letter-spacing: 0.06em;
    color: var(--text-muted);
    white-space: nowrap;
  }

  .col:hover {
    background: var(--chip-hover);
  }

  .col:disabled {
    cursor: default;
  }

  .col.on {
    color: var(--text);
  }

  .arrow {
    font: 700 10px/1 var(--font);
    color: var(--accent-text);
  }

  .skrow {
    padding: 12px 14px 12px 16px;
    border-bottom: 1px solid var(--line-soft);
  }

  .skp {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .nores {
    padding: 32px 20px;
    text-align: center;
    font: 500 14px/1.5 var(--font);
    color: var(--text-muted);
  }

  .empty {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    border: 1px dashed var(--line-control);
    border-radius: 12px;
  }

  .ebox {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 16px;
    max-width: 400px;
    text-align: center;
  }

  .eicon {
    width: 56px;
    height: 56px;
    border-radius: 14px;
    border: 1px solid var(--line-control);
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-muted);
  }

  .etitle {
    font: 700 24px/1 var(--font-display);
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }

  .etext {
    font: 500 14px/1.55 var(--font);
    color: var(--text-dim);
    text-wrap: pretty;
  }

  .golive {
    height: 34px;
    padding: 0 16px;
    border-radius: 8px;
    background: var(--button);
    color: var(--text);
    border: 1px solid var(--line-control);
    font: 600 13px/1 var(--font);
    cursor: pointer;
  }

  .golive:hover {
    background: var(--button-hover);
    border-color: var(--line-hover);
  }
</style>
