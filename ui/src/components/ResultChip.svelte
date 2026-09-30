<!--
  A W / L / D result chip (spec §1.5): 20px in the recent and encounter rows and the team headers (§6.4, §6.6, §6.10),
  28px in the Match header. Always the plain boxes, never the last-5 newest variant.
-->
<script lang="ts">
  import { boxStyle } from "../lib/resultBox";
  import type { Result } from "../lib/types";

  let { result, size }: { result: Result; size: 20 | 28 } = $props();

  const KEY = { win: "W", loss: "L", draw: "D" } as const;
  const key = $derived(KEY[result]);
</script>

<div class={["chip", `s${size}`]} style={boxStyle(key)}>{key}</div>

<style>
  .chip {
    flex: none;
    box-sizing: border-box;
    display: flex;
    align-items: center;
    justify-content: center;
    font-weight: 700;
  }

  .s20 {
    width: 20px;
    height: 20px;
    border-radius: 4px;
    font-size: 12px;
  }

  .s28 {
    width: 28px;
    height: 28px;
    border-radius: 5px;
    font-size: 14px;
  }
</style>
