<!--
  The Riot Client can't be reached (design v2 TrackerApp.dc.html 351–371): what happened, the code and the lockfile
  path, "Retry now", and the countdown to the next automatic retry (`retryInMs` counts from when the status arrived).
-->
<script lang="ts">
  import { retryNow } from "../../lib/api";
  import { retryCountFromNow, statusArrivedAt } from "../../lib/appdata.svelte";
  import { shortLockfilePath } from "../../lib/format";
  import { tip } from "../../lib/tooltip.svelte";
  import type { AppStatus } from "../../lib/types";
  import { reportError } from "../../lib/uilog";

  let { status }: { status: AppStatus } = $props();

  let nowMs = $state(Date.now());
  /** Bumped by "Retry now", which restarts the count. */
  let retried = $state(0);
  /**
   * When the next automatic retry happens: the status's `retryInMs`, counted from when that status arrived or "Retry
   * now" was clicked (not from this view's mount: coming back to Live must not restart the count).
   */
  const deadline = $derived.by(() => {
    void retried;
    return status.retryInMs === null ? null : statusArrivedAt() + status.retryInMs;
  });
  $effect(() => {
    if (deadline === null) return;
    nowMs = Date.now();
    const id = setInterval(() => (nowMs = Date.now()), 1000);
    return () => clearInterval(id);
  });

  /**
   * Rust keeps retrying every `retryInMs` while the status stays the same, and an unchanged status is not sent again,
   * so once the deadline passes the countdown starts over on the same period instead of stopping at 0.
   */
  const secondsLeft = $derived.by(() => {
    const period = status.retryInMs;
    if (deadline === null || period === null || period <= 0) return null;
    const left = deadline - nowMs;
    return Math.ceil((left > 0 ? left : period - (-left % period)) / 1000);
  });
  const lockfile = $derived(status.errorCode !== "CLIENT_UNREACHABLE");
  /** The design's `%LOCALAPPDATA%\…\lockfile`; the full path is the tooltip. */
  const shortPath = $derived(shortLockfilePath(status.lockfilePath));
  /** `21:41:07`, local time. */
  const at = $derived.by(() => {
    if (status.errorAtMs === null) return null;
    const d = new Date(status.errorAtMs);
    return [d.getHours(), d.getMinutes(), d.getSeconds()].map((n) => String(n).padStart(2, "0")).join(":");
  });

  function retry() {
    retryCountFromNow();
    retried++;
    retryNow().catch((err: unknown) => reportError("retry_now", err));
  }
</script>

<div class="err">
  <div class="box" role="alert">
    <div class="icon">
      <svg width="22" height="22" viewBox="0 0 22 22" fill="none" stroke="var(--loss)" stroke-width="1.8" stroke-linecap="round" aria-hidden="true"
        ><path d="M4 7a10 10 0 0 1 14 0M7 10.5a5.5 5.5 0 0 1 8 0M3 3l16 16"></path><circle cx="11" cy="15.5" r="1.2" fill="var(--loss)" stroke="none"></circle></svg
      >
    </div>
    <div class="copy">
      <h1>Can’t reach the Riot Client</h1>
      {#if lockfile}
        <p>
          OPD1 reads match data from the Riot Client running on this PC. Its lockfile wasn’t found, so the client may still be
          starting, or it’s installed in a different folder.
        </p>
      {:else}
        <p>OPD1 reads match data from the Riot Client running on this PC. The client was found but didn’t answer, so it may still be starting.</p>
      {/if}
    </div>
    <div class="code">
      <div><span class="c">{status.errorCode ?? "CLIENT_UNREACHABLE"}</span>{at ? ` · ${at}` : ""}</div>
      <div class="path" use:tip={shortPath === status.lockfilePath ? "" : status.lockfilePath}>{shortPath}</div>
    </div>
    <div class="actions">
      <button type="button" class="primary" onclick={retry}>Retry now</button>
      <!-- The design's "Choose Riot Client folder…" button is left out: it needs the dialog plugin, which is not approved
           (Tauri plugin = a new dependency). Add it back with a `choose_riot_folder` command if that changes. -->
      <div class="fill"></div>
      <!-- Outside the alert's speech: it changes every second, and the alert would be read out again each time. -->
      {#if secondsLeft !== null}<span class="auto" aria-hidden="true">AUTO-RETRY IN {secondsLeft}S</span>{/if}
    </div>
  </div>
</div>

<style>
  .err {
    flex: 1;
    min-height: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--card);
    border: 1px solid var(--line);
    border-radius: 12px;
  }

  .box {
    width: 100%;
    max-width: 580px;
    display: flex;
    flex-direction: column;
    gap: 20px;
    padding: 24px;
  }

  .icon {
    width: 48px;
    height: 48px;
    border-radius: 12px;
    background: rgba(239, 106, 91, 0.1);
    border: 1px solid rgba(239, 106, 91, 0.35);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .copy {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  h1 {
    margin: 0;
    font: 700 32px/1.05 var(--font-display);
    letter-spacing: 0.03em;
    text-transform: uppercase;
  }

  p {
    margin: 0;
    font: 500 15px/1.55 var(--font);
    color: var(--text-muted);
    text-wrap: pretty;
  }

  .code {
    font: 500 11.5px/1.7 var(--font-mono);
    color: var(--text-dim);
    background: var(--input);
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 12px 14px;
    overflow-wrap: anywhere;
  }

  .c {
    color: var(--loss);
  }

  .path {
    color: var(--text-muted);
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .primary {
    height: 36px;
    padding: 0 18px;
    border-radius: 8px;
    background: var(--accent);
    color: var(--on-accent);
    border: none;
    font: 600 13.5px/1 var(--font);
    cursor: pointer;
  }

  .primary:hover {
    background: var(--accent-hover);
  }

  .fill {
    flex: 1;
  }

  .auto {
    font: 500 10.5px/1 var(--font-mono);
    letter-spacing: 0.06em;
    color: var(--text-muted);
  }
</style>
