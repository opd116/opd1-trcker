<!--
  A record's W/L counts, typed (never parsed from a string): `41W – 24L`. A positive side takes the profile win/loss
  token; a zero side stays neutral; a missing/unknown record is `—`. The accessible label spells wins and losses out,
  so meaning never rests on colour or order alone (approved prototype `RecordValue.svelte`).
-->
<script lang="ts">
  import type { RecordCounts } from "../../lib/profile";
  let { record, compact = false }: { record: RecordCounts | null; compact?: boolean } = $props();
</script>

<span class="record" class:compact role="img" aria-label={record ? `${record.wins} wins, ${record.losses} losses` : "Record unknown"}>
  {#if record}<span aria-hidden="true"><span class:win={record.wins > 0}>{record.wins}<small>W</small></span><span class="separator"> – </span><span
        class:loss={record.losses > 0}>{record.losses}<small>L</small></span></span>{:else}—{/if}
</span>

<style>
  .record {
    font: 600 20px/26px var(--font);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
    color: var(--text-mid);
  }

  .record.compact {
    font: 500 13px/16px var(--font);
  }

  small {
    font-size: 12px;
    font-weight: 500;
  }

  .compact small {
    font-size: 11.5px;
  }

  .win {
    color: var(--win);
  }

  .loss {
    color: var(--loss);
  }

  .separator {
    margin: 0 3px;
  }
</style>
