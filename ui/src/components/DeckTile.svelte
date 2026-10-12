<script lang="ts">
  // Ein Deck: Cover, Titel, Tempo, Zeiten, Zustand, Wellenformen.
  import { conn } from "../lib/conn.svelte";
  import { clock, stateLabel } from "../lib/format";
  import { artworkUrl, loadTrack, type LoadedTrack } from "../lib/tracks";
  import type { DeckView } from "../lib/types";
  import Waveform from "./Waveform.svelte";

  let { deck, compact = false }: { deck: DeckView; compact?: boolean } = $props();

  let track = $state<LoadedTrack | null>(null);
  const key = $derived(deck.info?.loaded ? deck.info.key : null);
  const cues = $derived(key && conn.show ? (conn.show.triggers.tracks[key] ?? []) : []);

  $effect(() => {
    const k = key;
    track = null;
    if (!k) return;
    let alive = true;
    loadTrack(k).then((t) => {
      if (alive) track = t;
    });
    return () => {
      alive = false;
    };
  });

  const length = $derived(deck.track_length_ms ?? (deck.info ? deck.info.duration_s * 1000 : null));
  const remaining = $derived(length != null && deck.position_ms != null ? length - deck.position_ms : null);
  const color = $derived(`var(--deck-${((deck.number - 1) % 6) + 1})`);
  const colorHex = $derived(["#2fa8ff", "#ff8a1f", "#b36bff", "#3ddc84", "#ff4fa3", "#f5d90a"][(deck.number - 1) % 6]);
  const pitch = $derived(`${deck.pitch_percent >= 0 ? "+" : ""}${deck.pitch_percent.toFixed(2)} %`);
</script>

<section class="deck" class:master={deck.master} class:onair={deck.on_air} style:--deck={color}>
  <header>
    <span class="num">{deck.number}</span>
    <span class="state" class:playing={deck.moving}>{stateLabel(deck.play_state)}</span>
    {#if deck.master}<span class="badge master">MASTER</span>{/if}
    {#if deck.on_air}<span class="badge air">ON AIR</span>{/if}
    {#if deck.tempo_master}<span class="badge tempo">TEMPO</span>{/if}
    <span class="beats" aria-label="Beat {deck.beat_in_bar}">
      {#each [1, 2, 3, 4] as b}
        <i class:on={deck.beat_in_bar === b && deck.moving}></i>
      {/each}
    </span>
  </header>

  <div class="meta">
    <div class="cover">
      {#if deck.info?.has_artwork}
        <img src={artworkUrl(deck.info.key)} alt="" />
      {:else}
        <div class="nocover">{deck.number}</div>
      {/if}
    </div>
    <div class="text">
      <div class="title" title={deck.info?.title}>{deck.info?.title ?? "Kein Track"}</div>
      <div class="artist">{deck.info?.artist ?? deck.name}</div>
      {#if !compact && deck.info?.album}<div class="album">{deck.info.album}</div>{/if}
    </div>
    <div class="nums">
      <div class="bpm">{deck.effective_bpm?.toFixed(1) ?? "--"}<small>BPM</small></div>
      <div class="small">{pitch}{#if deck.info?.musical_key} · {deck.info.musical_key}{/if}</div>
    </div>
  </div>

  <div class="times">
    <span>{clock(deck.position_ms)}</span>
    <span class="rem">-{clock(remaining)}</span>
  </div>

  {#if !compact}
    <Waveform
      {track}
      position={() => conn.positionOf(deck.number)}
      mode="detail"
      color={colorHex}
      {cues}
      rekordboxCues={track?.detail.rekordbox_cues ?? []}
      height={92}
    />
  {/if}
  <Waveform
    {track}
    position={() => conn.positionOf(deck.number)}
    mode="overview"
    color={colorHex}
    {cues}
    rekordboxCues={track?.detail.rekordbox_cues ?? []}
    height={compact ? 40 : 46}
  />
</section>

<style>
  .deck {
    background: var(--surface);
    border: 1px solid var(--border);
    border-top: 3px solid var(--deck);
    border-radius: 10px;
    padding: 10px 12px 12px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }
  .deck.master {
    box-shadow: 0 0 0 1px var(--deck), 0 0 24px -8px var(--deck);
  }
  header {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
  }
  .num {
    font-weight: 800;
    font-size: 18px;
    color: var(--deck);
    width: 22px;
  }
  .state {
    color: var(--text-dim);
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }
  .state.playing {
    color: var(--ok);
  }
  .badge {
    padding: 1px 6px;
    border-radius: 4px;
    font-weight: 700;
    letter-spacing: 0.05em;
    font-size: 11px;
  }
  .badge.master {
    background: var(--deck);
    color: #000;
  }
  .badge.air {
    background: var(--danger);
    color: #fff;
  }
  .badge.tempo {
    border: 1px solid var(--border);
    color: var(--text-dim);
  }
  .beats {
    margin-left: auto;
    display: flex;
    gap: 4px;
  }
  .beats i {
    width: 10px;
    height: 10px;
    border-radius: 2px;
    background: var(--surface-2);
  }
  .beats i.on {
    background: var(--deck);
  }
  .meta {
    display: flex;
    gap: 10px;
    align-items: center;
    min-width: 0;
  }
  .cover,
  .cover img,
  .nocover {
    width: 56px;
    height: 56px;
    border-radius: 6px;
    flex: none;
  }
  .cover img {
    object-fit: cover;
    display: block;
  }
  .nocover {
    background: var(--surface-2);
    display: grid;
    place-items: center;
    color: var(--text-dim);
    font-weight: 800;
    font-size: 22px;
  }
  .text {
    min-width: 0;
    flex: 1;
  }
  .title {
    font-size: 17px;
    font-weight: 700;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .artist,
  .album {
    color: var(--text-dim);
    font-size: 13px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .nums {
    text-align: right;
    flex: none;
  }
  .bpm {
    font-family: var(--mono);
    font-size: 22px;
    font-weight: 700;
  }
  .bpm small {
    font-size: 10px;
    color: var(--text-dim);
    margin-left: 3px;
  }
  .small {
    font-family: var(--mono);
    font-size: 12px;
    color: var(--text-dim);
  }
  .times {
    display: flex;
    justify-content: space-between;
    font-family: var(--mono);
    font-size: 15px;
  }
  .rem {
    color: var(--text-dim);
  }
</style>
