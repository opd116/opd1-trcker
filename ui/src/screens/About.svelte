<!--
  About (design v2: only the data, `aboutInfo` at TrackerApp.dc.html 2040; no markup). Built from the design's own
  parts: the Settings page's title and 780px column, its section card, its row grid and hairlines, the title bar's
  wordmark, and the uppercase Geist Mono micro-labels. Values come from `app_info` and the connection status; a
  missing one reads "—", and the rows are neutral skeleton bars until `app_info` answers.
-->
<script lang="ts">
  import Dot from "../components/shell/Dot.svelte";
  import SettingsGroup from "../components/settings/SettingsGroup.svelte";
  import { aboutRows } from "../components/settings/about";
  import * as api from "../lib/api";
  import { appStatus } from "../lib/appdata.svelte";
  import { scrollHost } from "../lib/router.svelte";
  import type { AppInfo, ConnState } from "../lib/types";
  import { reportError } from "../lib/uilog";

  let info = $state<AppInfo | null>(null);
  let settled = $state(false);

  $effect(() => {
    api
      .appInfo()
      .then((i) => (info = i))
      .catch((err) => reportError("app_info", err))
      .finally(() => (settled = true));
  });

  const rows = $derived(aboutRows(info, $appStatus));
  const DOT: Record<ConnState, string> = { ok: "var(--win)", waiting: "var(--win)", lost: "var(--warn)", error: "var(--loss)" };
  /** Rows that need `app_info` show a skeleton until it answers; the static credit never waits. */
  const waits = (label: string) => !settled && label !== "GAME DATA" && label !== "RIOT CLIENT";
</script>

<div class="page" use:scrollHost>
  <div class="col">
    <div class="head">
      <h1 class="title">About</h1>
      <span class="sub">The app, where its data comes from, and its licences.</span>
    </div>

    <section class="card" aria-label="OPD1 TRCKER">
      <div class="brand">
        <div class="logo" aria-hidden="true"></div>
        <span class="word">OPD1</span>
        <span class="mark">TRCKER</span>
      </div>
      <dl class="info" aria-busy={!settled}>
        {#each rows as r (r.label)}
          <div class="row">
            <dt class="label">{r.label}</dt>
            <dd class="value">
              {#if waits(r.label)}
                <span class="skel" aria-hidden="true"></span>
              {:else}
                {#if r.conn}<Dot color={DOT[r.conn]} />{/if}{r.value}
              {/if}
            </dd>
          </div>
        {/each}
      </dl>
    </section>

    <SettingsGroup title="Credits">
      <div class="credits">
        <p class="credit">
        OPD1 TRCKER is proprietary software. Game data and images come from valorant-api.com. Not affiliated with or
        endorsed by Riot Games.
      </p>
        <p class="credit">Fonts: Barlow, Saira Condensed and Geist Mono, under the SIL Open Font License 1.1.</p>
      </div>
    </SettingsGroup>
  </div>
</div>

<style>
  .page {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 18px 24px 28px;
  }

  .col {
    max-width: 780px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .head {
    display: flex;
    flex-direction: column;
    gap: 9px;
    margin-bottom: 2px;
  }

  .title {
    margin: 0;
    font: 700 32px/1 var(--font-display);
    letter-spacing: 0.03em;
    text-transform: uppercase;
    color: var(--text);
  }

  .sub {
    font: 500 13px/1 var(--font);
    color: var(--text-muted);
  }

  .card {
    background: var(--card);
    border: 1px solid var(--line);
    border-radius: 12px;
    padding: 4px 20px;
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 18px 0 16px;
    border-bottom: 1px solid var(--line);
  }

  .logo {
    width: 20px;
    height: 20px;
    background: var(--accent);
    clip-path: polygon(0 0, 100% 0, 100% 64%, 64% 100%, 0 100%);
  }

  .word {
    font: 700 24px/1 var(--font-display);
    letter-spacing: 0.14em;
    color: var(--text);
  }

  .mark {
    font: 500 12px/1 var(--font-mono);
    letter-spacing: 0.06em;
    color: var(--text-muted);
  }

  .info {
    margin: 0;
  }

  .row {
    display: grid;
    grid-template-columns: 160px minmax(0, 1fr);
    column-gap: 24px;
    align-items: center;
    min-height: 46px;
    padding: 14px 0;
  }

  .row:not(:last-child) {
    border-bottom: 1px solid var(--line);
  }

  .label {
    font: 500 10.5px/1 var(--font-mono);
    letter-spacing: 0.06em;
    color: var(--text-label);
  }

  .value {
    margin: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    font: 600 14.5px/1.2 var(--font);
    color: var(--text);
    font-variant-numeric: tabular-nums;
  }

  .skel {
    width: 140px;
    height: 12px;
    border-radius: 4px;
    background: var(--skel);
  }

  .credits {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 8px 0 16px;
  }

  .credit {
    margin: 0;
    font: 500 13px/1.45 var(--font);
    color: var(--text-3);
    text-wrap: pretty;
  }
</style>
