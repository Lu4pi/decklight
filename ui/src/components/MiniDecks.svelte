<script lang="ts">
  // Leiste unten: alle Player kompakt mit Titel, Zeit, Tempo und Übersicht.
  import { conn } from "../lib/conn.svelte";
  import { elapsed, pitch, stateIcon, titleLine, tone } from "../lib/deck";
  import { live } from "../lib/live.svelte";
  import Waveform from "./Waveform.svelte";
  import SegText from "./SegText.svelte";

  const decks = $derived((conn.state?.decks ?? []).filter((d) => d.number <= 6));
  $effect(() => live.ensure(decks));
</script>

<div class="mini" style:--n={Math.max(decks.length, 1)}>
  {#each decks as d (d.number)}
    <div class="m" class:master={d.master}>
      <div class="titlebar">♪ {titleLine(d)}</div>
      <div class="row">
        <div class="box" class:red={d.master} class:air={d.on_air}>
          <span>Deck</span><b>{d.number}</b>
        </div>
        <div class="time {tone(d)}">
          <span class="cap">Time</span>
          <span class="t"><SegText text={elapsed(d.position_ms)} /></span>
        </div>
        <div class="right">
          <span class="bpm">{d.effective_bpm?.toFixed(2) ?? "--"}</span>
          <span class="p">{pitch(d)}</span>
        </div>
        <span class="ico">{stateIcon(d)}</span>
      </div>
      <Waveform track={live.of(d)} position={() => conn.positionOf(d.number)} mode="overview" height={18} />
    </div>
  {/each}
</div>

<style>
  .mini {
    display: grid;
    grid-template-columns: repeat(var(--n), minmax(0, 1fr));
    gap: 4px;
    padding: 4px;
    background: var(--surface);
    border-top: 1px solid var(--border);
  }
  @media (max-width: 900px) {
    .mini {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }
  /* Auf dem Handy zeigt /monitor die Player; hier wäre die Leiste nur im Weg. */
  @media (max-width: 699px) {
    .mini {
      display: none;
    }
  }
  .m {
    background: #050507;
    border: 1px solid var(--border);
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .m.master {
    border-color: var(--onair);
  }
  .titlebar {
    font-size: 10px;
    padding: 2px 6px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 3px 4px;
  }
  .box {
    border: 1px solid #fff;
    display: flex;
    flex-direction: column;
    align-items: center;
    line-height: 1;
    padding: 2px 4px;
    min-width: 30px;
  }
  .box span {
    font-size: 7px;
    text-transform: uppercase;
  }
  .box b {
    font-size: 16px;
  }
  .box.red {
    background: var(--onair);
    border-color: var(--onair);
  }
  .box.air:not(.red) {
    border-color: var(--onair);
    color: var(--onair);
  }
  .time {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
  }
  .time .cap {
    font-size: 8px;
  }
  .t {
    font-family: var(--seg);
    font-size: 15px;
    color: var(--tc-stop);
  }
  .run .t {
    color: var(--tc-run);
  }
  .loop .t {
    color: var(--tc-loop);
  }
  .off .t {
    color: var(--tc-off);
  }
  .right {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    font-family: var(--mono);
    line-height: 1.15;
  }
  .bpm {
    font-size: 13px;
    font-weight: 700;
  }
  .p {
    font-size: 10px;
    color: var(--text-dim);
  }
  .ico {
    width: 14px;
    text-align: center;
    font-size: 11px;
  }
</style>
