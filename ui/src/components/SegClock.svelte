<script lang="ts">
  // Großer Sieben-Segment-Timecode in vier Feldern (Stunden … Frames).
  import type { Tone } from "../lib/deck";

  let {
    text,
    tone = "stop",
    size = 40,
    labels = true,
  }: { text: string | null; tone?: Tone; size?: number; labels?: boolean } = $props();

  const parts = $derived((text ?? "--:--:--:--").split(/[:;.]/).slice(0, 4));
  const names = ["Hours", "Minutes", "Seconds", "Frames"];
</script>

<div class="seg {tone}" style:--size="{size}px" aria-label="Timecode {text ?? 'aus'}">
  {#each parts as p, i}
    <div class="cell">
      {#if labels}<span class="cap">{names[i]}</span>{/if}
      <span class="digits">
        <span class="ghost" aria-hidden="true">88</span>
        <span class="val">{p.replace(/-/g, "")}</span>
      </span>
    </div>
  {/each}
</div>

<style>
  .seg {
    display: flex;
    gap: calc(var(--size) * 0.14);
    --c: var(--tc-stop);
  }
  .run {
    --c: var(--tc-run);
  }
  .loop {
    --c: var(--tc-loop);
  }
  .off {
    --c: var(--tc-off);
  }
  .cell {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
  }
  .cap {
    font-size: max(8px, calc(var(--size) * 0.2));
  }
  .digits {
    position: relative;
    display: block;
    background: #050505;
    border: 1px solid var(--border);
    padding: calc(var(--size) * 0.1) calc(var(--size) * 0.12);
    font-family: var(--seg);
    font-size: var(--size);
    line-height: 1;
  }
  .ghost {
    color: rgba(255, 255, 255, 0.05);
  }
  .val {
    position: absolute;
    right: calc(var(--size) * 0.12);
    top: calc(var(--size) * 0.1);
    color: var(--c);
    text-shadow: 0 0 calc(var(--size) * 0.25) color-mix(in srgb, var(--c) 45%, transparent);
  }
</style>
