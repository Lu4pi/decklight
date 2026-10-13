<script lang="ts">
  // Kopf der Cue-Liste: Master-Track mit aktuellem und nächstem Cue groß, dazu Countdown.
  import { conn } from "../lib/conn.svelte";
  import { cueState, elapsed, stateIcon, titleLine, trackLength } from "../lib/deck";
  import { live } from "../lib/live.svelte";
  import { rgb } from "../lib/format";
  import SegClock from "./SegClock.svelte";
  import Waveform from "./Waveform.svelte";
  import SegText from "./SegText.svelte";

  const s = $derived(conn.state);
  const decks = $derived((s?.decks ?? []).filter((d) => d.number <= 6));
  const m = $derived(decks.find((d) => d.master));
  $effect(() => live.ensure(decks));
  const cues = $derived(m?.info && conn.show ? (conn.show.triggers.tracks[m.info.key] ?? []) : []);
  const cs = $derived(cueState(live.of(m), cues, m?.position_ms ?? null));
  const secs = $derived(cs.untilNext != null ? Math.max(0, Math.ceil(cs.untilNext / 1000)) : null);
  const len = $derived(m ? trackLength(m) : null);
</script>

<div class="head">
  <div class="left">
    <div class="line">
      <div class="num" class:red={!!m}><span>SMPTE</span><b>M</b></div>
      <div class="ttl">{m ? titleLine(m) : "Kein Master"}</div>
      <span class="ico">{stateIcon(m)}</span>
    </div>
    <div class="w">
      {#if m}
        <Waveform
          track={live.of(m)}
          position={() => conn.positionOf(m.number)}
          mode="overview"
          {cues}
          rekordboxCues={live.of(m)?.detail.rekordbox_cues ?? []}
          height={56}
        />
      {/if}
    </div>
    <div class="big">
      <div>
        <span class="cap">Active</span>
        <b class="cur" style:color={cs.current ? rgb(cs.current.color, "var(--tc-run)") : undefined}>{cs.current?.name ?? "—"}</b>
      </div>
      <div>
        <span class="cap">Next</span>
        <b class="nxt">{cs.next?.name ?? "—"}</b>
      </div>
    </div>
  </div>
  <div class="right">
    <SegClock text={s?.timecode.enabled ? s.timecode.text : null} tone={s?.timecode.running ? "run" : "stop"} size={26} />
    <div class="times">
      <div><span class="cap">Elapsed</span><b><SegText text={elapsed(m?.position_ms)} /></b></div>
      <div><span class="cap">Remain</span><b><SegText text={elapsed(len != null && m?.position_ms != null ? len - m.position_ms : null)} /></b></div>
    </div>
    <div class="count">
      <span class="cap">Cue countdown</span>
      <b class:soon={secs != null && secs <= 5}><SegText text={secs == null ? "--" : String(secs).padStart(2, " ")} /></b>
      <span class="cap">Seconds</span>
    </div>
  </div>
</div>

<style>
  .head {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 10px;
    background: var(--surface);
    border: 1px solid var(--border);
    padding: 6px;
    margin-bottom: 8px;
  }
  @media (max-width: 900px) {
    .head {
      grid-template-columns: minmax(0, 1fr);
    }
  }
  .left {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .line {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .num {
    border: 1px solid #fff;
    display: flex;
    flex-direction: column;
    align-items: center;
    line-height: 1;
    padding: 2px 6px;
  }
  .num span {
    font-size: 8px;
  }
  .num b {
    font-size: 22px;
  }
  .num.red {
    background: var(--onair);
    border-color: var(--onair);
  }
  .ttl {
    flex: 1;
    min-width: 0;
    font-size: 20px;
    font-weight: 700;
    text-transform: uppercase;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .w {
    background: #050507;
    min-height: 56px;
  }
  .big {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .big div {
    display: flex;
    align-items: baseline;
    gap: 10px;
    min-width: 0;
  }
  .big .cap {
    width: 46px;
    flex: none;
  }
  .big b {
    font-size: 28px;
    text-transform: uppercase;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .cur {
    color: var(--tc-run);
  }
  .nxt {
    color: var(--tc-loop);
  }
  .right {
    display: flex;
    flex-direction: column;
    gap: 8px;
    align-items: flex-end;
  }
  .times {
    display: flex;
    flex-direction: column;
    gap: 4px;
    align-items: flex-end;
  }
  .times div {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
  }
  .times b {
    font-family: var(--seg);
    font-size: 20px;
    background: #050505;
    border: 1px solid var(--border);
    padding: 3px 6px;
  }
  .count {
    display: flex;
    flex-direction: column;
    align-items: center;
    border: 1px solid var(--border);
    padding: 4px 14px;
    background: #050505;
  }
  .count b {
    font-family: var(--seg);
    font-size: 48px;
    color: #4f7dff;
    line-height: 1.1;
  }
  .count b.soon {
    color: var(--onair);
  }
</style>
