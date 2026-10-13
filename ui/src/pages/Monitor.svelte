<script lang="ts">
  // /monitor: Handy- und Tablet-Ansicht. Eine Karte je Player mit großem Timecode.
  import { conn } from "../lib/conn.svelte";
  import { deckTimecode, stateIcon, titleLine, tone } from "../lib/deck";
  import { live } from "../lib/live.svelte";
  import Waveform from "../components/Waveform.svelte";
  import PinPrompt from "../components/PinPrompt.svelte";
  import Logo from "../components/Logo.svelte";
  import SegText from "../components/SegText.svelte";

  const s = $derived(conn.state);
  const decks = $derived((s?.decks ?? []).filter((d) => d.number <= 6));
  $effect(() => live.ensure(decks));

  // Bildschirm anlassen, solange die Seite offen ist (geht nur, wo der Browser es erlaubt).
  $effect(() => {
    let lock: { release(): Promise<void> } | null = null;
    const nav = navigator as Navigator & { wakeLock?: { request(t: "screen"): Promise<{ release(): Promise<void> }> } };
    const request = () => {
      if (document.visibilityState === "visible") nav.wakeLock?.request("screen").then((l) => (lock = l)).catch(() => {});
    };
    request();
    document.addEventListener("visibilitychange", request);
    return () => {
      document.removeEventListener("visibilitychange", request);
      lock?.release();
    };
  });
</script>

{#if conn.needsPin}
  <PinPrompt />
{:else}
  <div class="monitor">
    <header>
      <Logo size={22} />
      <div class="status">
        <span class="mtc" class:run={s?.timecode.running}>TC {s?.timecode.enabled ? (s.timecode.text ?? "--:--:--:--") : "aus"}</span>
        <span>{s?.master ? `Master ${s.master}` : "kein Master"}</span>
        {#if s && !s.armed}<b class="off">Trigger gesperrt</b>{/if}
        {#if !conn.connected}<b class="off">keine Verbindung</b>{/if}
      </div>
    </header>

    {#if decks.length === 0}
      <p class="empty">{conn.connected ? "Keine Player im Netz." : "Verbinde mit DeckLight …"}</p>
    {/if}

    <div class="cards">
      {#each decks as d (d.number)}
        <div class="card" class:master={d.master}>
          <div class="titlebar">♪ {titleLine(d)}</div>
          <div class="mid">
            <div class="pl">
              <div class="box" class:red={d.master}><span>Player</span><b>{d.number}</b></div>
              <span class="tag" class:on={d.on_air}>{d.on_air ? "On Air" : ""}</span>
            </div>
            <div class="tcol">
              <div class="beats" aria-label="Beat {d.beat_in_bar}">
                {#each [1, 2, 3, 4] as b}<i class:on={d.beat_in_bar === b && d.position_ms != null}></i>{/each}
              </div>
              <div class="tc {tone(d)}"><SegText text={deckTimecode(d) ?? "--:--:--:--"} /></div>
            </div>
            <div class="ico">{stateIcon(d)}</div>
          </div>
          <div class="wave">
            <Waveform track={live.of(d)} position={() => conn.positionOf(d.number)} mode="overview" height={40} />
          </div>
        </div>
      {/each}
    </div>
  </div>
{/if}

<style>
  .monitor {
    min-height: 100vh;
    min-height: 100dvh;
    background: #000;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: max(10px, env(safe-area-inset-top)) max(8px, env(safe-area-inset-right)) max(10px, env(safe-area-inset-bottom))
      max(8px, env(safe-area-inset-left));
  }
  header {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
  }
  .status {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 4px 12px;
    font-size: 12px;
    color: var(--text-dim);
  }
  .mtc {
    font-family: var(--mono);
  }
  .mtc.run {
    color: var(--tc-run);
  }
  .off {
    color: var(--danger);
  }
  .empty {
    text-align: center;
    color: var(--text-dim);
  }
  .cards {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: 6px;
    align-content: start;
  }
  @media (min-width: 760px) {
    .cards {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }
  .card {
    border: 2px solid #1c1c26;
    background: #030306;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .card.master {
    border-color: #3a1013;
  }
  .titlebar {
    font-size: clamp(13px, 3.6vw, 17px);
    padding: 4px 8px;
  }
  .mid {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto;
    gap: clamp(6px, 2.5vw, 14px);
    align-items: center;
    padding: 6px 8px 2px;
  }
  .pl {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
  }
  .box {
    border: 2px solid #fff;
    display: flex;
    flex-direction: column;
    align-items: center;
    line-height: 1;
    padding: 3px 8px 4px;
    min-width: clamp(48px, 13vw, 64px);
  }
  .box span {
    font-size: clamp(9px, 2.4vw, 12px);
    text-transform: uppercase;
    color: #cfcfd8;
  }
  .box b {
    font-size: clamp(28px, 8vw, 40px);
    font-weight: 600;
  }
  .box.red {
    background: var(--onair);
    border-color: var(--onair);
  }
  .box.red span {
    color: #fff;
  }
  .tag {
    width: 100%;
    height: 9px;
    background: #24242c;
    font-size: 7px;
    font-weight: 800;
    color: #000;
    text-transform: uppercase;
    text-align: center;
    line-height: 9px;
  }
  .tag.on {
    background: var(--tag);
  }
  .tcol {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
  }
  .beats {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 3px;
    width: 72%;
    align-self: flex-end;
  }
  .beats i {
    height: clamp(8px, 2.4vw, 12px);
    background: #1d2160;
    border: 1px solid #333a90;
  }
  .beats i.on {
    background: var(--beat);
    border-color: var(--beat);
  }
  .tc {
    font-family: var(--seg);
    font-size: clamp(22px, 8.2vw, 46px);
    line-height: 1.05;
    white-space: nowrap;
    color: var(--tc-stop);
  }
  @media (min-width: 760px) {
    .tc {
      font-size: clamp(22px, 3.9vw, 46px);
    }
  }
  .tc.run {
    color: var(--tc-run);
  }
  .tc.loop {
    color: var(--tc-loop);
  }
  .tc.off {
    color: var(--tc-off);
  }
  .ico {
    font-size: clamp(18px, 5vw, 26px);
    width: 1.4em;
    text-align: center;
  }
  .wave {
    margin: 4px 6px 6px;
    background: #050507;
    border-bottom: 2px solid #2a2a36;
  }
</style>
