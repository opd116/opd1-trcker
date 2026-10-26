<!--
  The connection pill in the title bar (design v2 103–107, states 2055–2060). `dotOnly` draws just its dot (glance
  mode's title bar, 34), with the same tooltip. Nothing until the first status answer arrives.
-->
<script lang="ts">
  import { appStatus, statusArrivedAt } from "../../lib/appdata.svelte";
  import { live } from "../../lib/stores";
  import { tip } from "../../lib/tooltip.svelte";
  import Dot from "./Dot.svelte";
  import { pillView } from "./statusPill";

  let { dotOnly = false }: { dotOnly?: boolean } = $props();

  /** The tooltip's "N s ago" is read at hover time, so a 1 s clock keeps it honest while one could show. */
  let nowMs = $state(Date.now());
  const ticking = $derived($appStatus?.conn === "ok" || $appStatus?.conn === "lost");
  $effect(() => {
    if (!ticking) return;
    nowMs = Date.now();
    const id = setInterval(() => (nowMs = Date.now()), 1000);
    return () => clearInterval(id);
  });

  /** When the current status arrived (its retry wait counts from then, across remounts too). */
  const arrivedMs = $derived.by(() => {
    void $appStatus;
    return statusArrivedAt();
  });
  const view = $derived($appStatus ? pillView($appStatus, $live?.stage ?? null, nowMs, arrivedMs) : null);
</script>

{#if view}
  {#if dotOnly}
    <span class="dot-only" use:tip={view.tip} aria-label={view.label} role="img"><Dot color={view.dot} pulse={view.pulse} /></span>
  {:else}
    <div class="pill" data-conn={view.conn} style:background={view.bg} style:border-color={view.bd} use:tip={view.tip}>
      <Dot color={view.dot} pulse={view.pulse} />
      <span class="label" style:color={view.color}>{view.label}</span>
      {#if view.detail}<span class="detail">{view.detail}</span>{/if}
    </div>
  {/if}
{/if}

<style>
  .pill {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 22px;
    cursor: default;
    padding: 0 10px;
    border-radius: 11px;
    border: 1px solid;
  }

  .label {
    font: 600 12px/1 var(--font);
    white-space: nowrap;
  }

  .detail {
    font: 500 10.5px/1 var(--font-mono);
    letter-spacing: 0.08em;
    color: var(--text-muted);
    white-space: nowrap;
  }

  .dot-only {
    display: inline-flex;
    align-items: center;
  }
</style>
