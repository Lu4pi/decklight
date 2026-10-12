<script lang="ts">
  // /monitor: Vollbild fürs Tablet oder Handy, nur lesend.
  import { conn } from "../lib/conn.svelte";
  import { clock, stateLabel } from "../lib/format";
  import { artworkUrl, loadTrack, type LoadedTrack } from "../lib/tracks";
  import Waveform from "../components/Waveform.svelte";
  import PinPrompt from "../components/PinPrompt.svelte";

  const s = $derived(conn.state);
  const decks = $derived((s?.decks ?? []).filter((d) => d.number <= 6));
  const colors = ["#2fa8ff", "#ff8a1f", "#b36bff", "#3ddc84", "#ff4fa3", "#f5d90a"];

  let tracks = $state<Record<string, LoadedTrack | null>>({});
  $effect(() => {
    for (const d of decks) {
      const k = d.info?.loaded ? d.info.key : null;
      if (k && !(k in tracks)) {
        tracks[k] = null;
        loadTrack(k).then((t) => (tracks[k] = t));
      }
    }
  });

  // Bildschirm anlassen, solange die Seite offen ist.
  $effect(() => {
    let lock: { release(): Promise<void> } | null = null;
    const nav = navigator as Navigator & { wakeLock?: { request(t: "screen"): Promise<{ release(): Promise<void> }> } };
    nav.wakeLock?.request("screen").then((l) => (lock = l)).catch(() => {});
    return () => {
      lock?.release();
    };
  });
</script>

{#if conn.needsPin}
  <PinPrompt />
{:else}
  <div class="monitor">
    <header>
      <span class="brand">DeckLight</span>
      <span class="tc" class:run={s?.timecode.running}>{s?.timecode.text ?? "--:--:--:--"}</span>
      <span class="info">
        {s?.master ? `Master: Deck ${s.master}` : "kein Master"}
        {#if s && !s.armed}<b class="off">TRIGGER GESPERRT</b>{/if}
        {#if !conn.connected}<b class="off">keine Verbindung</b>{/if}
      </span>
    </header>
    <div class="decks" style:--n={Math.min(decks.length, 2) || 1}>
      {#each decks as d (d.number)}
        {@const c = colors[(d.number - 1) % 6]}
        {@const len = d.track_length_ms ?? (d.info ? d.info.duration_s * 1000 : null)}
        <div class="deck" class:master={d.master} style:--c={c}>
          <div class="top">
            <span class="num">{d.number}</span>
            {#if d.info?.has_artwork}<img src={artworkUrl(d.info.key)} alt="" />{/if}
            <div class="text">
              <div class="title">{d.info?.title ?? "—"}</div>
              <div class="artist">{d.info?.artist ?? ""}</div>
            </div>
            <div class="right">
              <div class="bpm">{d.effective_bpm?.toFixed(1) ?? "--"}</div>
              <div class="state">
                {stateLabel(d.play_state)}{#if d.on_air} · <b class="air">ON AIR</b>{/if}
              </div>
            </div>
          </div>
          <Waveform
            track={d.info ? (tracks[d.info.key] ?? null) : null}
            position={() => conn.positionOf(d.number)}
            mode="overview"
            color={c}
            height={70}
          />
          <div class="times">
            <span>{clock(d.position_ms, false)}</span>
            <span class="rem">-{clock(len != null && d.position_ms != null ? len - d.position_ms : null, false)}</span>
          </div>
        </div>
      {/each}
    </div>
  </div>
{/if}

<style>
  .monitor {
    height: 100vh;
    display: flex;
    flex-direction: column;
    padding: 12px;
    gap: 12px;
  }
  header {
    display: flex;
    align-items: center;
    gap: 18px;
    flex-wrap: wrap;
  }
  .brand {
    font-weight: 800;
    color: var(--accent);
  }
  .tc {
    font-family: var(--mono);
    font-size: clamp(28px, 6vw, 64px);
    font-weight: 700;
    color: var(--text-dim);
  }
  .tc.run {
    color: var(--accent);
  }
  .info {
    color: var(--text-dim);
    display: flex;
    gap: 12px;
  }
  .off {
    color: var(--danger);
  }
  .decks {
    flex: 1;
    display: grid;
    grid-template-columns: repeat(var(--n), minmax(0, 1fr));
    gap: 12px;
    align-content: start;
  }
  @media (max-width: 800px) {
    .decks {
      grid-template-columns: 1fr;
    }
  }
  .deck {
    background: var(--surface);
    border: 1px solid var(--border);
    border-left: 6px solid var(--c);
    border-radius: 10px;
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .deck.master {
    box-shadow: 0 0 0 2px var(--c);
  }
  .top {
    display: flex;
    gap: 12px;
    align-items: center;
    min-width: 0;
  }
  .num {
    font-size: 40px;
    font-weight: 800;
    color: var(--c);
    width: 36px;
  }
  img {
    width: 72px;
    height: 72px;
    border-radius: 6px;
    object-fit: cover;
  }
  .text {
    flex: 1;
    min-width: 0;
  }
  .title {
    font-size: clamp(18px, 2.6vw, 30px);
    font-weight: 700;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .artist {
    color: var(--text-dim);
    font-size: clamp(14px, 1.8vw, 20px);
  }
  .right {
    text-align: right;
  }
  .bpm {
    font-family: var(--mono);
    font-size: clamp(22px, 3vw, 36px);
    font-weight: 700;
  }
  .state {
    color: var(--text-dim);
    font-size: 13px;
  }
  .air {
    color: var(--danger);
  }
  .times {
    display: flex;
    justify-content: space-between;
    font-family: var(--mono);
    font-size: clamp(18px, 2.4vw, 28px);
  }
  .rem {
    color: var(--text-dim);
  }
</style>
