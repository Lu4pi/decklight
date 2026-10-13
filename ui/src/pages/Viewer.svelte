<script lang="ts">
  // Viewer: Deck-Köpfe oben, darunter die Detail-Wellenformen aller Decks übereinander.
  import { conn } from "../lib/conn.svelte";
  import { elapsed, pitch, stateIcon, tone, trackLength } from "../lib/deck";
  import { live } from "../lib/live.svelte";
  import Waveform from "../components/Waveform.svelte";
  import SegText from "../components/SegText.svelte";

  const decks = $derived((conn.state?.decks ?? []).filter((d) => d.number <= 6));
  $effect(() => live.ensure(decks));
  const cuesOf = (key: string | undefined) => (key && conn.show ? (conn.show.triggers.tracks[key] ?? []) : []);
  let windowSec = $state(8);
</script>

<div class="viewer">
  <div class="heads">
    {#each decks as d (d.number)}
      {@const len = trackLength(d)}
      <div class="head" class:master={d.master}>
        <div class="top">
          <div class="num" class:red={d.master}><span>Deck</span><b>{d.number}</b></div>
          <div class="title">
            <span class="cap">Track title</span>
            <b>{d.info?.title ?? "Kein Track"}</b>
          </div>
          <div class="time {tone(d)}">
            <SegText text={elapsed(d.position_ms)} />
            <small><SegText text={elapsed(len != null && d.position_ms != null ? len - d.position_ms : null)} /></small>
          </div>
          <div class="bpm"><span>{d.effective_bpm?.toFixed(2) ?? "--"}</span><small>BPM</small></div>
        </div>
        <div class="ov">
          <span class="ico">{stateIcon(d)}</span>
          <div class="w">
            <Waveform
              track={live.of(d)}
              position={() => conn.positionOf(d.number)}
              mode="overview"
              cues={cuesOf(d.info?.key)}
              rekordboxCues={live.of(d)?.detail.rekordbox_cues ?? []}
              height={26}
            />
          </div>
        </div>
      </div>
    {/each}
  </div>

  <div class="zoom">
    <span class="cap">Ausschnitt</span>
    {#each [4, 8, 16] as w}
      <button class:on={windowSec === w} onclick={() => (windowSec = w)}>{w} s</button>
    {/each}
  </div>

  <div class="stack">
    {#each decks as d (d.number)}
      <div class="lane" class:master={d.master}>
        <div class="lbl" class:red={d.master}><span>Deck</span><b>{d.number}</b><i>{stateIcon(d)}</i></div>
        <div class="w">
          <Waveform
            track={live.of(d)}
            position={() => conn.positionOf(d.number)}
            mode="detail"
            {windowSec}
            cues={cuesOf(d.info?.key)}
            rekordboxCues={live.of(d)?.detail.rekordbox_cues ?? []}
            height={84}
          />
        </div>
        <div class="tempo">
          <b>{pitch(d)}</b>
          <span class="cap">Tempo</span>
        </div>
      </div>
    {/each}
  </div>
</div>

<style>
  .viewer {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .heads {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 4px;
  }
  @media (max-width: 800px) {
    .heads {
      grid-template-columns: minmax(0, 1fr);
    }
  }
  .head {
    background: var(--surface);
    border: 1px solid var(--border);
    padding: 4px 6px;
    min-width: 0;
  }
  .head.master {
    border-color: var(--onair);
  }
  .top {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .num,
  .lbl {
    border: 1px solid #fff;
    display: flex;
    flex-direction: column;
    align-items: center;
    line-height: 1;
    padding: 2px 5px;
    flex: none;
  }
  .num span,
  .lbl span {
    font-size: 7px;
    text-transform: uppercase;
  }
  .num b {
    font-size: 18px;
  }
  .red {
    background: var(--onair);
    border-color: var(--onair);
  }
  .title {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .title .cap {
    font-size: 8px;
  }
  .title b {
    font-size: 16px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .time {
    font-family: var(--seg);
    font-size: 15px;
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    color: var(--tc-stop);
  }
  .time small {
    font-size: 10px;
    color: var(--text-dim);
  }
  .time.run {
    color: var(--tc-run);
  }
  .time.loop {
    color: var(--tc-loop);
  }
  .time.off {
    color: var(--tc-off);
  }
  .bpm {
    border: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 1px 6px;
    font-family: var(--mono);
    font-weight: 700;
    line-height: 1.1;
  }
  .bpm small {
    font-size: 8px;
    color: var(--text-dim);
  }
  .ov {
    display: flex;
    gap: 6px;
    align-items: center;
    margin-top: 3px;
  }
  .ico {
    width: 14px;
    font-size: 11px;
    text-align: center;
  }
  .w {
    flex: 1;
    min-width: 0;
    background: #050507;
  }
  .zoom {
    display: flex;
    gap: 4px;
    align-items: center;
    justify-content: flex-end;
  }
  .zoom button {
    font-size: 11px;
    padding: 2px 8px;
  }
  .zoom button.on {
    background: var(--accent);
    border-color: var(--accent);
  }
  .stack {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .lane {
    display: flex;
    gap: 6px;
    align-items: stretch;
    background: var(--surface);
    border: 1px solid var(--border);
    padding: 3px;
  }
  .lane.master {
    border-color: var(--onair);
  }
  .lbl {
    justify-content: center;
    width: 42px;
  }
  .lbl b {
    font-size: 20px;
  }
  .lbl i {
    font-style: normal;
    font-size: 11px;
    margin-top: 4px;
  }
  .tempo {
    width: 70px;
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    justify-content: center;
    font-family: var(--mono);
  }
  .tempo b {
    font-size: 13px;
  }
</style>
