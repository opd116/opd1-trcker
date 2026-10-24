<!--
  Settings (design v2, TrackerApp.dc.html 918–948, words in `extraVals4` 2012–2038): four section cards in a 780px
  column. Every control writes through `updateSettings` (the store changes at once; Rust stores the copy and applies
  keep-on-top / autostart / tray). The title bar's pin writes the same `onTop`, so the two always agree.
-->
<script lang="ts">
  import OptionDetail from "../components/settings/OptionDetail.svelte";
  import Segmented from "../components/settings/Segmented.svelte";
  import SettingRow from "../components/settings/SettingRow.svelte";
  import SettingsGroup from "../components/settings/SettingsGroup.svelte";
  import Toggle from "../components/settings/Toggle.svelte";
  import { disabledReason, GROUPS, PAGE, SMURF, TOGGLES } from "../components/settings/copy";
  import { settings, updateSettings } from "../lib/appdata.svelte";
  import { scrollHost } from "../lib/router.svelte";
  import type { SmurfLevel } from "../lib/types";

  const smurfLabel = $derived(SMURF.options.find((o) => o.value === $settings.smurf)?.label ?? "");
</script>

<div class="page" use:scrollHost>
  <div class="col">
    <div class="head">
      <h1 class="title">{PAGE.title}</h1>
      <span class="sub">{PAGE.subtitle}</span>
    </div>
    {#each GROUPS as g (g.title)}
      <SettingsGroup title={g.title} desc={g.desc}>
        {#each g.rows as r (r.kind === "toggle" ? r.key : r.kind)}
          {#if r.kind === "toggle"}
            {@const why = disabledReason(r.key, $settings)}
            <SettingRow id="opd-set-{r.key}" label={TOGGLES[r.key].label} desc={TOGGLES[r.key].desc} disabled={why !== null}>
              {#snippet control()}
                <Toggle
                  on={$settings[r.key]}
                  labelledby="opd-set-{r.key}"
                  disabled={why !== null}
                  tip={why ?? ""}
                  onchange={(on) => updateSettings({ [r.key]: on })}
                />
              {/snippet}
            </SettingRow>
          {:else}
            <SettingRow id="opd-set-smurf" label={SMURF.label} desc={SMURF.desc}>
              {#snippet control()}
                <Segmented
                  options={SMURF.options}
                  value={$settings.smurf}
                  labelledby="opd-set-smurf"
                  onchange={(v: SmurfLevel) => updateSettings({ smurf: v })}
                />
              {/snippet}
              {#snippet detail()}
                <OptionDetail rows={SMURF.options.map((o) => ({ name: o.label, desc: o.desc }))} selected={smurfLabel} />
              {/snippet}
            </SettingRow>
          {/if}
        {/each}
      </SettingsGroup>
    {/each}
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
</style>
