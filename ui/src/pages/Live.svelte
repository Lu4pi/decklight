<script lang="ts">
  // Live: Master-Timecode oben, darunter jedes Deck mit Wellenform und eigenem Track-Timecode.
  import { conn } from "../lib/conn.svelte";
  import { cueState, deckTimecode, pitch, stateIcon, titleLine, tone, type Tone } from "../lib/deck";
  import { live } from "../lib/live.svelte";
  import { clock, time } from "../lib/format";
  import { toast } from "../lib/toast.svelte";
  import { artworkUrl } from "../lib/tracks";
  import type { MasterMode } from "../lib/types";
  import SegClock from "../components/SegClock.svelte";
  import Waveform from "../components/Waveform.svelte";

  const s = $derived(conn.state);
  const decks = $derived((s?.decks ?? []).filter((d) => d.number <= 6));
  const master = $derived(decks.find((d) => d.master));
  $effect(() => live.ensure(decks));

  const cuesOf = (key: string | undefined) => (key && conn.show ? (conn.show.triggers.tracks[key] ?? []) : []);
  const masterCues = $derived(cueState(live.of(master), cuesOf(master?.info?.key), master?.position_ms ?? null));
  const masterTone = $derived<Tone>(
    !s?.timecode.enabled || !s.timecode.text ? "off" : master?.play_state === "looping" ? "loop" : s.timecode.running ? "run" : "stop",
  );
  let width = $state(1200);
  const seg = $derived(width < 700 ? Math.max(18, Math.floor((width - 90) / 13)) : width < 1300 ? 30 : 38);
  const recent = $derived((conn.monitor?.log ?? []).slice(-5).reverse());

  async function setMaster(mode: MasterMode) {
    const r = await conn.command({ cmd: "set_master_mode", mode });
    if (!r.ok) toast(r.error ?? "Fehler", "error");
  }
  const modeIs = (m: string, deck?: number) =>
    s?.master_mode.mode === m && (deck == null || (s.master_mode.mode === "manual" && s.master_mode.deck === deck));
  const offsetLabel = (ms: number | null | undefined) => {
    if (ms == null) return "kein Offset";
    const h = ms / 3_600_000;
    return Number.isInteger(h) ? `${String(h).padStart(2, "0")}h` : clock(ms, false);
  };
</script>

<svelte:window bind:innerWidth={width} />

{#if decks.length === 0}
  <div class="empty">
    <h2>Keine Player gefunden</h2>
    <p>
      Prüfe unter <a href="#outputs">Netzwerk</a> die Schnittstelle. Zum Ausprobieren ohne Hardware:
      <code>decklight simulate</code>.
    </p>
  </div>
{:else}
  <div class="rows">
    <div class="row masterrow">
      <div class="num" class:red={!!master}><span>SMPTE</span><b>M</b></div>
      <div class="body">
        <div class="mtitle">{master ? titleLine(master) : "Kein Master"}</div>
        <div class="cues">
          <div><span class="cap">Current cue</span><b>{masterCues.current?.name ?? "NONE"}</b></div>
          <div>
            <span class="cap">Next cue</span><b>{masterCues.next?.name ?? "NONE"}</b>
            {#if masterCues.untilNext != null}<i>in {clock(masterCues.untilNext)}</i>{/if}
          </div>
        </div>
      </div>
      <div class="clock"><SegClock text={s?.timecode.enabled ? s.timecode.text : null} tone={masterTone} size={seg} /></div>
      <div class="bpmcol">
        <span class="cap">BPM</span>
        <span class="bpm">{master?.effective_bpm?.toFixed(1) ?? "--"}</span>
        <span class="ico">{stateIcon(master)}</span>
      </div>
      <div class="btns">
        <button class:on={modeIs("on_air")} disabled={conn.readonly} onclick={() => setMaster({ mode: "on_air" })}>On Air</button>
        <button class:on={modeIs("tempo_master")} disabled={conn.readonly} onclick={() => setMaster({ mode: "tempo_master" })}>Tempo</button>
        <span class="cap">{s?.timecode.rate ?? ""} fps</span>
      </div>
    </div>

    {#each decks as d (d.number)}
      <div class="row" class:onair={d.on_air}>
        <div class="num" class:red={d.master}>
          <span>Player</span><b>{d.number}</b>
          {#if d.on_air}<em>On Air</em>{/if}
        </div>
        <div class="body">
          <div class="titlebar">♪ {titleLine(d)}</div>
          <div class="wave">
            <div class="w">
              <Waveform
                track={live.of(d)}
                position={() => conn.positionOf(d.number)}
                mode="overview"
                cues={cuesOf(d.info?.key)}
                rekordboxCues={live.of(d)?.detail.rekordbox_cues ?? []}
                height={48}
              />
            </div>
            {#if d.info?.has_artwork}
              <img src={artworkUrl(d.info.key)} alt="" />
            {:else}
              <div class="noart">{d.number}</div>
            {/if}
          </div>
        </div>
        <div class="clock"><SegClock text={deckTimecode(d)} tone={tone(d)} size={seg} /></div>
        <div class="bpmcol">
          <span class="cap">BPM</span>
          <span class="bpm">{d.effective_bpm?.toFixed(1) ?? "--"}</span>
          <span class="pitch">{pitch(d)} <span class="ico">{stateIcon(d)}</span></span>
        </div>
        <div class="btns">
          <button
            class:on={modeIs("manual", d.number)}
            disabled={conn.readonly}
            title="Timecode fest von diesem Deck"
            onclick={() => setMaster({ mode: "manual", deck: d.number })}>Deck {d.number}</button
          >
          <span class="cap off" title="Timecode-Offset des Tracks">{offsetLabel(d.info?.offset_ms)}</span>
        </div>
      </div>
    {/each}
  </div>

  <section class="recent">
    <span class="cap">Zuletzt ausgelöst</span>
    {#if recent.length === 0}
      <span class="dim">Noch keine Cues oder Trigger ausgelöst.</span>
    {:else}
      {#each recent as e}
        <div class="log" class:err={e.error}>
          <span class="t">{time(e.unix_ms)}</span>
          <span class="src">{e.source}{e.deck ? ` · Deck ${e.deck}` : ""}</span>
          <span class="act">{e.error ?? e.actions.join(", ")}</span>
        </div>
      {/each}
    {/if}
  </section>
{/if}

<style>
  .rows {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .row {
    display: grid;
    grid-template-columns: 58px minmax(0, 1fr) auto 92px 92px;
    gap: 8px;
    align-items: center;
    background: var(--surface);
    border: 1px solid var(--border);
    padding: 5px 6px;
  }
  .num {
    align-self: stretch;
    border: 1px solid #fff;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    line-height: 1;
    gap: 2px;
  }
  .num span {
    font-size: 9px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }
  .num b {
    font-size: 28px;
  }
  .num em {
    font-style: normal;
    font-size: 8px;
    font-weight: 800;
    background: var(--tag);
    color: #000;
    padding: 1px 3px;
    text-transform: uppercase;
  }
  .num.red {
    background: var(--onair);
    border-color: var(--onair);
  }
  .body {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .titlebar {
    font-size: 12px;
    padding: 2px 8px;
  }
  .wave {
    display: flex;
    gap: 6px;
    align-items: stretch;
  }
  .w {
    flex: 1;
    min-width: 0;
    background: #050507;
  }
  img,
  .noart {
    width: 48px;
    height: 48px;
    object-fit: cover;
    flex: none;
  }
  .noart {
    background: var(--surface-2);
    display: grid;
    place-items: center;
    color: var(--text-dim);
    font-weight: 800;
  }
  .mtitle {
    font-size: 20px;
    font-weight: 700;
    text-transform: uppercase;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .cues {
    display: flex;
    flex-direction: column;
    gap: 1px;
    font-size: 12px;
  }
  .cues div {
    display: flex;
    gap: 8px;
    align-items: baseline;
    min-width: 0;
  }
  .cues .cap {
    width: 80px;
    flex: none;
  }
  .cues b {
    text-transform: uppercase;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .cues i {
    font-style: normal;
    color: var(--tc-loop);
    font-family: var(--mono);
  }
  .bpmcol {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    line-height: 1.1;
  }
  .bpm {
    font-family: var(--mono);
    font-size: 24px;
    font-weight: 700;
  }
  .pitch {
    font-family: var(--mono);
    font-size: 12px;
    color: var(--text-dim);
  }
  .ico {
    color: var(--text);
    font-size: 14px;
  }
  .btns {
    display: flex;
    flex-direction: column;
    gap: 3px;
    align-items: stretch;
  }
  .btns button {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    padding: 4px 6px;
  }
  .btns button.on {
    background: var(--accent);
    border-color: var(--accent);
  }
  .btns .cap {
    text-align: center;
  }
  .btns .off {
    font-family: var(--mono);
  }
  .recent {
    margin-top: 10px;
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-size: 12px;
  }
  .log {
    display: flex;
    gap: 12px;
  }
  .log .t {
    font-family: var(--mono);
    color: var(--text-dim);
  }
  .log .src {
    min-width: 150px;
    font-weight: 600;
  }
  .log .act {
    color: var(--text-dim);
  }
  .log.err .src,
  .log.err .act {
    color: var(--danger);
  }
  .dim {
    color: var(--text-dim);
  }
  .empty {
    color: var(--text-dim);
    padding: 40px;
    text-align: center;
  }
  .empty a {
    color: #7aa2ff;
  }
  @media (max-width: 699px) {
    .row {
      grid-template-columns: 58px minmax(0, 1fr) 84px;
    }
    .clock {
      grid-column: 1 / -1;
      grid-row: 2;
      justify-self: center;
    }
    .bpmcol {
      grid-column: 3;
      grid-row: 1;
    }
    .btns {
      grid-column: 1 / -1;
      grid-row: 3;
      flex-direction: row;
      justify-content: flex-end;
      align-items: center;
    }
    .w + img,
    .wave .noart {
      display: none;
    }
  }
  @media (min-width: 700px) and (max-width: 1100px) {
    .row {
      grid-template-columns: 58px minmax(0, 1fr) 92px;
    }
    .clock {
      grid-column: 2;
      grid-row: 2;
    }
    .bpmcol {
      grid-column: 3;
      grid-row: 1;
    }
    .btns {
      grid-column: 3;
      grid-row: 2;
    }
  }
</style>
