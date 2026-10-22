<!--
  Connection lost (design v2 TrackerApp.dc.html 144–152): the last data stays on screen under this banner. The age of
  that data ticks every second while the banner shows.
-->
<script lang="ts">
  import { retryNow } from "../../lib/api";
  import { ageShort } from "../../lib/format";
  import type { AppStatus } from "../../lib/types";
  import { reportError } from "../../lib/uilog";
  import Dot from "../shell/Dot.svelte";

  let { status }: { status: AppStatus } = $props();

  let nowMs = $state(Date.now());
  $effect(() => {
    nowMs = Date.now();
    const id = setInterval(() => (nowMs = Date.now()), 1000);
    return () => clearInterval(id);
  });

  const age = $derived(status.lastDataMs === null ? null : ageShort(nowMs - status.lastDataMs));
  const attempt = $derived(status.attempt > 0 ? `Reconnecting, attempt ${status.attempt} of ${status.maxAttempts}.` : "Reconnecting.");

  function retry() {
    retryNow().catch((err: unknown) => reportError("retry_now", err));
  }
</script>

<div class="lost" role="status">
  <Dot color="var(--warn)" pulse />
  <span class="head">Lost connection to the Riot Client</span>
  <span class="text" data-ellipsis>{age === null ? "" : `Showing data from ${age} ago. `}{attempt}</span>
  <div class="fill"></div>
  <button type="button" onclick={retry}>Retry now</button>
</div>

<style>
  .lost {
    flex: none;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 0 8px 0 14px;
    height: 40px;
    border-radius: 8px;
    background: var(--warn-faint);
    border: 1px solid rgba(255, 154, 60, 0.32);
  }

  .head {
    font: 600 13px/1 var(--font);
    color: var(--lost-text);
    white-space: nowrap;
  }

  .text {
    font: 500 13px/1 var(--font);
    color: var(--text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }

  .fill {
    flex: 1;
  }

  button {
    flex: none;
    height: 28px;
    padding: 0 12px;
    border-radius: 6px;
    background: transparent;
    color: var(--lost-text);
    border: 1px solid rgba(255, 154, 60, 0.4);
    font: 600 12.5px/1 var(--font);
    cursor: pointer;
  }

  button:hover {
    background: rgba(255, 154, 60, 0.12);
  }
</style>
